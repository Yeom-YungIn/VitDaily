pub mod commands;
pub mod types;

use commands::{
    delete_api_credentials, delete_schedule, get_api_status, get_schedules,
    save_api_credentials, save_schedule, test_api_connection, toggle_schedule,
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
            get_schedules,
            save_schedule,
            delete_schedule,
            toggle_schedule,
        ])
        .run(tauri::generate_context!())
        .expect("error while running vitdaily");
}
