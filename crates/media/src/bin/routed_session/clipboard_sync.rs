//! Explicit, independent and time-limited text-only sync. Initial content
//! is a local baseline and is never sent. No clipboard access by other modes.
use super::*;
use remote_ex_desktop_features::{checkpoint,exchange::CHUNK};
const MAX_TEXT:usize=8192;
const NONE:u8=0xB0;const TEXT:u8=0xB1;const DATA:u8=0xB2;
const STOP_SYNC:u8=0xB3;const ACK_SYNC:u8=0xB4;
pub(super) const HOST_DONE:&[u8]=b"remote-ex-clipboard/host-done/v1";
trait Clipboard{
    fn read(&mut self)->MediaResult<Option<Vec<u8>>>;
    fn write(&mut self,data:&[u8])->MediaResult<()>;
    fn before_round(&mut self,_round:usize){}
}
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
struct Native(arboard::Clipboard);
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
impl Clipboard for Native{
    fn read(&mut self)->MediaResult<Option<Vec<u8>>>{
        if canceled(){return Err("Clipboard authorization revoked".into());}
        match self.0.get_text(){
            Ok(text) if text.len()<=MAX_TEXT=>Ok(Some(text.into_bytes())),
            Ok(_)|Err(arboard::Error::ContentNotAvailable)=>Ok(None),
            Err(_)=>Err("Cannot read local clipboard".into()),
        }
    }
    fn write(&mut self,data:&[u8])->MediaResult<()>{
        if canceled(){return Err("Clipboard authorization revoked".into());}
        let text=String::from_utf8(data.to_vec()).map_err(|_|"Invalid clipboard text")?;
        self.0.set_text(text).map_err(|_|"Cannot write local clipboard".into())
    }
}
struct Fixture{host:bool,content:Vec<u8>}
impl Clipboard for Fixture{
    fn read(&mut self)->MediaResult<Option<Vec<u8>>>{Ok(Some(self.content.clone()))}
    fn write(&mut self,data:&[u8])->MediaResult<()>{self.content=data.to_vec();Ok(())}
    fn before_round(&mut self,round:usize){
        if self.host&&round==0{self.content=b"host test update".to_vec();}
        if !self.host&&round==1{self.content=b"controller test update".to_vec();}
    }
}
async fn packet(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<Vec<u8>>{
    exchange::receive(peer,channel,conn,deadline.min(Instant::now()+Duration::from_secs(3))).await
}
fn send_text(channel:&ControlChannel,text:Option<&[u8]>)->MediaResult<()>{
    if canceled(){return Err("Clipboard authorization revoked".into());}
    let Some(data)=text else{return channel.send(&[NONE]);};
    if data.len()>MAX_TEXT||std::str::from_utf8(data).is_err(){return Err("Invalid clipboard text".into());}
    let mut header=vec![TEXT];header.extend_from_slice(&(data.len()as u32).to_be_bytes());header.extend_from_slice(&checkpoint::digest(data));
    channel.send(&header)?;
    for (n,part) in data.chunks(CHUNK).enumerate(){
        if canceled(){return Err("Clipboard authorization revoked".into());}
        let mut b=vec![DATA];b.extend_from_slice(&((n*CHUNK)as u32).to_be_bytes());b.extend_from_slice(part);
        channel.send(&b)?;
    }
    Ok(())
}
enum Message{NoChange,Text(Vec<u8>),Stop}
async fn receive_text(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<Message>{
    let h=packet(peer,channel,conn,deadline).await?;
    if h==[NONE]{return Ok(Message::NoChange);}
    if h==[STOP_SYNC]{return Ok(Message::Stop);}
    if h.len()!=37||h[0]!=TEXT{return Err("Invalid clipboard record".into());}
    let len=u32::from_be_bytes(h[1..5].try_into().map_err(error)?)as usize;
    if len>MAX_TEXT{return Err("Clipboard text exceeds limit".into());}
    let mut data=Vec::with_capacity(len);
    while data.len()<len{
        let b=packet(peer,channel,conn,deadline).await?;
        if b.len()<6||b.len()>5+CHUNK||b[0]!=DATA||u32::from_be_bytes(b[1..5].try_into().map_err(error)?)as usize!=data.len()||data.len()+b.len()-5>len{
            return Err("Invalid clipboard chunk".into());
        }
        data.extend_from_slice(&b[5..]);
    }
    if checkpoint::digest(&data).as_slice()!=&h[5..]||std::str::from_utf8(&data).is_err(){return Err("Clipboard integrity mismatch".into());}
    Ok(Message::Text(data))
}
fn observe(io:&mut impl Clipboard,last:&mut Option<Vec<u8>>)->MediaResult<Option<Vec<u8>>>{
    if canceled(){return Err("Clipboard authorization revoked".into());}
    let current=io.read()?;
    if current==*last{return Ok(None);}
    *last=current.clone();Ok(current)
}
fn apply(io:&mut impl Clipboard,last:&mut Option<Vec<u8>>,data:Vec<u8>)->MediaResult<bool>{
    if canceled(){return Err("Clipboard authorization revoked".into());}
    // If local content changed during this round, keep that change for the
    // next round rather than silently overwrite a newly copied value.
    if io.read()?!=*last{return Ok(false);}
    if last.as_ref()==Some(&data){return Ok(false);}
    io.write(&data)?;*last=Some(data);Ok(true)
}
async fn sync(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,host:bool,test:bool,mut io:impl Clipboard)->MediaResult<()>{
    if canceled(){return Err("Clipboard authorization revoked".into());}
    let mut last=io.read()?; // local baseline, do not send
    let start=Instant::now();let deadline=start+Duration::from_secs(30);
    let mut round=0usize;let mut sent=0u32;let mut received=0u32;
    out("CONNECTED ROUTED CLIPBOARD SYNC")?;
    loop{
        if canceled(){return Err("Clipboard authorization revoked".into());}
        if host&&(round>=if test{5}else{60}||Instant::now()>=deadline-Duration::from_secs(3)){
            channel.send(&[STOP_SYNC])?;
            if packet(peer,channel,conn,deadline).await?!=[ACK_SYNC]{return Err("Clipboard stop not acknowledged".into());}
            break;
        }
        if Instant::now()>=deadline{return Err("Clipboard session expired".into());}
        connected(peer,channel)?;
        io.before_round(round);
        let local=observe(&mut io,&mut last)?;
        if host{
            send_text(channel,local.as_deref())?;sent+=u32::from(local.is_some());
            match receive_text(peer,channel,conn,deadline).await?{
                Message::Text(data)=>{received+=u32::from(apply(&mut io,&mut last,data)?);},
                Message::NoChange=>{},Message::Stop=>return Err("Unexpected clipboard stop".into()),
            }
        }else{
            match receive_text(peer,channel,conn,deadline).await?{
                Message::Stop=>{channel.send(&[ACK_SYNC])?;break;},
                // In simultaneous changes, the controller's current observed
                // text wins this round. Local changes after observation wait.
                Message::Text(data) if local.is_none()=>{received+=u32::from(apply(&mut io,&mut last,data)?);},
                _=>{},
            }
            send_text(channel,local.as_deref())?;sent+=u32::from(local.is_some());
        }
        out(&format!("CLIPBOARD_SYNC_COUNTS {sent} {received}"))?;
        round+=1;
        if host{
            let next=Instant::now()+Duration::from_millis(350);
            while Instant::now()<next{if canceled(){return Err("Clipboard authorization revoked".into());}tick(conn).await?;}
        }
    }
    if test&&(sent!=1||received!=1||io.read()?.as_deref()!=Some(b"controller test update".as_slice())){
        return Err("Clipboard fixture did not converge without echo".into());
    }
    out("CLIPBOARD_SYNC_ENDED")
}
pub(super) async fn run(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,host:bool,test:bool)->MediaResult<()>{
    if test{return sync(peer,channel,conn,host,true,Fixture{host,content:if host{b"host initial private baseline".to_vec()}else{b"controller initial private baseline".to_vec()}}).await;}
    #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
    {if canceled(){return Err("Clipboard authorization revoked".into());}let io=Native(arboard::Clipboard::new().map_err(|_|"Cannot open local clipboard")?);sync(peer,channel,conn,host,false,io).await}
    #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
    {Err("Native clipboard unavailable".into())}
}
