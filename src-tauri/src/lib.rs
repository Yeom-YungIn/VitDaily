pub mod commands;
pub mod strategy;
pub mod types;

use commands::{
    activate_thread_live, complete_thread, delete_api_credentials, delete_investment_thread,
    delete_schedule, get_api_status, get_app_settings, get_investment_threads,
    get_legacy_schedule_live_policy_statuses, get_live_activation_confirmation_phrase,
    get_live_order_chance_status, get_portfolio_analytics, get_portfolio_snapshot,
    get_purchase_logs, get_safety_events, get_schedules, get_strategy_profiles,
    get_supported_markets, get_thread_validation_results, pause_thread,
    preview_thread_live_order_payload, run_all_thread_auto_loop_ticks, run_thread_auto_loop_tick,
    run_thread_backtest, run_thread_paper_execution, save_api_credentials, save_investment_thread,
    save_schedule, set_live_trading_settings, set_notifications_enabled, start_thread_live,
    stop_thread, submit_thread_live_market_buy, submit_thread_live_market_sell,
    test_api_connection, toggle_schedule,
};
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("vitdaily".to_string()),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
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
            get_live_order_chance_status,
            get_schedules,
            get_legacy_schedule_live_policy_statuses,
            save_schedule,
            delete_schedule,
            toggle_schedule,
            get_supported_markets,
            get_strategy_profiles,
            get_investment_threads,
            save_investment_thread,
            delete_investment_thread,
            run_thread_backtest,
            run_thread_paper_execution,
            get_live_activation_confirmation_phrase,
            activate_thread_live,
            start_thread_live,
            pause_thread,
            stop_thread,
            complete_thread,
            run_thread_auto_loop_tick,
            run_all_thread_auto_loop_ticks,
            preview_thread_live_order_payload,
            submit_thread_live_market_buy,
            submit_thread_live_market_sell,
            get_thread_validation_results,
            get_safety_events,
            get_app_settings,
            set_notifications_enabled,
            set_live_trading_settings,
            get_portfolio_analytics,
            get_portfolio_snapshot,
            get_purchase_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running vitdaily");
}
