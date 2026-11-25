//! Native session owner; real frames and OS input are strictly after endpoint
//! authorization, signed SDP, DTLS verification and encrypted readiness.
use super::*;
use remote_ex_input::native::NativeBackend;
use std::sync::{Arc,mpsc::{sync_channel,TryRecvError,TrySendError}};
use xcap::{Monitor,image::{DynamicImage,RgbaImage,Rgba,imageops::{self,FilterType}}};

fn monitor(display:u32,area:Option<DisplayArea>,input:bool)->MediaResult<(Monitor,DisplayArea)>{
    let m=Monitor::all().map_err(error)?.into_iter().find(|m|m.id().ok()==Some(display)).ok_or("Authorized display disconnected")?;
    if input&&!m.is_primary().map_err(error)?{return Err("Control requires the primary display".into());}
    let current=DisplayArea{x:0,y:0,width:m.width().map_err(error)?,height:m.height().map_err(error)?}.validate()?;
    if area.is_some_and(|a|a!=current){return Err("Display geometry changed".into());}
    if u64::from(current.width)*u64::from(current.height)>32_000_000{return Err("Display exceeds capture limits".into());}
    Ok((m,current))
}
fn permissions(input:bool)->MediaResult<()>{
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
struct Capture{rx:std::sync::mpsc::Receiver<MediaResult<Vec<u8>>>,stop:Arc<AtomicBool>}
impl Drop for Capture{fn drop(&mut self){self.stop.store(true,Ordering::Release);}}
fn capture(display:u32,area:DisplayArea,geometry:Geometry,input:bool,config:VideoConfig)->Capture{
    let(tx,rx)=sync_channel(1);let stop=Arc::new(AtomicBool::new(false));let worker=stop.clone();
    std::thread::spawn(move||{
        while !canceled()&&!worker.load(Ordering::Acquire){
            let next=Instant::now()+Duration::from_millis(1000/u64::from(config.fps));
            let result=(||->MediaResult<Vec<u8>>{
                permissions(input)?;
                let(m,_)=monitor(display,Some(area),input)?;
                let image=m.capture_image().map_err(error)?;
                if image.width()==0||image.height()==0||u64::from(image.width())*u64::from(image.height())>32_000_000
                    ||u64::from(image.width())*u64::from(area.height)!=u64::from(image.height())*u64::from(area.width){
                    return Err("Unsupported captured geometry".into());
                }
                let scaled=DynamicImage::ImageRgba8(image).resize_exact(geometry.width,geometry.height,FilterType::Triangle).into_rgba8();
                let mut canvas=RgbaImage::from_pixel(config.width,config.height,Rgba([0,0,0,255]));
                imageops::replace(&mut canvas,&scaled,i64::from(geometry.x),i64::from(geometry.y));Ok(canvas.into_raw())
            })();
            if canceled()||worker.load(Ordering::Acquire){break;}
            let bad=result.is_err();
            match tx.try_send(result){Ok(())=>{},Err(TrySendError::Full(_)) if !bad=>{},_=>break}
            if bad{break;}
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    });
    Capture{rx,stop}
}
async fn serve<B:Backend>(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,session:&str,
    mut dispatcher:Dispatcher<B>,capture:Capture,input:bool)->MediaResult<()>{
    let started=Instant::now();let mut last=started;let mut last_frame=started;
    let mut stats=remote_ex_media::stats::Sampler::default();
    let mut bitrate=peer.config().bitrate;let ceiling=bitrate;let floor=(ceiling/4).max(100_000);let mut good=0u8;
    let result:MediaResult<()> = async{
        loop{
            if canceled(){return Ok(());}
            connected(peer,channel)?;
            if started.elapsed()>=Duration::from_secs(30){return Ok(());}
            if last.elapsed()>Duration::from_secs(2)||last_frame.elapsed()>Duration::from_secs(2){return Err("Control heartbeat or video stalled".into());}
            for _ in 0..32{
                if canceled(){return Ok(());}
                let Some(bytes)=channel.receive()? else{break;};
                if bytes==STOP{return Ok(());}
                if bytes==HEARTBEAT{last=Instant::now();}
                else if input{permissions(true)?;dispatcher.dispatch(&bytes)?;}
                else{return Err("Input is not authorized in view-only mode".into());}
            }
            match capture.rx.try_recv(){
                Ok(frame)=>{if canceled(){return Ok(());}peer.push_rgba(frame?)?;last_frame=Instant::now();},
                Err(TryRecvError::Empty)=>{},
                Err(TryRecvError::Disconnected)=>return Err("Capture worker stopped".into()),
            }
            if let Some(sample)=stats.poll(peer){
                out(&sample.line())?;
                if let Some(loss)=sample.loss_pct{
                    let old=bitrate;
                    if loss>5.0{bitrate=(bitrate*3/4).max(floor);good=0;}
                    else if loss<1.0{good=good.saturating_add(1);if good>=3{bitrate=(bitrate+ceiling/10).min(ceiling);good=0;}}else{good=0;}
                    if bitrate!=old{peer.set_video_bitrate(bitrate)?;}
                }
            }
            tick(conn).await?;
        }
    }.await;
    drop(capture);
    dispatcher.close()?;
    out("INPUT_RELEASED")?;
    result?;
    channel.send(ACK)?;
    finish(conn,session).await?;
    out("PASS ROUTED HOST: native session ended, input released")
}
pub(super) async fn host(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,cipher:&mut RouteCipher,
    session:&str,gate:SessionGate,profile:Profile,display:u32)->MediaResult<()>{
    if canceled(){return Err("Authorization revoked".into());}
    let input=profile==Profile::Control;
    permissions(input)?;
    let config=peer.config();
    let(_,area)=monitor(display,None,input)?;
    let geometry=Geometry::from_area(area,config)?;
    cipher.send(conn,geometry.signal()).await?;
    wait_data(peer,channel,conn,READY).await?;
    if canceled(){return Err("Authorization revoked".into());}
    if input{
        let backend=NativeBackend::new()?;
        if backend.primary_area()?!=area{return Err("Primary display DPI/geometry mismatch".into());}
        let d=Dispatcher::new(gate,backend,area)?;
        channel.send(READY_ACK)?;out("CONNECTED ROUTED CONTROL")?;
        serve(peer,channel,conn,session,d,capture(display,area,geometry,true,config),true).await
    }else{
        // Never instantiate a native input backend in the view-only profile.
        let d=Dispatcher::new(gate,Inert::default(),area)?;
        channel.send(READY_ACK)?;out("CONNECTED ROUTED VIEW")?;
        serve(peer,channel,conn,session,d,capture(display,area,geometry,false,config),false).await
    }
}
pub(super) async fn controller(peer:&Peer,channel:&ControlChannel,mut conn:Connection,session:String,sid:[u8;16],geometry:Geometry,profile:Profile)->MediaResult<()>{
    // The native viewer must run on the main thread. Monitor the existing
    // authenticated WSS connection on a runtime worker, not the GUI thread.
    let (done,wait)=tokio::sync::oneshot::channel::<()>();
    let monitor=tokio::spawn(async move{
        let result:MediaResult<()>=async{tokio::select!{
            _=wait=>{conn.send(Client::Close{session:session.clone()}).await?;finish(&mut conn,&session).await},
            event=conn.receive(Duration::from_secs(40))=>{let _=event?;Err("Routed session interrupted".into())}
        }}.await;
        if result.is_err(){CANCEL.store(true,Ordering::Release);}
        let close=conn.close().await;result?;close
    });
    out(if profile==Profile::Control{"CONNECTED ROUTED CONTROL"}else{"CONNECTED ROUTED VIEW"})?;
    let result=control_window::run(peer,channel,sid,geometry,profile==Profile::Control);
    let (frames,released)=match result{
        Ok(v)=>v,
        Err(e)=>{let _=done.send(());let _=monitor.await;return Err(e);}
    };
    if !released{
        channel.send(STOP)?;
        let deadline=Instant::now()+Duration::from_secs(3);
        loop{
            peer.check_health()?;
            if poll_release(channel)?{break;}
            if Instant::now()>=deadline{return Err("Input release not confirmed".into());}
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    out("INPUT_RELEASED")?;
    let _=done.send(());
    monitor.await.map_err(error)??;
    out(&format!("PASS ROUTED CONTROLLER: decoded_frames={frames}, release_ack=true"))
}
