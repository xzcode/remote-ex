#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            capture::capture_permission,
            capture::request_capture_permission,
            capture::list_displays,
            capture::capture_preview
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Remote EX");
}
