use crate::types::{
    ApiStatus, AppSettings, AuditCategory, CredentialReadinessStatus, ExecutionMode,
    InvestmentThread, LegacyScheduleLivePolicy, LegacyScheduleLivePolicyStatus,
    LiveActivationRequest, LiveMarketSellRequest, LiveOrderChanceStatus,
    LiveOrderFinalConfirmationStatus, LiveOrderGateBlockReason, LiveOrderGateCheck,
    LiveOrderGateDecision, LiveOrderGateSource, LiveOrderIntent, PaperExecutionResult,
    PaperSignalAction, PortfolioAllocation, PortfolioAnalytics, PortfolioPointSource,
    PortfolioSnapshot, PortfolioSummary, PortfolioTimePoint, PurchaseLog, PurchaseLogAction,
    PurchaseLogSource, PurchaseStatus, SafetyEvent, SafetyEventType, Schedule, StorageEnvelope,
    StrategyProfile, StrategyProfileInfo, StrategySignalEvaluation, SupportedMarket,
    ThreadAnalytics, ThreadAutoLoopAction, ThreadAutoLoopMode, ThreadAutoLoopResult, ThreadStatus,
    ThreadValidationResult, UpbitOrderPayloadPreview, ValidationStatus,
};
use chrono::{Local, NaiveTime, Timelike, Utc};
use keyring::Entry;
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
};
use tauri::{command, AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::time::{interval, Duration};

const KEYRING_SERVICE: &str = "vitdaily";
const KEYRING_ACCESS_KEY: &str = "upbit_access_key";
const KEYRING_SECRET_KEY: &str = "upbit_secret_key";
const DEFAULT_DAILY_TRADE_CAP: u32 = 10;
const DEFAULT_LIVE_SELL_GATE_AMOUNT_KRW: u64 = 5_000;
const LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK: usize = 3;
const DEFAULT_LIVE_CHANCE_PROBE_AMOUNT_KRW: u64 = 5_000;
const REQUIRED_LIVE_CONFIRMATION_PHRASE: &str = "실거래 위험을 이해하고 Live 주문을 활성화합니다";

// --- API Credentials ---

#[command]
pub async fn save_api_credentials(access_key: String, secret_key: String) -> Result<(), String> {
    let had_credentials = stored_credentials_available();
    reset_live_readiness_after_credential_change(if had_credentials {
        "credential_replace"
    } else {
        "credential_save"
    })?;

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
    reset_live_readiness_after_credential_change("credential_delete")?;
    Ok(())
}

#[command]
pub async fn get_api_status() -> ApiStatus {
    let has_credentials = stored_credentials_available();

    ApiStatus {
        connected: false,
        has_credentials,
        credential_readiness: if has_credentials {
            CredentialReadinessStatus::StoredUnchecked
        } else {
            CredentialReadinessStatus::Missing
        },
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
            credential_readiness: CredentialReadinessStatus::Connected,
            error: None,
        }),
        Err(e) => Ok(ApiStatus {
            connected: false,
            has_credentials: true,
            credential_readiness: credential_readiness_from_error(&e),
            error: Some(e),
        }),
    }
}

#[command]
pub async fn get_live_order_chance_status() -> Result<LiveOrderChanceStatus, String> {
    let market = SupportedMarket::KrwBtc;
    let checked_at = Utc::now();
    let Ok((access_key, secret_key)) = get_credentials() else {
        return Ok(build_live_order_chance_status(
            &market,
            None,
            vec![LiveOrderGateBlockReason::CredentialsMissing],
            CredentialReadinessStatus::Missing,
            None,
            checked_at,
        ));
    };

    let executor = UpbitLiveOrderExecutor;
    match executor
        .order_chance(&access_key, &secret_key, &market)
        .await
    {
        Ok(chance) => {
            let block_reasons = live_order_chance_settings_block_reasons(&chance);
            Ok(build_live_order_chance_status(
                &market,
                Some(&chance),
                block_reasons,
                CredentialReadinessStatus::Connected,
                None,
                checked_at,
            ))
        }
        Err(error) => {
            let credential_readiness = credential_readiness_from_error(&error);
            Ok(build_live_order_chance_status(
                &market,
                None,
                vec![live_order_block_reason_from_credential_readiness(
                    &credential_readiness,
                )],
                credential_readiness,
                Some(error),
                checked_at,
            ))
        }
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

#[command]
pub async fn get_legacy_schedule_live_policy_statuses(
) -> Result<Vec<LegacyScheduleLivePolicyStatus>, String> {
    let schedules = load_schedules().map_err(|e| e.to_string())?;
    Ok(schedules
        .iter()
        .map(build_legacy_schedule_live_policy_status)
        .collect())
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
            title: "안정 평균회귀".to_string(),
            risk_label: "낮은 노출 · 1 round-trip/일".to_string(),
            trade_frequency: "최대 1 round-trip/일".to_string(),
            indicators: vec!["Bollinger 20/2".to_string(), "MACD 회복 확인".to_string(), "ATR stop".to_string()],
            summary: "하단 밴드 회복 후 중단 목표에서 빠르게 정리합니다. 평균 보유 8시간과 시장 노출 20% 이하를 목표로 합니다.".to_string(),
        },
        StrategyProfileInfo {
            profile: StrategyProfile::Conservative,
            title: "보수 평균회귀".to_string(),
            risk_label: "균형형 · 2 round-trip/일".to_string(),
            trade_frequency: "최대 2 round-trip/일".to_string(),
            indicators: vec!["Bollinger 회복".to_string(), "MACD histogram".to_string(), "ATR stop".to_string()],
            summary: "하단 밴드 회복과 MACD 개선을 함께 확인하고 중단/상단 회귀에서 청산합니다. 기본 검증 후보입니다.".to_string(),
        },
        StrategyProfileInfo {
            profile: StrategyProfile::Aggressive,
            title: "공격 평균회귀".to_string(),
            risk_label: "높은 빈도 · 4 round-trip/일".to_string(),
            trade_frequency: "최대 4 round-trip/일".to_string(),
            indicators: vec!["Bollinger %B".to_string(), "MACD 반등".to_string(), "ATR trailing stop".to_string()],
            summary: "더 낮은 밴드 구간에서 빠르게 진입하고 상단 회복을 노립니다. 노출 50%와 평균 보유 18시간 기준을 넘으면 실패 처리됩니다.".to_string(),
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

    let candles =
        crate::strategy::fetch_backtest_hourly_candles(&thread.market, thread.duration_days)
            .await?;
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
pub async fn run_thread_paper_execution(thread_id: String) -> Result<PaperExecutionResult, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == uuid)
        .cloned()
        .ok_or_else(|| "Paper 실행할 스레드를 찾을 수 없습니다".to_string())?;

    if !matches!(thread.status, ThreadStatus::Draft | ThreadStatus::Paper) {
        return Err(
            "Paper 실행은 Draft 또는 Paper 상태의 스레드에서만 실행할 수 있습니다".to_string(),
        );
    }

    let candles = crate::strategy::fetch_recent_signal_hourly_candles(&thread.market).await?;
    let signal = crate::strategy::evaluate_latest_signal_for_thread(&thread, &candles)?;
    let requested_at = signal.evaluated_at;
    let amount_krw = paper_order_amount_krw(&thread);
    let gate = evaluate_live_order_gate(LiveOrderGateInput::investment_thread(
        &thread,
        amount_krw,
        requested_at,
    ));
    let mut logs = load_logs().map_err(|e| e.to_string())?;
    let result = build_paper_execution_result(&thread, signal, gate, &logs, amount_krw);

    if !result.duplicate {
        if let Some(log) = result.log.clone() {
            logs.push(log);
            persist_logs(&logs).map_err(|e| e.to_string())?;
        }
    }

    if let Some(saved_thread) = threads.iter_mut().find(|thread| thread.id == uuid) {
        if matches!(saved_thread.status, ThreadStatus::Draft) {
            saved_thread.status = ThreadStatus::Paper;
        }
        saved_thread.updated_at = Utc::now();
    }
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;

    let _ = record_safety_event(
        Some(uuid),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::PaperTrade,
            source: Some("paper_execution_loop".to_string()),
            related_schedule_id: None,
            reason: Some(result.message.clone()),
        },
        format!(
            "{} Paper 실행 · {:?} · {}",
            thread.market.as_upbit_market(),
            result.signal.action,
            result.message
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

#[command]
pub fn get_live_activation_confirmation_phrase() -> String {
    REQUIRED_LIVE_CONFIRMATION_PHRASE.to_string()
}

#[command]
pub async fn activate_thread_live(
    request: LiveActivationRequest,
) -> Result<InvestmentThread, String> {
    let confirmation_text = validate_live_confirmation_text(&request.confirmation_text)?;

    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter_mut()
        .find(|thread| thread.id == request.thread_id)
        .ok_or_else(|| "활성화할 스레드를 찾을 수 없습니다".to_string())?;

    if !matches!(
        thread.status,
        ThreadStatus::Draft | ThreadStatus::Paper | ThreadStatus::Paused | ThreadStatus::Armed
    ) {
        return Err(
            "Draft, Paper, Paused, Armed 상태의 스레드만 실거래 준비 상태로 전환할 수 있습니다"
                .to_string(),
        );
    }

    let previous_thread = thread.clone();
    let confirmed_at = Utc::now();
    let mut activation_candidate = thread.clone();
    apply_live_confirmation(&mut activation_candidate, confirmation_text, confirmed_at);
    activation_candidate.status = ThreadStatus::Armed;
    activation_candidate.updated_at = confirmed_at;

    let gate = evaluate_live_order_gate(LiveOrderGateInput::investment_thread(
        &activation_candidate,
        paper_order_amount_krw(&activation_candidate),
        confirmed_at,
    ));
    if !gate.allowed {
        let _ = record_safety_event(
            Some(activation_candidate.id),
            SafetyEventDraft {
                event_type: SafetyEventType::Blocked,
                category: AuditCategory::SafetyGate,
                source: Some("live_activation".to_string()),
                related_schedule_id: None,
                reason: Some(gate.reason.clone()),
            },
            format!(
                "{} 실거래 준비 상태 전환 차단 · {}",
                activation_candidate.market.as_upbit_market(),
                gate.reason
            ),
        );
        return Err(gate.reason);
    }

    record_safety_event(
        Some(activation_candidate.id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_activation".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=attempt; previousStatus={:?}; newStatus=Armed; previousFinalConfirmation={:?}; newFinalConfirmation=Confirmed; finalConfirmedAt={}; gate={}",
                previous_thread.status,
                previous_thread.final_confirmation_status,
                confirmed_at.to_rfc3339(),
                gate.reason
            )),
        },
        format!(
            "{} 실거래 준비 상태 전환 시도 · {}",
            activation_candidate.market.as_upbit_market(),
            gate.reason
        ),
    )
    .map_err(|e| e.to_string())?;

    *thread = activation_candidate.clone();
    let activated = activation_candidate;
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;

    if let Err(error) = record_safety_event(
        Some(activated.id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_activation".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=success; previousStatus={:?}; newStatus=Armed; previousFinalConfirmation={:?}; newFinalConfirmation=Confirmed; finalConfirmedAt={}; gate={}",
                previous_thread.status,
                previous_thread.final_confirmation_status,
                confirmed_at.to_rfc3339(),
                gate.reason
            )),
        },
        format!(
            "{} 실거래 준비 상태 전환 성공 · {}",
            activated.market.as_upbit_market(),
            gate.reason
        ),
    ) {
        if let Some(saved) = threads.iter_mut().find(|thread| thread.id == activated.id) {
            *saved = previous_thread;
        }
        let _ = persist_investment_threads(&threads);
        return Err(error.to_string());
    }

    Ok(activated)
}

#[command]
pub async fn start_thread_live(thread_id: String) -> Result<InvestmentThread, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter_mut()
        .find(|thread| thread.id == uuid)
        .ok_or_else(|| "Live로 전환할 스레드를 찾을 수 없습니다".to_string())?;

    if thread.status != ThreadStatus::Armed {
        return Err("Armed 상태의 스레드만 Live로 전환할 수 있습니다".to_string());
    }
    if !thread_has_valid_live_confirmation(thread) {
        return Err("저장된 최종 확인 문구가 유효하지 않아 Live 전환을 차단했습니다".to_string());
    }

    let previous_thread = thread.clone();
    let live_started_at = Utc::now();
    let mut live_candidate = thread.clone();
    live_candidate.status = ThreadStatus::Live;
    live_candidate.updated_at = live_started_at;

    let gate = evaluate_live_order_gate(LiveOrderGateInput::investment_thread(
        &live_candidate,
        paper_order_amount_krw(&live_candidate),
        live_started_at,
    ));
    if !gate.allowed {
        let _ = record_safety_event(
            Some(live_candidate.id),
            SafetyEventDraft {
                event_type: SafetyEventType::Blocked,
                category: AuditCategory::SafetyGate,
                source: Some("live_start".to_string()),
                related_schedule_id: None,
                reason: Some(gate.reason.clone()),
            },
            format!(
                "{} Live 전환 차단 · {}",
                live_candidate.market.as_upbit_market(),
                gate.reason
            ),
        );
        return Err(gate.reason);
    }

    record_safety_event(
        Some(live_candidate.id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_start".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=attempt; previousStatus={:?}; newStatus=Live; finalConfirmation={:?}; finalConfirmedAt={}; gate={}",
                previous_thread.status,
                previous_thread.final_confirmation_status,
                previous_thread
                    .final_confirmed_at
                    .as_ref()
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_else(|| "missing".to_string()),
                gate.reason
            )),
        },
        format!(
            "{} Live 전환 시도 · {}",
            live_candidate.market.as_upbit_market(),
            gate.reason
        ),
    )
    .map_err(|e| e.to_string())?;

    *thread = live_candidate.clone();
    let live_thread = live_candidate;
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;

    if let Err(error) = record_safety_event(
        Some(live_thread.id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_start".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=success; previousStatus={:?}; newStatus=Live; finalConfirmation={:?}; finalConfirmedAt={}; gate={}",
                previous_thread.status,
                previous_thread.final_confirmation_status,
                previous_thread
                    .final_confirmed_at
                    .as_ref()
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_else(|| "missing".to_string()),
                gate.reason
            )),
        },
        format!(
            "{} Live 전환 성공 · {}",
            live_thread.market.as_upbit_market(),
            gate.reason
        ),
    ) {
        if let Some(saved) = threads.iter_mut().find(|thread| thread.id == live_thread.id) {
            *saved = previous_thread;
        }
        let _ = persist_investment_threads(&threads);
        return Err(error.to_string());
    }

    Ok(live_thread)
}

#[command]
pub async fn pause_thread(thread_id: String) -> Result<InvestmentThread, String> {
    transition_thread_to_safety_state(
        thread_id,
        ThreadStatus::Paused,
        SafetyEventType::Warning,
        "manual_pause",
        "스레드를 일시정지했습니다",
    )
}

#[command]
pub async fn stop_thread(thread_id: String) -> Result<InvestmentThread, String> {
    transition_thread_to_safety_state(
        thread_id,
        ThreadStatus::Stopped,
        SafetyEventType::Stopped,
        "manual_stop",
        "스레드를 중지했고 이후 실주문을 차단합니다",
    )
}

#[command]
pub async fn complete_thread(thread_id: String) -> Result<InvestmentThread, String> {
    transition_thread_to_safety_state(
        thread_id,
        ThreadStatus::Completed,
        SafetyEventType::Info,
        "manual_complete",
        "스레드를 완료 처리했고 이후 실주문을 차단합니다",
    )
}

#[command]
pub async fn run_thread_auto_loop_tick(thread_id: String) -> Result<ThreadAutoLoopResult, String> {
    let executor = UpbitLiveOrderExecutor;
    run_thread_auto_loop_tick_with_executor(thread_id, &executor).await
}

#[command]
pub async fn run_all_thread_auto_loop_ticks() -> Result<Vec<ThreadAutoLoopResult>, String> {
    let executor = UpbitLiveOrderExecutor;
    run_all_thread_auto_loop_ticks_with_executor(&executor).await
}

#[command]
pub async fn preview_thread_live_order_payload(
    thread_id: String,
    intent: LiveOrderIntent,
    amount_krw: Option<u64>,
    volume: Option<String>,
) -> Result<UpbitOrderPayloadPreview, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == uuid)
        .cloned()
        .ok_or_else(|| "실주문 미리보기를 생성할 스레드를 찾을 수 없습니다".to_string())?;
    let order_amount = amount_krw.unwrap_or_else(|| paper_order_amount_krw(&thread));
    let gate = evaluate_live_order_gate(LiveOrderGateInput::investment_thread(
        &thread,
        order_amount,
        Utc::now(),
    ));

    if !gate.allowed {
        let safety_event_id = record_live_order_gate_block_event(&gate).ok();
        let mut logs = load_logs().map_err(|e| e.to_string())?;
        logs.push(build_live_order_blocked_log(&gate, safety_event_id));
        persist_logs(&logs).map_err(|e| e.to_string())?;
        return Err(gate.reason);
    }

    build_upbit_order_payload_preview(&thread.market, intent, order_amount, volume)
}

#[command]
pub async fn submit_thread_live_market_buy(
    thread_id: String,
    amount_krw: Option<u64>,
) -> Result<Vec<PurchaseLog>, String> {
    let executor = UpbitLiveOrderExecutor;
    submit_thread_live_market_buy_with_executor(thread_id, amount_krw, &executor).await
}

