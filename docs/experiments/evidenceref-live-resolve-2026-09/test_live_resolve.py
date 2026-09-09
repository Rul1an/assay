"""Tests for the live evidenceRef resolution run.

Three things are pinned here rather than demonstrated: that the capture is self-verifying, that the
refusal ORDER holds (integrity before interpretation), and that the two runners agree. A shell run
proves the resolution worked this afternoon; only these keep it working.
"""
import ast
import hashlib
import json
import pathlib
import subprocess
import sys

import pytest

import independent_resolve as ind
import resolve as ref

HERE = pathlib.Path(__file__).parent
OCTETS = (HERE / "record" / "gate-record-4dffe218.json").read_bytes()
BODY = json.loads(OCTETS)


def test_capture_is_self_verifying():
    # The pinned bytes hash to the last path segment of the URL they came from, so the capture needs
    # no trust in the capturer. This is the only reason the record may sit in this repo at all.
    manifest = json.loads((HERE / "record" / "capture.json").read_text())
    digest = hashlib.sha256(OCTETS).hexdigest()
    assert manifest["source_url"].rsplit("/", 1)[-1] == digest
    assert manifest["content_address"] == "sha256:" + digest
    assert manifest["byte_length"] == len(OCTETS)


def test_every_case_lands_where_expected():
    out = ref.run()
    assert out["all_expected"], [c for c in out["cases"] if not c["match"]]
    # Derived, not asserted against a literal: the roster follows build_cases so a new case cannot
    # slip in unexercised, and mutate.py scopes itself the same way.
    assert len(out["cases"]) == len(ref.build_cases())
    assert len({c["id"] for c in out["cases"]}) == len(out["cases"])


def test_one_object_three_addresses_and_jcs_is_not_the_served_bytes():
    out = ref.run()
    addrs = out["content_addresses_of_one_object"]
    assert len({addrs["octets_as_served"], addrs["jcs_json_v1"], addrs["cbor_deterministic_v1"]}) == 3
    # The whole point, and the reason a scheme name has to travel: same object, same byte count,
    # different address. A consumer that guesses the profile gets a mismatch, not an error.
    assert out["same_object_same_length_different_address"] is True


def test_value_space_is_inside_the_naive_jcs_scope():
    # The published consumer's JCS is scoped to a float-free value space. This asserts the subject
    # record is inside that scope, so the B verdict is a real RFC 8785 result and not an artifact of
    # naive key sorting. Without this the digest_mismatch in B would prove nothing.
    ind.assert_value_space(BODY)
    assert not any(b > 127 for b in OCTETS)


def test_refusal_order_integrity_before_interpretation():
    # A body that is BOTH tampered and incomplete must be refused at the address, never at the
    # completeness rule. If interpretation ran first, a producer could pick which rule judged it.
    trimmed = {k: v for k, v in BODY.items() if k != "absence_vs_failure"}
    both = json.dumps(trimmed, separators=(",", ":"), ensure_ascii=False).encode()
    r = ref.consume_octets(
        {"digest": ref.ADDRESS, "ref": "r", "canonicalization": ref.OCTETS_PROFILE,
         "schema": "mcp-verification-gate", "schema_version": "0.4.1"},
        {"r": both},
    )
    assert r["verdict"] == "digest_mismatch"
    # Positive control on the same line: the untouched body under the same reference IS clean, so the
    # assertion above is discriminating rather than passing for an unrelated reason.
    clean = ref.consume_octets(
        {"digest": ref.ADDRESS, "ref": "r", "canonicalization": ref.OCTETS_PROFILE,
         "schema": "mcp-verification-gate", "schema_version": "0.4.1"},
        {"r": OCTETS},
    )
    assert clean["verdict"] == "recomputed"


def test_schema_authority_is_consumer_side():
    # The record carries no schema identity of its own, so a producer cannot name one. Declaring an
    # identity the consumer's registry does not hold is refused rather than accepted on the word of
    # the reference.
    assert "schema" not in BODY and "schema_version" not in BODY
    r = ref.consume_octets(
        {"digest": ref.ADDRESS, "ref": "r", "canonicalization": ref.OCTETS_PROFILE,
         "schema": "whatever-the-producer-says", "schema_version": "1"},
        {"r": OCTETS},
    )
    assert r["verdict"] == "schema_mismatch"


def test_unsupported_profile_never_defaults():
    for name in ("gate-internal-v9", "", None, {"rules": "embedded"}):
        r = ref.consume_octets(
            {"digest": ref.ADDRESS, "ref": "r", "canonicalization": name,
             "schema": "mcp-verification-gate", "schema_version": "0.4.1"},
            {"r": OCTETS},
        )
        assert r["verdict"] in ("unsupported_canonicalization", "malformed_ref"), (name, r)


def test_independent_reproducer_shares_no_code():
    tree = ast.parse((HERE / "independent_resolve.py").read_text())
    imported = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imported.update(a.name for a in node.names)
        elif isinstance(node, ast.ImportFrom):
            imported.add(node.module or "")
    assert not any(m in ("resolve", "evidenceref_consumer") for m in imported), imported


