//! Local language preference propagated only to newly spawned native viewers.
use std::sync::atomic::{AtomicU8,Ordering};
use remote_ex_desktop_features::viewer_ui::Locale;
use tauri::WebviewWindow;
static LOCALE:AtomicU8=AtomicU8::new(0);
pub(crate) fn current()->Locale{Locale::from_index(LOCALE.load(Ordering::Acquire))}
#[tauri::command]
pub fn set_viewer_locale(window:WebviewWindow,locale:String)->Result<(),String>{
    if window.label()!="main"{return Err("Local main window required".into());}
    let locale=Locale::parse(&locale).ok_or("Unsupported local language")?;
    LOCALE.store(locale as u8,Ordering::Release);Ok(())
}
