# SPLIT CHECKLIST — Wave O1 Ringbuf Telemetry Step1

## Scope discipline
- [ ] Only allowlisted files changed for this step
- [ ] No `.github/workflows/*` changes
- [ ] No `crates/assay-core/src/engine/*` changes
- [ ] No registry, Python SDK, fuzz, or release-workflow changes
- [ ] No new CLI flags or command modes

## Contract checks
- [ ] Kernel tracepoint paths increment emit/drop counters
- [ ] LSM path increments emit/drop counters
- [ ] Socket path increments emit/drop counters
- [ ] Userspace can snapshot both `STATS` and `SOCKET_STATS`
- [ ] `assay monitor` prints an end-of-run summary
- [ ] Ring-buffer pressure is surfaced as an explicit warning when any drop counter is non-zero

## Non-goals
- [ ] No policy-evaluation OTel spans in this step
- [ ] No trust-root implementation changes in this step
- [ ] No workflow or release artifact changes in this step

## Validation
- [ ] `BASE_REF=origin/codex/codebase-analysis-followups bash scripts/ci/review-wave-o1-ringbuf-telemetry-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-monitor -p assay-cli --all-targets -- -D warnings` passes
- [ ] `cargo check -p assay-monitor` passes
- [ ] `cargo test -p assay-monitor` passes
- [ ] `cargo check -p assay-cli` passes
- [ ] eBPF target check is either green or explicitly skipped because `bpfel-unknown-none` is not installed
