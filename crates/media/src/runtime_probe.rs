//! Offline plugin discovery and one tiny synthetic encode/decode pipeline.
//! No screen, sound devices, identity, socket, or permission request.
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as app;
use remote_ex_desktop_features::runtime::{Probe,CORE,AUDIO,OPTIONAL};
fn synthetic()->Result<(),String>{
    let p=gst::parse::launch("videotestsrc num-buffers=1 pattern=black ! video/x-raw,width=160,height=90,framerate=10/1 ! videoconvert ! vp8enc deadline=1 ! vp8dec ! videoconvert ! video/x-raw,format=RGBA,width=160,height=90 ! appsink name=check sync=false max-buffers=1 drop=true").map_err(|_|"Synthetic pipeline creation failed")?.downcast::<gst::Pipeline>().map_err(|_|"Invalid synthetic pipeline")?;
    let result=(||{
        let sink=p.by_name("check").ok_or("Missing synthetic sink")?.downcast::<app::AppSink>().map_err(|_|"Invalid synthetic sink")?;
        p.set_state(gst::State::Playing).map_err(|_|"Synthetic pipeline start failed")?;
        let sample=sink.try_pull_sample(gst::ClockTime::from_seconds(3)).ok_or("Synthetic frame timed out")?;
        let info=crate::Peer::frame_info(&sample)?;
        if info.width!=160||info.height!=90{return Err("Synthetic geometry mismatch".into());}Ok(())
    })();
    let _=p.set_state(gst::State::Null);
    result
}
pub fn run()->Probe{
    let all=||CORE.iter().chain(AUDIO).chain(OPTIONAL);
    if gst::init().is_err(){return Probe{schema:1,initialized:false,version:None,missing:all().map(|s|(*s).to_owned()).collect(),synthetic_video:false};}
    let (a,b,c,d)=gst::version();
    let missing=all().filter(|name|gst::ElementFactory::find(**name).is_none()).map(|s|(*s).to_owned()).collect();
    Probe{schema:1,initialized:true,version:Some([a,b,c,d]),missing,synthetic_video:synthetic().is_ok()}
}
/// A dedicated exact CLI flag is recognized before any routed session parser.
pub fn handle_cli()->bool{
    let mut args=std::env::args_os().skip(1);
    if args.next().as_deref()!=Some(std::ffi::OsStr::new("--runtime-check"))||args.next().is_some(){return false;}
    let report=run();
    match serde_json::to_string(&report){Ok(s)=>println!("{s}"),Err(_)=>std::process::exit(1)}
    true
}
