use std::env;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub timeout_ms: u64,
    pub max_msg_bytes: usize,
    pub max_tool_calls: usize,
    pub max_field_bytes: usize,
    pub cache_entries: u64,
    pub log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 2000,
            max_msg_bytes: 1_000_000,
            max_tool_calls: 2000,
            max_field_bytes: 64_000,
            cache_entries: 128,
            log_level: "info".to_string(),
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = env::var("ASSAY_MCP_TIMEOUT_MS") {
            if let Ok(n) = v.parse() {
                cfg.timeout_ms = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_msg_bytes = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_FIELD_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_field_bytes = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_TOOL_CALLS") {
            if let Ok(n) = v.parse() {
                cfg.max_tool_calls = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_CACHE_ENTRIES") {
            if let Ok(n) = v.parse() {
                cfg.cache_entries = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_LOG") {
            cfg.log_level = v;
        }
        cfg
    }
}
