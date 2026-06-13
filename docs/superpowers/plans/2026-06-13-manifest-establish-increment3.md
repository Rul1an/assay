# Manifest Establish Increment 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `assay.manifest_establish.v0` consumable and regression-locked after the live pre-call establish flow lands.

**Architecture:** Increment 1 defined the pure establish decision/path model. Increment 2 wires live proxy-originated `tools/list` and emits a sibling per-call establish carrier. Increment 3 closes the producer/consumer loop: Assay emits a canonical producer fixture for the new carrier, Plimsoll vendors and validates it, and Plimsoll can summarize/gate malformed establish evidence without treating it as the allow/deny verdict. The enforcement verdict remains exclusively in `assay.enforcement_decision.v0`.

**Tech Stack:** Rust (`assay-mcp-server`, `serde_json`, Cargo tests), Python (`plimsoll.review`, `unittest`, `ruff`, `mypy`), GitHub Actions path-scoped freshness checks.

---

## Scope Boundaries

This increment starts **after Increment 2 has merged** and the proxy can write a per-call NDJSON stream for `assay.manifest_establish.v0`, expected as:

```text
assay-mcp-server --manifest-establish-out manifest-establish.ndjson
```

If Increment 2 chooses a different flag name, apply that exact name consistently in Task 3. Do not change `assay.enforcement_decision.v0`.

Increment 3 does **not**:

- originate `tools/list`;
- change `enforce::decide()`;
- infer delivery or side effects;
- use `establish_path` as a verdict proxy;
- require cross-carrier per-call joins without a stable correlation id.

`assay.manifest_establish.v0` answers: *what establish journey did the proxy take before/around the policy decision?*
`assay.enforcement_decision.v0` answers: *what policy decision did the proxy make?*

## Files

Use these roots in every command below:

```bash
ASSAY_ROOT=$(git rev-parse --show-toplevel)
PLIMSOLL_ROOT=$(cd ../plimsoll 2>/dev/null && pwd -P || cd "$ASSAY_ROOT/../plimsoll" && pwd -P)
```

Assay producer side:

- Modify: `crates/assay-mcp-server/src/proxy/establish.rs`
- Create: `crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json`

Plimsoll consumer side:

- Modify: `$PLIMSOLL_ROOT/src/plimsoll/review.py`
- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_contract.v0.json`
- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_contract.provenance.json`
- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_consumer_rejection.v0.json`
- Create: `$PLIMSOLL_ROOT/tests/test_manifest_establish_contract.py`
- Create: `$PLIMSOLL_ROOT/tests/test_manifest_establish.py`
- Create: `$PLIMSOLL_ROOT/scripts/check_vendored_assay_manifest_establish_contract.py`
- Create: `$PLIMSOLL_ROOT/.github/workflows/vendored-manifest-establish-contract.yml`
- Modify: `$PLIMSOLL_ROOT/CHANGELOG.md`

## Contract Shape

> **Increment 2c amendment (run_outcome) — SUPERSEDES the 4-field shape.** The carrier landed in 2c
> (`assay#1665`, slice 2c) with **five** fields: `run_outcome` is added. `build_manifest_establish_record`
> now takes `run_outcome: &str` (not a bool), and **`establish_attempted` is DERIVED** from it
> (`establish_attempted == (run_outcome != "not_performed")`) so the two can never disagree. Valid
> `run_outcome` values: `complete | timed_out | partial | transport_error | error_response |
> register_refused | not_performed` (`not_performed` ⇔ no establish ran). Per-path values:
> `no_establish_needed → not_performed`; `established_then_allowed → complete`;
> `established_then_denied → complete`; `immediate_deny` → `not_performed` (ambiguous / no attempt)
> OR a failed-run value (`timed_out` / `partial` / `transport_error` / `error_response` /
> `register_refused`) when a re-list was attempted but failed. The classifier MUST validate
> `run_outcome` against that set and enforce the derived `establish_attempted` invariant; a
> missing/invalid `run_outcome` is `malformed`. Every 4-field record example below is illustrative of
> the path/attempted logic only — the real record carries `run_outcome` as the fifth field.

The producer fixture contains one real record per stable (establish_path, run_outcome) shape:

```json
{
  "schema_contract": "assay.manifest_establish.v0",
  "generated_by": "assay crates/assay-mcp-server proxy::establish::build_manifest_establish_record (manifest_establish_contract_fixture)",
  "note": "Canonical producer output, regenerated from build_manifest_establish_record. Consumers vendor this file verbatim.",
  "records": [
    {
      "case": "no_establish_needed",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "not_performed"
      }
    },
    {
      "case": "established_then_allowed",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "established_then_allowed",
        "establish_attempted": true,
        "action_class": "github_deploy_key",
        "run_outcome": "complete"
      }
    },
    {
      "case": "established_then_denied",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "established_then_denied",
        "establish_attempted": true,
        "action_class": "github_deploy_key",
        "run_outcome": "complete"
      }
    },
    {
      "case": "immediate_deny",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "immediate_deny",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "not_performed"
      }
    },
    {
      "case": "unclassified_immediate_deny",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "immediate_deny",
        "establish_attempted": false,
        "action_class": null,
        "run_outcome": "not_performed"
      }
    },
    {
      "case": "immediate_deny_after_failed_establish",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "immediate_deny",
        "establish_attempted": true,
        "action_class": "github_deploy_key",
        "run_outcome": "timed_out"
      }
    }
  ]
}
```

