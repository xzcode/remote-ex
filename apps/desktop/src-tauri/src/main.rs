#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod lan;
mod presence;
mod trust;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(lan::SessionManager::default())
        .manage(presence::PresenceManager::default())
        .invoke_handler(tauri::generate_handler![
            capture::capture_permission,
            capture::request_capture_permission,
            capture::list_displays,
            capture::capture_preview,
            lan::lan_prepare,
            lan::lan_status,
            lan::lan_confirm,
            lan::lan_disconnect,
            presence::presence_start,
            presence::presence_status,
            presence::presence_stop,
            presence::persistent_identity_info,
            trust::trusted_list,
            trust::trusted_add,
            trust::trusted_remove,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.state::<presence::PresenceManager>().request_stop();
                window.state::<lan::SessionManager>().shutdown();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Remote EX")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<presence::PresenceManager>().request_stop();
                app.state::<lan::SessionManager>().shutdown();
            }
        });
}
