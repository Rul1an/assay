// Serializable export types for incident bundles

use serde::Serialize;
use std::collections::HashMap;
use std::string::String;
use std::vec::Vec;
use std::option::Option; // Option is usually in core prelude, but to be safe. Actually Option/Result are usually available.
// String and Vec are the main ones missing in no_std default.

/// Exported process node (from ProcessTreeTracker)
#[derive(Debug, Clone, Serialize)]
pub struct ProcessNodeExport {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub children: Vec<u32>,
    pub exe: Option<String>,
    pub cmdline: Option<String>,
    pub cwd: Option<String>,
    pub state: ProcessStateExport,
    pub depth: u32,
}

/// Process state for export
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStateExport {
    Running,
    Exited,
    Killed,
}

/// Exported process tree
#[derive(Debug, Clone, Serialize)]
#[derive(Default)]
pub struct ProcessTreeExport {
    /// Root PIDs (explicitly monitored)
    pub roots: Vec<u32>,

    /// All nodes in the tree
    pub nodes: HashMap<u32, ProcessNodeExport>,

    /// Total count of nodes
    pub total_count: usize,
}



/// Kill result export (from kill_tree)
#[derive(Debug, Clone, Serialize)]
pub struct KillResultExport {
    /// PIDs successfully killed
    pub killed: Vec<u32>,

    /// PIDs that failed to kill
    pub failed: Vec<KillFailureExport>,

    /// Total attempted
    pub attempted: usize,

    /// Overall success
    pub success: bool,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Kill order used
    pub order: String,

    /// Kill mode used
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KillFailureExport {
    pub pid: u32,
    pub error: String,
    pub retries: u32,
}

/// Event record for incident bundles
#[derive(Debug, Clone, Serialize)]
pub struct EventRecordExport {
    /// ISO timestamp
    pub timestamp: String,

    /// Process ID
    pub pid: u32,

    /// Event type name
    pub event_type: String,

    /// Event-specific details
    pub details: serde_json::Value,
}

impl EventRecordExport {
    /// Create from a decoded event.
    /// Note: This simplifies the previous implementation by avoiding circular dependency on super::events.
    /// The caller is responsible for converting their specific Event enum to this strict output format.
    pub fn new(
        pid: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
        event_type: String,
        details: serde_json::Value
    ) -> Self {
        Self {
            timestamp: timestamp.to_rfc3339(),
            pid,
            event_type,
            details,
        }
    }
}
