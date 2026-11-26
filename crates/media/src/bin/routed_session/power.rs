//! One authenticated power request, no shell or OS invocation in sidecar.
use super::*;
use remote_ex_desktop_features::power::PowerReview;
use std::sync::Mutex;
const PROPOSE:u8=0xE1;const WAIT:u8=0xE2;const ACCEPT:u8=0xE3;
pub(super) const HOST_DONE:&[u8]=b"remote-ex-power/host-done/v1";
static APPROVAL:Mutex<Option<String>>=Mutex::new(None);
pub(super) fn approve(bytes:&[u8])->MediaResult<()>{
    let token=std::str::from_utf8(bytes).map_err(error)?;
    if token.len()!=64||!token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Invalid power review token".into());}
    let mut a=APPROVAL.lock().map_err(|_|"Power review unavailable")?;
    if a.is_some(){return Err("Only one local power approval".into());}*a=Some(token.into());Ok(())
}
pub(super) async fn host(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,session:[u8;16])->MediaResult<PowerReview>{
    out("CONNECTED ROUTED POWER")?;
    let deadline=Instant::now()+Duration::from_secs(55);
    let bytes=exchange::receive(peer,channel,conn,deadline).await?;
    if bytes.len()!=2||bytes[0]!=PROPOSE{return Err("Invalid power proposal".into());}
    let review=PowerAction::from_code(bytes[1])?.review(session)?;
    if canceled(){return Err("Power review revoked".into());}
    out(&format!("POWER_REVIEW {} {}",review.token,review.action.name()))?;
    let mut last=Instant::now()-Duration::from_secs(1);
    loop{
        if canceled()||Instant::now()>=deadline{return Err("Local power review canceled or expired".into());}
        connected(peer,channel)?;
        if channel.receive()?.is_some(){return Err("Unexpected second power request".into());}
        if let Some(token)=APPROVAL.lock().map_err(|_|"Local power approval unavailable")?.clone(){
            if token!=review.token{return Err("Power approval does not match exact review".into());}
            break;
        }
        if last.elapsed()>=Duration::from_secs(1){channel.send(&[WAIT])?;last=Instant::now();}
        tick(conn).await?;
    }
    if canceled(){return Err("Power review revoked".into());}
    tick(conn).await?;
    channel.send(&[ACCEPT,review.action.code()])?;Ok(review)
}
pub(super) async fn controller(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,action:PowerAction)->MediaResult<()>{
    out("CONNECTED ROUTED POWER")?;
    channel.send(&[PROPOSE,action.code()])?;
    let deadline=Instant::now()+Duration::from_secs(60);
    loop{
        let bytes=exchange::receive(peer,channel,conn,deadline).await?;
        if bytes==[WAIT]{continue;}
        if bytes!=[ACCEPT,action.code()]{return Err("Unexpected power approval receipt".into());}
        return Ok(());
    }
}
