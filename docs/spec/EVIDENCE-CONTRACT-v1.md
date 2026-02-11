# Evidence Contract v1 — Versioning and Freeze Policy

**Status:** Normative (freeze as of 2026-02)
**Scope:** Bundle container, evidence event envelope, and pack compatibility.
**Positioning (informational):** Assay Evidence = CloudEvents + W3C Trace Context + Deterministic Bundle.
**Related:** [ADR-006 Evidence Contract](../architecture/ADR-006-Evidence-Contract.md), [ADR-007 Deterministic Provenance](../architecture/ADR-007-Deterministic-Provenance.md).

**Governance:** Changes to this document MUST follow the versioning and deprecation policy set out in §5 (deprecation) and §2/§4 (version axes and breaking-change rules): no breaking changes without a new major Assay Evidence Spec version or bundle schema_version, with migration notes. Additive clarifications and version history entries are allowed within v1.

## 1. Terminology and layers

| Layer | Identifier | Meaning |
|-------|------------|---------|
| **Assay Evidence Spec** | v1.0 (string) | Assay’s envelope + payload contract per ADR-006. Implemented as `SPEC_VERSION = "1.0"` in code. |
| Bundle container | schema_version = 1 (integer) | Manifest and .tar.gz layout. Only value 1 is valid for v1; any other is rejected by verify/reader. |
| Pack compatibility | evidence_schema_version: "1.0" | Packs declare the Assay Evidence Spec version they support. **Interpretation (Pack Engine v1 policy):** Pack engines MUST interpret this field as follows. For v1 freeze: *exact match* on `"1.0"` (only bundles with Assay Evidence Spec v1.0 and bundle schema_version 1 are accepted). If the field is absent, assume v1.0. If the value is present and not recognized, the pack engine MUST fail closed (reject) unless explicitly configured otherwise. This is a contract between pack engine and pack. Enforcement lives in the pack engine and lint layer, not in evidence verify. Future Pack Engine specs MAY adopt SemVer range semantics; build metadata is ignored; prerelease has lower precedence than release. |

**Naming note:** *CloudEvents* `specversion` is the string `"1.0"` (CloudEvents context attribute). *Assay Evidence Spec* is Assay’s contract version (also `"1.0"` for v1). They are distinct concepts; tooling and tickets should distinguish “CloudEvents specversion” from “Assay Evidence Spec”.

**Source of truth for breaking changes:** This spec and the bundle manifest schema_version. Event envelope or payload changes that break existing consumers require a new spec version (e.g. v2) or a new bundle schema_version, with migration notes.

## 2. Compatibility matrix

| Producer | Consumer | Bundle schema_version | Result |
|----------|----------|------------------------|--------|
| Assay Evidence Spec v1.0 | Assay Evidence Spec v1.0 | 1 | Supported. |
| Assay Evidence Spec v1.1 (additive) | Assay Evidence Spec v1.0 | 1 | Supported by verify. v1.x stays in schema_version 1. Pack engine policy may be stricter (see note below). |
| Assay Evidence Spec v1.0 | Pack evidence_schema_version "1.0" | 1 | Supported. |
| Any | verify/reader | != 1 | Rejected. schema_version is fixed at 1 for v1; any other value is rejected. |

**Note (pack vs verify):** Evidence verify accepts v1.x additive bundles (schema_version 1). Pack engines that enforce exact match on `"1.0"` may reject bundles produced with Assay Evidence Spec v1.1+ until the pack or engine is updated. So: supported by verify; pack engine policy may be stricter than verify.

**Normative rule:** For Evidence Contract v1, the only supported bundle container axis is schema_version 1, and the baseline Assay Evidence Spec is v1.0. Verify MAY accept v1.x additive bundles (see note above). Pack engine policy MAY require exact 1.0 (see §1); that match is enforced by the pack engine/lint, not by evidence verify.

