"""A limited evidence check on this package. Not a v0.1 validator.

    python3 check.py            # exit 0 only if every check below holds
    python3 check.py --controls # also run must-fail variants; each must be
                                # rejected for its own stated reason

Standard library only; reads only files in this directory and the experiment
record in its parent directory.

What it checks.
  Evidence: the report's evidence_set names the record files it cites in the
  parent directory, with their digests and their count (3.1); each file must
  exist and match, and match the parent record's own SHA256SUMS.txt. Every
  path the report or the proposal names must exist, and every digest stated
  beside a path must be that of a listed member.
  Report (records.v0.1.json): each record's state equals the baseline log;
  rows 1, 6 and 7 over the records; every qualifier is unknown (this
  package's conservative rule, not a v0.1 row); the 5.3 counter recounts from
  the records; each 5.2 claim carries its population (the section 4 run-level
  rejection); the aggregate recounts; 5.4 answers explicitly (nothing, while
  the checker boundary is undecided); the checker boundary claims no reading.
  Rows 8 to 13 have nothing to read in the report, because no qualifier is
  asserted; they are applied to the proposal instead.
  Proposal (mapping-proposal.json): `changed` is in section 2's closed
  vocabulary; `moved` is resolved from the two logs, not taken from the
  declaration; the delta touches no test file; rows 8, 9, 11, 12 and 13 and
  row 14 (form) over the proposed qualifiers and evidence object; a qualifier
  citing an evidence id that is not in the package is reported as an
  unresolved reference (section 3), not as row 11; the 5.5-shaped fixture
  expects fail and names a declared check.

What it does not establish: which reading of the checker boundary is right,
and therefore whether the proposal is a valid v0.1 claim at all. It does not
rebuild or rerun anything; ../recompute.sh does that, and this script only
reads the observations it compares against.
Its must-fail variants show that these checks reject these mutations for the
stated reasons, not that the mapping is correct.
"""
from __future__ import annotations

import copy
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REC = HERE.parent
VERDICT = {"pass", "fail"}
NON_VERDICT = {"not-exercised", "inconclusive", "void"}
CHANGED = {"input artifact", "checker rule", "constraint"}
SHAPES = {"flat", "rfc6962", "RFC9162_SHA256"}  # the v0.1 tree-shape set named in 3.1
PATH = re.compile(r"\.\./[A-Za-z0-9_./-]*[A-Za-z0-9_]")


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def states(rel: str) -> dict:
    """Per-test outcome from a cargo test log: ok -> pass, FAILED -> fail."""
    out = {}
    p = HERE / rel
    if not p.is_file():
        return out  # reported by ref_problems as a missing path
    for m in re.finditer(r"^test (\S+) \.\.\. (ok|FAILED)$", p.read_text(), re.M):
        out[m.group(1)] = "pass" if m.group(2) == "ok" else "fail"
    return out


def evidence_problems(report: dict, sums_text: str | None = None) -> tuple[list[str], dict]:
    errs, listed = [], {}
    tree = report["evidence_set"]["tree"]
    if tree["shape"] not in SHAPES:
        errs.append("3.1: tree shape is not a declared identifier")
    if tree["entries"] != len(tree["members"]):
        errs.append("3.1: declared entry count does not match the members")
    text = sums_text if sums_text is not None else (REC / "SHA256SUMS.txt").read_text()
    record_sums = {name.removeprefix("./"): d for d, name in (l.split("  ", 1) for l in text.splitlines() if l)}
    for m in tree["members"]:
        ref, digest = m["ref"], m["sha256"]
        listed[ref] = digest
        p = HERE / ref
        if not p.is_file() or sha(p) != digest:
            errs.append(f"evidence: {ref} does not match its stated digest")
        if record_sums.get(ref.removeprefix("../")) != digest:
            errs.append(f"evidence: {ref} differs from the record's SHA256SUMS.txt")
    return errs, listed


def refs(obj):
    """Every relative path named anywhere, with the digest stated beside it, if any."""
    if isinstance(obj, dict):
        if isinstance(obj.get("ref"), str):
            yield obj["ref"], obj.get("sha256")
        for k, v in obj.items():
            if k == "ref":
                continue
            yield from refs(v)
    elif isinstance(obj, list):
        for v in obj:
            yield from refs(v)
    elif isinstance(obj, str):
        for m in PATH.finditer(obj):
            yield m.group(0), None


