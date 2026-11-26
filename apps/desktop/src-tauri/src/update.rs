//! User-initiated release discovery only. Fixed official HTTPS endpoint,
 //! bounded body and no redirects, executable downloads or auto-updates.
use remote_ex_desktop_features::updates::{self,Report};
use serde::Serialize;
use std::{sync::Mutex,time::{Duration,Instant}};
use tauri::{Manager,WebviewWindow};
use tauri_plugin_opener::OpenerExt;
static LAST:Mutex<Option<Instant>>=Mutex::new(None);
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
    updates::select(&bytes,b.tag.as_deref(),b.commit.as_deref(),&b.platform,&b.arch,include_previews)
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
