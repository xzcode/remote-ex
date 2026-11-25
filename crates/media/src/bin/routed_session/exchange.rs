//! Separate explicit, one-way file/clipboard transfer session. No remote
//! paths or arbitrary file management; receiver chooses the exact local target.
use super::*;
use remote_ex_desktop_features::{checkpoint::{self,Scope},exchange::{self,Kind,Offer,Receiver,CHUNK}};
use std::{fs::OpenOptions,io::{Cursor,Seek,SeekFrom},path::Path};
const OFFER:u8=1;const ACCEPT:u8=2;const DATA:u8=3;const CREDIT:u8=4;const END:u8=5;const DONE:u8=6;const PAUSE:u8=7;const RESUME:u8=8;const WAIT:u8=9;const RESUME_ACCEPT:u8=10;
pub(super) static PAUSED:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
pub(super) const HOST_DONE:&[u8]=b"remote-ex-transfer/host-done/v1";
const WINDOW:usize=16;
const LIMIT:Duration=Duration::from_secs(75);
#[derive(Default)]
pub(super) struct ResumeOptions{pub from:Option<std::path::PathBuf>,pub save_to:Option<std::path::PathBuf>}
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) struct Mode{pub kind:Kind,host_sends:bool}
impl Mode{
    pub fn parse(kind:&str,sender:&str)->MediaResult<Self>{
        let kind=match kind{"file"=>Kind::File,"text"=>Kind::Text,"png"=>Kind::Png,"directory"=>Kind::Directory,_=>return Err("Invalid transfer kind".into())};
        let host_sends=match sender{"host"=>true,"controller"=>false,_=>return Err("Invalid sender role".into())};
        Ok(Self{kind,host_sends})
    }
    pub fn sending(self,host:bool)->bool{host==self.host_sends}
    pub fn code(self)->u8{if self.kind==Kind::Directory{11+u8::from(self.host_sends)}else{3+2*match self.kind{Kind::File=>0,Kind::Text=>1,Kind::Png=>2,Kind::Directory=>unreachable!()}+u8::from(self.host_sends)}}
    pub fn consent(self,host:bool)->&'static str{match(self.kind,self.sending(host)){
        (Kind::Directory,true)=>"ALLOW SEND DIRECTORY",(Kind::Directory,false)=>"ALLOW RECEIVE DIRECTORY",
        (Kind::File,true)=>"ALLOW SEND FILE",(Kind::File,false)=>"ALLOW RECEIVE FILE",
        (Kind::Text,true)=>"ALLOW SEND CLIPBOARD TEXT",(Kind::Text,false)=>"ALLOW RECEIVE CLIPBOARD TEXT",
        (Kind::Png,true)=>"ALLOW SEND CLIPBOARD IMAGE",(Kind::Png,false)=>"ALLOW RECEIVE CLIPBOARD IMAGE",
    }}
}
pub(super) async fn receive(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<Vec<u8>>{
    let step=deadline.min(Instant::now()+Duration::from_secs(10));
    loop{
        if canceled()||Instant::now()>=step{return Err("Transfer canceled or timed out".into());}
        connected(peer,channel)?;
        if let Some(bytes)=channel.receive()?{return Ok(bytes);}
        tick(conn).await?;
    }
}
fn packet(kind:u8,body:&[u8])->Vec<u8>{let mut p=vec![kind];p.extend_from_slice(body);p}
fn credit(offset:u64)->Vec<u8>{packet(CREDIT,&offset.to_be_bytes())}
fn check_local_path(path:Option<&Path>)->MediaResult<&Path>{
    let p=path.ok_or("Choose a local path")?;
    if !p.is_absolute()||p.file_name().is_none(){return Err("Choose an absolute local file path".into());}
    Ok(p)
}
async fn send_source<R:Read+Seek>(source:&mut R,offer:&Offer,peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<()>{
    let json=serde_json::to_vec(offer).map_err(error)?;
    channel.send(&packet(OFFER,&json))?;
    let response=receive(peer,channel,conn,deadline).await?;
    let mut sent=0u64;
    if response!=[ACCEPT]{
        if offer.kind!=Kind::File||response.len()!=41||response[0]!=RESUME_ACCEPT{return Err("Transfer declined or invalid resume request".into());}
        sent=u64::from_be_bytes(response[1..9].try_into().map_err(error)?);
        if sent==0||sent>offer.size{return Err("Resume offset exceeds selected source".into());}
        let mut prefix=vec![0;sent as usize];
        source.read_exact(&mut prefix).map_err(|_|"Cannot verify selected source prefix")?;
        if checkpoint::digest(&prefix).as_slice()!=&response[9..]{return Err("Receiver checkpoint differs from selected source".into());}
        out(&format!("TRANSFER_RESUME_OFFSET {sent} {}",offer.size))?;
    }
    let mut next_report=Instant::now();
    let mut buffer=[0u8;CHUNK];
    while sent<offer.size{
        if offer.kind==Kind::File&&PAUSED.load(Ordering::Acquire){
            channel.send(&[PAUSE])?;out("TRANSFER_PAUSED")?;
            let mut keepalive=Instant::now();
            while PAUSED.load(Ordering::Acquire){
                if canceled()||Instant::now()>=deadline{return Err("Transfer canceled or timed out while paused".into());}
                connected(peer,channel)?;
                if keepalive.elapsed()>=Duration::from_secs(2){channel.send(&[WAIT])?;keepalive=Instant::now();}
                tick(conn).await?;
            }
            if canceled(){return Err("Local grant revoked".into());}
            channel.send(&[RESUME])?;out("TRANSFER_RESUMED")?;
        }
        for _ in 0..WINDOW{
            if canceled()||Instant::now()>=deadline{return Err("Transfer canceled".into());}
            if sent==offer.size{break;}
            let n=usize::try_from((offer.size-sent).min(CHUNK as u64)).map_err(error)?;
            source.read_exact(&mut buffer[..n]).map_err(|_|"Local source changed or read failed")?;
            let mut bytes=packet(DATA,&sent.to_be_bytes());bytes.extend_from_slice(&buffer[..n]);
            channel.send(&bytes)?;sent+=n as u64;
        }
        if receive(peer,channel,conn,deadline).await?!=credit(sent){return Err("Invalid transfer acknowledgement".into());}
        if Instant::now()>=next_report||sent==offer.size{
            out(&format!("TRANSFER_PROGRESS {sent} {}",offer.size))?;next_report=Instant::now()+Duration::from_millis(300);
        }
    }
    let mut extra=[0u8;1];
    if source.read(&mut extra).map_err(error)?!=0{return Err("Source size changed".into());}
    channel.send(&[END])?;
    if receive(peer,channel,conn,deadline).await?!=[DONE]{return Err("Transfer completion not confirmed".into());}
    Ok(())
}
async fn receive_to<W:Write>(writer:W,offer:Offer,peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant,prefix:&[u8])->MediaResult<W>{
    let size=offer.size;let file=offer.kind==Kind::File;let mut paused=false;let mut r=Receiver::with_prefix(writer,offer,true,prefix)?;
    if prefix.is_empty(){channel.send(&[ACCEPT])?;}else{
        let mut response=packet(RESUME_ACCEPT,&r.offset().to_be_bytes());response.extend_from_slice(&checkpoint::digest(prefix));
        channel.send(&response)?;
        out(&format!("TRANSFER_RESUME_OFFSET {} {size}",r.offset()))?;
    }
    let mut count=0usize;let mut next_report=Instant::now();
    loop{
        let bytes=receive(peer,channel,conn,deadline).await?;
        match bytes.first(){
            Some(&PAUSE) if file&&!paused&&count==0&&bytes.len()==1=>{paused=true;out("TRANSFER_PAUSED")?;}
            Some(&WAIT) if file&&paused&&bytes.len()==1=>{}
            Some(&RESUME) if file&&paused&&bytes.len()==1=>{paused=false;out("TRANSFER_RESUMED")?;}
            Some(&DATA) if !paused&&bytes.len()>9=>{
                let offset=u64::from_be_bytes(bytes[1..9].try_into().map_err(error)?);
                r.push(offset,&bytes[9..])?;count+=1;
                if count==WINDOW||r.offset()==size{channel.send(&credit(r.offset()))?;count=0;}
                if Instant::now()>=next_report||r.offset()==size{
                    out(&format!("TRANSFER_PROGRESS {} {size}",r.offset()))?;next_report=Instant::now()+Duration::from_millis(300);
                }
            },
            Some(&END) if !paused&&bytes.len()==1=>return r.finish(),
            _=>return Err("Unexpected transfer packet".into()),
        }
    }
}
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
fn clipboard_read(kind:Kind)->MediaResult<Vec<u8>>{
    let mut c=arboard::Clipboard::new().map_err(|_|"Cannot open clipboard")?;
    match kind{
        Kind::Text=>{let text=c.get_text().map_err(|_|"Clipboard has no text")?;if text.len()>exchange::MAX_TEXT as usize{return Err("Clipboard text too large".into());}Ok(text.into_bytes())},
        Kind::Png=>{
            use xcap::image::{DynamicImage,RgbaImage,ImageFormat};
            let image=c.get_image().map_err(|_|"Clipboard has no image")?;
            if image.width==0||image.height==0||image.width>2048||image.height>2048{return Err("Clipboard image dimensions exceed 2048×2048".into());}
            let raw=RgbaImage::from_raw(image.width as u32,image.height as u32,image.bytes.into_owned()).ok_or("Invalid clipboard image")?;
            let mut png=Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(raw).write_to(&mut png,ImageFormat::Png).map_err(|_|"Cannot encode clipboard image")?;
            Ok(png.into_inner())
        },
        Kind::File|Kind::Directory=>Err("File is not clipboard content".into()),
    }
}
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
fn clipboard_write(kind:Kind,data:Vec<u8>)->MediaResult<()>{
    let mut c=arboard::Clipboard::new().map_err(|_|"Cannot open clipboard")?;
    match kind{
        Kind::Text=>c.set_text(String::from_utf8(data).map_err(|_|"Invalid UTF-8 clipboard text")?).map_err(|_|"Cannot write clipboard text".into()),
        Kind::Png=>{
            use xcap::image::{ImageReader,ImageFormat,Limits};
            let mut reader=ImageReader::with_format(Cursor::new(data),ImageFormat::Png);
            let mut limits=Limits::default();limits.max_image_width=Some(2048);limits.max_image_height=Some(2048);limits.max_alloc=Some(32*1024*1024);reader.limits(limits);
            let image=reader.decode().map_err(|_|"Invalid or oversized clipboard PNG")?.into_rgba8();
            let(width,height)=image.dimensions();
            c.set_image(arboard::ImageData{width:width as usize,height:height as usize,bytes:std::borrow::Cow::Owned(image.into_raw())}).map_err(|_|"Cannot write clipboard image".into())
        },
        Kind::File|Kind::Directory=>Err("File is not clipboard content".into()),
    }
}
#[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
fn clipboard_read(_:Kind)->MediaResult<Vec<u8>>{Err("Clipboard backend unavailable".into())}
#[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
fn clipboard_write(_:Kind,_:Vec<u8>)->MediaResult<()>{Err("Clipboard backend unavailable".into())}

fn read_checkpoint(path:&Path,scope:Scope,offer:&Offer)->MediaResult<Vec<u8>>{
    let meta=std::fs::symlink_metadata(path).map_err(|_|"Selected checkpoint unavailable")?;
    if !meta.file_type().is_file()||meta.len()>exchange::MAX_FILE+4110{return Err("Invalid checkpoint file".into());}
    let file=std::fs::File::open(path).map_err(|_|"Cannot open selected checkpoint")?;
    if !file.metadata().map_err(error)?.is_file(){return Err("Checkpoint must be a regular file".into());}
    checkpoint::load(file,scope,offer)
}
fn preserve_checkpoint(tmp:&mut tempfile::NamedTempFile,path:&Path,scope:Scope,offer:&Offer)->MediaResult<u64>{
    let size=tmp.as_file().metadata().map_err(error)?.len();
    if size==0{return Ok(0);}
    if size>offer.size||size>exchange::MAX_FILE{return Err("Invalid checkpoint length".into());}
    tmp.as_file_mut().seek(SeekFrom::Start(0)).map_err(error)?;
    let mut prefix=vec![0;size as usize];
    tmp.as_file_mut().read_exact(&mut prefix).map_err(error)?;
    let parent=path.parent().ok_or("Missing checkpoint directory")?;
    let mut saved=tempfile::NamedTempFile::new_in(parent).map_err(|_|"Cannot create checkpoint")?;
    checkpoint::save(&mut saved,scope,offer,&prefix)?;
    saved.as_file().sync_all().map_err(|_|"Cannot flush checkpoint")?;
    saved.persist_noclobber(path).map_err(|_|"Checkpoint exists or cannot be saved without overwrite")?;
    Ok(size)
}
pub(super) async fn run(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,_session:&str,mode:Mode,host:bool,path:Option<&Path>,scope:Scope,resume:&ResumeOptions)->MediaResult<()>{
    if canceled(){return Err("Local grant revoked".into());}
    let deadline=Instant::now()+LIMIT;
    out("CONNECTED ROUTED EXCHANGE")?;
    if mode.sending(host){
        if mode.kind==Kind::Directory{
            let path=check_local_path(path)?;
            let data=remote_ex_desktop_features::directory::pack(path)?;
            let (entries,size)=remote_ex_desktop_features::directory::inspect(&data)?;
            out(&format!("DIRECTORY_CONTENT {entries} {size}"))?;
            if canceled(){return Err("Local directory grant revoked".into());}
            let offer=exchange::describe(data.as_slice(),Kind::Directory,"directory.rexdir".into())?;
            send_source(&mut Cursor::new(data),&offer,peer,channel,conn,deadline).await?;
        }else if mode.kind==Kind::File{
            let path=check_local_path(path)?;
            let metadata=std::fs::symlink_metadata(path).map_err(|_|"Selected source unavailable")?;
            if !metadata.file_type().is_file()||metadata.len()>exchange::MAX_FILE{return Err("Select a regular file up to 64 MiB".into());}
            let mut file=OpenOptions::new().read(true).open(path).map_err(|_|"Cannot open selected source")?;
            if !file.metadata().map_err(error)?.is_file(){return Err("Source is not a regular file".into());}
            let name=path.file_name().and_then(|s|s.to_str()).ok_or("Unsupported source name")?.to_owned();
            let offer=exchange::describe(&mut file,Kind::File,name)?;
            file.seek(SeekFrom::Start(0)).map_err(error)?;
            send_source(&mut file,&offer,peer,channel,conn,deadline).await?;
        }else{
            let bytes=clipboard_read(mode.kind)?;
            let name=if mode.kind==Kind::Text{"clipboard.txt"}else{"clipboard.png"};
            let offer=exchange::describe(bytes.as_slice(),mode.kind,name.into())?;
            send_source(&mut Cursor::new(bytes),&offer,peer,channel,conn,deadline).await?;
        }
    }else{
        let bytes=receive(peer,channel,conn,deadline).await?;
        if bytes.first()!=Some(&OFFER){return Err("Expected transfer offer".into());}
        let offer:Offer=serde_json::from_slice(&bytes[1..]).map_err(|_|"Invalid transfer offer")?;
        offer.validate()?;if offer.kind!=mode.kind{return Err("Transfer kind differs from local grant".into());}
        // Recipient has already selected an exact destination and explicitly
        // approved this peer, content kind and direction. Never use offer.name
        // for filesystem paths. Never overwrite an existing file.
        if mode.kind==Kind::Directory{
            let target=check_local_path(path)?;
            if target.exists(){return Err("Destination directory exists; overwrite refused".into());}
            let content=receive_to(Vec::new(),offer,peer,channel,conn,deadline,&[]).await?;
            let (entries,size)=remote_ex_desktop_features::directory::inspect(&content)?;
            remote_ex_desktop_features::directory::unpack(&content,target,true,canceled)?;
            out(&format!("DIRECTORY_CONTENT {entries} {size}"))?;
        }else if mode.kind==Kind::File{
            let target=check_local_path(path)?;
            if target.exists(){return Err("Destination exists; overwrite refused".into());}
            let parent=target.parent().ok_or("Missing destination directory")?;
            let mut tmp=tempfile::NamedTempFile::new_in(parent).map_err(|_|"Cannot create local temporary file")?;
            let prefix=match &resume.from{Some(path)=>read_checkpoint(path,scope,&offer)?,None=>Vec::new()};
            let result=receive_to(tmp.as_file_mut(),offer.clone(),peer,channel,conn,deadline,&prefix).await.map(|_|());
            if let Err(e)=result{
                if let Some(path)=&resume.save_to{
                    match preserve_checkpoint(&mut tmp,path,scope,&offer){
                        Ok(size) if size>0=>out(&format!("CHECKPOINT_SAVED {size}"))?,
                        Ok(_)=>{},
                        Err(_)=>return Err("Transfer failed; requested checkpoint could not be saved".into()),
                    }
                }
                return Err(e);
            }
            if canceled(){return Err("Local grant revoked before saving".into());}
            tmp.as_file().sync_all().map_err(|_|"Cannot flush received file")?;
            tmp.persist_noclobber(target).map_err(|_|"Cannot save selected new file without overwriting")?;
        }else{
            let content=receive_to(Vec::new(),offer,peer,channel,conn,deadline,&[]).await?;
            if canceled(){return Err("Local grant revoked before clipboard write".into());}
            clipboard_write(mode.kind,content)?;
        }
        channel.send(&[DONE])?;
    }
    out("TRANSFER_VERIFIED")?;
    Ok(())
}