`immediate_deny` is the only path that pairs with BOTH `establish_attempted` values: `false` /
`run_outcome:not_performed` when no establish was attempted (an inconclusive/ambiguous observation that
establish cannot resolve), and `true` with a failed `run_outcome` (`timed_out`/`partial`/
`transport_error`/`error_response`/`register_refused`) when a re-list WAS attempted but failed. Both are
valid producer output and both must round-trip; the consumer must not assume `immediate_deny ⇒ not
attempted`.

This deliberately carries no caller id, tool name, target digest, credential alias, transport, delivery,
side-effect, or `decision` field. Those belong to `assay.enforcement_decision.v0`.

## Task 1: Assay Producer Contract Fixture

**Files:**

- Modify: `crates/assay-mcp-server/src/proxy/establish.rs`
- Create: `crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json`

> **Already done by `assay#1659` — do NOT redo.** Steps 1–3 below (tightening the `establish_path`
> doc comment and adding the `NoEstablishNeeded + allow` coexistence test) were merged in `#1659`. The
> comment already reads "only establish-derived allow path" / "NoEstablishNeeded is orthogonal", and the
> test exists as `no_establish_needed_coexists_with_allow`. Re-applying Steps 1–3 verbatim would re-edit
> the comment and create a duplicate test under a second name. Treat Steps 1–3 as satisfied and start at
> **Step 4** (the producer-contract generation). Steps 1–3 are kept below only as the rationale of
> record.

- [ ] **Step 1: Tighten the path doc comment before adding the fixture**

Replace the `establish_path` comment with wording that cannot be misread as a verdict claim:

```rust
/// Resolve the establish journey from the action, the attempt outcome, and the re-evaluated decision.
///
/// Load-bearing invariant: the only establish-derived allow journey is `EstablishedThenAllowed`, and
/// it requires BOTH an `EstablishedComplete` outcome AND a `decide_allowed` re-evaluation.
/// `NoEstablishNeeded` is also compatible with a separate allowed `assay.enforcement_decision.v0`
/// record: it means the establish step was a no-op because a current complete observation already
/// existed. The policy verdict always lives in the separate enforcement-decision carrier.
```

- [ ] **Step 2: Add a unit test pinning `NoEstablishNeeded + allow` as valid**

Add this test inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn no_establish_needed_can_coexist_with_allowed_enforcement_decision() {
    assert_eq!(
        establish_path(
            EstablishAction::NotNeeded,
            EstablishOutcome::NotPerformed,
            true
        ),
        EstablishPath::NoEstablishNeeded
    );
}
```

- [ ] **Step 3: Run the focused test and confirm it passes**

Run:

```bash
cargo test -p assay-mcp-server no_establish_needed_can_coexist_with_allowed_enforcement_decision
```

Expected: one test passes.

- [ ] **Step 4: Add producer-contract generation helpers**

Add these helpers inside `#[cfg(test)] mod tests`:

```rust
fn manifest_establish_contract_fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/manifest_establish_contract.v0.json")
}

fn manifest_establish_contract_records() -> Vec<Value> {
    // build_manifest_establish_record takes run_outcome (&str) and DERIVES establish_attempted
    // (true iff run_outcome != "not_performed") — see Increment 2c.
    vec![
        json!({
            "case": "no_establish_needed",
            "record": build_manifest_establish_record(
                EstablishPath::NoEstablishNeeded,
                Some("github_deploy_key"),
                "not_performed",
            ),
        }),
        json!({
            "case": "established_then_allowed",
            "record": build_manifest_establish_record(
                EstablishPath::EstablishedThenAllowed,
                Some("github_deploy_key"),
                "complete",
            ),
        }),
        json!({
            "case": "established_then_denied",
            "record": build_manifest_establish_record(
                EstablishPath::EstablishedThenDenied,
                Some("github_deploy_key"),
                "complete",
            ),
        }),
        json!({
            "case": "immediate_deny",
            "record": build_manifest_establish_record(
                EstablishPath::ImmediateDeny,
                Some("github_deploy_key"),
                "not_performed",
            ),
        }),
        json!({
            "case": "unclassified_immediate_deny",
            "record": build_manifest_establish_record(
                EstablishPath::ImmediateDeny,
                None,
                "not_performed",
            ),
        }),
        json!({
            // immediate_deny is also reached when a re-list WAS attempted but failed
            // (EstablishFailed: timeout/partial/transport/unusable) -> run_outcome != not_performed,
            // so the derived establish_attempted = true.
            "case": "immediate_deny_after_failed_establish",
            "record": build_manifest_establish_record(
                EstablishPath::ImmediateDeny,
                Some("github_deploy_key"),
                "timed_out",
            ),
        }),
    ]
}

fn manifest_establish_contract_document() -> Value {
    json!({
        "schema_contract": MANIFEST_ESTABLISH_SCHEMA,
        "generated_by": "assay crates/assay-mcp-server proxy::establish::build_manifest_establish_record (manifest_establish_contract_fixture)",
        "note": "Canonical producer output, regenerated from build_manifest_establish_record. Consumers vendor this file verbatim. Regenerate with ASSAY_UPDATE_GOLDEN=1.",
        "records": manifest_establish_contract_records(),
    })
}
```

