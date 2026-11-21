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


# The four tests below exist because the previous round tested helpers while the production funnels
# went unguarded: reverting each fix left the whole suite green. Each one now reverts with the suite.


class _Resp:
    """Minimal stand-in for the urlopen context manager, so refetch() itself is exercised."""

    def __init__(self, payload: bytes):
        self._payload = payload

    def read(self, n: int = -1) -> bytes:
        return self._payload if n < 0 else self._payload[:n]

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def test_refetch_enforces_the_address_binding_and_the_bound(monkeypatch):
    # Guards capture.refetch(), not its helpers. Replacing the bounded read with r.read() or
    # dropping the binding refusal must fail here.
    import capture

    monkeypatch.setattr(capture.urllib.request, "urlopen", lambda *a, **k: _Resp(b'{"not":"the record"}'))
    assert capture.refetch() == 1, "bytes that are not the named address must be refused"

    monkeypatch.setattr(capture.urllib.request, "urlopen", lambda *a, **k: _Resp(OCTETS + b"x"))
    with pytest.raises(SystemExit):
        capture.refetch()

    monkeypatch.setattr(capture.urllib.request, "urlopen", lambda *a, **k: _Resp(OCTETS))
    assert capture.refetch() == 0, "the real bytes at the real address must still pass"


def _isolated_copy(tmp_path):
    import shutil

    work = tmp_path / "exp"
    shutil.copytree(HERE, work, ignore=shutil.ignore_patterns("__pycache__", ".pytest_cache"))
    return work


def test_a_case_missing_from_the_run_record_fails_the_reproducer(tmp_path):
    # Guards the set-equality check in independent_resolve. Disabling it must fail here rather
    # than leaving a clean-looking n-of-n over a smaller set.
    work = _isolated_copy(tmp_path)
    record = work / "runs" / "resolve-run.json"
    doc = json.loads(record.read_text())
    doc["cases"] = doc["cases"][:-1]
    record.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    r = subprocess.run([sys.executable, "independent_resolve.py"], cwd=work, capture_output=True, text=True)
    assert r.returncode == 1, r.stdout
    assert "case sets differ" in r.stdout


def test_a_duplicated_case_fails_the_reproducer(tmp_path):
    work = _isolated_copy(tmp_path)
    record = work / "runs" / "resolve-run.json"
    doc = json.loads(record.read_text())
    doc["cases"].append(doc["cases"][0])
    record.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    r = subprocess.run([sys.executable, "independent_resolve.py"], cwd=work, capture_output=True, text=True)
    assert r.returncode == 1, r.stdout
    assert "duplicate case ids" in r.stdout


def test_the_runner_refuses_a_consumer_that_is_not_the_pinned_blob(tmp_path):
    # Guards resolve.load_published_consumer, which is what actually loads the helper. Retargeting
    # the runner at an unpinned copy must fail here, not pass because a test hashed another path.
    import shutil

    fake = tmp_path / "consumer"
    fake.mkdir()
    src = ref.PUBLISHED / "evidenceref_consumer.py"
    shutil.copy(src, fake / "evidenceref_consumer.py")
    assert ref.git_blob_sha1((fake / "evidenceref_consumer.py").read_bytes()) == ref.PUBLISHED_BLOB
    # Same length, different bytes: an appended line would trip the size ceiling instead, and this
    # test would stop exercising the blob comparison it exists for.
    original = (fake / "evidenceref_consumer.py").read_bytes()
    drifted = original.replace(b"# ", b"#_", 1)
    assert len(drifted) == len(original) and drifted != original
    (fake / "evidenceref_consumer.py").write_bytes(drifted)
    with pytest.raises(SystemExit) as excinfo:
        ref.load_published_consumer(fake)
    assert "unpinned consumer" in str(excinfo.value)


def test_a_kill_requires_the_control_and_a_control_that_can_fail():
    # Guards the single verdict function. Reintroducing a per-rule exemption, or leaving the probe
    # unconsumed, must fail here.
    import mutate

    assert mutate.kill_verdict(True, True, True) is True
    assert mutate.kill_verdict(True, True, False) is False, "a control that cannot fail scores nothing"
    assert mutate.kill_verdict(True, False, True) is False, "a blinded control scores nothing"
    assert mutate.kill_verdict(False, True, True) is False, "an unmoved mutant is not a kill"

    report = json.loads((HERE / "runs" / "mutation.json").read_text())
    fired = report["blinded_control_probe"]["control_detected_as_blinded"]
    for rule, res in report["rules"].items():
        assert res["valid_kill"] == mutate.kill_verdict(
            bool(res["killed_by"]), res["control_preserved"], fired
        ), rule


def test_control_survival_cannot_depend_on_which_rule_was_mutated():
    # The previous round's assertion compared the report with the verdict function and passed
    # either way, because restoring the exemption made control_preserved spuriously true and the
    # two stayed consistent. This pins the property instead: rule identity is not an input to the
    # control, and no rule-conditional exists in the scoring loop.
    import inspect

    import mutate

    assert list(inspect.signature(mutate.control_survived).parameters) == ["observations"]
    assert mutate.control_survived({"C_octets_profile_named": "recomputed"}) is True
    assert mutate.control_survived({"C_octets_profile_named": "digest_mismatch"}) is False
    assert mutate.control_survived({}) is False

    scoring = inspect.getsource(mutate.main)
    assert "control_survived(got)" in scoring, "the loop must score through the pure function"
    assert "rule ==" not in scoring, "a per-rule exemption reappeared in the scoring loop"


