"""Tests for the independent evidenceRef recomputation consumer.

Covers the full happy + negative matrix, the load-bearing fail-closed invariants, the cross-profile
(JCS vs deterministic-CBOR) distinctness, and the two-implementation interop bar: the reference
consumer and the independent reproducer must agree on every vector, byte for byte.
"""
import json
import pathlib
import subprocess
import sys

import evidenceref_consumer as ref
import independent_consumer as ind

HERE = pathlib.Path(__file__).parent


def _verdicts():
    cases = ref.build_corpus()
    return {c["id"]: ref.consume(c["ref"], c["body_store"])["verdict"] for c in cases}


def test_happy_paths_recompute_under_both_profiles():
    v = _verdicts()
    assert v["c1_happy_jcs"] == "recomputed"
    assert v["c2_happy_cbor"] == "recomputed"


def test_clean_only_on_independent_recomputation():
    v = _verdicts()
    clean = {cid for cid, verdict in v.items() if verdict == "recomputed"}
    assert clean == {"c1_happy_jcs", "c2_happy_cbor"}


def test_digest_only_and_unresolved_fail_closed():
    v = _verdicts()
    assert v["c3_digest_only_no_body"] == "unresolvable_digest_only"
    assert v["c4_unresolved_ref"] == "unresolved_ref"


def test_tamper_is_digest_mismatch():
    v = _verdicts()
    assert v["c5_digest_mismatch"] == "digest_mismatch"


def test_wrong_profile_is_canonicalization_mismatch_not_tamper():
    v = _verdicts()
    assert v["c6_canon_mismatch"] == "canonicalization_mismatch"


def test_unsupported_profile_is_never_assumed():
    v = _verdicts()
    assert v["c7_unsupported_canon"] == "unsupported_canonicalization"


def test_schema_mismatch_detected():
    v = _verdicts()
    assert v["c8_schema_mismatch"] == "schema_mismatch"


def test_redacted_projection_cannot_launder_missing_evidence():
    v = _verdicts()
    # A marked redaction and a silent elision both fail to look clean.
    assert v["c9_redacted_projection_marked"] == "redacted_projection_incomplete"
    assert v["c10_silent_elision"] == "redacted_projection_incomplete"


def test_producer_self_clean_never_promotes():
    v = _verdicts()
    assert v["c11_producer_clean_no_body"] == "unresolvable_digest_only"
    assert v["c12_producer_clean_mismatch"] == "digest_mismatch"


def test_producer_state_is_never_consulted():
    cases = {c["id"]: c for c in ref.build_corpus()}
    happy = cases["c1_happy_jcs"]
    tamper = cases["c5_digest_mismatch"]
    base_happy = ref.consume(happy["ref"], happy["body_store"])
    base_tamper = ref.consume(tamper["ref"], tamper["body_store"])
    assert ref.consume({**happy["ref"], "producer_state": "dirty"}, happy["body_store"]) == base_happy
    assert ref.consume({**tamper["ref"], "producer_state": "clean"}, tamper["body_store"]) == base_tamper


def test_malformed_refs():
    v = _verdicts()
    assert v["c13_malformed_no_digest"] == "malformed_ref"
    assert v["c14_malformed_no_canon"] == "malformed_ref"


def test_schema_authority_is_consumer_controlled():
    v = _verdicts()
    # A producer hint on the ref and a body-local _schema override both claim "no required fields";
    # neither can clean a redacted projection, because completeness is the consumer registry's call.
    assert v["c15_producer_schema_hint"] == "redacted_projection_incomplete"
    assert v["c16_body_local_schema_override"] == "redacted_projection_incomplete"


def test_producer_schema_hint_cannot_clean_or_move_a_verdict():
    cases = {c["id"]: c for c in ref.build_corpus()}
    redacted = cases["c9_redacted_projection_marked"]
    happy = cases["c1_happy_jcs"]
    base_redacted = ref.consume(redacted["ref"], redacted["body_store"])
    base_happy = ref.consume(happy["ref"], happy["body_store"])
    hint = {"producer_schema_hint": {"required_fields": []}}
    assert ref.consume({**redacted["ref"], **hint}, redacted["body_store"]) == base_redacted
    assert ref.consume({**happy["ref"], **hint}, happy["body_store"]) == base_happy


def test_canonicalization_profile_authority_is_consumer_controlled():
    v = _verdicts()
    # A producer that embeds its own profile definition (object instead of a name) is refused.
    assert v["c17_producer_defined_canon"] == "unsupported_canonicalization"


