# Kernel Incident Package Admission (`runner-kernel-ndjson-v0`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Admit Runner v0 kernel-layer NDJSON (`runner-kernel-ndjson-v0`, `schema = "assay.runner.kernel_event.v0"`) as an explicit first-class input format under existing Incident Package v1 document identities (`assay.incident.inventory.v1`, `assay.incident.assessment.v1`, `assay.incident.verify.v1`), preserving raw line bytes, strict pre-materialization resource bounds, and honest observation/claim ceilings.

**Architecture:** Share the actual `KernelLayerEvent` wire struct and `KERNEL_EVENT_SCHEMA` constant (`"assay.runner.kernel_event.v0"`) in `assay-runner-schema::kernel_event` (re-exported by `assay-runner-core::kernel`) alongside a single closed wire/variant validator matching `crates/assay-runner-core/src/kernel/decode.rs`. Extend `assay-evidence::incident_package` to admit `runner-kernel-ndjson-v0` using existing `line:<ordinal>:<byte-start>:<byte-length>` NDJSON framing, `transcript_bytes` / `record_bytes` / `records` / strict JSON preflight ceilings, exact non-empty surface mapping (`filesystem`, `network`, `process`), present-empty subset support, no inferred timestamps or strong joins, and an observation ceiling of `unknown` without context and at most `limited` for raw kernel surfaces even when operator applicability requests `adequate` with complete `ObservationHealth` or a retained full coverage descriptor.

**Tech Stack:** Rust, `serde`/`serde_json`, `assay-runner-schema`, `assay-runner-core`, `assay-evidence`, `assay-canonical` (RFC 8785 JCS), `sha2`.

## Global Constraints

- Base SHA: `5611e2f9ab15ace0512f63cef2f95c58d59963f8` on branch `codex/3345-kernel-incident-admission`.
- Stage 1 freeze: edit only `docs/architecture/SPEC-Incident-Package-v1.md`, `docs/superpowers/plans/2026-10-17-kernel-incident-admission.md`, and `crates/assay-evidence/tests/incident_kernel_admission.rs`. Do not edit production code, `Cargo.toml`, run builds, or commit/push until Codex records the Linux RED baseline and authorizes Stage 2.
- No profile conversion, no fake Inspector entries, no inferred timestamps, no strong kernel joins, and no Runner archive membership verification claim.
- Raw kernel sources (`runner-kernel-ndjson-v0`) cannot on their own establish `adequate`, `exhaustive_set`, or `bounded_negative` (absence) coverage: pin `unknown` without context and at most `limited` for supplied raw kernel surfaces even if operator applicability requests `adequate` with complete `ObservationHealth` or a retained full coverage descriptor.
- Existing unrelated format semantics (`assay-bundle-v1`, `proxy-decision-ndjson`, `inspector-protocol-793d103`, etc.) remain unchanged.

---

### Task 1: Stage 1 — Amend Spec Contract and Freeze Behavioral RED Tests

**Files:**
- Modify: `docs/architecture/SPEC-Incident-Package-v1.md`
- Create: `docs/superpowers/plans/2026-10-17-kernel-incident-admission.md`
- Create: `crates/assay-evidence/tests/incident_kernel_admission.rs`

- [x] **Step 1: Amend `docs/architecture/SPEC-Incident-Package-v1.md`**
  - Add `runner-kernel-ndjson-v0` to §3 admitted formats with locator `line:<ordinal>:<byte-start>:<byte-length>`.
  - Document the pinned `KernelLayerEvent` wire rules (`schema = "assay.runner.kernel_event.v0"`, `run_id`, `seq`, `pid`, `event_type`, `kind`, `value`, and per-variant optional fields for `openat`, `file_blocked`, `connect`, `sendto`, `sendmsg`, `connect_blocked`, `exec`).
  - Document §4 population mapping (`filesystem`, `network`, `process`), §7.1 surface declaration rules (exact mapped surface set for non-empty objects; canonical-order subset of `[filesystem, network, process]` for present-empty objects), §7.3 `external_receipt` role with no strong joins or timestamps, and §9 resource limits (`transcript_bytes`, `record_bytes`, `records`) and observation ceiling (`unknown` without context, at most `limited` for raw kernel surfaces including with full coverage context).

- [x] **Step 2: Write self-contained behavioral RED tests in `crates/assay-evidence/tests/incident_kernel_admission.rs`**
  - Derive all expected `assessment.json` units, locators, JCS digests, CAP-1 strata, surface results, and `IncidentVerifyReport` values independently from the spec without calling verifier logic.
  - Cover positive three-surface admission, duplicate raw lines with distinct `line:<ordinal>:<byte-start>:<byte-length>` locators, all 7 emitted `decode.rs` variants (including `openat` optional fields, `connect_blocked` decoded and fallback shapes, `value: null`, CRLF + unterminated final line, and present-empty kernel input), and byte-identical `build_verified_incident_package` export round-trip.
  - Cover observation ceiling: `unknown` without context; capped at `limited` when operator requests `adequate` with complete `ObservationHealth` (and `StaleAssessment` refusal if `assessment.json` claims `adequate`).
  - Cover hostile negatives: wrong `schema` (`UnknownSchema`), mismatched `event_type`/`kind` and invalid variant fields (`InputShape`), invalid/mismatched surface declarations (`Mapping`), truncated/blank/BOM/lone-CR/duplicate-key/deep lines (`InputShape`/`ResourceSelected`/`ResourceHard`), selected/hard `transcript_bytes`/`record_bytes`/`records`/string limits, digest/locator/disclosure mismatches, and strong join / `bounded_negative` / `exhaustive_set` overclaims (`ClaimBoundary`).

