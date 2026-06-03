use crate::types::{
    ApiStatus, AppSettings, AuditCategory, ExecutionMode, InvestmentThread, PortfolioAllocation,
    PortfolioAnalytics, PortfolioPointSource, PortfolioSnapshot, PortfolioSummary,
    PortfolioTimePoint, PurchaseLog, PurchaseLogAction, PurchaseLogSource, PurchaseStatus,
    SafetyEvent, SafetyEventType, Schedule, StorageEnvelope, StrategyProfile, StrategyProfileInfo,
    SupportedMarket, ThreadAnalytics, ThreadStatus, ThreadValidationResult, ValidationStatus,
};
use chrono::{Local, NaiveTime, Timelike, Utc};
use keyring::Entry;
use std::collections::{BTreeMap, HashMap};
use tauri::{command, AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::time::{interval, Duration};

const KEYRING_SERVICE: &str = "vitdaily";
const KEYRING_ACCESS_KEY: &str = "upbit_access_key";
const KEYRING_SECRET_KEY: &str = "upbit_secret_key";

// --- API Credentials ---

#[command]
pub async fn save_api_credentials(access_key: String, secret_key: String) -> Result<(), String> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)
        .map_err(|e| e.to_string())?
        .set_password(&access_key)
        .map_err(|e| e.to_string())?;

    Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)
        .map_err(|e| e.to_string())?
        .set_password(&secret_key)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[command]
pub async fn delete_api_credentials() -> Result<(), String> {
    let _ = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY).and_then(|e| e.delete_credential());
    let _ = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY).and_then(|e| e.delete_credential());
    Ok(())
}

#[command]
pub async fn get_api_status() -> ApiStatus {
    let has_credentials = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some();

    ApiStatus {
        connected: false,
        has_credentials,
        error: None,
    }
}

#[command]
pub async fn test_api_connection() -> Result<ApiStatus, String> {
    let access_key = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|_| "API 키가 저장되어 있지 않습니다".to_string())?;

    let secret_key = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|_| "API 키가 저장되어 있지 않습니다".to_string())?;

    match upbit_check_balance(&access_key, &secret_key).await {
        Ok(_) => Ok(ApiStatus {
            connected: true,
            has_credentials: true,
            error: None,
        }),
        Err(e) => Ok(ApiStatus {
            connected: false,
            has_credentials: true,
            error: Some(e),
        }),
    }
}

#[command]
pub async fn get_portfolio_snapshot() -> Result<PortfolioSnapshot, String> {
    let (access_key, secret_key) = get_credentials().map_err(|_| {
        "업비트 API 키가 저장되어 있지 않습니다. 설정에서 API 키를 먼저 저장해주세요".to_string()
    })?;

    let accounts = upbit_get_accounts(&access_key, &secret_key).await?;
    let btc_balance = accounts
        .iter()
        .find(|account| account.currency == "BTC")
        .map(|account| account.balance + account.locked)
        .unwrap_or(0.0);
    let btc_locked = accounts
        .iter()
        .find(|account| account.currency == "BTC")
        .map(|account| account.locked)
        .unwrap_or(0.0);
    let btc_price_krw = upbit_get_btc_price_krw().await?;

    Ok(PortfolioSnapshot {
        btc_balance: btc_balance - btc_locked,
        btc_locked,
        btc_total: btc_balance,
        btc_price_krw,
        btc_value_krw: btc_balance * btc_price_krw,
    })
}

#[command]
pub async fn get_portfolio_analytics() -> Result<PortfolioAnalytics, String> {
    let logs = load_logs().map_err(|e| e.to_string())?;
    let threads = load_investment_threads().map_err(|e| e.to_string())?;
    let validation_results = load_thread_validation_results().map_err(|e| e.to_string())?;
    let safety_events = load_safety_events().map_err(|e| e.to_string())?;
    let analytics = build_portfolio_analytics(&logs, &threads, &validation_results, &safety_events);

    persist_portfolio_snapshots(&analytics.time_series).map_err(|e| e.to_string())?;
    Ok(analytics)
}

// --- Schedules ---

#[command]
pub async fn get_schedules() -> Result<Vec<Schedule>, String> {
    load_schedules().map_err(|e| e.to_string())
}

#[command]
pub async fn save_schedule(schedule: Schedule) -> Result<Vec<Schedule>, String> {
    validate_schedule(&schedule)?;

    let mut schedules = load_schedules().map_err(|e| e.to_string())?;
    match schedules.iter().position(|s| s.id == schedule.id) {
        Some(i) => schedules[i] = schedule,
        None => schedules.push(schedule),
    }
    persist_schedules(&schedules).map_err(|e| e.to_string())?;
    Ok(schedules)
}

#[command]
pub async fn delete_schedule(id: String) -> Result<Vec<Schedule>, String> {
    let uuid = id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 ID".to_string())?;
    let mut schedules = load_schedules().map_err(|e| e.to_string())?;
    schedules.retain(|s| s.id != uuid);
    persist_schedules(&schedules).map_err(|e| e.to_string())?;
    Ok(schedules)
}

#[command]
pub async fn toggle_schedule(id: String) -> Result<Vec<Schedule>, String> {
    let uuid = id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 ID".to_string())?;
    let mut schedules = load_schedules().map_err(|e| e.to_string())?;
    if let Some(schedule) = schedules.iter_mut().find(|s| s.id == uuid) {
        schedule.enabled = !schedule.enabled;
        schedule.updated_at = Utc::now();
    }
    persist_schedules(&schedules).map_err(|e| e.to_string())?;
    Ok(schedules)
}

// --- Investment Threads ---