def test_producer_canonicalization_rules_sibling_is_never_read():
    cases = {c["id"]: c for c in ref.build_corpus()}
    happy = cases["c1_happy_jcs"]
    base = ref.consume(happy["ref"], happy["body_store"])
    ruled = ref.consume({**happy["ref"], "canonicalization_rules": "producer-defined-bogus"}, happy["body_store"])
    assert ruled == base


def test_profiles_are_envelope_distinct():
    obj = {"schema": "assay.policy_decision", "schema_version": "v1", "decision": "x",
           "effect": {"e": 1}, "target": {"t": 1}}
    assert ref.content_address(obj, "jcs-json-v1") != ref.content_address(obj, "cbor-deterministic-v1")


def test_measurement_passes_all_invariants():
    result = ref.measure(ref.emit())
    assert result["all_expected"] is True
    assert result["all_invariants_hold"] is True
    assert result["verdict_counts"]["recomputed"] == 2


def test_independent_reproducer_matches_reference():
    """Two-implementation interop bar: the independent runner reproduces every committed verdict."""
    doc = ref.emit()
    registry = doc["schema_registry"]
    for c in doc["cases"]:
        got = ind.reproduce(c["ref"], c["body_store"], registry)
        assert got == c["expected"], f"{c['id']}: independent={got} reference={c['expected']}"


def test_independent_consumer_imports_no_reference_code():
    """The independence is structural, not just claimed: parse the AST and prove no import statement
    pulls in the reference runner (a docstring mention of the filename does not count as sharing code)."""
    import ast

    tree = ast.parse((HERE / "independent_consumer.py").read_text())
    imported = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imported.update(a.name for a in node.names)
        elif isinstance(node, ast.ImportFrom):
            imported.add(node.module or "")
    assert not any("evidenceref_consumer" in m for m in imported), f"shares code via import: {imported}"


def test_cbor_is_rfc8949_deterministic():
    # Spot-check the core deterministic encoding against hand-computed bytes (RFC 8949 sec 3 / 4.2).
    assert ref._cbor(0) == b"\x00"
    assert ref._cbor(23) == b"\x17"
    assert ref._cbor(24) == b"\x18\x18"
    assert ref._cbor(256) == b"\x19\x01\x00"
    assert ref._cbor("a") == b"\x61a"
    assert ref._cbor([1, 2]) == b"\x82\x01\x02"
    # Map keys sort by their encoded bytes: shorter key "a" before longer key "bb".
    assert ref._cbor({"bb": 2, "a": 1}) == b"\xa2\x61a\x01\x62bb\x02"
    assert ref._cbor(True) == b"\xf5" and ref._cbor(False) == b"\xf4" and ref._cbor(None) == b"\xf6"


def test_cli_emit_and_independent_runner_agree_via_files(tmp_path):
    """End-to-end through the published bytes: emit vectors.json, both runners agree on it."""
    emitted = subprocess.run(
        [sys.executable, str(HERE / "evidenceref_consumer.py"), "emit"],
        capture_output=True, text=True, check=True,
    ).stdout
    (tmp_path / "vectors.json").write_text(emitted)
    doc = json.loads(emitted)
    assert doc["schema"] == "assay.experiment.evidenceref_recompute_consumer.v0"

    # Run from tmp_path so the confined vectors path resolves inside the working directory.
    verify = subprocess.run(
        [sys.executable, str(HERE / "evidenceref_consumer.py"), "verify", "vectors.json"],
        cwd=tmp_path, capture_output=True, text=True,
    )
    assert verify.returncode == 0, verify.stdout + verify.stderr

    independent = subprocess.run(
        [sys.executable, str(HERE / "independent_consumer.py"), "vectors.json"],
        cwd=tmp_path, capture_output=True, text=True,
    )
    assert independent.returncode == 0, independent.stdout + independent.stderr
    assert json.loads(independent.stdout)["all_reproduced"] is True


def test_vectors_path_is_confined_to_working_directory(tmp_path):
    """A path that resolves outside the working directory is refused rather than read."""
    outside = tmp_path / "vectors.json"
    outside.write_text("{}")
    # cwd here is the test's directory, not tmp_path, so the absolute path escapes and is refused.
    result = subprocess.run(
        [sys.executable, str(HERE / "independent_consumer.py"), str(outside)],
        capture_output=True, text=True,
    )
    assert result.returncode != 0
    assert "refusing an absolute vectors path" in (result.stdout + result.stderr)