- [ ] **Step 5: Add the golden-fixture test**

Add this test:

```rust
#[test]
fn manifest_establish_contract_fixture() {
    let generated = manifest_establish_contract_document();
    let path = manifest_establish_contract_fixture_path();

    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&generated).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
    }

    let committed_text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed_text).unwrap();
    assert_eq!(
        committed, generated,
        "the committed manifest-establish contract fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
    );

    for entry in generated["records"].as_array().unwrap() {
        let rec = &entry["record"];
        assert_eq!(rec["schema"], json!(MANIFEST_ESTABLISH_SCHEMA));
        assert!(
            rec.get("decision").is_none(),
            "manifest-establish records must not carry policy verdicts"
        );
        assert!(
            rec.get("forwarded").is_none(),
            "manifest-establish records must not carry transport/delivery claims"
        );
        for forbidden in [
            "caller_id",
            "tool_name",
            "target_digest",
            "transport",
            "side_effect",
            "side_effects",
            "credential_alias",
        ] {
            assert!(
                rec.get(forbidden).is_none(),
                "manifest-establish records must not carry {forbidden}"
            );
        }
    }
}
```

- [ ] **Step 6: Generate the fixture**

Run:

```bash
ASSAY_UPDATE_GOLDEN=1 cargo test -p assay-mcp-server manifest_establish_contract_fixture
```

Expected: test passes and creates `crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json`.

- [ ] **Step 7: Verify the fixture is pinned**

Run:

```bash
cargo test -p assay-mcp-server manifest_establish_contract_fixture
cargo test -p assay-mcp-server establish
cargo fmt --check
```

Expected: all pass.

- [ ] **Step 8: Commit the assay producer contract**

Run:

```bash
git add crates/assay-mcp-server/src/proxy/establish.rs \
  crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json
git commit -s -m "test(mcp-server): pin manifest-establish carrier contract"
```

Expected: one signed commit. Open this as the Assay Increment 3 producer-contract PR and merge it before the Plimsoll vendor PR.

## Task 2: Plimsoll Manifest-Establish Classifier

**Files:**

- Modify: `$PLIMSOLL_ROOT/src/plimsoll/review.py`
- Create: `$PLIMSOLL_ROOT/tests/test_manifest_establish.py`

- [ ] **Step 1: Write failing classifier tests**

Create `$PLIMSOLL_ROOT/tests/test_manifest_establish.py`:

```python
import unittest

from plimsoll.review import (
    _classify_manifest_establish_record,
    manifest_establish_findings,
    manifest_establish_summary,
)


def _rec(path="no_establish_needed", attempted=False, action_class="github_deploy_key", run_outcome=None):
    # run_outcome defaults coherently with `attempted` (complete if attempted else not_performed); pass
    # it explicitly to test a failed-establish immediate_deny (e.g. run_outcome="timed_out").
    if run_outcome is None:
        run_outcome = "complete" if attempted else "not_performed"
    return {
        "schema": "assay.manifest_establish.v0",
        "establish_path": path,
        "establish_attempted": attempted,
        "action_class": action_class,
        "run_outcome": run_outcome,
    }


class TestManifestEstablish(unittest.TestCase):
    def test_valid_records_classify_by_path(self):
        self.assertEqual(_classify_manifest_establish_record(_rec()), "valid")
        self.assertEqual(
            _classify_manifest_establish_record(_rec("established_then_allowed", True)),
            "valid",
        )
        self.assertEqual(
            _classify_manifest_establish_record(_rec("established_then_denied", True)),
            "valid",
        )
        self.assertEqual(
            _classify_manifest_establish_record(_rec("immediate_deny", False, None)),
            "valid",
        )
        # immediate_deny pairs with BOTH attempted values: false = ambiguous (no attempt),
        # true = a re-list was attempted but failed (EstablishFailed). Both are valid.
        self.assertEqual(
            _classify_manifest_establish_record(_rec("immediate_deny", True)),
            "valid",
        )

    def test_unsupported_schema_and_malformed(self):
        self.assertEqual(
            _classify_manifest_establish_record({"schema": "other"}),
            "unsupported_schema",
        )
        self.assertEqual(_classify_manifest_establish_record("not-json"), "malformed")
        self.assertEqual(
            _classify_manifest_establish_record({"schema": "assay.manifest_establish.v0"}),
            "malformed",
        )

    def test_inconsistent_attempted_flag(self):
        self.assertEqual(
            _classify_manifest_establish_record(_rec("established_then_allowed", False)),
            "inconsistent",
        )
        self.assertEqual(
            _classify_manifest_establish_record(_rec("no_establish_needed", True)),
            "inconsistent",
        )

    def test_record_cannot_claim_verdict_or_delivery(self):
        rec = _rec()
        rec["decision"] = "allow"
        self.assertEqual(_classify_manifest_establish_record(rec), "inconsistent")
        rec = _rec()
        rec["forwarded"] = True
        self.assertEqual(_classify_manifest_establish_record(rec), "inconsistent")

    def test_summary_counts_valid_paths_only(self):
        summary = manifest_establish_summary(
            [
                _rec(),
                _rec("established_then_allowed", True),
                _rec("established_then_denied", True),
                _rec("immediate_deny", False),
                {"schema": "other"},
            ]
        )
        self.assertEqual(summary["schema"], "assay.manifest_establish.v0")
        self.assertEqual(summary["total"], 5)
        self.assertEqual(summary["counts"]["valid"], 4)
        self.assertEqual(summary["counts"]["unsupported_schema"], 1)
        self.assertEqual(summary["by_establish_path"]["no_establish_needed"], 1)
        self.assertEqual(summary["by_establish_path"]["established_then_allowed"], 1)
        self.assertEqual(summary["by_establish_path"]["established_then_denied"], 1)
        self.assertEqual(summary["by_establish_path"]["immediate_deny"], 1)
        self.assertIn("journey", " ".join(summary["non_claims"]))

    def test_findings_only_gate_when_expected(self):
        findings, warnings = manifest_establish_findings([{"schema": "other"}], expect=False)
        self.assertEqual(findings, [])
        self.assertEqual(len(warnings), 1)

        findings, warnings = manifest_establish_findings(None, expect=True)
        self.assertEqual(warnings, [])
        self.assertEqual(findings[0]["reason_code"], "manifest_establish_expected_but_absent")

        findings, warnings = manifest_establish_findings([{"schema": "other"}], expect=True)
        self.assertEqual(warnings, [])
        self.assertEqual(findings[0]["reason_code"], "manifest_establish_unsupported_schema")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest tests.test_manifest_establish -v
```

