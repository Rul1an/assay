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
    // producer: String, // unused
    producer_version: String,
    // start_time: DateTime<Utc>, // unused
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
            format!("run_{}", &hash[..16])
        };

        Self {
            run_id,
            // producer: "assay-cli".to_string(),
            producer_version: env!("CARGO_PKG_VERSION").to_string(),
            // start_time: Utc::now(),
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
        let export_time = Utc::now();

        // 1. Started Event (Control)
        events.push(self.create_event(
            "assay.profile.started",
            "urn:assay:phase:start",
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

        // 3. Finished Event (Control) - with summary counts
        events.push(self.create_event(
            "assay.profile.finished",
            "urn:assay:phase:finish",
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
                // Redaction for Observed mode:
                // 1. Path Generalization: /home/roelschuurkes/file -> ~/**/file
                // 2. Token Scrubbing: --token=xyz -> --token=***
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
                // Observed: Minimized payload
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

        // 1. Path generalization (very basic for v1)
        if scrubbed.starts_with("/Users/") || scrubbed.starts_with("/home/") {
            let parts: Vec<&str> = scrubbed.split('/').collect();
            if parts.len() > 3 {
                // /Users/name/rest -> ~/**/rest
                scrubbed = format!("~/**/{}", parts[3..].join("/"));
            }
        }

        // 2. Token/Secret scrubbing
        let sensitive = ["token=", "key=", "Authorization:", "password="];
        for pattern in sensitive {
            if let Some(idx) = scrubbed.find(pattern) {
                scrubbed.truncate(idx + pattern.len());
                scrubbed.push_str("***");
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
        let trace_id = &t_hash[..32];

        // SpanID determined by Event ID (uniqueness)
        let mut s_hasher = sha2::Sha256::new();
        s_hasher.update(id.as_bytes());
        let s_hash = hex::encode(s_hasher.finalize());
        let span_id = &s_hash[..16];

        let mut ev = EvidenceEvent::new(type_, &source, &self.run_id, self.seq, data);
        ev.id = id;
        ev.time = time;
        if !subject.is_empty() && !subject.starts_with("urn:") {
            ev.subject = Some(subject.to_string());
        }

        ev.trace_parent = Some(format!("00-{}-{}-01", trace_id, span_id));

        self.seq += 1;
        ev
    }
}
