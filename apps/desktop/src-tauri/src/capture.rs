//! A bounded, user-initiated, local-only preview. Not a video transport.
use serde::Serialize;
use std::{
    io::Cursor,
    sync::{Mutex, TryLockError},
    time::{Duration, Instant},
};
use tauri::{ipc::Response, WebviewWindow};
use xcap::{image::{DynamicImage, ImageFormat}, Monitor};

const MAX_CAPTURE_PIXELS: u64 = 32_000_000;
const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;
static LAST_CAPTURE: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Granted,
    Required,
    SystemManaged,
}

#[cfg(target_os = "macos")]
mod mac_permission {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    pub fn granted() -> bool {
        // SAFETY: These parameterless system calls return a C Boolean.
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    pub fn request() -> bool {
        // SAFETY: Only invoked by the explicit local permission command.
        unsafe { CGRequestScreenCaptureAccess() }
    }
}

fn permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        if mac_permission::granted() { Permission::Granted } else { Permission::Required }
    }
    #[cfg(not(target_os = "macos"))]
    { Permission::SystemManaged }
}

fn require_main(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" { Ok(()) } else { Err("Local main window required".into()) }
}

#[tauri::command]
pub fn capture_permission(window: WebviewWindow) -> Result<Permission, String> {
    require_main(&window)?;
    Ok(permission())
}

#[tauri::command]
pub fn request_capture_permission(window: WebviewWindow) -> Result<Permission, String> {
    require_main(&window)?;
    #[cfg(target_os = "macos")]
    if permission() == Permission::Required {
        let _ = mac_permission::request();
    }
    Ok(permission())
}

#[derive(Serialize,Clone)]
pub struct Display {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) primary: bool,
}

fn require_capture_permission() -> Result<(), String> {
    #[cfg(target_os="linux")]
    remote_ex_desktop_features::platform::require_linux_x11()?;
    if permission() == Permission::Required {
        return Err("Screen recording permission is required. Use the operating system settings.".into());
    }
    Ok(())
}

pub(crate) fn enumerate() -> Result<Vec<Display>, String> {
    require_capture_permission()?;
    Monitor::all().map_err(|e| e.to_string())?.into_iter().map(|m| {
        Ok(Display {
            id: m.id().map_err(|e| e.to_string())?,
            name: m.name().map_err(|e| e.to_string())?,
            width: m.width().map_err(|e| e.to_string())?,
            height: m.height().map_err(|e| e.to_string())?,
            primary: m.is_primary().unwrap_or(false),
        })
    }).collect()
}

#[tauri::command]
pub async fn list_displays(window: WebviewWindow) -> Result<Vec<Display>, String> {
    require_main(&window)?;
    tauri::async_runtime::spawn_blocking(enumerate).await.map_err(|e| e.to_string())?
}

fn check_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_CAPTURE_PIXELS {
        return Err("Display dimensions exceed the local diagnostic limit".into());
    }
    Ok(())
}

fn capture(display_id: u32) -> Result<Vec<u8>, String> {
    require_capture_permission()?;
    // Hold the lock until capture and encoding are complete; no concurrent screenshots.
    let mut last = LAST_CAPTURE.try_lock().map_err(|e| match e {
        TryLockError::WouldBlock => "Another local preview is in progress".to_string(),
        TryLockError::Poisoned(_) => "Capture state unavailable; restart the app".to_string(),
    })?;
    if last.as_ref().is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
        return Err("Please wait before capturing another preview".into());
    }
    *last = Some(Instant::now());
    let monitor = Monitor::all().map_err(|e| e.to_string())?.into_iter()
        .find(|m| m.id().ok() == Some(display_id))
        .ok_or_else(|| "Display disconnected or not found; refresh displays".to_string())?;
    check_dimensions(monitor.width().map_err(|e| e.to_string())?, monitor.height().map_err(|e| e.to_string())?)?;
    let frame = monitor.capture_image().map_err(|e| e.to_string())?;
    check_dimensions(frame.width(), frame.height())?;
    let preview = DynamicImage::ImageRgba8(frame).thumbnail(1280, 720);
    let mut bytes = Cursor::new(Vec::new());
    preview.write_to(&mut bytes, ImageFormat::Png).map_err(|e| e.to_string())?;
    let bytes = bytes.into_inner();
    if bytes.len() > MAX_PREVIEW_BYTES { return Err("Preview exceeds the IPC limit".into()); }
    *last = Some(Instant::now());
    Ok(bytes)
}

#[tauri::command]
pub async fn capture_preview(window: WebviewWindow, display_id: u32) -> Result<Response, String> {
    require_main(&window)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || capture(display_id))
        .await.map_err(|e| e.to_string())??;
    // Raw binary response is a one-shot diagnostic; do not reuse for video streaming.
    Ok(Response::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_reject_empty_and_oversized_frames() {
        assert!(check_dimensions(1920, 1080).is_ok());
        assert!(check_dimensions(0, 1080).is_err());
        assert!(check_dimensions(u32::MAX, u32::MAX).is_err());
    }
}
