mod fetch;
mod poster;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(Arc::new(poster::PosterState::default()))
        .invoke_handler(tauri::generate_handler![
            fetch::fetch_transfer_draft,
            poster::open_login_window,
            poster::start_posting,
            poster::continue_posting,
            poster::stop_posting
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
