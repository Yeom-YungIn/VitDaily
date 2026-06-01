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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    pub connected: bool,
    pub has_credentials: bool,
    pub error: Option<String>,
}