#[command]
pub async fn submit_thread_live_market_sell(
    request: LiveMarketSellRequest,
) -> Result<Vec<PurchaseLog>, String> {
    let executor = UpbitLiveOrderExecutor;
    submit_thread_live_market_sell_with_executor(request, &executor).await
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

#[command]
pub async fn set_live_trading_settings(
    global_live_locked: bool,
    strategy_logic_approved: bool,
) -> Result<AppSettings, String> {
    let mut settings = load_settings().map_err(|e| e.to_string())?;
    let changed = settings.global_live_locked != global_live_locked
        || settings.strategy_logic_approved != strategy_logic_approved;
    let previous_global_live_locked = settings.global_live_locked;
    let previous_strategy_logic_approved = settings.strategy_logic_approved;
    if changed {
        record_safety_event(
            None,
            SafetyEventDraft {
                event_type: SafetyEventType::Info,
                category: AuditCategory::SafetyGate,
                source: Some("live_trading_settings".to_string()),
                related_schedule_id: None,
                reason: Some(format!(
                    "outcome=attempt; previousGlobalLiveLocked={previous_global_live_locked}, newGlobalLiveLocked={global_live_locked}, previousStrategyLogicApproved={previous_strategy_logic_approved}, newStrategyLogicApproved={strategy_logic_approved}"
                )),
            },
            "Live Trading 설정 변경 시도".to_string(),
        )
        .map_err(|e| e.to_string())?;
    }
    settings.global_live_locked = global_live_locked;
    settings.strategy_logic_approved = strategy_logic_approved;
    persist_settings(&settings).map_err(|e| e.to_string())?;
    if changed {
        if let Err(error) = record_safety_event(
            None,
            SafetyEventDraft {
                event_type: SafetyEventType::Info,
                category: AuditCategory::SafetyGate,
                source: Some("live_trading_settings".to_string()),
                related_schedule_id: None,
                reason: Some(format!(
                    "outcome=success; previousGlobalLiveLocked={previous_global_live_locked}, newGlobalLiveLocked={global_live_locked}, previousStrategyLogicApproved={previous_strategy_logic_approved}, newStrategyLogicApproved={strategy_logic_approved}"
                )),
            },
            "Live Trading 설정 변경 성공".to_string(),
        ) {
            settings.global_live_locked = previous_global_live_locked;
            settings.strategy_logic_approved = previous_strategy_logic_approved;
            let _ = persist_settings(&settings);
            return Err(error.to_string());
        }
    }
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
            log::error!("scheduler error: {error}");
        }
        if let Err(error) =
            run_all_thread_auto_loop_ticks_with_executor(&UpbitLiveOrderExecutor).await
        {
            log::error!("thread auto loop error: {error}");
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
    let should_persist_migration =
        !content.contains("\"globalLiveLocked\"") || !content.contains("\"strategyLogicApproved\"");
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

fn transition_thread_to_safety_state(
    thread_id: String,
    status: ThreadStatus,
    event_type: SafetyEventType,
    source: &str,
    message: &str,
) -> Result<InvestmentThread, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let mut threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter_mut()
        .find(|thread| thread.id == uuid)
        .ok_or_else(|| "상태를 변경할 스레드를 찾을 수 없습니다".to_string())?;
    validate_thread_safety_transition(&thread.status, &status)?;

    thread.status = status;
    clear_live_confirmation(thread);
    thread.updated_at = Utc::now();
    let updated = thread.clone();
    persist_investment_threads(&threads).map_err(|e| e.to_string())?;

    let _ = record_safety_event(
        Some(updated.id),
        SafetyEventDraft {
            event_type,
            category: AuditCategory::SafetyGate,
            source: Some(source.to_string()),
            related_schedule_id: None,
            reason: Some(message.to_string()),
        },
        format!("{} · {}", updated.name, message),
    );

    Ok(updated)
}

fn validate_thread_safety_transition(
    current: &ThreadStatus,
    target: &ThreadStatus,
) -> Result<(), String> {
    if matches!(current, ThreadStatus::Completed) {
        return Err("완료된 스레드는 상태를 변경할 수 없습니다".to_string());
    }
    if matches!(current, ThreadStatus::Stopped) && !matches!(target, ThreadStatus::Stopped) {
        return Err(
            "중지된 스레드는 별도 재활성화 흐름 없이 상태를 변경할 수 없습니다".to_string(),
        );
    }
    match target {
        ThreadStatus::Paused => {
            if !matches!(
                current,
                ThreadStatus::Paper | ThreadStatus::Armed | ThreadStatus::Live
            ) {
                return Err("Paper, Armed, Live 상태의 스레드만 일시정지할 수 있습니다".to_string());
            }
        }
        ThreadStatus::Stopped => {
            if matches!(current, ThreadStatus::Stopped) {
                return Err("이미 중지된 스레드입니다".to_string());
            }
        }
        ThreadStatus::Completed => {
            if !matches!(
                current,
                ThreadStatus::Paper
                    | ThreadStatus::Paused
                    | ThreadStatus::Armed
                    | ThreadStatus::Live
            ) {
                return Err(
                    "Paper, Paused, Armed, Live 상태의 스레드만 완료 처리할 수 있습니다"
                        .to_string(),
                );
            }
        }
        ThreadStatus::Draft | ThreadStatus::Paper | ThreadStatus::Armed | ThreadStatus::Live => {
            return Err("이 전환은 전용 command를 통해서만 수행할 수 있습니다".to_string());
        }
    }
    Ok(())
}

async fn run_thread_auto_loop_tick_with_executor<E: LiveOrderExecutor>(
    thread_id: String,
    executor: &E,
) -> Result<ThreadAutoLoopResult, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let threads = load_investment_threads().map_err(|error| error.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == uuid)
        .cloned()
        .ok_or_else(|| "자동 실행할 스레드를 찾을 수 없습니다".to_string())?;

    run_auto_loop_tick_for_thread_with_executor(thread, executor).await
}

async fn run_all_thread_auto_loop_ticks_with_executor<E: LiveOrderExecutor>(
    executor: &E,
) -> Result<Vec<ThreadAutoLoopResult>, String> {
    let threads = load_investment_threads().map_err(|error| error.to_string())?;
    let runnable_threads: Vec<InvestmentThread> = threads
        .into_iter()
        .filter(|thread| matches!(thread.status, ThreadStatus::Paper | ThreadStatus::Live))
        .collect();
    let mut results = Vec::with_capacity(runnable_threads.len());

    for thread in runnable_threads {
        match run_auto_loop_tick_for_thread_with_executor(thread.clone(), executor).await {
            Ok(result) => results.push(result),
            Err(error) => {
                let _ = record_safety_event(
                    Some(thread.id),
                    SafetyEventDraft {
                        event_type: SafetyEventType::Warning,
                        category: AuditCategory::SafetyGate,
                        source: Some("thread_auto_loop".to_string()),
                        related_schedule_id: None,
                        reason: Some(error.clone()),
                    },
                    format!("{} 자동 실행 tick 실패 · {}", thread.name, error),
                );
                results.push(ThreadAutoLoopResult {
                    thread_id: thread.id,
                    mode: if thread.status == ThreadStatus::Live {
                        ThreadAutoLoopMode::Live
                    } else {
                        ThreadAutoLoopMode::Paper
                    },
                    action: ThreadAutoLoopAction::Skipped,
                    message: error,
                    idempotency_key: None,
                    retry_count: 0,
                    paper_result: None,
                    live_order_gate: None,
                    logs: Vec::new(),
                });
            }
        }
    }

    Ok(results)
}

async fn run_auto_loop_tick_for_thread_with_executor<E: LiveOrderExecutor>(
    thread: InvestmentThread,
    executor: &E,
) -> Result<ThreadAutoLoopResult, String> {
    match thread.status {
        ThreadStatus::Paper => {
            let paper_result = run_thread_paper_execution(thread.id.to_string()).await?;
            Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Paper,
                action: if paper_result.duplicate {
                    ThreadAutoLoopAction::DuplicateTick
                } else {
                    ThreadAutoLoopAction::PaperTick
                },
                message: paper_result.message.clone(),
                idempotency_key: Some(paper_result.idempotency_key.clone()),
                retry_count: 0,
                paper_result: Some(paper_result),
                live_order_gate: None,
                logs: Vec::new(),
            })
        }
        ThreadStatus::Live => run_live_auto_loop_tick_with_executor(thread, executor).await,
        ThreadStatus::Draft
        | ThreadStatus::Armed
        | ThreadStatus::Paused
        | ThreadStatus::Stopped
        | ThreadStatus::Completed => {
            Err("자동 실행 loop는 Paper 또는 Live 상태의 스레드만 실행할 수 있습니다".to_string())
        }
    }
}

async fn run_live_auto_loop_tick_with_executor<E: LiveOrderExecutor>(
    thread: InvestmentThread,
    executor: &E,
) -> Result<ThreadAutoLoopResult, String> {
    let candles = crate::strategy::fetch_recent_signal_hourly_candles(&thread.market).await?;
    let signal = crate::strategy::evaluate_latest_signal_for_thread(&thread, &candles)?;
    let amount_krw = paper_order_amount_krw(&thread);
    let logs = load_logs().map_err(|error| error.to_string())?;
    let open_position = live_open_position(&thread, &logs);

    if signal.action == PaperSignalAction::Hold {
        let idempotency_key = live_loop_idempotency_key(&thread, &signal, amount_krw);
        let _ = record_safety_event(
            Some(thread.id),
            SafetyEventDraft {
                event_type: SafetyEventType::Info,
                category: AuditCategory::Trade,
                source: Some("live_auto_loop".to_string()),
                related_schedule_id: None,
                reason: Some(format!(
                    "idempotencyKey={idempotency_key}; signal=hold; reason={}",
                    signal.reason
                )),
            },
            format!("{} Live 자동 tick 대기 · {}", thread.name, signal.reason),
        );
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::Hold,
            message: "전략 신호가 대기 상태라 Live 주문을 제출하지 않았습니다".to_string(),
            idempotency_key: Some(idempotency_key),
            retry_count: 0,
            paper_result: None,
            live_order_gate: None,
            logs: Vec::new(),
        });
    }

    if signal.action == PaperSignalAction::Sell {
        let Some(position) = open_position else {
            let idempotency_key = live_loop_idempotency_key(&thread, &signal, amount_krw);
            let _ = record_safety_event(
                Some(thread.id),
                SafetyEventDraft {
                    event_type: SafetyEventType::Info,
                    category: AuditCategory::Trade,
                    source: Some("live_auto_loop".to_string()),
                    related_schedule_id: None,
                    reason: Some(format!(
                        "idempotencyKey={idempotency_key}; signal=sell; openPosition=none; reason={}",
                        signal.reason
                    )),
                },
                format!("{} Live 자동 매도 대기 · 열린 포지션 없음", thread.name),
            );
            return Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::SellSkipped,
                message:
                    "매도 신호가 있지만 이 스레드에서 확인된 열린 Live 포지션이 없어 매도하지 않았습니다"
                        .to_string(),
                idempotency_key: Some(idempotency_key),
                retry_count: 0,
                paper_result: None,
                live_order_gate: None,
                logs: Vec::new(),
            });
        };

        let estimated_amount_krw = estimated_live_sell_amount_krw(&position, signal.price_krw);
        let idempotency_key = live_loop_idempotency_key(&thread, &signal, estimated_amount_krw);

        if live_loop_has_pending_or_filled_order(&logs, &idempotency_key) {
            let retry_count = live_loop_failed_retry_count(&logs, &idempotency_key);
            return Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::DuplicateTick,
                message: "동일한 Live tick 매도 주문이 이미 제출 또는 체결되어 중복 제출을 막았습니다"
                    .to_string(),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32,
                paper_result: None,
                live_order_gate: None,
                logs: Vec::new(),
            });
        }

        let retry_count = live_loop_failed_retry_count(&logs, &idempotency_key);
        if retry_count >= LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK {
            let _ = record_safety_event(
                Some(thread.id),
                SafetyEventDraft {
                    event_type: SafetyEventType::Warning,
                    category: AuditCategory::ApiFailure,
                    source: Some("live_auto_loop".to_string()),
                    related_schedule_id: None,
                    reason: Some(format!(
                        "idempotencyKey={idempotency_key}; retryCount={retry_count}; retryLimit={LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK}"
                    )),
                },
                format!(
                    "{} Live 자동 매도 retry 제한 도달 · {}회",
                    thread.name, retry_count
                ),
            );
            return Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::RetryLimited,
                message: "동일 tick의 Upbit 매도 오류 retry 제한에 도달해 주문을 제출하지 않았습니다"
                    .to_string(),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32,
                paper_result: None,
                live_order_gate: None,
                logs: Vec::new(),
            });
        }

        return submit_live_auto_market_sell_with_executor(
            thread,
            signal,
            position,
            estimated_amount_krw,
            idempotency_key,
            retry_count,
            executor,
        )
        .await;
    }

    let idempotency_key = live_loop_idempotency_key(&thread, &signal, amount_krw);

    if open_position.is_some() {
        let _ = record_safety_event(
            Some(thread.id),
            SafetyEventDraft {
                event_type: SafetyEventType::Info,
                category: AuditCategory::Trade,
                source: Some("live_auto_loop".to_string()),
                related_schedule_id: None,
                reason: Some(format!(
                    "idempotencyKey={idempotency_key}; signal=buy; openPosition=present; reason={}",
                    signal.reason
                )),
            },
            format!("{} Live 자동 매수 대기 · 열린 포지션 유지", thread.name),
        );
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::Hold,
            message: "이미 열린 Live 포지션이 있어 추가 매수하지 않고 매도 신호를 기다립니다"
                .to_string(),
            idempotency_key: Some(idempotency_key),
            retry_count: 0,
            paper_result: None,
            live_order_gate: None,
            logs: Vec::new(),
        });
    }

    if live_loop_has_pending_or_filled_order(&logs, &idempotency_key) {
        let retry_count = live_loop_failed_retry_count(&logs, &idempotency_key);
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::DuplicateTick,
            message: "동일한 Live tick 주문이 이미 제출 또는 체결되어 중복 제출을 막았습니다"
                .to_string(),
            idempotency_key: Some(idempotency_key),
            retry_count: retry_count as u32,
            paper_result: None,
            live_order_gate: None,
            logs: Vec::new(),
        });
    }

    let retry_count = live_loop_failed_retry_count(&logs, &idempotency_key);
    if retry_count >= LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK {
        let _ = record_safety_event(
            Some(thread.id),
            SafetyEventDraft {
                event_type: SafetyEventType::Warning,
                category: AuditCategory::ApiFailure,
                source: Some("live_auto_loop".to_string()),
                related_schedule_id: None,
                reason: Some(format!(
                    "idempotencyKey={idempotency_key}; retryCount={retry_count}; retryLimit={LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK}"
                )),
            },
            format!(
                "{} Live 자동 tick retry 제한 도달 · {}회",
                thread.name, retry_count
            ),
        );
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::RetryLimited,
            message: "동일 tick의 Upbit 오류 retry 제한에 도달해 주문을 제출하지 않았습니다"
                .to_string(),
            idempotency_key: Some(idempotency_key),
            retry_count: retry_count as u32,
            paper_result: None,
            live_order_gate: None,
            logs: Vec::new(),
        });
    }

    submit_live_auto_market_buy_with_executor(
        thread,
        signal,
        amount_krw,
        idempotency_key,
        retry_count,
        executor,
    )
    .await
}

