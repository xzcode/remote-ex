//! Independently authorized, bounded content transfer primitives. No remote
//! path traversal, automatic clipboard capture, shell commands or overwrite.
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::io::{Read,Write};
pub const MAX_FILE:u64=64*1024*1024;
pub const MAX_TEXT:u64=64*1024;
pub const CHUNK:usize=768;
#[derive(Debug,Clone,Copy,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum Kind{File,Text,Png}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Offer{pub kind:Kind,pub name:String,pub size:u64,pub sha256:[u8;32]}
impl Offer{
    pub fn validate(&self)->Result<(),String>{
        let cap=match self.kind{Kind::File=>MAX_FILE,Kind::Text=>MAX_TEXT,Kind::Png=>4*1024*1024};
        if self.size>cap||self.name.is_empty()||self.name.len()>200||self.name=="."||self.name==".."||self.name.chars().any(|c|c.is_control()||matches!(c,'/'|'\\'|':'|'\0'|'*'|'?'|'<'|'>'|'|'|'"'))||self.name.ends_with(['.',' ']){return Err("Invalid transfer offer".into());}
        Ok(())
    }
}
pub struct Receiver<W:Write>{writer:W,offer:Offer,offset:u64,hash:Sha256}
impl<W:Write> Receiver<W>{
    pub fn new(writer:W,offer:Offer,confirmed:bool)->Result<Self,String>{
        if !confirmed{return Err("Explicit receiver consent required".into());}
        offer.validate()?;Ok(Self{writer,offer,offset:0,hash:Sha256::new()})
    }
    pub fn offset(&self)->u64{self.offset}
    pub fn push(&mut self,offset:u64,data:&[u8])->Result<(),String>{
        if offset!=self.offset||data.is_empty()||data.len()>CHUNK||self.offset.checked_add(data.len() as u64).is_none_or(|end|end>self.offer.size){return Err("Invalid transfer chunk".into());}
        self.writer.write_all(data).map_err(|_|"Local write failed")?;
        self.hash.update(data);self.offset+=data.len() as u64;Ok(())
    }
    pub fn finish(mut self)->Result<W,String>{
        if self.offset!=self.offer.size||<[u8;32]>::from(self.hash.finalize())!=self.offer.sha256{return Err("Transfer integrity mismatch".into());}
        self.writer.flush().map_err(|_|"Local flush failed")?;Ok(self.writer)
    }
}
pub fn describe(mut source:impl Read,kind:Kind,name:String)->Result<Offer,String>{
    let max=match kind{Kind::File=>MAX_FILE,Kind::Text=>MAX_TEXT,Kind::Png=>4*1024*1024};
    let mut total=0u64;let mut hash=Sha256::new();let mut buf=[0u8;8192];
    loop{let n=source.read(&mut buf).map_err(|_|"Local read failed")?;if n==0{break;}total+=n as u64;if total>max{return Err("Transfer too large".into());}hash.update(&buf[..n]);}
    let offer=Offer{kind,name,size:total,sha256:hash.finalize().into()};offer.validate()?;Ok(offer)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn integrity_consent_and_bounds(){
        let bytes=b"local test";let o=describe(&bytes[..],Kind::File,"test.txt".into()).unwrap();
        assert!(Receiver::new(Vec::new(),o.clone(),false).is_err());
        let mut r=Receiver::new(Vec::new(),o.clone(),true).unwrap();
        assert!(r.push(1,bytes).is_err());r.push(0,bytes).unwrap();assert_eq!(r.finish().unwrap(),bytes);
        let mut r=Receiver::new(Vec::new(),o,true).unwrap();r.push(0,b"wrong data").unwrap();assert!(r.finish().is_err());
        assert!(describe(&bytes[..],Kind::File,"../secret".into()).is_err());
    }
}
