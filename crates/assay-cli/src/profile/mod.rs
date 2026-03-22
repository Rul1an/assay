pub mod events;
pub mod generalize;
pub mod suggest;
pub mod writer;

use self::events::{BackendHint, FsOp, ProfileEvent};
use assay_evidence::types::PayloadSandboxDegraded;
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

    /// Structured containment degradations that weakened enforcement while execution continued.
    pub sandbox_degradations: Vec<PayloadSandboxDegraded>,
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

    pub fn to_evidence_profile(
        &self,
        name: &str,
        run_id: &str,
    ) -> crate::cli::commands::profile_types::Profile {
        use crate::cli::commands::profile_types::{Profile, ProfileEntry};
        use std::collections::BTreeMap;

        let mut profile = Profile::new(name, None);
        let now = chrono::Utc::now().to_rfc3339();
        profile.created_at = now.clone();
        profile.updated_at = now;
        profile.total_runs = 1;
        profile.run_ids.push_back(run_id.to_string());

        let mut file_hits: BTreeMap<String, u64> = BTreeMap::new();
        for (_, path, _) in &self.agg.fs {
            *file_hits.entry(path.clone()).or_default() += 1;
        }
        for (path, hits) in file_hits {
            profile
                .entries
                .files
                .insert(path, ProfileEntry::new(0, hits));
        }

        for (argv0, hits) in &self.agg.execs {
            profile
                .entries
                .processes
                .insert(argv0.clone(), ProfileEntry::new(0, *hits));
        }

        profile.sandbox_degradations = self.agg.sandbox_degradations.clone();
        profile
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
            ProfileEvent::SandboxDegraded { payload } => {
                let exists = agg.sandbox_degradations.iter().any(|existing| {
                    existing.component == payload.component
                        && existing.reason_code == payload.reason_code
                });
                if !exists {
                    agg.sandbox_degradations.push(payload);
                    agg.sandbox_degradations.sort();
                }
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
        let c = ProfileCollector::new(cfg);

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

    #[test]
    fn collector_suppresses_duplicate_sandbox_degradations_per_component_reason() {
        let cfg = ProfileConfig {
            cwd: PathBuf::from("/repo"),
            home: None,
            assay_tmp: None,
        };
        let c = ProfileCollector::new(cfg);

        let payload = PayloadSandboxDegraded {
            reason_code: assay_evidence::types::SandboxDegradationReasonCode::PolicyConflict,
            degradation_mode: assay_evidence::types::SandboxDegradationMode::AuditFallback,
            component: assay_evidence::types::SandboxDegradationComponent::Landlock,
            detail: Some("safe-detail".into()),
        };

        c.record(ProfileEvent::SandboxDegraded {
            payload: payload.clone(),
        });
        c.record(ProfileEvent::SandboxDegraded { payload });

        let report = c.finish();
        assert_eq!(report.agg.sandbox_degradations.len(), 1);
    }

    #[test]
    fn evidence_profile_keeps_single_run_cardinality_for_repeated_fs_paths() {
        let cfg = ProfileConfig {
            cwd: PathBuf::from("/repo"),
            home: None,
            assay_tmp: None,
        };
        let c = ProfileCollector::new(cfg);

        c.record(ProfileEvent::FsObserved {
            op: FsOp::Read,
            path: "/repo/data.txt".into(),
            backend: BackendHint::Injected,
        });
        c.record(ProfileEvent::FsObserved {
            op: FsOp::Read,
            path: "/repo/data.txt".into(),
            backend: BackendHint::Injected,
        });

        let report = c.finish();
        let profile = report.to_evidence_profile("sandbox", "run123");
        let entry = profile
            .entries
            .files
            .get("/repo/data.txt")
            .expect("expected file entry");

        assert_eq!(entry.runs_seen, 1);
        assert_eq!(entry.hits_total, 2);
    }

    fn normalize(s: &str) -> String {
        s.replace("\r\n", "\n")
    }
}