Expected: import errors for the missing functions.

- [ ] **Step 3: Add constants and classifier**

In `$PLIMSOLL_ROOT/src/plimsoll/review.py`, after the enforcement-decision section or immediately before it, add:

```python
# ----- manifest establish (P61e establish journey carrier) -------------------
#
# Separate from enforcement_decision: this carrier describes whether the proxy had to establish a
# fresh current complete manifest before the policy decision. It is a journey/operability carrier,
# never an allow/deny verdict proxy and never a delivery or side-effect claim.

MANIFEST_ESTABLISH_SCHEMA = "assay.manifest_establish.v0"

_MANIFEST_ESTABLISH_PATHS = {
    "no_establish_needed",
    "established_then_allowed",
    "established_then_denied",
    "immediate_deny",
}

# Only the three deterministic paths pin `establish_attempted`. `immediate_deny` is intentionally
# absent: it pairs with `false` (no establish attempted — an inconclusive/ambiguous observation) AND
# with `true` (a re-list was attempted but failed: EstablishFailed). Both are valid producer output, so
# `immediate_deny` is not constrained here.
_MANIFEST_ESTABLISH_ATTEMPTED = {
    "no_establish_needed": False,
    "established_then_allowed": True,
    "established_then_denied": True,
}

# Increment 2c: the diagnostic run outcome. `not_performed` ⇔ no establish ran. The producer derives
# `establish_attempted` from this (true iff run_outcome != "not_performed"), and the consumer enforces
# that invariant below.
_MANIFEST_ESTABLISH_RUN_OUTCOMES = {
    "complete",
    "timed_out",
    "partial",
    "transport_error",
    "error_response",
    "register_refused",
    "not_performed",
}


def _classify_manifest_establish_record(rec) -> str:
    if not isinstance(rec, dict):
        return "malformed"
    if rec.get("schema") != MANIFEST_ESTABLISH_SCHEMA:
        return "unsupported_schema"
    path = rec.get("establish_path")
    attempted = rec.get("establish_attempted")
    run_outcome = rec.get("run_outcome")
    if (
        path not in _MANIFEST_ESTABLISH_PATHS
        or not isinstance(attempted, bool)
        or run_outcome not in _MANIFEST_ESTABLISH_RUN_OUTCOMES
    ):
        return "malformed"
    action_class = rec.get("action_class")
    if action_class is not None and not (isinstance(action_class, str) and action_class):
        return "malformed"
    for forbidden in (
        "caller_id",
        "tool_name",
        "target_digest",
        "transport",
        "side_effect",
        "side_effects",
        "credential_alias",
        "forwarded",
        "decision",
    ):
        if forbidden in rec:
            return "inconsistent"
    expected_attempted = _MANIFEST_ESTABLISH_ATTEMPTED.get(path)
    if expected_attempted is not None and attempted is not expected_attempted:
        return "inconsistent"
    # Derived invariant: establish_attempted iff an establish actually ran (run_outcome != not_performed).
    if attempted != (run_outcome != "not_performed"):
        return "inconsistent"
    # immediate_deny accepts either attempted value (ambiguous vs failed establish); the run_outcome
    # invariant above already constrains the pair.
    return "valid"
```

- [ ] **Step 4: Add summary and findings**

Add:

```python
def manifest_establish_summary(records) -> dict | None:
    if not records:
        return None
    counts = {"valid": 0, "malformed": 0, "unsupported_schema": 0, "inconsistent": 0}
    by_path: dict = {}
    by_action_class: dict = {}
    attempted = 0
    for rec in records:
        cat = _classify_manifest_establish_record(rec)
        counts[cat] += 1
        if cat == "valid":
            path = rec["establish_path"]
            by_path[path] = by_path.get(path, 0) + 1
            ac = rec.get("action_class")
            by_action_class[ac] = by_action_class.get(ac, 0) + 1
            if rec["establish_attempted"]:
                attempted += 1
    return {
        "schema": MANIFEST_ESTABLISH_SCHEMA,
        "total": sum(counts.values()),
        "counts": counts,
        "defect_count": counts["malformed"] + counts["unsupported_schema"] + counts["inconsistent"],
        "establish_attempted_count": attempted,
        "by_establish_path": by_path,
        "by_action_class": by_action_class,
        "non_claims": [
            "establish_path is the manifest-establish journey, not the allow/deny verdict",
            "NoEstablishNeeded can coexist with an allowed enforcement_decision; the verdict lives in assay.enforcement_decision.v0",
            "no delivery, transport, credential, or side-effect claim is made by this carrier",
        ],
    }


def manifest_establish_findings(records, expect: bool):
    findings: list = []
    warnings: list = []
    defects = {"malformed": 0, "unsupported_schema": 0, "inconsistent": 0}
    if records:
        for rec in records:
            cat = _classify_manifest_establish_record(rec)
            if cat in defects:
                defects[cat] += 1
    defect_total = sum(defects.values())

    if not expect:
        if records and defect_total:
            warnings.append(
                f"manifest_establish stream has {defect_total} unusable record(s) "
                "(malformed/unsupported_schema/inconsistent); inspected only, not gated "
                "(policy.expect_manifest_establish is off)"
            )
        return findings, warnings

    if not records:
        findings.append(
            {
                "kind": "manifest_establish",
                "item": "manifest_establish",
                "reason": "manifest establish evidence was expected but is absent or empty",
                "reason_code": "manifest_establish_expected_but_absent",
            }
        )
        return findings, warnings

    for cat in ("malformed", "unsupported_schema", "inconsistent"):
        if defects[cat]:
            findings.append(
                {
                    "kind": "manifest_establish",
                    "item": f"manifest_establish:{cat}",
                    "reason": f"{defects[cat]} {cat} manifest_establish record(s) in an expected stream",
                    "reason_code": f"manifest_establish_{cat}",
                    "count": defects[cat],
                }
            )
    return findings, warnings
```

- [ ] **Step 5: Run focused tests**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest tests.test_manifest_establish -v
```

Expected: all tests pass.

## Task 3: Plimsoll Review and CLI Integration

**Files:**

- Modify: `$PLIMSOLL_ROOT/src/plimsoll/review.py`
- Modify: `$PLIMSOLL_ROOT/tests/test_manifest_establish.py`

- [ ] **Step 1: Extend `build_review` and CLI loading**

Add a new optional parameter to `build_review`:

```python
def build_review(
    before_path,
    after_path,
    before,
    after,
    policy,
    require_coverage,
    enforcement_health=None,
    tool_decisions=None,
    declared_tool_surface=None,
    provider_audit_records=None,
    mcp_manifest_observed=None,
    mcp_declared_manifest=None,
    enforcement_decisions=None,
    manifest_establish=None,
) -> dict:
```

Inside `build_review`, after enforcement-decision findings:

```python
    manifest_establish_gate, manifest_establish_warnings = manifest_establish_findings(
        manifest_establish, bool(policy.get("expect_manifest_establish", False))
    )
    if manifest_establish_gate:
        seen_me = {(x["kind"], x["item"]) for x in manifest_establish_gate}
        f = manifest_establish_gate + [x for x in f if (x["kind"], x["item"]) not in seen_me]
```

When building warnings:

```python
    warnings.extend(manifest_establish_warnings)
```

In the `review` object:

```python
        "manifest_establish": manifest_establish_summary(manifest_establish),
```

- [ ] **Step 2: Add the NDJSON loader**

Add:

```python
def _load_manifest_establish(path: str) -> list:
    """Load the NDJSON `assay.manifest_establish.v0` stream.

    Tolerant like enforcement_decision: an unparsable line is retained as a sentinel so the
    classifier reports `malformed` rather than crashing, without retaining the raw broken content.
    """
    out: list = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                out.append({"_malformed_line": True})
    return out
```

- [ ] **Step 3: Add the CLI flag**

Add to the `diff` parser:

```python
    d.add_argument(
        "--manifest-establish",
        default="",
        help="path to the NDJSON assay.manifest_establish.v0 stream (the enforcing proxy's "
        "pre-call manifest-establish journey per privileged call). Separate carrier: it records "
        "whether a current complete manifest had to be established before the policy decision. "
        "It is never an allow/deny verdict, delivery proof, or side-effect verification. "
        "Descriptive by default; gates only when policy.expect_manifest_establish is true",
    )
```

In `cmd_diff`, load and pass the stream:

```python
    manifest_establish = None
    if getattr(args, "manifest_establish", ""):
        manifest_establish = _load_manifest_establish(args.manifest_establish)
```

Pass `manifest_establish=manifest_establish` into `build_review`.

- [ ] **Step 4: Add end-to-end review tests**

Append to `$PLIMSOLL_ROOT/tests/test_manifest_establish.py`:

```python
import json
import tempfile

from plimsoll.review import build_review, default_policy


def _surface():
    return {
        "schema": "assay.capability_surface.v1",
        "files": [],
        "network": [],
        "tools": [],
        "observation_health": {"coverage": "sufficient", "surfaces": {"files": "sufficient", "network": "sufficient", "tools": "sufficient"}},
    }