#[command]
pub async fn get_supported_markets() -> Vec<SupportedMarket> {
    SupportedMarket::all()
}

#[command]
pub async fn get_strategy_profiles() -> Vec<StrategyProfileInfo> {
    vec![
        StrategyProfileInfo {
            profile: StrategyProfile::Stable,
            title: "안정적".to_string(),
            risk_label: "낮은 빈도 · 손실 제한 우선".to_string(),
            trade_frequency: "0–2회/일".to_string(),
            indicators: vec!["MACD 12/26/9".to_string(), "Bollinger 20/2".to_string(), "ATR 14".to_string()],
            summary: "DCA에 가까운 저빈도 전략입니다. 강한 약세와 높은 변동성을 회피하고, 실거래 전 백테스트 통과가 필요합니다.".to_string(),
        },
        StrategyProfileInfo {
            profile: StrategyProfile::Conservative,
            title: "보수적".to_string(),
            risk_label: "균형형 · 추세와 평균회귀 조합".to_string(),
            trade_frequency: "0–5회/일".to_string(),
            indicators: vec!["MACD 12/26/9".to_string(), "Bollinger 20/2".to_string(), "ATR 14".to_string()],
            summary: "추세 확인과 과매수/과매도 신호를 함께 사용합니다. 첫 자동매매 구현 후보입니다.".to_string(),
        },
        StrategyProfileInfo {
            profile: StrategyProfile::Aggressive,
            title: "공격적".to_string(),
            risk_label: "높은 빈도 · 모멘텀/돌파".to_string(),
            trade_frequency: "0–10회/일".to_string(),
            indicators: vec!["MACD momentum".to_string(), "Bollinger breakout".to_string(), "ATR trailing stop".to_string()],
            summary: "더 빠른 가격 변화와 돌파 신호를 활용합니다. 기본값으로 권장하지 않으며 강한 경고와 검증이 필요합니다.".to_string(),
        },
    ]
}

#[command]
pub async fn get_investment_threads() -> Result<Vec<InvestmentThread>, String> {
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(threads)
}

#[command]
pub async fn save_investment_thread(
    mut thread: InvestmentThread,
) -> Result<Vec<InvestmentThread>, String> {
    validate_investment_thread(&thread)?;

    let now = Utc::now();
    thread.updated_at = now;

    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    match threads.iter().position(|existing| existing.id == thread.id) {
        Some(index) => {
            threads[index] = merge_investment_thread(Some(&threads[index]), thread, now);
        }
        None => {
            threads.push(merge_investment_thread(None, thread, now));
        }
    }

    persist_investment_threads(&threads).map_err(|e| e.to_string())?;
    threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(threads)
}

#[command]
pub async fn delete_investment_thread(id: String) -> Result<Vec<InvestmentThread>, String> {
    let uuid = id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let before = threads.len();
    threads.retain(|thread| thread.id != uuid);
    if threads.len() == before {
        return Err("삭제할 스레드를 찾을 수 없습니다".to_string());
    }
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;
    threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(threads)
}

#[command]
pub async fn run_thread_backtest(thread_id: String) -> Result<ThreadValidationResult, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == uuid)
        .cloned()
        .ok_or_else(|| "백테스트할 스레드를 찾을 수 없습니다".to_string())?;

    let candles = crate::strategy::fetch_recent_year_hourly_candles(&thread.market).await?;
    let result = crate::strategy::run_backtest_for_thread(&thread, &candles)?;

    if let Some(saved_thread) = threads.iter_mut().find(|thread| thread.id == uuid) {
        saved_thread.validation_status = result.status.clone();
        saved_thread.updated_at = Utc::now();
    }
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;

    let mut results = load_thread_validation_results().map_err(|e| e.to_string())?;
    results.retain(|existing| existing.thread_id != uuid);
    results.push(result.clone());
    persist_thread_validation_results(&results).map_err(|e| e.to_string())?;

    let _ = record_safety_event(
        Some(uuid),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::Validation,
            source: Some("strategy_backtest".to_string()),
            related_schedule_id: None,
            reason: Some("백테스트는 주문 전송 없이 검증 결과만 저장합니다".to_string()),
        },
        format!(
            "{} {} 백테스트 완료 · 상태 {:?} · 주문 전송 없음",
            thread.market.as_upbit_market(),
            strategy_profile_label(&thread.strategy_profile),
            result.status
        ),
    );

    Ok(result)
}

#[command]
pub async fn get_thread_validation_results() -> Result<Vec<ThreadValidationResult>, String> {
    let mut results = load_thread_validation_results().map_err(|e| e.to_string())?;
    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(results)
}

#[command]
pub async fn get_safety_events() -> Result<Vec<SafetyEvent>, String> {
    let mut events = load_safety_events().map_err(|e| e.to_string())?;
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(events)
}

// --- Settings ---

#[command]
pub async fn get_app_settings() -> Result<AppSettings, String> {
    load_settings().map_err(|e| e.to_string())
}

#[command]
pub async fn set_notifications_enabled(
    enabled: bool,
    permission_requested: Option<bool>,
) -> Result<AppSettings, String> {
    let mut settings = load_settings().map_err(|e| e.to_string())?;
    settings.notifications_enabled = enabled;
    settings.notification_permission_requested =
        settings.notification_permission_requested || permission_requested.unwrap_or(false);
    persist_settings(&settings).map_err(|e| e.to_string())?;
    Ok(settings)
}

// --- Logs ---

#[command]
pub async fn get_purchase_logs() -> Result<Vec<PurchaseLog>, String> {
    let mut logs = load_logs().map_err(|e| e.to_string())?;
    logs.sort_by(|a, b| b.executed_at.cmp(&a.executed_at));
    Ok(logs)
}