**Version axes and evolution (mechanically testable):**
- In v1, bundle schema_version is always 1. There is no schema_version 2 while the Assay Evidence Spec is v1.x.
- Assay Evidence Spec minor (v1.1, v1.2, …) MAY contain only additive changes (new optional fields, new event types). Breaking changes are not allowed within v1.x.
- **Breaking change rule:** (1) Container/layout breaking ⇒ bundle schema_version bump (e.g. 2) with migration notes. (2) Meaning or payload breaking for an existing event type ⇒ new type identifier (preferred) or Assay Evidence Spec major (v2). Prefer adding a new type over bumping the spec major to reduce disruption for tooling.

## 3. CloudEvents invariants

**Canonical split (use consistently):** CE-required = specversion, type, source, id. Assay-required = time, assay* extensions, datacontenttype, data. No other attribute is CloudEvents-required beyond those four.

- **Required (CloudEvents v1.x minimum only):** specversion, type, source, id. (`time` is not required by CloudEvents v1.x.)
- **Required by Assay (stricter-than-CloudEvents):** time; assay-flattened fields (assayrunid, assayseq, assayproducer, assayproducerversion, assaycontenthash) per ADR-006; datacontenttype and data for the payload. Assay requires time even though CloudEvents does not; downstream tooling that validates CloudEvents “by the book” should treat this as an Assay-specific requirement.
- **Constant:** Assay Evidence Spec v1 uses CloudEvents 1.0 (specversion=`"1.0"`). Any change to envelope versioning requires a new Assay Evidence Spec major (v2).
- **Free/optional:** traceparent, tracestate, subject, and other CloudEvents extensions as defined in ADR-006 (optional where stated).
- **Type naming:** Event type is a stable namespaced identifier (recommended: reverse-DNS or dot-separated; e.g. assay.profile.started, assay.tool.decision). No whitespace; ASCII recommended; treat as case-sensitive. New event types (new identifier values) are additive and allowed. Changing the meaning or payload schema of an existing type is breaking and requires a new type identifier (preferred) or new spec version. (Use “URN” only when referring to URI-style identifiers such as urn:assay:….)

## 4. Schema evolution rules

- **Additive only for v1:** New optional fields and new event types are allowed. New fields MUST be optional or have defined default/absence semantics so existing consumers remain valid; producers MUST NOT introduce new required fields in an existing event type within v1.x.
- **Producers MUST NOT:** Emit duplicate keys in event JSON at any nesting level (avoids parser confusion / differential parsing); change the meaning of existing fields within v1; reuse an existing event type with an incompatible payload (use a new type identifier instead). JSON (RFC 8259) says object member names SHOULD be unique; Assay requires MUST NOT for safety.
- **No semantic change:** The meaning of existing fields MUST NOT change.
- **No type change:** Changing the type of an existing field is breaking.
- **Removal or rename:** Breaking. Allowed only after deprecation window and only in a new spec version (v2) or new bundle schema_version, with migration notes.
- **Unknown fields (compatibility mode vs strict JSON):** Verify/reader MUST accept events that contain unknown JSON object keys, *unless* one of the following applies (closed list): duplicate keys at any nesting level, event line bytes not valid UTF-8 or invalid Unicode escapes (including lone surrogates)—event lines are UTF-8 NDJSON (events.ndjson), manifest or event size limit exceeded, event count limit exceeded, decompression limit exceeded. No other condition may be used to reject solely on “unknown” keys. **Compatibility mode:** Unknown keys MUST be ignored by consumers. **Strict JSON (parser hardening):** The conditions above are the only security overrides; this aligns with JCS/canonical JSON and fail-closed verification.

### 4.1 New event types (compatibility)

**Policy (normative):** A new evidence event type string MUST NOT be introduced unless:

