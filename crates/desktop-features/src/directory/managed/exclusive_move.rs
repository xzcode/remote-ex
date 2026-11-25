//! Native same-volume no-replace rename. No clobbering, copying, deletion,
//! delayed moves, or privileged fallback.
use cap_std::fs::Dir;
use std::path::Path;

#[cfg(any(target_os="linux",target_os="macos"))]
pub(super) fn move_entry(from:&Dir,name:&str,to:&Dir,dest:&str,_absolute_from:&Path,_absolute_to:&Path)->Result<(),String>{
    use std::{ffi::{CString,c_char,c_int,c_uint},os::fd::AsRawFd};
    #[cfg(target_os="linux")]
    unsafe extern "C"{fn renameat2(oldfd:c_int,old:*const c_char,newfd:c_int,new:*const c_char,flags:c_uint)->c_int;}
    #[cfg(target_os="macos")]
    unsafe extern "C"{fn renameatx_np(oldfd:c_int,old:*const c_char,newfd:c_int,new:*const c_char,flags:c_uint)->c_int;}
    let old=CString::new(name).map_err(|_|"Invalid source name")?;
    let new=CString::new(dest).map_err(|_|"Invalid destination name")?;
    // SAFETY: live directory descriptors and valid single-component C strings;
    // flags are RENAME_NOREPLACE (Linux) / RENAME_EXCL (Darwin).
    #[cfg(target_os="linux")]
    let result=unsafe{renameat2(from.as_raw_fd(),old.as_ptr(),to.as_raw_fd(),new.as_ptr(),1)};
    #[cfg(target_os="macos")]
    let result=unsafe{renameatx_np(from.as_raw_fd(),old.as_ptr(),to.as_raw_fd(),new.as_ptr(),4)};
    if result!=0{return Err(format!("Exclusive move refused: {:?}",std::io::Error::last_os_error().kind()));}Ok(())
}
#[cfg(target_os="windows")]
pub(super) fn move_entry(_from:&Dir,_name:&str,_to:&Dir,_dest:&str,absolute_from:&Path,absolute_to:&Path)->Result<(),String>{
    use std::os::windows::ffi::OsStrExt;
    #[link(name="kernel32")]
    unsafe extern "system"{fn MoveFileExW(existing:*const u16,new:*const u16,flags:u32)->i32;}
    let wide=|p:&Path|->Result<Vec<u16>,String>{
        if !p.is_absolute(){return Err("Invalid local move path".into());}
        let mut v:Vec<_>=p.as_os_str().encode_wide().collect();
        if v.contains(&0){return Err("Invalid local path".into());}v.push(0);Ok(v)
    };
    let old=wide(absolute_from)?;let new=wide(absolute_to)?;
    // SAFETY: NUL-terminated paths derived from the locally checked root
    // and reviewed relative components. No replace or cross-volume copy.
    // Windows absolute path race against malicious local mutation is outside
    // this prototype's verified boundary.
    if unsafe{MoveFileExW(old.as_ptr(),new.as_ptr(),0)}==0{return Err(format!("Exclusive move refused: {:?}",std::io::Error::last_os_error().kind()));}Ok(())
}
#[cfg(not(any(target_os="linux",target_os="macos",target_os="windows")))]
pub(super) fn move_entry(_from:&Dir,_name:&str,_to:&Dir,_dest:&str,_absolute_from:&Path,_absolute_to:&Path)->Result<(),String>{
    Err("Exclusive move unavailable on this platform".into())
}
#[cfg(all(test,any(target_os="linux",target_os="macos",target_os="windows")))]
mod tests{
    use super::*;
    #[test]fn refuses_existing_empty_destination_directory(){
        let d=tempfile::tempdir().unwrap();let root=d.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("source")).unwrap();std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("source/file"),b"keep").unwrap();
        let dir=Dir::open_ambient_dir(&root,cap_std::ambient_authority()).unwrap();
        assert!(move_entry(&dir,"source",&dir,"target",&root.join("source"),&root.join("target")).is_err());
        assert_eq!(std::fs::read(root.join("source/file")).unwrap(),b"keep");
        assert_eq!(std::fs::read_dir(root.join("target")).unwrap().count(),0);
    }
}