pub async fn run_scheduler<R: Runtime>(app: AppHandle<R>) {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;
        if let Err(error) = execute_due_schedules(&app).await {
            eprintln!("scheduler error: {error}");
        }
    }
}

// --- Internal helpers ---

fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("vitdaily")
}

fn load_schedules() -> anyhow::Result<Vec<Schedule>> {
    let path = data_dir().join("schedules.json");
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)?;
    let mut schedules: Vec<Schedule> = serde_json::from_str(&content)?;
    let now = Utc::now();
    let mut changed = false;

    for schedule in &mut schedules {
        if let Some(change) = schedule.pending_change.as_ref() {
            if validate_schedule_values(&change.time, change.amount).is_err() {
                schedule.pending_change = None;
                schedule.updated_at = now;
                changed = true;
                continue;
            }
        }

        changed = schedule.apply_due_pending_change(now) || changed;
    }

    if changed {
        persist_schedules(&schedules)?;
    }

    Ok(schedules)
}

fn persist_schedules(schedules: &[Schedule]) -> anyhow::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("schedules.json"),
        serde_json::to_string_pretty(schedules)?,
    )?;
    Ok(())
}

fn load_logs() -> anyhow::Result<Vec<PurchaseLog>> {
    let path = data_dir().join("logs.json");
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn persist_logs(logs: &[PurchaseLog]) -> anyhow::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("logs.json"), serde_json::to_string_pretty(logs)?)?;
    Ok(())
}

fn load_settings() -> anyhow::Result<AppSettings> {
    let path = data_dir().join("settings.json");
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = std::fs::read_to_string(path)?;
    let should_persist_migration = !content.contains("\"globalLiveLocked\"");
    let mut settings: AppSettings = serde_json::from_str(&content)?;
    let mut settings_changed = should_persist_migration;
    if settings.notifications_enabled && !settings.notification_permission_requested {
        settings.notifications_enabled = false;
        settings_changed = true;
    }
    if settings_changed {
        persist_settings(&settings)?;
    }
    Ok(settings)
}

fn persist_settings(settings: &AppSettings) -> anyhow::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(settings)?,
    )?;
    Ok(())
}

fn load_investment_threads() -> anyhow::Result<Vec<InvestmentThread>> {
    load_enveloped_vec("investment-threads.json")
}

fn persist_investment_threads(threads: &[InvestmentThread]) -> anyhow::Result<()> {
    persist_enveloped_vec("investment-threads.json", threads)
}

fn load_safety_events() -> anyhow::Result<Vec<SafetyEvent>> {
    load_enveloped_vec("safety-events.json")
}

fn persist_safety_events(events: &[SafetyEvent]) -> anyhow::Result<()> {
    persist_enveloped_vec("safety-events.json", events)
}

fn load_thread_validation_results() -> anyhow::Result<Vec<ThreadValidationResult>> {
    load_enveloped_vec("thread-validations.json")
}

fn persist_thread_validation_results(results: &[ThreadValidationResult]) -> anyhow::Result<()> {
    persist_enveloped_vec("thread-validations.json", results)
}

fn persist_portfolio_snapshots(snapshots: &[PortfolioTimePoint]) -> anyhow::Result<()> {
    persist_enveloped_vec("portfolio-snapshots.json", snapshots)
}

fn build_portfolio_analytics(
    logs: &[PurchaseLog],
    threads: &[InvestmentThread],
    validation_results: &[ThreadValidationResult],
    safety_events: &[SafetyEvent],
) -> PortfolioAnalytics {
    let mut time_series = build_local_portfolio_time_series(logs);
    if time_series.is_empty() {
        time_series = build_simulated_portfolio_time_series(threads, validation_results);
    }

    let total_budget_krw = threads.iter().map(|thread| thread.initial_budget_krw).sum();
    let invested_krw = logs
        .iter()
        .filter(|log| matches!(log.status, PurchaseStatus::Success))
        .map(|log| log.amount_krw)
        .sum();
    let current_value_krw = time_series
        .last()
        .map(|point| point.estimated_value_krw)
        .unwrap_or(0);
    let return_percent = time_series
        .last()
        .map(|point| point.return_percent)
        .unwrap_or(0.0);
    let max_drawdown_percent = time_series
        .iter()
        .map(|point| point.drawdown_percent)
        .fold(0.0, f64::max);
    let successful_buys = logs
        .iter()
        .filter(|log| matches!(log.status, PurchaseStatus::Success))
        .count() as u32;
    let blocked_orders = logs
        .iter()
        .filter(|log| matches!(log.status, PurchaseStatus::Blocked))
        .count() as u32;

    PortfolioAnalytics {
        summary: PortfolioSummary {
            total_budget_krw,
            invested_krw,
            current_value_krw,
            return_percent: round2(return_percent),
            max_drawdown_percent: round2(max_drawdown_percent),
            successful_buys,
            blocked_orders,
            safety_events: safety_events.len() as u32,
            latest_point_source: time_series.last().map(|point| point.source.clone()),
        },
        allocations: build_allocations(threads),
        threads: build_thread_analytics(threads, validation_results),
        time_series,
    }
}

