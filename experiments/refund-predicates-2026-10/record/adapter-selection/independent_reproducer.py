#!/usr/bin/env python3
"""Independent review recount only. Stdlib; no producer imports or runtime.
Accept the preserved retained-copy root (containing files/mount/run).
This freezes the raw receipt/equality logic used in the interactive review;
the parameterized file itself is not claimed executed before freeze.
"""
import collections
import hashlib
import json
import pathlib
import stat
import sys

CAP = 524288
STORES = (
    ("S1/e68117cf-23ba-43c2-8300-ac800e777673", 70),
    ("S1/926707ac-1d9c-4e25-922c-9e03e934e418", 140),
    ("S2/d94709c6-8cde-49dd-9a34-7d4f4191e62a", 28),
    ("S2/347db940-3fff-4f4f-8adc-60547b76584e", 56),
)
FINALS = (
    (STORES[1][0], "efec90bf2156ed58e0254c45f41d8c426a05ae35c9121080fd5a7eb241ff0453"),
    (STORES[3][0], "05ee915994b96f094862c9fadec4afdf8ad2519c50424ae947af3e4120f7f3db"),
)

def digest(b):
    return hashlib.sha256(b).hexdigest()

def read(p):
    s = p.lstat()
    assert stat.S_ISREG(s.st_mode) and s.st_size <= CAP, str(p)
    with p.open("rb") as f:
        b = f.read(CAP + 1)
    assert len(b) == s.st_size and len(b) <= CAP, str(p)
    return b

def pairs(items):
    d = {}
    for k, v in items:
        assert k not in d, k
        d[k] = v
    return d

def parse(b):
    def invalid(v):
        raise AssertionError("nonfinite JSON: " + v)
    return json.loads(b, object_pairs_hook=pairs, parse_constant=invalid)

def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=True, allow_nan=False)

def keyed(v):
    d = {}
    for k, x in v["__tuple_keyed__"]:
        k = tuple(k)
        assert k not in d
        d[k] = x
    return d

def classify(old, new):
    # Compare canonical values, preserving int/float/bool distinctions.
    # Derive classes from the design's claim/refusal transitions, not its table.
    if canon(old) == canon(new):
        return "U"
    if canon(old[:6]) == canon(new[:6]):
        return "U*"
    old_claim, new_claim = old[0] != "failed", new[0] != "failed"
    if old_claim and new_claim:
        assert canon(old[:2]) != canon(new[:2]), "unclassified claim-only change"
        return "SM"
    if not old_claim and new_claim:
        return "AI"
    if old_claim and not new_claim:
        assert new[2] in ("projector", "postcondition")
        return "GR-P" if new[2] == "projector" else "GR-A"
    assert canon(old[:4]) != canon(new[:4]), "unclassified refusal-only change"
    return "RO"

