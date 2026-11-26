//! Explicit server registration preview. Does not accept remote requests.
use remote_ex_rendezvous_client::{Endpoint,PresenceLifecycle,online_with_queries_and_roots,tls_roots};
use remote_ex_security::DeviceIdentity;
use remote_ex_signaling::{decode_hex,hex};
use serde::Serialize;
use std::{sync::{Arc,Mutex,MutexGuard},time::Duration};
use tauri::{State,WebviewWindow};
use tokio::sync::{watch,mpsc};
use remote_ex_rendezvous_protocol::DeviceStatus;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
enum Stage{Offline,Connecting,Online,Reconnecting,Stopping,Failed}
impl Stage{fn active(self)->bool{matches!(self,Self::Connecting|Self::Online|Self::Reconnecting|Self::Stopping)}}
#[derive(Clone,Serialize)]
pub struct Snapshot{
    stage:Stage,
    active:bool,
    device_id:Option<String>,
    public_key:Option<String>,
    retries:u8,
    persistent:bool,
    heartbeats:u32,
    declined_requests:u32,
    detail:Option<String>,
    devices:Vec<DeviceStatus>,
    checked_at:Option<u64>,
}
impl Default for Snapshot{
    fn default()->Self{Self{stage:Stage::Offline,active:false,device_id:None,public_key:None,retries:0,persistent:false,heartbeats:0,declined_requests:0,detail:None,devices:Vec::new(),checked_at:None}}
}
#[derive(Default)]
struct Inner{snapshot:Snapshot,stop:Option<watch::Sender<bool>>,queries:Option<mpsc::Sender<Vec<String>>>,server_id:String}
#[derive(Default)]
pub struct PresenceManager{inner:Arc<Mutex<Inner>>}
fn locked<T>(m:&Mutex<T>)->MutexGuard<'_,T>{m.lock().unwrap_or_else(|e|e.into_inner())}
fn identity(persistent:bool)->Result<DeviceIdentity,String>{
    if !persistent{return Ok(DeviceIdentity::generate());}
    #[cfg(any(target_os="windows",target_os="macos"))]
    {remote_ex_security::identity_store::native::open()}
    #[cfg(not(any(target_os="windows",target_os="macos")))]
    {Err("Persistent device identity is not available on this platform yet".into())}
}
async fn worker(inner:Arc<Mutex<Inner>>,endpoint:Endpoint,persistent:bool,reconnect:bool,ca_path:Option<String>,mut stop:watch::Receiver<bool>,mut queries:mpsc::Receiver<Vec<String>>)->Result<(),String>{
    // Resolve explicitly selected CA before requesting any identity.
    let roots=tokio::select!{biased;_=stop.changed()=>return Ok(()),r=tauri::async_runtime::spawn_blocking(move||match ca_path.as_deref(){Some(p)=>tls_roots::private(p),None=>Ok(tls_roots::public())})=>r.map_err(|_|"CA loading interrupted")??};
    // Key store operations may prompt or block. They never run on the UI
    // thread; cancellation prevents any later network registration.
    let id=tokio::select!{
        biased;
        _=stop.changed()=>return Ok(()),
        result=tauri::async_runtime::spawn_blocking(move||identity(persistent))=>result.map_err(|_|"Identity task failed")??,
    };
    if *stop.borrow(){return Ok(());}
    let public_key=hex(&id.public_key());
    online_with_queries_and_roots(endpoint,&id,stop,Duration::from_secs(300),reconnect,Some(&mut queries),roots,|event|{
        let mut state=locked(&inner);
        if state.snapshot.stage==Stage::Stopping{return;}
        match event{
            PresenceLifecycle::Online(device_id)=>{
                state.snapshot.stage=Stage::Online;state.snapshot.device_id=Some(device_id);
                state.snapshot.public_key=Some(public_key.clone());
            },
            PresenceLifecycle::Heartbeat=>state.snapshot.heartbeats=state.snapshot.heartbeats.saturating_add(1),
            PresenceLifecycle::RequestDeclined=>state.snapshot.declined_requests=state.snapshot.declined_requests.saturating_add(1),
            PresenceLifecycle::Reconnecting(attempt)=>{state.snapshot.stage=Stage::Reconnecting;state.snapshot.retries=attempt;state.snapshot.devices.clear();state.snapshot.checked_at=None;},
            PresenceLifecycle::Devices(items)=>{state.snapshot.devices=items;state.snapshot.checked_at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d|d.as_secs());},
        }
    }).await
}
impl PresenceManager{
    fn snapshot(&self)->Snapshot{locked(&self.inner).snapshot.clone()}
    fn start(&self,url:&str,server_id:&str,development:bool,persistent:bool,storage_consent:bool,reconnect:bool,ca_path:Option<String>)->Result<Snapshot,String>{
        if development&&ca_path.is_some(){return Err("Private CA requires WSS".into());}
        if url.len()>512||server_id.len()>64{return Err("Server configuration exceeds limits".into());}
        if persistent&&(!storage_consent||development){return Err("Persistent identity requires explicit consent and WSS".into());}
        let endpoint=if development{
            if !server_id.is_empty(){return Err("Loopback development does not use a server pin".into());}
            Endpoint::development(url)?
        }else{
            Endpoint::secure(url,decode_hex::<32>(server_id).map_err(|_|"Invalid configured server identity")?)?
        };
        let mut state=locked(&self.inner);
        if state.snapshot.stage.active(){return Err("A registration is already active".into());}
        let(tx,rx)=watch::channel(false);
        let(query_tx,query_rx)=mpsc::channel(1);
        state.stop=Some(tx);state.queries=Some(query_tx);state.server_id=server_id.to_ascii_lowercase();
        state.snapshot=Snapshot{stage:Stage::Connecting,active:true,persistent,..Snapshot::default()};
        let inner=self.inner.clone();
        tauri::async_runtime::spawn(async move{
            let result=worker(inner.clone(),endpoint,persistent,reconnect,ca_path,rx,query_rx).await;
            let mut state=locked(&inner);
            state.snapshot.stage=if result.is_ok(){Stage::Offline}else{Stage::Failed};
            state.snapshot.active=false;
            state.snapshot.detail=result.err();
            state.stop=None;state.queries=None;state.snapshot.devices.clear();state.snapshot.checked_at=None;
        });
        Ok(state.snapshot.clone())
    }
    pub(crate) fn busy(&self)->bool{locked(&self.inner).snapshot.active}
    pub fn request_stop(&self){
        let mut state=locked(&self.inner);
        if state.snapshot.stage.active(){
            state.snapshot.stage=Stage::Stopping;
            if let Some(stop)=&state.stop{let _=stop.send(true);}
        }
    }
}
impl Drop for PresenceManager{fn drop(&mut self){self.request_stop();}}
fn main_window(window:&WebviewWindow)->Result<(),String>{
    if window.label()=="main"{Ok(())}else{Err("Local main window required".into())}
}
#[tauri::command]
pub fn presence_start(window:WebviewWindow,manager:State<'_,PresenceManager>,url:String,server_id:String,development:bool,persistent:bool,storage_consent:bool,reconnect:bool,ca_path:Option<String>)->Result<Snapshot,String>{
    main_window(&window)?;
    manager.start(url.trim(),server_id.trim(),development,persistent,storage_consent,reconnect,ca_path.filter(|s|!s.is_empty()))
}
#[tauri::command]
pub fn presence_status(window:WebviewWindow,manager:State<'_,PresenceManager>)->Result<Snapshot,String>{
    main_window(&window)?;Ok(manager.snapshot())
}
#[tauri::command]
pub fn presence_stop(window:WebviewWindow,manager:State<'_,PresenceManager>)->Result<Snapshot,String>{
    main_window(&window)?;manager.request_stop();Ok(manager.snapshot())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rejects_untrusted_endpoints_and_implicit_persistence_before_starting(){
        let m=PresenceManager::default();
        assert!(m.start("ws://192.168.1.2/v1/ws","",true,false,false,false,None).is_err());
        assert!(m.start("ws://127.0.0.1/v1/ws","",true,true,true,false,None).is_err());
        assert!(m.start("wss://example.com/v1/ws",&"01".repeat(32),false,true,false,false,None).is_err());
        assert!(m.start("wss://example.com/v1/ws","",false,false,false,false,None).is_err());
        assert!(!m.snapshot().active);
    }
}