fn build_local_portfolio_time_series(logs: &[PurchaseLog]) -> Vec<PortfolioTimePoint> {
    let mut successful_logs: Vec<&PurchaseLog> = logs
        .iter()
        .filter(|log| matches!(log.status, PurchaseStatus::Success) && log.volume_btc > 0.0)
        .collect();
    successful_logs.sort_by(|a, b| a.executed_at.cmp(&b.executed_at));

    let mut daily: BTreeMap<String, (u64, f64, f64)> = BTreeMap::new();
    for log in successful_logs {
        let date = log.executed_at.date_naive().to_string();
        let unit_price = log.amount_krw as f64 / log.volume_btc;
        let entry = daily.entry(date).or_insert((0, 0.0, unit_price));
        entry.0 += log.amount_krw;
        entry.1 += log.volume_btc;
        entry.2 = unit_price;
    }

    let mut invested = 0_u64;
    let mut btc_total = 0.0;
    let mut peak_value = 0_u64;
    let mut points = Vec::with_capacity(daily.len());

    for (date, (daily_invested, daily_btc, unit_price)) in daily {
        invested += daily_invested;
        btc_total += daily_btc;
        let estimated_value = (btc_total * unit_price).round().max(0.0) as u64;
        peak_value = peak_value.max(estimated_value);
        points.push(portfolio_point(
            date,
            invested,
            estimated_value,
            peak_value,
            PortfolioPointSource::Local,
        ));
    }

    points
}

fn build_simulated_portfolio_time_series(
    threads: &[InvestmentThread],
    validation_results: &[ThreadValidationResult],
) -> Vec<PortfolioTimePoint> {
    let mut latest_by_thread = latest_validation_by_thread(validation_results);
    let mut simulated_rows: Vec<(&InvestmentThread, ThreadValidationResult)> = threads
        .iter()
        .filter_map(|thread| {
            latest_by_thread
                .remove(&thread.id)
                .map(|result| (thread, result))
        })
        .collect();
    simulated_rows.sort_by(|a, b| a.1.period_end.cmp(&b.1.period_end));

    let mut invested = 0_u64;
    let mut estimated_value = 0_u64;
    let mut peak_value = 0_u64;
    let mut points = Vec::with_capacity(simulated_rows.len());

    for (thread, result) in simulated_rows {
        let thread_value = (thread.initial_budget_krw as f64
            * (1.0 + result.return_percent / 100.0))
            .round()
            .max(0.0) as u64;
        invested += thread.initial_budget_krw;
        estimated_value += thread_value;
        peak_value = peak_value.max(estimated_value);
        points.push(portfolio_point(
            result.period_end.date_naive().to_string(),
            invested,
            estimated_value,
            peak_value,
            PortfolioPointSource::Simulated,
        ));
    }

    points
}

fn portfolio_point(
    date: String,
    invested_krw: u64,
    estimated_value_krw: u64,
    peak_value_krw: u64,
    source: PortfolioPointSource,
) -> PortfolioTimePoint {
    let return_percent = if invested_krw == 0 {
        0.0
    } else {
        ((estimated_value_krw as f64 - invested_krw as f64) / invested_krw as f64) * 100.0
    };
    let drawdown_percent = if peak_value_krw == 0 {
        0.0
    } else {
        ((peak_value_krw as f64 - estimated_value_krw as f64) / peak_value_krw as f64) * 100.0
    };

    PortfolioTimePoint {
        date,
        invested_krw,
        estimated_value_krw,
        return_percent: round2(return_percent),
        drawdown_percent: round2(drawdown_percent),
        source,
    }
}

fn build_allocations(threads: &[InvestmentThread]) -> Vec<PortfolioAllocation> {
    let total_budget: u64 = threads.iter().map(|thread| thread.initial_budget_krw).sum();
    SupportedMarket::all()
        .into_iter()
        .filter_map(|market| {
            let budget_krw = threads
                .iter()
                .filter(|thread| thread.market == market)
                .map(|thread| thread.initial_budget_krw)
                .sum();
            if budget_krw == 0 {
                return None;
            }
            Some(PortfolioAllocation {
                market,
                budget_krw,
                share_percent: if total_budget == 0 {
                    0.0
                } else {
                    round2((budget_krw as f64 / total_budget as f64) * 100.0)
                },
            })
        })
        .collect()
}

fn build_thread_analytics(
    threads: &[InvestmentThread],
    validation_results: &[ThreadValidationResult],
) -> Vec<ThreadAnalytics> {
    let latest_by_thread = latest_validation_by_thread(validation_results);
    let mut rows: Vec<ThreadAnalytics> = threads
        .iter()
        .map(|thread| {
            let result = latest_by_thread.get(&thread.id);
            ThreadAnalytics {
                thread_id: thread.id,
                thread_name: thread.name.clone(),
                market: thread.market.clone(),
                budget_krw: thread.initial_budget_krw,
                validation_status: thread.validation_status.clone(),
                return_percent: result.map(|item| item.return_percent),
                max_drawdown_percent: result.map(|item| item.max_drawdown_percent),
                baseline_dca_return_percent: result.map(|item| item.baseline_dca_return_percent),
                simulated_trades: result.map(|item| item.simulated_trades),
                updated_at: result
                    .map(|item| item.created_at)
                    .unwrap_or(thread.updated_at),
            }
        })
        .collect();
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    rows
}

