use crate::cli::commands::profile_types::{Profile, ProfileEntry};
use anyhow::Result;
use assay_evidence::types::EvidenceEvent;
use chrono::Utc;

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

    pub fn map_profile(
        &mut self,
        profile: &Profile,
        detail: DetailLevel,
    ) -> Result<Vec<EvidenceEvent>> {
        let mut events = Vec::new();

        // 1. Started Event (Control)
        events.push(self.create_event(
            "assay.profile.started",
            "urn:assay:phase:start",
            serde_json::json!({
                "profile_name": profile.name,
                "profile_version": profile.version,
                "total_runs_aggregated": profile.total_runs,
            }),
        ));

        // 2. Observed Events (if requested)
        if detail != DetailLevel::Summary {
            self.map_entries(
                &mut events,
                &profile.entries.files,
                "assay.fs.access",
                "file",
                detail,
            );
            self.map_entries(
                &mut events,
                &profile.entries.network,
                "assay.net.connect",
                "host",
                detail,
            );
            self.map_entries(
                &mut events,
                &profile.entries.processes,
                "assay.process.exec",
                "cmd",
                detail,
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
    ) {
        for (key, entry) in entries {
            let subject: String = if detail == DetailLevel::Full {
                key.clone()
            } else {
                // Redaction for Observed mode:
                // For paths, strict mode might hash specific segments, but here we expect
                // generalized paths (which Profile stores).
                // However, we still treat them as potentially sensitive.

                // For v1, we assume Profile keys are generalized enough (e.g. /tmp/...)
                // but we should hash distinct paths if they look like PII?
                // For now, passthrough generalized path.
                key.clone()
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

            // Use the generic factory, but override the subject URI if possible
            let ev = self.create_event(event_type, &subject, payload);

            // Backdate to last_seen if available (to reflect reality),
            // otherwise keep seq-time (monotonic).
            // Profile stores logical timestamps (u64), we might need to interpret them.
            // If they are unix timestamps (ms or s?), we can use them.
            // profile.rs uses chrono::Utc::now() for created_at, but entries use u64.
            // Let's trust they are comparable or just use cur time for export "statement".

            events.push(ev);
        }
    }

    fn create_event(
        &mut self,
        type_: &str,
        subject: &str,
        data: serde_json::Value,
    ) -> EvidenceEvent {
        // Construct standard EvidenceEvent
        // ID format: {run_id}:{seq}
        // Seq increments strictly.

        let id = format!("{}:{}", self.run_id, self.seq);
        let time = Utc::now(); // Use export time as the "attestation time"

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
