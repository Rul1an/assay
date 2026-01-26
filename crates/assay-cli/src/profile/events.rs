/// Profile events tracked during sandbox execution.
#[derive(Debug, Clone)]
pub enum ProfileEvent {
    /// Generic counter (e.g., "sandbox.failsafe_triggered")
    Counter { name: String, inc: u64 },

    /// Environment variable provided (after filtering)
    EnvProvided {
        key: String,
        /// Whether the value was scrubbed/masked
        scrubbed: bool,
    },

    /// Command execution observed (argv[0])
    ExecObserved { argv0: String },

    /// Filesystem operation observed
    FsObserved {
        op: FsOp,
        path: String,
        /// How this event was detected (Injected, Landlock, Ptrace...)
        backend: BackendHint,
    },

    /// Degradation event (e.g., Landlock conflict)
    Degraded {
        reason: String,
        /// Detailed context (may be redacted later)
        detail: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FsOp {
    Read,
    Write,
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BackendHint {
    /// Injected via test hook
    Injected,
    /// Detected via Landlock violation
    Landlock,
    /// Detected via Ptrace syscall intercept
    Ptrace,
    /// Detected via eBPF probe
    Ebpf,
}

impl FsOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            FsOp::Read => "read",
            FsOp::Write => "write",
            FsOp::Exec => "exec",
        }
    }
}

// Optional parsing logic for test hooks
#[cfg(any(test, feature = "profile-test-hook"))]
pub fn try_load_test_events() -> Option<Vec<ProfileEvent>> {
    use std::env;
    let raw = env::var("ASSAY_PROFILE_TEST_EVENTS").ok()?;

    // Simple JSON parsing logic - implementing manually to avoid pulling serde for this tiny hook
    // unless we decide to make ProfileEvent Deserialize (which is fine too).
    // For now, let's use serde_json::Value as an intermediate if available, or just strict parsing.
    // Given we depend on serde for policy, we can use serde_json here.

    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let arr = v.as_array()?;

    let mut out = Vec::new();

    for item in arr {
        let obj = item.as_object()?;

        if let Some(c) = obj.get("Counter") {
            let c = c.as_object()?;
            out.push(ProfileEvent::Counter {
                name: c.get("name")?.as_str()?.to_string(),
                inc: c.get("inc")?.as_u64()?,
            });
        } else if let Some(e) = obj.get("EnvProvided") {
            let e = e.as_object()?;
            out.push(ProfileEvent::EnvProvided {
                key: e.get("key")?.as_str()?.to_string(),
                scrubbed: e.get("scrubbed")?.as_bool().unwrap_or(false),
            });
        } else if let Some(e) = obj.get("ExecObserved") {
            let e = e.as_object()?;
            out.push(ProfileEvent::ExecObserved {
                argv0: e.get("argv0")?.as_str()?.to_string(),
            });
        } else if let Some(f) = obj.get("FsObserved") {
            let f = f.as_object()?;
            let op_str = f.get("op")?.as_str()?;
            let op = match op_str {
                "Read" => FsOp::Read,
                "Write" => FsOp::Write,
                "Exec" => FsOp::Exec,
                _ => continue,
            };
            out.push(ProfileEvent::FsObserved {
                op,
                path: f.get("path")?.as_str()?.to_string(),
                backend: BackendHint::Injected,
            });
        } else if let Some(d) = obj.get("Degraded") {
            let d = d.as_object()?;
            out.push(ProfileEvent::Degraded {
                reason: d.get("reason")?.as_str()?.to_string(),
                detail: d.get("detail").and_then(|v| v.as_str()).map(String::from),
            });
        }
    }

    Some(out)
}