class TestManifestEstablishReviewIntegration(unittest.TestCase):
    def test_review_carries_manifest_establish_summary_when_supplied(self):
        with tempfile.TemporaryDirectory() as td:
            before = f"{td}/before.json"
            after = f"{td}/after.json"
            data = _surface()
            with open(before, "w", encoding="utf-8") as fh:
                json.dump(data, fh)
            with open(after, "w", encoding="utf-8") as fh:
                json.dump(data, fh)
            review = build_review(
                before,
                after,
                data,
                data,
                default_policy(),
                True,
                manifest_establish=[
                    {
                        "schema": "assay.manifest_establish.v0",
                        "establish_path": "no_establish_needed",
                        "establish_attempted": False,
                        "action_class": "github_deploy_key",
                    }
                ],
            )
        self.assertEqual(review["manifest_establish"]["counts"]["valid"], 1)
        self.assertEqual(review["decision"], "auto_clear_no_new_capability")

    def test_expected_manifest_establish_absent_gates_pending(self):
        with tempfile.TemporaryDirectory() as td:
            before = f"{td}/before.json"
            after = f"{td}/after.json"
            data = _surface()
            with open(before, "w", encoding="utf-8") as fh:
                json.dump(data, fh)
            with open(after, "w", encoding="utf-8") as fh:
                json.dump(data, fh)
            policy = default_policy()
            policy["expect_manifest_establish"] = True
            review = build_review(before, after, data, data, policy, True)
        self.assertEqual(review["decision"], "pending")
        self.assertEqual(
            review["findings_requiring_approval"][0]["reason_code"],
            "manifest_establish_expected_but_absent",
        )
```

- [ ] **Step 5: Run review integration tests**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest tests.test_manifest_establish -v
```

Expected: all tests pass.

## Task 4: Plimsoll Producer/Consumer Contract Fixture

**Files:**

- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_contract.v0.json`
- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_contract.provenance.json`
- Create: `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_consumer_rejection.v0.json`
- Create: `$PLIMSOLL_ROOT/tests/test_manifest_establish_contract.py`

- [ ] **Step 1: Vendor the Assay fixture byte-for-byte**

From the merged Assay producer-contract PR:

```bash
cd "$PLIMSOLL_ROOT"
cp "$ASSAY_ROOT/crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json" \
  tests/fixtures/manifest_establish_contract.v0.json
shasum -a 256 tests/fixtures/manifest_establish_contract.v0.json
```

Expected: record the printed SHA-256 for the provenance sidecar.

- [ ] **Step 2: Create provenance sidecar**

Create `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_contract.provenance.json`
from the merged Assay producer-contract branch:

```bash
cd "$PLIMSOLL_ROOT"
SOURCE_COMMIT=$(git -C "$ASSAY_ROOT" rev-parse HEAD)
SOURCE_PR=$(gh pr list \
  --repo Rul1an/assay \
  --state merged \
  --search "$SOURCE_COMMIT" \
  --json number \
  --jq '.[0].number')
SHA256=$(shasum -a 256 tests/fixtures/manifest_establish_contract.v0.json | awk '{print $1}')
python3 - <<PY
import json
from pathlib import Path

doc = {
    "vendored_file": "tests/fixtures/manifest_establish_contract.v0.json",
    "source_repo": "Rul1an/assay",
    "source_path": "crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json",
    "source_commit": "$SOURCE_COMMIT",
    "source_pr": "$SOURCE_PR",
    "sha256": "$SHA256",
}
Path("tests/fixtures/manifest_establish_contract.provenance.json").write_text(
    json.dumps(doc, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
```

Expected: the sidecar records the exact source commit, source PR number, and vendored file digest.

- [ ] **Step 3: Create rejection fixture**

Create `$PLIMSOLL_ROOT/tests/fixtures/manifest_establish_consumer_rejection.v0.json`:

```json
{
  "schema_contract": "assay.manifest_establish.v0 consumer-only rejection cases",
  "rejected": [
    {
      "case": "wrong_schema",
      "record": {
        "schema": "assay.enforcement_decision.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key"
      },
      "expected": "unsupported_schema"
    },
    {
      "case": "missing_path",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_attempted": false,
        "action_class": "github_deploy_key"
      },
      "expected": "malformed"
    },
    {
      "case": "unknown_path",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "established_then_forwarded",
        "establish_attempted": true,
        "action_class": "github_deploy_key"
      },
      "expected": "malformed"
    },
    {
      "case": "missing_run_outcome",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key"
      },
      "expected": "malformed"
    },
    {
      "case": "invalid_run_outcome",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "succeeded"
      },
      "expected": "malformed"
    },
    {
      "case": "attempted_flag_contradicts_path",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "established_then_allowed",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "complete"
      },
      "expected": "inconsistent"
    },
    {
      "case": "attempted_flag_contradicts_run_outcome",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "immediate_deny",
        "establish_attempted": true,
        "action_class": "github_deploy_key",
        "run_outcome": "not_performed"
      },
      "expected": "inconsistent"
    },
    {
      "case": "verdict_field_present",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "not_performed",
        "decision": "allow"
      },
      "expected": "inconsistent"
    },
    {
      "case": "delivery_field_present",
      "record": {
        "schema": "assay.manifest_establish.v0",
        "establish_path": "no_establish_needed",
        "establish_attempted": false,
        "action_class": "github_deploy_key",
        "run_outcome": "not_performed",
        "forwarded": true
      },
      "expected": "inconsistent"
    }
  ]
}
```

