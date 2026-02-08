use crate::cli::commands::profile_types::{Profile, ProfileEntry};
use anyhow::Result;
use assay_evidence::types::EvidenceEvent;
use chrono::{DateTime, Utc};

/// Level of detail to include in the evidence bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum DetailLevel {
    /// Only control events (start/finish) and summary counts.
    Summary,
    /// (Default) Include observed events (fs, net, exec) with redaction.
    #[default]
    Observed,
    /// Include full debug details (local only, NOT for compliance storage).
    Full,
}

/// Maps a Profile to a sequence of EvidenceEvents.
pub struct EvidenceMapper {
    run_id: String,
    producer_version: String,
    seq: u64,
}

impl EvidenceMapper {
    pub fn new(run_id_input: Option<String>, profile_name: &str) -> Self {
        let run_id = if let Some(id) = run_id_input {
            id
        } else {
            // Deterministic fallback: run_<sha256(name)>[..16]
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(profile_name.as_bytes());
            let hash = hex::encode(hasher.finalize());
            assert!(hash.len() >= 16, "SHA256 hex hash too short");
            format!("run_{}", &hash[..16])
        };

        Self {
            run_id,
            producer_version: env!("CARGO_PKG_VERSION").to_string(),
            seq: 0,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn map_profile(
        &mut self,
        profile: &Profile,
        detail: DetailLevel,
    ) -> Result<Vec<EvidenceEvent>> {
        let mut events = Vec::new();

        // One stable export-run timestamp (anchored to profile for determinism)
        let export_time = DateTime::parse_from_rfc3339(&profile.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        events.push(self.create_event(
            "assay.profile.started",
            "urn:assay:phase:profile",
            serde_json::json!({
                "profile_name": profile.name,
                "profile_version": profile.version,
                "total_runs_aggregated": profile.total_runs,
            }),
            export_time,
        ));

        // 2. Observed Events (if requested)
        if detail != DetailLevel::Summary {
            // Note: BTreeMap keys are already sorted in Rust.
            // We just need to ensure we map them in a stable way.
            self.map_entries(
                &mut events,
                &profile.entries.files,
                "assay.fs.access",
                "file",
                detail,
                export_time,
            );
            self.map_entries(
                &mut events,
                &profile.entries.network,
                "assay.net.connect",
                "host",
                detail,
                export_time,
            );
            self.map_entries(
                &mut events,
                &profile.entries.processes,
                "assay.process.exec",
                "cmd",
                detail,
                export_time,
            );
        }

        events.push(self.create_event(
            "assay.profile.finished",
            "urn:assay:phase:profile",
            serde_json::json!({
                "files_count": profile.entries.files.len(),
                "network_count": profile.entries.network.len(),
                "processes_count": profile.entries.processes.len(),
                "integrity_scope": profile.scope,
            }),
            export_time,
        ));

        Ok(events)
    }

    fn map_entries(
        &mut self,
        events: &mut Vec<EvidenceEvent>,
        entries: &std::collections::BTreeMap<String, ProfileEntry>,
        event_type: &str,
        subject_key: &str,
        detail: DetailLevel,
        export_time: DateTime<Utc>,
    ) {
        // Entries is a BTreeMap, so it's already sorted by key (path/host/cmd).
        for (key, entry) in entries {
            let subject: String = if detail == DetailLevel::Full {
                key.clone()
            } else {
                // Observed mode:
                // Generalized paths and token scrubbing for privacy preservation.
                self.scrub_subject(key)
            };

            let payload = if detail == DetailLevel::Full {
                serde_json::json!({
                    subject_key: subject,
                    "first_seen": entry.first_seen,
                    "last_seen": entry.last_seen,
                    "hits": entry.hits_total,
                    "runs_seen": entry.runs_seen,
                })
            } else {
                // Observed: Minimized payload (subject_key is intentionally omitted for privacy)
                serde_json::json!({
                    "hits": entry.hits_total,
                })
            };

            let ev = self.create_event(event_type, &subject, payload, export_time);
            events.push(ev);
        }
    }

    fn scrub_subject(&self, input: &str) -> String {
        // Simple 2026-style scrubbing
        let mut scrubbed = input.to_string();

        // 1. Path generalization
        // Handle /Users (Mac), /home (Linux), and C:\Users (Windows)
        let normalized = scrubbed.replace('\\', "/");
        if normalized.starts_with("/Users/")
            || normalized.starts_with("/home/")
            || normalized.contains("/Users/")
        {
            // Try to find the user segment and generalize
            let parts: Vec<&str> = normalized.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "Users" || *part == "home") && i + 1 < parts.len() {
                    // generalize from here
                    scrubbed = format!("~/**/{}", parts[i + 2..].join("/"));
                    break;
                }
            }
        }

        // 2. Token/Secret scrubbing (case-insensitive matching)
        let sensitive = [
            "authorization: bearer ",
            "authorization: ",
            "token=",
            "key=",
            "bearer ",
            "api_key=",
            "apikey=",
            "secret=",
            "password=",
            "session=",
            "cookie=",
            "x-api-key:",
            "x-token:",
        ];
        let lower_scrubbed = scrubbed.to_lowercase();
        for pattern in sensitive {
            if let Some(idx) = lower_scrubbed.find(pattern) {
                scrubbed.truncate(idx + pattern.len());
                scrubbed.push_str("***");
                break;
            }
        }

        scrubbed
    }

    fn create_event(
        &mut self,
        type_: &str,
        subject: &str,
        data: serde_json::Value,
        time: DateTime<Utc>,
    ) -> EvidenceEvent {
        // Construct standard EvidenceEvent
        // ID format: {run_id}:{seq}
        // Seq increments strictly.

        let id = format!("{}:{}", self.run_id, self.seq);

        let source = format!("urn:assay:cli:{}", self.producer_version);

        use sha2::Digest;

        // TraceID determined by RunID (correlation)
        let mut t_hasher = sha2::Sha256::new();
        t_hasher.update(self.run_id.as_bytes());
        let t_hash = hex::encode(t_hasher.finalize());
        assert!(t_hash.len() >= 32, "SHA256 hex hash too short");
        let trace_id = &t_hash[..32];

        // SpanID determined by Event ID (uniqueness)
        let mut s_hasher = sha2::Sha256::new();
        s_hasher.update(id.as_bytes());
        let s_hash = hex::encode(s_hasher.finalize());
        assert!(s_hash.len() >= 16, "SHA256 hex hash too short");
        let span_id = &s_hash[..16];

        let mut ev = EvidenceEvent::new(type_, &source, &self.run_id, self.seq, data);
        ev.id = id;
        ev.time = time;
        if !subject.is_empty() {
            ev.subject = Some(subject.to_string());
        }

        ev.trace_parent = Some(format!("00-{}-{}-01", trace_id, span_id));

        self.seq += 1;
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrub_subject_paths() {
        let mapper = EvidenceMapper::new(None, "test");
        assert_eq!(
            mapper.scrub_subject("/Users/alice/repo/file.txt"),
            "~/**/repo/file.txt"
        );
        assert_eq!(
            mapper.scrub_subject("/home/bob/scripts/run.sh"),
            "~/**/scripts/run.sh"
        );
        assert_eq!(
            mapper.scrub_subject("C:\\Users\\charlie\\docs\\sec.txt"),
            "~/**/docs/sec.txt"
        );
        assert_eq!(mapper.scrub_subject("/etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn test_scrub_subject_secrets() {
        let mapper = EvidenceMapper::new(None, "test");
        // Mixed case
        assert_eq!(
            mapper.scrub_subject("curl -H 'Authorization: Bearer MOCKED_TOKEN'"),
            "curl -H 'Authorization: Bearer ***"
        );
        // Lower case
        assert_eq!(
            mapper.scrub_subject("curl -H 'authorization: bearer MOCKED_TOKEN'"),
            "curl -H 'authorization: bearer ***"
        );
        assert_eq!(
            mapper.scrub_subject("mysql --password=secret123"),
            "mysql --password=***"
        );
        assert_eq!(
            mapper.scrub_subject("https://api.com?api_key=12345&other=val"),
            "https://api.com?api_key=***"
        );
        // Variant header
        assert_eq!(mapper.scrub_subject("X-API-Key: 12345"), "X-API-Key:***");
    }

    #[test]
    fn test_determinism_stable_time() {
        let mut mapper = EvidenceMapper::new(None, "test");
        let profile = Profile {
            version: "1.0".into(),
            name: "test".into(),
            created_at: "2026-01-26T22:00:00Z".into(),
            updated_at: "2026-01-26T23:00:00Z".into(),
            total_runs: 1,
            run_ids: vec!["run1".into()],
            run_id_digests: vec![],
            scope: None,
            entries: Default::default(),
        };

        let events = mapper.map_profile(&profile, DetailLevel::Summary).unwrap();
        for ev in events {
            assert_eq!(ev.time.to_rfc3339(), "2026-01-26T23:00:00+00:00");
        }
    }
}