fn latest_validation_by_thread(
    validation_results: &[ThreadValidationResult],
) -> HashMap<uuid::Uuid, ThreadValidationResult> {
    let mut latest_by_thread: HashMap<uuid::Uuid, ThreadValidationResult> = HashMap::new();
    for result in validation_results {
        let should_replace = latest_by_thread
            .get(&result.thread_id)
            .map(|current| result.created_at > current.created_at)
            .unwrap_or(true);
        if should_replace {
            latest_by_thread.insert(result.thread_id, result.clone());
        }
    }
    latest_by_thread
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn load_enveloped_vec<T>(file_name: &str) -> anyhow::Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    let path = data_dir().join(file_name);
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path)?;
    match parse_storage_envelope(&content) {
        Ok(envelope) => Ok(envelope.data),
        Err(error) => {
            let backup = path.with_extension(format!(
                "json.corrupt-{}",
                Utc::now().format("%Y%m%d%H%M%S")
            ));
            std::fs::rename(&path, &backup)?;
            anyhow::bail!(
                "{file_name} 저장 데이터를 읽을 수 없어 백업했습니다: {} ({error})",
                backup.display()
            );
        }
    }
}

fn parse_storage_envelope<T>(content: &str) -> anyhow::Result<StorageEnvelope<Vec<T>>>
where
    T: serde::de::DeserializeOwned,
{
    let envelope = serde_json::from_str::<StorageEnvelope<Vec<T>>>(content)?;
    if envelope.schema_version != 1 {
        anyhow::bail!(
            "지원하지 않는 저장소 스키마 버전: {}",
            envelope.schema_version
        );
    }
    Ok(envelope)
}

fn persist_enveloped_vec<T>(file_name: &str, values: &[T]) -> anyhow::Result<()>
where
    T: serde::Serialize + Clone,
{
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let envelope = StorageEnvelope::new(values.to_vec());
    std::fs::write(
        dir.join(file_name),
        serde_json::to_string_pretty(&envelope)?,
    )?;
    Ok(())
}

fn validate_investment_thread(thread: &InvestmentThread) -> Result<(), String> {
    if thread.name.trim().is_empty() {
        return Err("스레드 이름을 입력해주세요".to_string());
    }
    if thread.initial_budget_krw < 5_000 {
        return Err("스레드 투자금은 최소 5,000원 이상이어야 합니다".to_string());
    }
    if thread.duration_days == 0 {
        return Err("투자 기간은 1일 이상이어야 합니다".to_string());
    }
    if !(0.0..=100.0).contains(&thread.max_loss_percent) || thread.max_loss_percent <= 0.0 {
        return Err("최대 손실률은 0% 초과 100% 이하로 입력해주세요".to_string());
    }
    if thread.daily_trade_cap == 0 || thread.daily_trade_cap > 10 {
        return Err("일일 거래 횟수는 1회 이상 10회 이하로 입력해주세요".to_string());
    }
    Ok(())
}

fn merge_investment_thread(
    existing: Option<&InvestmentThread>,
    mut incoming: InvestmentThread,
    now: chrono::DateTime<Utc>,
) -> InvestmentThread {
    match existing {
        Some(existing) => {
            let invalidates_validation = existing.market != incoming.market
                || existing.strategy_profile != incoming.strategy_profile
                || existing.initial_budget_krw != incoming.initial_budget_krw
                || existing.duration_days != incoming.duration_days
                || (existing.max_loss_percent - incoming.max_loss_percent).abs() > f64::EPSILON
                || existing.daily_trade_cap != incoming.daily_trade_cap;

            incoming.created_at = existing.created_at;
            incoming.updated_at = now;
            incoming.status = existing.status.clone();
            incoming.validation_status = existing.validation_status.clone();

            if matches!(existing.status, ThreadStatus::Armed | ThreadStatus::Live) {
                incoming.status = ThreadStatus::Draft;
                incoming.validation_status = ValidationStatus::Missing;
            } else if invalidates_validation {
                incoming.validation_status = ValidationStatus::Stale;
            }

            incoming
        }
        None => {
            incoming.created_at = now;
            incoming.updated_at = now;
            incoming.status = ThreadStatus::Draft;
            incoming.validation_status = ValidationStatus::Missing;
            incoming
        }
    }
}

fn validate_schedule(schedule: &Schedule) -> Result<(), String> {
    validate_schedule_values(&schedule.time, schedule.amount)?;
    if let Some(change) = schedule.pending_change.as_ref() {
        validate_schedule_values(&change.time, change.amount)?;
    }
    Ok(())
}

