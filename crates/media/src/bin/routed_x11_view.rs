//! Linux native screen view/control sidecar, with explicitly authorized Wayland portal.
#[cfg(target_os="linux")]
mod routed_session;
#[cfg(target_os="linux")]
#[tokio::main(flavor="multi_thread")]
async fn main(){
    if remote_ex_media::runtime_probe::handle_cli(){return;}
    if let Err(e)=run().await{eprintln!("FAIL: {e}");std::process::exit(1);}
}
#[cfg(target_os="linux")]
async fn run()->Result<(),String>{
    // Explicit local capture diagnostic only. No network or saved image.
    if std::env::args().len()==2&&std::env::args().nth(1).as_deref()==Some("--probe-x11"){
        remote_ex_desktop_features::platform::require_linux_x11()?;
        let display=xcap::Monitor::all().map_err(|e|e.to_string())?.into_iter().next().ok_or("No X11 display")?;
        let id=display.id().map_err(|e|e.to_string())?;
        let width=display.width().map_err(|e|e.to_string())?;let height=display.height().map_err(|e|e.to_string())?;
        if width==0||height==0||u64::from(width)*u64::from(height)>32_000_000{return Err("Invalid X11 geometry".into());}
        let frame=display.capture_image().map_err(|e|e.to_string())?;
        if frame.width()==0||frame.height()==0||u64::from(frame.width())*u64::from(frame.height())>32_000_000{return Err("Invalid captured geometry".into());}
        println!("PASS X11 CAPTURE {id} {} {}",frame.width(),frame.height());return Ok(());
    }
    routed_session::run(true).await
}
#[cfg(not(target_os="linux"))]
fn main(){eprintln!("X11 view is Linux-only");std::process::exit(1);}
