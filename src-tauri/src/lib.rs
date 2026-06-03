pub mod commands;
pub mod strategy;
pub mod types;

use commands::{
    delete_api_credentials, delete_investment_thread, delete_schedule, get_api_status,
    get_app_settings, get_investment_threads, get_portfolio_analytics, get_portfolio_snapshot,
    get_purchase_logs, get_safety_events, get_schedules, get_strategy_profiles, get_supported_markets,
    get_thread_validation_results, run_thread_backtest, save_api_credentials,
    save_investment_thread, save_schedule, set_notifications_enabled, test_api_connection,
    toggle_schedule,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            tauri::async_runtime::spawn(commands::run_scheduler(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            save_api_credentials,
            delete_api_credentials,
            get_api_status,
            test_api_connection,
            get_schedules,
            save_schedule,
            delete_schedule,
            toggle_schedule,
            get_supported_markets,
            get_strategy_profiles,
            get_investment_threads,
            save_investment_thread,
            delete_investment_thread,
            run_thread_backtest,
            get_thread_validation_results,
            get_safety_events,
            get_app_settings,
            set_notifications_enabled,
            get_portfolio_analytics,
            get_portfolio_snapshot,
            get_purchase_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running vitdaily");
}