- [ ] **Step 4: Create contract test**

Create `$PLIMSOLL_ROOT/tests/test_manifest_establish_contract.py`:

```python
"""Shared producer/consumer contract for `assay.manifest_establish.v0`.

The producer is the assay enforcing proxy establish layer. The vendored fixture is byte-for-byte
producer output from `build_manifest_establish_record`; rejection cases are consumer-only records the
producer must never emit.
"""

import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from plimsoll.review import (  # noqa: E402
    MANIFEST_ESTABLISH_SCHEMA,
    _MANIFEST_ESTABLISH_PATHS,
    _classify_manifest_establish_record,
)

FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
PRODUCER_FIXTURE = FIXTURES / "manifest_establish_contract.v0.json"
REJECTION_FIXTURE = FIXTURES / "manifest_establish_consumer_rejection.v0.json"


def _load(path: pathlib.Path) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


class TestManifestEstablishContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.producer = _load(PRODUCER_FIXTURE)
        cls.cases = cls.producer["records"]
        cls.records = [e["record"] for e in cls.cases]
        cls.rejection = _load(REJECTION_FIXTURE)["rejected"]

    def test_vendored_fixture_is_the_producer_contract(self):
        self.assertEqual(self.producer["schema_contract"], MANIFEST_ESTABLISH_SCHEMA)
        self.assertIn("build_manifest_establish_record", self.producer["generated_by"])

    def test_every_producer_record_is_valid(self):
        for entry in self.cases:
            with self.subTest(case=entry["case"]):
                self.assertEqual(_classify_manifest_establish_record(entry["record"]), "valid")

    def test_producer_records_cover_every_establish_path(self):
        covered = {r["establish_path"] for r in self.records}
        self.assertEqual(covered, set(_MANIFEST_ESTABLISH_PATHS))

    def test_producer_records_carry_no_verdict_or_delivery(self):
        for entry in self.cases:
            with self.subTest(case=entry["case"]):
                rec = entry["record"]
                for forbidden in (
                    "caller_id",
                    "tool_name",
                    "target_digest",
                    "transport",
                    "side_effect",
                    "side_effects",
                    "credential_alias",
                    "forwarded",
                    "decision",
                ):
                    self.assertNotIn(forbidden, rec)

    def test_rejection_fixture_is_rejected_as_expected(self):
        for entry in self.rejection:
            with self.subTest(case=entry["case"]):
                self.assertEqual(
                    _classify_manifest_establish_record(entry["record"]),
                    entry["expected"],
                )


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 5: Run contract tests**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest tests.test_manifest_establish_contract -v
```

Expected: all tests pass.

## Task 5: Plimsoll Vendor Freshness Guard

**Files:**

- Create: `$PLIMSOLL_ROOT/scripts/check_vendored_assay_manifest_establish_contract.py`
- Create: `$PLIMSOLL_ROOT/.github/workflows/vendored-manifest-establish-contract.yml`

- [ ] **Step 1: Copy and adapt the existing freshness script**

Copy the existing enforcement-decision guard:

```bash
cd "$PLIMSOLL_ROOT"
cp scripts/check_vendored_assay_contract.py scripts/check_vendored_assay_manifest_establish_contract.py
```

Then replace:

```text
enforcement_decision_contract
```

with:

```text
manifest_establish_contract
```

and replace the module docstring with:

```python
"""Vendor-freshness guard for the assay.manifest_establish.v0 contract fixture.

`tests/fixtures/manifest_establish_contract.v0.json` is the assay producer's own output, vendored
verbatim. Default mode is deterministic and offline: it re-hashes the vendored file and compares it
to the SHA-256 recorded in its provenance sidecar. Optional --fetch compares against the pinned
assay commit through the gh CLI.
"""
```

- [ ] **Step 2: Run self-test and local check**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python3 scripts/check_vendored_assay_manifest_establish_contract.py --self-test
python3 scripts/check_vendored_assay_manifest_establish_contract.py
```

Expected:

```text
self-test=passed
vendored-assay-contract=ok
```

- [ ] **Step 3: Add path-scoped workflow**

Create `$PLIMSOLL_ROOT/.github/workflows/vendored-manifest-establish-contract.yml`:

```yaml
name: Vendored Manifest Establish Contract Freshness

on:
  pull_request:
    paths:
      - "tests/fixtures/manifest_establish_contract.v0.json"
      - "tests/fixtures/manifest_establish_contract.provenance.json"
      - "scripts/check_vendored_assay_manifest_establish_contract.py"
      - ".github/workflows/vendored-manifest-establish-contract.yml"
  push:
    branches: [main]
    paths:
      - "tests/fixtures/manifest_establish_contract.v0.json"
      - "tests/fixtures/manifest_establish_contract.provenance.json"
      - "scripts/check_vendored_assay_manifest_establish_contract.py"
      - ".github/workflows/vendored-manifest-establish-contract.yml"
  workflow_dispatch:

permissions: {}

concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  vendored-manifest-establish-contract:
    name: Vendored Manifest Establish Contract Freshness
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10
        with:
          persist-credentials: false

      - name: Self-test freshness guard
        run: python3 scripts/check_vendored_assay_manifest_establish_contract.py --self-test

      - name: Verify vendored fixture matches recorded provenance
        run: python3 scripts/check_vendored_assay_manifest_establish_contract.py