async fn submit_live_auto_market_buy_with_executor<E: LiveOrderExecutor>(
    thread: InvestmentThread,
    signal: StrategySignalEvaluation,
    amount_krw: u64,
    idempotency_key: String,
    retry_count: usize,
    executor: &E,
) -> Result<ThreadAutoLoopResult, String> {
    let checked_at = signal.evaluated_at;
    let identifier = live_loop_order_identifier(&idempotency_key, &LiveOrderIntent::MarketBuy);
    let order_request = build_upbit_order_request_with_identifier(
        &thread.market,
        LiveOrderIntent::MarketBuy,
        amount_krw,
        None,
        Some(identifier),
    )?;
    let credentials = get_credentials().map_err(|error| error.to_string());
    let order_chance = match &credentials {
        Ok((access_key, secret_key)) => {
            executor
                .order_chance(access_key, secret_key, &thread.market)
                .await
        }
        Err(error) => Err(error.clone()),
    };
    let chance_error = order_chance.as_ref().err().cloned();
    let gate = evaluate_live_order_gate(
        LiveOrderGateInput::investment_thread(&thread, amount_krw, checked_at).with_order_probe(
            LiveOrderIntent::MarketBuy,
            order_request.preview.clone(),
            order_chance,
        ),
    );

    if !gate.allowed {
        let safety_event_id = record_live_order_gate_block_event(&gate).ok();
        let mut logs = load_logs().map_err(|error| error.to_string())?;
        if !logs
            .iter()
            .any(|log| log.idempotency_key.as_deref() == Some(idempotency_key.as_str()))
        {
            let mut blocked_log = build_live_order_blocked_log(&gate, safety_event_id);
            blocked_log.idempotency_key = Some(idempotency_key.clone());
            blocked_log.strategy_signal_reason = Some(signal.reason.clone());
            logs.push(blocked_log);
            persist_logs(&logs).map_err(|error| error.to_string())?;
        }
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::LiveGateBlocked,
            message: chance_error
                .map(|error| format!("{} · {}", gate.reason, error))
                .unwrap_or_else(|| gate.reason.clone()),
            idempotency_key: Some(idempotency_key),
            retry_count: retry_count as u32,
            paper_result: None,
            live_order_gate: Some(gate),
            logs: Vec::new(),
        });
    }

    let (access_key, secret_key) = credentials?;
    let mut submission =
        prepare_live_market_buy_submission_with_request(&thread, &gate, order_request, checked_at)?;
    submission.idempotency_key = Some(idempotency_key.clone());

    let submitted_event_id =
        record_live_market_buy_submitted_event(&submission).map_err(|error| error.to_string())?;
    let mut submitted_log = build_live_market_buy_submitted_log(&submission);
    submitted_log.safety_event_id = Some(submitted_event_id);
    submitted_log.strategy_signal_reason = Some(signal.reason.clone());

    let mut logs = load_logs().map_err(|error| error.to_string())?;
    logs.push(submitted_log.clone());
    persist_logs(&logs).map_err(|error| error.to_string())?;

    match executor
        .market_buy(&access_key, &secret_key, &submission)
        .await
    {
        Ok(receipt) => {
            let filled_event_id = record_live_market_buy_filled_event(&submission, &receipt)
                .map_err(|error| error.to_string())?;
            let mut filled_log = build_live_market_buy_filled_log(&submission, &receipt);
            filled_log.safety_event_id = Some(filled_event_id);
            filled_log.strategy_signal_reason = Some(signal.reason);
            logs.push(filled_log.clone());
            persist_logs(&logs).map_err(|error| error.to_string())?;
            Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::LiveMarketBuySubmitted,
                message: "Live 자동 tick이 gate 통과 후 시장가 매수를 제출했습니다".to_string(),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32,
                paper_result: None,
                live_order_gate: Some(gate),
                logs: vec![submitted_log, filled_log],
            })
        }
        Err(error) => {
            let failed_event_id = record_live_market_buy_failed_event(&submission, &error).ok();
            let mut failed_log =
                build_live_market_buy_failed_log(&submission, &error, failed_event_id);
            failed_log.strategy_signal_reason = Some(signal.reason);
            logs.push(failed_log.clone());
            persist_logs(&logs).map_err(|persist_error| persist_error.to_string())?;
            Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::Skipped,
                message: format!("Upbit 주문 오류로 retry 대상이 되었습니다: {error}"),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32 + 1,
                paper_result: None,
                live_order_gate: Some(gate),
                logs: vec![submitted_log, failed_log],
            })
        }
    }
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
        .filter(|log| is_successful_buy(log))
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
    let successful_buys = logs.iter().filter(|log| is_successful_buy(log)).count() as u32;
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

    let mut daily: BTreeMap<String, (u64, f64, u64, f64)> = BTreeMap::new();
    for log in successful_logs {
        let date = log.executed_at.date_naive().to_string();
        let unit_price = log.amount_krw as f64 / log.volume_btc;
        let entry = daily.entry(date).or_insert((0, 0.0, 0, unit_price));
        match log.action {
            PurchaseLogAction::MarketSell => {
                entry.1 -= log.volume_btc;
                entry.2 += log.amount_krw;
            }
            PurchaseLogAction::MarketBuy | PurchaseLogAction::SafetyCheck => {
                entry.0 += log.amount_krw;
                entry.1 += log.volume_btc;
            }
        }
        entry.3 = unit_price;
    }

    let mut invested = 0_u64;
    let mut btc_total = 0.0;
    let mut realized_cash = 0_u64;
    let mut peak_value = 0_u64;
    let mut points = Vec::with_capacity(daily.len());

    for (date, (daily_invested, daily_btc, daily_cash, unit_price)) in daily {
        invested += daily_invested;
        btc_total += daily_btc;
        realized_cash += daily_cash;
        let estimated_value = ((btc_total * unit_price).round().max(0.0) as u64) + realized_cash;
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

fn is_successful_buy(log: &PurchaseLog) -> bool {
    matches!(log.status, PurchaseStatus::Success | PurchaseStatus::Filled)
        && matches!(log.action, PurchaseLogAction::MarketBuy)
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

fn validate_live_confirmation_text(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed != REQUIRED_LIVE_CONFIRMATION_PHRASE {
        return Err(format!(
            "최종 확인 문구를 정확히 입력해주세요: {}",
            REQUIRED_LIVE_CONFIRMATION_PHRASE
        ));
    }
    Ok(trimmed.to_string())
}

fn apply_live_confirmation(
    thread: &mut InvestmentThread,
    confirmation_text: String,
    confirmed_at: chrono::DateTime<Utc>,
) {
    thread.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;
    thread.final_confirmation_text = Some(confirmation_text);
    thread.final_confirmed_at = Some(confirmed_at);
}

fn clear_live_confirmation(thread: &mut InvestmentThread) {
    thread.final_confirmation_status = LiveOrderFinalConfirmationStatus::Missing;
    thread.final_confirmation_text = None;
    thread.final_confirmed_at = None;
}

fn thread_has_valid_live_confirmation(thread: &InvestmentThread) -> bool {
    thread.final_confirmation_status == LiveOrderFinalConfirmationStatus::Confirmed
        && thread.final_confirmation_text.as_deref() == Some(REQUIRED_LIVE_CONFIRMATION_PHRASE)
        && thread.final_confirmed_at.is_some()
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
            clear_live_confirmation(&mut incoming);

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
            clear_live_confirmation(&mut incoming);
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
        PurchaseStatus::Submitted => (
            "VitDaily 주문 제출".to_string(),
            format!("{}원 매수 주문 제출 · 체결 대기", log.amount_krw),
        ),
        PurchaseStatus::Filled => (
            "VitDaily 매수 체결".to_string(),
            format!("{}원 매수 체결 · {:.8} BTC", log.amount_krw, log.volume_btc),
        ),
        PurchaseStatus::Failed => (
            "VitDaily 주문 실패".to_string(),
            format!(
                "{}원 주문 실패 · {}",
                log.amount_krw,
                log.error_message.as_deref().unwrap_or("알 수 없는 오류")
            ),
        ),
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

fn build_legacy_schedule_live_policy_status(schedule: &Schedule) -> LegacyScheduleLivePolicyStatus {
    let gate = evaluate_live_order_gate(LiveOrderGateInput::legacy_schedule(schedule));
    LegacyScheduleLivePolicyStatus {
        schedule_id: schedule.id,
        enabled: schedule.enabled,
        time: schedule.time.clone(),
        amount_krw: schedule.amount,
        policy: LegacyScheduleLivePolicy::BlockedUseInvestmentThread,
        live_order_allowed: false,
        live_order_gate: gate,
        title: "레거시 DCA 스케줄 실거래 차단".to_string(),
        description: "기존 스케줄러는 공유 Live Order Gate에서 항상 차단되며, 실거래는 백테스트/최종확인/Upbit 주문가능성 검사를 통과한 투자 스레드 경로만 사용할 수 있습니다.".to_string(),
    }
}

fn reconcile_legacy_schedule_order(schedule: &Schedule) -> PurchaseLog {
    let gate = build_legacy_schedule_live_policy_status(schedule).live_order_gate;
    let safety_event_id = record_live_order_gate_block_event(&gate).ok();

    build_live_order_blocked_log(&gate, safety_event_id)
}

fn build_live_order_blocked_log(
    gate: &LiveOrderGateDecision,
    safety_event_id: Option<uuid::Uuid>,
) -> PurchaseLog {
    let title = match gate.check.source {
        LiveOrderGateSource::LegacySchedule => "레거시 DCA 스케줄 실거래 차단",
        LiveOrderGateSource::InvestmentThread => "투자 스레드 실거래 차단",
    };

    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: gate
            .check
            .related_schedule_id
            .unwrap_or_else(uuid::Uuid::nil),
        thread_id: gate.check.thread_id,
        executed_at: gate.check.checked_at,
        amount_krw: gate.check.amount_krw,
        volume_btc: 0.0,
        status: PurchaseStatus::Blocked,
        error_message: Some(gate.reason.clone()),
        source: purchase_log_source_for_gate(&gate.check.source),
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::SafetyCheck,
        audit_category: AuditCategory::BlockedOrder,
        title: Some(title.to_string()),
        reason: Some(gate.reason.clone()),
        safety_event_id,
        strategy_signal_reason: None,
        idempotency_key: None,
    }
}

fn build_paper_execution_result(
    thread: &InvestmentThread,
    signal: StrategySignalEvaluation,
    live_order_gate: LiveOrderGateDecision,
    existing_logs: &[PurchaseLog],
    amount_krw: u64,
) -> PaperExecutionResult {
    let idempotency_key = paper_idempotency_key(thread, &signal, amount_krw);
    let open_position = paper_open_position(thread, existing_logs);
    let duplicate_log = existing_logs
        .iter()
        .find(|log| log.idempotency_key.as_deref() == Some(idempotency_key.as_str()))
        .cloned();

    if let Some(log) = duplicate_log {
        let position_open = open_position.is_some();
        return PaperExecutionResult {
            thread_id: thread.id,
            signal,
            live_order_gate,
            idempotency_key,
            duplicate: true,
            log: Some(log),
            realized_pnl_krw: None,
            position_open,
            message: "동일한 Paper tick이 이미 기록되어 새 모의 주문을 만들지 않았습니다"
                .to_string(),
        };
    }

    let (log, realized_pnl_krw, position_open, message) = match signal.action {
        PaperSignalAction::Buy => {
            if open_position.is_some() {
                (
                    None,
                    None,
                    true,
                    "이미 열린 Paper 포지션이 있어 추가 모의 매수를 만들지 않았습니다".to_string(),
                )
            } else {
                (
                    Some(build_paper_buy_log(
                        thread,
                        &signal,
                        &live_order_gate,
                        &idempotency_key,
                        amount_krw,
                    )),
                    None,
                    true,
                    "전략 신호가 모의 매수를 생성했고 실제 Upbit 주문은 제출하지 않았습니다"
                        .to_string(),
                )
            }
        }
        PaperSignalAction::Sell => {
            if let Some(position) = open_position {
                let log = build_paper_sell_log(
                    thread,
                    &signal,
                    &live_order_gate,
                    &idempotency_key,
                    &position,
                );
                let realized = log.amount_krw as i64 - position.amount_krw as i64;
                (
                    Some(log),
                    Some(realized),
                    false,
                    format!(
                        "전략 신호가 Paper 포지션을 모의 청산했습니다 · 추정 P/L {}원 · 실제 Upbit 주문 없음",
                        realized
                    ),
                )
            } else {
                (
                    None,
                    None,
                    false,
                    "열린 Paper 포지션이 없어 모의 매도 로그를 생성하지 않았습니다".to_string(),
                )
            }
        }
        PaperSignalAction::Hold => (
            None,
            None,
            open_position.is_some(),
            "전략 신호가 대기 상태라 모의 주문을 생성하지 않았습니다".to_string(),
        ),
    };

    PaperExecutionResult {
        thread_id: thread.id,
        signal,
        live_order_gate,
        idempotency_key,
        duplicate: false,
        log,
        realized_pnl_krw,
        position_open,
        message,
    }
}

fn build_paper_buy_log(
    thread: &InvestmentThread,
    signal: &StrategySignalEvaluation,
    live_order_gate: &LiveOrderGateDecision,
    idempotency_key: &str,
    amount_krw: u64,
) -> PurchaseLog {
    let volume = if signal.price_krw <= 0.0 {
        0.0
    } else {
        (amount_krw as f64 * 0.9995) / signal.price_krw
    };

    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(thread.id),
        executed_at: signal.evaluated_at,
        amount_krw,
        volume_btc: volume,
        status: PurchaseStatus::Success,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Paper,
        action: PurchaseLogAction::MarketBuy,
        audit_category: AuditCategory::PaperTrade,
        title: Some("Paper 모의 매수".to_string()),
        reason: Some(format!("Live Order Gate 확인: {}", live_order_gate.reason)),
        safety_event_id: None,
        strategy_signal_reason: Some(signal.reason.clone()),
        idempotency_key: Some(idempotency_key.to_string()),
    }
}

#[derive(Debug, Clone)]
struct PaperOpenPosition {
    amount_krw: u64,
    volume_btc: f64,
}

#[derive(Debug, Clone)]
struct LiveOpenPosition {
    amount_krw: u64,
    volume_btc: f64,
}

fn paper_open_position(
    thread: &InvestmentThread,
    existing_logs: &[PurchaseLog],
) -> Option<PaperOpenPosition> {
    let mut thread_logs: Vec<&PurchaseLog> = existing_logs
        .iter()
        .filter(|log| {
            log.thread_id == Some(thread.id)
                && log.source == PurchaseLogSource::InvestmentThread
                && log.mode == ExecutionMode::Paper
                && log.status == PurchaseStatus::Success
                && matches!(
                    log.action,
                    PurchaseLogAction::MarketBuy | PurchaseLogAction::MarketSell
                )
        })
        .collect();
    thread_logs.sort_by(|a, b| a.executed_at.cmp(&b.executed_at));

    let mut position: Option<PaperOpenPosition> = None;
    for log in thread_logs {
        match log.action {
            PurchaseLogAction::MarketBuy => {
                position = Some(PaperOpenPosition {
                    amount_krw: log.amount_krw,
                    volume_btc: log.volume_btc,
                });
            }
            PurchaseLogAction::MarketSell => {
                position = None;
            }
            PurchaseLogAction::SafetyCheck => {}
        }
    }
    position
}

fn live_open_position(
    thread: &InvestmentThread,
    existing_logs: &[PurchaseLog],
) -> Option<LiveOpenPosition> {
    let mut thread_logs: Vec<&PurchaseLog> = existing_logs
        .iter()
        .filter(|log| {
            log.thread_id == Some(thread.id)
                && log.source == PurchaseLogSource::InvestmentThread
                && log.mode == ExecutionMode::Live
                && matches!(log.status, PurchaseStatus::Filled | PurchaseStatus::Success)
                && matches!(
                    log.action,
                    PurchaseLogAction::MarketBuy | PurchaseLogAction::MarketSell
                )
        })
        .collect();
    thread_logs.sort_by(|a, b| a.executed_at.cmp(&b.executed_at));

    let mut position: Option<LiveOpenPosition> = None;
    for log in thread_logs {
        match log.action {
            PurchaseLogAction::MarketBuy => {
                if log.volume_btc <= f64::EPSILON {
                    continue;
                }
                if let Some(open) = position.as_mut() {
                    open.amount_krw = open.amount_krw.saturating_add(log.amount_krw);
                    open.volume_btc += log.volume_btc;
                } else {
                    position = Some(LiveOpenPosition {
                        amount_krw: log.amount_krw,
                        volume_btc: log.volume_btc,
                    });
                }
            }
            PurchaseLogAction::MarketSell => {
                let Some(open) = position.as_mut() else {
                    continue;
                };
                if log.volume_btc + f64::EPSILON >= open.volume_btc {
                    position = None;
                } else {
                    let remaining_ratio = (open.volume_btc - log.volume_btc) / open.volume_btc;
                    open.volume_btc -= log.volume_btc;
                    open.amount_krw = ((open.amount_krw as f64) * remaining_ratio).round() as u64;
                }
            }
            PurchaseLogAction::SafetyCheck => {}
        }
    }

    position.filter(|position| position.volume_btc > f64::EPSILON)
}

fn estimated_live_sell_amount_krw(position: &LiveOpenPosition, price_krw: f64) -> u64 {
    if !price_krw.is_finite() || price_krw <= 0.0 {
        return 0;
    }
    (position.volume_btc * price_krw * 0.9995)
        .round()
        .max(0.0) as u64
}

fn format_live_order_volume(volume: f64) -> String {
    let trimmed = format!("{volume:.12}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    if trimmed == "0" || trimmed.is_empty() {
        volume.to_string()
    } else {
        trimmed
    }
}

fn build_paper_sell_log(
    thread: &InvestmentThread,
    signal: &StrategySignalEvaluation,
    live_order_gate: &LiveOrderGateDecision,
    idempotency_key: &str,
    position: &PaperOpenPosition,
) -> PurchaseLog {
    let estimated_amount_krw = (position.volume_btc * signal.price_krw * 0.9995)
        .round()
        .max(0.0) as u64;

    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(thread.id),
        executed_at: signal.evaluated_at,
        amount_krw: estimated_amount_krw,
        volume_btc: position.volume_btc,
        status: PurchaseStatus::Success,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Paper,
        action: PurchaseLogAction::MarketSell,
        audit_category: AuditCategory::PaperTrade,
        title: Some("Paper 모의 매도".to_string()),
        reason: Some(format!("Live Order Gate 확인: {}", live_order_gate.reason)),
        safety_event_id: None,
        strategy_signal_reason: Some(signal.reason.clone()),
        idempotency_key: Some(idempotency_key.to_string()),
    }
}

fn paper_order_amount_krw(thread: &InvestmentThread) -> u64 {
    let days = thread.duration_days.max(1) as u64;
    (thread.initial_budget_krw / days).max(5_000)
}

fn paper_idempotency_key(
    thread: &InvestmentThread,
    signal: &StrategySignalEvaluation,
    amount_krw: u64,
) -> String {
    format!(
        "paper:{}:{}:{:?}:{}",
        thread.id,
        signal.candle_timestamp.timestamp(),
        signal.action,
        amount_krw
    )
}

fn live_loop_idempotency_key(
    thread: &InvestmentThread,
    signal: &StrategySignalEvaluation,
    amount_krw: u64,
) -> String {
    let action_code = match signal.action {
        PaperSignalAction::Buy => "buy",
        PaperSignalAction::Sell => "sell",
        PaperSignalAction::Hold => "hold",
    };
    format!(
        "live:{}:{}:{}:{}",
        thread.id,
        signal.candle_timestamp.timestamp(),
        action_code,
        amount_krw
    )
}

fn live_loop_order_identifier(idempotency_key: &str, intent: &LiveOrderIntent) -> String {
    let intent_code = match intent {
        LiveOrderIntent::MarketBuy => "b",
        LiveOrderIntent::MarketSell => "s",
    };
    format!("vl{intent_code}{}", stable_hex_suffix(idempotency_key, 29))
}

fn stable_hex_suffix(input: &str, width: usize) -> String {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in input.as_bytes() {
        hash ^= *byte as u128;
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    let hex = format!("{hash:032x}");
    hex[hex.len().saturating_sub(width)..].to_string()
}

fn live_loop_failed_retry_count(logs: &[PurchaseLog], idempotency_key: &str) -> usize {
    logs.iter()
        .filter(|log| {
            log.idempotency_key.as_deref() == Some(idempotency_key)
                && log.mode == ExecutionMode::Live
                && log.source == PurchaseLogSource::InvestmentThread
                && log.status == PurchaseStatus::Failed
        })
        .count()
}

fn live_loop_has_pending_or_filled_order(logs: &[PurchaseLog], idempotency_key: &str) -> bool {
    let submitted = logs
        .iter()
        .filter(|log| {
            log.idempotency_key.as_deref() == Some(idempotency_key)
                && log.mode == ExecutionMode::Live
                && log.source == PurchaseLogSource::InvestmentThread
                && log.status == PurchaseStatus::Submitted
        })
        .count();
    let failed = live_loop_failed_retry_count(logs, idempotency_key);
    let filled = logs.iter().any(|log| {
        log.idempotency_key.as_deref() == Some(idempotency_key)
            && log.mode == ExecutionMode::Live
            && log.source == PurchaseLogSource::InvestmentThread
            && log.status == PurchaseStatus::Filled
    });

    filled || submitted > failed
}

#[derive(Debug, Clone)]
struct LiveOrderGateInput {
    source: LiveOrderGateSource,
    thread: Option<InvestmentThread>,
    related_schedule_id: Option<uuid::Uuid>,
    market: SupportedMarket,
    intent: Option<LiveOrderIntent>,
    amount_krw: u64,
    order_preview: Option<UpbitOrderPayloadPreview>,
    order_chance: Option<Result<LiveOrderChance, String>>,
    requested_at: chrono::DateTime<Utc>,
}

impl LiveOrderGateInput {
    fn legacy_schedule(schedule: &Schedule) -> Self {
        Self {
            source: LiveOrderGateSource::LegacySchedule,
            thread: None,
            related_schedule_id: Some(schedule.id),
            market: SupportedMarket::KrwBtc,
            intent: None,
            amount_krw: schedule.amount,
            order_preview: None,
            order_chance: None,
            requested_at: Utc::now(),
        }
    }

    #[allow(dead_code)]
    fn investment_thread(
        thread: &InvestmentThread,
        amount_krw: u64,
        requested_at: chrono::DateTime<Utc>,
    ) -> Self {
        Self {
            source: LiveOrderGateSource::InvestmentThread,
            thread: Some(thread.clone()),
            related_schedule_id: None,
            market: thread.market.clone(),
            intent: None,
            amount_krw,
            order_preview: None,
            order_chance: None,
            requested_at,
        }
    }

    fn with_order_probe(
        mut self,
        intent: LiveOrderIntent,
        order_preview: UpbitOrderPayloadPreview,
        order_chance: Result<LiveOrderChance, String>,
    ) -> Self {
        self.intent = Some(intent);
        self.order_preview = Some(order_preview);
        self.order_chance = Some(order_chance);
        self
    }
}

struct LiveOrderGateData<'a> {
    settings: Result<&'a AppSettings, String>,
    credentials_available: Result<bool, String>,
    logs: Result<&'a [PurchaseLog], String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct LiveOrderGateApproval {
    market: SupportedMarket,
    amount_krw: u64,
}

#[derive(Debug, Clone)]
struct UpbitOrderRequest {
    preview: UpbitOrderPayloadPreview,
    json_body: String,
}

