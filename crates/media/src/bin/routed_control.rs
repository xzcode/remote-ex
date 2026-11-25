//! Separately authorized native routed control/view candidate.
mod routed_session;
#[tokio::main(flavor="multi_thread")]
async fn main(){if let Err(e)=routed_session::run(true).await{eprintln!("FAIL: {e}");std::process::exit(1);}}