fn validate_schedule_values(time: &str, amount: u64) -> Result<(), String> {
    if amount < 5_000 {
        return Err("최소 주문 금액은 5,000원입니다".to_string());
    }
    NaiveTime::parse_from_str(time, "%H:%M")
        .map_err(|_| "매수 시간은 HH:MM 형식이어야 합니다".to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
struct SafetyEventDraft {
    event_type: SafetyEventType,
    category: AuditCategory,
    source: Option<String>,
    related_schedule_id: Option<uuid::Uuid>,
    reason: Option<String>,
}

fn record_safety_event(
    thread_id: Option<uuid::Uuid>,
    draft: SafetyEventDraft,
    message: String,
) -> anyhow::Result<uuid::Uuid> {
    let mut events = load_safety_events()?;
    let id = uuid::Uuid::new_v4();
    events.push(SafetyEvent {
        id,
        thread_id,
        event_type: draft.event_type,
        message,
        created_at: Utc::now(),
        category: draft.category,
        source: draft.source,
        related_schedule_id: draft.related_schedule_id,
        reason: draft.reason,
    });
    persist_safety_events(&events)?;
    Ok(id)
}

fn strategy_profile_label(profile: &StrategyProfile) -> &'static str {
    match profile {
        StrategyProfile::Stable => "안정적",
        StrategyProfile::Conservative => "보수적",
        StrategyProfile::Aggressive => "공격적",
    }
}

async fn execute_due_schedules<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<()> {
    let schedules = load_schedules()?;
    let now = Local::now();
    let current_time = format!("{:02}:{:02}", now.hour(), now.minute());
    let today = now.date_naive();
    let mut logs = load_logs()?;
    let mut changed = false;

    for schedule in schedules
        .iter()
        .filter(|schedule| schedule.enabled && schedule.time == current_time)
    {
        let already_executed = logs.iter().any(|log| {
            log.schedule_id == schedule.id
                && log.executed_at.with_timezone(&Local).date_naive() == today
        });

        if already_executed {
            continue;
        }

        let log = reconcile_legacy_schedule_order(schedule);
        notify_purchase_result(app, &log);
        logs.push(log);
        changed = true;
    }

    if changed {
        persist_logs(&logs)?;
    }

    Ok(())
}

fn notify_purchase_result<R: Runtime>(app: &AppHandle<R>, log: &PurchaseLog) {
    let Ok(settings) = load_settings() else {
        return;
    };

    if !settings.notifications_enabled {
        return;
    }

    let (title, body) = match log.status {
        PurchaseStatus::Success => (
            "VitDaily 매수 성공".to_string(),
            format!(
                "{}원 매수 주문 완료 · {:.8} BTC",
                log.amount_krw, log.volume_btc
            ),
        ),
        PurchaseStatus::Failure => (
            "VitDaily 매수 실패".to_string(),
            format!(
                "{}원 매수 실패 · {}",
                log.amount_krw,
                log.error_message.as_deref().unwrap_or("알 수 없는 오류")
            ),
        ),
        PurchaseStatus::Blocked => (
            "VitDaily 주문 차단".to_string(),
            format!(
                "{}원 주문 차단 · {}",
                log.amount_krw,
                log.error_message.as_deref().unwrap_or("안전 게이트 차단")
            ),
        ),
    };

    let _ = app.notification().builder().title(title).body(body).show();
}

fn reconcile_legacy_schedule_order(schedule: &Schedule) -> PurchaseLog {
    let executed_at = Utc::now();
    let gate = evaluate_legacy_schedule_live_gate();
    let safety_event_id = record_safety_event(
        None,
        SafetyEventDraft {
            event_type: SafetyEventType::Blocked,
            category: AuditCategory::SafetyGate,
            source: Some("legacy_schedule".to_string()),
            related_schedule_id: Some(schedule.id),
            reason: Some(gate.reason.clone()),
        },
        format!(
            "레거시 DCA 스케줄 {}의 {}원 실거래 주문 차단 · {}",
            schedule.id, schedule.amount, gate.reason
        ),
    )
    .ok();

    build_legacy_schedule_blocked_log(schedule, executed_at, gate, safety_event_id)
}

fn build_legacy_schedule_blocked_log(
    schedule: &Schedule,
    executed_at: chrono::DateTime<Utc>,
    gate: LiveOrderGateDecision,
    safety_event_id: Option<uuid::Uuid>,
) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: schedule.id,
        executed_at,
        amount_krw: schedule.amount,
        volume_btc: 0.0,
        status: PurchaseStatus::Blocked,
        error_message: Some(gate.reason.clone()),
        source: PurchaseLogSource::LegacySchedule,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::SafetyCheck,
        audit_category: AuditCategory::BlockedOrder,
        title: Some("레거시 DCA 스케줄 실거래 차단".to_string()),
        reason: Some(gate.reason),
        safety_event_id,
        strategy_signal_reason: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveOrderGateDecision {
    allowed: bool,
    reason: String,
}

fn evaluate_legacy_schedule_live_gate() -> LiveOrderGateDecision {
    match load_settings() {
        Ok(settings) if settings.global_live_locked => LiveOrderGateDecision {
            allowed: false,
            reason: "Global Live Lock이 잠겨 있어 레거시 스케줄 실주문이 차단되었습니다"
                .to_string(),
        },
        Ok(_) => LiveOrderGateDecision {
            allowed: false,
            reason:
                "레거시 DCA 스케줄은 아직 공유 Live Order Gate로 마이그레이션되지 않아 실주문이 차단되었습니다"
                    .to_string(),
        },
        Err(error) => LiveOrderGateDecision {
            allowed: false,
            reason: format!(
                "설정 로드 실패로 안전 기본값을 적용해 레거시 스케줄 실주문이 차단되었습니다: {error}"
            ),
        },
    }
}

fn get_credentials() -> anyhow::Result<(String, String)> {
    let access_key = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)?.get_password()?;
    let secret_key = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)?.get_password()?;
    Ok((access_key, secret_key))
}

async fn upbit_check_balance(access_key: &str, secret_key: &str) -> Result<(), String> {
    upbit_get_accounts(access_key, secret_key).await.map(|_| ())
}

#[derive(Debug)]
struct UpbitAccount {
    currency: String,
    balance: f64,
    locked: f64,
}

async fn upbit_get_accounts(
    access_key: &str,
    secret_key: &str,
) -> Result<Vec<UpbitAccount>, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct Claims {
        access_key: String,
        nonce: String,
    }

    #[derive(Deserialize)]
    struct AccountResponse {
        currency: String,
        balance: String,
        locked: String,
    }

    let nonce = uuid::Uuid::new_v4().to_string();
    let claims = Claims {
        access_key: access_key.to_string(),
        nonce,
    };

    let token = encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(secret_key.as_bytes()),
    )
    .map_err(|e| e.to_string())?;

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.upbit.com/v1/accounts")
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        let accounts = resp
            .json::<Vec<AccountResponse>>()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|account| UpbitAccount {
                currency: account.currency,
                balance: account.balance.parse::<f64>().unwrap_or(0.0),
                locked: account.locked.parse::<f64>().unwrap_or(0.0),
            })
            .collect();
        Ok(accounts)
    } else {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("업비트 API 오류: HTTP {status} {body}"))
    }
}