#[derive(Debug, Clone)]
struct LiveMarketBuySubmission {
    approval: LiveOrderGateApproval,
    request: UpbitOrderRequest,
    thread_id: uuid::Uuid,
    gate_reason: String,
    idempotency_key: Option<String>,
    submitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct LiveMarketSellSubmission {
    approval: LiveOrderGateApproval,
    request: UpbitOrderRequest,
    thread_id: uuid::Uuid,
    volume: String,
    policy_reason: Option<String>,
    gate_reason: String,
    idempotency_key: Option<String>,
    submitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct LiveOrderExecutionReceipt {
    upbit_uuid: Option<String>,
    state: String,
    executed_volume: f64,
    executed_funds_krw: Option<u64>,
}

#[derive(Debug, Clone)]
struct LiveOrderChance {
    market: SupportedMarket,
    bid_currency: String,
    bid_balance: f64,
    ask_currency: String,
    ask_balance: f64,
    order_sides: Vec<String>,
    order_types: Vec<String>,
    bid_types: Vec<String>,
    ask_types: Vec<String>,
    bid_min_total_krw: Option<u64>,
    ask_min_total_krw: Option<u64>,
}

trait LiveOrderExecutor {
    fn order_chance<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        market: &'a SupportedMarket,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderChance, String>> + Send + 'a>>;

    fn market_buy<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        submission: &'a LiveMarketBuySubmission,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>;

    fn market_sell<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        submission: &'a LiveMarketSellSubmission,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>;
}

struct UpbitLiveOrderExecutor;

fn evaluate_live_order_gate(input: LiveOrderGateInput) -> LiveOrderGateDecision {
    let settings = load_settings();
    let logs = load_logs();

    evaluate_live_order_gate_with_data(
        input,
        LiveOrderGateData {
            settings: settings.as_ref().map_err(|error| error.to_string()),
            credentials_available: Ok(get_credentials().is_ok()),
            logs: logs
                .as_ref()
                .map(|items| items.as_slice())
                .map_err(|error| error.to_string()),
        },
    )
}

fn evaluate_live_order_gate_with_data(
    input: LiveOrderGateInput,
    data: LiveOrderGateData<'_>,
) -> LiveOrderGateDecision {
    let thread_id = input.thread.as_ref().map(|thread| thread.id);
    let final_confirmation_status = input
        .thread
        .as_ref()
        .map(|thread| thread.final_confirmation_status.clone())
        .unwrap_or_default();
    let daily_trade_cap = input
        .thread
        .as_ref()
        .map(|thread| thread.daily_trade_cap)
        .unwrap_or(DEFAULT_DAILY_TRADE_CAP);

    let mut block_reasons = Vec::new();
    match data.settings {
        Ok(settings) => {
            if settings.global_live_locked {
                block_reasons.push(LiveOrderGateBlockReason::GlobalLiveLocked);
            }
            if !settings.strategy_logic_approved {
                block_reasons.push(LiveOrderGateBlockReason::StrategyLogicNotApproved);
            }
        }
        Err(_) => block_reasons.push(LiveOrderGateBlockReason::SettingsUnavailable),
    }

    match data.credentials_available {
        Ok(true) => {}
        Ok(false) => block_reasons.push(LiveOrderGateBlockReason::CredentialsMissing),
        Err(_) => block_reasons.push(LiveOrderGateBlockReason::CredentialsMissing),
    }

    if !SupportedMarket::all().contains(&input.market) {
        block_reasons.push(LiveOrderGateBlockReason::SupportedMarketRequired);
    }

    let logs = match data.logs {
        Ok(logs) => logs,
        Err(_) => {
            block_reasons.push(LiveOrderGateBlockReason::AuditDataUnavailable);
            &[]
        }
    };
    let daily_trade_count = live_daily_trade_count(
        logs,
        &input.source,
        thread_id,
        input.related_schedule_id,
        input.requested_at,
    );
    if daily_trade_count >= daily_trade_cap {
        block_reasons.push(LiveOrderGateBlockReason::DailyTradeCapExceeded);
    }

    let latest_max_drawdown_percent = None;
    match input.source {
        LiveOrderGateSource::LegacySchedule => {
            block_reasons.push(LiveOrderGateBlockReason::LegacyScheduleNotMigrated);
            block_reasons.push(LiveOrderGateBlockReason::FinalConfirmationMissing);
        }
        LiveOrderGateSource::InvestmentThread => {
            let Some(thread) = input.thread.as_ref() else {
                block_reasons.push(LiveOrderGateBlockReason::LiveModeNotEnabled);
                block_reasons.push(LiveOrderGateBlockReason::FinalConfirmationMissing);
                let check = build_live_order_gate_check(
                    &input,
                    final_confirmation_status,
                    daily_trade_count,
                    daily_trade_cap,
                    None,
                    latest_max_drawdown_percent,
                );
                return live_order_gate_decision(check, block_reasons);
            };

            if !matches!(thread.status, ThreadStatus::Armed | ThreadStatus::Live) {
                block_reasons.push(LiveOrderGateBlockReason::LiveModeNotEnabled);
            }

            if !thread_has_valid_live_confirmation(thread) {
                block_reasons.push(LiveOrderGateBlockReason::FinalConfirmationMissing);
            }
        }
    }

    if input.intent.is_some() {
        match (&input.order_preview, &input.order_chance) {
            (Some(preview), Some(Ok(chance))) => {
                block_reasons.extend(live_order_chance_submission_block_reasons(
                    chance,
                    preview,
                    input.amount_krw,
                ));
            }
            (Some(_), Some(Err(error))) => {
                block_reasons.push(live_order_chance_error_block_reason(error));
            }
            _ => block_reasons.push(LiveOrderGateBlockReason::OrderChanceUnavailable),
        }
    }

    let check = build_live_order_gate_check(
        &input,
        final_confirmation_status,
        daily_trade_count,
        daily_trade_cap,
        input.thread.as_ref().map(|thread| thread.max_loss_percent),
        latest_max_drawdown_percent,
    );

    live_order_gate_decision(check, block_reasons)
}

fn build_live_order_gate_check(
    input: &LiveOrderGateInput,
    final_confirmation_status: LiveOrderFinalConfirmationStatus,
    daily_trade_count: u32,
    daily_trade_cap: u32,
    max_loss_percent: Option<f64>,
    latest_max_drawdown_percent: Option<f64>,
) -> LiveOrderGateCheck {
    LiveOrderGateCheck {
        source: input.source.clone(),
        thread_id: input.thread.as_ref().map(|thread| thread.id),
        related_schedule_id: input.related_schedule_id,
        market: input.market.clone(),
        intent: input.intent.clone(),
        amount_krw: input.amount_krw,
        final_confirmation_status,
        daily_trade_count,
        daily_trade_cap,
        max_loss_percent,
        latest_max_drawdown_percent,
        checked_at: input.requested_at,
    }
}

fn live_order_gate_decision(
    check: LiveOrderGateCheck,
    mut block_reasons: Vec<LiveOrderGateBlockReason>,
) -> LiveOrderGateDecision {
    block_reasons.sort_by_key(|reason| live_order_block_reason_rank(reason));
    block_reasons.dedup();
    let allowed = block_reasons.is_empty();
    let reason = if allowed {
        "공유 Live Order Gate를 통과했습니다".to_string()
    } else {
        block_reasons
            .iter()
            .map(live_order_block_reason_text)
            .collect::<Vec<_>>()
            .join(" · ")
    };

    LiveOrderGateDecision {
        allowed,
        check,
        block_reasons,
        reason,
    }
}

fn live_order_block_reason_rank(reason: &LiveOrderGateBlockReason) -> u8 {
    match reason {
        LiveOrderGateBlockReason::SettingsUnavailable => 0,
        LiveOrderGateBlockReason::GlobalLiveLocked => 1,
        LiveOrderGateBlockReason::CredentialsMissing => 2,
        LiveOrderGateBlockReason::InvalidApiKey => 3,
        LiveOrderGateBlockReason::RevokedApiKey => 4,
        LiveOrderGateBlockReason::StrategyLogicNotApproved => 5,
        LiveOrderGateBlockReason::LegacyScheduleNotMigrated => 6,
        LiveOrderGateBlockReason::LiveModeNotEnabled => 7,
        LiveOrderGateBlockReason::FinalConfirmationMissing => 8,
        LiveOrderGateBlockReason::ValidationMissing => 9,
        LiveOrderGateBlockReason::ValidationNotPassed => 10,
        LiveOrderGateBlockReason::MaxLossExceeded => 11,
        LiveOrderGateBlockReason::DailyTradeCapExceeded => 12,
        LiveOrderGateBlockReason::SupportedMarketRequired => 13,
        LiveOrderGateBlockReason::OrderPermissionDenied => 14,
        LiveOrderGateBlockReason::OrderChanceUnavailable => 15,
        LiveOrderGateBlockReason::MarketOrderUnavailable => 16,
        LiveOrderGateBlockReason::MinimumOrderAmountNotMet => 17,
        LiveOrderGateBlockReason::InsufficientBalance => 18,
        LiveOrderGateBlockReason::AuditDataUnavailable => 19,
    }
}

fn live_order_block_reason_text(reason: &LiveOrderGateBlockReason) -> &'static str {
    match reason {
        LiveOrderGateBlockReason::GlobalLiveLocked => {
            "Global Live Lock이 잠겨 있어 실주문이 차단되었습니다"
        }
        LiveOrderGateBlockReason::CredentialsMissing => "Upbit API 키 확인이 필요합니다",
        LiveOrderGateBlockReason::InvalidApiKey => {
            "Upbit API 키가 유효하지 않아 실거래 readiness를 해제했습니다"
        }
        LiveOrderGateBlockReason::RevokedApiKey => {
            "Upbit API 키가 만료/폐기되어 실거래 readiness를 해제했습니다"
        }
        LiveOrderGateBlockReason::StrategyLogicNotApproved => "전략 로직 실거래 승인이 필요합니다",
        LiveOrderGateBlockReason::FinalConfirmationMissing => "최종 확인이 필요합니다",
        LiveOrderGateBlockReason::LiveModeNotEnabled => "스레드가 실거래 상태가 아닙니다",
        LiveOrderGateBlockReason::DailyTradeCapExceeded => "일일 거래 한도에 도달했습니다",
        LiveOrderGateBlockReason::MaxLossExceeded => "최대 손실률 기준을 초과했습니다",
        LiveOrderGateBlockReason::SupportedMarketRequired => "지원하지 않는 마켓입니다",
        LiveOrderGateBlockReason::ValidationMissing => "통과한 백테스트 검증 결과가 없습니다",
        LiveOrderGateBlockReason::ValidationNotPassed => "백테스트 검증 상태가 통과가 아닙니다",
        LiveOrderGateBlockReason::LegacyScheduleNotMigrated => {
            "레거시 DCA 스케줄은 아직 공유 Live Order Gate로 마이그레이션되지 않았습니다"
        }
        LiveOrderGateBlockReason::SettingsUnavailable => {
            "설정 로드 실패로 안전 기본값을 적용했습니다"
        }
        LiveOrderGateBlockReason::AuditDataUnavailable => {
            "거래/검증 감사 데이터 로드 실패로 안전 기본값을 적용했습니다"
        }
        LiveOrderGateBlockReason::InsufficientBalance => {
            "Upbit 사용 가능 잔고가 주문 금액 또는 수량보다 부족합니다"
        }
        LiveOrderGateBlockReason::MinimumOrderAmountNotMet => {
            "Upbit 최소 주문금액 기준을 충족하지 못했습니다"
        }
        LiveOrderGateBlockReason::MarketOrderUnavailable => {
            "해당 마켓에서 요청한 시장가 주문 유형을 지원하지 않습니다"
        }
        LiveOrderGateBlockReason::OrderPermissionDenied => {
            "Upbit 주문 권한 또는 주문 가능 정보 조회 권한 확인에 실패했습니다"
        }
        LiveOrderGateBlockReason::OrderChanceUnavailable => {
            "Upbit 주문 가능 정보 조회 결과를 사용할 수 없어 안전 기본값을 적용했습니다"
        }
    }
}

fn live_daily_trade_count(
    logs: &[PurchaseLog],
    source: &LiveOrderGateSource,
    thread_id: Option<uuid::Uuid>,
    related_schedule_id: Option<uuid::Uuid>,
    requested_at: chrono::DateTime<Utc>,
) -> u32 {
    let requested_day = requested_at.with_timezone(&Local).date_naive();
    logs.iter()
        .filter(|log| {
            matches!(log.status, PurchaseStatus::Success | PurchaseStatus::Filled)
                && log.mode == ExecutionMode::Live
                && matches!(
                    log.action,
                    PurchaseLogAction::MarketBuy | PurchaseLogAction::MarketSell
                )
                && log.source == purchase_log_source_for_gate(source)
                && log.executed_at.with_timezone(&Local).date_naive() == requested_day
        })
        .filter(|log| match source {
            LiveOrderGateSource::InvestmentThread => thread_id == log.thread_id,
            LiveOrderGateSource::LegacySchedule => related_schedule_id == Some(log.schedule_id),
        })
        .count() as u32
}

#[allow(dead_code)]
fn live_order_approval_from_gate(gate: &LiveOrderGateDecision) -> Option<LiveOrderGateApproval> {
    gate.allowed.then(|| LiveOrderGateApproval {
        market: gate.check.market.clone(),
        amount_krw: gate.check.amount_krw,
    })
}

fn persist_live_order_block(gate: &LiveOrderGateDecision) -> Result<(), String> {
    let safety_event_id = record_live_order_gate_block_event(gate).ok();
    let mut logs = load_logs().map_err(|e| e.to_string())?;
    logs.push(build_live_order_blocked_log(gate, safety_event_id));
    persist_logs(&logs).map_err(|e| e.to_string())
}

fn live_order_chance_submission_block_reasons(
    chance: &LiveOrderChance,
    preview: &UpbitOrderPayloadPreview,
    amount_krw: u64,
) -> Vec<LiveOrderGateBlockReason> {
    let mut block_reasons = Vec::new();
    if chance.market != preview.market
        || !chance.order_sides.iter().any(|side| side == &preview.side)
        || !chance.supports_order_type(&preview.side, &preview.ord_type)
    {
        block_reasons.push(LiveOrderGateBlockReason::MarketOrderUnavailable);
    }

    let min_total = if preview.side == "bid" {
        chance.bid_min_total_krw
    } else {
        chance.ask_min_total_krw
    };
    if let Some(min_total) = min_total {
        if amount_krw < min_total {
            block_reasons.push(LiveOrderGateBlockReason::MinimumOrderAmountNotMet);
        }
    } else {
        block_reasons.push(LiveOrderGateBlockReason::OrderChanceUnavailable);
    }

    if preview.side == "bid" {
        if chance.bid_balance < amount_krw as f64 {
            block_reasons.push(LiveOrderGateBlockReason::InsufficientBalance);
        }
    } else {
        let requested_volume = preview
            .volume
            .as_deref()
            .and_then(|volume| volume.parse::<f64>().ok())
            .unwrap_or(0.0);
        if requested_volume <= 0.0 || chance.ask_balance < requested_volume {
            block_reasons.push(LiveOrderGateBlockReason::InsufficientBalance);
        }
    }

    block_reasons
}

fn live_order_chance_error_block_reason(error: &str) -> LiveOrderGateBlockReason {
    live_order_block_reason_from_credential_readiness(&credential_readiness_from_error(error))
}

fn live_order_block_reason_from_credential_readiness(
    readiness: &CredentialReadinessStatus,
) -> LiveOrderGateBlockReason {
    match readiness {
        CredentialReadinessStatus::Missing => LiveOrderGateBlockReason::CredentialsMissing,
        CredentialReadinessStatus::InvalidKey => LiveOrderGateBlockReason::InvalidApiKey,
        CredentialReadinessStatus::RevokedKey => LiveOrderGateBlockReason::RevokedApiKey,
        CredentialReadinessStatus::OrderPermissionMissing => {
            LiveOrderGateBlockReason::OrderPermissionDenied
        }
        CredentialReadinessStatus::NetworkError
        | CredentialReadinessStatus::StoredUnchecked
        | CredentialReadinessStatus::Connected => LiveOrderGateBlockReason::OrderChanceUnavailable,
    }
}

fn credential_readiness_from_error(error: &str) -> CredentialReadinessStatus {
    let lower = error.to_ascii_lowercase();
    if lower.contains("revoked")
        || lower.contains("revoke")
        || lower.contains("expired")
        || lower.contains("disabled")
        || lower.contains("deleted")
        || lower.contains("suspended")
        || lower.contains("폐기")
        || lower.contains("만료")
    {
        CredentialReadinessStatus::RevokedKey
    } else if lower.contains("invalid")
        || lower.contains("jwt")
        || lower.contains("signature")
        || lower.contains("verification")
        || lower.contains("access_key")
        || lower.contains("secret")
        || lower.contains("유효하지")
    {
        CredentialReadinessStatus::InvalidKey
    } else if lower.contains("out_of_scope")
        || lower.contains("no_authorization")
        || lower.contains("permission")
        || lower.contains("권한")
        || lower.contains("create_bid")
        || lower.contains("create_ask")
        || lower.contains("403")
    {
        CredentialReadinessStatus::OrderPermissionMissing
    } else {
        CredentialReadinessStatus::NetworkError
    }
}

fn live_order_chance_settings_block_reasons(
    chance: &LiveOrderChance,
) -> Vec<LiveOrderGateBlockReason> {
    let mut block_reasons = Vec::new();
    if !chance.order_sides.iter().any(|side| side == "bid")
        || !chance.supports_order_type("bid", "price")
        || !chance.order_sides.iter().any(|side| side == "ask")
        || !chance.supports_order_type("ask", "market")
    {
        block_reasons.push(LiveOrderGateBlockReason::MarketOrderUnavailable);
    }
    if chance
        .bid_min_total_krw
        .map(|min_total| DEFAULT_LIVE_CHANCE_PROBE_AMOUNT_KRW < min_total)
        .unwrap_or(true)
        || chance.ask_min_total_krw.is_none()
    {
        block_reasons.push(LiveOrderGateBlockReason::MinimumOrderAmountNotMet);
    }
    if chance.bid_balance < DEFAULT_LIVE_CHANCE_PROBE_AMOUNT_KRW as f64 || chance.ask_balance <= 0.0
    {
        block_reasons.push(LiveOrderGateBlockReason::InsufficientBalance);
    }
    block_reasons
}

fn build_live_order_chance_status(
    market: &SupportedMarket,
    chance: Option<&LiveOrderChance>,
    block_reasons: Vec<LiveOrderGateBlockReason>,
    credential_readiness: CredentialReadinessStatus,
    detail: Option<String>,
    checked_at: chrono::DateTime<Utc>,
) -> LiveOrderChanceStatus {
    let allowed = block_reasons.is_empty();
    let reason = if allowed {
        "Upbit /orders/chance 주문 가능성 확인을 통과했습니다".to_string()
    } else {
        let base = block_reasons
            .iter()
            .map(live_order_block_reason_text)
            .collect::<Vec<_>>()
            .join(" · ");
        match detail {
            Some(detail) if !detail.trim().is_empty() => format!("{base} · {detail}"),
            _ => base,
        }
    };

    LiveOrderChanceStatus {
        allowed,
        market: market.clone(),
        bid_currency: chance
            .map(|item| item.bid_currency.clone())
            .unwrap_or_else(|| "KRW".to_string()),
        bid_balance: chance.map(|item| item.bid_balance).unwrap_or(0.0),
        ask_currency: chance
            .map(|item| item.ask_currency.clone())
            .unwrap_or_else(|| market_base_currency(market).to_string()),
        ask_balance: chance.map(|item| item.ask_balance).unwrap_or(0.0),
        minimum_bid_total_krw: chance.and_then(|item| item.bid_min_total_krw),
        minimum_ask_total_krw: chance.and_then(|item| item.ask_min_total_krw),
        market_buy_supported: chance
            .map(|item| {
                item.order_sides.iter().any(|side| side == "bid")
                    && item.supports_order_type("bid", "price")
            })
            .unwrap_or(false),
        market_sell_supported: chance
            .map(|item| {
                item.order_sides.iter().any(|side| side == "ask")
                    && item.supports_order_type("ask", "market")
            })
            .unwrap_or(false),
        credential_readiness,
        block_reasons,
        reason,
        checked_at,
    }
}

async fn submit_thread_live_market_buy_with_executor<E: LiveOrderExecutor>(
    thread_id: String,
    amount_krw: Option<u64>,
    executor: &E,
) -> Result<Vec<PurchaseLog>, String> {
    let uuid = thread_id
        .parse::<uuid::Uuid>()
        .map_err(|_| "잘못된 스레드 ID".to_string())?;
    let threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == uuid)
        .cloned()
        .ok_or_else(|| "시장가 매수를 제출할 스레드를 찾을 수 없습니다".to_string())?;
    let order_amount = amount_krw.unwrap_or_else(|| paper_order_amount_krw(&thread));
    let checked_at = Utc::now();
    let order_request = build_upbit_order_request(
        &thread.market,
        LiveOrderIntent::MarketBuy,
        order_amount,
        None,
    )?;
    let credentials = get_credentials().map_err(|error| error.to_string());
    let order_chance = match &credentials {
        Ok((access_key, secret_key)) => {
            executor
                .order_chance(access_key, secret_key, &thread.market)
                .await
        }
        Err(error) => Err(error.clone()),
    };
    let chance_error = order_chance.as_ref().err().cloned();
    let gate = evaluate_live_order_gate(
        LiveOrderGateInput::investment_thread(&thread, order_amount, checked_at).with_order_probe(
            LiveOrderIntent::MarketBuy,
            order_request.preview.clone(),
            order_chance,
        ),
    );

