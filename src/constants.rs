// SSE protocol
pub const SSE_DATA_PREFIX: &str = "data: ";
pub const SSE_DONE:        &str = "[DONE]";

// Timeouts (seconds)
pub const TIMEOUT_CHAT_SECS:   u64 = 120;
pub const TIMEOUT_MODELS_SECS: u64 = 30;

// Session
pub const ENV_SESSION:           &str  = "ZURO_SESSION";
pub const SESSIONS_SUBDIR:       &str  = "sessions";
pub const ACTIVE_SESSION_FILE:   &str  = "active_session";
pub const SESSION_ID_PREFIX_LEN: usize = 8;
pub const MESSAGE_ID_PREFIX_LEN: usize = 8;
pub const META_FILE:             &str  = "meta.json";
pub const HISTORY_FILE:          &str  = "history.jsonl";
#[allow(dead_code)]
pub const POOL_FILE:             &str  = "pool.json";

// .zuro directory structure
pub const ZURO_DIR:            &str = ".zuro";
pub const COMMANDS_SUBDIR:     &str = "commands";
pub const MEMORY_FILE:         &str = "memory.md";
pub const MEMORY_LOCAL_FILE:   &str = "memory.local.md";
pub const SYSTEM_FILE:         &str = "system.md";
pub const POOL_PROJECT_FILE:   &str = "pool.json";
pub const POOL_LOCAL_FILE:     &str = "pool.local.json";
pub const PROJECT_CONFIG_FILE: &str = "config.toml";

// RFC 3339 timestamp slicing
pub const DATETIME_DISPLAY_LEN: usize = 16; // "YYYY-MM-DDTHH:MM"
pub const RFC3339_SECONDS_LEN:  usize = 19; // "YYYY-MM-DDTHH:MM:SS"
pub const RFC3339_TIME_OFFSET:  usize = 11; // length of "YYYY-MM-DDT"

// Output table — stats
pub const TABLE_WIDTH:    usize = 88;
pub const COL_EXCHANGE_ID: usize = MESSAGE_ID_PREFIX_LEN + 2;
pub const COL_TIMESTAMP:  usize = 22;
pub const COL_MODEL:      usize = 24;
pub const COL_TOKENS:     usize = 6;
pub const COL_DURATION:   usize = 10;
