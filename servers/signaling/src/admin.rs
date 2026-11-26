//! Offline operator staging tools. Enrollment permits signaling registration
//! only. No service reconfiguration, automatic apply/restart, network listener,
//! identity generation or remote session grant.
use std::{collections::{BTreeSet,HashSet},io::{Read,Write},path::Path};
use serde::Serialize;
use sha2::{Digest,Sha256};
use remote_ex_signaling::{hex,decode_hex};
const MAX_BYTES:usize=512*1024;
const MAX_IDS:usize=4096;
type Result<T>=std::result::Result<T,String>;
#[derive(Clone,Debug)]
pub struct Allowlist{ids:BTreeSet<String>}
#[derive(Serialize)]
pub struct Summary{schema:u32,count:usize,canonical_sha256:String,network_started:bool}
impl Allowlist{
    pub fn parse(text:&str)->Result<Self>{
        if text.len()>MAX_BYTES{return Err("Device allowlist exceeds limit".into());}
        let mut ids=BTreeSet::new();
        for line in text.lines().map(str::trim).filter(|l|!l.is_empty()&&!l.starts_with('#')){
            if !crate::valid_device_id(line){return Err("Invalid device allowlist entry".into());}
            ids.insert(line.to_owned());
            if ids.len()>MAX_IDS{return Err("Device allowlist count exceeds limit".into());}
        }
        if ids.is_empty(){return Err("Device allowlist cannot be empty".into());}
        Ok(Self{ids})
    }
    pub fn read(path:&Path)->Result<Self>{
        // Existing service configurations can use managed symlinks to regular
        // files. Staging never replaces an existing output or symlink.
        let meta=std::fs::metadata(path).map_err(|_|"Cannot inspect device allowlist")?;
        if !meta.is_file()||meta.len()>MAX_BYTES as u64{return Err("Invalid allowlist file type or size".into());}
        let file=std::fs::File::open(path).map_err(|_|"Cannot open device allowlist")?;
        let mut text=String::new();file.take(MAX_BYTES as u64+1).read_to_string(&mut text).map_err(|_|"Cannot read device allowlist")?;
        Self::parse(&text)
    }
    pub fn into_set(self)->HashSet<String>{self.ids.into_iter().collect()}
    fn text(&self)->String{let mut text=String::new();for id in &self.ids{text.push_str(id);text.push('\n');}text}
    pub fn difference(&self,next:&Self)->serde_json::Value{serde_json::json!({"schema":1,"before":self.summary(),"after":next.summary(),"added":next.ids.difference(&self.ids).count(),"removed":self.ids.difference(&next.ids).count(),"unchanged":self.ids.intersection(&next.ids).count(),"active_config_changed":false})}
    pub fn summary(&self)->Summary{Summary{schema:1,count:self.ids.len(),canonical_sha256:hex(&Sha256::digest(self.text().as_bytes())),network_started:false}}
    fn checked_identity(id:&str,key:&str)->Result<String>{
        if !crate::valid_device_id(id)||!crate::valid_device_id(key){return Err("Use independently verified complete lowercase device ID and public key".into());}
        let key=decode_hex::<32>(key).map_err(|_|"Invalid public key")?;
        if !remote_ex_security::valid_public_key(&key){return Err("Invalid Ed25519 public key".into());}
        if hex(&remote_ex_security::fingerprint(&key))!=id{return Err("Device ID does not match public key".into());}Ok(id.into())
    }
    pub fn new_one(id:&str,key:&str)->Result<Self>{Ok(Self{ids:BTreeSet::from([Self::checked_identity(id,key)?])})}
    pub fn enroll(&mut self,id:&str,key:&str)->Result<()>{
        let id=Self::checked_identity(id,key)?;
        if self.ids.contains(&id){return Err("Device is already enrolled".into());}
        if self.ids.len()>=MAX_IDS{return Err("Device allowlist count exceeds limit".into());}
        self.ids.insert(id);Ok(())
    }
    pub fn revoke(&mut self,id:&str)->Result<()>{
        if !crate::valid_device_id(id)||!self.ids.contains(id){return Err("Device is not enrolled".into());}
        if self.ids.len()==1{return Err("Refuse empty staged allowlist; stop the service instead of disabling enrollment".into());}
        self.ids.remove(id);Ok(())
    }
    /// Write a new staging file only. No overwrite of an existing file,
    /// including the active configuration or symlink.
    pub fn stage(&self,path:&Path)->Result<()>{
        if path.file_name().is_none(){return Err("Choose a new staging filename".into());}
        let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        if std::fs::symlink_metadata(path).is_ok(){return Err("Staging output already exists".into());}
        let mut tmp=tempfile::NamedTempFile::new_in(parent).map_err(|_|"Cannot create staging file")?;
        tmp.write_all(self.text().as_bytes()).map_err(|_|"Cannot write staging file")?;
        tmp.as_file().sync_all().map_err(|_|"Cannot sync staging file")?;
        tmp.persist_noclobber(path).map_err(|_|"Cannot create new staging file without overwrite")?;
        Ok(())
    }
}
pub fn command(args:&[String])->Result<Option<serde_json::Value>>{
    let parts:Vec<&str>=args.iter().map(String::as_str).collect();
    let result=match parts.as_slice(){
        ["--protocol-info"]=>crate::contract::info(),
        ["--allowlist-diff",source,next]=>Allowlist::read(Path::new(source))?.difference(&Allowlist::read(Path::new(next))?),
        ["--allowlist-info",path]=>serde_json::to_value(Allowlist::read(Path::new(path))?.summary()).map_err(|_|"Cannot serialize allowlist summary")?,
        ["--new-allowlist",output,id,key]=>{
            let list=Allowlist::new_one(id,key)?;list.stage(Path::new(output))?;
            serde_json::json!({"schema":1,"operation":"new","after":list.summary(),"staged":true,"active_config_changed":false,"restart_required":true})
        },
        ["--enroll-plan",source,output,id,key]=>{
            let mut list=Allowlist::read(Path::new(source))?;let before=list.summary();
            list.enroll(id,key)?;list.stage(Path::new(output))?;
            serde_json::json!({"schema":1,"operation":"enroll","before":before,"after":list.summary(),"staged":true,"active_config_changed":false,"restart_required":true})
        },
        ["--revoke-plan",source,output,id]=>{
            let mut list=Allowlist::read(Path::new(source))?;let before=list.summary();
            list.revoke(id)?;list.stage(Path::new(output))?;
            serde_json::json!({"schema":1,"operation":"revoke","before":before,"after":list.summary(),"staged":true,"active_config_changed":false,"restart_required":true})
        },
        _=>return Ok(None),
    };Ok(Some(result))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn staged_changes_never_overwrite_or_remove_last(){
        let d=tempfile::tempdir().unwrap();
        let a=remote_ex_security::DeviceIdentity::generate();let b=remote_ex_security::DeviceIdentity::generate();
        let aid=hex(&a.fingerprint());let bid=hex(&b.fingerprint());let ak=hex(&a.public_key());let bk=hex(&b.public_key());
        let active=d.path().join("active");let next=d.path().join("next");
        let mut list=Allowlist::new_one(&aid,&ak).unwrap();list.stage(&active).unwrap();
        assert!(list.stage(&active).is_err());assert!(list.revoke(&aid).is_err());assert!(list.enroll(&aid,&bk).is_err());
        list.enroll(&bid,&bk).unwrap();list.stage(&next).unwrap();
        assert_eq!(Allowlist::read(&active).unwrap().ids.len(),1);
        assert_eq!(Allowlist::read(&next).unwrap().ids.len(),2);
        assert!(list.enroll(&bid,&bk).is_err());list.revoke(&aid).unwrap();assert!(list.revoke(&bid).is_err());
        assert!(Allowlist::parse("# no ids").is_err());assert!(Allowlist::parse("ABC").is_err());
    }
}