def test_both_runners_agree_end_to_end():
    subprocess.run([sys.executable, "resolve.py"], cwd=HERE, check=True, capture_output=True)
    r = subprocess.run([sys.executable, "independent_resolve.py"], cwd=HERE, capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr
    n = len(ref.build_cases())
    assert f"agrees on {n} of {n}" in r.stdout


# The consumer as linked from the June comment in SEP-1913, pinned by commit AND blob. Comparing
# against HEAD instead would follow any future edit of the consumer and still pass, which is the
# opposite of the property this guard claims. The two are the same object today; that is a fact
# about today, not a guarantee.
PUBLISHED_COMMIT = "a052bf7f68d1e371485e33541c3e4a48e9ec6ef5"
PUBLISHED_BLOB = "f48bb7410935caddd19f3d3b6d4a789b4bd02e97"
PUBLISHED_PATH = "docs/experiments/evidenceref-recompute-consumer-2026-06/evidenceref_consumer.py"


def test_published_consumer_is_the_frozen_blob():
    frozen = subprocess.run(
        ["git", "rev-parse", f"{PUBLISHED_COMMIT}:{PUBLISHED_PATH}"],
        cwd=HERE.parents[2], capture_output=True, text=True,
    )
    assert frozen.returncode == 0, frozen.stderr
    assert frozen.stdout.strip() == PUBLISHED_BLOB, "the pinned commit no longer names the pinned blob"

    on_disk = subprocess.run(
        ["git", "hash-object", str(HERE.parent / "evidenceref-recompute-consumer-2026-06" / "evidenceref_consumer.py")],
        cwd=HERE.parents[2], capture_output=True, text=True,
    )
    assert on_disk.returncode == 0, on_disk.stderr
    assert on_disk.stdout.strip() == PUBLISHED_BLOB, "the consumer on disk is not the published object"


def test_every_octets_rule_is_exercised():
    # Pins the mutation result: a rule no case can kill is decoration. This is the check that found
    # four unexercised rules on the first run, and the roster is derived so a later case cannot
    # silently escape it.
    import mutate

    r = subprocess.run([sys.executable, "mutate.py"], cwd=HERE, capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr
    report = json.loads((HERE / "runs" / "mutation.json").read_text())
    assert report["all_killed"] is True
    for rule, res in report["rules"].items():
        assert res["valid_kill"], (rule, res)
    assert set(mutate.octet_case_ids()) == {c["id"] for c in ref.build_cases() if c["mode"] == "octets_consumer"}


def test_our_jcs_matches_a_real_rfc8785_implementation():
    # The B verdict only means something if our JCS really is RFC 8785 on this record. Cross-checked
    # against an independent library rather than reasoned from the value space alone.
    # No importorskip. A silently skipped cross-check leaves the B verdict standing on our own
    # JCS alone, which is the one thing this test exists to corroborate, while the suite still
    # reports a clean run. A missing dependency is a failure here, not a skip.
    try:
        import rfc8785
    except ImportError as exc:  # pragma: no cover
        raise AssertionError(
            "rfc8785 is required for this suite; run "
            "`uv run --with pytest --with rfc8785 python3 -m pytest -q test_live_resolve.py`"
        ) from exc
    ours = ref.published.canonical_bytes(BODY, "jcs-json-v1")
    assert ours == rfc8785.dumps(BODY)
    assert hashlib.sha256(ours).hexdigest() != hashlib.sha256(OCTETS).hexdigest()


def test_capture_refuses_bytes_that_are_not_the_address_the_url_names():
    # The wrong-address case: bytes that are perfectly valid, served from a URL claiming a
    # different digest. Comparing only against our pinned copy would accept this.
    import capture

    ok, got, claimed = capture.address_matches(OCTETS, capture.MANIFEST["source_url"])
    assert ok and got == claimed
    wrong = "https://gate.horizonshield.dev/record/" + "0" * 64
    ok, got, claimed = capture.address_matches(OCTETS, wrong)
    assert not ok and claimed == "0" * 64 and got != claimed


def test_capture_refuses_a_body_longer_than_the_pinned_length():
    import io

    import capture

    assert capture.read_bounded(io.BytesIO(OCTETS)) == OCTETS
    with pytest.raises(SystemExit):
        capture.read_bounded(io.BytesIO(OCTETS + b"x"))


def test_capture_detects_divergence_from_the_pinned_copy():
    import capture

    mutated = OCTETS.replace(b'"gate_version":"0.4.1"', b'"gate_version":"0.4.2"', 1)
    assert mutated != OCTETS
    ok, _, _ = capture.address_matches(mutated, capture.MANIFEST["source_url"])
    assert not ok, "a diverged body must not satisfy the pinned address"


def test_mutation_report_carries_a_fired_blinded_control_probe():
    # Pins finding P1-4: the control must be able to go false, and the report must say it was
    # checked. Without this, control_preserved: true on every row proves nothing.
    report = json.loads((HERE / "runs" / "mutation.json").read_text())
    probe = report["blinded_control_probe"]
    assert probe["control_detected_as_blinded"] is True
    assert probe["control_verdict_under_probe"] != "recomputed"
    for rule, res in report["rules"].items():
        assert res["control_preserved"] is True, rule
