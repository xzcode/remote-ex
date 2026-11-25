//! Local SQLite public-key pin book. A saved pin establishes identity only,
//! not an input, screen, file, unattended or future session grant.
use remote_ex_security::{fingerprint,PublicKey};
use remote_ex_signaling::{decode_hex,hex};
use rusqlite::{params,Connection};
use serde::Serialize;
use std::path::Path;
use tauri::{Manager,WebviewWindow};
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]
pub struct TrustedDevice { pub device_id:String,pub public_key:String,pub label:String }
fn canonical_key(s:&str)->Result<PublicKey,String>{
    let bytes=decode_hex::<32>(s).map_err(|_|"Expected full 64-character hexadecimal key")?;
    if bytes==[0;32]{return Err("Zero key is not valid".into());}Ok(bytes)
}
fn scope(s:&str)->Result<String,String>{Ok(hex(&canonical_key(s)?))}
fn open(path:&Path)->Result<Connection,String>{
    let db=Connection::open(path).map_err(|_|"Cannot open trusted device database")?;
    db.busy_timeout(std::time::Duration::from_secs(2)).map_err(|_|"Cannot configure database")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS trusted_device (
      server_id TEXT NOT NULL, device_id TEXT NOT NULL, public_key TEXT NOT NULL,
      label TEXT NOT NULL COLLATE NOCASE,
      PRIMARY KEY(server_id,device_id), UNIQUE(server_id,label)
    );").map_err(|_|"Cannot initialize trusted devices")?;
    Ok(db)
}
fn database(window:&WebviewWindow)->Result<Connection,String>{
    if window.label()!="main"{return Err("Local main window required".into());}
    let dir=window.app_handle().path().app_local_data_dir().map_err(|_|"No app data directory")?;
    std::fs::create_dir_all(&dir).map_err(|_|"Cannot create local app data directory")?;
    open(&dir.join("trusted-devices.sqlite3"))
}
fn list(db:&Connection,server:&str)->Result<Vec<TrustedDevice>,String>{
    let server=scope(server)?;
    let mut statement=db.prepare("SELECT device_id,public_key,label FROM trusted_device WHERE server_id=?1 ORDER BY label LIMIT 257").map_err(|_|"Cannot read trust records")?;
    let rows=statement.query_map([server],|r|Ok(TrustedDevice{device_id:r.get(0)?,public_key:r.get(1)?,label:r.get(2)?})).map_err(|_|"Cannot read trust records")?;
    let records=rows.collect::<Result<Vec<_>,_>>().map_err(|_|"Invalid trust record")?;
    if records.len()>256{return Err("Too many trust records".into());}
    for d in &records{if hex(&fingerprint(&canonical_key(&d.public_key)?))!=d.device_id{return Err("Stored device key does not match its fingerprint".into());}}
    Ok(records)
}
fn add(db:&mut Connection,server:&str,id:&str,key:&str,label:&str,confirmed:bool)->Result<(),String>{
    if !confirmed{return Err("Independently verify and explicitly confirm the device key".into());}
    let server=scope(server)?;let key=canonical_key(key)?;
    let device_id=hex(&fingerprint(&key));
    if scope(id)?!=device_id{return Err("Device ID does not match public key".into());}
    let label=label.trim();
    if label.is_empty()||label.chars().count()>64||label.chars().any(char::is_control){return Err("Invalid device name".into());}
    let tx=db.transaction().map_err(|_|"Trust database busy")?;
    let count:u32=tx.query_row("SELECT count(*) FROM trusted_device WHERE server_id=?1",[&server],|r|r.get(0)).map_err(|_|"Cannot count trust records")?;
    if count>=256{return Err("Trust record limit exceeded".into());}
    tx.execute("INSERT INTO trusted_device(server_id,device_id,public_key,label) VALUES(?1,?2,?3,?4)",
        params![server,device_id,hex(&key),label]).map_err(|_|"Device or name already exists. Revoke the old pin before trusting a changed key.")?;
    tx.commit().map_err(|_|"Cannot save trusted device".to_owned())
}
fn remove(db:&Connection,server:&str,id:&str,confirmed:bool)->Result<(),String>{
    if !confirmed{return Err("Confirm removal of the device pin".into());}
    if db.execute("DELETE FROM trusted_device WHERE server_id=?1 AND device_id=?2",params![scope(server)?,scope(id)?]).map_err(|_|"Cannot revoke device pin")?!=1{return Err("Device pin not found".into());}
    Ok(())
}
#[tauri::command]
pub fn trusted_list(window:WebviewWindow,server_id:String)->Result<Vec<TrustedDevice>,String>{list(&database(&window)?,&server_id)}
#[tauri::command]
pub fn trusted_add(window:WebviewWindow,server_id:String,device_id:String,public_key:String,label:String,confirmed:bool)->Result<Vec<TrustedDevice>,String>{
    let mut db=database(&window)?;add(&mut db,&server_id,&device_id,&public_key,&label,confirmed)?;list(&db,&server_id)
}
#[tauri::command]
pub fn trusted_remove(window:WebviewWindow,server_id:String,device_id:String,confirmed:bool)->Result<Vec<TrustedDevice>,String>{
    let db=database(&window)?;remove(&db,&server_id,&device_id,confirmed)?;list(&db,&server_id)
}
#[cfg(test)]
mod tests{
    use super::*;
    use remote_ex_security::DeviceIdentity;
    #[test]
    fn scoped_pins_require_confirmation_and_never_silently_replace_keys(){
        let mut db=open(Path::new(":memory:")).unwrap();
        let a=DeviceIdentity::generate();let b=DeviceIdentity::generate();
        let server="01".repeat(32);let id=hex(&a.fingerprint());let key=hex(&a.public_key());
        assert!(add(&mut db,&server,&id,&key,"Work",false).is_err());
        assert!(add(&mut db,&server,&id,&hex(&b.public_key()),"Work",true).is_err());
        add(&mut db,&server,&id,&key,"Work",true).unwrap();
        assert!(add(&mut db,&server,&hex(&b.fingerprint()),&hex(&b.public_key()),"work",true).is_err());
        assert_eq!(list(&db,&server).unwrap()[0].device_id,id);
        assert!(list(&db,&"02".repeat(32)).unwrap().is_empty());
        assert!(remove(&db,&server,&id,false).is_err());
        remove(&db,&server,&id,true).unwrap();assert!(list(&db,&server).unwrap().is_empty());
    }
}
