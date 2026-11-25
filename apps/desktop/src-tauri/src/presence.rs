//! Explicit server registration preview. Does not accept remote requests.
use remote_ex_rendezvous_client::{Connection,Endpoint,PresenceEvent,presence};
use remote_ex_security::DeviceIdentity;
use remote_ex_signaling::decode_hex;
use serde::Serialize;
use std::{sync::{Arc,Mutex,MutexGuard},time::Duration};
use tauri::{State,WebviewWindow};
use tokio::sync::watch;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
enum Stage{Offline,Connecting,Online,Stopping,Failed}
impl Stage{fn active(self)->bool{matches!(self,Self::Connecting|Self::Online|Self::Stopping)}}
#[derive(Clone,Serialize)]
pub struct Snapshot{
    stage:Stage,
    active:bool,
    device_id:Option<String>,
    persistent:bool,
    heartbeats:u32,
    declined_requests:u32,
    detail:Option<String>,
}
impl Default for Snapshot{
    fn default()->Self{Self{stage:Stage::Offline,active:false,device_id:None,persistent:false,heartbeats:0,declined_requests:0,detail:None}}
}
#[derive(Default)]
struct Inner{snapshot:Snapshot,stop:Option<watch::Sender<bool>>}
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
async fn worker(inner:Arc<Mutex<Inner>>,endpoint:Endpoint,persistent:bool,mut stop:watch::Receiver<bool>)->Result<(),String>{
    // Key store operations may prompt or block. They never run on the UI
    // thread; cancellation prevents any later network registration.
    let id=tokio::select!{
        biased;
        _=stop.changed()=>return Ok(()),
        result=tauri::async_runtime::spawn_blocking(move||identity(persistent))=>result.map_err(|_|"Identity task failed")??,
    };
    if *stop.borrow(){return Ok(());}
    let mut conn=tokio::select!{
        biased;
        _=stop.changed()=>return Ok(()),
        result=Connection::register(endpoint,&id)=>result?,
    };
    {
        let mut state=locked(&inner);
        if !*stop.borrow(){
            state.snapshot.stage=Stage::Online;
            state.snapshot.device_id=Some(conn.device_id().into());
        }
    }
    let result=presence(&mut conn,stop,Duration::from_secs(300),|event|{
        let mut state=locked(&inner);
        match event{
            PresenceEvent::Heartbeat=>state.snapshot.heartbeats=state.snapshot.heartbeats.saturating_add(1),
            PresenceEvent::RequestDeclined=>state.snapshot.declined_requests=state.snapshot.declined_requests.saturating_add(1),
        }
    }).await;
    // Close the outbound registration even after a failure; no automatic
    // reconnection, no inbound OS commands, no capture or input authority.
    let close=conn.close().await;
    result?;
    close
}
impl PresenceManager{
    fn snapshot(&self)->Snapshot{locked(&self.inner).snapshot.clone()}
    fn start(&self,url:&str,server_id:&str,development:bool,persistent:bool,storage_consent:bool)->Result<Snapshot,String>{
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
        state.stop=Some(tx);
        state.snapshot=Snapshot{stage:Stage::Connecting,active:true,persistent,..Snapshot::default()};
        let inner=self.inner.clone();
        tauri::async_runtime::spawn(async move{
            let result=worker(inner.clone(),endpoint,persistent,rx).await;
            let mut state=locked(&inner);
            state.snapshot.stage=if result.is_ok(){Stage::Offline}else{Stage::Failed};
            state.snapshot.active=false;
            state.snapshot.detail=result.err();
            state.stop=None;
        });
        Ok(state.snapshot.clone())
    }
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
pub fn presence_start(window:WebviewWindow,manager:State<'_,PresenceManager>,url:String,server_id:String,development:bool,persistent:bool,storage_consent:bool)->Result<Snapshot,String>{
    main_window(&window)?;
    manager.start(url.trim(),server_id.trim(),development,persistent,storage_consent)
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
        assert!(m.start("ws://192.168.1.2/v1/ws","",true,false,false).is_err());
        assert!(m.start("ws://127.0.0.1/v1/ws","",true,true,true).is_err());
        assert!(m.start("wss://example.com/v1/ws",&"01".repeat(32),false,true,false).is_err());
        assert!(m.start("wss://example.com/v1/ws","",false,false,false).is_err());
        assert!(!m.snapshot().active);
    }
}
