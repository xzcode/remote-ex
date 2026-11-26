//! Test-only executable, not a deployable service or remote controller.
#[cfg(not(target_arch="wasm32"))]
fn main(){
    if std::env::args_os().len()!=1{std::process::exit(2);}
    if let Err(e)=remote_ex_web_crypto::run_native_fixture(){eprintln!("Fixture failed: {e}");std::process::exit(1);}
}
#[cfg(target_arch="wasm32")]
fn main(){}
