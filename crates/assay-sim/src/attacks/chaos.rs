use super::test_bundle::create_single_event_bundle;
use crate::report::{AttackResult, AttackStatus};
use anyhow::Result;
use assay_evidence::{verify_bundle, VerifyError};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::{self, Cursor, ErrorKind, Read};
use std::time::Instant;

/// A `Read` wrapper that injects IO faults based on RNG.
///
/// Fault types:
/// - `ErrorKind::Interrupted` — tests retry logic
/// - `ErrorKind::WouldBlock` — tests non-blocking awareness
/// - Short reads — returns fewer bytes than buffer size
pub struct IOChaosReader<R: Read> {
    inner: R,
    rng: StdRng,
    fault_probability: f64,
}

impl<R: Read> IOChaosReader<R> {
    pub fn new(inner: R, seed: u64, fault_probability: f64) -> Self {
        Self {
            inner,
            rng: StdRng::seed_from_u64(seed),
            fault_probability,
        }
    }
}

impl<R: Read> Read for IOChaosReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let roll: f64 = self.rng.gen();
        if roll < self.fault_probability {
            // Pick a fault type
            let fault_kind = self.rng.gen_range(0..3);
            match fault_kind {
                0 => {
                    // Interrupted — caller should retry
                    return Err(io::Error::new(
                        ErrorKind::Interrupted,
                        "chaos: simulated interrupt",
                    ));
                }
                1 => {
                    // WouldBlock — tests non-blocking awareness
                    return Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "chaos: simulated would-block",
                    ));
                }
                _ => {
                    // Short read — return fewer bytes than requested
                    let max_read = (buf.len() / 4).max(1);
                    return self.inner.read(&mut buf[..max_read]);
                }
            }
        }

        self.inner.read(buf)
    }
}

/// Generate a malformed gzip stream: valid header + truncated/garbage body.
pub fn malformed_gzip_stream() -> Vec<u8> {
    // Valid gzip header (RFC 1952)
    let mut data = vec![
        0x1f, 0x8b, // Magic number
        0x08, // Compression method (deflate)
        0x00, // Flags
        0x00, 0x00, 0x00, 0x00, // Modification time
        0x00, // Extra flags
        0xff, // OS (unknown)
    ];
    // Append garbage body (not valid deflate)
    data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42, 0x43]);
    data
}

/// Run chaos IO fault injection attacks.
///
/// Each iteration wraps a valid bundle in `IOChaosReader` with random fault injection
/// and verifies the outcome is either `Blocked` or `Error` (never `Bypassed`, never panic).
pub fn check_chaos_attacks(seed: u64) -> Result<Vec<AttackResult>> {
    let mut results = Vec::new();
    let valid_bundle = create_single_event_bundle()?;
    let iterations = 20;

    // 1. IO chaos reader iterations
    for i in 0..iterations {
        let iter_seed = seed.wrapping_add(i as u64);
        let start = Instant::now();

        let chaos_reader = IOChaosReader::new(Cursor::new(valid_bundle.clone()), iter_seed, 0.05);
        let res = verify_bundle(chaos_reader);
        let duration = start.elapsed().as_millis() as u64;

        let (status, error_class, message) = match res {
            Ok(_) => {
                // Verification passed despite chaos — faults were non-fatal (EINTR retried, etc.)
                (AttackStatus::Passed, None, None)
            }
            Err(e) => {
                let msg = format!("{:#}", e);
                if let Some(ve) = e.downcast_ref::<VerifyError>() {
                    // Structural verification failure → real "Blocked"
                    (
                        AttackStatus::Blocked,
                        Some(format!("{:?}", ve.class)),
                        Some(msg),
                    )
                } else {
                    // IO error from chaos injection (WouldBlock, persistent EINTR, etc.)
                    // This is infrastructure/transient — not a "blocked attack"
                    (AttackStatus::Error, Some("InfraIO".into()), Some(msg))
                }
            }
        };

        results.push(AttackResult {
            name: format!("chaos.io_fault.iter_{}", i),
            status,
            error_class,
            error_code: None,
            message,
            duration_ms: duration,
        });
    }

    // 2. Malformed gzip stream
    {
        let start = Instant::now();
        let data = malformed_gzip_stream();
        let res = verify_bundle(Cursor::new(data));
        let duration = start.elapsed().as_millis() as u64;

        let status = match res {
            Ok(_) => AttackStatus::Bypassed,
            Err(_) => AttackStatus::Blocked,
        };

        results.push(AttackResult {
            name: "chaos.malformed_gzip".into(),
            status,
            error_class: None,
            error_code: None,
            message: None,
            duration_ms: duration,
        });
    }

    // 3. Empty reader
    {
        let start = Instant::now();
        let res = verify_bundle(Cursor::new(Vec::<u8>::new()));
        let duration = start.elapsed().as_millis() as u64;

        let status = match res {
            Ok(_) => AttackStatus::Bypassed,
            Err(_) => AttackStatus::Blocked,
        };

        results.push(AttackResult {
            name: "chaos.empty_input".into(),
            status,
            error_class: None,
            error_code: None,
            message: None,
            duration_ms: duration,
        });
    }

    Ok(results)
}
