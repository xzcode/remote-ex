//! Bounded directory bundle. Local source tree and new destination directory
//! must be explicitly selected. No remote browsing, arbitrary commands,
//! symlinks, special files, path traversal, overwrite or automatic execution.
use crate::exchange::MAX_FILE;
use serde::{Deserialize,Serialize};
use std::{collections::BTreeSet,fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
pub mod managed;
const MAGIC:&[u8;8]=b"REXDIR01";
const MAX_ENTRIES:usize=256;
const MAX_DEPTH:usize=8;
const MAX_HEADER:usize=1024*1024;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry{pub parts:Vec<String>,pub directory:bool,pub size:u64}
fn component(s:&str)->bool{
    if s.is_empty()||s.len()>200||matches!(s,"."|"..")||s.ends_with(['.',' '])
        ||s.chars().any(|c|c.is_control()||matches!(c,'/'|'\\'|':'|'*'|'?'|'<'|'>'|'|'|'"')){return false;}
    let base=s.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(base.as_str(),"CON"|"PRN"|"AUX"|"NUL"|"CONIN$"|"CONOUT$"|"COM1"|"COM2"|"COM3"|"COM4"|"COM5"|"COM6"|"COM7"|"COM8"|"COM9"|"LPT1"|"LPT2"|"LPT3"|"LPT4"|"LPT5"|"LPT6"|"LPT7"|"LPT8"|"LPT9")
}
fn check_metadata(m:&fs::Metadata)->bool{
    if m.file_type().is_symlink(){return false;}
    #[cfg(windows)]{
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes()&0x400!=0{return false;}
    }
    m.is_file()||m.is_dir()
}
fn relative(root:&Path,parts:&[String])->PathBuf{parts.iter().fold(root.to_path_buf(),|p,s|p.join(s))}
fn validate(entries:&[Entry])->Result<u64,String>{
    if entries.len()>MAX_ENTRIES{return Err("Directory entry limit exceeded".into());}
    let mut paths=BTreeSet::new();let mut folders=BTreeSet::new();let mut total=0u64;
    for e in entries{
        if e.parts.is_empty()||e.parts.len()>MAX_DEPTH||e.parts.iter().any(|s|!component(s))
            ||e.parts.iter().map(String::len).sum::<usize>()+e.parts.len()>768||e.directory&&e.size!=0{return Err("Invalid directory entry".into());}
        let key=e.parts.iter().map(|s|s.to_lowercase()).collect::<Vec<_>>().join("/");
        if !paths.insert(key.clone()){return Err("Duplicate or case-conflicting directory entry".into());}
        // Require explicit parent directories, in traversal order.
        if e.parts.len()>1{
            let parent=e.parts[..e.parts.len()-1].join("/");
            if !folders.contains(&parent){return Err("Missing directory parent".into());}
        }
        if e.directory{folders.insert(e.parts.join("/"));}
        total=total.checked_add(e.size).filter(|n|*n<=MAX_FILE).ok_or("Directory exceeds 64 MiB")?;
    }
    Ok(total)
}
fn walk(root:&Path,parts:&mut Vec<String>,entries:&mut Vec<Entry>,total:&mut u64)->Result<(),String>{
    let parent=relative(root,parts);
    let mut children=Vec::new();
    for item in fs::read_dir(&parent).map_err(|_|"Cannot list selected directory")?{
        if entries.len()+children.len()>=MAX_ENTRIES{return Err("Directory entry limit exceeded".into());}
        children.push(item.map_err(|_|"Cannot read directory entry")?);
    }
    children.sort_by_key(|e|e.file_name());
    for child in children{
        let name=child.file_name().into_string().map_err(|_|"Unsupported directory name")?;
        if !component(&name)||parts.len()>=MAX_DEPTH||entries.len()>=MAX_ENTRIES{return Err("Directory name, depth or entry limit exceeded".into());}
        let meta=fs::symlink_metadata(child.path()).map_err(|_|"Cannot inspect directory entry")?;
        if !check_metadata(&meta){return Err("Symbolic links, reparse points and special files are not supported".into());}
        parts.push(name);
        let size=if meta.is_dir(){0}else{meta.len()};
        *total=total.checked_add(size).filter(|n|*n<=MAX_FILE).ok_or("Directory exceeds 64 MiB")?;
        entries.push(Entry{parts:parts.clone(),directory:meta.is_dir(),size});
        if meta.is_dir(){walk(root,parts,entries,total)?;}
        parts.pop();
    }
    Ok(())
}
fn secure_source(root:&Path,parts:&[String])->Result<File,String>{
    let mut p=root.to_path_buf();
    for part in parts{
        p.push(part);
        let meta=fs::symlink_metadata(&p).map_err(|_|"Directory content changed")?;
        if !check_metadata(&meta){return Err("Directory content changed to an unsafe link or special file".into());}
    }
    if !p.canonicalize().map_err(|_|"Cannot resolve selected file")?.starts_with(root){return Err("File outside selected directory".into());}
    if !fs::symlink_metadata(&p).map_err(|_|"Cannot inspect selected file")?.is_file(){return Err("Selected entry is not a regular file".into());}
    let f=File::open(&p).map_err(|_|"Cannot open selected file")?;
    if !f.metadata().map_err(|_|"Cannot inspect open file")?.is_file(){return Err("Selected entry is not a regular file".into());}
    Ok(f)
}
pub const CATALOG_LIMIT:usize=512*1024;
/// Immutable, bounded catalog of a locally approved root. Only relative
/// names and sizes leave the host. Selections are indexes, never peer paths.
pub struct Catalog{root:PathBuf,entries:Vec<Entry>}
impl Catalog{
    pub fn open(root:&Path)->Result<Self,String>{
        let meta=fs::symlink_metadata(root).map_err(|_|"Selected directory unavailable")?;
        if !root.is_absolute()||!check_metadata(&meta)||!meta.is_dir(){return Err("Choose an ordinary absolute local directory".into());}
        let root=root.canonicalize().map_err(|_|"Cannot resolve selected directory")?;
        let mut entries=Vec::new();let mut total=0;
        walk(&root,&mut Vec::new(),&mut entries,&mut total)?;validate(&entries)?;
        Ok(Self{root,entries})
    }
    /// Pack only the selected files and their necessary parent directories.
    pub fn pack_selected(&self,indexes:&[usize])->Result<Vec<u8>,String>{
        let entries=selected_entries(&self.entries,indexes)?;
        let total=validate(&entries)?;
        let header=serde_json::to_vec(&entries).map_err(|_|"Cannot encode selection")?;
        if header.len()>MAX_HEADER||total+12+header.len() as u64>MAX_FILE{return Err("Selected bundle exceeds 64 MiB".into());}
        let mut data=Vec::with_capacity(total as usize+12+header.len());
        data.extend_from_slice(MAGIC);data.extend_from_slice(&(header.len() as u32).to_be_bytes());data.extend_from_slice(&header);
        for e in entries.iter().filter(|e|!e.directory){
            let index=self.entries.iter().position(|x|x==e).ok_or("Selected entry unavailable")?;
            let (file,_)=self.open_file(index)?;
            let before=data.len();
            file.take(e.size+1).read_to_end(&mut data).map_err(|_|"Cannot read selected file")?;
            if data.len()-before!=e.size as usize{return Err("Selected file changed during download".into());}
        }
        Ok(data)
    }
    pub fn manifest(&self)->Result<Vec<u8>,String>{
        let data=serde_json::to_vec(&self.entries).map_err(|_|"Cannot encode catalog")?;
        if data.len()>CATALOG_LIMIT{return Err("Catalog exceeds limit".into());}
        Ok(data)
    }
    pub fn open_file(&self,index:usize)->Result<(File,&Entry),String>{
        let e=self.entries.get(index).filter(|e|!e.directory).ok_or("Not an approved catalog file")?;
        let root_meta=fs::symlink_metadata(&self.root).map_err(|_|"Approved root unavailable")?;
        if !check_metadata(&root_meta)||!root_meta.is_dir()||self.root.canonicalize().map_err(|_|"Approved root changed")?!=self.root{return Err("Approved root changed".into());}
        let f=secure_source(&self.root,&e.parts)?;
        if f.metadata().map_err(|_|"Cannot inspect selected file")?.len()!=e.size{return Err("Selected catalog file changed".into());}
        Ok((f,e))
    }
}
pub const MAX_BATCH_FILES:usize=4;
/// Indexes are immutable catalog positions, not remote filesystem paths.
pub fn selected_entries(entries:&[Entry],indexes:&[usize])->Result<Vec<Entry>,String>{
    validate(entries)?;
    if indexes.is_empty()||indexes.len()>MAX_BATCH_FILES{return Err("Select one to four files".into());}
    let mut seen=BTreeSet::new();
    for &i in indexes{
        if !seen.insert(i)||!entries.get(i).is_some_and(|e|!e.directory){return Err("Invalid or duplicate file selection".into());}
    }
    let chosen=entries.iter().enumerate().filter(|(i,e)|seen.contains(i)||e.directory&&indexes.iter().any(|&n|entries[n].parts.starts_with(&e.parts))).map(|(_,e)|e.clone()).collect::<Vec<_>>();
    validate(&chosen)?;Ok(chosen)
}
pub fn unpack_selected(bundle:&[u8],entries:&[Entry],indexes:&[usize],target:&Path,confirmed:bool,canceled:impl FnMut()->bool)->Result<(),String>{
    let expected=selected_entries(entries,indexes)?;
    let (actual,_)=parse(bundle)?;
    if actual!=expected{return Err("Download bundle does not match selected files".into());}
    unpack(bundle,target,confirmed,canceled)
}
pub fn decode_catalog(data:&[u8])->Result<Vec<Entry>,String>{
    if data.is_empty()||data.len()>CATALOG_LIMIT{return Err("Catalog exceeds limit".into());}
    let entries:Vec<Entry>=serde_json::from_slice(data).map_err(|_|"Invalid catalog")?;
    validate(&entries)?;Ok(entries)
}
pub fn pack(root:&Path)->Result<Vec<u8>,String>{
    let meta=fs::symlink_metadata(root).map_err(|_|"Selected directory unavailable")?;
    if !check_metadata(&meta)||!meta.is_dir(){return Err("Choose an ordinary local directory".into());}
    let root=root.canonicalize().map_err(|_|"Cannot resolve selected directory")?;
    let mut entries=Vec::new();let mut total=0;
    walk(&root,&mut Vec::new(),&mut entries,&mut total)?;
    let total=validate(&entries)?;
    let header=serde_json::to_vec(&entries).map_err(|_|"Cannot encode directory manifest")?;
    if header.len()>MAX_HEADER||total+12+header.len()as u64>MAX_FILE{return Err("Directory bundle exceeds 64 MiB".into());}
    let mut data=Vec::with_capacity(total as usize+12+header.len());
    data.extend_from_slice(MAGIC);data.extend_from_slice(&(header.len()as u32).to_be_bytes());data.extend_from_slice(&header);
    for e in entries.iter().filter(|e|!e.directory){
        let f=secure_source(&root,&e.parts)?;
        if f.metadata().map_err(|_|"Cannot inspect selected file")?.len()!=e.size{return Err("Directory file changed during selection".into());}
        let before=data.len();
        f.take(e.size+1).read_to_end(&mut data).map_err(|_|"Cannot read selected file")?;
        if data.len()-before!=e.size as usize{return Err("Directory file changed during transfer".into());}
    }
    Ok(data)
}
fn parse(bundle:&[u8])->Result<(Vec<Entry>,&[u8]),String>{
    if bundle.len()<12||bundle.len()as u64>MAX_FILE||&bundle[..8]!=MAGIC{return Err("Invalid directory bundle".into());}
    let n=u32::from_be_bytes(bundle[8..12].try_into().unwrap())as usize;
    if n==0||n>MAX_HEADER||n>bundle.len()-12{return Err("Invalid directory manifest size".into());}
    let entries:Vec<Entry>=serde_json::from_slice(&bundle[12..12+n]).map_err(|_|"Invalid directory manifest")?;
    let total=validate(&entries)?;
    let body=&bundle[12+n..];
    if body.len()as u64!=total{return Err("Directory content length mismatch".into());}
    Ok((entries,body))
}
pub fn inspect(bundle:&[u8])->Result<(usize,u64),String>{let (e,b)=parse(bundle)?;Ok((e.len(),b.len()as u64))}
pub fn unpack(bundle:&[u8],target:&Path,confirmed:bool,mut canceled:impl FnMut()->bool)->Result<(),String>{
    if !confirmed||canceled(){return Err("Directory receive not authorized".into());}
    // Validate every entry before touching the user's destination.
    let(entries,body)=parse(bundle)?;
    if !target.is_absolute()||target.file_name().is_none()||!target.parent().is_some_and(|p|p.is_dir()){return Err("Choose a new local directory path".into());}
    fs::create_dir(target).map_err(|_|"Destination must be a new directory; overwrite refused")?;
    let root=target.canonicalize().map_err(|_|"Cannot resolve new destination")?;
    if !check_metadata(&fs::symlink_metadata(target).map_err(|_|"Destination changed")?){return Err("Destination changed to an unsafe link".into());}
    let result=(||->Result<(),String>{
        let mut offset=0usize;
        for e in entries{
            if canceled(){return Err("Directory receive canceled".into());}
            let p=relative(&root,&e.parts);
            let parent=p.parent().ok_or("Invalid directory path")?;
            if !parent.canonicalize().map_err(|_|"Directory parent unavailable")?.starts_with(&root){return Err("Destination parent outside selected directory".into());}
            if e.directory{fs::create_dir(&p).map_err(|_|"Cannot create new subdirectory")?;}
            else{
                let mut f=OpenOptions::new().write(true).create_new(true).open(&p).map_err(|_|"Cannot create new destination file")?;
                let end=offset+e.size as usize;
                f.write_all(&body[offset..end]).and_then(|_|f.sync_all()).map_err(|_|"Cannot write destination file")?;
                offset=end;
            }
        }
        Ok(())
    })();
    // Do not recursively delete a directory on failure: the user may have
    // changed it locally. Report that the newly created destination is partial.
    result.map_err(|e|format!("{e}; newly created destination may be incomplete"))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn catalog_only_opens_preapproved_regular_file_indexes(){
        let dir=tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested/a.txt"),b"abc").unwrap();
        let catalog=Catalog::open(dir.path()).unwrap();
        let data=decode_catalog(&catalog.manifest().unwrap()).unwrap();
        assert_eq!(data.len(),2);assert!(data[0].directory);
        assert!(catalog.open_file(0).is_err());assert!(catalog.open_file(2).is_err());
        let(mut file,entry)=catalog.open_file(1).unwrap();let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();
        assert_eq!(entry.parts,vec!["nested","a.txt"]);assert_eq!(bytes,b"abc");
        fs::write(dir.path().join("nested/a.txt"),b"changed").unwrap();
        assert!(catalog.open_file(1).is_err());
    }
    #[test]fn batch_download_preserves_paths_and_rejects_unselected_content(){
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join("root");fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::write(root.join("nested/a.txt"),b"alpha").unwrap();
        fs::write(root.join("nested/b.txt"),b"excluded").unwrap();
        fs::write(root.join("z.txt"),b"omega").unwrap();
        let catalog=Catalog::open(&root).unwrap();
        let entries=decode_catalog(&catalog.manifest().unwrap()).unwrap();
        for invalid in [vec![],vec![0],vec![1,1],vec![256],vec![1,2,3,4,5]]{assert!(catalog.pack_selected(&invalid).is_err());}
        let data=catalog.pack_selected(&[3,1]).unwrap();
        let target=dir.path().join("received");
        assert!(unpack_selected(&data,&entries,&[1,2],&target,true,||false).is_err());assert!(!target.exists());
        assert!(unpack_selected(&data,&entries,&[1,3],&target,false,||false).is_err());assert!(!target.exists());
        unpack_selected(&data,&entries,&[1,3],&target,true,||false).unwrap();
        assert_eq!(fs::read(target.join("nested/a.txt")).unwrap(),b"alpha");
        assert_eq!(fs::read(target.join("z.txt")).unwrap(),b"omega");
        assert!(!target.join("nested/b.txt").exists());
        assert!(unpack_selected(&data,&entries,&[1,3],&target,true,||false).is_err());
    }
    #[test]fn directory_roundtrip_and_no_overwrite(){
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("src");fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("sub")).unwrap();fs::create_dir(source.join("empty")).unwrap();fs::write(source.join("sub/a.bin"),b"fixture").unwrap();
        let data=pack(&source).unwrap();assert_eq!(inspect(&data).unwrap(),(3,7));
        let target=dir.path().join("dst");
        assert!(unpack(&data,&target,false,||false).is_err());assert!(!target.exists());
        unpack(&data,&target,true,||false).unwrap();
        assert_eq!(fs::read(target.join("sub/a.bin")).unwrap(),b"fixture");assert!(target.join("empty").is_dir());
        assert!(unpack(&data,&target,true,||false).is_err());
    }
    #[test]fn rejects_traversal_conflicts_and_special_names(){
        let entry=|name:&str|Entry{parts:vec![name.into()],directory:false,size:0};
        for s in ["..","/absolute","a/b","a\\b","NUL.txt","C:evil","a."]{assert!(validate(&[entry(s)]).is_err());}
        assert!(validate(&[entry("a"),entry("A")]).is_err());
        assert!(validate(&[Entry{parts:vec!["missing".into(),"file".into()],directory:false,size:0}]).is_err());
        #[cfg(unix)]{
            let d=tempfile::tempdir().unwrap();fs::write(d.path().join("a"),b"test").unwrap();
            std::os::unix::fs::symlink(d.path().join("a"),d.path().join("link")).unwrap();assert!(pack(d.path()).is_err());
        }
    }
}