```

- [ ] **Step 4: Verify workflow YAML parses**

Run:

```bash
cd "$PLIMSOLL_ROOT"
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/vendored-manifest-establish-contract.yml"); puts "yaml=ok"'
```

Expected:

```text
yaml=ok
```

## Task 6: Plimsoll Full Verification and PR

**Files:**

- Modify: `$PLIMSOLL_ROOT/CHANGELOG.md`

- [ ] **Step 1: Add changelog entry**

Under `[Unreleased]`, add:

```markdown
- Consume `assay.manifest_establish.v0` as a separate pre-call manifest-establish journey carrier:
  descriptive by default, gateable with `expect_manifest_establish`, and never treated as the
  enforcement verdict or delivery proof.
```

- [ ] **Step 2: Run full Plimsoll gates**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest
ruff check .
ruff format --check .
mypy src/plimsoll
python3 scripts/check_vendored_assay_manifest_establish_contract.py --self-test
python3 scripts/check_vendored_assay_manifest_establish_contract.py
```

Expected: all pass.

- [ ] **Step 3: Commit Plimsoll consumer**

Run:

```bash
cd "$PLIMSOLL_ROOT"
git add src/plimsoll/review.py \
  tests/test_manifest_establish.py \
  tests/test_manifest_establish_contract.py \
  tests/fixtures/manifest_establish_contract.v0.json \
  tests/fixtures/manifest_establish_contract.provenance.json \
  tests/fixtures/manifest_establish_consumer_rejection.v0.json \
  scripts/check_vendored_assay_manifest_establish_contract.py \
  .github/workflows/vendored-manifest-establish-contract.yml \
  CHANGELOG.md
git commit -m "feat(review): consume manifest-establish carrier"
```

Expected: one Plimsoll commit.

- [ ] **Step 4: Open Plimsoll PR**

Run:

```bash
cd "$PLIMSOLL_ROOT"
git push -u origin codex/manifest-establish-consumer
gh pr create \
  --repo Rul1an/plimsoll \
  --base main \
  --head codex/manifest-establish-consumer \
  --title "feat(review): consume assay.manifest_establish.v0 carrier" \
  --body "Consumes the pre-call manifest-establish journey carrier as a separate Plimsoll review block. Descriptive by default, gateable with policy.expect_manifest_establish, and explicitly not an allow/deny verdict or delivery proof. Vendors the Assay producer fixture byte-for-byte and adds a path-scoped freshness guard."
```

Expected: ready PR with CI running.

## Task 7: Acceptance Criteria

**Files:**

- Assay producer PR from Task 1
- Plimsoll consumer PR from Task 6

- [ ] **Assay acceptance**

Run:

```bash
cd "$ASSAY_ROOT"
cargo test -p assay-mcp-server manifest_establish_contract_fixture
cargo test -p assay-mcp-server establish
cargo fmt --check
```

Expected: all pass.

- [ ] **Plimsoll acceptance**

Run:

```bash
cd "$PLIMSOLL_ROOT"
python -m unittest tests.test_manifest_establish tests.test_manifest_establish_contract -v
python -m unittest
ruff check .
ruff format --check .
mypy src/plimsoll
```

Expected: all pass.

- [ ] **Contract acceptance**

Verify:

```bash
cd "$PLIMSOLL_ROOT"
python3 scripts/check_vendored_assay_manifest_establish_contract.py
```

Expected:

```text
vendored-assay-contract=ok
```

- [ ] **Claim-boundary acceptance**

Search:

```bash
cd "$PLIMSOLL_ROOT"
rg -n '"forwarded"|"decision"|"credential_alias"|"caller_id"|"tool_name"|"target_digest"|"transport"|"side_effect"' tests/fixtures/manifest_establish_contract.v0.json src/plimsoll/review.py tests/test_manifest_establish*.py
```

Expected: fixture has no `forwarded`, no `decision`, no `credential_alias`, no caller/tool/target/transport fields, and no side-effect claim fields; consumer tests mention those strings only as rejection cases.

## Self-Review

Spec coverage:

- Producer contract fixture is covered by Task 1.
- Consumer classifier, summary, findings, CLI loading, and build-review integration are covered by Tasks 2 and 3.
- Producer/consumer freshness loop is covered by Tasks 4 and 5.
- Verification and PR handoff are covered by Tasks 6 and 7.
- The Increment 2/3 boundary is explicit: no live proxy origination work appears in this plan.

Placeholder scan:

- The only angle-bracket values appear in the provenance creation step and are explicitly replaced by concrete PR/commit values before committing.
- No code step relies on unnamed error handling or unspecified tests.

Type consistency:

- The schema constant is `MANIFEST_ESTABLISH_SCHEMA`.
- The classifier is `_classify_manifest_establish_record`.
- The summary is `manifest_establish_summary`.
- The gate is `manifest_establish_findings`.
- The CLI flag is `--manifest-establish`.
- The policy key is `expect_manifest_establish`.

Plan complete and saved to `docs/superpowers/plans/2026-06-13-manifest-establish-increment3.md`. Two execution options:

1. **Subagent-Driven (recommended)** - Dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints.