async fn upbit_get_btc_price_krw() -> Result<f64, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.upbit.com/v1/ticker")
        .query(&[("markets", "KRW-BTC")])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("업비트 현재가 오류: HTTP {status} {body}"));
    }

    let value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(value
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("trade_price"))
        .and_then(|price| price.as_f64())
        .unwrap_or(0.0))
}

#[allow(dead_code)]
async fn upbit_market_buy(
    access_key: &str,
    secret_key: &str,
    amount_krw: u64,
) -> Result<f64, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use sha2::{Digest, Sha512};

    #[derive(Serialize)]
    struct Claims {
        access_key: String,
        nonce: String,
        query_hash: String,
        query_hash_alg: String,
    }

    let query = format!("market=KRW-BTC&side=bid&price={amount_krw}&ord_type=price");
    let query_hash = hex_string(Sha512::digest(query.as_bytes()).as_slice());
    let claims = Claims {
        access_key: access_key.to_string(),
        nonce: uuid::Uuid::new_v4().to_string(),
        query_hash,
        query_hash_alg: "SHA512".to_string(),
    };

    let token = encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(secret_key.as_bytes()),
    )
    .map_err(|e| e.to_string())?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.upbit.com/v1/orders")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(query)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("업비트 주문 오류: HTTP {status} {body}"));
    }

    let value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(value
        .get("executed_volume")
        .and_then(|volume| volume.as_str())
        .and_then(|volume| volume.parse::<f64>().ok())
        .unwrap_or(0.0))
}

