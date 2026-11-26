//! Local server address book. A saved WSS URL and server ID are configuration,
//! not peer trust, permission, authentication bypass or auto-connect.
use rusqlite::{Connection,params};
use serde::{Serialize,Deserialize};
use tauri::WebviewWindow;
use remote_ex_signaling::decode_hex;
const LIMIT:usize=64;
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile{pub label:String,pub url:String,pub server_id:String}
fn validated(p:Profile)->Result<Profile,String>{
    let label=p.label.trim();
    if label.is_empty()||label.chars().count()>64||label.chars().any(char::is_control){return Err("Invalid server profile name".into());}
    let server_id=crate::trust::scope(&p.server_id)?;
    let url=p.url.trim();
    if url.len()>512||url.chars().any(|c|c.is_whitespace()||c.is_control()){return Err("Invalid WSS server profile URL".into());}
    let pin=decode_hex::<32>(&server_id).map_err(|_|"Invalid server ID")?;
    remote_ex_rendezvous_client::Endpoint::secure(url,pin)?;
    Ok(Profile{label:label.into(),url:url.into(),server_id})
}
fn list(db:&Connection)->Result<Vec<Profile>,String>{
    let mut stmt=db.prepare("SELECT label,url,server_id FROM server_profile ORDER BY label COLLATE NOCASE LIMIT 65").map_err(|_|"Cannot read server profiles")?;
    let rows=stmt.query_map([],|r|Ok(Profile{label:r.get(0)?,url:r.get(1)?,server_id:r.get(2)?})).map_err(|_|"Cannot read server profiles")?;
    let mut out=Vec::new();
    for row in rows{let p=row.map_err(|_|"Invalid stored server profile")?;let valid=validated(p.clone())?;if valid!=p{return Err("Invalid stored server profile".into());}out.push(valid);}
    if out.len()>LIMIT{return Err("Too many saved server profiles".into());}Ok(out)
}
fn save(db:&mut Connection,p:Profile,confirmed:bool)->Result<(),String>{
    if !confirmed{return Err("Verify the server ID independently and explicitly confirm saving".into());}
    let p=validated(p)?;
    let tx=db.transaction().map_err(|_|"Server profile database busy")?;
    let count:u32=tx.query_row("SELECT COUNT(*) FROM server_profile",[],|r|r.get(0)).map_err(|_|"Cannot count server profiles")?;
    if count>=LIMIT as u32{return Err("Server profile limit reached".into());}
    // No silent update of an existing name, endpoint or identity.
    tx.execute("INSERT INTO server_profile(label,url,server_id) VALUES(?1,?2,?3)",params![p.label,p.url,p.server_id]).map_err(|_|"Server name, URL or ID already saved; explicitly remove old profile first")?;
    tx.commit().map_err(|_|"Cannot save server profile".into())
}
fn remove(db:&Connection,id:&str,confirmed:bool)->Result<(),String>{
    if !confirmed{return Err("Confirm removing this saved server profile".into());}
    if db.execute("DELETE FROM server_profile WHERE server_id=?1",[crate::trust::scope(id)?]).map_err(|_|"Cannot remove server profile")?!=1{return Err("Saved server profile not found".into());}Ok(())
}
#[tauri::command]
pub fn server_profiles_list(window:WebviewWindow)->Result<Vec<Profile>,String>{list(&crate::trust::open(&crate::trust::database_path(&window)?)?)}
#[tauri::command]
pub fn server_profile_save(window:WebviewWindow,profile:Profile,confirmed:bool)->Result<Vec<Profile>,String>{
    let mut db=crate::trust::open(&crate::trust::database_path(&window)?)?;save(&mut db,profile,confirmed)?;list(&db)
}
#[tauri::command]
pub fn server_profile_remove(window:WebviewWindow,server_id:String,confirmed:bool)->Result<Vec<Profile>,String>{
    let db=crate::trust::open(&crate::trust::database_path(&window)?)?;remove(&db,&server_id,confirmed)?;list(&db)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn explicit_config_only_never_replaces_pin(){
        let mut db=crate::trust::open(std::path::Path::new(":memory:")).unwrap();
        let p=Profile{label:"My server".into(),url:"wss://example.org:47822/v1/ws".into(),server_id:"12".repeat(32)};
        assert!(save(&mut db,p.clone(),false).is_err());
        assert!(save(&mut db,p.clone(),true).is_ok());
        assert_eq!(list(&db).unwrap(),vec![p.clone()]);
        let mut changed=p.clone();changed.server_id="34".repeat(32);
        assert!(save(&mut db,changed,true).is_err());
        for u in ["ws://example.org/v1/ws","wss://user:password@example.org/v1/ws","wss://example.org/v1/ws?secret=1","wss://example.org/other"]{let mut bad=p.clone();bad.url=u.into();assert!(validated(bad).is_err());}
        assert!(remove(&db,&p.server_id,false).is_err());assert!(remove(&db,&p.server_id,true).is_ok());assert!(list(&db).unwrap().is_empty());
    }
}
