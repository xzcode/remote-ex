//! Explicit connection diagnosis without device registration or authorization.
use std::{sync::Mutex,time::{Duration,Instant}};
use tauri::{Manager,WebviewWindow};
use remote_ex_rendezvous_client::{Connection,Endpoint};
use serde::Serialize;
static LAST:Mutex<Option<Instant>>=Mutex::new(None);
#[derive(Serialize)]
pub struct Report{verified:bool,elapsed_ms:u64,custom_ca:bool}
#[tauri::command]
pub async fn server_preflight(window:WebviewWindow,url:String,server_id:String,ca_path:Option<String>,confirmed:bool)->Result<Report,String>{
    if window.label()!="main"||!confirmed{return Err("Explicit local server preflight required".into());}
    if window.state::<crate::lan::SessionManager>().busy()||window.state::<crate::lan::RoutedManagers>().any_busy()||window.state::<crate::presence::PresenceManager>().busy(){return Err("Finish active sessions and registration before checking the server".into());}
    if url.len()>512||url.chars().any(|c|c.is_whitespace()||c.is_control()){return Err("Invalid WSS address".into());}
    let pin=remote_ex_signaling::decode_hex::<32>(&server_id).map_err(|_|"Invalid expected server ID")?;
    let endpoint=Endpoint::secure(&url,pin)?;
    {
        let mut last=LAST.lock().map_err(|_|"Server check unavailable")?;
        if last.is_some_and(|x|x.elapsed()<Duration::from_secs(3)){return Err("Wait before checking the server again".into());}
        *last=Some(Instant::now());
    }
    let ca=ca_path.filter(|s|!s.is_empty());
    let custom=ca.is_some();
    let roots=tauri::async_runtime::spawn_blocking(move||load_roots(ca.as_deref())).await.map_err(|_|"CA check interrupted")??;
    let start=Instant::now();
    Connection::preflight_with_roots(endpoint,roots).await?;
    Ok(Report{verified:true,elapsed_ms:start.elapsed().as_millis().min(u64::MAX as u128)as u64,custom_ca:custom})
}
fn load_roots(ca:Option<&str>)->Result<rustls::RootCertStore,String>{
    match ca{Some(p)=>remote_ex_rendezvous_client::tls_roots::private(p),None=>Ok(remote_ex_rendezvous_client::tls_roots::public())}
}
