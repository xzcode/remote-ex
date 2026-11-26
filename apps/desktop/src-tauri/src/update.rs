//! User-initiated release discovery only. Fixed official HTTPS endpoint,
 //! bounded body and no redirects, executable downloads or auto-updates.
use remote_ex_desktop_features::updates::{self,Report};
use serde::Serialize;
use std::{sync::Mutex,time::{Duration,Instant}};
use tauri::{Manager,WebviewWindow};
use tauri_plugin_opener::OpenerExt;
static LAST:Mutex<Option<Instant>>=Mutex::new(None);
// Only metadata fetched successfully from the fixed official API can be used
// by the local archive checker; no caller-supplied hashes or paths.
static VERIFIED_METADATA:Mutex<Option<(Instant,updates::Candidate)>>=Mutex::new(None);
static VERIFY_BUSY:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
#[derive(Serialize)]
pub struct BuildInfo{version:String,tag:Option<String>,commit:Option<String>,platform:String,arch:String}
fn local(window:&WebviewWindow)->Result<(),String>{
    if window.label()!="main"{return Err("Local main window required".into());}
    Ok(())
}
fn current()->BuildInfo{
    BuildInfo{version:env!("CARGO_PKG_VERSION").into(),tag:option_env!("REMOTE_EX_BUILD_TAG").filter(|s|updates::version(s).is_some()).map(str::to_owned),commit:option_env!("REMOTE_EX_BUILD_COMMIT").filter(|s|s.len()==40&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).map(str::to_owned),platform:std::env::consts::OS.into(),arch:std::env::consts::ARCH.into()}
}
#[tauri::command]
pub fn build_info(window:WebviewWindow)->Result<BuildInfo,String>{local(&window)?;Ok(current())}
#[tauri::command]
pub async fn check_updates(window:WebviewWindow,include_previews:bool,confirmed:bool)->Result<Report,String>{
    local(&window)?;
    if !confirmed{return Err("Explicit check for updates required".into());}
    if window.state::<crate::lan::SessionManager>().busy()||window.state::<crate::lan::RoutedManagers>().any_busy(){return Err("Finish active sessions before checking releases".into());}
    {
        let mut last=LAST.lock().map_err(|_|"Update check unavailable")?;
        if last.is_some_and(|t|t.elapsed()<Duration::from_secs(10)){return Err("Please wait before checking again".into());}
        *last=Some(Instant::now());
    }
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(4)).timeout(Duration::from_secs(12)).user_agent("Remote-EX-release-check").build().map_err(|_|"Cannot initialize HTTPS release check")?;
    let mut response=client.get(updates::RELEASES_API).header("Accept","application/vnd.github+json").send().await.map_err(|_|"Cannot reach official release service")?;
    if !response.status().is_success(){return Err(format!("Official release service returned HTTP {}",response.status().as_u16()));}
    const MAX:usize=1024*1024;
    if response.content_length().is_some_and(|n|n>MAX as u64){return Err("Release response exceeds limit".into());}
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_|"Release response interrupted")?{
        if bytes.len().saturating_add(chunk.len())>MAX{return Err("Release response exceeds limit".into());}bytes.extend_from_slice(&chunk);
    }
    let b=current();
    let report=updates::select(&bytes,b.tag.as_deref(),b.commit.as_deref(),&b.platform,&b.arch,include_previews)?;
    *VERIFIED_METADATA.lock().map_err(|_|"Release metadata unavailable")?=report.candidate.clone().map(|candidate|(Instant::now(),candidate));
    Ok(report)
}
/// Fixed local buttons only; never accept arbitrary URLs or open downloaded binaries.
#[tauri::command]
pub fn open_help(window:WebviewWindow,target:String,confirmed:bool)->Result<(),String>{
    local(&window)?;if !confirmed{return Err("Confirm opening an official help page".into());}
    let url=match target.as_str(){
        "releases"=>updates::RELEASES_PAGE,
        "gstreamer"=>"https://gstreamer.freedesktop.org/download/",
        "webview2"=>"https://developer.microsoft.com/en-us/microsoft-edge/webview2/",
        "install"=>"https://github.com/xzcode/remote-ex/blob/master/docs/INSTALL.md",
        _=>return Err("Unknown official help page".into()),
    };
    window.app_handle().opener().open_url(url,None::<&str>).map_err(|_|"Cannot open default browser".into())
}

#[tauri::command]
pub fn open_release(window:WebviewWindow,tag:String,confirmed:bool)->Result<(),String>{
    local(&window)?;if !confirmed{return Err("Confirm opening an official release page".into());}
    let url=updates::release_page(&tag)?;
    window.app_handle().opener().open_url(url,None::<&str>).map_err(|_|"Cannot open official release page".into())
}

#[derive(Serialize)]
pub struct ArchiveCheck{asset:String,matches:bool}
struct BusyGuard;
impl Drop for BusyGuard{fn drop(&mut self){VERIFY_BUSY.store(false,std::sync::atomic::Ordering::Release);}}
#[tauri::command]
pub async fn verify_release_archive(window:WebviewWindow,tag:String,asset_name:String,confirmed:bool)->Result<Option<ArchiveCheck>,String>{
    local(&window)?;if !confirmed{return Err("Explicit local archive verification required".into());}
    if window.state::<crate::lan::SessionManager>().busy()||window.state::<crate::lan::RoutedManagers>().any_busy()||window.state::<crate::presence::PresenceManager>().busy(){return Err("Finish active sessions before verifying an archive".into());}
    let asset={let saved=VERIFIED_METADATA.lock().map_err(|_|"Release metadata unavailable")?;let (at,candidate)=saved.as_ref().ok_or("Check official release metadata first")?;
        if at.elapsed()>Duration::from_secs(900)||candidate.tag!=tag{return Err("Refresh official release metadata first".into());}
        candidate.assets.iter().find(|a|a.name==asset_name&&a.sha256.is_some()).cloned().ok_or("No matching official release digest")?};
    if VERIFY_BUSY.compare_exchange(false,true,std::sync::atomic::Ordering::AcqRel,std::sync::atomic::Ordering::Acquire).is_err(){return Err("Archive verification already running".into());}
    let _guard=BusyGuard;
    let Some(file)=rfd::AsyncFileDialog::new().set_title("Remote EX — verify downloaded release archive").pick_file().await else{return Ok(None);};
    let path=file.path().to_path_buf();
    let result=tauri::async_runtime::spawn_blocking(move||updates::check_archive(&path,&asset).map(|matches|ArchiveCheck{asset:asset.name,matches})).await.map_err(|_|"Archive verification interrupted")??;
    Ok(Some(result))
}