1. It is added to the [Event types (v1)](#42-event-types-v1) registry table.
2. Its payload contract is documented in [ADR-006](../architecture/ADR-006-Evidence-Contract.md) or the relevant spec: at minimum the type string, payload shape (required/optional fields + types), semantics (1–2 sentences), and versioning posture (v1 additive-only; breaking ⇒ new type or v2). A field-level contract is required; full JSON Schema is not.
3. At least one conformance test verifies that an event of this type passes verify (happy path) and checks at least one required invariant. The payload must be validated/decoded by the consumer(s) that are supposed to understand it—at minimum: evidence verify can parse the envelope and content-hash invariants hold. **At minimum, the test MUST assert: (a) verify succeeds, and (b) assaycontenthash matches the canonicalized payload bytes per v1 rules (directly or indirectly via verify).** Lint/explore coverage is optional.

Existing type strings MUST NOT change payload meaning; breaking changes require a new type string.

This policy applies to any new type intended for production/stable use. Experimental and test-only types MUST still be registered, but MAY use TBD links and MAY use weaker test coverage; they MUST NOT be emitted by default in released builds. **A type may only be marked experimental or test-only temporarily; promotion to stable requires the full bar (schema link + conformance test) and MUST be recorded in version history.**

Version suffixes in the type string (e.g. `.v1`) are allowed but not required; use them when the payload contract itself is versioned independently (e.g. mandate lifecycle).

**Source of truth:** The contract registry is the Event types (v1) table in this spec, not "grep in code". Code that emits a type string not listed in the registry is a process breach; reviewers should require "where is the registry row?".

### 4.2 Event types (v1)

This table lists event types for Assay Evidence Spec v1.x (bundle schema_version 1).

**Stable rows MUST link to a concrete payload section (no "implied"). Types without such a section MUST remain experimental.**

**Test coverage MUST reference at least one concrete test identifier (suite::testname) or generator (generate_fixture) that demonstrably emits/verifies the type.**

| Event type | Status | Description | Payload contract | Test coverage |
|------------|--------|-------------|-------------------|---------------|
| assay.profile.started | stable | Run context start | [ADR-006 §3.A](../architecture/ADR-006-Evidence-Contract.md#3-core-payload-schemas-v10) | generate_fixture, evidence mapping/lint/diff |
| assay.profile.finished | stable | Run context end | ADR-006 §3.A (same) | generate_fixture, evidence mapping |
| assay.fs.access | stable | Filesystem activity (generalized) | [ADR-006 §3.D](../architecture/ADR-006-Evidence-Contract.md#3-core-payload-schemas-v10) | generate_fixture, evidence diff_test, explore |
| assay.net.connect | experimental | Network connection | ADR-006 (no §anchor yet) — add payload section before stable | generate_fixture, evidence diff_test, lint |
| assay.process.exec | experimental | Process execution | ADR-006 (no §anchor yet) — add payload section before stable | generate_fixture, evidence diff_test |
| assay.tool.decision | stable | Policy enforcement decision | [ADR-006 §3.B](../architecture/ADR-006-Evidence-Contract.md#3-core-payload-schemas-v10) | evidence verify_strict_test, mandate/lint |
| assay.env.filtered | stable | Env filtering | ADR-006 envelope + example | assay_evidence types unit test |
| assay.mandate.v1 | stable | Mandate content | [SPEC-Mandate-v1](../architecture/SPEC-Mandate-v1.md) | mandate golden/crypto vectors |
| assay.mandate.used.v1 | stable | Mandate used lifecycle | SPEC-Mandate-v1 | mandate events tests |
| assay.mandate.revoked.v1 | stable | Mandate revoked lifecycle | SPEC-Mandate-v1 | mandate events tests |
| sandbox.degraded | stable | Operational integrity | [ADR-006 §3.C](../architecture/ADR-006-Evidence-Contract.md#3-core-payload-schemas-v10) | generate_fixture, evidence tests |

## 5. Deprecation policy

- **Announcement:** In release notes and in this spec (version history or a dedicated "Deprecations" subsection).
- **Window:** At least 2 minor releases or 6 months, whichever is longer, before removal or breaking change. Security fixes may shorten the window when necessary; breaking changes still require a new spec or container version and migration notes. Security fixes may tighten validation, but MUST NOT silently ignore invalid events.
- **Release discipline (SemVer):** Deprecations MUST ship in a minor release (not a patch). Removals only in a major Assay Evidence Spec version (e.g. v2) or a new bundle schema_version, with explicit migration notes.
- **Marking:** Documented in this spec and release notes. Optional: schema or type-level annotation in code/docs.
- **Removal:** Only in a new major spec version (e.g. v2) or new bundle schema_version, with explicit migration notes.
- **Migration notes:** Required for any breaking change; must describe how to migrate from deprecated to supported form.

**Canonicalization and hashing (freeze):** Canonicalization (e.g. JCS / RFC 8785) is used so that content hashes and signing are deterministic and reproducible across implementations. If canonicalization is used for signing or digests, it MUST follow the project’s chosen scheme (JCS for v1) and be versioned if changed. Content hash algorithm and encoding MUST be stable for v1 (e.g. SHA-256, lowercase hex). Any change requires a version bump and migration notes. See ADR-007.

## 6. Golden fixtures contract

The following fixtures are normative. Consumers (verify, lint, explore, and CI) MUST pass the checks referenced below against these fixtures.

**Container golden vs event golden:** *Container golden* covers tar layout, manifest schema, file hashes, and entry ordering. *Event golden* covers CloudEvents envelope and payload semantics (required attributes, types, content hashes). Both are part of the normative contract; fixtures may target one or both.

**Container determinism (tar.gz):** For reproducible “container golden” builds the canonical writer SHOULD normalize so output is platform-independent: tar entry ordering fixed (e.g. manifest.json first, then events.ndjson); tar header uid, gid, uname, gname, mtime (e.g. 0 / epoch); gzip mtime and OS byte (e.g. mtime=0, OS=255 “unknown”). determinism_test pins the container hash for the canonical writer output; implementations that produce different tar/gzip header bytes are not byte-for-byte compatible with that pinned output. They may still be semantically compatible (verify passes), but are not deterministic-identical to the canonical writer output.

**Determinism goldens vs smoke goldens (do not conflate):**
- **Determinism goldens (pinned hashes):** The pinned hashes in `determinism_test.rs` apply to a bundle *generated inside that test* (in-memory), not to the file `test-bundle.tar.gz`. Updating “pinned hashes” means updating the assertions in `test_golden_hash` after changing the writer or event format.
- **Smoke goldens (file fixtures):** `test-bundle.tar.gz` is a separate file fixture used to verify layout, verify, lint, and explore; it uses a different event set (from `generate_fixture`). Regenerating `test-bundle.tar.gz` does not change the determinism_test pinned values.

**What is pinned (exact inputs):** (1) SHA-256 of the manifest.json bytes as stored in the tar; (2) SHA-256 of the events.ndjson bytes (UTF-8, one event per line, newline `\n`); (3) SHA-256 of the entire compressed tar.gz. Event-level content hashes use JCS (RFC 8785) canonicalization for the hash input. JCS is the norm for deterministic hashing and signing in v1; implementations that hash different normalized bytes are not compatible.

| Fixture | Purpose | Location | Determinism | Tests / consumers |
|---------|---------|----------|-------------|-------------------|
| test-bundle.tar.gz | Smoke: layout, verify, lint, explore | [tests/fixtures/evidence/](../../tests/fixtures/evidence/) | Generated by generate_fixture (see [fixture README](../../tests/fixtures/evidence/README.md)) | CI (action-v2-test, action-tests), verify_strict_test, lint |
| minimal | Single event, single type | In-memory in tests | In-memory or generated | verify_strict_test, lint_test create_golden_bundle |
| unknown optional fields | Event with extra optional fields; enforces compat promise | Normative (see [fixture README](../../tests/fixtures/evidence/README.md)) | Generated | **Contract:** verify MUST accept an event with extra unknown top-level and payload keys. A conformance test MUST exist; a dedicated on-disk fixture file is OPTIONAL. |
| invalid | Fail-closed: bad digest, malformed JSON, duplicate keys | In-memory / test constructs | N/A | bundle_security_test, verify_strict_test, payload_type_confusion_test |

**Required conformance tests (stable IDs):** Implementations and downstream consumers may reference these test IDs for assay-evidence-v1 conformance. MUST pass at least:

| Stable test ID | Purpose |
|----------------|---------|
| `test_verify_accepts_unknown_optional_fields` | Unknown fields compat: verify accepts event with extra keys |
| `test_verifier_rejects_missing_content_hash_raw_tar` | Security: content_hash required |
| `test_golden_hash` | Pinned format determinism (manifest, events, container hashes) |
| `bundle_security_test` (suite) | Duplicate keys, invalid Unicode reject |
| Lint/explore tests using smoke fixture | Layout, verify, lint, explore |

These IDs are stable; if a test is renamed, maintain a compatibility alias (e.g. old test name as wrapper) or document the change in the spec version history. See [Codebase verification](./EVIDENCE-CONTRACT-v1-VERIFICATION.md). Test names live in `crates/assay-evidence/tests/` (verify_strict_test.rs, determinism_test.rs, bundle_security_test.rs).

**Regeneration (reproducible-by-default):** For test-bundle.tar.gz, run `cargo test -p assay-evidence --test generate_fixture -- --ignored --nocapture` from the workspace root. Use the same Rust/toolchain and crate features as CI. **Pinned-hash assertions (exact location):** `crates/assay-evidence/tests/determinism_test.rs`, test `test_golden_hash`. Search for the three `assert_eq!` that compare `hash_bytes(&manifest)`, `hash_bytes(&events)`, and `_container_hash` to hex strings; those are the pinned values. After any change to the writer or event format, regenerate and update those assertions. See the [fixture README](../../tests/fixtures/evidence/README.md) for toolchain and hash locations.

## 7. Version history

| schema_version (bundle) | Date   | Changes |
|-------------------------|--------|---------|
| 1                        | 2026-01| Initial: manifest schema, events.ndjson, CloudEvents envelope per ADR-006/007. |
| 1                        | 2026-02| Contract freeze: this spec (compat matrix, evolution rules, deprecation, golden fixtures). |
| 1                        | 2026-02| New event types policy (§4.1): no new type without registry + schema + conformance test; Event types (v1) registry table (§4.2); §2 normative rule clarified (schema_version 1 + v1.0 baseline; verify MAY v1.x, pack MAY exact 1.0). |

## 8. Normative checklist (summary)

Before treating this document as normative, confirm:

- **Terminologie:** CloudEvents `specversion` vs Assay Evidence Spec are clearly separated; no conflation in tooling or tickets.
- **Unknown keys:** Verify MUST accept unknown keys unless one of the closed list applies (duplicate keys, invalid UTF-8/surrogate, size/count/decompression limits); no other reject reason for “unknown” alone.
- **v1.x additive-only:** In v1, bundle schema_version remains 1; Assay Evidence Spec minor (v1.1, v1.2) contains only additive changes; breaking ⇒ v2 or schema_version bump.
- **Deprecations:** Deprecations ship in a minor release; removals only in a major spec or new schema_version.
- **Container determinism:** Canonical writer SHOULD normalize tar/gzip headers; determinism_test pins container hash for that output.
- **Fixture regen:** Pinned-hash locations are exactly named (see §6: `determinism_test.rs`, test `test_golden_hash`, three `assert_eq!` hex values).
- **New event types (§4.1):** No new type without registry row + payload contract (concrete section for stable) + conformance test (verify + assaycontenthash invariant). Stable rows no "implied"; experimental/test-only temporary until full bar; promotion in version history.

## 9. References

- [ADR-006 Evidence Contract](../architecture/ADR-006-Evidence-Contract.md)
- [ADR-007 Deterministic Provenance](../architecture/ADR-007-Deterministic-Provenance.md)
- [SPEC-Pack-Engine-v1](../architecture/SPEC-Pack-Engine-v1.md)
- [SPEC-Replay-Bundle-v1](../architecture/SPEC-Replay-Bundle-v1.md)
- [Evidence fixture README](../../tests/fixtures/evidence/README.md)
- [Codebase verification](./EVIDENCE-CONTRACT-v1-VERIFICATION.md)
- RFC 2119 (MUST/SHOULD/MAY); SemVer 2.0.0; CloudEvents v1.x; RFC 8785 (JCS).
