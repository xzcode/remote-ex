//! Wayland RemoteDesktop portal. The compositor separately grants keyboard
//! and pointer devices; no evdev device access, XWayland or restore token.
//! Pointer moves are stream-relative. Portal button/key events are desktop
//! scoped; do not advertise this as an isolated window or pointer readback.
use super::*;
use ashpd::desktop::{PersistMode,Session,remote_desktop::{RemoteDesktop,DeviceType,SelectDevicesOptions,KeyState,Axis},screencast::{Screencast,SelectSourcesOptions,SourceType,CursorMode}};
use futures_util::StreamExt;
use gstreamer as gst;
use gst::prelude::*;
use gstreamer_app as app;
use std::{os::fd::AsRawFd,sync::mpsc::{Receiver,SyncSender}};
use super::portal_display::{source_spec,restrict_stream};

struct Pipeline(gst::Pipeline);
impl Drop for Pipeline{fn drop(&mut self){let _=self.0.set_state(gst::State::Null);}}
#[derive(Clone,Copy)]
enum Op{Move(i32,i32),Key(i32,bool),Button(i32,bool,Option<(i32,i32)>),Scroll(i32,i32,(i32,i32))}
struct Job{op:Op,ack:SyncSender<MediaResult<()>>,expires:Instant}
pub(super) struct PortalBackend{jobs:SyncSender<Job>,alive:Arc<AtomicBool>,area:Option<DisplayArea>,point:Option<(i32,i32)>}
impl PortalBackend{
    pub(super) fn with_area(mut self,area:DisplayArea)->Self{self.area=Some(area);self}
    fn send(&self,op:Op)->InputResult<()>{
        if !self.alive.load(Ordering::Acquire){return Err("RemoteDesktop portal is closed".into());}
        let(tx,rx)=sync_channel(1);
        self.jobs.try_send(Job{op,ack:tx,expires:Instant::now()+Duration::from_millis(650)}).map_err(|_|"Portal input queue unavailable")?;
        rx.recv_timeout(Duration::from_millis(700)).map_err(|_|"Portal input confirmation timed out")?
    }
    fn inside(&self,x:i32,y:i32)->bool{self.area.is_some_and(|a|x>=0&&y>=0&&x<a.width as i32&&y<a.height as i32)}
}
impl Backend for PortalBackend{
    fn check_display(&mut self,area:DisplayArea)->InputResult<()>{if self.area!=Some(area)||!self.alive.load(Ordering::Acquire){return Err("Portal display changed or revoked".into());}Ok(())}
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{if !self.inside(x,y){return Err("Pointer outside portal stream".into());}self.send(Op::Move(x,y))?;self.point=Some((x,y));Ok(())}
    // Portal API cannot query the actual OS pointer. Retain the last accepted
    // stream-relative target; re-anchor immediately before press/scroll.
    fn position(&mut self)->InputResult<(i32,i32)>{self.point.ok_or("Position pointer on the shared monitor first".into())}
    fn key(&mut self,usage:u16,down:bool)->InputResult<()>{
        let code=linux_display::physical(usage).and_then(|v|v.checked_sub(8)).ok_or("Unsupported physical key")?;
        if down&&self.point.is_none(){return Err("Position pointer before using portal keyboard".into());}
        self.send(Op::Key(i32::from(code),down))
    }
    fn button(&mut self,id:u8,down:bool)->InputResult<()>{
        let code=match id{1=>272,2=>274,3=>273,_=>return Err("Unsupported portal mouse button".into())};
        let point=if down{Some(self.position()?)}else{None};
        self.send(Op::Button(code,down,point))
    }
    fn scroll_steps(&mut self,x:i32,y:i32)->InputResult<()>{if x.unsigned_abs()>20||y.unsigned_abs()>20{return Err("Portal scroll limit exceeded".into());}let point=self.position()?;self.send(Op::Scroll(x,y,point))}
}
async fn apply(rd:&RemoteDesktop,session:&Session<RemoteDesktop>,node:u32,op:Op)->MediaResult<()>{
    let state=|down|if down{KeyState::Pressed}else{KeyState::Released};
    match op{
        Op::Move(x,y)=>rd.notify_pointer_motion_absolute(session,node,f64::from(x),f64::from(y),Default::default()).await.map_err(error),
        Op::Key(k,down)=>rd.notify_keyboard_keycode(session,k,state(down),Default::default()).await.map_err(error),
        Op::Button(b,down,point)=>{
            if let Some((x,y))=point{rd.notify_pointer_motion_absolute(session,node,f64::from(x),f64::from(y),Default::default()).await.map_err(error)?;}
            rd.notify_pointer_button(session,b,state(down),Default::default()).await.map_err(error)
        },
        Op::Scroll(x,y,(px,py))=>{
            rd.notify_pointer_motion_absolute(session,node,f64::from(px),f64::from(py),Default::default()).await.map_err(error)?;
            if x!=0{rd.notify_pointer_axis_discrete(session,Axis::Horizontal,x,Default::default()).await.map_err(error)?;}
            if y!=0{rd.notify_pointer_axis_discrete(session,Axis::Vertical,y,Default::default()).await.map_err(error)?;}Ok(())
        },
    }
}
fn area_for(size:Option<(i32,i32)>)->MediaResult<DisplayArea>{
    let(w,h)=size.ok_or("Portal monitor did not report logical geometry")?;
    if w<=0||h<=0||i64::from(w)*i64::from(h)>32_000_000{return Err("Invalid portal logical monitor geometry".into());}
    DisplayArea{x:0,y:0,width:w as u32,height:h as u32}.validate()
}
fn format_matches(area:DisplayArea,w:u32,h:u32)->bool{
    w>0&&h>0&&u64::from(w)*u64::from(h)<=32_000_000&&u64::from(w)*u64::from(area.height)==u64::from(h)*u64::from(area.width)
}
fn frame(sample:&gst::Sample,area:DisplayArea,c:VideoConfig,expected:&mut Option<(u32,u32)>)->MediaResult<Captured>{
    let caps=sample.caps().and_then(|c|c.structure(0)).ok_or("Missing portal video caps")?;
    let w=caps.get::<i32>("width").map_err(error)?;let h=caps.get::<i32>("height").map_err(error)?;
    if w<=0||h<=0||!format_matches(area,w as u32,h as u32)||caps.get::<String>("format").map_err(error)?!="RGBA"{return Err("Portal capture cannot be mapped to the authorized monitor".into());}
    let size=(w as u32,h as u32);
    if expected.is_some_and(|old|old!=size){return Err("Portal monitor pixel geometry changed".into());}
    *expected=Some(size);
    let buf=sample.buffer().ok_or("Missing portal frame")?;let bytes=buf.map_readable().map_err(error)?;
    if bytes.size()!=size.0 as usize*size.1 as usize*4{return Err("Unsupported portal frame stride".into());}
    let image=RgbaImage::from_raw(size.0,size.1,bytes.as_slice().to_vec()).ok_or("Invalid portal pixels")?;
    let g=Geometry::from_area(area,c)?;
    let mut canvas=RgbaImage::from_pixel(c.width,c.height,Rgba([0,0,0,255]));
    let scaled=DynamicImage::ImageRgba8(image).resize_exact(g.width,g.height,FilterType::Triangle).into_rgba8();
    imageops::replace(&mut canvas,&scaled,i64::from(g.x),i64::from(g.y));
    Ok(Captured{layout:0,pixels:canvas.into_raw(),epoch:0})
}
async fn wait_stop(stop:&AtomicBool){while !canceled()&&!stop.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(50)).await;}}
async fn run(c:VideoConfig,stop:Arc<AtomicBool>,alive:Arc<AtomicBool>,frames:&SyncSender<MediaResult<Captured>>,ready:&SyncSender<MediaResult<DisplayArea>>,jobs:&Receiver<Job>)->MediaResult<()>{
    remote_ex_desktop_features::platform::require_wayland()?;
    let(rd,sc)=tokio::time::timeout(Duration::from_secs(5),async{
        Ok::<_,ashpd::Error>((RemoteDesktop::new().await?,Screencast::new().await?))
    }).await.map_err(|_|"RemoteDesktop portal unavailable")?.map_err(error)?;
    if rd.version()<2||sc.version()<2{return Err("RemoteDesktop and ScreenCast v2 or newer are required".into());}
    let session=tokio::time::timeout(Duration::from_secs(5),rd.create_session(Default::default())).await.map_err(|_|"RemoteDesktop session creation timed out")?.map_err(error)?;
    let result:MediaResult<()>=async{
        let mut closed=session.receive_closed().await.map_err(error)?;
        let picker=async{
            let devices=DeviceType::KEYBOARD|DeviceType::POINTER;
            if !rd.available_device_types().await.map_err(error)?.contains(devices)||!sc.available_source_types().await.map_err(error)?.contains(SourceType::MONITOR)||!sc.available_cursor_modes().await.map_err(error)?.contains(CursorMode::EMBEDDED){return Err("The local compositor does not offer the required portal devices and monitor".to_owned());}
            rd.select_devices(&session,SelectDevicesOptions::default().set_devices(devices).set_persist_mode(PersistMode::DoNot)).await.map_err(error)?.response().map_err(error)?;
            sc.select_sources(&session,SelectSourcesOptions::default().set_sources(SourceType::MONITOR).set_cursor_mode(CursorMode::EMBEDDED).set_multiple(false).set_persist_mode(PersistMode::DoNot)).await.map_err(error)?.response().map_err(error)?;
            let grant=rd.start(&session,None,Default::default()).await.map_err(error)?.response().map_err(error)?;
            if grant.devices()!=devices||grant.is_clipboard_enabled()||grant.streams().len()!=1{return Err("Portal did not grant exactly the requested keyboard, pointer and single monitor".into());}
            let stream=grant.streams()[0].clone();
            if stream.source_type()!=Some(SourceType::MONITOR){return Err("Portal control requires a monitor stream".into());}
            let area=area_for(stream.size())?;
            let fd=sc.open_pipe_wire_remote(&session,Default::default()).await.map_err(error)?;
            Ok((stream,fd,area))
        };
        let(stream,fd,area)=tokio::select!{
            _=wait_stop(&stop)=>return Err("RemoteDesktop selection canceled".into()),
            _=closed.next()=>return Err("RemoteDesktop portal closed during selection".into()),
            result=tokio::time::timeout(Duration::from_secs(25),picker)=>result.map_err(|_|"RemoteDesktop selection timed out")??,
        };
        if canceled()||stop.load(Ordering::Acquire){return Err("Portal grant revoked".into());}
        let node=stream.pipe_wire_node_id();
        let source=source_spec(fd.as_raw_fd(),sc.version(),&stream)?;
        let p=Pipeline(gst::parse::launch(&format!("{source} ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! videoconvert ! video/x-raw,format=RGBA,pixel-aspect-ratio=1/1 ! appsink name=portal_frames sync=false max-buffers=1 drop=true")).map_err(error)?.downcast::<gst::Pipeline>().map_err(|_|"Invalid portal control pipeline")?);
        restrict_stream(&p.0)?;
        let sink=p.0.by_name("portal_frames").ok_or("Missing portal frame sink")?.downcast::<app::AppSink>().map_err(|_|"Invalid portal frame sink")?;
        let bus=p.0.bus().ok_or("Missing portal control bus")?;
        p.0.set_state(gst::State::Playing).map_err(error)?;
        let mut actual=None;let mut announced=false;
        loop{
            if canceled()||stop.load(Ordering::Acquire){break;}
            while let Some(msg)=bus.pop(){if matches!(msg.view(),gst::MessageView::Error(_)|gst::MessageView::Eos(_)){return Err("Portal PipeWire stream stopped".into());}}
            tokio::select!{biased;_=closed.next()=>return Err("The local desktop revoked control".into()),_=tokio::time::sleep(Duration::from_millis(5))=>{}}
            if canceled()||stop.load(Ordering::Acquire){break;}
            if let Some(sample)=sink.try_pull_sample(gst::ClockTime::ZERO){
                let f=frame(&sample,area,c,&mut actual)?;
                match frames.try_send(Ok(f)){Ok(())|Err(TrySendError::Full(_))=>{},Err(TrySendError::Disconnected(_))=>break,}
                if !announced{alive.store(true,Ordering::Release);ready.try_send(Ok(area)).map_err(|_|"Portal owner disappeared")?;announced=true;}
            }
            if announced{
                match jobs.try_recv(){
                    Ok(job)=>{
                        if canceled()||stop.load(Ordering::Acquire)||Instant::now()>=job.expires{return Err("Portal input revoked or stale".into());}
                        let result=tokio::select!{
                            biased;
                            _=closed.next()=>Err("Portal control closed".into()),
                            result=tokio::time::timeout(Duration::from_millis(450),apply(&rd,&session,node,job.op))=>result.unwrap_or_else(|_|Err("Portal input call timed out".into())),
                        };
                        let failed=result.is_err();let _=job.ack.try_send(result);
                        if failed{return Err("Portal input action not confirmed".into());}
                    },
                    Err(TryRecvError::Empty)=>{},Err(TryRecvError::Disconnected)=>break,
                }
            }
        }
        drop(p);drop(fd);Ok(())
    }.await;
    alive.store(false,Ordering::Release);
    let _=tokio::time::timeout(Duration::from_secs(1),session.close()).await;
    result
}
pub(super) fn start(c:VideoConfig)->(Capture,Receiver<MediaResult<DisplayArea>>,PortalBackend){
    let(tx,rx)=sync_channel(1);let(ready_tx,ready)=sync_channel(1);let(done_tx,done)=sync_channel(1);let(jobs,job_rx)=sync_channel(4);
    let stop=Arc::new(AtomicBool::new(false));let worker=stop.clone();let alive=Arc::new(AtomicBool::new(false));let running=alive.clone();
    std::thread::spawn(move||{
        let result=match tokio::runtime::Builder::new_current_thread().enable_all().build(){Ok(rt)=>rt.block_on(run(c,worker.clone(),running.clone(),&tx,&ready_tx,&job_rx)),Err(e)=>Err(error(e))};
        running.store(false,Ordering::Release);
        if let Err(e)=result{let _=ready_tx.try_send(Err(e.clone()));let _=tx.try_send(Err(e));}
        drop(tx);drop(ready_tx);let _=done_tx.try_send(());
    });
    (Capture{rx,stop,layout:Arc::new(AtomicUsize::new(0)),count:1,first_wait:Duration::from_secs(3),done:Some(done),portal:true},ready,PortalBackend{jobs,alive,area:None,point:None})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn logical_geometry_is_strict(){
        let a=area_for(Some((1920,1080))).unwrap();
        assert!(format_matches(a,3840,2160));assert!(!format_matches(a,1920,1200));assert!(!format_matches(a,0,0));
        assert!(area_for(None).is_err());assert!(area_for(Some((-1,100))).is_err());
    }
    #[test]fn backend_does_not_inject_without_portal_and_bounds(){
        let(tx,_)=sync_channel(1);
        let mut b=PortalBackend{jobs:tx,alive:Arc::new(AtomicBool::new(false)),area:Some(area_for(Some((100,100))).unwrap()),point:None};
        assert!(b.move_to(100,0).is_err());assert!(b.move_to(0,0).is_err());assert!(b.key(4,true).is_err());assert!(b.button(1,true).is_err());
    }
}
