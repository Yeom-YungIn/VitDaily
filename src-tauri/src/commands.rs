use crate::types::{ApiStatus, PurchaseLog, PurchaseStatus, Schedule};
use chrono::{Local, Timelike, Utc};
use keyring::Entry;
use tauri::command;
use tokio::time::{interval, Duration};

const KEYRING_SERVICE: &str = "vitdaily";
const KEYRING_ACCESS_KEY: &str = "upbit_access_key";
const KEYRING_SECRET_KEY: &str = "upbit_secret_key";

// --- API Credentials ---

#[command]
pub async fn save_api_credentials(
    access_key: String,
    secret_key: String,
) -> Result<(), String> {
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
    let _ = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)
        .and_then(|e| e.delete_credential());
    let _ = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)
        .and_then(|e| e.delete_credential());
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

// --- Schedules ---

#[command]
pub async fn get_schedules() -> Result<Vec<Schedule>, String> {
    load_schedules().map_err(|e| e.to_string())
}

#[command]
pub async fn save_schedule(schedule: Schedule) -> Result<Vec<Schedule>, String> {
    if schedule.amount < 5_000 {
        return Err("최소 주문 금액은 5,000원입니다".to_string());
    }

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
    let uuid = id.parse::<uuid::Uuid>().map_err(|_| "잘못된 ID".to_string())?;
    let mut schedules = load_schedules().map_err(|e| e.to_string())?;
    schedules.retain(|s| s.id != uuid);
    persist_schedules(&schedules).map_err(|e| e.to_string())?;
    Ok(schedules)
}

#[command]
pub async fn toggle_schedule(id: String) -> Result<Vec<Schedule>, String> {
    let uuid = id.parse::<uuid::Uuid>().map_err(|_| "잘못된 ID".to_string())?;
    let mut schedules = load_schedules().map_err(|e| e.to_string())?;
    if let Some(schedule) = schedules.iter_mut().find(|s| s.id == uuid) {
        schedule.enabled = !schedule.enabled;
        schedule.updated_at = Utc::now();
    }
    persist_schedules(&schedules).map_err(|e| e.to_string())?;
    Ok(schedules)
}

pub async fn run_scheduler() {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;
        if let Err(error) = execute_due_schedules().await {
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
    let changed = schedules
        .iter_mut()
        .any(|schedule| schedule.apply_due_pending_change(now));

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

async fn execute_due_schedules() -> anyhow::Result<()> {
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
            log.schedule_id == schedule.id && log.executed_at.with_timezone(&Local).date_naive() == today
        });

        if already_executed {
            continue;
        }

        logs.push(execute_market_buy(schedule).await);
        changed = true;
    }

    if changed {
        persist_logs(&logs)?;
    }

    Ok(())
}

async fn execute_market_buy(schedule: &Schedule) -> PurchaseLog {
    let executed_at = Utc::now();
    let result = get_credentials().map_err(|err| err.to_string());

    let order_result = match result {
        Ok((access_key, secret_key)) => {
            upbit_market_buy(&access_key, &secret_key, schedule.amount).await
        }
        Err(err) => Err(err),
    };

    match order_result {
        Ok(volume_btc) => PurchaseLog {
            id: uuid::Uuid::new_v4(),
            schedule_id: schedule.id,
            executed_at,
            amount_krw: schedule.amount,
            volume_btc,
            status: PurchaseStatus::Success,
            error_message: None,
        },
        Err(error) => PurchaseLog {
            id: uuid::Uuid::new_v4(),
            schedule_id: schedule.id,
            executed_at,
            amount_krw: schedule.amount,
            volume_btc: 0.0,
            status: PurchaseStatus::Failure,
            error_message: Some(error),
        },
    }
}

fn get_credentials() -> anyhow::Result<(String, String)> {
    let access_key = Entry::new(KEYRING_SERVICE, KEYRING_ACCESS_KEY)?.get_password()?;
    let secret_key = Entry::new(KEYRING_SERVICE, KEYRING_SECRET_KEY)?.get_password()?;
    Ok((access_key, secret_key))
}

async fn upbit_check_balance(access_key: &str, secret_key: &str) -> Result<(), String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Claims {
        access_key: String,
        nonce: String,
    }

    let nonce = uuid::Uuid::new_v4().to_string();
    let claims = Claims {
        access_key: access_key.to_string(),
        nonce,
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
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
        Ok(())
    } else {
        let status = resp.status().as_u16();
        Err(format!("업비트 API 오류: HTTP {status}"))
    }
}

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
        &Header::new(Algorithm::HS256),
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

    let value = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(value
        .get("executed_volume")
        .and_then(|volume| volume.as_str())
        .and_then(|volume| volume.parse::<f64>().ok())
        .unwrap_or(0.0))
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
