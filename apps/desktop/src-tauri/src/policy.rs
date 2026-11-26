//! Explicit, revocable local host policies. A trusted public key alone
//! never grants access. Policies survive application restarts but are inert
//! until separately activated by the local user, one visible session at a time.
use rusqlite::{params,Connection,OptionalExtension};
use serde::{Serialize,Deserialize};
use std::{path::Path,time::{SystemTime,UNIX_EPOCH}};
use tauri::{Manager,WebviewWindow};
use crate::{trust,capture};
#[derive(Clone,Debug,Serialize,PartialEq,Eq)]
pub(crate) struct Policy{
    pub server_id:String,pub device_id:String,pub peer_key:String,pub host_key:String,pub url:String,
    pub display_id:u32,pub display_name:String,pub display_width:u32,pub display_height:u32,
    pub mode:String,pub quality:String,pub relay_only:bool,pub extended:bool,pub expires_at:u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft{server_id:String,device_id:String,url:String,display_id:u32,mode:String,quality:String,relay_only:bool,extended:bool,days:u32}
pub(crate) fn now()->Result<u64,String>{SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_secs()).map_err(|_|"Invalid system clock".into())}
fn validate(p:&Policy)->Result<(),String>{
    let scope=trust::scope(&p.server_id)?;
    if scope!=p.server_id||trust::scope(&p.device_id)?!=p.device_id||trust::scope(&p.host_key)?!=p.host_key||trust::scope(&p.peer_key)?!=p.peer_key||p.peer_key==p.host_key{return Err("Invalid host policy identity".into());}
    let key=remote_ex_signaling::decode_hex::<32>(&p.peer_key).map_err(|_|"Invalid policy peer key")?;
    if remote_ex_signaling::hex(&remote_ex_security::fingerprint(&key))!=p.device_id{return Err("Policy key mismatch".into());}
    if !matches!(p.mode.as_str(),"view"|"control")||p.display_width==0||p.display_height==0||u64::from(p.display_width)*u64::from(p.display_height)>32_000_000||p.display_name.len()>512||p.display_name.chars().any(char::is_control)||p.url.len()>512{return Err("Invalid host policy scope".into());}
    remote_ex_desktop_features::quality::preset(&p.quality)?;
    let server=remote_ex_signaling::decode_hex::<32>(&p.server_id).map_err(|_|"Invalid server ID")?;
    remote_ex_rendezvous_client::Endpoint::secure(&p.url,server)?;Ok(())
}
fn row(r:&rusqlite::Row<'_>)->rusqlite::Result<Policy>{Ok(Policy{server_id:r.get(0)?,device_id:r.get(1)?,peer_key:r.get(2)?,host_key:r.get(3)?,url:r.get(4)?,display_id:r.get(5)?,display_name:r.get(6)?,display_width:r.get(7)?,display_height:r.get(8)?,mode:r.get(9)?,quality:r.get(10)?,relay_only:r.get(11)?,extended:r.get(12)?,expires_at:r.get(13)?})}
const SELECT:&str="SELECT p.server_id,p.device_id,p.peer_key,p.host_key,p.url,p.display_id,p.display_name,p.display_width,p.display_height,p.mode,p.quality,p.relay_only,p.extended,p.expires_at FROM host_policy p JOIN trusted_device d ON d.server_id=p.server_id AND d.device_id=p.device_id AND d.public_key=p.peer_key";
fn list(db:&Connection)->Result<Vec<Policy>,String>{
    let mut s=db.prepare(&format!("{SELECT} ORDER BY p.server_id,p.device_id LIMIT 65")).map_err(|_|"Cannot list host policies")?;
    let items=s.query_map([],row).map_err(|_|"Cannot list host policies")?.collect::<Result<Vec<_>,_>>().map_err(|_|"Invalid stored policy")?;
    if items.len()>64{return Err("Host policy limit exceeded".into());}
    for p in &items{validate(p)?;}Ok(items)
}
fn insert(db:&mut Connection,p:&Policy,confirmed:bool,at:u64)->Result<(),String>{
    if !confirmed{return Err("Confirm exact host policy scope".into());}validate(p)?;
    if p.expires_at<=at||p.expires_at-at>30*86400{return Err("Host policy expires in 1–30 days".into());}
    let tx=db.transaction().map_err(|_|"Host policy database busy")?;
    let key=tx.query_row("SELECT public_key FROM trusted_device WHERE server_id=?1 AND device_id=?2",params![p.server_id,p.device_id],|r|r.get::<_,String>(0)).optional().map_err(|_|"Cannot verify trusted peer")?;
    if key.as_deref()!=Some(p.peer_key.as_str()){return Err("Host policy requires an existing exact trusted key".into());}
    let n:u32=tx.query_row("SELECT count(*) FROM host_policy",[],|r|r.get(0)).map_err(|_|"Cannot count host policies")?;
    if n>=64{return Err("Host policy limit exceeded".into());}
    tx.execute("INSERT INTO host_policy(server_id,device_id,peer_key,host_key,url,display_id,display_name,display_width,display_height,mode,quality,relay_only,extended,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![p.server_id,p.device_id,p.peer_key,p.host_key,p.url,p.display_id,p.display_name,p.display_width,p.display_height,p.mode,p.quality,p.relay_only,p.extended,p.expires_at]).map_err(|_|"Revoke any existing policy before creating a replacement")?;
    tx.commit().map_err(|_|"Cannot save host policy".into())
}
fn get_at(db:&Connection,server:&str,device:&str,at:u64)->Result<Policy,String>{
    let p=db.query_row(&format!("{SELECT} WHERE p.server_id=?1 AND p.device_id=?2"),params![trust::scope(server)?,trust::scope(device)?],row).optional().map_err(|_|"Cannot read host policy")?.ok_or("Policy missing or trusted key revoked")?;
    validate(&p)?;
    if p.expires_at<=at||p.expires_at-at>30*86400{return Err("Saved host policy expired or clock invalid".into());}Ok(p)
}
pub(crate) fn get(path:&Path,server:&str,device:&str)->Result<Policy,String>{get_at(&trust::open(path)?,server,device,now()?)}
pub(crate) fn verify_display(p:&Policy)->Result<(),String>{
    let display=capture::enumerate()?.into_iter().find(|d|d.id==p.display_id).ok_or("Saved display disconnected")?;
    if display.name!=p.display_name||display.width!=p.display_width||display.height!=p.display_height||p.mode=="control"&&!display.primary{return Err("Saved display geometry or identity changed; revoke and create a new policy".into());}Ok(())
}
#[tauri::command]
pub fn host_policy_list(window:WebviewWindow)->Result<Vec<Policy>,String>{list(&trust::open(&trust::database_path(&window)?)?)}
#[tauri::command]
pub async fn host_policy_save(window:WebviewWindow,draft:Draft,confirmed:bool)->Result<Vec<Policy>,String>{
    if !confirmed||!matches!(draft.days,1|7|30){return Err("Confirm the exact policy and select a 1, 7 or 30 day expiry".into());}
    let path=trust::database_path(&window)?;
    #[cfg(any(target_os="windows",target_os="macos"))]
    {
        tauri::async_runtime::spawn_blocking(move||{
            let server=trust::scope(&draft.server_id)?;let device=trust::scope(&draft.device_id)?;
            let mut db=trust::open(&path)?;
            let peer=trust::list(&db,&server)?.into_iter().find(|d|d.device_id==device).ok_or("Choose a saved, independently verified controller")?;
            let display=capture::enumerate()?.into_iter().find(|d|d.id==draft.display_id).ok_or("Choose an available display")?;
            if draft.mode=="control"&&!display.primary{return Err("Saved control policies require the primary display".into());}
            let host=remote_ex_security::identity_store::native::open_slot(0)?;
            let at=now()?;
            let p=Policy{server_id:server,device_id:device,peer_key:peer.public_key,host_key:remote_ex_signaling::hex(&host.public_key()),url:draft.url.trim().into(),
                display_id:display.id,display_name:display.name,display_width:display.width,display_height:display.height,mode:draft.mode,quality:draft.quality,relay_only:draft.relay_only,extended:draft.extended,expires_at:at.checked_add(u64::from(draft.days)*86400).ok_or("Expiry overflow")?};
            insert(&mut db,&p,true,at)?;list(&db)
        }).await.map_err(|_|"Host policy save task failed")?
    }
    #[cfg(not(any(target_os="windows",target_os="macos")))]
    {let _=(path,draft);Err("Persistent host policies currently require Windows/macOS device identity storage".into())}
}
#[tauri::command]
pub fn host_policy_revoke(window:WebviewWindow,server_id:String,device_id:String,confirmed:bool)->Result<Vec<Policy>,String>{
    if !confirmed{return Err("Confirm revoking this host policy".into());}
    let server=trust::scope(&server_id)?;let device=trust::scope(&device_id)?;
    let db=trust::open(&trust::database_path(&window)?)?;
    if db.execute("DELETE FROM host_policy WHERE server_id=?1 AND device_id=?2",params![server,device]).map_err(|_|"Cannot revoke policy")?!=1{return Err("Host policy not found".into());}
    window.state::<crate::lan::RoutedManagers>().revoke_pin(&server,&device);list(&db)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn policy_scope_expiry_and_trust_cascade(){
        let mut db=trust::open(Path::new(":memory:")).unwrap();
        let peer=remote_ex_security::DeviceIdentity::generate();let host=remote_ex_security::DeviceIdentity::generate();
        let p=Policy{server_id:"01".repeat(32),device_id:remote_ex_signaling::hex(&peer.fingerprint()),peer_key:remote_ex_signaling::hex(&peer.public_key()),host_key:remote_ex_signaling::hex(&host.public_key()),url:"wss://example.com/v1/ws".into(),display_id:1,display_name:"Test".into(),display_width:1920,display_height:1080,mode:"view".into(),quality:"compat".into(),relay_only:false,extended:false,expires_at:100+86400};
        assert!(insert(&mut db,&p,true,100).is_err());
        db.execute("INSERT INTO trusted_device(server_id,device_id,public_key,label) VALUES(?1,?2,?3,'Fixture')",params![p.server_id,p.device_id,p.peer_key]).unwrap();
        assert!(insert(&mut db,&p,false,100).is_err());
        insert(&mut db,&p,true,100).unwrap();
        assert_eq!(get_at(&db,&p.server_id,&p.device_id,100).unwrap(),p);
        assert!(insert(&mut db,&p,true,100).is_err());
        assert!(get_at(&db,&p.server_id,&p.device_id,p.expires_at).is_err());
        assert!(get_at(&db,&"02".repeat(32),&p.device_id,100).is_err());
        db.execute("DELETE FROM trusted_device WHERE server_id=?1 AND device_id=?2",params![p.server_id,p.device_id]).unwrap();
        assert!(list(&db).unwrap().is_empty());assert!(get_at(&db,&p.server_id,&p.device_id,100).is_err());
    }
}
