//! Store metrics for P0.3 performance assessment: contention, wait time, write time, batch size.
//! Exposed in run output (e.g. run.json / summary) for regression and bottleneck analysis.
//!
//! **Busy handler:** SQLite allows only one busy handler per connection. We use a single handler
//! that both counts retries and implements timeout (sleep/backoff); we do *not* set PRAGMA busy_timeout
//! because that would conflict with our custom handler.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Default max wait for busy handler (matches typical PRAGMA busy_timeout).
const BUSY_TIMEOUT_MS: u64 = 5000;

/// Process-wide SQLITE_BUSY retry count (rusqlite busy_handler is a function pointer, so we use a static).
/// Reset at run start when using a single store. For multi-store or tests running in parallel, counts are ambiguous; run_context.db_mode identifies which DB was used.
pub(crate) static SQLITE_BUSY_COUNT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Start of current "busy wait" session (per thread) for timeout enforcement.
    static BUSY_SESSION_START: std::cell::RefCell<Option<Instant>> = std::cell::RefCell::new(None);
}

/// Inner metrics: atomics in microseconds for precision (avoids rounding to 0 ms on fast writes).
#[derive(Debug, Default)]
pub struct StoreMetricsInner {
    pub store_wait_us: AtomicU64,
    pub store_write_us: AtomicU64,
    pub last_txn_batch_size: AtomicU64,
    pub max_txn_batch_size: AtomicU64,
}

impl StoreMetricsInner {
    pub fn add_wait_us(&self, us: u64) {
        self.store_wait_us.fetch_add(us, Ordering::Relaxed);
    }

    pub fn add_write_us(&self, us: u64) {
        self.store_write_us.fetch_add(us, Ordering::Relaxed);
    }

    pub fn record_batch_size(&self, n: usize) {
        let n = n as u64;
        self.last_txn_batch_size.store(n, Ordering::Relaxed);
        self.max_txn_batch_size.fetch_max(n, Ordering::Relaxed);
    }

    /// Take a snapshot (for serialization) and optionally reset atomics for next run.
    pub fn snapshot(&self, reset: bool) -> StoreMetricsSnapshot {
        let sqlite_busy_count = SQLITE_BUSY_COUNT.load(Ordering::Relaxed);
        let store_wait_us = self.store_wait_us.load(Ordering::Relaxed);
        let store_write_us = self.store_write_us.load(Ordering::Relaxed);
        let _last_txn_batch_size = self.last_txn_batch_size.load(Ordering::Relaxed);
        let max_txn_batch_size = self.max_txn_batch_size.load(Ordering::Relaxed);

        if reset {
            SQLITE_BUSY_COUNT.store(0, Ordering::Relaxed);
            self.store_wait_us.store(0, Ordering::Relaxed);
            self.store_write_us.store(0, Ordering::Relaxed);
            self.last_txn_batch_size.store(0, Ordering::Relaxed);
            self.max_txn_batch_size.store(0, Ordering::Relaxed);
        }

        StoreMetricsSnapshot {
            sqlite_busy_count,
            store_wait_ms: store_wait_us / 1000,
            store_write_ms: store_write_us / 1000,
            store_wait_us: Some(store_wait_us),
            store_write_us: Some(store_write_us),
            txn_batch_size: if max_txn_batch_size > 0 {
                Some(max_txn_batch_size)
            } else {
                None
            },
            effective_pragmas: None,
            wal_checkpoint: None,
            busy_handler: Some("counting_timeout".to_string()),
            busy_timeout_configured_ms: Some(BUSY_TIMEOUT_MS),
            store_wait_pct: None,
            store_write_pct: None,
        }
    }
}

/// Effective SQLite pragmas (queried after open) for perf assessment.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct EffectivePragmas {
    pub journal_mode: String,
    /// Raw from PRAGMA synchronous (0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA).
    pub synchronous: String,
    /// Human-readable for DX; in WAL mode NORMAL defers fsyncs to checkpoint, FULL is more durable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synchronous_human: Option<String>,
    /// From PRAGMA busy_timeout; often 0 when we use our own busy handler (we don't set PRAGMA busy_timeout).
    pub busy_timeout: i64,
    pub wal_autocheckpoint: i64,
}

