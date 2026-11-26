//! CI-only host for browser profile-2 protocol acceptance. It never opens
//! an OS capture or input backend and is not a distributed desktop sidecar.
mod routed_session;
#[tokio::main(flavor="multi_thread")]
async fn main(){
    if cfg!(any(feature="native-control",feature="native-x11-view",feature="local-screen")){
        eprintln!("FAIL: Browser fixture must be built without native desktop capabilities");
        std::process::exit(1);
    }
    if let Err(e)=routed_session::run_browser_control_fixture().await{
        eprintln!("FAIL: {e}");std::process::exit(1);
    }
}
