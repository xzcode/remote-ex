//! Local development package completeness and corruption check.
//! Not publisher authentication: unsigned manifests can be replaced locally.
//! Never resolve an arbitrary path supplied by a manifest or a remote peer.
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::{fs,io::{Read,BufReader},path::{Path,PathBuf}};
const MAX_MANIFEST:u64=16384;
const MAX_BINARY:u64=512*1024*1024;
pub const WINDOWS:&[&str]=&["lan_view.exe","lan_diagnostic.exe","lan_control.exe","routed_diagnostic.exe","routed_control.exe"];
pub const MACOS:&[&str]=&["lan_view","lan_diagnostic","lan_control","routed_diagnostic","routed_control","remote-ex-mac-audio"];
pub const LINUX:&[&str]=&["lan_view","lan_diagnostic","routed_x11_view","routed_diagnostic"];
pub fn names(platform:&str)->Result<&'static [&'static str],String>{match platform{"windows"=>Ok(WINDOWS),"macos"=>Ok(MACOS),"linux"=>Ok(LINUX),_=>Err("Unsupported bundle platform".into())}}
pub fn probe_name(platform:&str)->Result<&'static str,String>{match platform{"windows"=>Ok("routed_control.exe"),"macos"=>Ok("routed_control"),"linux"=>Ok("routed_x11_view"),_=>Err("Unsupported runtime platform".into())}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry{pub name:String,pub size:u64,pub sha256:String}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest{pub schema:u32,pub commit:String,pub platform:String,pub arch:String,pub files:Vec<Entry>}
fn lower_hex(s:&str,n:usize)->bool{s.len()==n&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
impl Manifest{
    pub fn validate(&self,platform:&str,arch:&str)->Result<(),String>{
        let allowed=names(platform)?;
        if self.schema!=1||self.platform!=platform||self.arch!=arch||!lower_hex(&self.commit,40)||self.files.len()!=allowed.len(){return Err("Invalid bundle identity or component count".into());}
        for name in allowed{
            let mut matches=self.files.iter().filter(|e|e.name==*name);
            let e=matches.next().ok_or("Missing expected bundle entry")?;
            if matches.next().is_some()||e.size==0||e.size>MAX_BINARY||!lower_hex(&e.sha256,64){return Err("Invalid or duplicate bundle component".into());}
        }Ok(())
    }
    pub fn verify_name(&self,dir:&Path,name:&str)->Result<(),String>{
        self.validate(&self.platform,&self.arch)?;
        let entry=self.files.iter().find(|e|e.name==name).ok_or("Not an expected bundle component")?;
        let path=dir.join(name);
        let meta=fs::symlink_metadata(&path).map_err(|_|format!("Missing component: {name}"))?;
        if !meta.file_type().is_file()||meta.len()!=entry.size{return Err(format!("Component type or size mismatch: {name}"));}
        let f=fs::File::open(&path).map_err(|_|format!("Cannot read component: {name}"))?;
        let mut reader=BufReader::new(f).take(entry.size+1);
        let mut hash=Sha256::new();let mut buf=[0u8;64*1024];let mut size=0u64;
        loop{let n=reader.read(&mut buf).map_err(|_|format!("Cannot read component: {name}"))?;if n==0{break;}size+=n as u64;hash.update(&buf[..n]);}
        let digest=remote_ex_signaling::hex(&hash.finalize());
        if size!=entry.size||digest!=entry.sha256{return Err(format!("Component checksum mismatch: {name}"));}Ok(())
    }
}
pub fn manifest_path(dir:&Path,platform:&str)->PathBuf{
    if platform=="macos"&&dir.file_name().is_some_and(|x|x=="MacOS"){
        if let Some(parent)=dir.parent().filter(|p|p.file_name().is_some_and(|x|x=="Contents")){
            return parent.join("Resources").join("sidecars.json");
        }
    }
    dir.join("sidecars.json")
}
/// Missing manifest is allowed for an unpackaged development tree only.
/// An existing malformed manifest must never silently become "absent".
pub fn load(dir:&Path,platform:&str,arch:&str)->Result<Option<Manifest>,String>{
    let path=manifest_path(dir,platform);
    let meta=match fs::symlink_metadata(&path){Ok(m)=>m,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(_)=>return Err("Cannot inspect component manifest".into())};
    if !meta.file_type().is_file()||meta.len()==0||meta.len()>MAX_MANIFEST{return Err("Invalid component manifest file".into());}
    let mut data=Vec::new();fs::File::open(path).map_err(|_|"Cannot open component manifest")?.take(MAX_MANIFEST+1).read_to_end(&mut data).map_err(|_|"Cannot read component manifest")?;
    if data.len() as u64>MAX_MANIFEST{return Err("Component manifest too large".into());}
    let m:Manifest=serde_json::from_slice(&data).map_err(|_|"Malformed component manifest")?;
    m.validate(platform,arch)?;Ok(Some(m))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn checks_exact_files_and_rejects_paths(){
        let root=tempfile::tempdir().unwrap();let dir=root.path();
        let content=b"fixture";
        for n in LINUX{fs::write(dir.join(n),content).unwrap();}
        let m=Manifest{schema:1,commit:"a".repeat(40),platform:"linux".into(),arch:"x86_64".into(),files:LINUX.iter().map(|n|Entry{name:(*n).into(),size:content.len()as u64,sha256:remote_ex_signaling::hex(&Sha256::digest(content))}).collect()};
        assert!(load(dir,"linux","x86_64").unwrap().is_none());
        fs::write(manifest_path(dir,"linux"),serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(load(dir,"linux","x86_64").unwrap(),Some(m.clone()));
        assert!(m.verify_name(dir,LINUX[0]).is_ok());
        fs::write(dir.join(LINUX[0]),b"changed").unwrap();
        assert!(m.verify_name(dir,LINUX[0]).is_err());assert!(m.verify_name(dir,"../fixture").is_err());
        let mut bad=m.clone();bad.files[0].name="../other".into();assert!(bad.validate("linux","x86_64").is_err());
        fs::write(manifest_path(dir,"linux"),b"{}").unwrap();assert!(load(dir,"linux","x86_64").is_err());
        assert!(m.validate("linux","aarch64").is_err());
    }
    #[test]fn macos_bundle_resources_path(){
        assert_eq!(manifest_path(Path::new("/tmp/Remote EX.app/Contents/MacOS"),"macos"),PathBuf::from("/tmp/Remote EX.app/Contents/Resources/sidecars.json"));
    }
}
