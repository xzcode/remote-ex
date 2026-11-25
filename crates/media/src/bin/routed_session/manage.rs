//! Separate visible reviewed file-management session. Controller sends
//! only bounded catalog-index changes; the host must approve the exact
//! reviewed batch locally after receiving it. No implicit permission.
use super::*;
use remote_ex_desktop_features::{
    directory::{decode_catalog,CATALOG_LIMIT,managed::{Manager,Plan,MAX_PLAN_BYTES}},
    exchange::{self as content,Kind,Offer},
};
use std::{io::Cursor,path::Path,sync::Mutex};
static PLAN:Mutex<Option<Plan>>=Mutex::new(None);
static APPROVAL:Mutex<Option<String>>=Mutex::new(None);
pub(super) const HOST_DONE:&[u8]=b"remote-ex-manage/host-done/v1";
const PROPOSE:u8=0xC3;const WAIT:u8=0xC4;const APPLIED:u8=0xC5;
const LIMIT:Duration=Duration::from_secs(75);

pub(super) fn submit(bytes:&[u8])->MediaResult<()>{
    let plan=Plan::decode(bytes)?;
    let mut p=PLAN.lock().map_err(|_|"Plan unavailable")?;
    if p.is_some(){return Err("Only one batch per session".into());}
    *p=Some(plan);Ok(())
}
pub(super) fn approve(bytes:&[u8])->MediaResult<()>{
    let token=std::str::from_utf8(bytes).map_err(|_|"Invalid review approval")?;
    if token.len()!=64||!token.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Invalid review token".into());}
    let mut a=APPROVAL.lock().map_err(|_|"Review unavailable")?;
    if a.is_some(){return Err("Only one approval per session".into());}
    *a=Some(token.to_ascii_lowercase());Ok(())
}
async fn receive(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<Vec<u8>>{
    exchange::receive(peer,channel,conn,deadline).await
}
async fn wait_local<T:Clone>(value:&Mutex<Option<T>>,peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<T>{
    let mut keepalive=Instant::now()-Duration::from_secs(1);
    loop{
        if canceled()||Instant::now()>=deadline{return Err("Local file review canceled or timed out".into());}
        connected(peer,channel)?;
        if let Some(bytes)=channel.receive()?{if bytes!=[WAIT]{return Err("Unexpected record while awaiting local review".into());}}
        if let Some(v)=value.lock().map_err(|_|"Local review unavailable")?.clone(){return Ok(v);}
        if keepalive.elapsed()>=Duration::from_secs(1){channel.send(&[WAIT])?;keepalive=Instant::now();}
        tick(conn).await?;
    }
}
pub(super) async fn host(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,root:&Path,session:[u8;16])->MediaResult<()>{
    if canceled(){return Err("File-management grant revoked".into());}
    let deadline=Instant::now()+LIMIT;
    let manager=Manager::open(root)?;
    let bytes=manager.manifest()?;
    let manifest=content::describe(bytes.as_slice(),Kind::Directory,"manage.json".into())?;
    out("CONNECTED ROUTED MANAGE")?;
    exchange::send_source(&mut Cursor::new(bytes),&manifest,peer,channel,conn,deadline).await?;
    let plan=loop{
        let data=receive(peer,channel,conn,deadline).await?;
        if data==[WAIT]{continue;}
        if data.first()!=Some(&PROPOSE)||data.len()>MAX_PLAN_BYTES+1{return Err("Invalid file change request".into());}
        break Plan::decode(&data[1..])?;
    };
    if canceled(){return Err("File-management grant revoked".into());}
    let prepared=manager.prepare(&plan,session)?;
    let review=serde_json::to_string(prepared.review()).map_err(error)?;
    // Only local supervisor receives the exact resolved paths and review ID.
    out(&format!("MANAGE_REVIEW {} {review}",prepared.approval_id()))?;
    let approval=wait_local(&APPROVAL,peer,channel,conn,deadline).await?;
    if approval!=prepared.approval_id(){return Err("Local approval does not match the reviewed changes".into());}
    connected(peer,channel)?;
    // Require the signaling route to still be live immediately before apply.
    tick(conn).await?;
    let count=manager.apply(prepared,&approval,||canceled()||Instant::now()>=deadline)?;
    channel.send(&[APPLIED,count as u8])?;
    out(&format!("MANAGE_APPLIED {count}"))
}
pub(super) async fn controller(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,session:[u8;16])->MediaResult<()>{
    let deadline=Instant::now()+LIMIT;
    let bytes=receive(peer,channel,conn,deadline).await?;
    if bytes.first()!=Some(&exchange::OFFER){return Err("Expected authorized management catalog".into());}
    let o:Offer=serde_json::from_slice(&bytes[1..]).map_err(error)?;
    o.validate()?;
    if o.kind!=Kind::Directory||o.name!="manage.json"||o.size>CATALOG_LIMIT as u64{return Err("Invalid management catalog".into());}
    let manifest=exchange::receive_to(Vec::new(),o,peer,channel,conn,deadline,&[]).await?;
    let entries=decode_catalog(&manifest)?;
    channel.send(&[exchange::DONE])?;
    out("CONNECTED ROUTED MANAGE")?;
    for (index,entry) in entries.iter().enumerate(){
        out(&format!("CATALOG_ENTRY {index} {}",serde_json::to_string(entry).map_err(error)?))?;
        if index%8==7{for _ in 0..5{if canceled(){return Err("File-management grant revoked".into());}tick(conn).await?;}}
    }
    out(&format!("CATALOG_READY {}",entries.len()))?;
    let plan=wait_local(&PLAN,peer,channel,conn,deadline).await?;
    plan.review(&entries,session)?;
    let mut packet=vec![PROPOSE];packet.extend(plan.encode()?);
    channel.send(&packet)?;
    out("MANAGE_WAITING_FOR_HOST")?;
    loop{
        let data=receive(peer,channel,conn,deadline).await?;
        if data==[WAIT]{continue;}
        if data!=[APPLIED,plan.changes.len()as u8]{return Err("Host did not confirm the full reviewed batch".into());}
        out(&format!("MANAGE_APPLIED {}",plan.changes.len()))?;
        return Ok(());
    }
}
