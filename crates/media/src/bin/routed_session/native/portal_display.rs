//! Native Wayland screen sharing via the desktop's ScreenCast portal.
//! No XWayland, direct compositor capture, restore token or input injection.
//! The restricted PipeWire fd and selected node are local portal outputs only.
use super::*;
use ashpd::desktop::{PersistMode,screencast::{Screencast,SelectSourcesOptions,SourceType,CursorMode}};
use futures_util::StreamExt;
use gstreamer as gst;
use gst::prelude::*;
use gstreamer_app as app;
use remote_ex_desktop_features::platform::{PortalSource,PortalTarget,portal_target};
use std::{os::fd::AsRawFd,sync::mpsc::SyncSender};

struct Pipeline(gst::Pipeline);
impl Drop for Pipeline{fn drop(&mut self){let _=self.0.set_state(gst::State::Null);}}
pub(super) fn source_spec(fd:i32,version:u32,stream:&ashpd::desktop::screencast::Stream)->MediaResult<String>{
    if fd<0{return Err("Invalid portal remote fd".into());}
    let target=portal_target(version,stream.pipe_wire_node_id(),stream.pipewire_serial())?;
    let selector=match target{PortalTarget::Node(n)=>format!("path={n}"),PortalTarget::Serial(n)=>format!("target-object={n}")};
    Ok(format!("pipewiresrc name=portal_source fd={fd} {selector} client-name=RemoteEX always-copy=true"))
}
pub(super) fn restrict_stream(p:&gst::Pipeline)->MediaResult<()>{
    let src=p.by_name("portal_source").ok_or("Missing restricted PipeWire source")?;
    let props=gst::Structure::builder("props").field("node.dont-reconnect",true).field("node.dont-fallback",true).field("node.dont-move",true).build();
    if src.find_property("stream-properties").is_none(){return Err("PipeWire plugin cannot restrict source selection".into());}
    src.set_property("stream-properties",&props);Ok(())
}
fn frame_pipeline(source:&str,config:VideoConfig)->MediaResult<(Pipeline,app::AppSink)>{
    config.validate()?;
    let p=gst::parse::launch(&format!("{source} ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! videoconvert ! videoscale add-borders=true ! videorate drop-only=true ! video/x-raw,format=RGBA,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! appsink name=portal_frames sync=false max-buffers=1 drop=true",config.width,config.height,config.fps)).map_err(error)?.downcast::<gst::Pipeline>().map_err(|_|"Invalid portal pipeline")?;
    let sink=p.by_name("portal_frames").ok_or("Missing portal frame sink")?.downcast::<app::AppSink>().map_err(|_|"Invalid portal frame sink")?;
    Ok((Pipeline(p),sink))
}
fn pixels(sample:&gst::Sample,config:VideoConfig)->MediaResult<Vec<u8>>{
    let caps=sample.caps().and_then(|c|c.structure(0)).ok_or("Missing portal frame caps")?;
    if caps.get::<i32>("width").map_err(error)?!=config.width as i32||caps.get::<i32>("height").map_err(error)?!=config.height as i32||caps.get::<String>("format").map_err(error)?!="RGBA"{return Err("Unexpected portal frame format".into());}
    let buf=sample.buffer().ok_or("Missing portal frame")?;
    let map=buf.map_readable().map_err(error)?;
    if map.size()!=config.rgba_bytes(){return Err("Unsupported portal row stride".into());}
    Ok(map.as_slice().to_vec())
}
fn stopped(stop:&AtomicBool)->bool{canceled()||stop.load(Ordering::Acquire)}
async fn wait_stop(stop:&AtomicBool){while !stopped(stop){tokio::time::sleep(Duration::from_millis(50)).await;}}
async fn run(source:PortalSource,config:VideoConfig,stop:Arc<AtomicBool>,tx:&SyncSender<MediaResult<Captured>>)->MediaResult<()>{
    remote_ex_desktop_features::platform::require_wayland()?;
    let proxy=tokio::time::timeout(Duration::from_secs(5),Screencast::new()).await.map_err(|_|"ScreenCast portal unavailable")?.map_err(error)?;
    let version=proxy.version();if version<2{return Err("ScreenCast portal v2 or newer is required".into());}
    let types=match source{PortalSource::Monitor=>SourceType::MONITOR,PortalSource::Window=>SourceType::WINDOW,PortalSource::Virtual=>SourceType::VIRTUAL};
    let session=tokio::time::timeout(Duration::from_secs(5),proxy.create_session(Default::default())).await.map_err(|_|"Portal session creation timed out")?.map_err(error)?;
    let result:MediaResult<()>=async{
        let mut closed=session.receive_closed().await.map_err(error)?;
        let picker=async{
            if !proxy.available_source_types().await.map_err(error)?.contains(types){return Err("The desktop portal does not support the requested source".to_owned());}
            if !proxy.available_cursor_modes().await.map_err(error)?.contains(CursorMode::EMBEDDED){return Err("The desktop portal cannot embed the shared cursor".to_owned());}
            proxy.select_sources(&session,SelectSourcesOptions::default().set_sources(types).set_cursor_mode(CursorMode::EMBEDDED).set_multiple(false).set_persist_mode(PersistMode::DoNot)).await.map_err(error)?.response().map_err(error)?;
            let response=proxy.start(&session,None,Default::default()).await.map_err(error)?.response().map_err(error)?;
            let streams=response.streams();if streams.len()!=1{return Err("Select exactly one portal stream".into());}
            if streams[0].source_type().is_some_and(|kind|kind!=types){return Err("Portal returned a different source type".into());}
            let stream=streams[0].clone();
            let fd=proxy.open_pipe_wire_remote(&session,Default::default()).await.map_err(error)?;
            Ok((stream,fd))
        };
        let(stream,fd)=tokio::select!{
            _=wait_stop(&stop)=>return Err("Portal selection canceled".into()),
            _=closed.next()=>return Err("Portal session closed during selection".into()),
            result=tokio::time::timeout(Duration::from_secs(25),picker)=>result.map_err(|_|"Portal selection timed out")??,
        };
        if stopped(&stop){return Err("Portal sharing revoked".into());}
        // Only numeric fd and exact node from the portal response; no default
        // PipeWire server or remotely supplied source. Retain fd through stop.
        let(p,sink)=frame_pipeline(&source_spec(fd.as_raw_fd(),version,&stream)?,config)?;
        restrict_stream(&p.0)?;
        p.0.set_state(gst::State::Playing).map_err(error)?;
        let bus=p.0.bus().ok_or("Missing portal bus")?;
        let result=loop{
            if stopped(&stop){break Ok(());}
            while let Some(msg)=bus.pop(){use gst::MessageView;match msg.view(){MessageView::Error(_)|MessageView::Eos(_)=>return Err("Portal PipeWire stream stopped".into()),_=>{}}}
            tokio::select!{
                biased;
                _=closed.next()=>break Err("The local desktop revoked screen sharing".into()),
                _=tokio::time::sleep(Duration::from_millis(10))=>{},
            }
            if stopped(&stop){break Ok(());}
            if let Some(sample)=sink.try_pull_sample(gst::ClockTime::ZERO){
                let frame=Captured{layout:0,pixels:pixels(&sample,config)?,epoch:0};
                match tx.try_send(Ok(frame)){
                    Ok(())|Err(TrySendError::Full(_))=>{},
                    Err(TrySendError::Disconnected(_))=>break Ok(()),
                }
            }
        };
        drop(p);drop(fd);result
    }.await;
    // Always close the locally granted session, including cancellation and
    // selection denial. If the portal already closed, ignore its close error.
    let _=tokio::time::timeout(Duration::from_secs(1),session.close()).await;
    result
}
pub(super) fn capture(source:PortalSource,config:VideoConfig)->Capture{
    let(tx,rx)=sync_channel(1);let(done_tx,done)=sync_channel(1);
    let stop=Arc::new(AtomicBool::new(false));let worker=stop.clone();
    std::thread::spawn(move||{
        let result=match tokio::runtime::Builder::new_current_thread().enable_all().build(){
            Ok(rt)=>rt.block_on(run(source,config,worker.clone(),&tx)),Err(e)=>Err(error(e)),
        };
        if !stopped(&worker){if let Err(e)=result{let _=tx.try_send(Err(e));}}
        drop(tx);let _=done_tx.try_send(());
    });
    Capture{rx,stop,layout:Arc::new(AtomicUsize::new(0)),count:1,first_wait:Duration::from_secs(40),done:Some(done),portal:true}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn bounded_frame_conversion_without_portal_or_capture(){
        gst::init().unwrap();
        let c=VideoConfig{width:320,height:180,fps:10,bitrate:200_000};
        let(p,sink)=frame_pipeline("videotestsrc num-buffers=3",c).unwrap();
        p.0.set_state(gst::State::Playing).unwrap();
        let sample=sink.try_pull_sample(gst::ClockTime::from_seconds(3)).unwrap();
        assert_eq!(pixels(&sample,c).unwrap().len(),c.rgba_bytes());
    }
}