    if !gate.allowed {
        persist_live_order_block(&gate)?;
        return Err(match chance_error {
            Some(error) => format!("{} · {}", gate.reason, error),
            None => gate.reason,
        });
    }

    let (access_key, secret_key) = credentials?;
    let submission =
        prepare_live_market_buy_submission_with_request(&thread, &gate, order_request, checked_at)?;

    let submitted_event_id =
        record_live_market_buy_submitted_event(&submission).map_err(|error| error.to_string())?;
    let mut submitted_log = build_live_market_buy_submitted_log(&submission);
    submitted_log.safety_event_id = Some(submitted_event_id);

    let mut logs = load_logs().map_err(|e| e.to_string())?;
    logs.push(submitted_log.clone());
    persist_logs(&logs).map_err(|e| e.to_string())?;

    match executor
        .market_buy(&access_key, &secret_key, &submission)
        .await
    {
        Ok(receipt) => {
            let filled_event_id = record_live_market_buy_filled_event(&submission, &receipt)
                .map_err(|error| error.to_string())?;
            let mut filled_log = build_live_market_buy_filled_log(&submission, &receipt);
            filled_log.safety_event_id = Some(filled_event_id);
            logs.push(filled_log.clone());
            persist_logs(&logs).map_err(|e| e.to_string())?;
            Ok(vec![submitted_log, filled_log])
        }
        Err(error) => {
            let failed_event_id = record_live_market_buy_failed_event(&submission, &error).ok();
            let failed_log = build_live_market_buy_failed_log(&submission, &error, failed_event_id);
            logs.push(failed_log);
            persist_logs(&logs).map_err(|e| e.to_string())?;
            Err(error)
        }
    }
}

async fn submit_live_auto_market_sell_with_executor<E: LiveOrderExecutor>(
    thread: InvestmentThread,
    signal: StrategySignalEvaluation,
    position: LiveOpenPosition,
    estimated_amount_krw: u64,
    idempotency_key: String,
    retry_count: usize,
    executor: &E,
) -> Result<ThreadAutoLoopResult, String> {
    let checked_at = signal.evaluated_at;
    let volume = format_live_order_volume(position.volume_btc);
    let identifier = live_loop_order_identifier(&idempotency_key, &LiveOrderIntent::MarketSell);
    let order_request = build_upbit_order_request_with_identifier(
        &thread.market,
        LiveOrderIntent::MarketSell,
        estimated_amount_krw,
        Some(volume.clone()),
        Some(identifier),
    )?;
    let credentials = get_credentials().map_err(|error| error.to_string());
    let order_chance = match &credentials {
        Ok((access_key, secret_key)) => {
            executor
                .order_chance(access_key, secret_key, &thread.market)
                .await
        }
        Err(error) => Err(error.clone()),
    };
    let chance_error = order_chance.as_ref().err().cloned();
    let gate = evaluate_live_order_gate(
        LiveOrderGateInput::investment_thread(&thread, estimated_amount_krw, checked_at)
            .with_order_probe(
                LiveOrderIntent::MarketSell,
                order_request.preview.clone(),
                order_chance,
            ),
    );

    if !gate.allowed {
        let safety_event_id = record_live_order_gate_block_event(&gate).ok();
        let mut logs = load_logs().map_err(|error| error.to_string())?;
        if !logs
            .iter()
            .any(|log| log.idempotency_key.as_deref() == Some(idempotency_key.as_str()))
        {
            let mut blocked_log = build_live_order_blocked_log(&gate, safety_event_id);
            blocked_log.idempotency_key = Some(idempotency_key.clone());
            blocked_log.strategy_signal_reason = Some(signal.reason.clone());
            logs.push(blocked_log);
            persist_logs(&logs).map_err(|error| error.to_string())?;
        }
        return Ok(ThreadAutoLoopResult {
            thread_id: thread.id,
            mode: ThreadAutoLoopMode::Live,
            action: ThreadAutoLoopAction::LiveGateBlocked,
            message: chance_error
                .map(|error| format!("{} · {}", gate.reason, error))
                .unwrap_or_else(|| gate.reason.clone()),
            idempotency_key: Some(idempotency_key),
            retry_count: retry_count as u32,
            paper_result: None,
            live_order_gate: Some(gate),
            logs: Vec::new(),
        });
    }

    let (access_key, secret_key) = credentials?;
    let request = LiveMarketSellRequest {
        thread_id: thread.id,
        volume,
        estimated_amount_krw: Some(estimated_amount_krw),
        policy_reason: Some("live_auto_loop_strategy_signal_sell".to_string()),
    };
    let mut submission = prepare_live_market_sell_submission_with_request(
        &thread,
        request,
        &gate,
        order_request,
        checked_at,
    )?;
    submission.idempotency_key = Some(idempotency_key.clone());

    let submitted_event_id =
        record_live_market_sell_submitted_event(&submission).map_err(|error| error.to_string())?;
    let mut submitted_log = build_live_market_sell_submitted_log(&submission);
    submitted_log.safety_event_id = Some(submitted_event_id);
    submitted_log.strategy_signal_reason = Some(signal.reason.clone());

    let mut logs = load_logs().map_err(|error| error.to_string())?;
    logs.push(submitted_log.clone());
    persist_logs(&logs).map_err(|error| error.to_string())?;

    match executor
        .market_sell(&access_key, &secret_key, &submission)
        .await
    {
        Ok(receipt) => {
            let filled_event_id = record_live_market_sell_filled_event(&submission, &receipt)
                .map_err(|error| error.to_string())?;
            let mut filled_log = build_live_market_sell_filled_log(&submission, &receipt);
            filled_log.safety_event_id = Some(filled_event_id);
            filled_log.strategy_signal_reason = Some(signal.reason);
            logs.push(filled_log.clone());
            persist_logs(&logs).map_err(|error| error.to_string())?;
            Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::LiveMarketSellSubmitted,
                message: "Live 자동 tick이 열린 포지션 수량으로 시장가 매도를 제출했습니다"
                    .to_string(),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32,
                paper_result: None,
                live_order_gate: Some(gate),
                logs: vec![submitted_log, filled_log],
            })
        }
        Err(error) => {
            let failed_event_id = record_live_market_sell_failed_event(&submission, &error).ok();
            let mut failed_log =
                build_live_market_sell_failed_log(&submission, &error, failed_event_id);
            failed_log.strategy_signal_reason = Some(signal.reason);
            logs.push(failed_log.clone());
            persist_logs(&logs).map_err(|persist_error| persist_error.to_string())?;
            Ok(ThreadAutoLoopResult {
                thread_id: thread.id,
                mode: ThreadAutoLoopMode::Live,
                action: ThreadAutoLoopAction::Skipped,
                message: format!("Upbit 매도 주문 오류로 retry 대상이 되었습니다: {error}"),
                idempotency_key: Some(idempotency_key),
                retry_count: retry_count as u32 + 1,
                paper_result: None,
                live_order_gate: Some(gate),
                logs: vec![submitted_log, failed_log],
            })
        }
    }
}

async fn submit_thread_live_market_sell_with_executor<E: LiveOrderExecutor>(
    request: LiveMarketSellRequest,
    executor: &E,
) -> Result<Vec<PurchaseLog>, String> {
    let threads = load_investment_threads().map_err(|e| e.to_string())?;
    let thread = threads
        .iter()
        .find(|thread| thread.id == request.thread_id)
        .cloned()
        .ok_or_else(|| "시장가 매도를 제출할 스레드를 찾을 수 없습니다".to_string())?;
    let order_amount = request
        .estimated_amount_krw
        .unwrap_or(DEFAULT_LIVE_SELL_GATE_AMOUNT_KRW)
        .max(DEFAULT_LIVE_SELL_GATE_AMOUNT_KRW);
    let checked_at = Utc::now();
    let order_request = build_upbit_order_request(
        &thread.market,
        LiveOrderIntent::MarketSell,
        order_amount,
        Some(request.volume.trim().to_string()),
    )?;
    let credentials = get_credentials().map_err(|error| error.to_string());
    let order_chance = match &credentials {
        Ok((access_key, secret_key)) => {
            executor
                .order_chance(access_key, secret_key, &thread.market)
                .await
        }
        Err(error) => Err(error.clone()),
    };
    let chance_error = order_chance.as_ref().err().cloned();
    let gate = evaluate_live_order_gate(
        LiveOrderGateInput::investment_thread(&thread, order_amount, checked_at).with_order_probe(
            LiveOrderIntent::MarketSell,
            order_request.preview.clone(),
            order_chance,
        ),
    );

    if !gate.allowed {
        persist_live_order_block(&gate)?;
        return Err(match chance_error {
            Some(error) => format!("{} · {}", gate.reason, error),
            None => gate.reason,
        });
    }

    let (access_key, secret_key) = credentials?;
    let submission = prepare_live_market_sell_submission_with_request(
        &thread,
        request,
        &gate,
        order_request,
        checked_at,
    )?;

    let submitted_event_id =
        record_live_market_sell_submitted_event(&submission).map_err(|error| error.to_string())?;
    let mut submitted_log = build_live_market_sell_submitted_log(&submission);
    submitted_log.safety_event_id = Some(submitted_event_id);

    let mut logs = load_logs().map_err(|e| e.to_string())?;
    logs.push(submitted_log.clone());
    persist_logs(&logs).map_err(|e| e.to_string())?;

    match executor
        .market_sell(&access_key, &secret_key, &submission)
        .await
    {
        Ok(receipt) => {
            let filled_event_id = record_live_market_sell_filled_event(&submission, &receipt)
                .map_err(|error| error.to_string())?;
            let mut filled_log = build_live_market_sell_filled_log(&submission, &receipt);
            filled_log.safety_event_id = Some(filled_event_id);
            logs.push(filled_log.clone());
            persist_logs(&logs).map_err(|e| e.to_string())?;
            Ok(vec![submitted_log, filled_log])
        }
        Err(error) => {
            let failed_event_id = record_live_market_sell_failed_event(&submission, &error).ok();
            let failed_log =
                build_live_market_sell_failed_log(&submission, &error, failed_event_id);
            logs.push(failed_log);
            persist_logs(&logs).map_err(|e| e.to_string())?;
            Err(error)
        }
    }
}

#[cfg(test)]
fn prepare_live_market_buy_submission(
    thread: &InvestmentThread,
    gate: &LiveOrderGateDecision,
    submitted_at: chrono::DateTime<Utc>,
) -> Result<LiveMarketBuySubmission, String> {
    let request = build_upbit_order_request(
        &thread.market,
        LiveOrderIntent::MarketBuy,
        gate.check.amount_krw,
        None,
    )?;
    prepare_live_market_buy_submission_with_request(thread, gate, request, submitted_at)
}

fn prepare_live_market_buy_submission_with_request(
    thread: &InvestmentThread,
    gate: &LiveOrderGateDecision,
    request: UpbitOrderRequest,
    submitted_at: chrono::DateTime<Utc>,
) -> Result<LiveMarketBuySubmission, String> {
    let approval = live_order_approval_from_gate(gate).ok_or_else(|| gate.reason.clone())?;
    Ok(LiveMarketBuySubmission {
        approval,
        request,
        thread_id: thread.id,
        gate_reason: gate.reason.clone(),
        idempotency_key: None,
        submitted_at,
    })
}

#[cfg(test)]
fn prepare_live_market_sell_submission(
    thread: &InvestmentThread,
    request: LiveMarketSellRequest,
    gate: &LiveOrderGateDecision,
    submitted_at: chrono::DateTime<Utc>,
) -> Result<LiveMarketSellSubmission, String> {
    let volume = request.volume.trim().to_string();
    let upbit_request = build_upbit_order_request(
        &thread.market,
        LiveOrderIntent::MarketSell,
        gate.check.amount_krw,
        Some(volume.clone()),
    )?;
    prepare_live_market_sell_submission_with_request(
        thread,
        request,
        gate,
        upbit_request,
        submitted_at,
    )
}

fn prepare_live_market_sell_submission_with_request(
    thread: &InvestmentThread,
    request: LiveMarketSellRequest,
    gate: &LiveOrderGateDecision,
    upbit_request: UpbitOrderRequest,
    submitted_at: chrono::DateTime<Utc>,
) -> Result<LiveMarketSellSubmission, String> {
    let approval = live_order_approval_from_gate(gate).ok_or_else(|| gate.reason.clone())?;
    let volume = request.volume.trim().to_string();
    Ok(LiveMarketSellSubmission {
        approval,
        request: upbit_request,
        thread_id: thread.id,
        volume,
        policy_reason: request
            .policy_reason
            .filter(|reason| !reason.trim().is_empty()),
        gate_reason: gate.reason.clone(),
        idempotency_key: None,
        submitted_at,
    })
}

fn build_upbit_order_request(
    market: &SupportedMarket,
    intent: LiveOrderIntent,
    amount_krw: u64,
    volume: Option<String>,
) -> Result<UpbitOrderRequest, String> {
    build_upbit_order_request_with_identifier(market, intent, amount_krw, volume, None)
}

fn build_upbit_order_request_with_identifier(
    market: &SupportedMarket,
    intent: LiveOrderIntent,
    amount_krw: u64,
    volume: Option<String>,
    identifier: Option<String>,
) -> Result<UpbitOrderRequest, String> {
    let identifier = identifier.unwrap_or_else(|| live_order_identifier(&intent));
    if identifier.is_empty() || identifier.len() > 32 {
        return Err("Upbit 주문 identifier는 1~32자여야 합니다".to_string());
    }
    match intent {
        LiveOrderIntent::MarketBuy => {
            if amount_krw == 0 {
                return Err("시장가 매수 금액은 0원보다 커야 합니다".to_string());
            }
            let price = amount_krw.to_string();
            let side = "bid".to_string();
            let ord_type = "price".to_string();
            let params = vec![
                ("market", market.as_upbit_market().to_string()),
                ("side", side.clone()),
                ("ord_type", ord_type.clone()),
                ("price", price.clone()),
                ("identifier", identifier.clone()),
            ];
            let query_string = upbit_order_query_string(&params);
            let json_body = upbit_order_json_body(&params)?;
            let preview = UpbitOrderPayloadPreview {
                market: market.clone(),
                side: side.clone(),
                ord_type: ord_type.clone(),
                price: Some(price),
                volume: None,
                identifier: identifier.clone(),
                query_string,
            };
            Ok(UpbitOrderRequest { json_body, preview })
        }
        LiveOrderIntent::MarketSell => {
            let volume = volume
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "시장가 매도에는 volume이 필요합니다".to_string())?;
            let parsed_volume = volume
                .parse::<f64>()
                .map_err(|_| "시장가 매도 volume은 숫자여야 합니다".to_string())?;
            if !parsed_volume.is_finite() || parsed_volume <= 0.0 {
                return Err("시장가 매도 volume은 0보다 커야 합니다".to_string());
            }
            let volume = parsed_volume.to_string();
            let side = "ask".to_string();
            let ord_type = "market".to_string();
            let params = vec![
                ("market", market.as_upbit_market().to_string()),
                ("side", side.clone()),
                ("ord_type", ord_type.clone()),
                ("volume", volume.clone()),
                ("identifier", identifier.clone()),
            ];
            let query_string = upbit_order_query_string(&params);
            let json_body = upbit_order_json_body(&params)?;
            let preview = UpbitOrderPayloadPreview {
                market: market.clone(),
                side: side.clone(),
                ord_type: ord_type.clone(),
                price: None,
                volume: Some(volume.clone()),
                identifier: identifier.clone(),
                query_string,
            };
            Ok(UpbitOrderRequest { json_body, preview })
        }
    }
}

