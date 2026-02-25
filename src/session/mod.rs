mod active;
mod management;
mod store;

pub use active::{get_active, resolve, set_active};
pub use management::{clear_all, delete, list, resolve_prefix, stats};
pub use store::Session;

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::{
    ACTIVE_SESSION_FILE, HISTORY_FILE, META_FILE, POOL_FILE, SESSIONS_SUBDIR,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionHeader {
    pub created_at:         String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_at_exchange: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Exchange {
    pub message_id: String,
    pub ts:         String,
    pub role:       String,
    pub content:    String,
    #[serde(default)]
    pub meta:       ExchangeMeta,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ExchangeMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model:       Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage:       Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens:  Option<u32>,
    pub output_tokens: Option<u32>,
}

impl Exchange {
    pub fn now(role: &str, content: String, meta: ExchangeMeta) -> Self {
        Self {
            message_id: Uuid::new_v4().to_string(),
            ts:         Utc::now().to_rfc3339(),
            role:       role.into(),
            content,
            meta,
        }
    }
}

pub struct SessionInfo {
    pub id:                 String,
    pub is_active:          bool,
    pub created_at:         Option<String>,
    pub updated_at:         String,
    pub forked_from:        Option<String>,
    pub forked_at_exchange: Option<String>,
    pub tokens_in:          u32,
    pub tokens_out:         u32,
    pub duration_ms:        u64,
}

pub struct ExchangeStats {
    pub exchange_id:   Option<String>,
    pub ts:            String,
    pub model:         String,
    pub input_tokens:  u32,
    pub output_tokens: u32,
    pub duration_ms:   Option<u64>,
}

pub struct SessionStats {
    pub exchanges:    Vec<ExchangeStats>,
    pub total_input:  u32,
    pub total_output: u32,
    pub total_dur_ms: u64,
}

fn sessions_dir(data_dir: &Path) -> PathBuf { data_dir.join(SESSIONS_SUBDIR) }
fn active_path(data_dir: &Path) -> PathBuf  { data_dir.join(ACTIVE_SESSION_FILE) }
fn session_dir(data_dir: &Path, id: &str) -> PathBuf { sessions_dir(data_dir).join(id) }
fn history_path(dir: &Path) -> PathBuf { dir.join(HISTORY_FILE) }
fn meta_path(dir: &Path) -> PathBuf    { dir.join(META_FILE) }
#[allow(dead_code)]
fn pool_path(dir: &Path) -> PathBuf    { dir.join(POOL_FILE) }