def ref_problems(doc, listed, label):
    errs = []
    for ref, digest in refs(doc):
        if not ref.startswith("../"):
            continue
        if not (HERE / ref).is_file():
            errs.append(f"{label}: ref {ref} does not exist")
        elif digest is not None and ref not in listed:
            errs.append(f"{label}: ref {ref} is not in the evidence set")
        elif digest is not None and digest != listed[ref]:
            errs.append(f"{label}: ref {ref} digest does not match")
    return errs


def report_problems(doc: dict) -> list[str]:
    errs = []
    declared = doc["run"]["declared_checks"]
    base = states(doc["run"]["observation"]["ref"])
    recs = doc["records"]
    if [r["check_id"] for r in recs] != declared:
        errs.append("report: records are not exactly the declared checks")
    for r in recs:
        cid, st = r["check_id"], r["state"]
        ov, di = r.get("other_verdict", "unknown"), r.get("discrimination", "unknown")
        if base.get(cid) != st:
            errs.append(f"report: {cid} state {st} differs from the baseline log ({base.get(cid)})")
        if st in NON_VERDICT and "cause" not in r:
            errs.append(f"row 1: {cid}")
        if st in VERDICT and "cause" in r:
            errs.append(f"row 7: {cid}")
        if st in NON_VERDICT and (ov != "unknown" or di != "unknown"):
            errs.append(f"row 6: {cid}")
        if ov != "unknown" or di != "unknown":
            errs.append(f"package rule: {cid} asserts a qualifier while the checker boundary is undecided")

    ru = doc["rollup"]
    # 5.3: recount carried against referenced from the records themselves.
    cited = [q for r in recs for q in (r.get("other_verdict"), r.get("discrimination")) if isinstance(q, dict)]
    carried = sum(1 for q in cited if "carry" in q)
    referenced = len(cited) - carried
    c = ru["carried_vs_referenced"]
    if (c.get("carried"), c.get("referenced")) != (carried, referenced):
        errs.append(f"5.3: counter {c.get('carried')}/{c.get('referenced')} does not recount ({carried}/{referenced})")
    # Section 4 run-level rejection: a completeness claim without its population.
    for name, claim in ru["completeness"].items():
        if not isinstance(claim.get("population"), int):
            errs.append(f"run-level: completeness claim {name} has no population")
    agg = ru["aggregate"]
    if (agg["count"], agg["pass"], agg["fail"]) != (len(recs), sum(r["state"] == "pass" for r in recs),
                                                    sum(r["state"] == "fail" for r in recs)):
        errs.append("5.1: aggregate does not recount from the records")
    dp = ru["discriminating_power"]
    if dp.get("run_produced_fail") != any(r["state"] == "fail" for r in recs):
        errs.append("5.4: run_produced_fail does not match the records")
    if not dp.get("answer"):
        errs.append("5.4: silence in the discriminating-power field")
    cb = doc["checker_boundary"]
    if cb.get("status") != "undecided" or cb.get("claimed") != "none":
        errs.append("boundary: a reading of the checker boundary is claimed")
    if cb.get("status") == "undecided" and dp.get("answer") not in (None, "nothing"):
        errs.append("5.4: a control is named while the checker boundary is undecided")
    return errs


