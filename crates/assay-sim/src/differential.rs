use anyhow::{Context, Result};
use assay_evidence::types::EvidenceEvent;
use assay_evidence::bundle::writer::BundleWriter;
use assay_evidence::{verify_bundle_with_limits, VerifyLimits};
use chrono::{TimeZone, Utc};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::io::Cursor;

pub fn check_invariants(iterations: usize, seed: Option<u64>) -> Result<()> {
    let mut rng = match seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_entropy(),
    };

    println!("Running Differential Tests ({} iterations)...", iterations);

    for i in 0..iterations {
        let event_count = rng.gen_range(1..100);
        let mut buffer = Vec::new();

        let run_id = format!("run_{}", i);
        let _run_root = "urn:assay:test";

        // 1. Generate Bundle
        {
            let mut writer = BundleWriter::new(&mut buffer);
            for seq in 0..event_count {
                let event = generate_random_event(&mut rng, &run_id, seq);
                writer.add_event(event);
            }
            writer.finish().context("Failed to write bundle in differential test")?;
        }

        // 2. Verify Bundle
        let cursor = Cursor::new(&buffer);
        // Use default limits, but ensure max_events covers our range
        let limits = VerifyLimits {
            max_events: 1000,
            ..VerifyLimits::default()
        };

        match verify_bundle_with_limits(cursor, limits) {
            Ok(result) => {
                if result.event_count != event_count as usize {
                    anyhow::bail!(
                        "Invariant Broken: Event count mismatch. Written: {}, Verified: {}",
                        event_count,
                        result.event_count
                    );
                }
            }
            Err(e) => {
                anyhow::bail!(
                    "Invariant Broken: Writer output failed verification.\n\
                     Iteration: {}\n\
                     Error: {:?}\n\
                     Seed: {:?}",
                    i,
                    e,
                    seed
                );
            }
        }
    }

    println!("Differential Tests Passed: {} iterations OK.", iterations);
    Ok(())
}

// Wrapper to expose verify with custom limits if accessible,
// otherwise use verify_bundle (if limits are default enough).
// NOTE: We rely on verify_bundle accepting the valid bundle.

fn generate_random_event(rng: &mut StdRng, run_id: &str, seq: u64) -> EvidenceEvent {
    let payload_size = rng.gen_range(10..1000);
    let random_str: String = (0..payload_size)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect();

    let mut event = EvidenceEvent::new(
        "assay.sim.random",
        "urn:assay:sim",
        run_id,
        seq,
        serde_json::json!({
            "seq": seq,
            "random_data": random_str,
            "nested": {
                "a": 1,
                "b": [1, 2, 3]
            }
        }),
    );
    // Randomize time slightly around fixed point
    let offset = rng.gen_range(-1000..1000);
    event.time = Utc.timestamp_opt(1700000000 + offset, 0).unwrap();
    event
}
