//! Separately granted, one-shot file management. Local host approval binds a
//! specific reviewed batch to the session, source fingerprints and destination.
//! All mutations are relative to the locally opened directory capability.
//! No permanent deletion, arbitrary paths, overwrite, shell or execution.
//! Directory relocation uses native exclusive same-volume rename, never copy/delete.
use super::{Catalog,Entry,component,check_metadata,MAX_DEPTH,MAX_ENTRIES};
use crate::{checkpoint::digest,exchange::MAX_FILE};
use cap_std::{ambient_authority,fs::{Dir,OpenOptions}};
// Keep native exclusive rename FFI isolated from the safe core.
#[allow(unsafe_code)]
mod exclusive_move;
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,io::{Read,ErrorKind},path::Path};
pub const MAX_PLAN_BYTES:usize=900;
pub const MAX_CHANGES:usize=4;
pub const MAX_REVIEW_BYTES:usize=3600;
const RECOVERY_PREFIX:&str="Remote-EX-Recovery-";

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(tag="op",rename_all="snake_case",deny_unknown_fields)]
pub enum Change{
    CreateFolder{parent:Option<usize>,name:String},
    CreateFile{parent:Option<usize>,name:String},
    Rename{index:usize,name:String},
    Recycle{index:usize},
    Restore{index:usize,parent:Option<usize>,name:String},
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan{pub changes:Vec<Change>}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewItem{
    pub operation:String,pub source:Option<Vec<String>>,pub destination:Vec<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub subtree_entries:Option<usize>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub subtree_bytes:Option<u64>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Review{pub items:Vec<ReviewItem>,pub recovery:Option<String>}
fn safe_name(s:&str)->bool{
    s.len()<=96&&component(s)&&s.trim()==s&&!s.to_ascii_lowercase().starts_with(&RECOVERY_PREFIX.to_ascii_lowercase())
        &&!s.chars().any(|c|matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200e}'|'\u{200f}'))
}
fn key(p:&[String])->String{p.iter().map(|s|s.to_lowercase()).collect::<Vec<_>>().join("/")}
fn visible_component(s:&str)->bool{component(s)&&!s.chars().any(|c|matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200e}'|'\u{200f}'))}
fn path_allowed(parts:&[String])->bool{
    !parts.is_empty()&&parts.len()<=MAX_DEPTH&&parts.iter().all(|s|visible_component(s)&&!s.to_ascii_lowercase().starts_with(&RECOVERY_PREFIX.to_ascii_lowercase()))
        &&parts.iter().map(String::len).sum::<usize>()+parts.len()<=768
}
fn is_recovery(s:&str)->bool{
    s.strip_prefix(RECOVERY_PREFIX).is_some_and(|id|id.len()==32&&id.bytes().all(|b|b.is_ascii_hexdigit()))
}
fn valid_restore(parts:&[String])->bool{
    parts.len()==2&&is_recovery(&parts[0])&&visible_component(&parts[1])&&!parts[1].to_ascii_lowercase().starts_with(&RECOVERY_PREFIX.to_ascii_lowercase())
}
fn is_prefix(a:&[String],b:&[String])->bool{
    a.len()<=b.len()&&a.iter().zip(b).all(|(x,y)|x.to_lowercase()==y.to_lowercase())
}
impl Plan{
    pub fn decode(bytes:&[u8])->Result<Self,String>{
        if bytes.is_empty()||bytes.len()>MAX_PLAN_BYTES{return Err("File operation plan exceeds limit".into());}
        let plan:Self=serde_json::from_slice(bytes).map_err(|_|"Invalid file operation plan")?;
        if plan.changes.is_empty()||plan.changes.len()>MAX_CHANGES{return Err("Select one to four file changes".into());}
        Ok(plan)
    }
    pub fn encode(&self)->Result<Vec<u8>,String>{
        let data=serde_json::to_vec(self).map_err(|_|"Cannot encode file plan")?;Self::decode(&data)?;Ok(data)
    }
    /// Resolve immutable catalog indexes and validated single-component names.
    /// All items must be independently valid against the original catalog.
    pub fn review(&self,entries:&[Entry],session:[u8;16])->Result<Review,String>{
        self.encode()?;super::validate(entries)?;
        if session==[0;16]||entries.len()>MAX_ENTRIES{return Err("Invalid file management context".into());}
        let recovery=format!("{RECOVERY_PREFIX}{}",remote_ex_signaling::hex(&session));
        let existing:BTreeSet<_>=entries.iter().map(|e|key(&e.parts)).collect();
        let mut sources=BTreeSet::new();let mut destinations=BTreeSet::new();
        let mut items=Vec::new();let mut uses_recovery=false;
        for (n,change) in self.changes.iter().enumerate(){
            let (operation,source,destination)=match change{
                Change::CreateFolder{parent,name}|Change::CreateFile{parent,name}=>{
                    if !safe_name(name){return Err("Invalid new name".into());}
                    let mut parts=match parent{None=>Vec::new(),Some(i)=>entries.get(*i).filter(|e|e.directory&&path_allowed(&e.parts)).ok_or("Invalid parent directory")?.parts.clone()};
                    parts.push(name.clone());
                    (if matches!(change,Change::CreateFolder{..}){"create_folder"}else{"create_file"},None,parts)
                },
                Change::Rename{index,name}=>{
                    if !safe_name(name){return Err("Invalid rename target".into());}
                    let entry=entries.get(*index).filter(|e|path_allowed(&e.parts)).ok_or("Choose a listed file or directory outside recovery")?;
                    let mut parts=entry.parts.clone();*parts.last_mut().ok_or("Invalid source name")?=name.clone();
                    ("rename",Some(entry.parts.clone()),parts)
                },
                Change::Recycle{index}=>{
                    let entry=entries.get(*index).filter(|e|path_allowed(&e.parts)).ok_or("Choose a listed file or directory outside recovery")?;
                    uses_recovery=true;
                    // Preserve each full filename without collisions. No remote
                    // naming authority over the recovery directory.
                    let target=format!("{n}-{}",entry.parts.last().ok_or("Invalid source name")?);
                    if !component(&target){return Err("Recovery filename exceeds limit".into());}
                    ("recycle",Some(entry.parts.clone()),vec![recovery.clone(),target])
                },
                Change::Restore{index,parent,name}=>{
                    if !safe_name(name){return Err("Invalid restored name".into());}
                    let entry=entries.get(*index).filter(|e|valid_restore(&e.parts)).ok_or("Select one top-level entry in a recovery folder")?;
                    let mut parts=match parent{None=>Vec::new(),Some(i)=>entries.get(*i).filter(|e|e.directory&&path_allowed(&e.parts)).ok_or("Invalid restore parent")?.parts.clone()};
                    parts.push(name.clone());("restore",Some(entry.parts.clone()),parts)
                },
            };
            if operation!="recycle"&&!path_allowed(&destination){return Err("Invalid destination".into());}
            if existing.contains(&key(&destination))||!destinations.insert(key(&destination)){return Err("Destination exists or conflicts with another change".into());}

            let(mut subtree_entries,mut subtree_bytes)=(None,None);
            if let Some(src)=&source{
                if !sources.insert(key(src)){return Err("Select each source only once".into());}
                if is_prefix(src,&destination)||is_prefix(&destination,src){return Err("Source and destination overlap".into());}
                let entry=entries.iter().find(|e|&e.parts==src).ok_or("Source not in catalog")?;
                if entry.directory{
                    let children=entries.iter().filter(|e|e.parts.len()>src.len()&&e.parts.starts_with(src)).collect::<Vec<_>>();
                    let mut bytes=0u64;
                    for child in &children{
                        let tail=&child.parts[src.len()..];
                        if tail.iter().any(|s|!visible_component(s)||s.to_ascii_lowercase().starts_with(&RECOVERY_PREFIX.to_ascii_lowercase())){return Err("Directory contains reserved or unsafe paths".into());}
                        let mut moved=destination.clone();moved.extend_from_slice(tail);
                        if moved.len()>MAX_DEPTH||moved.iter().map(String::len).sum::<usize>()+moved.len()>768{return Err("Moved subtree exceeds directory limits".into());}
                        bytes=bytes.checked_add(child.size).filter(|n|*n<=MAX_FILE).ok_or("Directory exceeds limit")?;
                    }
                    subtree_entries=Some(children.len());subtree_bytes=Some(bytes);
                }
            }
            items.push(ReviewItem{operation:operation.into(),source,destination,subtree_entries,subtree_bytes});
        }
        for (i,a) in items.iter().enumerate(){
            for b in &items[..i]{
                let mut ap=vec![a.destination.as_slice()];if let Some(src)=&a.source{ap.push(src.as_slice());}
                let mut bp=vec![b.destination.as_slice()];if let Some(src)=&b.source{bp.push(src.as_slice());}
                if ap.iter().any(|x|bp.iter().any(|y|is_prefix(x,y)||is_prefix(y,x))){return Err("Overlapping file change paths; use separate reviews".into());}
            }
        }
        let created=items.iter().filter(|i|i.source.is_none()).count()+usize::from(uses_recovery);
        if entries.len()+created>MAX_ENTRIES{return Err("Changes would exceed catalog entry limit".into());}
        if uses_recovery&&existing.contains(&recovery.to_lowercase()){return Err("Recovery directory already exists".into());}
        let review=Review{items,recovery:uses_recovery.then_some(recovery)};
        if serde_json::to_vec(&review).map_err(|_|"Cannot encode review")?.len()>MAX_REVIEW_BYTES{return Err("Review exceeds display limit".into());}
        Ok(review)
    }
}
pub struct Manager{catalog:Catalog,root:Dir}
pub struct Prepared{review:Review,id:String,fingerprints:Vec<Option<(u64,[u8;32])>>}
impl Prepared{
    pub fn review(&self)->&Review{&self.review}
    pub fn approval_id(&self)->&str{&self.id}
}
fn no_existing(dir:&Dir,name:&str)->Result<(),String>{
    match dir.symlink_metadata(name){
        Err(e) if e.kind()==ErrorKind::NotFound=>Ok(()),
        _=>Err("Destination already exists or cannot be checked".into()),
    }
}
fn checked_parent(root:&Dir,parts:&[String])->Result<Dir,String>{
    let mut dir=root.try_clone().map_err(|_|"Cannot open approved directory handle")?;
    for p in parts{
        if !component(p){return Err("Invalid directory component".into());}
        let m=dir.symlink_metadata(p).map_err(|_|"Directory changed")?;
        if m.is_symlink()||!m.is_dir(){return Err("Directory changed or unsafe link".into());}
        if dir.canonicalize(p).map_err(|_|"Cannot check directory")?!=Path::new(p){return Err("Directory is not the selected relative path".into());}
        dir=dir.open_dir(p).map_err(|_|"Cannot open approved directory")?;
    }
    Ok(dir)
}
fn parent_and_name<'a>(root:&Dir,parts:&'a[String])->Result<(Dir,&'a str),String>{
    let(name,parent)=parts.split_last().ok_or("Missing relative entry")?;
    Ok((checked_parent(root,parent)?,name.as_str()))
}
fn fingerprint(root:&Dir,parts:&[String])->Result<(u64,[u8;32]),String>{
    let(dir,name)=parent_and_name(root,parts)?;
    let m=dir.symlink_metadata(name).map_err(|_|"Source unavailable")?;
    if !m.is_file()||m.is_symlink()||m.len()>MAX_FILE{return Err("Source must remain a bounded regular file".into());}
    if dir.canonicalize(name).map_err(|_|"Cannot check source")?!=Path::new(name){return Err("Source changed or unsafe link".into());}
    let f=dir.open(name).map_err(|_|"Cannot read approved source")?;
    if !f.metadata().map_err(|_|"Cannot inspect source handle")?.is_file(){return Err("Source changed".into());}
    let mut reader=f.take(m.len()+1);let mut hash=Sha256::new();let mut total=0u64;let mut buffer=[0;8192];
    loop{let n=reader.read(&mut buffer).map_err(|_|"Cannot hash approved file")?;if n==0{break;}total+=n as u64;hash.update(&buffer[..n]);}
    if total!=m.len(){return Err("Source size changed".into());}Ok((total,hash.finalize().into()))
}
impl Manager{
    pub fn open(root:&Path)->Result<Self,String>{
        let catalog=Catalog::open(root)?;
        let dir=Dir::open_ambient_dir(&catalog.root,ambient_authority()).map_err(|_|"Cannot open locally approved directory")?;
        Ok(Self{catalog,root:dir})
    }
    pub fn manifest(&self)->Result<Vec<u8>,String>{self.catalog.manifest()}
    fn verify_local_root(&self)->Result<(),String>{
        let m=std::fs::symlink_metadata(&self.catalog.root).map_err(|_|"Approved root unavailable")?;
        if !check_metadata(&m)||!m.is_dir()||self.catalog.root.canonicalize().map_err(|_|"Approved root changed")?!=self.catalog.root{return Err("Approved root changed".into());}
        Ok(())
    }
    fn verify_ambient_entry(&self,parts:&[String])->Result<(),String>{
        // Retain platform reparse-point checks as well as capability-relative
        // access. Concurrent malicious LOCAL filesystem mutation is not an
        // accepted production threat boundary for this candidate.
        let mut p=self.catalog.root.clone();
        for part in parts{p.push(part);let m=std::fs::symlink_metadata(&p).map_err(|_|"Approved entry changed")?;if !check_metadata(&m){return Err("Unsafe link or special file".into());}}
        Ok(())
    }
    fn source_fingerprint(&self,parts:&[String])->Result<(u64,[u8;32]),String>{
        self.verify_ambient_entry(parts)?;
        let entry=self.catalog.entries.iter().find(|e|e.parts.as_slice()==parts).ok_or("Source not in approved catalog")?;
        if !entry.directory{
            let fp=fingerprint(&self.root,parts)?;
            if fp.0!=entry.size{return Err("File no longer matches the catalog".into());}
            return Ok(fp);
        }
        let path=super::relative(&self.catalog.root,parts);
        let current=Catalog::open(&path)?;
        let expected=self.catalog.entries.iter().filter(|e|e.parts.len()>parts.len()&&e.parts.starts_with(parts)).map(|e|Entry{parts:e.parts[parts.len()..].to_vec(),directory:e.directory,size:e.size}).collect::<Vec<_>>();
        if current.entries!=expected{return Err("Directory tree no longer matches the catalog".into());}
        let mut hash=Sha256::new();hash.update(b"Remote EX/directory-fingerprint/v1");
        hash.update(current.manifest()?);
        let mut total=0u64;
        for e in &expected{
            if e.directory{continue;}
            let mut full=parts.to_vec();full.extend_from_slice(&e.parts);
            self.verify_ambient_entry(&full)?;
            let (n,h)=fingerprint(&self.root,&full)?;
            if n!=e.size{return Err("Directory file changed size".into());}
            total=total.checked_add(n).filter(|n|*n<=MAX_FILE).ok_or("Directory exceeds limit")?;
            hash.update(n.to_be_bytes());hash.update(h);
        }
        Ok((total,hash.finalize().into()))
    }
    pub fn prepare(&self,plan:&Plan,session:[u8;16])->Result<Prepared,String>{
        self.verify_local_root()?;
        let review=plan.review(&self.catalog.entries,session)?;
        let mut fingerprints=Vec::new();
        for item in &review.items{
            if let Some(parts)=&item.source{
                let fp=self.source_fingerprint(parts)?;
                fingerprints.push(Some(fp));
            }else{fingerprints.push(None);}
            if item.operation!="recycle"{
                self.verify_ambient_entry(&item.destination[..item.destination.len()-1])?;
                let(dir,name)=parent_and_name(&self.root,&item.destination)?;no_existing(&dir,name)?;
            }
        }
        if let Some(name)=&review.recovery{no_existing(&self.root,name)?;}
        let mut context=b"Remote EX/managed-file-approval/v1\0".to_vec();context.extend_from_slice(&session);
        context.extend(serde_json::to_vec(&review).map_err(|_|"Cannot encode review")?);
        context.extend(serde_json::to_vec(&fingerprints).map_err(|_|"Cannot bind source fingerprints")?);
        Ok(Prepared{id:remote_ex_signaling::hex(&digest(&context)),review,fingerprints})
    }
    pub fn apply(&self,prepared:Prepared,approved_id:&str,mut canceled:impl FnMut()->bool)->Result<usize,String>{
        if approved_id!=prepared.id||canceled(){return Err("Exact local file review approval required".into());}
        self.verify_local_root()?;
        // Recheck ALL entries before any mutation.
        for (item,expected) in prepared.review.items.iter().zip(&prepared.fingerprints){
            if let Some(src)=&item.source{if Some(self.source_fingerprint(src)?)!=*expected{return Err("Source changed after local review".into());}}
            if item.operation!="recycle"{self.verify_ambient_entry(&item.destination[..item.destination.len()-1])?;let(dir,name)=parent_and_name(&self.root,&item.destination)?;no_existing(&dir,name)?;}
        }
        if canceled(){return Err("File operation canceled".into());}
        if let Some(name)=&prepared.review.recovery{no_existing(&self.root,name)?;self.root.create_dir(name).map_err(|_|"Cannot create new recovery directory")?;}
        let mut done=0;
        for (item,expected) in prepared.review.items.iter().zip(&prepared.fingerprints){
            let result=(||->Result<(),String>{
                if canceled(){return Err("File operation canceled".into());}
                let(to,dest)=parent_and_name(&self.root,&item.destination)?;no_existing(&to,dest)?;
                match item.operation.as_str(){
                    "create_folder"=>to.create_dir(dest).map_err(|_|"Cannot create directory".into()),
                    "create_file"=>{let mut options=OpenOptions::new();options.write(true).create_new(true);let file=to.open_with(dest,&options).map_err(|_|"Cannot create new empty file")?;file.sync_all().map_err(|_|"Cannot flush new empty file".into())},
                    "rename"|"recycle"|"restore"=>{
                        let src=item.source.as_ref().ok_or("Missing reviewed source")?;
                        if Some(self.source_fingerprint(src)?)!=*expected{return Err("Source changed after local review".into());}
                        let(from,name)=parent_and_name(&self.root,src)?;
                        if canceled(){return Err("File operation canceled".into());}
                        exclusive_move::move_entry(&from,name,&to,dest,&super::relative(&self.catalog.root,src),&super::relative(&self.catalog.root,&item.destination))
                    },
                    _=>Err("Unknown reviewed operation".into()),
                }
            })();
            if let Err(e)=result{return Err(format!("{done} completed; {e}. Check the local directory and recovery folder; no automatic rollback."));}
            done+=1;
        }
        Ok(done)
    }
}
#[cfg(test)]mod tests{
    use super::*;
    use std::fs;
    fn fixture()->(tempfile::TempDir,Manager){
        let d=tempfile::tempdir().unwrap();fs::write(d.path().join("a.txt"),b"alpha").unwrap();fs::write(d.path().join("b.txt"),b"beta").unwrap();
        let manager=Manager::open(d.path()).unwrap();(d,manager)
    }
    #[test]fn exact_review_creates_and_moves_without_overwrite(){
        let(d,m)=fixture();let plan=Plan{changes:vec![Change::CreateFolder{parent:None,name:"new".into()},Change::Rename{index:0,name:"renamed.txt".into()},Change::Recycle{index:1}]};
        let prep=m.prepare(&plan,[1;16]).unwrap();assert!(m.apply(prep,"wrong",||false).is_err());assert!(d.path().join("a.txt").exists());
        let prep=m.prepare(&plan,[1;16]).unwrap();let id=prep.approval_id().to_owned();let recovery=prep.review().recovery.clone().unwrap();
        assert_eq!(m.apply(prep,&id,||false).unwrap(),3);
        assert!(d.path().join("new").is_dir());assert_eq!(fs::read(d.path().join("renamed.txt")).unwrap(),b"alpha");
        assert_eq!(fs::read(d.path().join(recovery).join("2-b.txt")).unwrap(),b"beta");
        assert!(!d.path().join("a.txt").exists());assert!(!d.path().join("b.txt").exists());
    }
    #[cfg(any(target_os="linux",target_os="macos",target_os="windows"))]
    #[test]fn directory_rename_recovery_restore_and_stale_tree(){
        let d=tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("old/sub")).unwrap();
        fs::write(d.path().join("old/sub/data.txt"),b"alpha").unwrap();
        let m=Manager::open(d.path()).unwrap();
        let rename=Plan{changes:vec![Change::Rename{index:0,name:"new".into()}]};
        let prep=m.prepare(&rename,[7;16]).unwrap();
        assert_eq!(prep.review().items[0].subtree_entries,Some(2));
        assert_eq!(prep.review().items[0].subtree_bytes,Some(5));
        let id=prep.approval_id().to_owned();
        fs::write(d.path().join("old/sub/data.txt"),b"other").unwrap();
        assert!(m.apply(prep,&id,||false).is_err());assert!(!d.path().join("new").exists());
        let prep=m.prepare(&rename,[7;16]).unwrap();let id=prep.approval_id().to_owned();
        assert_eq!(m.apply(prep,&id,||false).unwrap(),1);drop(m);
        assert_eq!(fs::read(d.path().join("new/sub/data.txt")).unwrap(),b"other");
        let m=Manager::open(d.path()).unwrap();
        let p=Plan{changes:vec![Change::Recycle{index:0}]};
        let prep=m.prepare(&p,[8;16]).unwrap();let recovery=prep.review().recovery.clone().unwrap();let id=prep.approval_id().to_owned();
        assert_eq!(m.apply(prep,&id,||false).unwrap(),1);drop(m);
        assert!(!d.path().join("new").exists());assert_eq!(fs::read(d.path().join(&recovery).join("0-new/sub/data.txt")).unwrap(),b"other");
        let m=Manager::open(d.path()).unwrap();
        let index=m.catalog.entries.iter().position(|e|e.parts.len()==2&&e.parts[0]==recovery&&e.parts[1]=="0-new").unwrap();
        assert!(m.prepare(&Plan{changes:vec![Change::Rename{index,name:"bad".into()}]},[9;16]).is_err());
        let plan=Plan{changes:vec![Change::Restore{index,parent:None,name:"restored".into()}]};
        let prep=m.prepare(&plan,[9;16]).unwrap();let id=prep.approval_id().to_owned();
        assert_eq!(m.apply(prep,&id,||false).unwrap(),1);
        assert_eq!(fs::read(d.path().join("restored/sub/data.txt")).unwrap(),b"other");
        assert!(!d.path().join(&recovery).join("0-new").exists());
    }
    #[test]fn rejects_overlapping_directory_changes(){
        let d=tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("dir")).unwrap();fs::write(d.path().join("dir/a"),b"data").unwrap();
        let m=Manager::open(d.path()).unwrap();
        let p=Plan{changes:vec![Change::Rename{index:0,name:"renamed".into()},Change::Recycle{index:1}]};
        assert!(m.prepare(&p,[1;16]).is_err());
        let p=Plan{changes:vec![Change::Recycle{index:0},Change::CreateFile{parent:Some(0),name:"x".into()}]};
        assert!(m.prepare(&p,[1;16]).is_err());
    }
    #[test]fn rejects_escape_collision_stale_and_cancellation(){
        let(d,m)=fixture();
        for name in ["../bad","a.txt","NUL.txt","a/b"]{
            assert!(m.prepare(&Plan{changes:vec![Change::Rename{index:1,name:name.into()}]},[2;16]).is_err());
        }
        let plan=Plan{changes:vec![Change::Rename{index:0,name:"renamed.txt".into()}]};
        let prep=m.prepare(&plan,[2;16]).unwrap();let id=prep.approval_id().to_owned();fs::write(d.path().join("a.txt"),b"other").unwrap();
        assert!(m.apply(prep,&id,||false).is_err());assert!(!d.path().join("renamed.txt").exists());
        let prep=m.prepare(&plan,[3;16]).unwrap();let id=prep.approval_id().to_owned();
        assert!(m.apply(prep,&id,||true).is_err());assert!(d.path().join("a.txt").exists());
    }
}
