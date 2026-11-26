//! Fixed local media preflight. Never starts a listener, network connection,
//! OS capture, input, credentials, or arbitrary user-selected executable.
use remote_ex_desktop_features::{bundle,runtime::Probe};
use serde::Serialize;
use std::{io::Read,path::{Path,PathBuf},process::{Command,Stdio},sync::{Mutex,mpsc},time::{Duration,Instant}};
use tauri::{Manager,WebviewWindow};
static BUSY:Mutex<()>=Mutex::new(());
const MAX_OUTPUT:usize=8192;
const LIMIT:Duration=Duration::from_secs(12);
#[derive(Serialize)]
struct Component{name:String,present:bool,integrity:String}
#[derive(Serialize)]
pub struct Report{
    schema:u32,app_version:String,platform:String,arch:String,bundle_commit:Option<String>,
    manifest:String,components:Vec<Component>,probe:Option<Probe>,probe_status:String,
    core_ready:bool,audio_ready:bool,portal_ready:bool,
}
fn app_directory()->Result<PathBuf,String>{std::env::current_exe().map_err(|_|"Cannot locate desktop executable")?.parent().map(Path::to_path_buf).ok_or_else(||"Cannot locate app directory".into())}
fn installed_manifest(dir:&Path,platform:&str,arch:&str)->Result<Option<bundle::Manifest>,String>{
    let m=bundle::load(dir,platform,arch)?;
    check_manifest_commit(option_env!("REMOTE_EX_BUILD_COMMIT"),m.as_ref().map(|x|x.commit.as_str()))?;
    Ok(m)
}
fn check_manifest_commit(expected:Option<&str>,actual:Option<&str>)->Result<(),String>{
    if expected.is_some_and(|e|Some(e)!=actual){return Err("Installed component manifest is missing or belongs to another build. Reinstall the complete matching package.".into());}
    Ok(())
}
pub(crate) fn verify_component(dir:&Path,name:&str)->Result<(),String>{
    let platform=std::env::consts::OS;let arch=std::env::consts::ARCH;
    if !bundle::names(platform)?.contains(&name)||!std::fs::symlink_metadata(dir.join(name)).is_ok_and(|m|m.file_type().is_file()){
        return Err("Missing or invalid installed media component".into());
    }
    if let Some(m)=installed_manifest(dir,platform,arch)?{m.verify_name(dir,name)?;}Ok(())
}
#[cfg(windows)]
fn installed_gstreamer_bin()->Option<PathBuf>{
    let mut roots=Vec::new();
    // These are documented local installation hints, never peer values.
    if let Some(root)=std::env::var_os("GSTREAMER_1_0_ROOT_MSVC_X86_64"){roots.push(PathBuf::from(root));}
    if let Some(root)=std::env::var_os("ProgramFiles"){
        roots.push(PathBuf::from(&root).join("gstreamer").join("1.0").join("msvc_x86_64"));
        roots.push(PathBuf::from(root).join("gstreamer"));
    }
    roots.push(PathBuf::from(r"C:\gstreamer\1.0\msvc_x86_64"));
    roots.into_iter().filter(|p|p.is_absolute()).find_map(|root|{
        let bin=root.join("bin");
        if !root.join("lib").join("gstreamer-1.0").is_dir()||!bin.join("gst-inspect-1.0.exe").is_file()||!(bin.join("gstreamer-1.0-0.dll").is_file()||bin.join("libgstreamer-1.0-0.dll").is_file()){return None;}
        bin.canonicalize().ok()
    })
}
pub(crate) fn media_command(path:&Path)->Command{
    #[allow(unused_mut)]
    let mut command=Command::new(path);
    command.env("REMOTE_EX_VIEWER_LOCALE",crate::viewer_locale::current().code());
    #[cfg(windows)]
    {
        // Scope to this child. Do not modify global PATH, use a shell, or
        // install DLLs. Honor an existing valid user/official runtime path.
        if let Some(bin)=installed_gstreamer_bin(){
            let inherited=std::env::var_os("PATH").unwrap_or_default();
            let paths=std::iter::once(bin).chain(std::env::split_paths(&inherited));
            if let Ok(value)=std::env::join_paths(paths){command.env("PATH",value);}
        }
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
fn invoke_probe(path:&Path)->Result<Probe,&'static str>{
    let mut child=media_command(path).arg("--runtime-check").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_|"launch_failed")?;
    let stdout=match child.stdout.take(){Some(s)=>s,None=>{let _=child.kill();let _=child.wait();return Err("invalid_output");}};
    let(tx,rx)=mpsc::sync_channel(1);
    std::thread::spawn(move||{
        let mut bytes=Vec::new();
        let result=stdout.take((MAX_OUTPUT+1)as u64).read_to_end(&mut bytes).map(|_|bytes);
        let _=tx.send(result);
    });
    let until=Instant::now()+LIMIT;
    loop{
        match child.try_wait(){
            Ok(Some(status))=>{
                if !status.success(){return Err("launch_failed");}
                let data=rx.recv_timeout(until.saturating_duration_since(Instant::now())).map_err(|_|"timeout")?.map_err(|_|"invalid_output")?;
                return parse_report(&data);
            },
            Ok(None) if Instant::now()<until=>std::thread::sleep(Duration::from_millis(30)),
            _=>{let _=child.kill();let _=child.wait();return Err("timeout");}
        }
    }
}
fn parse_report(data:&[u8])->Result<Probe,&'static str>{
    if data.len()>MAX_OUTPUT{return Err("invalid_output");}
    let p:Probe=serde_json::from_slice(data).map_err(|_|"invalid_output")?;
    p.validate().map_err(|_|"invalid_output")?;Ok(p)
}
fn inspect()->Result<Report,String>{
    let _guard=BUSY.try_lock().map_err(|_|"A local runtime check is already running")?;
    let dir=app_directory()?;
    let platform=std::env::consts::OS;let arch=std::env::consts::ARCH;
    let names=bundle::names(platform)?;
    let (manifest,invalid)=match installed_manifest(&dir,platform,arch){Ok(m)=>(m,false),Err(_)=>(None,true)};
    let mut components=Vec::new();
    for name in names{
        let present=std::fs::symlink_metadata(dir.join(name)).is_ok_and(|m|m.file_type().is_file());
        let integrity=if invalid{"invalid"}else if !present{"missing"}else if let Some(m)=&manifest{
            if m.verify_name(&dir,name).is_ok(){"verified"}else{"mismatch"}
        }else{"unverified"};
        components.push(Component{name:(*name).to_owned(),present,integrity:integrity.into()});
    }
    let probe_name=bundle::probe_name(platform)?;
    let can_launch=components.iter().any(|c|c.name==probe_name&&c.present&&matches!(c.integrity.as_str(),"verified"|"unverified"))&&!invalid;
    let (probe,probe_status)=if can_launch{match invoke_probe(&dir.join(probe_name)){Ok(p)=>(Some(p),"complete"),Err(e)=>(None,e)}}else{(None,"component_unavailable")};
    let manifest_status=if invalid{"invalid"}else if manifest.is_none(){"absent"}else if components.iter().all(|c|c.integrity=="verified"){"verified"}else{"mismatch"};
    let complete=components.iter().all(|c|c.present&&matches!(c.integrity.as_str(),"verified"|"unverified"));
    // These describe the package/runtime, not OS permission or remote access.
    let core_ready=complete&&probe.as_ref().is_some_and(Probe::core_ready);
    let audio_ready=core_ready&&probe.as_ref().is_some_and(Probe::audio_ready)&&platform!="linux";
    let portal_ready=core_ready&&platform=="linux"&&probe.as_ref().is_some_and(Probe::portal_ready);
    Ok(Report{schema:1,app_version:env!("CARGO_PKG_VERSION").into(),platform:platform.into(),arch:arch.into(),bundle_commit:manifest.map(|m|m.commit),manifest:manifest_status.into(),components,probe,probe_status:probe_status.into(),core_ready,audio_ready,portal_ready})
}
#[tauri::command]
pub async fn runtime_check(window:WebviewWindow)->Result<Report,String>{
    if window.label()!="main"{return Err("Local main window required".into());}
    if window.state::<crate::lan::SessionManager>().busy()||window.state::<crate::lan::RoutedManagers>().any_busy(){return Err("Finish active sessions before checking the local runtime".into());}
    tauri::async_runtime::spawn_blocking(inspect).await.map_err(|_|"Local runtime check worker failed")?
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn installed_revision_never_accepts_other_or_missing_manifest(){assert!(check_manifest_commit(Some("a"),Some("a")).is_ok());assert!(check_manifest_commit(Some("a"),Some("b")).is_err());assert!(check_manifest_commit(Some("a"),None).is_err());assert!(check_manifest_commit(None,None).is_ok());}
    #[test]fn rejects_unbounded_and_unexpected_probe_output(){
        assert!(parse_report(&vec![b' ';MAX_OUTPUT+1]).is_err());
        assert!(parse_report(br#"{"schema":1,"initialized":true,"version":[1,26,11,0],"missing":["/secret"],"synthetic_video":true}"#).is_err());
        assert!(parse_report(br#"{"schema":1,"initialized":true,"version":[1,26,11,0],"missing":[],"synthetic_video":true}"#).is_ok());
    }
}
