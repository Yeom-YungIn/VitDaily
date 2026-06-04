use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: Uuid,
    pub time: String,
    pub amount: u64,
    pub enabled: bool,
    pub pending_change: Option<PendingChange>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Schedule {
    pub fn apply_due_pending_change(&mut self, now: DateTime<Utc>) -> bool {
        let Some(change) = self.pending_change.clone() else {
            return false;
        };

        if change.apply_at > now {
            return false;
        }

        self.time = change.time;
        self.amount = change.amount;
        self.pending_change = None;
        self.updated_at = now;
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    pub time: String,
    pub amount: u64,
    pub apply_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLog {
    pub id: Uuid,
    pub schedule_id: Uuid,
    #[serde(default)]
    pub thread_id: Option<Uuid>,
    pub executed_at: DateTime<Utc>,
    pub amount_krw: u64,
    pub volume_btc: f64,
    pub status: PurchaseStatus,
    pub error_message: Option<String>,
    #[serde(default = "default_purchase_log_source")]
    pub source: PurchaseLogSource,
    #[serde(default = "default_purchase_log_mode")]
    pub mode: ExecutionMode,
    #[serde(default = "default_purchase_log_action")]
    pub action: PurchaseLogAction,
    #[serde(default)]
    pub audit_category: AuditCategory,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub safety_event_id: Option<Uuid>,
    #[serde(default)]
    pub strategy_signal_reason: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PurchaseStatus {
    Submitted,
    Filled,
    Failed,
    Success,
    Failure,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseLogSource {
    LegacySchedule,
    InvestmentThread,
    System,
}

fn default_purchase_log_source() -> PurchaseLogSource {
    PurchaseLogSource::LegacySchedule
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Live,
    Paper,
    System,
}

fn default_purchase_log_mode() -> ExecutionMode {
    ExecutionMode::Live
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseLogAction {
    MarketBuy,
    MarketSell,
    SafetyCheck,
}

fn default_purchase_log_action() -> PurchaseLogAction {
    PurchaseLogAction::MarketBuy
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    Trade,
    PaperTrade,
    BlockedOrder,
    ApiFailure,
    SafetyGate,
    Validation,
    Schedule,
}

impl Default for AuditCategory {
    fn default() -> Self {
        Self::Trade
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    pub connected: bool,
    pub has_credentials: bool,
    pub credential_readiness: CredentialReadinessStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialReadinessStatus {
    Missing,
    StoredUnchecked,
    Connected,
    InvalidKey,
    RevokedKey,
    OrderPermissionMissing,
    NetworkError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOrderChanceStatus {
    pub allowed: bool,
    pub market: SupportedMarket,
    pub bid_currency: String,
    pub bid_balance: f64,
    pub ask_currency: String,
    pub ask_balance: f64,
    pub minimum_bid_total_krw: Option<u64>,
    pub minimum_ask_total_krw: Option<u64>,
    pub market_buy_supported: bool,
    pub market_sell_supported: bool,
    pub credential_readiness: CredentialReadinessStatus,
    pub block_reasons: Vec<LiveOrderGateBlockReason>,
    pub reason: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSnapshot {
    pub btc_balance: f64,
    pub btc_locked: f64,
    pub btc_total: f64,
    pub btc_price_krw: f64,
    pub btc_value_krw: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioTimePoint {
    pub date: String,
    pub invested_krw: u64,
    pub estimated_value_krw: u64,
    pub return_percent: f64,
    pub drawdown_percent: f64,
    pub source: PortfolioPointSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortfolioPointSource {
    Local,
    Simulated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioAllocation {
    pub market: SupportedMarket,
    pub budget_krw: u64,
    pub share_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSummary {
    pub total_budget_krw: u64,
    pub invested_krw: u64,
    pub current_value_krw: u64,
    pub return_percent: f64,
    pub max_drawdown_percent: f64,
    pub successful_buys: u32,
    pub blocked_orders: u32,
    pub safety_events: u32,
    pub latest_point_source: Option<PortfolioPointSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAnalytics {
    pub thread_id: Uuid,
    pub thread_name: String,
    pub market: SupportedMarket,
    pub budget_krw: u64,
    pub validation_status: ValidationStatus,
    pub return_percent: Option<f64>,
    pub max_drawdown_percent: Option<f64>,
    pub baseline_dca_return_percent: Option<f64>,
    pub simulated_trades: Option<u32>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioAnalytics {
    pub summary: PortfolioSummary,
    pub time_series: Vec<PortfolioTimePoint>,
    pub allocations: Vec<PortfolioAllocation>,
    pub threads: Vec<ThreadAnalytics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub notifications_enabled: bool,
    #[serde(default)]
    pub notification_permission_requested: bool,
    #[serde(default = "default_global_live_locked")]
    pub global_live_locked: bool,
    #[serde(default)]
    pub strategy_logic_approved: bool,
}

fn default_global_live_locked() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: false,
            notification_permission_requested: false,
            global_live_locked: true,
            strategy_logic_approved: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupportedMarket {
    #[serde(rename = "KRW-BTC")]
    KrwBtc,
    #[serde(rename = "KRW-ETH")]
    KrwEth,
    #[serde(rename = "KRW-XRP")]
    KrwXrp,
}

impl SupportedMarket {
    pub fn all() -> Vec<Self> {
        vec![Self::KrwBtc, Self::KrwEth, Self::KrwXrp]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StrategyProfile {
    Stable,
    Conservative,
    Aggressive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThreadStatus {
    Draft,
    Paper,
    Armed,
    Live,
    Paused,
    Stopped,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    Missing,
    Running,
    Pass,
    Fail,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveOrderFinalConfirmationStatus {
    Missing,
    Confirmed,
}

impl Default for LiveOrderFinalConfirmationStatus {
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveOrderGateSource {
    LegacySchedule,
    InvestmentThread,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveOrderGateBlockReason {
    GlobalLiveLocked,
    CredentialsMissing,
    StrategyLogicNotApproved,
    FinalConfirmationMissing,
    LiveModeNotEnabled,
    DailyTradeCapExceeded,
    MaxLossExceeded,
    SupportedMarketRequired,
    ValidationMissing,
    ValidationNotPassed,
    LegacyScheduleNotMigrated,
    SettingsUnavailable,
    AuditDataUnavailable,
    InvalidApiKey,
    RevokedApiKey,
    InsufficientBalance,
    MinimumOrderAmountNotMet,
    MarketOrderUnavailable,
    OrderPermissionDenied,
    OrderChanceUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LegacyScheduleLivePolicy {
    BlockedUseInvestmentThread,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOrderGateCheck {
    pub source: LiveOrderGateSource,
    pub thread_id: Option<Uuid>,
    pub related_schedule_id: Option<Uuid>,
    pub market: SupportedMarket,
    pub intent: Option<LiveOrderIntent>,
    pub amount_krw: u64,
    pub final_confirmation_status: LiveOrderFinalConfirmationStatus,
    pub daily_trade_count: u32,
    pub daily_trade_cap: u32,
    pub max_loss_percent: Option<f64>,
    pub latest_max_drawdown_percent: Option<f64>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOrderGateDecision {
    pub allowed: bool,
    pub check: LiveOrderGateCheck,
    pub block_reasons: Vec<LiveOrderGateBlockReason>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyScheduleLivePolicyStatus {
    pub schedule_id: Uuid,
    pub enabled: bool,
    pub time: String,
    pub amount_krw: u64,
    pub policy: LegacyScheduleLivePolicy,
    pub live_order_allowed: bool,
    pub live_order_gate: LiveOrderGateDecision,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivationRequest {
    pub thread_id: Uuid,
    pub confirmation_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveMarketSellRequest {
    pub thread_id: Uuid,
    pub volume: String,
    pub estimated_amount_krw: Option<u64>,
    pub policy_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveOrderIntent {
    MarketBuy,
    MarketSell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpbitOrderPayloadPreview {
    pub market: SupportedMarket,
    pub side: String,
    pub ord_type: String,
    pub price: Option<String>,
    pub volume: Option<String>,
    pub identifier: String,
    pub query_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaperSignalAction {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategySignalEvaluation {
    pub thread_id: Uuid,
    pub market: SupportedMarket,
    pub strategy_profile: StrategyProfile,
    pub action: PaperSignalAction,
    pub reason: String,
    pub evaluated_at: DateTime<Utc>,
    pub candle_timestamp: DateTime<Utc>,
    pub price_krw: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperExecutionResult {
    pub thread_id: Uuid,
    pub signal: StrategySignalEvaluation,
    pub live_order_gate: LiveOrderGateDecision,
    pub idempotency_key: String,
    pub duplicate: bool,
    pub log: Option<PurchaseLog>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadAutoLoopMode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadAutoLoopAction {
    PaperTick,
    LiveMarketBuySubmitted,
    LiveGateBlocked,
    DuplicateTick,
    RetryLimited,
    Hold,
    SellSkipped,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAutoLoopResult {
    pub thread_id: Uuid,
    pub mode: ThreadAutoLoopMode,
    pub action: ThreadAutoLoopAction,
    pub message: String,
    pub idempotency_key: Option<String>,
    pub retry_count: u32,
    pub paper_result: Option<PaperExecutionResult>,
    pub live_order_gate: Option<LiveOrderGateDecision>,
    pub logs: Vec<PurchaseLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentThread {
    pub id: Uuid,
    pub name: String,
    pub market: SupportedMarket,
    pub initial_budget_krw: u64,
    pub duration_days: u32,
    pub strategy_profile: StrategyProfile,
    pub max_loss_percent: f64,
    pub daily_trade_cap: u32,
    pub status: ThreadStatus,
    pub validation_status: ValidationStatus,
    #[serde(default)]
    pub final_confirmation_status: LiveOrderFinalConfirmationStatus,
    #[serde(default)]
    pub final_confirmation_text: Option<String>,
    #[serde(default)]
    pub final_confirmed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadValidationResult {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub status: ValidationStatus,
    pub period_days: u32,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub market: SupportedMarket,
    pub strategy_profile: StrategyProfile,
    pub simulated_trades: u32,
    pub return_percent: f64,
    pub max_drawdown_percent: f64,
    pub baseline_dca_return_percent: f64,
    pub baseline_dca_max_drawdown_percent: f64,
    pub baseline_buy_hold_return_percent: f64,
    pub baseline_buy_hold_max_drawdown_percent: f64,
    pub recent_90d_return_percent: f64,
    pub recent_90d_dca_return_percent: f64,
    pub fees_krw: u64,
    pub fee_percent: f64,
    pub slippage_percent: f64,
    pub doubled_slippage_return_percent: f64,
    pub reasons: Vec<String>,
    pub assumptions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyEventType {
    Blocked,
    Warning,
    Stopped,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyEvent {
    pub id: Uuid,
    pub thread_id: Option<Uuid>,
    pub event_type: SafetyEventType,
    pub message: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub category: AuditCategory,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub related_schedule_id: Option<Uuid>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyProfileInfo {
    pub profile: StrategyProfile,
    pub title: String,
    pub risk_label: String,
    pub trade_frequency: String,
    pub indicators: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageEnvelope<T> {
    pub schema_version: u32,
    pub data: T,
}

impl<T> StorageEnvelope<T> {
    pub fn new(data: T) -> Self {
        Self {
            schema_version: 1,
            data,
        }
    }
}