#[allow(dead_code)]
fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_storage_envelope() {
        let envelope = StorageEnvelope::new(vec!["KRW-BTC".to_string()]);
        let content = serde_json::to_string(&envelope).expect("serialize envelope");

        let parsed = parse_storage_envelope::<String>(&content).expect("parse envelope");

        assert_eq!(parsed.data, vec!["KRW-BTC".to_string()]);
    }

    #[test]
    fn rejects_corrupt_storage_envelope_instead_of_returning_empty_data() {
        let parsed = parse_storage_envelope::<String>("{ not valid json");

        assert!(parsed.is_err());
    }

    #[test]
    fn rejects_schema_mismatch_instead_of_returning_empty_data() {
        let parsed =
            parse_storage_envelope::<String>(r#"{"schemaVersion":1,"data":{"unexpected":true}}"#);

        assert!(parsed.is_err());
    }

    #[test]
    fn rejects_unsupported_storage_schema_version() {
        let parsed = parse_storage_envelope::<String>(r#"{"schemaVersion":2,"data":[]}"#);

        assert!(parsed.is_err());
    }

    #[test]
    fn old_settings_json_defaults_global_live_locked_to_true() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"notificationsEnabled":false}"#).expect("parse settings");

        assert!(settings.global_live_locked);
    }

    #[test]
    fn new_thread_live_input_is_forced_to_draft_missing() {
        let now = Utc::now();
        let mut incoming = sample_thread(now);
        incoming.status = ThreadStatus::Live;
        incoming.validation_status = ValidationStatus::Pass;

        let saved = merge_investment_thread(None, incoming, now);

        assert_eq!(saved.status, ThreadStatus::Draft);
        assert_eq!(saved.validation_status, ValidationStatus::Missing);
    }

    #[test]
    fn existing_live_thread_edit_resets_to_draft_missing() {
        let created_at = Utc::now();
        let now = created_at + chrono::Duration::seconds(10);
        let mut existing = sample_thread(created_at);
        existing.status = ThreadStatus::Live;
        existing.validation_status = ValidationStatus::Pass;
        let mut incoming = existing.clone();
        incoming.name = "편집된 스레드".to_string();
        incoming.status = ThreadStatus::Live;
        incoming.validation_status = ValidationStatus::Pass;

        let saved = merge_investment_thread(Some(&existing), incoming, now);

        assert_eq!(saved.name, "편집된 스레드");
        assert_eq!(saved.status, ThreadStatus::Draft);
        assert_eq!(saved.validation_status, ValidationStatus::Missing);
        assert_eq!(saved.created_at, created_at);
        assert_eq!(saved.updated_at, now);
    }

    #[test]
    fn pending_schedule_change_below_minimum_is_rejected() {
        let now = Utc::now();
        let mut schedule = Schedule {
            id: uuid::Uuid::new_v4(),
            time: "09:00".to_string(),
            amount: 5_000,
            enabled: true,
            pending_change: None,
            created_at: now,
            updated_at: now,
        };
        schedule.pending_change = Some(crate::types::PendingChange {
            time: "10:00".to_string(),
            amount: 4_999,
            apply_at: now,
        });

        assert!(validate_schedule(&schedule).is_err());
    }

    #[test]
    fn local_purchase_logs_build_portfolio_time_series() {
        let schedule_id = uuid::Uuid::new_v4();
        let logs = vec![
            sample_purchase_log(schedule_id, "2026-01-01T00:00:00Z", 10_000, 1.0),
            sample_purchase_log(schedule_id, "2026-01-02T00:00:00Z", 10_000, 1.0),
        ];

        let analytics = build_portfolio_analytics(&logs, &[], &[], &[]);

        assert_eq!(analytics.time_series.len(), 2);
        assert_eq!(analytics.summary.invested_krw, 20_000);
        assert_eq!(analytics.summary.current_value_krw, 20_000);
        assert_eq!(analytics.summary.successful_buys, 2);
        assert_eq!(
            analytics.summary.latest_point_source,
            Some(PortfolioPointSource::Local)
        );
    }

    #[test]
    fn validation_results_build_simulated_portfolio_when_no_logs_exist() {
        let now = Utc::now();
        let thread = sample_thread(now);
        let result = sample_validation_result(&thread, 12.5, 4.0);

        let analytics = build_portfolio_analytics(&[], &[thread], &[result], &[]);

        assert_eq!(analytics.time_series.len(), 1);
        assert_eq!(analytics.summary.total_budget_krw, 100_000);
        assert_eq!(analytics.summary.current_value_krw, 112_500);
        assert_eq!(analytics.summary.return_percent, 12.5);
        assert_eq!(analytics.summary.max_drawdown_percent, 0.0);
        assert_eq!(
            analytics.summary.latest_point_source,
            Some(PortfolioPointSource::Simulated)
        );
    }

    #[test]
    fn thread_allocations_match_backend_summary_budget() {
        let now = Utc::now();
        let mut btc_thread = sample_thread(now);
        btc_thread.initial_budget_krw = 80_000;
        let mut eth_thread = sample_thread(now);
        eth_thread.id = uuid::Uuid::new_v4();
        eth_thread.market = SupportedMarket::KrwEth;
        eth_thread.initial_budget_krw = 20_000;

        let analytics = build_portfolio_analytics(&[], &[btc_thread, eth_thread], &[], &[]);

        assert_eq!(analytics.summary.total_budget_krw, 100_000);
        assert_eq!(analytics.allocations.len(), 2);
        assert_eq!(analytics.allocations[0].share_percent, 80.0);
        assert_eq!(analytics.allocations[1].share_percent, 20.0);
    }

    fn sample_thread(now: chrono::DateTime<Utc>) -> InvestmentThread {
        InvestmentThread {
            id: uuid::Uuid::new_v4(),
            name: "테스트 스레드".to_string(),
            market: SupportedMarket::KrwBtc,
            initial_budget_krw: 100_000,
            duration_days: 30,
            strategy_profile: StrategyProfile::Conservative,
            max_loss_percent: 50.0,
            daily_trade_cap: 10,
            status: ThreadStatus::Draft,
            validation_status: ValidationStatus::Missing,
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_purchase_log(
        schedule_id: uuid::Uuid,
        executed_at: &str,
        amount_krw: u64,
        volume_btc: f64,
    ) -> PurchaseLog {
        PurchaseLog {
            id: uuid::Uuid::new_v4(),
            schedule_id,
            executed_at: executed_at.parse().expect("valid timestamp"),
            amount_krw,
            volume_btc,
            status: PurchaseStatus::Success,
            error_message: None,
            source: PurchaseLogSource::LegacySchedule,
            mode: ExecutionMode::Live,
            action: PurchaseLogAction::MarketBuy,
            audit_category: AuditCategory::Trade,
            title: Some("시장가 매수".to_string()),
            reason: None,
            safety_event_id: None,
            strategy_signal_reason: None,
        }
    }

    #[test]
    fn legacy_schedule_reconciliation_records_blocked_safety_audit_log() {
        let now = Utc::now();
        let schedule = Schedule {
            id: uuid::Uuid::new_v4(),
            time: "09:00".to_string(),
            amount: 10_000,
            enabled: true,
            pending_change: None,
            created_at: now,
            updated_at: now,
        };
        let gate = LiveOrderGateDecision {
            allowed: false,
            reason: "테스트 안전 게이트 차단".to_string(),
        };
        let safety_event_id = uuid::Uuid::new_v4();

        let log = build_legacy_schedule_blocked_log(&schedule, now, gate, Some(safety_event_id));

        assert_eq!(log.schedule_id, schedule.id);
        assert_eq!(log.status, PurchaseStatus::Blocked);
        assert_eq!(log.source, PurchaseLogSource::LegacySchedule);
        assert_eq!(log.mode, ExecutionMode::Live);
        assert_eq!(log.action, PurchaseLogAction::SafetyCheck);
        assert_eq!(log.audit_category, AuditCategory::BlockedOrder);
        assert_eq!(log.amount_krw, 10_000);
        assert_eq!(log.volume_btc, 0.0);
        assert!(log.reason.unwrap_or_default().contains("차단"));
        assert_eq!(log.safety_event_id, Some(safety_event_id));
    }

    fn sample_validation_result(
        thread: &InvestmentThread,
        return_percent: f64,
        max_drawdown_percent: f64,
    ) -> ThreadValidationResult {
        let now = Utc::now();
        ThreadValidationResult {
            id: uuid::Uuid::new_v4(),
            thread_id: thread.id,
            status: ValidationStatus::Pass,
            period_days: 365,
            period_start: now - chrono::Duration::days(365),
            period_end: now,
            market: thread.market.clone(),
            strategy_profile: thread.strategy_profile.clone(),
            simulated_trades: 12,
            return_percent,
            max_drawdown_percent,
            baseline_dca_return_percent: return_percent - 1.0,
            baseline_dca_max_drawdown_percent: max_drawdown_percent + 1.0,
            baseline_buy_hold_return_percent: return_percent - 2.0,
            baseline_buy_hold_max_drawdown_percent: max_drawdown_percent + 2.0,
            recent_90d_return_percent: return_percent / 2.0,
            recent_90d_dca_return_percent: return_percent / 3.0,
            fees_krw: 100,
            fee_percent: 0.05,
            slippage_percent: 0.05,
            doubled_slippage_return_percent: return_percent - 0.5,
            reasons: vec!["테스트".to_string()],
            assumptions: vec!["테스트 가정".to_string()],
            created_at: now,
        }
    }
}
