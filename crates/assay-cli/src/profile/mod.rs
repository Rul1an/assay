pub mod events;
pub mod generalize;
pub mod suggest;
pub mod writer;

use self::events::{BackendHint, FsOp, ProfileEvent};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Configuration for the profile collector.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    pub assay_tmp: Option<PathBuf>,
}

/// Aggregated profile data state (deterministic).
#[derive(Debug, Clone, Default)]
pub struct ProfileAgg {
    /// event_name -> count
    pub counters: BTreeMap<String, u64>,

    /// env keys provided (keys only) -> count
    /// Map<Key, Count>
    pub env_provided: BTreeMap<String, u64>,

    /// Exec paths observed -> count
    pub execs: BTreeMap<String, u64>,

    /// FS operations (unprocessed raw paths)
    /// We keep them raw here; generalization happens in suggest step.
    /// Vector is sorted deterministically at finish time if needed,
    /// but usually we just collect and then generalize into Sets.
    pub fs: Vec<(FsOp, String, BackendHint)>,

    /// Degradations / Warnings
    pub notes: Vec<String>,
}

/// Finished profile report ready for suggestion generation.
#[derive(Debug, Clone)]
pub struct ProfileReport {
    pub version: u32,
    pub config: ProfileConfig,
    pub agg: ProfileAgg,
}

impl ProfileReport {
    pub fn to_suggestion(&self, cfg: suggest::SuggestConfig) -> suggest::PolicySuggestion {
        suggest::build_policy_suggestion(self, cfg)
    }
}

use std::sync::{Arc, Mutex};

/// Collector for profiling events.
///
/// NOTE: This is thread-safe and can be shared across async tasks or threads.
#[derive(Debug, Clone)]
pub struct ProfileCollector {
    cfg: ProfileConfig,
    agg: Arc<Mutex<ProfileAgg>>,
}

impl ProfileCollector {
    pub fn new(cfg: ProfileConfig) -> Self {
        Self {
            cfg,
            agg: Arc::new(Mutex::new(ProfileAgg::default())),
        }
    }

    pub fn record(&self, ev: ProfileEvent) {
        let mut agg = self.agg.lock().unwrap_or_else(|e| e.into_inner());
        match ev {
            ProfileEvent::Counter { name, inc } => {
                *agg.counters.entry(name).or_default() += inc;
            }
            ProfileEvent::EnvProvidedKeys { key, scrubbed: _ } => {
                *agg.env_provided.entry(key).or_default() += 1;
            }
            ProfileEvent::ExecObserved { argv0 } => {
                *agg.execs.entry(argv0).or_default() += 1;
            }
            ProfileEvent::FsObserved { op, path, backend } => {
                agg.fs.push((op, path, backend));
            }
            ProfileEvent::AuditFallback { reason, detail: _ } => {
                agg.notes.push(format!("audit_fallback: {}", reason));
                *agg.counters
                    .entry("sandbox.audit_fallback".to_string())
                    .or_default() += 1;
            }
            ProfileEvent::EnforcementFailed { reason, detail: _ } => {
                agg.notes.push(format!("enforcement_failed: {}", reason));
                *agg.counters
                    .entry("sandbox.fail_closed_triggered".to_string())
                    .or_default() += 1;
            }
        }
    }

    pub fn note<S: Into<String>>(&self, s: S) {
        let mut agg = self.agg.lock().unwrap_or_else(|e| e.into_inner());
        agg.notes.push(s.into());
    }

    pub fn finish(self) -> ProfileReport {
        let agg = Arc::try_unwrap(self.agg)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_else(|a| a.lock().unwrap_or_else(|e| e.into_inner()).clone());

        ProfileReport {
            version: 1,
            config: self.cfg,
            agg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::events::{BackendHint, FsOp, ProfileEvent};
    use std::path::PathBuf;

    #[test]
    fn golden_profile_yaml_stable() {
        let cfg = ProfileConfig {
            cwd: PathBuf::from("/repo"),
            home: Some(PathBuf::from("/home/u")),
            assay_tmp: Some(PathBuf::from("/tmp/assay-1000-999")),
        };
        let mut c = ProfileCollector::new(cfg);

        c.record(ProfileEvent::Counter {
            name: "sandbox.env_strict_used".into(),
            inc: 1,
        });
        c.record(ProfileEvent::EnvProvidedKeys {
            key: "FOO_FEATURE".into(),
            scrubbed: false,
        });
        c.record(ProfileEvent::ExecObserved {
            argv0: "/usr/bin/cat".into(),
        });
        c.record(ProfileEvent::FsObserved {
            op: FsOp::Read,
            path: "/repo/data/input.txt".into(),
            backend: BackendHint::Injected,
        });
        c.record(ProfileEvent::AuditFallback {
            reason: "landlock policy conflict".into(),
            detail: None,
        });

        let report = c.finish();
        let sugg = report.to_suggestion(crate::profile::suggest::SuggestConfig {
            widen_dirs_to_glob: false,
        });
        let got = crate::profile::writer::write_yaml(&sugg);

        let expected = include_str!("../../tests/golden/profile_basic.yaml");
        assert_eq!(normalize(&got), normalize(expected));
    }

    fn normalize(s: &str) -> String {
        s.replace("\r\n", "\n")
    }
}