#[derive(Serialize)]
pub struct IdentityInfo{device_id:String,public_key:String}
/// Enables administrator enrollment before attempting registration.
#[tauri::command]
pub async fn persistent_identity_info(window:WebviewWindow,confirmed:bool)->Result<IdentityInfo,String>{
    main_window(&window)?;
    if !confirmed{return Err("Explicit system credential storage consent is required".into());}
    tauri::async_runtime::spawn_blocking(||{
        let id=identity(true)?;
        Ok(IdentityInfo{device_id:hex(&id.fingerprint()),public_key:hex(&id.public_key())})
    }).await.map_err(|_|"Identity task failed")?
}

#[tauri::command]
pub fn presence_lookup(window:WebviewWindow,manager:State<'_,PresenceManager>,server_id:String,device_ids:Vec<String>)->Result<(),String>{
    main_window(&window)?;
    if device_ids.is_empty()||device_ids.len()>32{return Err("Select up to 32 trusted devices".into());}
    let trusted=crate::trust::trusted_list(window,server_id.clone())?;
    if !device_ids.iter().all(|id|trusted.iter().any(|d|d.device_id==*id)){return Err("Lookup only supports saved trusted devices".into());}
    let state=locked(&manager.inner);
    if state.snapshot.stage!=Stage::Online||state.server_id!=server_id.to_ascii_lowercase(){return Err("Go online with this server first".into());}
    state.queries.as_ref().ok_or("Presence query unavailable")?.try_send(device_ids).map_err(|_|"A lookup is already queued".to_owned())
}
