use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    FileOpen {
        path: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
    NetConnect {
        dest: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
    ProcExec {
        path: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
}