/// Result of PRAGMA wal_checkpoint(PASSIVE). SQLite returns three integers; see sqlite.org/pragma.html#wal_checkpoint.
/// Column order: (busy/blocked, log frames, checkpointed frames).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WalCheckpointResult {
    /// Busy/blocked flag: 0 = checkpoint completed (or PASSIVE didn't block); 1 = checkpoint was blocked (e.g. readers).
    pub blocked: i32,
    /// Total number of frames in the WAL log; -1 if checkpoint could not run or DB not in WAL mode.
    pub log_frames: i32,
    /// Number of checkpointed frames; -1 if checkpoint could not run.
    pub checkpointed_frames: i32,
}

/// Snapshot of store metrics for a single run (serializable).
///
/// **Semantics:** `store_wait_ms` = time waiting for the store mutex (lock contention).
/// `store_write_ms` = time the mutex is held in write path (includes SQLite work, busy-sleeps in our busy handler, and our code). If store_write_ms is high but sqlite_busy_count is low, time is likely in payload/serde/statement work; if sqlite_busy_count is high, lock contention or checkpointing is more likely.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct StoreMetricsSnapshot {
    /// Number of SQLITE_BUSY retries (our busy handler counts and sleeps with backoff; process-wide, reset at run start).
    pub sqlite_busy_count: u64,
    /// Time waiting for the store mutex (lock contention); derived from µs.
    pub store_wait_ms: u64,
    /// Time mutex is held in write path; derived from µs.
    pub store_write_ms: u64,
    /// Same in microseconds (avoids rounding to 0 on fast writes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_wait_us: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_write_us: Option<u64>,
    /// Max batch size observed (e.g. insert_batch length).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_batch_size: Option<u64>,
    /// Effective pragmas (journal_mode, synchronous, busy_timeout, wal_autocheckpoint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_pragmas: Option<EffectivePragmas>,
    /// Result of PRAGMA wal_checkpoint(PASSIVE) after run (file-backed only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_checkpoint: Option<WalCheckpointResult>,
    /// We use a custom busy handler that counts + sleeps + timeout; PRAGMA busy_timeout is not set (would conflict).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy_handler: Option<String>,
    /// Max wait (ms) in our busy handler before returning SQLITE_BUSY.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy_timeout_configured_ms: Option<u64>,
    /// store_wait_ms as percentage of total_ms (set by CLI when phases.total_ms is available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_wait_pct: Option<f64>,
    /// store_write_ms as percentage of total_ms (set by CLI when phases.total_ms is available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_write_pct: Option<f64>,
}

/// Reset process-wide busy count (call at start of a run when using a single store).
pub fn reset_busy_count() {
    SQLITE_BUSY_COUNT.store(0, Ordering::Relaxed);
}

/// Busy handler for rusqlite: count retries, sleep with backoff, and respect timeout.
/// Return true to retry, false to give up (SQLite will then return SQLITE_BUSY to the caller).
/// We do *not* set PRAGMA busy_timeout because SQLite allows only one busy handler; this handler implements both counting and timeout.
pub fn busy_handler(retries: i32) -> bool {
    SQLITE_BUSY_COUNT.fetch_add(1, Ordering::Relaxed);

    BUSY_SESSION_START.with(|cell| {
        let mut start = cell.borrow_mut();
        // Each new "busy wait" starts with retries == 0; use that to start the session timer.
        if retries == 0 {
            *start = Some(Instant::now());
        }
        let elapsed_ms = start
            .as_ref()
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);
        if elapsed_ms >= BUSY_TIMEOUT_MS {
            *start = None;
            return false;
        }
        // Backoff: 1, 2, 4, 8, ... ms, capped at 50 ms
        let delay_ms = (1u64 << retries.min(10)).min(50);
        thread::sleep(Duration::from_millis(delay_ms));
        true
    })
}