- [x] **Step 3: Freeze Stage 1 and hand off to Codex for Linux RED**
  - Focused test command:
    ```bash
    cargo test -p assay-evidence --test incident_kernel_admission
    ```
  - Expected RED on baseline `5611e2f9ab15ace0512f63cef2f95c58d59963f8`: tests compile cleanly and fail behaviorally with `IncidentOutcome::PackageRefused` / `Some(IncidentReason::UnknownFormat)` against expected `PackageVerified` (and expected post-Phase-3 refusal reasons).
  - Retain the source snapshot and logs for behavioral RED separately; compile errors and earlier metadata-fixture failures do not qualify.

---

### Task 2: Stage 2 (After Authorization) — Share `KernelLayerEvent` Wire Contract in `assay-runner-schema`

**Files:**
- Create: `crates/assay-runner-schema/src/kernel_event.rs`
- Modify: `crates/assay-runner-schema/src/lib.rs`
- Modify: `crates/assay-runner-core/src/run.rs`
- Modify: `crates/assay-runner-core/src/kernel.rs`
- Modify: `crates/assay-runner-core/src/kernel/decode.rs`
- Modify: `crates/assay-runner-core/src/kernel/tests/events.rs`
- Modify: `scripts/ci/assay_runner_gating_map.txt`

- [x] **Step 1: Define `KernelLayerEvent`, `KERNEL_EVENT_SCHEMA`, and closed wire validation in `crates/assay-runner-schema/src/kernel_event.rs`**
  - Move `KERNEL_EVENT_SCHEMA` (`"assay.runner.kernel_event.v0"`) and `KernelLayerEvent` to `assay-runner-schema::kernel_event` with identical field names, field order, types (`mode: Option<u64>`, `rule_id: Option<u32>`), and serde attributes, and re-export them from `assay-runner-schema` and `assay-runner-core::kernel`.
  - Share `is_safe_run_id`, `open_access_mode`, `open_operation_flags`, and `open_status` between `assay-runner-core` (`run.rs`, `kernel.rs`, `kernel/decode.rs`) and `KernelLayerEvent::validate_emitted`.
  - Add producer parity test (`kernel_layer_builder_seven_emitted_variants_pass_shared_schema_validator`) in `crates/assay-runner-core/src/kernel/tests/events.rs` verifying all 7 emitted variants via `KernelLayerBuilder`.

- [x] **Step 2: Verify `assay-runner-schema` and `assay-runner-core` unit tests on Linux**
  - Run `cargo test -p assay-runner-schema -p assay-runner-core`.

---

### Task 3: Stage 2 (After Authorization) — Wire `runner-kernel-ndjson-v0` in `assay-evidence`

**Files:**
- Modify: `crates/assay-evidence/src/incident_package/verify.rs`
- Modify: `crates/assay-evidence/tests/incident_kernel_admission.rs`

- [x] **Step 1: Admit `runner-kernel-ndjson-v0` in Phase 3, preflight, and action-unit extraction**
  - Add `"runner-kernel-ndjson-v0"` to `ADMITTED_FORMATS`.
  - In `admit_metadata` (Phase 3), verify `input.surfaces` for `runner-kernel-ndjson-v0` is a canonical-order duplicate-free subset of `["filesystem", "network", "process"]` (`IncidentReason::Mapping` otherwise).
  - Charge `transcript_bytes` in `source_byte_limit`, per-line `record_bytes` and `records` in `preflight_non_bundle_source` (per input occurrence, including equal-digest inputs declared twice), and `SourceJsonKind::Kernel` (`value` as `StringClass::Payload`, other strings as `StringClass::NonPayload`) in `SourceJsonHooks`, rejecting BOM, blank lines, and lone `\r` without altering existing `proxy-decision-ndjson` behavior.
  - In `admitted_action_records`, validate each line's raw JSON key shape (`is_known_kernel_event_key`, `REQUIRED_KERNEL_EVENT_KEYS`, no non-`value` nulls, no empty `operation_flags` array) and `KernelLayerEvent::validate_emitted()`, enforce exact surface set equality for non-empty objects (`IncidentReason::Mapping` on mismatch), and emit one `AdmittedActionRecord` per line with `operation: "runner-kernel-ndjson-v0"`, `locator: "line:<ordinal>:<offset>:<len>"`, `timestamp: None`, and `examination_succeeded: true`.
  - Support `runner-kernel-ndjson-v0` line locators in `original_ref_input`.
  - In `retained_context_inventory`, cap surfaces supplied by `runner-kernel-ndjson-v0` (without a descriptor-to-record binding contract) to `ObservationCeiling::Limited`, preserving `unknown` without context and at most `limited` when operator requests `adequate` with complete `ObservationHealth`; retained full descriptors cannot lift this ceiling.

- [x] **Step 2: Run focused and regression verification**
  - Run:
    ```bash
    cargo test -p assay-evidence --test incident_kernel_admission
    cargo test -p assay-evidence --test incident_package_verify
    cargo test -p assay-evidence --test incident_container
    cargo clippy -p assay-runner-schema -p assay-runner-core -p assay-evidence --all-targets -- -D warnings
    cargo fmt --all -- --check
    git diff --check
    ```
