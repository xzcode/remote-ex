//! Explicit local checkpoint format. No authorization is stored. Both peer
//! keys and the complete source offer must match a newly approved session.
use crate::exchange::{Kind,Offer,MAX_FILE};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::io::{Read,Write};
const MAGIC:&[u8;8]=b"REXPART1";
const MAX_HEADER:usize=4096;

#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope{pub sender:[u8;32],pub receiver:[u8;32]}
impl Scope{
    fn validate(self)->Result<(),String>{
        if self.sender==[0;32]||self.receiver==[0;32]||self.sender==self.receiver{return Err("Invalid checkpoint peer scope".into());}
        Ok(())
    }
}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Header{scope:Scope,offer:Offer,offset:u64,prefix_hash:[u8;32]}
pub fn digest(data:&[u8])->[u8;32]{Sha256::digest(data).into()}
pub fn save(mut out:impl Write,scope:Scope,offer:&Offer,prefix:&[u8])->Result<(),String>{
    scope.validate()?;offer.validate()?;
    if offer.kind!=Kind::File||prefix.is_empty()||prefix.len()as u64>offer.size||prefix.len()as u64>MAX_FILE{return Err("Invalid checkpoint prefix".into());}
    let h=Header{scope,offer:offer.clone(),offset:prefix.len()as u64,prefix_hash:digest(prefix)};
    let data=serde_json::to_vec(&h).map_err(|_|"Cannot encode checkpoint")?;
    if data.len()>MAX_HEADER{return Err("Checkpoint header exceeds limit".into());}
    out.write_all(MAGIC).and_then(|_|out.write_all(&(data.len()as u32).to_be_bytes())).and_then(|_|out.write_all(&data)).and_then(|_|out.write_all(prefix)).and_then(|_|out.flush()).map_err(|_|"Cannot write checkpoint".into())
}
pub fn load(mut input:impl Read,scope:Scope,offer:&Offer)->Result<Vec<u8>,String>{
    scope.validate()?;offer.validate()?;
    let mut magic=[0;8];let mut len=[0;4];
    input.read_exact(&mut magic).and_then(|_|input.read_exact(&mut len)).map_err(|_|"Truncated checkpoint header")?;
    let n=u32::from_be_bytes(len)as usize;
    if &magic!=MAGIC||n==0||n>MAX_HEADER{return Err("Invalid checkpoint format".into());}
    let mut data=vec![0;n];input.read_exact(&mut data).map_err(|_|"Truncated checkpoint header")?;
    let h:Header=serde_json::from_slice(&data).map_err(|_|"Invalid checkpoint metadata")?;
    if h.scope!=scope||h.offer!=*offer||offer.kind!=Kind::File||h.offset==0||h.offset>offer.size||h.offset>MAX_FILE{return Err("Checkpoint peer or source file does not match this new session".into());}
    let mut prefix=vec![0;h.offset as usize];
    input.read_exact(&mut prefix).map_err(|_|"Truncated checkpoint content")?;
    let mut extra=[0u8;1];
    if input.read(&mut extra).map_err(|_|"Cannot read checkpoint")?!=0||digest(&prefix)!=h.prefix_hash{return Err("Checkpoint content integrity mismatch".into());}
    Ok(prefix)
}
#[cfg(test)]mod tests{
    use super::*;
    use crate::exchange::{describe,Receiver};
    #[test]fn resume_rehashes_prefix_and_requires_same_identity_and_offer(){
        let bytes=b"test file content";let scope=Scope{sender:[1;32],receiver:[2;32]};
        let offer=describe(&bytes[..],Kind::File,"a.txt".into()).unwrap();
        let mut encoded=Vec::new();save(&mut encoded,scope,&offer,&bytes[..7]).unwrap();
        let prefix=load(&encoded[..],scope,&offer).unwrap();
        let mut receiver=Receiver::with_prefix(Vec::new(),offer.clone(),true,&prefix).unwrap();
        assert_eq!(receiver.offset(),7);receiver.push(7,&bytes[7..]).unwrap();
        assert_eq!(receiver.finish().unwrap(),bytes);
        assert!(load(&encoded[..],Scope{sender:[3;32],receiver:[2;32]},&offer).is_err());
        let mut other=offer.clone();other.sha256=[0;32];assert!(load(&encoded[..],scope,&other).is_err());
        *encoded.last_mut().unwrap()^=1;assert!(load(&encoded[..],scope,&offer).is_err());
        assert!(Receiver::with_prefix(Vec::new(),offer,false,&prefix).is_err());
    }
}
