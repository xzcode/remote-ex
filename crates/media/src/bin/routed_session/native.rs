//! Native session owner; real frames and OS input are strictly after endpoint
//! authorization, signed SDP, DTLS verification and encrypted readiness.
use super::*;
#[cfg(any(target_os="windows",target_os="macos"))]
use remote_ex_input::native::NativeBackend;
use std::sync::{Arc,atomic::AtomicUsize,mpsc::{sync_channel,TryRecvError,TrySendError}};
use remote_ex_desktop_features::viewport::{self,Rect};
use xcap::{Monitor,image::{DynamicImage,RgbaImage,Rgba,imageops::{self,FilterType}}};
#[cfg(target_os="windows")] mod windows_display;
#[cfg(target_os="macos")] mod macos_display;
#[cfg(target_os="linux")] mod linux_display;

fn monitor(display:u32,area:Option<DisplayArea>,input:bool,secondary:bool)->MediaResult<(Monitor,DisplayArea)>{
    #[cfg(target_os="windows")]
    let _dpi=if secondary{Some(windows_display::DpiGuard::new()?)}else{None};
    let all=Monitor::all().map_err(error)?;
    #[cfg(target_os="linux")]
    if input&&(all.len()!=1||secondary){return Err("X11 control requires exactly one locally approved primary display".into());}
    let m=all.into_iter().find(|m|m.id().ok()==Some(display)).ok_or("Authorized display disconnected")?;
    let primary=m.is_primary().map_err(error)?;
    if input&&!primary&&!secondary{return Err("Non-primary control requires explicit experimental approval".into());}
    if secondary&&!cfg!(any(target_os="windows",target_os="macos")){return Err("Secondary control requires Windows or macOS".into());}
    let(x,y)=if secondary{(m.x().map_err(error)?,m.y().map_err(error)?)}else{(0,0)};
    let current=DisplayArea{x,y,width:m.width().map_err(error)?,height:m.height().map_err(error)?}.validate()?;
    #[cfg(target_os="windows")]
    if secondary{windows_display::verify_area(current)?;}
    #[cfg(target_os="macos")]
    if secondary{macos_display::verify_area(display,current)?;}
    if area.is_some_and(|a|a!=current){return Err("Display geometry changed".into());}
    if u64::from(current.width)*u64::from(current.height)>32_000_000{return Err("Display exceeds capture limits".into());}
    Ok((m,current))
}
fn permissions(input:bool)->MediaResult<()>{
    #[cfg(target_os="linux")]
    {remote_ex_desktop_features::platform::require_linux_x11()?;if input{linux_display::require_local_session()?;}}
    #[cfg(target_os="macos")]
    {
        #[link(name="CoreGraphics",kind="framework")] extern "C"{fn CGPreflightScreenCaptureAccess()->bool;}
        #[link(name="ApplicationServices",kind="framework")] extern "C"{fn AXIsProcessTrusted()->bool;}
        if !unsafe{CGPreflightScreenCaptureAccess()}||(input&&!unsafe{AXIsProcessTrusted()}){
            return Err("Grant ordinary screen recording / accessibility permissions in System Settings".into());
        }
    }
    let _=input;Ok(())
}
struct Captured{layout:usize,pixels:Vec<u8>}
struct Capture{rx:std::sync::mpsc::Receiver<MediaResult<Captured>>,stop:Arc<AtomicBool>,layout:Arc<AtomicUsize>,count:usize}
impl Drop for Capture{fn drop(&mut self){self.stop.store(true,Ordering::Release);}}
fn capture(displays:Vec<(u32,DisplayArea)>,input:bool,config:VideoConfig,secondary:bool)->Capture{
    let(tx,rx)=sync_channel(1);let stop=Arc::new(AtomicBool::new(false));let worker=stop.clone();
    let layout=Arc::new(AtomicUsize::new(0));let selected=layout.clone();let count=displays.len();
    std::thread::spawn(move||{
        while !canceled()&&!worker.load(Ordering::Acquire){
            let next=Instant::now()+Duration::from_millis(1000/u64::from(config.fps));
            let index=selected.load(Ordering::Acquire);
            let result=(||->MediaResult<Captured>{
                #[cfg(target_os="windows")]
                let _dpi=if secondary{Some(windows_display::DpiGuard::new()?)}else{None};
                permissions(input)?;
                let chosen:Vec<_>=if index==displays.len()&&displays.len()>1&&!input{displays.clone()}
                    else{vec![*displays.get(index).ok_or("Unapproved display selection")?]};
                let tiles=viewport::tiles(config.width,config.height,chosen.len()).ok_or("Unsupported display layout")?;
                let mut canvas=RgbaImage::from_pixel(config.width,config.height,Rgba([0,0,0,255]));
                for ((display,area),tile) in chosen.into_iter().zip(tiles){
                    if canceled()||worker.load(Ordering::Acquire){return Err("Capture canceled".into());}
                    let(m,_)=monitor(display,Some(area),input,secondary)?;
                    let image=m.capture_image().map_err(error)?;
                    if (secondary&&cfg!(target_os="windows")&&(image.width()!=area.width||image.height()!=area.height))||image.width()==0||image.height()==0||u64::from(image.width())*u64::from(image.height())>32_000_000
                        ||u64::from(image.width())*u64::from(area.height)!=u64::from(image.height())*u64::from(area.width){
                        return Err("Unsupported captured geometry".into());
                    }
                    let g=viewport::fit((area.width,area.height),Rect{x:tile.x,y:tile.y,width:tile.width,height:tile.height}).ok_or("Invalid display tile")?;
                    let scaled=DynamicImage::ImageRgba8(image).resize_exact(g.width,g.height,FilterType::Triangle).into_rgba8();
                    imageops::replace(&mut canvas,&scaled,i64::from(g.x),i64::from(g.y));
                }
                Ok(Captured{layout:index,pixels:canvas.into_raw()})
            })();
            if canceled()||worker.load(Ordering::Acquire){break;}
            let bad=result.is_err();
            match tx.try_send(result){Ok(())=>{},Err(TrySendError::Full(_)) if !bad=>{},_=>break}
            if bad{break;}
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    });
    Capture{rx,stop,layout,count}
}
async fn serve<B:Backend>(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,session:&str,
    mut dispatcher:Dispatcher<B>,capture:Capture,input:bool,av:bool,extended:bool)->MediaResult<()>{
    // The native capture worker is already authorized and independent. Do not
    // acknowledge readiness or accept input before its first successful frame.
    let first_deadline=Instant::now()+Duration::from_secs(3);
    let initial=loop{
        if canceled()||Instant::now()>=first_deadline{return Err("First authorized display frame unavailable".into());}
        connected(peer,channel)?;
        match capture.rx.try_recv(){
            Ok(frame)=>break frame?,
            Err(TryRecvError::Empty)=>tick(conn).await?,
            Err(TryRecvError::Disconnected)=>return Err("Capture worker stopped before readiness".into()),
        }
    };
    peer.push_rgba(initial.pixels)?;
    let mut audio_capture=if av{Some(audio::Capture::with_limit(false,extended)?)}else{None};
    channel.send(READY_ACK)?;
    out(if input{"CONNECTED ROUTED CONTROL"}else{"CONNECTED ROUTED VIEW"})?;
    if !input{channel.send(&[VIEW_LAYOUT_ACK,initial.layout as u8,capture.count as u8])?;}
    let mut stream=runtime_quality::HostQuality::default();if extended{stream.publish(peer,channel)?;}
    let limit=if extended{runtime_quality::NATIVE_SECONDS}else{30};
    let started=Instant::now();let mut last=started;let mut last_frame=started;
    let mut stats=remote_ex_media::stats::Sampler::default();
    let mut last_audio=Instant::now();
    let mut published=Some(initial.layout);let mut last_switch=Instant::now()-Duration::from_secs(1);
    let mut bitrate=peer.config().bitrate;let ceiling=bitrate;let floor=(ceiling/4).max(100_000);let mut good=0u8;
    let result:MediaResult<()> = async{
        loop{
            if canceled(){return Ok(());}
            connected(peer,channel)?;
            if started.elapsed()>=Duration::from_secs(limit){return Ok(());}
            if last.elapsed()>Duration::from_secs(2)||last_frame.elapsed()>Duration::from_secs(2){return Err("Control heartbeat or video stalled".into());}
            for _ in 0..32{
                if canceled(){return Ok(());}
                let Some(bytes)=channel.receive()? else{break;};
                if bytes==STOP{return Ok(());}
                if bytes==HEARTBEAT{last=Instant::now();}
                else if extended&&bytes.first()==Some(&runtime_quality::REQUEST){stream.request(&bytes,peer,channel)?;}
                else if bytes==SWITCH_VIEW&&!input&&capture.count>1{
                    if last_switch.elapsed()<Duration::from_millis(250){return Err("Display switch rate exceeded".into());}
                    let next=(capture.layout.load(Ordering::Acquire)+1)%(capture.count+1);
                    capture.layout.store(next,Ordering::Release);last_switch=Instant::now();
                }
                else if input&&published.is_some(){permissions(true)?;dispatcher.dispatch(&bytes)?;}
                else if input{return Err("Input received before an actual captured frame".into());}
                else{return Err("Input is not authorized in view-only mode".into());}
            }
            match capture.rx.try_recv(){
                Ok(frame)=>{
                    if canceled(){return Ok(());}
                    let frame=frame?;
                    if frame.layout==capture.layout.load(Ordering::Acquire){
                        peer.push_rgba(frame.pixels)?;last_frame=Instant::now();
                        if published!=Some(frame.layout){
                            published=Some(frame.layout);
                            if !input{channel.send(&[VIEW_LAYOUT_ACK,frame.layout as u8,capture.count as u8])?;}
                        }
                    }
                },
                Err(TryRecvError::Empty)=>{},
                Err(TryRecvError::Disconnected)=>return Err("Capture worker stopped".into()),
            }
            if let Some(source)=audio_capture.as_mut(){
                for _ in 0..3{if let Some(pcm)=source.poll()?{peer.push_pcm(pcm)?;last_audio=Instant::now();}else{break;}}
                if last_audio.elapsed()>Duration::from_secs(5){return Err("Authorized audio capture stalled".into());}
            }
            if let Some(sample)=stats.poll(peer){
                out(&sample.line())?;
                if extended{stream.observe(peer,channel,sample.loss_pct,sample.rtt_ms)?;}
                else if let Some(loss)=sample.loss_pct{
                    let old=bitrate;
                    if loss>5.0{bitrate=(bitrate*3/4).max(floor);good=0;}
                    else if loss<1.0{good=good.saturating_add(1);if good>=3{bitrate=(bitrate+ceiling/10).min(ceiling);good=0;}}else{good=0;}
                    if bitrate!=old{peer.set_video_bitrate(bitrate)?;}
                }
            }
            tick(conn).await?;
        }
    }.await;
    drop(audio_capture);drop(capture);
    dispatcher.close()?;
    out("INPUT_RELEASED")?;
    result?;
    channel.send(ACK)?;
    finish(conn,session).await?;
    out("PASS ROUTED HOST: native session ended, input released")
}
pub(super) async fn host(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,cipher:&mut RouteCipher,
    session:&str,gate:SessionGate,profile:Profile,display:u32,extra_displays:Vec<u32>,secondary:bool,policy_display:Option<(u32,u32,String)>)->MediaResult<()>{
    if canceled(){return Err("Authorization revoked".into());}
    let input=profile.native_control();
    permissions(input)?;
    let config=peer.config();
    if input&&!extra_displays.is_empty(){return Err("Control is limited to one locally selected display".into());}
    if secondary&&!input{return Err("Secondary control requires an explicit control profile".into());}
    let mut ids=vec![display];ids.extend(extra_displays);
    if ids.len()>4||ids.iter().enumerate().any(|(i,d)|ids[..i].contains(d)){return Err("Invalid approved display list".into());}
    let mut displays=Vec::new();
    for d in ids{
        let(m,a)=monitor(d,None,input,secondary)?;
        if let Some((w,h,name))=&policy_display{
            if a.width!=*w||a.height!=*h||m.name().map_err(error)?.as_str()!=name.as_str(){return Err("Saved policy display changed while waiting; no capture authorized".into());}
        }
        displays.push((d,a));
    }
    let area=displays[0].1;
    let geometry=Geometry::from_area(area,config)?;
    cipher.send(conn,geometry.signal()).await?;
    if !input{cipher.send(conn,Signal::ViewLayout{count:displays.len() as u8}).await?;}
    wait_data(peer,channel,conn,READY).await?;
    if canceled(){return Err("Authorization revoked".into());}
    if input{
        #[cfg(target_os="windows")]
        if secondary{
            let backend=windows_display::ScopedBackend::new(display,area)?;
            let d=Dispatcher::new(gate,backend,area)?;
            return serve(peer,channel,conn,session,d,capture(displays,true,config,true),true,profile.av(),profile.extended()).await;
        }
        #[cfg(target_os="macos")]
        if secondary{
            let backend=macos_display::ScopedBackend::new(display,area)?;
            let d=Dispatcher::new(gate,backend,area)?;
            return serve(peer,channel,conn,session,d,capture(displays,true,config,true),true,profile.av(),profile.extended()).await;
        }
        #[cfg(target_os="linux")]
        {
            let backend=linux_display::ScopedBackend::new(display,area)?;
            let d=Dispatcher::new(gate,backend,area)?;
            return serve(peer,channel,conn,session,d,capture(displays,true,config,false),true,profile.av(),profile.extended()).await;
        }
        #[cfg(any(target_os="windows",target_os="macos"))]
        {let backend=NativeBackend::new()?;
        if backend.primary_area()?!=area{return Err("Primary display DPI/geometry mismatch".into());}
        let d=Dispatcher::new(gate,backend,area)?;
        serve(peer,channel,conn,session,d,capture(displays,true,config,false),true,profile.av(),profile.extended()).await}
    }else{
        // Never instantiate a native input backend in the view-only profile.
        let d=Dispatcher::new(gate,Inert::default(),area)?;
        serve(peer,channel,conn,session,d,capture(displays,false,config,false),false,profile.av(),profile.extended()).await
    }
}
pub(super) async fn controller(peer:&Peer,channel:&ControlChannel,mut conn:Connection,session:String,sid:[u8;16],geometry:Geometry,profile:Profile,screens:u8)->MediaResult<()>{
    // The native viewer must run on the main thread. Monitor the existing
    // authenticated WSS connection on a runtime worker, not the GUI thread.
    let (done,mut wait)=tokio::sync::oneshot::channel::<()>();
    let monitor=tokio::spawn(async move{
        let result:MediaResult<()>=async{
            loop{
                match wait.try_recv(){
                    Ok(())=>{conn.send(Client::Close{session:session.clone()}).await?;break finish(&mut conn,&session).await;},
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed)=>break Err("Viewer supervision ended".into()),
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty)=>tick(&mut conn).await?,
                }
            }
        }.await;
        if result.is_err(){CANCEL.store(true,Ordering::Release);}
        let close=conn.close().await;result?;close
    });
    out(if profile.native_control(){"CONNECTED ROUTED CONTROL"}else{"CONNECTED ROUTED VIEW"})?;
    let result=viewer::run(peer,channel,sid,geometry,profile.native_control(),screens,profile.av(),profile.extended());
    if profile.av(){peer.mute_audio();}
    let (frames,released)=match result{
        Ok(v)=>v,
        Err(e)=>{let _=done.send(());let _=monitor.await;return Err(e);}
    };
    if !released{
        channel.send(STOP)?;
        let deadline=Instant::now()+Duration::from_secs(3);
        loop{
            peer.check_health()?;
            match channel.receive()?{
                Some(b) if b==ACK=>break,
                Some(b) if profile.native_view()&&b.len()==3&&b[0]==VIEW_LAYOUT_ACK&&b[2]==screens&&b[1]<=screens=>{},
                Some(b) if profile.extended()&&b.first()==Some(&runtime_quality::STATUS)=>{runtime_quality::status(&b)?;},
                Some(_)=>return Err("Unexpected control response while closing".into()),
                None=>{},
            }
            if Instant::now()>=deadline{return Err("Input release not confirmed".into());}
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    out("INPUT_RELEASED")?;
    let _=done.send(());
    monitor.await.map_err(error)??;
    out(&format!("PASS ROUTED CONTROLLER: decoded_frames={frames}, release_ack=true"))
}