def test_refetch_divergence_names_which_copy_carries_the_address(monkeypatch, capsys):
    # A divergence report that does not say which copy is address-bound leaves the reader to
    # deduce it. The fetched copy is bound by the check above, so the pinned one is the odd one out.
    import capture

    other = json.dumps({"a": 1}, separators=(",", ":")).encode()
    digest = hashlib.sha256(other).hexdigest()
    url = "https://gate.horizonshield.dev/record/" + digest
    monkeypatch.setitem(capture.MANIFEST, "source_url", url)
    monkeypatch.setattr(capture.urllib.request, "urlopen", lambda *a, **k: _Resp(other))

    assert capture.refetch() == 1, "bytes that differ from the pinned copy are a divergence"
    out = capsys.readouterr().out
    assert "DIVERGENCE" in out
    assert "fetched copy   : address-bound" in out
    assert "pinned copy    : address-bound=False" in out


def test_a_preloaded_module_of_that_name_cannot_be_consumed_instead(monkeypatch):
    # The verified-bytes / executed-bytes gap. The blob check passed and the runner still received
    # a module pre-loaded under the imported name, whose `_is_redacted` returned False: the file was
    # verified, a different object was executed. Loading by path closes it; this fails if the loader
    # ever goes back to importing by name.
    import sys
    import types

    poisoned = types.ModuleType("evidenceref_consumer")
    poisoned._is_redacted = lambda value: False
    poisoned.MARKER = "POISONED"
    monkeypatch.setitem(sys.modules, "evidenceref_consumer", poisoned)

    module = ref.load_published_consumer()
    assert module is not poisoned, "the runner consumed a pre-loaded module, not the verified bytes"
    assert not hasattr(module, "MARKER")
    assert module._is_redacted({"_redacted": True}) is True

    # And the loaded object is the file that was checked, not merely some other file.
    assert ref.git_blob_sha1(pathlib.Path(module.__file__).read_bytes()) == ref.PUBLISHED_BLOB


def test_the_consumer_is_read_once_so_it_cannot_be_swapped_mid_load(monkeypatch, tmp_path):
    # The read/execute swap: hash the file, have it replaced, execute the replacement. Reading once
    # closes it, because there is one byte object and it is the one that was hashed. This asserts
    # the property directly rather than the absence of a symptom.
    import shutil

    staged = tmp_path / "consumer"
    staged.mkdir()
    shutil.copy(ref.PUBLISHED / "evidenceref_consumer.py", staged / "evidenceref_consumer.py")
    pinned_bytes = (staged / "evidenceref_consumer.py").read_bytes()

    opens = {"n": 0}
    real_open = pathlib.Path.open

    class _SwappingFile:
        """First open yields the pinned bytes; any later one yields a subverted file."""

        def __init__(self, nth):
            payload = pinned_bytes if nth == 1 else pinned_bytes.replace(
                b"def _is_redacted", b"def _unused_redacted", 1
            )
            self._payload = payload

        def read(self, n=-1):
            return self._payload if n < 0 else self._payload[:n]

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

    def counting_open(self, *a, **k):
        opens["n"] += 1
        return _SwappingFile(opens["n"])

    monkeypatch.setattr(pathlib.Path, "open", counting_open)
    module = ref.load_published_consumer(staged)

    assert opens["n"] == 1, "the consumer was opened more than once; a swap fits between the reads"
    assert module._is_redacted({"_redacted": True}) is True, "the executed bytes are not the verified ones"


def test_an_oversize_candidate_consumer_is_refused_before_it_is_materialized(tmp_path):
    # The digest refuses a wrong consumer, but only after reading all of it: a controlled 8 MiB
    # blob was fully materialized before the check fired, against a pinned file of 27,787 bytes.
    # A ceiling belongs before the hash, not after it, exactly as capture.read_bounded does on
    # the wire. Refusal alone is not the property; refusing without materialising is.
    staged = tmp_path / "consumer"
    staged.mkdir()
    (staged / "evidenceref_consumer.py").write_bytes(b"#" * (8 * 1024 * 1024))

    read = {"bytes": 0}
    real_read = pathlib.Path.read_bytes
    real_open = pathlib.Path.open

    def counting_read(self):
        data = real_read(self)
        read["bytes"] += len(data)
        return data

    class _CountingFile:
        def __init__(self, fh):
            self._fh = fh

        def read(self, n=-1):
            data = self._fh.read(n)
            read["bytes"] += len(data)
            return data

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            self._fh.close()
            return False

    def counting_open(self, *a, **k):
        return _CountingFile(real_open(self, *a, **k))

    pathlib.Path.read_bytes = counting_read
    pathlib.Path.open = counting_open
    try:
        with pytest.raises(SystemExit) as excinfo:
            ref.load_published_consumer(staged)
    finally:
        pathlib.Path.read_bytes = real_read
        pathlib.Path.open = real_open

    # The bounded read alone would truncate and then fail the digest, refusing for the wrong
    # reason. The explicit ceiling refuses before hashing and says why; without it this reads
    # "unpinned consumer" and the size fault is reported as a content fault.
    #
    # Matched on the prefix, not with `in`. The message carries the candidate path, pytest derives
    # tmp_path from the test's own name, and this test is named ...oversize..., so a substring
    # check was true no matter what the loader did. It passed with the ceiling deleted.
    assert str(excinfo.value).startswith("refusing an oversize consumer"), str(excinfo.value)

    assert read["bytes"] <= ref.PUBLISHED_LEN + 1, (
        f"materialised {read['bytes']} bytes for a candidate that cannot be the pinned "
        f"{ref.PUBLISHED_LEN}-byte consumer"
    )
