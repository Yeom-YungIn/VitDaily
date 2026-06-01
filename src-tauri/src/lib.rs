pub mod commands;
pub mod types;

use commands::{
    delete_api_credentials, get_api_status, save_api_credentials, test_api_connection,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            save_api_credentials,
            delete_api_credentials,
            get_api_status,
            test_api_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running vitdaily");
}