fn upbit_order_query_string(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn upbit_order_json_body(params: &[(&str, String)]) -> Result<String, String> {
    let fields = params
        .iter()
        .map(|(key, value)| {
            let key = serde_json::to_string(key).map_err(|error| error.to_string())?;
            let value = serde_json::to_string(value).map_err(|error| error.to_string())?;
            Ok(format!("{key}:{value}"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(format!("{{{}}}", fields.join(",")))
}

fn build_upbit_order_payload_preview(
    market: &SupportedMarket,
    intent: LiveOrderIntent,
    amount_krw: u64,
    volume: Option<String>,
) -> Result<UpbitOrderPayloadPreview, String> {
    Ok(build_upbit_order_request(market, intent, amount_krw, volume)?.preview)
}

fn build_live_market_buy_submitted_log(submission: &LiveMarketBuySubmission) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: submission.submitted_at,
        amount_krw: submission.approval.amount_krw,
        volume_btc: 0.0,
        status: PurchaseStatus::Submitted,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketBuy,
        audit_category: AuditCategory::Trade,
        title: Some("Live 시장가 매수 제출".to_string()),
        reason: Some(format!(
            "Live Order Gate 승인 후 Upbit 주문 제출: {}",
            submission.gate_reason
        )),
        safety_event_id: None,
        strategy_signal_reason: None,
        idempotency_key: Some(live_market_buy_log_idempotency_key(submission)),
    }
}

fn build_live_market_buy_filled_log(
    submission: &LiveMarketBuySubmission,
    receipt: &LiveOrderExecutionReceipt,
) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: Utc::now(),
        amount_krw: submission.approval.amount_krw,
        volume_btc: receipt.executed_volume,
        status: PurchaseStatus::Filled,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketBuy,
        audit_category: AuditCategory::Trade,
        title: Some("Live 시장가 매수 체결".to_string()),
        reason: Some(format!(
            "Upbit 주문 응답 state={} uuid={}",
            receipt.state,
            receipt.upbit_uuid.as_deref().unwrap_or("unknown")
        )),
        safety_event_id: None,
        strategy_signal_reason: None,
        idempotency_key: Some(live_market_buy_log_idempotency_key(submission)),
    }
}

fn build_live_market_buy_failed_log(
    submission: &LiveMarketBuySubmission,
    error: &str,
    safety_event_id: Option<uuid::Uuid>,
) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: Utc::now(),
        amount_krw: submission.approval.amount_krw,
        volume_btc: 0.0,
        status: PurchaseStatus::Failed,
        error_message: Some(error.to_string()),
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketBuy,
        audit_category: AuditCategory::ApiFailure,
        title: Some("Live 시장가 매수 실패".to_string()),
        reason: Some("Upbit 주문 제출 또는 응답 처리 실패".to_string()),
        safety_event_id,
        strategy_signal_reason: None,
        idempotency_key: Some(live_market_buy_log_idempotency_key(submission)),
    }
}

fn build_live_market_sell_submitted_log(submission: &LiveMarketSellSubmission) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: submission.submitted_at,
        amount_krw: submission.approval.amount_krw,
        volume_btc: submission.volume.parse::<f64>().unwrap_or(0.0),
        status: PurchaseStatus::Submitted,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketSell,
        audit_category: AuditCategory::Trade,
        title: Some("Live 시장가 매도 제출".to_string()),
        reason: Some(format!(
            "Live Order Gate 승인 후 Upbit 매도 주문 제출: {}; policy={}",
            submission.gate_reason,
            submission.policy_reason.as_deref().unwrap_or("unspecified")
        )),
        safety_event_id: None,
        strategy_signal_reason: submission.policy_reason.clone(),
        idempotency_key: Some(live_market_sell_log_idempotency_key(submission)),
    }
}

fn build_live_market_sell_filled_log(
    submission: &LiveMarketSellSubmission,
    receipt: &LiveOrderExecutionReceipt,
) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: Utc::now(),
        amount_krw: receipt
            .executed_funds_krw
            .unwrap_or(submission.approval.amount_krw),
        volume_btc: receipt.executed_volume,
        status: PurchaseStatus::Filled,
        error_message: None,
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketSell,
        audit_category: AuditCategory::Trade,
        title: Some("Live 시장가 매도 체결".to_string()),
        reason: Some(format!(
            "Upbit 매도 응답 state={} uuid={}",
            receipt.state,
            receipt.upbit_uuid.as_deref().unwrap_or("unknown")
        )),
        safety_event_id: None,
        strategy_signal_reason: submission.policy_reason.clone(),
        idempotency_key: Some(live_market_sell_log_idempotency_key(submission)),
    }
}

fn build_live_market_sell_failed_log(
    submission: &LiveMarketSellSubmission,
    error: &str,
    safety_event_id: Option<uuid::Uuid>,
) -> PurchaseLog {
    PurchaseLog {
        id: uuid::Uuid::new_v4(),
        schedule_id: uuid::Uuid::nil(),
        thread_id: Some(submission.thread_id),
        executed_at: Utc::now(),
        amount_krw: submission.approval.amount_krw,
        volume_btc: submission.volume.parse::<f64>().unwrap_or(0.0),
        status: PurchaseStatus::Failed,
        error_message: Some(error.to_string()),
        source: PurchaseLogSource::InvestmentThread,
        mode: ExecutionMode::Live,
        action: PurchaseLogAction::MarketSell,
        audit_category: AuditCategory::ApiFailure,
        title: Some("Live 시장가 매도 실패".to_string()),
        reason: Some("Upbit 매도 주문 제출 또는 응답 처리 실패".to_string()),
        safety_event_id,
        strategy_signal_reason: submission.policy_reason.clone(),
        idempotency_key: Some(live_market_sell_log_idempotency_key(submission)),
    }
}

fn live_market_buy_log_idempotency_key(submission: &LiveMarketBuySubmission) -> String {
    submission
        .idempotency_key
        .clone()
        .unwrap_or_else(|| submission.request.preview.identifier.clone())
}

fn live_market_sell_log_idempotency_key(submission: &LiveMarketSellSubmission) -> String {
    submission
        .idempotency_key
        .clone()
        .unwrap_or_else(|| submission.request.preview.identifier.clone())
}

fn live_order_identifier(intent: &LiveOrderIntent) -> String {
    let intent_code = match intent {
        LiveOrderIntent::MarketBuy => "b",
        LiveOrderIntent::MarketSell => "s",
    };
    format!(
        "vt{intent_code}{}",
        &uuid::Uuid::new_v4().simple().to_string()[..29]
    )
}

fn record_live_order_gate_block_event(gate: &LiveOrderGateDecision) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        gate.check.thread_id,
        SafetyEventDraft {
            event_type: SafetyEventType::Blocked,
            category: AuditCategory::SafetyGate,
            source: Some(live_order_gate_source_value(&gate.check.source).to_string()),
            related_schedule_id: gate.check.related_schedule_id,
            reason: Some(gate.reason.clone()),
        },
        format!(
            "{} {}원 실거래 주문 차단 · {}",
            live_order_gate_source_label(&gate.check.source),
            gate.check.amount_krw,
            gate.reason
        ),
    )
}

fn record_live_market_buy_submitted_event(
    submission: &LiveMarketBuySubmission,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_market_buy".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=submitted; identifier={}; amountKrw={}; gate={}",
                submission.request.preview.identifier,
                submission.approval.amount_krw,
                submission.gate_reason
            )),
        },
        format!("{}원 Live 시장가 매수 제출", submission.approval.amount_krw),
    )
}

fn record_live_market_buy_filled_event(
    submission: &LiveMarketBuySubmission,
    receipt: &LiveOrderExecutionReceipt,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::Trade,
            source: Some("live_market_buy".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=filled; identifier={}; upbitUuid={}; state={}; executedVolume={}",
                submission.request.preview.identifier,
                receipt.upbit_uuid.as_deref().unwrap_or("unknown"),
                receipt.state,
                receipt.executed_volume
            )),
        },
        format!(
            "{}원 Live 시장가 매수 체결 · {:.8} BTC",
            submission.approval.amount_krw, receipt.executed_volume
        ),
    )
}

fn record_live_market_buy_failed_event(
    submission: &LiveMarketBuySubmission,
    error: &str,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Warning,
            category: AuditCategory::ApiFailure,
            source: Some("live_market_buy".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=failed; identifier={}; amountKrw={}; error={}",
                submission.request.preview.identifier, submission.approval.amount_krw, error
            )),
        },
        format!(
            "{}원 Live 시장가 매수 실패 · {}",
            submission.approval.amount_krw, error
        ),
    )
}

fn record_live_market_sell_submitted_event(
    submission: &LiveMarketSellSubmission,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::SafetyGate,
            source: Some("live_market_sell".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=submitted; identifier={}; amountKrw={}; volume={}; policy={}; gate={}",
                submission.request.preview.identifier,
                submission.approval.amount_krw,
                submission.volume,
                submission.policy_reason.as_deref().unwrap_or("unspecified"),
                submission.gate_reason
            )),
        },
        format!("{} BTC Live 시장가 매도 제출", submission.volume),
    )
}

fn record_live_market_sell_filled_event(
    submission: &LiveMarketSellSubmission,
    receipt: &LiveOrderExecutionReceipt,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Info,
            category: AuditCategory::Trade,
            source: Some("live_market_sell".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=filled; identifier={}; upbitUuid={}; state={}; executedVolume={}; executedFundsKrw={}",
                submission.request.preview.identifier,
                receipt.upbit_uuid.as_deref().unwrap_or("unknown"),
                receipt.state,
                receipt.executed_volume,
                receipt
                    .executed_funds_krw
                    .map(|amount| amount.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            )),
        },
        format!(
            "{} BTC Live 시장가 매도 체결",
            submission.volume
        ),
    )
}

fn record_live_market_sell_failed_event(
    submission: &LiveMarketSellSubmission,
    error: &str,
) -> anyhow::Result<uuid::Uuid> {
    record_safety_event(
        Some(submission.thread_id),
        SafetyEventDraft {
            event_type: SafetyEventType::Warning,
            category: AuditCategory::ApiFailure,
            source: Some("live_market_sell".to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "outcome=failed; identifier={}; amountKrw={}; volume={}; error={}",
                submission.request.preview.identifier,
                submission.approval.amount_krw,
                submission.volume,
                error
            )),
        },
        format!(
            "{} BTC Live 시장가 매도 실패 · {}",
            submission.volume, error
        ),
    )
}

fn live_order_gate_source_value(source: &LiveOrderGateSource) -> &'static str {
    match source {
        LiveOrderGateSource::LegacySchedule => "legacy_schedule",
        LiveOrderGateSource::InvestmentThread => "investment_thread",
    }
}

fn live_order_gate_source_label(source: &LiveOrderGateSource) -> &'static str {
    match source {
        LiveOrderGateSource::LegacySchedule => "레거시 DCA 스케줄",
        LiveOrderGateSource::InvestmentThread => "투자 스레드",
    }
}

fn purchase_log_source_for_gate(source: &LiveOrderGateSource) -> PurchaseLogSource {
    match source {
        LiveOrderGateSource::LegacySchedule => PurchaseLogSource::LegacySchedule,
        LiveOrderGateSource::InvestmentThread => PurchaseLogSource::InvestmentThread,
    }
}

impl LiveOrderChance {
    fn supports_order_type(&self, side: &str, ord_type: &str) -> bool {
        let side_types = match side {
            "bid" => &self.bid_types,
            "ask" => &self.ask_types,
            _ => return false,
        };
        if side_types.iter().any(|item| item == ord_type) {
            return true;
        }
        side_types.is_empty() && self.order_types.iter().any(|item| item == ord_type)
    }
}

fn market_base_currency(market: &SupportedMarket) -> &'static str {
    match market {
        SupportedMarket::KrwBtc => "BTC",
        SupportedMarket::KrwEth => "ETH",
        SupportedMarket::KrwXrp => "XRP",
    }
}

fn stored_credentials_available() -> bool {
    let access_key_available = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .is_some();
    let secret_key_available = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .is_some();

    access_key_available && secret_key_available
}

fn reset_live_readiness_after_credential_change(source: &str) -> Result<(), String> {
    let now = Utc::now();
    let mut settings = load_settings().map_err(|error| error.to_string())?;
    let previous_global_live_locked = settings.global_live_locked;
    let previous_strategy_logic_approved = settings.strategy_logic_approved;
    settings.global_live_locked = true;
    settings.strategy_logic_approved = false;
    persist_settings(&settings).map_err(|error| error.to_string())?;

    let mut threads = load_investment_threads().map_err(|error| error.to_string())?;
    let reset_thread_count = reset_threads_for_credential_change(&mut threads, now);
    if reset_thread_count > 0 {
        persist_investment_threads(&threads).map_err(|error| error.to_string())?;
    }

    let _ = record_safety_event(
        None,
        SafetyEventDraft {
            event_type: SafetyEventType::Warning,
            category: AuditCategory::SafetyGate,
            source: Some(source.to_string()),
            related_schedule_id: None,
            reason: Some(format!(
                "credentialChanged=true; previousGlobalLiveLocked={previous_global_live_locked}; newGlobalLiveLocked=true; previousStrategyLogicApproved={previous_strategy_logic_approved}; newStrategyLogicApproved=false; resetThreadCount={reset_thread_count}"
            )),
        },
        "API 키 변경으로 Live readiness와 최종 확인을 해제했습니다".to_string(),
    );

    Ok(())
}

fn reset_threads_for_credential_change(
    threads: &mut [InvestmentThread],
    now: chrono::DateTime<Utc>,
) -> usize {
    let mut reset_thread_count = 0usize;
    for thread in threads {
        let had_confirmation = !thread_has_missing_live_confirmation(thread);
        let was_live_ready = matches!(thread.status, ThreadStatus::Armed | ThreadStatus::Live);
        if had_confirmation || was_live_ready {
            clear_live_confirmation(thread);
            if was_live_ready {
                thread.status = ThreadStatus::Paused;
            }
            thread.updated_at = now;
            reset_thread_count += 1;
        }
    }
    reset_thread_count
}

fn thread_has_missing_live_confirmation(thread: &InvestmentThread) -> bool {
    thread.final_confirmation_status == LiveOrderFinalConfirmationStatus::Missing
        && thread.final_confirmation_text.is_none()
        && thread.final_confirmed_at.is_none()
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

impl LiveOrderExecutor for UpbitLiveOrderExecutor {
    fn order_chance<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        market: &'a SupportedMarket,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderChance, String>> + Send + 'a>> {
        Box::pin(
            async move { execute_upbit_order_chance_request(access_key, secret_key, market).await },
        )
    }

    fn market_buy<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        submission: &'a LiveMarketBuySubmission,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>> {
        Box::pin(async move {
            execute_upbit_order_request(access_key, secret_key, &submission.request).await
        })
    }

    fn market_sell<'a>(
        &'a self,
        access_key: &'a str,
        secret_key: &'a str,
        submission: &'a LiveMarketSellSubmission,
    ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>> {
        Box::pin(async move {
            execute_upbit_order_request(access_key, secret_key, &submission.request).await
        })
    }
}

async fn execute_upbit_order_chance_request(
    access_key: &str,
    secret_key: &str,
    market: &SupportedMarket,
) -> Result<LiveOrderChance, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha512};

    #[derive(Serialize)]
    struct Claims {
        access_key: String,
        nonce: String,
        query_hash: String,
        query_hash_alg: String,
    }

    #[derive(Deserialize)]
    struct ChanceAccountResponse {
        currency: String,
        balance: String,
    }

    #[derive(Deserialize, Default)]
    struct ChanceMarketTotalResponse {
        min_total: Option<serde_json::Value>,
    }

    #[derive(Deserialize, Default)]
    struct ChanceMarketResponse {
        id: Option<String>,
        #[serde(default)]
        order_sides: Vec<String>,
        #[serde(default)]
        order_types: Vec<String>,
        #[serde(default)]
        bid_types: Vec<String>,
        #[serde(default)]
        ask_types: Vec<String>,
        #[serde(default)]
        bid: Option<ChanceMarketTotalResponse>,
        #[serde(default)]
        ask: Option<ChanceMarketTotalResponse>,
    }

    #[derive(Deserialize)]
    struct ChanceResponse {
        bid_account: ChanceAccountResponse,
        ask_account: ChanceAccountResponse,
        market: ChanceMarketResponse,
    }

    let query_string = format!("market={}", market.as_upbit_market());
    let query_hash = hex_string(Sha512::digest(query_string.as_bytes()).as_slice());
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
        .get("https://api.upbit.com/v1/orders/chance")
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("market", market.as_upbit_market())])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("업비트 주문 가능 정보 오류: HTTP {status} {body}"));
    }

    let response = resp
        .json::<ChanceResponse>()
        .await
        .map_err(|e| e.to_string())?;
    if response.market.id.as_deref() != Some(market.as_upbit_market()) {
        return Err("업비트 주문 가능 정보의 마켓이 요청과 일치하지 않습니다".to_string());
    }

    Ok(LiveOrderChance {
        market: market.clone(),
        bid_currency: response.bid_account.currency,
        bid_balance: response.bid_account.balance.parse::<f64>().unwrap_or(0.0),
        ask_currency: response.ask_account.currency,
        ask_balance: response.ask_account.balance.parse::<f64>().unwrap_or(0.0),
        order_sides: response.market.order_sides,
        order_types: response.market.order_types,
        bid_types: response.market.bid_types,
        ask_types: response.market.ask_types,
        bid_min_total_krw: response
            .market
            .bid
            .and_then(|total| parse_upbit_krw_amount(total.min_total)),
        ask_min_total_krw: response
            .market
            .ask
            .and_then(|total| parse_upbit_krw_amount(total.min_total)),
    })
}

async fn execute_upbit_order_request(
    access_key: &str,
    secret_key: &str,
    request: &UpbitOrderRequest,
) -> Result<LiveOrderExecutionReceipt, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha512};

    #[derive(Serialize)]
    struct Claims {
        access_key: String,
        nonce: String,
        query_hash: String,
        query_hash_alg: String,
    }

    #[derive(Deserialize)]
    struct OrderResponse {
        uuid: Option<String>,
        state: Option<String>,
        executed_volume: Option<String>,
        executed_funds: Option<String>,
    }

    let query_hash = hex_string(Sha512::digest(request.preview.query_string.as_bytes()).as_slice());
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
        .header("Content-Type", "application/json; charset=utf-8")
        .body(request.json_body.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("업비트 주문 오류: HTTP {status} {body}"));
    }

    let response_body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    let response: OrderResponse =
        serde_json::from_value(response_body.clone()).map_err(|e| e.to_string())?;
    let executed_volume = response
        .executed_volume
        .as_deref()
        .and_then(|volume| volume.parse::<f64>().ok())
        .unwrap_or(0.0);
    let executed_funds_krw = response
        .executed_funds
        .as_deref()
        .and_then(|funds| funds.parse::<f64>().ok())
        .filter(|funds| funds.is_finite() && *funds >= 0.0)
        .map(|funds| funds.round() as u64);

    Ok(LiveOrderExecutionReceipt {
        upbit_uuid: response.uuid,
        state: response.state.unwrap_or_else(|| "unknown".to_string()),
        executed_volume,
        executed_funds_krw,
    })
}

