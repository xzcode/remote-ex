//! Local lifecycle controls. Login launch opens the ordinary visible UI
//! offline. No boot service, elevation, hidden listener or automatic approval.
use serde::Serialize;
use std::{sync::{Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use tauri::{AppHandle,Manager,WebviewWindow,menu::{Menu,MenuItem},tray::TrayIconBuilder};
use tauri_plugin_autostart::ManagerExt;
static TRAY_READY:AtomicBool=AtomicBool::new(false);
static LAST_WAKE:Mutex<Option<Instant>>=Mutex::new(None);
fn local(window:&WebviewWindow)->Result<(),String>{
    if window.label()!="main"{return Err("Local main window required".into());}Ok(())
}
fn disconnect(app:&AppHandle){
    app.state::<crate::presence::PresenceManager>().request_stop();
    app.state::<crate::lan::SessionManager>().request_stop();
    app.state::<crate::lan::RoutedManagers>().stop_all();
}
pub(crate) fn setup_tray(app:&AppHandle){
    let result=(||->tauri::Result<()>{
        let show=MenuItem::with_id(app,"rex_show","Show Remote EX / 显示",true,None::<&str>)?;
        let stop=MenuItem::with_id(app,"rex_stop","Disconnect all / 断开全部",true,None::<&str>)?;
        let quit=MenuItem::with_id(app,"rex_quit","Quit / 退出",true,None::<&str>)?;
        let menu=Menu::with_items(app,&[&show,&stop,&quit])?;
        let icon=match app.default_window_icon(){Some(icon)=>icon.clone(),None=>{
            // Small original, simple visible monitor icon, no external asset.
            let mut pixels=vec![0u8;32*32*4];
            for y in 5..25{for x in 3..29{if y<=7||y>=22||x<=5||x>=26{let i=(y*32+x)*4;pixels[i..i+4].copy_from_slice(&[65,128,210,255]);}}}
            for y in 25..29{for x in 12..20{let i=(y*32+x)*4;pixels[i..i+4].copy_from_slice(&[65,128,210,255]);}}
            tauri::image::Image::new_owned(pixels,32,32)
        }};
        TrayIconBuilder::with_id("remote-ex-tray").icon(icon).menu(&menu).tooltip("Remote EX — open for session status").on_menu_event(|app,event|{
            match event.id.as_ref(){
                "rex_show"=>{if let Some(w)=app.get_webview_window("main"){let _=w.unminimize();let _=w.show();let _=w.set_focus();}},
                "rex_stop"=>disconnect(app),
                "rex_quit"=>{disconnect(app);app.exit(0);},
                _=>{},
            }
        }).build(app)?;
        Ok(())
    })();
    TRAY_READY.store(result.is_ok(),Ordering::Release);
}
#[derive(Serialize)]
pub struct Capabilities{
    os:String,desktop_session:String,view_candidate:bool,input_candidate:bool,armed_candidate:bool,sidecar_present:bool,
    login_launch:bool,tray_ready:bool,wayland_portal:bool,remote_power:bool,virtual_display:bool,privacy_display:bool,
}
#[tauri::command]
pub fn platform_capabilities(window:WebviewWindow)->Result<Capabilities,String>{
    local(&window)?;
    let os=std::env::consts::OS;
    let desktop_session=if cfg!(target_os="linux"){match remote_ex_desktop_features::platform::linux_session(){
        remote_ex_desktop_features::platform::LinuxSession::X11=>"x11",
        remote_ex_desktop_features::platform::LinuxSession::Wayland=>"wayland",
        _=>"unavailable",
    }}else if cfg!(any(target_os="windows",target_os="macos")){"native"}else{"unsupported"};
    let desktop=cfg!(any(target_os="windows",target_os="macos"));
    let name=if cfg!(target_os="linux"){"routed_x11_view"}else if cfg!(windows){"routed_control.exe"}else{"routed_control"};
    let sidecar_present=std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join(name))).is_some_and(|p|p.is_file());
    Ok(Capabilities{os:os.into(),desktop_session:desktop_session.into(),view_candidate:desktop||desktop_session=="x11",
        input_candidate:desktop,armed_candidate:desktop,sidecar_present,login_launch:desktop||cfg!(target_os="linux"),tray_ready:TRAY_READY.load(Ordering::Acquire),
        wayland_portal:false,remote_power:false,virtual_display:false,privacy_display:false})
}
#[tauri::command]
pub fn startup_status(window:WebviewWindow)->Result<bool,String>{
    local(&window)?;window.app_handle().autolaunch().is_enabled().map_err(|e|e.to_string())
}
#[tauri::command]
pub fn startup_change(window:WebviewWindow,enabled:bool,confirmed:bool)->Result<bool,String>{
    local(&window)?;if !confirmed{return Err("Confirm the local login-launch change".into());}
    let manager=window.app_handle().autolaunch();
    if enabled{manager.enable().map_err(|e|e.to_string())?;}else{manager.disable().map_err(|e|e.to_string())?;}
    let actual=manager.is_enabled().map_err(|e|e.to_string())?;
    if actual!=enabled{return Err("Login-launch status differs from requested state".into());}Ok(actual)
}
#[tauri::command]
pub fn stop_all(window:WebviewWindow)->Result<(),String>{local(&window)?;disconnect(window.app_handle());Ok(())}
/// Explicit user action sends one standard magic packet on the local
/// IPv4 broadcast network. Never invokes a shell or sends remote shutdown.
#[tauri::command]
pub fn wake_lan(window:WebviewWindow,mac:String,confirmed:bool)->Result<(),String>{
    local(&window)?;
    if !confirmed{return Err("Confirm wake for your own or authorized device".into());}
    let packet=remote_ex_desktop_features::platform::wake_packet(&mac)?;
    let mut last=LAST_WAKE.lock().map_err(|_|"Wake state unavailable")?;
    if last.is_some_and(|v|v.elapsed()<Duration::from_secs(3)){return Err("Wait before another wake packet".into());}
    let socket=std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED,0)).map_err(|_|"Cannot open local UDP socket")?;
    socket.set_broadcast(true).map_err(|_|"Cannot enable local broadcast")?;
    socket.set_write_timeout(Some(Duration::from_secs(1))).map_err(|_|"Cannot set UDP timeout")?;
    *last=Some(Instant::now());
    if socket.send_to(&packet,(std::net::Ipv4Addr::BROADCAST,9)).map_err(|_|"Cannot send local wake packet")?!=packet.len(){return Err("Incomplete wake packet".into());}
    Ok(())
}
