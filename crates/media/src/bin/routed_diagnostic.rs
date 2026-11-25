//! Synthetic-only WSS test executable.
mod routed_session;
#[tokio::main(flavor="multi_thread")]
async fn main(){if let Err(e)=routed_session::run(false).await{eprintln!("FAIL: {e}");std::process::exit(1);}}