fn parse_upbit_krw_amount(value: Option<serde_json::Value>) -> Option<u64> {
    value
        .and_then(|value| match value {
            serde_json::Value::String(text) => text.parse::<f64>().ok(),
            serde_json::Value::Number(number) => number.as_f64(),
            _ => None,
        })
        .filter(|amount| amount.is_finite() && *amount >= 0.0)
        .map(|amount| amount.ceil() as u64)
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
        assert!(!settings.strategy_logic_approved);
    }

    #[test]
    fn new_thread_live_input_is_forced_to_draft_missing() {
        let now = Utc::now();
        let mut incoming = sample_thread(now);
        incoming.status = ThreadStatus::Live;
        incoming.validation_status = ValidationStatus::Pass;
        incoming.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;

        let saved = merge_investment_thread(None, incoming, now);

        assert_eq!(saved.status, ThreadStatus::Draft);
        assert_eq!(saved.validation_status, ValidationStatus::Missing);
        assert_eq!(
            saved.final_confirmation_status,
            LiveOrderFinalConfirmationStatus::Missing
        );
        assert_eq!(saved.final_confirmation_text, None);
        assert_eq!(saved.final_confirmed_at, None);
    }

    #[test]
    fn existing_live_thread_edit_resets_to_draft_missing() {
        let created_at = Utc::now();
        let now = created_at + chrono::Duration::seconds(10);
        let mut existing = sample_thread(created_at);
        existing.status = ThreadStatus::Live;
        existing.validation_status = ValidationStatus::Pass;
        existing.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;
        let mut incoming = existing.clone();
        incoming.name = "편집된 스레드".to_string();
        incoming.status = ThreadStatus::Live;
        incoming.validation_status = ValidationStatus::Pass;
        incoming.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;

        let saved = merge_investment_thread(Some(&existing), incoming, now);

        assert_eq!(saved.name, "편집된 스레드");
        assert_eq!(saved.status, ThreadStatus::Draft);
        assert_eq!(saved.validation_status, ValidationStatus::Missing);
        assert_eq!(
            saved.final_confirmation_status,
            LiveOrderFinalConfirmationStatus::Missing
        );
        assert_eq!(saved.final_confirmation_text, None);
        assert_eq!(saved.final_confirmed_at, None);
        assert_eq!(saved.created_at, created_at);
        assert_eq!(saved.updated_at, now);
    }

    #[test]
    fn completed_thread_is_terminal_for_safety_transitions() {
        assert!(validate_thread_safety_transition(
            &ThreadStatus::Completed,
            &ThreadStatus::Stopped
        )
        .is_err());
        assert!(
            validate_thread_safety_transition(&ThreadStatus::Completed, &ThreadStatus::Paused)
                .is_err()
        );
        assert!(
            validate_thread_safety_transition(&ThreadStatus::Stopped, &ThreadStatus::Paused)
                .is_err()
        );
        assert!(
            validate_thread_safety_transition(&ThreadStatus::Stopped, &ThreadStatus::Stopped)
                .is_err()
        );
        assert!(
            validate_thread_safety_transition(&ThreadStatus::Draft, &ThreadStatus::Paused).is_err()
        );
        assert!(
            validate_thread_safety_transition(&ThreadStatus::Paper, &ThreadStatus::Completed)
                .is_ok()
        );
    }

    #[test]
    fn old_thread_json_defaults_final_confirmation_to_missing() {
        let now = Utc::now().to_rfc3339();
        let json = format!(
            r#"{{
                "id":"{}",
                "name":"테스트 스레드",
                "market":"KRW-BTC",
                "initialBudgetKrw":100000,
                "durationDays":30,
                "strategyProfile":"conservative",
                "maxLossPercent":50.0,
                "dailyTradeCap":10,
                "status":"draft",
                "validationStatus":"missing",
                "createdAt":"{now}",
                "updatedAt":"{now}"
            }}"#,
            uuid::Uuid::new_v4()
        );

        let thread: InvestmentThread = serde_json::from_str(&json).expect("parse old thread");

        assert_eq!(
            thread.final_confirmation_status,
            LiveOrderFinalConfirmationStatus::Missing
        );
        assert_eq!(thread.final_confirmation_text, None);
        assert_eq!(thread.final_confirmed_at, None);
    }

    #[test]
    fn final_confirmation_phrase_must_match_exactly() {
        assert!(validate_live_confirmation_text(REQUIRED_LIVE_CONFIRMATION_PHRASE).is_ok());
        assert!(validate_live_confirmation_text("실거래 위험 확인").is_err());
    }

    #[test]
    fn final_confirmation_requires_saved_phrase_and_timestamp() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;

        assert!(!thread_has_valid_live_confirmation(&thread));

        thread.final_confirmation_text = Some(REQUIRED_LIVE_CONFIRMATION_PHRASE.to_string());
        assert!(!thread_has_valid_live_confirmation(&thread));

        thread.final_confirmed_at = Some(now);
        assert!(thread_has_valid_live_confirmation(&thread));
    }

    #[test]
    fn credential_change_resets_live_readiness_and_final_confirmation() {
        let created_at = Utc::now();
        let reset_at = created_at + chrono::Duration::seconds(10);
        let live_thread = confirmed_live_thread(created_at);
        let mut armed_thread = confirmed_live_thread(created_at);
        armed_thread.status = ThreadStatus::Armed;
        let mut paused_thread = confirmed_live_thread(created_at);
        paused_thread.status = ThreadStatus::Paused;
        let mut draft_thread = sample_thread(created_at);
        draft_thread.updated_at = created_at;

        let mut threads = vec![
            live_thread,
            armed_thread,
            paused_thread,
            draft_thread.clone(),
        ];

        let reset_count = reset_threads_for_credential_change(&mut threads, reset_at);

        assert_eq!(reset_count, 3);
        assert_eq!(threads[0].status, ThreadStatus::Paused);
        assert_eq!(threads[1].status, ThreadStatus::Paused);
        assert_eq!(threads[2].status, ThreadStatus::Paused);
        assert_eq!(threads[3].status, ThreadStatus::Draft);
        for thread in threads.iter().take(3) {
            assert_eq!(
                thread.final_confirmation_status,
                LiveOrderFinalConfirmationStatus::Missing
            );
            assert_eq!(thread.final_confirmation_text, None);
            assert_eq!(thread.final_confirmed_at, None);
            assert_eq!(thread.updated_at, reset_at);
        }
        assert_eq!(threads[3].updated_at, draft_thread.updated_at);
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
    fn sell_logs_do_not_inflate_invested_capital_or_buy_count() {
        let schedule_id = uuid::Uuid::new_v4();
        let mut sell = sample_purchase_log(schedule_id, "2026-01-02T00:00:00Z", 5_000, 0.5);
        sell.action = PurchaseLogAction::MarketSell;
        let logs = vec![
            sample_purchase_log(schedule_id, "2026-01-01T00:00:00Z", 10_000, 1.0),
            sell,
        ];

        let analytics = build_portfolio_analytics(&logs, &[], &[], &[]);

        assert_eq!(analytics.summary.invested_krw, 10_000);
        assert_eq!(analytics.summary.successful_buys, 1);
        assert_eq!(analytics.summary.current_value_krw, 10_000);
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
            final_confirmation_status: LiveOrderFinalConfirmationStatus::Missing,
            final_confirmation_text: None,
            final_confirmed_at: None,
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
            thread_id: None,
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
            idempotency_key: None,
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
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::LegacySchedule,
                thread_id: None,
                related_schedule_id: Some(schedule.id),
                market: SupportedMarket::KrwBtc,
                intent: None,
                amount_krw: schedule.amount,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Missing,
                daily_trade_count: 0,
                daily_trade_cap: DEFAULT_DAILY_TRADE_CAP,
                max_loss_percent: None,
                latest_max_drawdown_percent: None,
                checked_at: now,
            },
            vec![LiveOrderGateBlockReason::LegacyScheduleNotMigrated],
        );
        let safety_event_id = uuid::Uuid::new_v4();

        let log = build_live_order_blocked_log(&gate, Some(safety_event_id));

        assert_eq!(log.schedule_id, schedule.id);
        assert_eq!(log.thread_id, None);
        assert_eq!(log.status, PurchaseStatus::Blocked);
        assert_eq!(log.source, PurchaseLogSource::LegacySchedule);
        assert_eq!(log.mode, ExecutionMode::Live);
        assert_eq!(log.action, PurchaseLogAction::SafetyCheck);
        assert_eq!(log.audit_category, AuditCategory::BlockedOrder);
        assert_eq!(log.amount_krw, 10_000);
        assert_eq!(log.volume_btc, 0.0);
        assert!(log.reason.unwrap_or_default().contains("마이그레이션"));
        assert_eq!(log.safety_event_id, Some(safety_event_id));
    }

    #[test]
    fn legacy_schedule_live_policy_status_is_explicitly_blocked() {
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

        let status = build_legacy_schedule_live_policy_status(&schedule);

        assert_eq!(status.schedule_id, schedule.id);
        assert_eq!(
            status.policy,
            LegacyScheduleLivePolicy::BlockedUseInvestmentThread
        );
        assert!(!status.live_order_allowed);
        assert!(!status.live_order_gate.allowed);
        assert!(status
            .live_order_gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::LegacyScheduleNotMigrated));
        assert!(status.description.contains("투자 스레드"));
    }

    #[test]
    fn scheduler_path_cannot_submit_direct_upbit_orders() {
        let source = include_str!("commands.rs");
        let scheduler_section = source
            .split("async fn execute_due_schedules")
            .nth(1)
            .and_then(|section| section.split("fn notify_purchase_result").next())
            .expect("scheduler section");

        assert!(scheduler_section.contains("reconcile_legacy_schedule_order(schedule)"));
        assert!(!scheduler_section.contains("execute_upbit_order_request"));
        assert!(!scheduler_section.contains("submit_thread_live_market_buy"));
        assert!(!scheduler_section.contains("submit_thread_live_market_sell"));
        assert!(!scheduler_section.contains(".post(\"https://api.upbit.com/v1/orders\")"));
    }

    #[test]
    fn shared_gate_blocks_legacy_schedule_even_when_global_lock_is_open() {
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
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::legacy_schedule(&schedule),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::LegacyScheduleNotMigrated));
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::FinalConfirmationMissing));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_blocks_live_thread_without_final_confirmation() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Live;
        thread.validation_status = ValidationStatus::Pass;
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::FinalConfirmationMissing));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_blocks_live_thread_with_status_only_confirmation() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Live;
        thread.validation_status = ValidationStatus::Pass;
        thread.final_confirmation_status = LiveOrderFinalConfirmationStatus::Confirmed;
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::FinalConfirmationMissing));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_blocks_live_thread_without_credentials_or_strategy_approval() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let mut settings = unlocked_settings();
        settings.strategy_logic_approved = false;
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(false),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::CredentialsMissing));
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::StrategyLogicNotApproved));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_blocks_live_thread_at_daily_trade_cap() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = (0..thread.daily_trade_cap)
            .map(|_| sample_thread_purchase_log(&thread, now))
            .collect::<Vec<_>>();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert_eq!(gate.check.daily_trade_count, thread.daily_trade_cap);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::DailyTradeCapExceeded));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_does_not_block_live_thread_when_backtest_exceeds_max_loss() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(gate.allowed);
        assert_eq!(gate.check.max_loss_percent, Some(50.0));
        assert_eq!(gate.check.latest_max_drawdown_percent, None);
        assert!(!gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::MaxLossExceeded));
        assert!(live_order_approval_from_gate(&gate).is_some());
    }

    #[test]
    fn shared_gate_does_not_block_legacy_validation_strategy_version() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(gate.allowed);
        assert!(!gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::ValidationNotPassed));
        assert!(live_order_approval_from_gate(&gate).is_some());
    }

    #[test]
    fn shared_gate_returns_approval_only_after_all_live_thread_checks_pass() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        let approval = live_order_approval_from_gate(&gate).expect("gate approval");

        assert!(gate.allowed);
        assert!(gate.block_reasons.is_empty());
        assert_eq!(approval.market, SupportedMarket::KrwBtc);
        assert_eq!(approval.amount_krw, 20_000);
    }

    #[test]
    fn shared_gate_includes_order_chance_tradability_blocks_for_buy_action() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();
        let request =
            build_upbit_order_request(&thread.market, LiveOrderIntent::MarketBuy, 20_000, None)
                .expect("buy request");
        let mut chance = sample_live_order_chance(&thread.market, 10_000.0, 1.0);
        chance.bid_min_total_krw = Some(25_000);

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now).with_order_probe(
                LiveOrderIntent::MarketBuy,
                request.preview,
                Ok(chance),
            ),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert_eq!(gate.check.intent, Some(LiveOrderIntent::MarketBuy));
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::InsufficientBalance));
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::MinimumOrderAmountNotMet));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_includes_order_chance_failure_for_sell_action() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();
        let request = build_upbit_order_request(
            &thread.market,
            LiveOrderIntent::MarketSell,
            20_000,
            Some("0.001".to_string()),
        )
        .expect("sell request");

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now).with_order_probe(
                LiveOrderIntent::MarketSell,
                request.preview,
                Err("HTTP 503 temporary upstream failure".to_string()),
            ),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert_eq!(gate.check.intent, Some(LiveOrderIntent::MarketSell));
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::OrderChanceUnavailable));
        assert!(live_order_approval_from_gate(&gate).is_none());
    }

    #[test]
    fn shared_gate_classifies_order_permission_failure_from_chance_probe() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let settings = unlocked_settings();
        let logs = Vec::new();
        let request =
            build_upbit_order_request(&thread.market, LiveOrderIntent::MarketBuy, 20_000, None)
                .expect("buy request");

        let gate = evaluate_live_order_gate_with_data(
            LiveOrderGateInput::investment_thread(&thread, 20_000, now).with_order_probe(
                LiveOrderIntent::MarketBuy,
                request.preview,
                Err("HTTP 401 out_of_scope".to_string()),
            ),
            LiveOrderGateData {
                settings: Ok(&settings),
                credentials_available: Ok(true),
                logs: Ok(&logs),
            },
        );

        assert!(!gate.allowed);
        assert!(gate
            .block_reasons
            .contains(&LiveOrderGateBlockReason::OrderPermissionDenied));
    }

    #[test]
    fn upbit_market_buy_payload_uses_price_order_json_shape() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);

        let payload = build_upbit_order_payload_preview(
            &thread.market,
            LiveOrderIntent::MarketBuy,
            20_000,
            None,
        )
        .expect("market buy payload");

        assert_eq!(payload.market, SupportedMarket::KrwBtc);
        assert_eq!(payload.side, "bid");
        assert_eq!(payload.ord_type, "price");
        assert_eq!(payload.price.as_deref(), Some("20000"));
        assert_eq!(payload.volume, None);
        assert!(payload.query_string.contains("side=bid"));
        assert!(payload.query_string.contains("ord_type=price"));
        assert!(payload.query_string.contains("price=20000"));
        assert!(payload.identifier.len() <= 32);
    }

    #[test]
    fn upbit_market_buy_payload_allows_gate_to_decide_minimum_amount() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);

        let payload = build_upbit_order_payload_preview(
            &thread.market,
            LiveOrderIntent::MarketBuy,
            4_999,
            None,
        )
        .expect("market buy preview");

        assert_eq!(payload.price.as_deref(), Some("4999"));
    }

    #[test]
    fn upbit_market_sell_payload_uses_market_order_json_shape() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);

        let payload = build_upbit_order_payload_preview(
            &thread.market,
            LiveOrderIntent::MarketSell,
            0,
            Some("0.005".to_string()),
        )
        .expect("market sell payload");

        assert_eq!(payload.side, "ask");
        assert_eq!(payload.ord_type, "market");
        assert_eq!(payload.price, None);
        assert_eq!(payload.volume.as_deref(), Some("0.005"));
        assert!(payload.query_string.contains("side=ask"));
        assert!(payload.query_string.contains("ord_type=market"));
        assert!(payload.query_string.contains("volume=0.005"));
        assert!(payload.identifier.len() <= 32);
    }

    #[test]
    fn upbit_market_sell_payload_rejects_non_positive_or_non_numeric_volume() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);

        let zero = build_upbit_order_payload_preview(
            &thread.market,
            LiveOrderIntent::MarketSell,
            0,
            Some("0".to_string()),
        );
        let injected = build_upbit_order_payload_preview(
            &thread.market,
            LiveOrderIntent::MarketSell,
            0,
            Some("0.01&side=bid".to_string()),
        );

        assert!(zero.is_err());
        assert!(injected.is_err());
    }

    #[test]
    fn upbit_order_request_preview_and_json_body_share_serialization_contract() {
        let request = build_upbit_order_request(
            &SupportedMarket::KrwBtc,
            LiveOrderIntent::MarketBuy,
            20_000,
            None,
        )
        .expect("order request");

        assert_eq!(
            request.preview.query_string,
            format!(
                "market=KRW-BTC&side=bid&ord_type=price&price=20000&identifier={}",
                request.preview.identifier
            )
        );
        assert_eq!(
            request.json_body,
            format!(
                r#"{{"market":"KRW-BTC","side":"bid","ord_type":"price","price":"20000","identifier":"{}"}}"#,
                request.preview.identifier
            )
        );
    }

    #[test]
    fn upbit_market_sell_request_preview_and_json_body_share_serialization_contract() {
        let request = build_upbit_order_request_with_identifier(
            &SupportedMarket::KrwBtc,
            LiveOrderIntent::MarketSell,
            20_000,
            Some("0.005".to_string()),
            Some("selltest".to_string()),
        )
        .expect("sell request");

        assert_eq!(
            request.preview.query_string,
            "market=KRW-BTC&side=ask&ord_type=market&volume=0.005&identifier=selltest"
        );
        assert_eq!(
            request.json_body,
            r#"{"market":"KRW-BTC","side":"ask","ord_type":"market","volume":"0.005","identifier":"selltest"}"#
        );
    }

    #[test]
    fn live_order_identifier_policy_is_shared_and_short() {
        let buy_identifier = live_order_identifier(&LiveOrderIntent::MarketBuy);
        let sell_identifier = live_order_identifier(&LiveOrderIntent::MarketSell);

        assert!(buy_identifier.starts_with("vtb"));
        assert!(sell_identifier.starts_with("vts"));
        assert!(buy_identifier.len() <= 32);
        assert!(sell_identifier.len() <= 32);
        assert_ne!(
            buy_identifier,
            live_order_identifier(&LiveOrderIntent::MarketBuy)
        );
    }

    #[test]
    fn live_auto_loop_idempotency_is_stable_and_retry_limited() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let signal = sample_strategy_signal(&thread, PaperSignalAction::Buy, now);
        let key = live_loop_idempotency_key(&thread, &signal, 20_000);
        let same_key = live_loop_idempotency_key(&thread, &signal, 20_000);
        let identifier = live_loop_order_identifier(&key, &LiveOrderIntent::MarketBuy);

        assert_eq!(key, same_key);
        assert!(identifier.starts_with("vlb"));
        assert!(identifier.len() <= 32);

        let mut submitted = sample_thread_purchase_log(&thread, now);
        submitted.status = PurchaseStatus::Submitted;
        submitted.mode = ExecutionMode::Live;
        submitted.action = PurchaseLogAction::MarketBuy;
        submitted.idempotency_key = Some(key.clone());
        assert!(live_loop_has_pending_or_filled_order(
            &[submitted.clone()],
            &key
        ));

        let mut failed = submitted.clone();
        failed.status = PurchaseStatus::Failed;
        assert!(!live_loop_has_pending_or_filled_order(
            &[submitted.clone(), failed.clone()],
            &key
        ));
        assert_eq!(
            live_loop_failed_retry_count(&[failed.clone(), failed.clone(), failed], &key),
            LIVE_AUTO_LOOP_MAX_RETRIES_PER_TICK
        );
    }

    #[test]
    fn paper_buy_signal_creates_paper_log_without_live_approval() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Paper;
        thread.validation_status = ValidationStatus::Pass;
        let signal = sample_strategy_signal(&thread, PaperSignalAction::Buy, now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 5_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Missing,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: Some(4.0),
                checked_at: now,
            },
            vec![
                LiveOrderGateBlockReason::GlobalLiveLocked,
                LiveOrderGateBlockReason::LiveModeNotEnabled,
            ],
        );

        let result = build_paper_execution_result(&thread, signal, gate, &[], 5_000);
        let log = result.log.expect("paper buy log");

        assert!(!result.live_order_gate.allowed);
        assert!(!result.duplicate);
        assert!(result.position_open);
        assert_eq!(result.realized_pnl_krw, None);
        assert_eq!(log.thread_id, Some(thread.id));
        assert_eq!(log.source, PurchaseLogSource::InvestmentThread);
        assert_eq!(log.mode, ExecutionMode::Paper);
        assert_eq!(log.audit_category, AuditCategory::PaperTrade);
        assert_eq!(log.status, PurchaseStatus::Success);
        assert_eq!(log.amount_krw, 5_000);
        assert!(log.volume_btc > 0.0);
        assert_eq!(
            log.idempotency_key.as_deref(),
            Some(result.idempotency_key.as_str())
        );
        assert!(log
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("Live Order Gate 확인"));
    }

    #[test]
    fn duplicate_paper_tick_reuses_existing_log_by_idempotency_key() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Paper;
        let signal = sample_strategy_signal(&thread, PaperSignalAction::Buy, now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 5_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Missing,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: None,
                checked_at: now,
            },
            vec![LiveOrderGateBlockReason::LiveModeNotEnabled],
        );
        let first = build_paper_execution_result(&thread, signal.clone(), gate.clone(), &[], 5_000);
        let existing = first.log.clone().expect("first log");

        let second =
            build_paper_execution_result(&thread, signal, gate, &[existing.clone()], 5_000);

        assert!(second.duplicate);
        assert_eq!(second.idempotency_key, first.idempotency_key);
        assert_eq!(second.log.expect("existing log").id, existing.id);
    }

    #[test]
    fn paper_sell_signal_closes_existing_paper_position_with_estimated_pnl() {
        let now = Utc::now();
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Paper;
        let buy_signal = sample_strategy_signal(&thread, PaperSignalAction::Buy, now);
        let sell_signal = sample_strategy_signal(
            &thread,
            PaperSignalAction::Sell,
            now + chrono::Duration::hours(2),
        );
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 5_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Missing,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: None,
                checked_at: now,
            },
            vec![LiveOrderGateBlockReason::LiveModeNotEnabled],
        );
        let buy = build_paper_execution_result(&thread, buy_signal, gate.clone(), &[], 5_000);
        let buy_log = buy.log.expect("buy log");

        let sell =
            build_paper_execution_result(&thread, sell_signal, gate, &[buy_log.clone()], 5_000);
        let sell_log = sell.log.expect("sell log");

        assert!(!sell.position_open);
        assert_eq!(sell_log.action, PurchaseLogAction::MarketSell);
        assert_eq!(sell_log.volume_btc, buy_log.volume_btc);
        assert_eq!(
            sell.realized_pnl_krw,
            Some(sell_log.amount_krw as i64 - buy_log.amount_krw as i64)
        );
        assert!(sell.message.contains("실제 Upbit 주문 없음"));
    }

    #[test]
    fn live_open_position_tracks_filled_buys_and_sells() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let buy = sample_thread_purchase_log(&thread, now);

        let open = live_open_position(&thread, &[buy.clone()]).expect("open live position");
        assert_eq!(open.amount_krw, buy.amount_krw);
        assert_eq!(open.volume_btc, buy.volume_btc);

        let mut partial_sell =
            sample_thread_purchase_log(&thread, now + chrono::Duration::minutes(5));
        partial_sell.action = PurchaseLogAction::MarketSell;
        partial_sell.volume_btc = buy.volume_btc / 2.0;
        partial_sell.amount_krw = buy.amount_krw / 2;

        let partial = live_open_position(&thread, &[buy.clone(), partial_sell])
            .expect("remaining live position");
        assert_eq!(partial.volume_btc, buy.volume_btc / 2.0);
        assert_eq!(partial.amount_krw, buy.amount_krw / 2);

        let mut full_sell = sample_thread_purchase_log(&thread, now + chrono::Duration::minutes(10));
        full_sell.action = PurchaseLogAction::MarketSell;
        full_sell.volume_btc = buy.volume_btc;

        assert!(live_open_position(&thread, &[buy, full_sell]).is_none());
    }

    #[test]
    fn live_sell_estimate_uses_open_position_volume_and_signal_price() {
        let position = LiveOpenPosition {
            amount_krw: 20_000,
            volume_btc: 0.25,
        };

        assert_eq!(estimated_live_sell_amount_krw(&position, 100_000.0), 24_988);
        assert_eq!(estimated_live_sell_amount_krw(&position, 0.0), 0);
    }

    #[test]
    fn live_market_buy_submission_requires_gate_approval() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 20_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Confirmed,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: Some(4.0),
                checked_at: now,
            },
            vec![LiveOrderGateBlockReason::GlobalLiveLocked],
        );

        let submission = prepare_live_market_buy_submission(&thread, &gate, now);

        assert!(submission.is_err());
    }

    #[tokio::test]
    async fn live_market_buy_executor_boundary_is_mockable() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        struct MockExecutor {
            calls: Arc<AtomicUsize>,
        }

        impl LiveOrderExecutor for MockExecutor {
            fn order_chance<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                market: &'a SupportedMarket,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderChance, String>> + Send + 'a>>
            {
                Box::pin(async move { Ok(sample_live_order_chance(market, 100_000.0, 1.0)) })
            }

            fn market_buy<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                submission: &'a LiveMarketBuySubmission,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>
            {
                let calls = self.calls.clone();
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(LiveOrderExecutionReceipt {
                        upbit_uuid: Some("mock-order".to_string()),
                        state: "done".to_string(),
                        executed_volume: submission.approval.amount_krw as f64 / 50_000_000.0,
                        executed_funds_krw: Some(submission.approval.amount_krw),
                    })
                })
            }

            fn market_sell<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                _submission: &'a LiveMarketSellSubmission,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>
            {
                Box::pin(async move { Err("unexpected sell call".to_string()) })
            }
        }

        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 20_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Confirmed,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: Some(4.0),
                checked_at: now,
            },
            vec![],
        );
        let submission =
            prepare_live_market_buy_submission(&thread, &gate, now).expect("approved submission");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = MockExecutor {
            calls: calls.clone(),
        };

        let receipt = executor
            .market_buy("access", "secret", &submission)
            .await
            .expect("mock receipt");
        let submitted = build_live_market_buy_submitted_log(&submission);
        let filled = build_live_market_buy_filled_log(&submission, &receipt);

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(submitted.status, PurchaseStatus::Submitted);
        assert_eq!(filled.status, PurchaseStatus::Filled);
        assert_eq!(filled.idempotency_key, submitted.idempotency_key);
        assert_eq!(filled.volume_btc, 0.0004);
    }

    #[test]
    fn live_market_sell_submission_requires_gate_approval() {
        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 20_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Confirmed,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: Some(4.0),
                checked_at: now,
            },
            vec![LiveOrderGateBlockReason::GlobalLiveLocked],
        );
        let request = LiveMarketSellRequest {
            thread_id: thread.id,
            volume: "0.001".to_string(),
            estimated_amount_krw: Some(20_000),
            policy_reason: Some("strategy_signal_sell".to_string()),
        };

        let submission = prepare_live_market_sell_submission(&thread, request, &gate, now);

        assert!(submission.is_err());
    }

    #[tokio::test]
    async fn live_market_sell_executor_boundary_is_mockable() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        struct MockExecutor {
            calls: Arc<AtomicUsize>,
        }

        impl LiveOrderExecutor for MockExecutor {
            fn order_chance<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                market: &'a SupportedMarket,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderChance, String>> + Send + 'a>>
            {
                Box::pin(async move { Ok(sample_live_order_chance(market, 100_000.0, 1.0)) })
            }

            fn market_buy<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                _submission: &'a LiveMarketBuySubmission,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>
            {
                Box::pin(async move { Err("unexpected buy call".to_string()) })
            }

            fn market_sell<'a>(
                &'a self,
                _access_key: &'a str,
                _secret_key: &'a str,
                submission: &'a LiveMarketSellSubmission,
            ) -> Pin<Box<dyn Future<Output = Result<LiveOrderExecutionReceipt, String>> + Send + 'a>>
            {
                let calls = self.calls.clone();
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(LiveOrderExecutionReceipt {
                        upbit_uuid: Some("mock-sell-order".to_string()),
                        state: "done".to_string(),
                        executed_volume: submission.volume.parse::<f64>().unwrap_or(0.0),
                        executed_funds_krw: Some(25_000),
                    })
                })
            }
        }

        let now = Utc::now();
        let thread = confirmed_live_thread(now);
        let gate = live_order_gate_decision(
            LiveOrderGateCheck {
                source: LiveOrderGateSource::InvestmentThread,
                thread_id: Some(thread.id),
                related_schedule_id: None,
                market: thread.market.clone(),
                intent: None,
                amount_krw: 25_000,
                final_confirmation_status: LiveOrderFinalConfirmationStatus::Confirmed,
                daily_trade_count: 0,
                daily_trade_cap: thread.daily_trade_cap,
                max_loss_percent: Some(thread.max_loss_percent),
                latest_max_drawdown_percent: Some(4.0),
                checked_at: now,
            },
            vec![],
        );
        let request = LiveMarketSellRequest {
            thread_id: thread.id,
            volume: "0.001".to_string(),
            estimated_amount_krw: Some(25_000),
            policy_reason: Some("manual_stop_policy".to_string()),
        };
        let submission = prepare_live_market_sell_submission(&thread, request, &gate, now)
            .expect("sell submission");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = MockExecutor {
            calls: calls.clone(),
        };

        let receipt = executor
            .market_sell("access", "secret", &submission)
            .await
            .expect("mock sell receipt");
        let submitted = build_live_market_sell_submitted_log(&submission);
        let filled = build_live_market_sell_filled_log(&submission, &receipt);

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(submitted.status, PurchaseStatus::Submitted);
        assert_eq!(submitted.action, PurchaseLogAction::MarketSell);
        assert_eq!(filled.status, PurchaseStatus::Filled);
        assert_eq!(filled.action, PurchaseLogAction::MarketSell);
        assert_eq!(filled.amount_krw, 25_000);
        assert_eq!(filled.volume_btc, 0.001);
    }

    #[test]
    fn live_order_chance_blocks_market_buy_when_balance_or_minimum_fails() {
        let request = build_upbit_order_request(
            &SupportedMarket::KrwBtc,
            LiveOrderIntent::MarketBuy,
            5_000,
            None,
        )
        .expect("buy request");
        let mut chance = sample_live_order_chance(&SupportedMarket::KrwBtc, 4_999.0, 1.0);
        chance.bid_min_total_krw = Some(10_000);

        let block_reasons =
            live_order_chance_submission_block_reasons(&chance, &request.preview, 5_000);

        assert!(block_reasons.contains(&LiveOrderGateBlockReason::InsufficientBalance));
        assert!(block_reasons.contains(&LiveOrderGateBlockReason::MinimumOrderAmountNotMet));
    }

    #[test]
    fn live_order_chance_blocks_when_market_order_type_is_unavailable() {
        let request = build_upbit_order_request(
            &SupportedMarket::KrwBtc,
            LiveOrderIntent::MarketSell,
            20_000,
            Some("0.001".to_string()),
        )
        .expect("sell request");
        let mut chance = sample_live_order_chance(&SupportedMarket::KrwBtc, 100_000.0, 1.0);
        chance.ask_types = vec!["limit".to_string()];

        let block_reasons =
            live_order_chance_submission_block_reasons(&chance, &request.preview, 20_000);

        assert!(block_reasons.contains(&LiveOrderGateBlockReason::MarketOrderUnavailable));
    }

    #[test]
    fn live_order_chance_status_surfaces_permission_failures_for_settings() {
        let status = build_live_order_chance_status(
            &SupportedMarket::KrwBtc,
            None,
            vec![LiveOrderGateBlockReason::OrderPermissionDenied],
            CredentialReadinessStatus::OrderPermissionMissing,
            Some("HTTP 401 out_of_scope".to_string()),
            Utc::now(),
        );

        assert!(!status.allowed);
        assert_eq!(
            status.credential_readiness,
            CredentialReadinessStatus::OrderPermissionMissing
        );
        assert!(status
            .block_reasons
            .contains(&LiveOrderGateBlockReason::OrderPermissionDenied));
        assert!(status.reason.contains("권한"));
        assert!(status.reason.contains("out_of_scope"));
    }

    #[test]
    fn credential_errors_are_classified_for_backend_status_and_gate_blocks() {
        let invalid = credential_readiness_from_error("HTTP 401 invalid_access_key jwt signature");
        let revoked = credential_readiness_from_error("HTTP 401 revoked access key");
        let permission = credential_readiness_from_error("HTTP 403 out_of_scope create_bid");
        let network = credential_readiness_from_error("timeout while fetching order chance");

        assert_eq!(invalid, CredentialReadinessStatus::InvalidKey);
        assert_eq!(revoked, CredentialReadinessStatus::RevokedKey);
        assert_eq!(
            permission,
            CredentialReadinessStatus::OrderPermissionMissing
        );
        assert_eq!(network, CredentialReadinessStatus::NetworkError);
        assert_eq!(
            live_order_block_reason_from_credential_readiness(&invalid),
            LiveOrderGateBlockReason::InvalidApiKey
        );
        assert_eq!(
            live_order_block_reason_from_credential_readiness(&revoked),
            LiveOrderGateBlockReason::RevokedApiKey
        );
        assert_eq!(
            live_order_block_reason_from_credential_readiness(&permission),
            LiveOrderGateBlockReason::OrderPermissionDenied
        );
        assert_eq!(
            live_order_block_reason_from_credential_readiness(&network),
            LiveOrderGateBlockReason::OrderChanceUnavailable
        );
    }

    #[test]
    fn real_upbit_order_submission_remains_isolated_and_unwired() {
        let source = include_str!("commands.rs");
        let order_post_endpoint = concat!(".post(\"https://api.upbit.com", "/v1/orders\")");
        let order_chance_endpoint = concat!(".get(\"https://api.upbit.com", "/v1/orders/chance\")");
        let live_executor_impl = concat!("impl LiveOrderExecutor for ", "UpbitLiveOrderExecutor");

        assert_eq!(source.matches(order_post_endpoint).count(), 1);
        assert_eq!(source.matches(order_chance_endpoint).count(), 1);
        assert_eq!(source.matches(live_executor_impl).count(), 1);
        assert!(!source.contains(concat!("async fn ", "upbit_market_buy(")));
    }

    #[test]
    fn unsupported_market_is_rejected_before_thread_can_reach_gate() {
        let parsed = serde_json::from_str::<SupportedMarket>(r#""KRW-DOGE""#);

        assert!(parsed.is_err());
    }

    fn unlocked_settings() -> AppSettings {
        AppSettings {
            notifications_enabled: false,
            notification_permission_requested: false,
            global_live_locked: false,
            strategy_logic_approved: true,
        }
    }

    fn confirmed_live_thread(now: chrono::DateTime<Utc>) -> InvestmentThread {
        let mut thread = sample_thread(now);
        thread.status = ThreadStatus::Live;
        thread.validation_status = ValidationStatus::Pass;
        apply_live_confirmation(
            &mut thread,
            REQUIRED_LIVE_CONFIRMATION_PHRASE.to_string(),
            now,
        );
        thread
    }

    fn sample_live_order_chance(
        market: &SupportedMarket,
        bid_balance: f64,
        ask_balance: f64,
    ) -> LiveOrderChance {
        LiveOrderChance {
            market: market.clone(),
            bid_currency: "KRW".to_string(),
            bid_balance,
            ask_currency: market_base_currency(market).to_string(),
            ask_balance,
            order_sides: vec!["bid".to_string(), "ask".to_string()],
            order_types: Vec::new(),
            bid_types: vec!["limit".to_string(), "price".to_string()],
            ask_types: vec!["limit".to_string(), "market".to_string()],
            bid_min_total_krw: Some(5_000),
            ask_min_total_krw: Some(5_000),
        }
    }

    fn sample_thread_purchase_log(
        thread: &InvestmentThread,
        executed_at: chrono::DateTime<Utc>,
    ) -> PurchaseLog {
        PurchaseLog {
            id: uuid::Uuid::new_v4(),
            schedule_id: uuid::Uuid::nil(),
            thread_id: Some(thread.id),
            executed_at,
            amount_krw: 20_000,
            volume_btc: 0.0001,
            status: PurchaseStatus::Success,
            error_message: None,
            source: PurchaseLogSource::InvestmentThread,
            mode: ExecutionMode::Live,
            action: PurchaseLogAction::MarketBuy,
            audit_category: AuditCategory::Trade,
            title: Some("스레드 시장가 매수".to_string()),
            reason: None,
            safety_event_id: None,
            strategy_signal_reason: Some("테스트 신호".to_string()),
            idempotency_key: None,
        }
    }

    fn sample_strategy_signal(
        thread: &InvestmentThread,
        action: PaperSignalAction,
        now: chrono::DateTime<Utc>,
    ) -> StrategySignalEvaluation {
        StrategySignalEvaluation {
            thread_id: thread.id,
            market: thread.market.clone(),
            strategy_profile: thread.strategy_profile.clone(),
            strategy_version: crate::strategy::STRATEGY_VERSION_INTRADAY_MEAN_REVERSION.to_string(),
            action,
            reason: "테스트 Paper 신호".to_string(),
            exit_reason: None,
            evaluated_at: now,
            candle_timestamp: now - chrono::Duration::minutes(30),
            price_krw: 10_000_000.0,
        }
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
            strategy_version: crate::strategy::STRATEGY_VERSION_INTRADAY_MEAN_REVERSION.to_string(),
            strategy_variant_label: "Test mean reversion".to_string(),
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
            cost_drag_krw: 120,
            fee_percent: 0.05,
            slippage_percent: 0.05,
            doubled_slippage_return_percent: return_percent - 0.5,
            round_trips: 12,
            win_rate_percent: 55.0,
            profit_factor: 1.2,
            expectancy_krw: 500.0,
            average_hold_hours: 4.0,
            exposure_percent: 12.0,
            cash_flat_return_percent: 0.0,
            stop_exit_count: 1,
            time_exit_count: 2,
            day_flat_exit_count: 3,
            reasons: vec!["테스트".to_string()],
            assumptions: vec!["테스트 가정".to_string()],
            created_at: now,
        }
    }
}
