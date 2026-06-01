use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    pub connected: bool,
    pub has_credentials: bool,
    pub error: Option<String>,
}