def proposal_problems(prop: dict, report: dict) -> list[str]:
    errs = []
    evs = {e["id"]: e for e in prop["evidence_objects"]}
    declared = set(report["run"]["declared_checks"])
    for e in evs.values():
        if e["changed"] not in CHANGED:
            errs.append(f"section 2: {e['id']} changed {e['changed']!r} is outside the closed vocabulary")
        if len(e["observations"]) != 2 or not all(o.get("sha256") for o in e["observations"]):
            errs.append(f"row 14: {e['id']} lacks two digest-bound observations")
            continue
        a, b = (states(o["ref"]) for o in e["observations"])
        if any(c not in a or c not in b for c in e["compared"]):
            errs.append(f"{e['id']}: a compared check is missing from an observation")
        resolved = sorted(c for c in e["compared"] if a.get(c) != b.get(c))
        if sorted(e["moved"]) != resolved:
            errs.append(f"moved: {e['id']} declares {e['moved']}, the logs give {resolved}")
        d = e["delta"]
        patch = d["carry_utf8"] if "carry_utf8" in d else (
            (HERE / d["ref"]).read_text() if (HERE / d.get("ref", "")).is_file() else "")
        if patch:
            if any("/tests/" in t for t in re.findall(r"^diff --git a/(\S+)", patch, re.M)):
                errs.append(f"{e['id']}: delta touches the tests, which reading A takes as the checker")

    q = prop["proposed_qualifiers"]
    ov, di, cid = q["other_verdict"], q["discrimination"], q["check_id"]
    if isinstance(di, dict) and ov in ("unknown", "possible-not-demonstrated"):
        errs.append(f"row 9: {cid}")
    if isinstance(ov, dict) and "foreclosed" in ov and isinstance(di, dict):
        errs.append(f"row 8: {cid}")
    for v in (ov, di):
        if isinstance(v, dict):
            target = next(iter(v.values()), None)
            if not target:
                errs.append(f"row 11: {cid} asserts a value without its evidence reference")
            elif target not in evs:
                errs.append(f"section 3: {cid} cites {target}, which does not resolve in this package (unchecked)")
    if isinstance(di, dict) and di.get("demonstrated") in evs and evs[di["demonstrated"]]["changed"] == "checker rule":
        errs.append(f"row 12: {cid} cites discrimination evidence whose changed slot is the checker")
    if isinstance(ov, dict) and "demonstrated" in ov and ov["demonstrated"] in evs \
            and cid not in evs[ov["demonstrated"]]["moved"]:
        errs.append(f"row 13: {cid}")

    for fx in prop["proposed_control_5_5_shape"]:
        if fx["expected"]["state"] != "fail" or fx["observed"]["state"] != "fail":
            errs.append(f"5.5: {fx['fixture_id']} does not expect and observe fail")
        if fx["target_check"]["check_id"] not in declared:
            errs.append(f"5.4: {fx['fixture_id']} is not one of the run's declared checks")
        if fx["expected"]["rule"] != fx["target_check"]["check_id"]:
            errs.append(f"5.5: {fx['fixture_id']} expected rule is not the target check")
    return errs


def all_problems(report, prop, sums_text=None):
    ev_errs, listed = evidence_problems(report, sums_text)
    return (ev_errs + ref_problems(report, listed, "report")
            + ref_problems(prop, listed, "proposal") + report_problems(report) + proposal_problems(prop, report))


