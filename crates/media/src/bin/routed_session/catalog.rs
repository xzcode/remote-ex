//! Explicit read-only file browsing under a single pre-authorized local
//! directory. The controller may select ONE immutable catalog index; it
//! never sends filesystem paths or commands to the host.
use super::*;
use remote_ex_desktop_features::{directory::{self,Catalog,decode_catalog,CATALOG_LIMIT,MAX_BATCH_FILES},exchange::{self as content,Kind,Offer}};
use std::{io::{Cursor,Seek,SeekFrom},path::Path,sync::atomic::AtomicUsize};
pub(super) static SELECTED:AtomicUsize=AtomicUsize::new(usize::MAX);
static BATCH:std::sync::Mutex<Option<Vec<usize>>>=std::sync::Mutex::new(None);
const BATCH_SELECT:u8=0xC3;
pub(super) fn select_batch(raw:&[u8])->Result<(),String>{
    let s=std::str::from_utf8(raw).map_err(|_|"Invalid batch selection")?;
    let indexes=s.split(',').map(|v|{
        if v.is_empty()||v.len()>3||!v.bytes().all(|b|b.is_ascii_digit()){return Err("Invalid selection index".to_owned());}
        v.parse::<usize>().ok().filter(|n|*n<256).ok_or("Invalid selection index".to_owned())
    }).collect::<Result<Vec<_>,_>>()?;
    if indexes.is_empty()||indexes.len()>MAX_BATCH_FILES||indexes.iter().enumerate().any(|(i,n)|indexes[..i].contains(n)){return Err("Invalid batch selection".into());}
    let mut selected=BATCH.lock().map_err(|_|"Selection unavailable")?;
    if selected.is_some(){return Err("Selection already submitted".into());}
    *selected=Some(indexes);Ok(())
}
pub(super) const HOST_DONE:&[u8]=b"remote-ex-catalog/host-done/v1";
const SELECT:u8=0xC1;const WAIT:u8=0xC2;
const LIMIT:Duration=Duration::from_secs(75);
async fn offer(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,deadline:Instant)->MediaResult<Offer>{
    let bytes=exchange::receive(peer,channel,conn,deadline).await?;
    if bytes.first()!=Some(&exchange::OFFER){return Err("Expected catalog transfer offer".into());}
    let o:Offer=serde_json::from_slice(&bytes[1..]).map_err(error)?;o.validate()?;Ok(o)
}
pub(super) async fn host(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,root:&Path,batch:bool)->MediaResult<()>{
    if canceled(){return Err("Catalog permission revoked".into());}
    let deadline=Instant::now()+LIMIT;
    let catalog=Catalog::open(root)?;
    let data=catalog.manifest()?;
    let manifest=content::describe(data.as_slice(),Kind::Directory,"catalog.json".into())?;
    out("CONNECTED ROUTED CATALOG")?;
    exchange::send_source(&mut Cursor::new(data),&manifest,peer,channel,conn,deadline).await?;
    if batch{
        let indexes=loop{
            let bytes=exchange::receive(peer,channel,conn,deadline).await?;
            if bytes==[WAIT]{continue;}
            if bytes.first()!=Some(&BATCH_SELECT)||bytes.len()<3||bytes.len()>1+2*MAX_BATCH_FILES||bytes.len()%2!=1{return Err("Invalid batch selection frame".into());}
            break bytes[1..].chunks_exact(2).map(|c|usize::from(u16::from_be_bytes([c[0],c[1]]))).collect::<Vec<_>>();
        };
        if canceled(){return Err("Catalog permission revoked".into());}
        let data=catalog.pack_selected(&indexes)?;
        if canceled(){return Err("Catalog permission revoked".into());}
        let offer=content::describe(data.as_slice(),Kind::Directory,"selected-files.rexdir".into())?;
        exchange::send_source(&mut Cursor::new(data),&offer,peer,channel,conn,deadline).await?;
        return out("TRANSFER_VERIFIED");
    }
    let selected=loop{
        let bytes=exchange::receive(peer,channel,conn,deadline).await?;
        if bytes==[WAIT]{continue;}
        if bytes.len()!=3||bytes[0]!=SELECT{return Err("Invalid catalog selection".into());}
        break usize::from(u16::from_be_bytes([bytes[1],bytes[2]]));
    };
    if canceled(){return Err("Catalog permission revoked".into());}
    let(mut file,entry)=catalog.open_file(selected)?;
    let name=entry.parts.last().ok_or("Missing catalog file name")?.clone();
    let offer=content::describe(&mut file,Kind::File,name)?;
    if offer.size!=entry.size{return Err("Selected catalog entry changed".into());}
    file.seek(SeekFrom::Start(0)).map_err(error)?;
    exchange::send_source(&mut file,&offer,peer,channel,conn,deadline).await?;
    out("TRANSFER_VERIFIED")
}
pub(super) async fn controller(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,target:&Path,batch:bool)->MediaResult<()>{
    if canceled()||!target.is_absolute()||target.file_name().is_none()||target.exists()||!target.parent().is_some_and(|p|p.is_dir()){
        return Err("Choose an unused absolute local file destination".into());
    }
    let deadline=Instant::now()+LIMIT;
    let o=offer(peer,channel,conn,deadline).await?;
    if o.kind!=Kind::Directory||o.name!="catalog.json"||o.size>CATALOG_LIMIT as u64{return Err("Invalid catalog manifest offer".into());}
    let manifest=exchange::receive_to(Vec::new(),o,peer,channel,conn,deadline,&[]).await?;
    let entries=decode_catalog(&manifest)?;
    channel.send(&[exchange::DONE])?;
    out("CONNECTED ROUTED CATALOG")?;
    for (index,entry) in entries.iter().enumerate(){
        let json=serde_json::to_string(entry).map_err(error)?;
        out(&format!("CATALOG_ENTRY {index} {json}"))?;
        // Pace bounded supervisor output; never let a large list overwhelm
        // the parent pipe queue. Keep observing the authenticated route.
        if index%8==7{for _ in 0..5{if canceled(){return Err("Catalog permission revoked".into());}tick(conn).await?;}}
    }
    out(&format!("CATALOG_READY {}",entries.len()))?;
    let mut keepalive=Instant::now()-Duration::from_secs(2);
    let selection=loop{
        if canceled()||Instant::now()>=deadline{return Err("Catalog selection canceled or timed out".into());}
        connected(peer,channel)?;
        if channel.receive()?.is_some(){return Err("Unexpected catalog data before local selection".into());}
        if batch{
            let pending=BATCH.lock().map_err(|_|"Selection unavailable")?.clone();
            if let Some(indexes)=pending{break indexes;}
        }else{
            let n=SELECTED.load(Ordering::Acquire);
            if n!=usize::MAX{break vec![n];}
        }
        if keepalive.elapsed()>=Duration::from_secs(1){channel.send(&[WAIT])?;keepalive=Instant::now();}
        tick(conn).await?;
    };
    if batch{
        directory::selected_entries(&entries,&selection)?;
        let mut frame=vec![BATCH_SELECT];
        for &index in &selection{frame.extend_from_slice(&(index as u16).to_be_bytes());}
        channel.send(&frame)?;
        let o=offer(peer,channel,conn,deadline).await?;
        if o.kind!=Kind::Directory||o.name!="selected-files.rexdir"{return Err("Unexpected batch download offer".into());}
        let data=exchange::receive_to(Vec::new(),o,peer,channel,conn,deadline,&[]).await?;
        directory::unpack_selected(&data,&entries,&selection,target,true,canceled)?;
        channel.send(&[exchange::DONE])?;
        return out("TRANSFER_VERIFIED");
    }
    let index=selection[0];
    let selected=entries.get(index).filter(|e|!e.directory).ok_or("Select one catalog file")?;
    let n=(index as u16).to_be_bytes();channel.send(&[SELECT,n[0],n[1]])?;
    let o=offer(peer,channel,conn,deadline).await?;
    if o.kind!=Kind::File||o.size!=selected.size||selected.parts.last()!=Some(&o.name){return Err("Downloaded offer differs from selected catalog entry".into());}
    let parent=target.parent().ok_or("Missing destination directory")?;
    let mut tmp=tempfile::NamedTempFile::new_in(parent).map_err(|_|"Cannot create temporary destination")?;
    exchange::receive_to(tmp.as_file_mut(),o,peer,channel,conn,deadline,&[]).await?;
    if canceled(){return Err("Catalog permission revoked".into());}
    tmp.as_file().sync_all().map_err(error)?;
    tmp.persist_noclobber(target).map_err(|_|"Cannot publish selected new file without overwrite")?;
    channel.send(&[exchange::DONE])?;
    out("TRANSFER_VERIFIED")
}
