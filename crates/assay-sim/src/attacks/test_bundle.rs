use anyhow::Result;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::BundleWriter;
use chrono::{TimeZone, Utc};

pub(crate) fn create_single_event_bundle() -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    let mut event = EvidenceEvent::new("assay.test", "urn:test", "run", 0, serde_json::json!({}));
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    writer.add_event(event);
    writer.finish()?;
    Ok(buffer)
}

pub(crate) fn create_differential_bundle() -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for seq in 0..3u64 {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:test",
            "diffrun",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
        writer.add_event(event);
    }
    writer.finish()?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_single_event_bundle() -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);
        let mut event =
            EvidenceEvent::new("assay.test", "urn:test", "run", 0, serde_json::json!({}));
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
        writer.add_event(event);
        writer.finish()?;
        Ok(buffer)
    }

    fn legacy_differential_bundle() -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);
        for seq in 0..3u64 {
            let mut event = EvidenceEvent::new(
                "assay.test",
                "urn:test",
                "diffrun",
                seq,
                serde_json::json!({"seq": seq}),
            );
            event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
            writer.add_event(event);
        }
        writer.finish()?;
        Ok(buffer)
    }

    #[test]
    fn single_event_bundle_matches_legacy_bytes() {
        let legacy = legacy_single_event_bundle().unwrap();
        let shared = create_single_event_bundle().unwrap();
        assert_eq!(shared, legacy);
    }

    #[test]
    fn differential_bundle_matches_legacy_bytes() {
        let legacy = legacy_differential_bundle().unwrap();
        let shared = create_differential_bundle().unwrap();
        assert_eq!(shared, legacy);
    }
}