# Each control names the reason it must be rejected for (section 4: a rejection
# vector asserts on the reason, not on the fact of rejection).
CONTROLS = [
    ("report: qualifier asserted while the boundary is undecided", "package rule",
     lambda r, p: r["records"][3].update(other_verdict={"demonstrated": "ev-1"}, discrimination={"demonstrated": "ev-1"})),
    ("report: 5.3 counter overstated", "5.3:",
     lambda r, p: r["rollup"]["carried_vs_referenced"].update(referenced=1)),
    ("report: completeness claim without its population", "run-level:",
     lambda r, p: r["rollup"]["completeness"]["evidence"].pop("population")),
    ("report: 5.4 left silent", "silence",
     lambda r, p: r["rollup"]["discriminating_power"].pop("answer")),
    ("report: control named while the boundary is undecided", "a control is named",
     lambda r, p: r["rollup"]["discriminating_power"].update(answer="control built to fail")),
    ("report: a reading of the boundary claimed", "boundary:",
     lambda r, p: r["checker_boundary"].update(claimed="A")),
    ("report: state differs from the log", "differs from the baseline log",
     lambda r, p: r["records"][0].update(state="fail")),
    ("proposal: changed outside the closed vocabulary", "closed vocabulary",
     lambda r, p: p["evidence_objects"][0].update(changed="handler implementation")),
    ("proposal: evidence that changes the checker (row 12)", "row 12:",
     lambda r, p: p["evidence_objects"][0].update(changed="checker rule")),
    ("proposal: declared moved disagrees with the logs", "moved:",
     lambda r, p: p["evidence_objects"][0].update(moved=[])),
    ("proposal: other-verdict demonstrated on a check that did not move (row 13)", "row 13:",
     lambda r, p: p["proposed_qualifiers"].update(check_id=r["run"]["declared_checks"][0])),
    ("proposal: discrimination demonstrated with other-verdict unknown (row 9)", "row 9:",
     lambda r, p: p["proposed_qualifiers"].update(other_verdict="unknown")),
    ("proposal: qualifier asserted without its evidence reference (row 11)", "row 11:",
     lambda r, p: p["proposed_qualifiers"].update(discrimination={"demonstrated": ""})),
    ("proposal: qualifier citing an evidence id not in the package (section 3)", "does not resolve",
     lambda r, p: p["proposed_qualifiers"].update(discrimination={"demonstrated": "ev-9"})),
    ("proposal: foreclosed other-verdict beside demonstrated discrimination (row 8)", "row 8:",
     lambda r, p: p["proposed_qualifiers"].update(other_verdict={"foreclosed": "c1"})),
    ("proposal: evidence object with one observation (row 14)", "row 14:",
     lambda r, p: p["evidence_objects"][0].update(observations=p["evidence_objects"][0]["observations"][:1])),
    ("proposal: delta that touches the test source", "delta touches the tests",
     lambda r, p: p["evidence_objects"][0].update(delta={"carry_utf8":
         "diff --git a/crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs b/x\n"})),
    ("report: non-verdict state without a cause (row 1)", "row 1:",
     lambda r, p: r["records"][0].update(state="not-exercised")),
    ("report: non-verdict state carrying a qualifier (row 6)", "row 6:",
     lambda r, p: r["records"][0].update(state="void", cause="evidence-does-not-hold", discrimination={"demonstrated": "ev-1"})),
    ("report: verdict state carrying a cause (row 7)", "row 7:",
     lambda r, p: r["records"][0].update(cause="unavailable")),
    ("report: stated digest that does not match", "digest does not match",
     lambda r, p: r["run"]["observation"].update(sha256="0" * 64)),
    ("report: undeclared tree shape (3.1)", "tree shape",
     lambda r, p: r["evidence_set"]["tree"].update(shape="not-a-declared-shape")),
    ("report: entry count not bound to the members (3.1)", "entry count",
     lambda r, p: r["evidence_set"]["tree"].update(entries=4)),
    ("report: member digest that does not match its file", "does not match its stated digest",
     lambda r, p: r["evidence_set"]["tree"]["members"][0].update(sha256="0" * 64)),
    ("report: path that does not exist", "does not exist",
     lambda r, p: r["run"]["observation"].update(ref="../observations/other.txt", sha256=None)),
    ("proposal: 5.5 fixture that expects pass", "5.5:",
     lambda r, p: p["proposed_control_5_5_shape"][0]["expected"].update(state="pass")),
    ("proposal: ref to a file outside the evidence set", "not in the evidence set",
     lambda r, p: p["evidence_objects"][0]["delta"].update(ref="../recompute.sh")),
]

# Controls on the parent record's checksum file: (name, reason, alteration).
SUMS_CONTROLS = [
    ("evidence: a member that differs from the record's SHA256SUMS.txt", "differs from the record",
     lambda t: t.replace(t.split()[0], "0" * 64, 1)),
]


def load():
    return (json.loads((HERE / "records.v0.1.json").read_text()),
            json.loads((HERE / "mapping-proposal.json").read_text()))


def main(argv) -> int:
    report, prop = load()
    errs = all_problems(report, prop)
    for e in errs:
        print("FAIL", e)
    print("limited checks:", "ok (not a v0.1 validator; the checker boundary is not decided)" if not errs
          else f"{len(errs)} problem(s)")
    ok = not errs
    if "--controls" in argv:
        for name, reason, mutate in CONTROLS:
            r, p = copy.deepcopy(report), copy.deepcopy(prop)
            mutate(r, p)
            found = [e for e in all_problems(r, p) if reason in e]
            print("control", "rejected for its reason" if found else "NOT REJECTED FOR ITS REASON", "-", name)
            ok &= bool(found)
        text = (REC / "SHA256SUMS.txt").read_text()
        for name, reason, alter in SUMS_CONTROLS:
            found = [e for e in all_problems(report, prop, alter(text)) if reason in e]
            print("control", "rejected for its reason" if found else "NOT REJECTED FOR ITS REASON", "-", name)
            ok &= bool(found)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
