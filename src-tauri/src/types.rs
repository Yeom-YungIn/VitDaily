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
    pub executed_at: DateTime<Utc>,
    pub amount_krw: u64,
    pub volume_btc: f64,
    pub status: PurchaseStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PurchaseStatus {
    Success,
    Failure,
    Blocked,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    pub connected: bool,
    pub has_credentials: bool,
    pub error: Option<String>,
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