def main(root):
    base = root / "files/mount/run"
    result_bytes = read(base / "result.json")
    assert digest(result_bytes) == "12dbb151919c4c91d928c71bfc230bc2b4cf574b85f7d1aef5d9696b61dcd33d"
    result = parse(result_bytes)
    actual = {}
    # Fixed four store populations, no filesystem discovery or recursive scan.
    for relative, count in STORES:
        store = base / "ca-out" / relative
        for n in range(count):
            b = read(store / ("receipt-%06d.json" % n))
            r = parse(b)
            ident = r["invocation_id"]
            assert ident not in actual and r["returncode"] == 0 and r["abnormal"] is None
            h = r["stdout_sha256"].removeprefix("sha256:")
            assert len(h) == 64 and all(c in "0123456789abcdef" for c in h)
            raw = read(store / "blobs" / h)
            assert len(raw) == r["stdout_size"] and digest(raw) == h
            out = parse(raw)["outcome"]
            assert isinstance(out, list) and len(out) == 7
            actual[ident] = (out, r, digest(b))
    slots = []
    applications = []
    for relative, pin in FINALS:
        b = read(base / "ca-out" / relative / "final.json")
        assert digest(b) == pin
        final = parse(b)
        assert final["phase"] == "complete"
        plan = {row["step_id"]: row for row in final["schedule"]}
        assert len(plan) == len(final["schedule"])
        assert list(plan) == [step["step_id"] for step in final["steps"]]
        for step in final["steps"]:
            row = plan[step["step_id"]]
            if row["kind"] in ("ordinary", "control"):
                assert step["application"] == "applied"
                applications.append((row["label"], row["mutation_sha256"]))
            for slot in step["slots"]:
                assert slot["state"] == "observed"
                ref = slot["receipt"]
                out, receipt, rh = actual[ref["invocation_id"]]
                assert rh == ref["evidence_sha256"].removeprefix("sha256:")
                assert receipt["stdout_sha256"] == ref["raw_sha256"]
                assert receipt["stdout_size"] == ref["raw_size"]
                assert canon(slot["outcome"]) == canon([out])
                slots.append(ref["invocation_id"])
    assert len(actual) == len(slots) == len(set(slots)) == 294
    assert len(applications) == len(set(applications)) == 19
    assert len(result["records"]["t0"]) == 14

    def outcome(origin):
        out, receipt, rh = actual[origin["receipt"]["invocation_id"]]
        ref = origin["receipt"]
        assert receipt["stdout_sha256"] == ref["raw_sha256"]
        assert receipt["stdout_size"] == ref["raw_size"]
        assert rh == ref["evidence_sha256"].removeprefix("sha256:")
        return out

    baseline = {k: outcome(v) for k, v in result["origins"]["t0"].items()}
    matrices = {}
    for label, origin_name, expected_count in (("M", "s1", 140), ("C", "a0", 56)):
        origins = keyed(result["origins"][origin_name])
        computed = {}
        for key, origin in origins.items():
            role, vector = key
            if label == "C" and role not in ("C4", "C5", "C6", "C7"):
                continue
            computed[key] = classify(baseline[vector], outcome(origin))
        assert len(computed) == expected_count
        assert computed == keyed(result[label.lower() + "_matrix"]["classes"])
        matrices[label] = dict(sorted(collections.Counter(computed.values()).items()))

    # Four independently literal report identities and native statuses/reasons.
    expected = (
        ("V1", "not_established", "unsupported_definite_report", "LATE", "r", 3),
        ("V2", "established", "terminal_support", "LATE", "r2", 5),
        ("V3", "established", "terminal_support", "LATE-CONFLICT", "r", 4),
        ("V4", "not_established", "contrary_terminal_evidence", "LATE-CONFLICT", "r2", 7),
    )
    reports = {}
    for vector, status, reason, case, report, seq in expected:
        out = baseline[vector]
        assert out[0] == status and out[1].split(":")[-1] == reason
        selected = {"case_id": case, "evidence_operation_id": "op", "report_id": report, "report_seq": seq}
        assert canon(out[4]) == canon(selected)
        declared = result["four_reports"][vector]
        assert declared["selected_closed"] is True
        assert canon(declared["selected_expected"]) == canon(selected)
        assert canon(declared["selected_observed"]) == canon(selected)
        assert declared["commitment_expected"] == declared["commitment_observed"] == out[5]
        reports[vector] = {"status": status, "selected": selected, "commitment": out[5]}
    assert len({v["commitment"] for v in reports.values()}) == 4
    assert baseline["S1"][:4] == ["failed", "evidence:selector_missing", "front", "evidence:selector_missing"]
    assert baseline["S3"][:4] == ["failed", "selection:subject_not_found", "projector", "selection:subject_not_found"]
    print(json.dumps({"invocations": len(actual), "unique_applications": len(applications), "matrix_cells": 196, "classes": matrices, "four_reports": reports}, sort_keys=True))

if __name__ == "__main__":
    assert len(sys.argv) == 2, "provide retained-copy root"
    main(pathlib.Path(sys.argv[1]))
