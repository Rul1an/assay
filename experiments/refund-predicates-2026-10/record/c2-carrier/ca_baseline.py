#!/usr/bin/env python3
"""Baseline-only Corpus Adequacy carrier for the C2 reader (Task 3; CARRIER-PLAN.md).

Route, public CA API only, at the pinned export: observe_prefix(stop_before='control') runs the 20 baseline
vectors and stops before the declared positive control; close_observation then closes that stopped prefix with no
admission, context or decision. No resume, recovery, control preflight, vector selection or ledger. The positive
control and the ordinary mutant are declared (CA's schedule needs both) and are never applied.

Patterns adapted, with attribution, from refund-c3-mutation-2026-10/carrier.py (supplied-root checks, pinned
export, bytecode-free engine load, blob reader, budget hook); nothing is imported from C3.

Every write is counted (budget.py): 20 dispatches are reserved before observe_prefix, the receipt hook re-checks
before each next dispatch, and close reserves the interval plus the measured prefix root. Refusals before the
first dispatch, an unclosed prefix and an unresolvable close are reported as such and never as results. Not a
sandbox: trusted-local runs an owned copy of the pinned subject.
"""
import argparse
import hashlib
import importlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

import budget
import consumer

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
POLICY = b"refund.c2-baseline: baseline-only stop before control; no admission, ledger or resume; no score"
ENGINE_MODULES = ("observation_session", "execution_observation", "corpus_adequacy", "bounded_run",
                  "contained_contract", "isolated_tree", "module_child")
MAX_ARTIFACT = 8 * 1024 * 1024
FINAL_MARGIN = 64 * 1024  # provenance growth between its draft and final form (decisions, free samples)
SUBJECT_FILES = ("reader.py", "ca-manifest.json", "ca-vectors.json")
SELECTORS = list(consumer.SELECTORS)
POSITIVE = {"anchor": 'result, reason = "ESTABLISHED", "at_most_one_committed"',
            "control": True, "control_polarity": "positive",
            "label": "control-positive-established-to-contradicted",
            "replacement": 'result, reason = "CONTRADICTED", "at_most_one_committed"', "scope": "declared"}
ORDINARY = {"anchor": "if len(committed) >= 2:", "label": "ordinary-committed-threshold-three",
            "replacement": "if len(committed) >= 3:", "scope": "declared"}
FROZEN_MANIFEST = {
    "accepted_exit_codes": [0], "build": [], "default_group": "c2",
    "entrypoint_command": ["python3", "reader.py", "{vector}"], "id_key": "id",
    "implementation_sources": ["reader.py"], "mutants": {"c2": [POSITIVE, ORDINARY]},
    "outcome_from": SELECTORS, "repo_root": ".", "runner": "process", "schema": "corpus-adequacy.manifest.v0",
    "unproved_exit_codes": [2], "vector_path_key": "path", "vector_timeout": 10, "vectors": "ca-vectors.json",
}


class CarrierRefused(Exception):
    """The carrier cannot start safely; nothing has been dispatched."""


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pins():
    return json.loads((HERE / "ca-source-pin.json").read_bytes())


def git_show(repo, commit, path):
    try:
        return subprocess.check_output(["git", "-C", str(repo), "show", f"{commit}:{path}"],
                                       stderr=subprocess.PIPE, timeout=30)
    except (subprocess.SubprocessError, OSError) as exc:
        raise CarrierRefused(f"git object unavailable: {commit}:{path}") from exc


def source_digest(reader_raw):
    """CA's observation source digest (observation_session.source_digest) for implementation_sources=[reader.py]."""
    digest = hashlib.sha256(b"corpus-adequacy.observation-sources.v0\n")
    name = b"reader.py"
    digest.update(str(len(name)).encode("ascii") + b"\n" + name)
    digest.update(str(len(reader_raw)).encode("ascii") + b"\n" + reader_raw)
    return "sha256:" + digest.hexdigest()


# ------------------------------------------------------------------ frozen route

def check_route(manifest):
    """The manifest must be exactly the frozen one-group, empty-build, POS+ordinary baseline-only route."""
    if manifest != FROZEN_MANIFEST:
        raise CarrierRefused("manifest differs from the frozen baseline-only route")
    return manifest


def check_anchors(reader_text, manifest):
    for declaration in manifest["mutants"]["c2"]:
        if reader_text.count(declaration["anchor"]) != 1:
            raise CarrierRefused("declared anchor does not occur exactly once: " + declaration["label"])


def check_vectors(vectors, case_ids):
    expected = [{"id": cid, "path": f"fixtures/{cid}.json"} for cid in sorted(case_ids)]
    if vectors != expected:
        raise CarrierRefused("vectors differ from the sorted frozen case set")
    if len(vectors) > budget.MAX_CALLS:
        raise CarrierRefused("more vectors than reserved dispatches")


def _regular(path):
    st = os.lstat(path)
    if not stat.S_ISREG(st.st_mode) or st.st_size > MAX_ARTIFACT:
        raise CarrierRefused("subject file is not a bounded regular file: " + str(path))
    return Path(path).read_bytes()


def load_subject(subject_root, subject_pin, case_ids):
    """Read, pin-check and route-check the subject bytes; nothing is written."""
    subject_root = Path(subject_root)
    files = {rel: _regular(subject_root / rel) for rel in SUBJECT_FILES}
    try:
        manifest = json.loads(files["ca-manifest.json"])
        vectors = json.loads(files["ca-vectors.json"])
    except ValueError as exc:
        raise CarrierRefused("subject manifest or vectors are not JSON") from exc
    if type(vectors) is not list or any(type(v) is not dict or type(v.get("path")) is not str for v in vectors):
        raise CarrierRefused("vectors index shape")
    for v in vectors:
        files[v["path"]] = _regular(subject_root / v["path"])
    if set(subject_pin) != set(files):
        raise CarrierRefused("subject pin covers a different file set")
    for rel, raw in files.items():
        if sha(raw) != subject_pin[rel]:
            raise CarrierRefused("subject bytes differ from pin: " + rel)
    check_route(manifest)
    check_anchors(files["reader.py"].decode("utf-8"), manifest)
    check_vectors(vectors, case_ids)
    return files


def production_frozen():
    pin = pins()
    oracle_raw = (HERE / "expectations.json").read_bytes()
    if sha(oracle_raw) != pin["oracle"]["expectations.json"]:
        raise CarrierRefused("oracle bytes differ from pin")
    oracle = json.loads(oracle_raw)
    fixtures = {cid: (HERE / "fixtures" / f"{cid}.json").read_bytes() for cid in oracle}
    return consumer.Frozen(oracle=oracle, fixtures=fixtures)


# ------------------------------------------------------------------ preparation

class Context:
    def __init__(self, **kw):
        self.__dict__.update(kw)


def _check_supplied_root(run_root):
    """Inspect the supplied entry as given, before any resolve or mkdir (C3 R1 pattern).

    os.path.lexists also sees a dangling symlink; platform parent aliases (macOS /tmp) are not rejected.
    """
    run_root = Path(run_root)
    if os.path.lexists(run_root):
        kind = "symbolic link" if os.path.islink(run_root) else "existing path"
        raise CarrierRefused(f"run root is an {kind}; a fresh path is required: {run_root}")
    resolved = run_root.resolve()
    if resolved == REPO.resolve() or REPO.resolve() in resolved.parents:
        raise CarrierRefused("run root must be outside the repository: " + str(resolved))
    return resolved


def _child_python():
    path = shutil.which("python3")
    if path is None:
        raise CarrierRefused("child python3 unavailable")
    version = subprocess.check_output([path, "-c", "import sys; print('.'.join(map(str, sys.version_info[:3])))"],
                                      timeout=10).decode().strip()
    if tuple(map(int, version.split("."))) < (3, 11):
        raise CarrierRefused("child Python below 3.11")
    return {"path": str(Path(path).absolute()), "version": version,
            "identity_claim": "path-and-version-observation-only"}


def _load_engine(export_dir, cap):
    if any(name in sys.modules for name in ENGINE_MODULES):
        raise CarrierRefused("CA runtime already loaded; use a fresh process")
    sys.dont_write_bytecode = True  # no uncounted cache files in the export store
    sys.path.insert(0, str(export_dir))
    modules = {name: importlib.import_module(name) for name in ENGINE_MODULES}
    for name, module in modules.items():
        if Path(module.__file__).resolve().parent != export_dir.resolve():
            raise CarrierRefused("CA module not loaded from verified export: " + name)
    if modules["contained_contract"].OUTPUT_CAP_BYTES != cap:
        raise CarrierRefused("CA output cap differs from pin")
    return modules["observation_session"], modules["execution_observation"]


def prepare(ca_repo, run_root, *, subject_root=None, subject_pin=None, frozen=None, budget_kwargs=None):
    """All checks before any write; then the counted export, subject copy and engine load."""
    run_root = _check_supplied_root(run_root)
    pin = pins()
    frozen = frozen if frozen is not None else production_frozen()
    subject_root = Path(subject_root) if subject_root is not None else HERE
    subject_pin = subject_pin if subject_pin is not None else pin["subject"]["files"]
    files = load_subject(subject_root, subject_pin, frozen.oracle)
    instrument = pin["instrument"]
    export = {}
    for name, digest in sorted(instrument["files"].items()):
        raw = git_show(ca_repo, instrument["commit"], name)
        if sha(raw) != digest:
            raise CarrierRefused("pinned CA digest mismatch: " + name)
        export[name] = raw
    child_python = _child_python()
    run_root.mkdir(parents=True)
    for sub in ("export", "subject", "operator"):
        (run_root / sub).mkdir()
    ledger = budget.Budget([run_root], free_paths=[tempfile.gettempdir()], **(budget_kwargs or {}))
    ctx = Context(run_root=run_root, budget=ledger, frozen=frozen, files=files, subject_pin=dict(subject_pin),
                  subject=run_root / "subject", child_python=child_python, engine=None, codec=None,
                  n_vectors=len(json.loads(files["ca-vectors.json"])), pins=pin, run_id=uuid.uuid4().hex,
                  interpreter_identity="sha256:" + sha((HERE / "ca_baseline.py").read_bytes()),
                  policy_identity="sha256:" + sha(POLICY), rec={"state": "prepared", "detail": None})
    try:
        for name, raw in export.items():
            ledger.operator_write(run_root / "export" / name, raw)
        for rel, raw in sorted(files.items()):
            (ctx.subject / rel).parent.mkdir(parents=True, exist_ok=True)
            ledger.operator_write(ctx.subject / rel, raw)
    except budget.BudgetRefused as exc:
        return _fail(ctx, "refused-before-dispatch", exc)
    ctx.engine, ctx.codec = _load_engine(run_root / "export", instrument["output_cap_bytes"])
    return ctx


# ------------------------------------------------------------------ helpers

def blob_reader(root):
    root = Path(root)

    def read(digest):
        hexd = digest.removeprefix("sha256:") if type(digest) is str else ""
        if len(hexd) != 64 or any(c not in "0123456789abcdef" for c in hexd):
            raise KeyError("malformed blob digest")
        path = root / "blobs" / hexd
        st = os.lstat(path)
        if not stat.S_ISREG(st.st_mode) or st.st_size > MAX_ARTIFACT:
            raise OSError("blob is not a bounded regular file")
        return path.read_bytes()
    return read


def _artifact(path):
    st = os.lstat(path)
    if not stat.S_ISREG(st.st_mode) or st.st_size > MAX_ARTIFACT:
        raise CarrierRefused("artifact is not a bounded regular file: " + str(path))
    return Path(path).read_bytes()


def _fail(ctx, state, exc=None):
    ctx.rec["state"] = state
    if exc is not None:
        ctx.rec["detail"] = f"{type(exc).__name__}: {exc}"[:500]
    return ctx


def dispatch_journal(ctx):
    """Independent dispatch count: unique invocation IDs across CA dispatch-*.json files of the prefix root."""
    root = ctx.run_root / "prefix"
    ids = {json.loads(_artifact(p))["invocation_id"] for p in root.rglob("dispatch-*.json")} if root.exists() else set()
    return len(ids)


# ------------------------------------------------------------------ phases

def run_prefix(ctx):
    rec = ctx.rec
    if rec["state"] != "prepared":
        return ctx
    context_raw = consumer.canonical({
        "schema": "refund.c2-baseline-context.v0", "run_id": ctx.run_id, "pins": ctx.pins["instrument"],
        "subject_pin": ctx.subject_pin, "child_python": ctx.child_python,
        "carrier_sha256": sha((HERE / "ca_baseline.py").read_bytes()),
        "consumer_sha256": sha((HERE / "consumer.py").read_bytes()),
        "budget_sha256": sha((HERE / "budget.py").read_bytes())})
    try:
        ctx.budget.operator_write(ctx.run_root / "operator" / "context.bin", context_raw)
        ctx.budget.begin("prefix", ctx.n_vectors)
    except budget.BudgetRefused as exc:
        return _fail(ctx, "refused-before-dispatch", exc)
    try:
        prefix_path = ctx.engine.observe_prefix(
            ctx.subject / "ca-manifest.json", execution_profile="trusted-local", context_raw=context_raw,
            output_root=ctx.run_root / "prefix", policy_identity=ctx.policy_identity,
            interpreter_identity=ctx.interpreter_identity, on_verified_receipt=ctx.budget.hook,
            stop_before="control")
    except Exception as exc:  # noqa: BLE001 - any CA or hook failure is retained as unclosed, never judged
        return _fail(ctx, "unclosed", exc)
    finally:
        ctx.budget.end()
    try:
        raw = _artifact(prefix_path)
        rec.update(prefix_path=prefix_path, prefix_raw=raw,
                   prefix=consumer.load_evidence(raw, "prefix", ctx.codec, blob_reader(prefix_path.parent)))
    except Exception as exc:  # noqa: BLE001 - a prefix that cannot be reloaded is never judged
        return _fail(ctx, "unclosed", exc)
    rec["state"] = "prefix-" + rec["prefix"].doc["phase"]
    return ctx


def close_prefix(ctx):
    rec = ctx.rec
    if "prefix" not in rec:
        return ctx
    try:
        ctx.budget.begin("close", 0, copy_allowance=budget.root_bytes(rec["prefix_path"].parent))
    except budget.BudgetRefused as exc:
        return _fail(ctx, "unresolved", exc)
    try:
        final_path = ctx.engine.close_observation(rec["prefix_path"], output_root=ctx.run_root / "close")
        raw = _artifact(final_path)
        final = consumer.load_evidence(raw, "final", ctx.codec, blob_reader(final_path.parent))
    except Exception as exc:  # noqa: BLE001 - CA raises ManifestError(Exception); retained as unresolved
        return _fail(ctx, "unresolved", exc)
    finally:
        ctx.budget.end()
    rec.update(final_path=final_path, final_raw=raw)
    if not consumer.final_binds_prefix(final, rec["prefix_raw"]):
        return _fail(ctx, "unresolved", CarrierRefused("final does not bind the exact prefix without admission"))
    rec["state"] = "closed"
    return ctx


def judge(ctx):
    """Judgement of a closed prefix, plus the source binding to the pinned reader bytes."""
    rec = ctx.rec
    if rec["state"] != "closed":
        return {"status": rec["state"], "stop": None, "cases": []}
    judgement = consumer.judge_prefix(rec["prefix"], ctx.frozen)
    doc = rec["prefix"].doc
    expected = source_digest(ctx.files["reader.py"])
    baseline = doc["steps"][1] if len(doc.get("steps", [])) > 1 else {}
    bound = doc["bindings"]["source_sha256"] == expected and baseline.get("source_sha256") in (expected, None)
    rec["source_bound"] = bound
    if not bound:
        judgement = {"status": "evidence-incomplete", "stop": {"reason": None, "shape": "source binding differs"},
                     "cases": []}
    return judgement


def _provenance(ctx, judgement, result_raw, persistence):
    rec = ctx.rec
    return {
        "schema": "refund.c2-baseline-provenance.v0", "run_id": ctx.run_id, "argv": list(sys.argv),
        "carrier_python": sys.version.split()[0], "child_python": ctx.child_python, "pins": ctx.pins,
        "subject_pin": ctx.subject_pin,
        "attempt": {"state": rec["state"], "detail": rec["detail"], "source_bound": rec.get("source_bound"),
                    "judged_status": judgement["status"], "persistence": persistence},
        "result_sha256": sha(result_raw),
        "session": rec["prefix"].doc["session"] if "prefix" in rec else None,
        "prefix_sha256": sha(rec["prefix_raw"]) if "prefix_raw" in rec else None,
        "final_sha256": sha(rec["final_raw"]) if "final_raw" in rec else None,
        "prefix_path": str(rec.get("prefix_path")) if "prefix_path" in rec else None,
        "final_path": str(rec.get("final_path")) if "final_path" in rec else None,
        "dispatch_journal": dispatch_journal(ctx), "budget": ctx.budget.record()}


def _publish(pending, final):
    """Exclusive publish: link (never overwrites an existing result.json), unlink the pending name, fsync dir."""
    os.link(pending, final, follow_symlinks=False)
    os.unlink(pending)
    fd = os.open(final.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def finish(ctx, judgement):
    """Persist result and provenance; success requires both, bound together (review 48bf F2).

    Order: reserve both sizes plus FINAL_MARGIN plus the publication peak; write result.pending.json; write
    provenance.json, which names the result's SHA256 and serves as the commit record; then publish result.json by
    exclusive link. The link briefly adds a second result-sized name, which the logical per-path budget counts
    (review 1b5 F4). The upfront reservation includes it, and an exact check (`final-publish`) runs immediately
    before the link, so a limit that fits one name but not the peak refuses before the second name exists. A failure at any
    stage (reservation, budget, host floor or OS error) gives status persistence-failed with a nonzero exit. A
    best-effort persistence-failure.json is attempted, and everything already written stays in place. So a
    result.json exists only when the bound pair is complete.
    """
    op = ctx.run_root / "operator"
    result = consumer.result_document(judgement)
    result_raw = consumer.canonical(result) + b"\n"
    draft = consumer.canonical(_provenance(ctx, judgement, result_raw, "pending")) + b"\n"
    stage = "reserve"
    try:
        ctx.budget.require_bytes("final-artifacts", 2 * len(result_raw) + len(draft) + FINAL_MARGIN)
        stage = "result"
        ctx.budget.operator_write(op / "result.pending.json", result_raw)
        stage = "provenance"
        provenance_raw = consumer.canonical(_provenance(ctx, judgement, result_raw, "provenance-written")) + b"\n"
        ctx.budget.operator_write(op / "provenance.json", provenance_raw)
        stage = "publish"
        ctx.budget.require_bytes("final-publish", (op / "result.pending.json").stat().st_size)
        _publish(op / "result.pending.json", op / "result.json")
    except (budget.BudgetRefused, OSError) as exc:
        persistence = {"state": "failed", "stage": stage, "detail": f"{type(exc).__name__}: {exc}"[:300]}
        try:
            ctx.budget.operator_write(op / "persistence-failure.json", consumer.canonical(
                {"schema": "refund.c2-baseline-persistence-failure.v0", **persistence,
                 "judged_status": judgement["status"]}) + b"\n")
        except (budget.BudgetRefused, OSError):
            pass  # the returned status and the exit code still carry the failure
        return {"status": "persistence-failed", "attempt_status": judgement["status"],
                "persistence": persistence, "result": result}
    return {"status": judgement["status"], "attempt_status": judgement["status"],
            "persistence": {"state": "complete", "stage": None, "detail": None}, "result": result}


def run(ca_repo, run_root, *, subject_root=None, subject_pin=None, frozen=None, budget_kwargs=None):
    ctx = prepare(ca_repo, run_root, subject_root=subject_root, subject_pin=subject_pin, frozen=frozen,
                  budget_kwargs=budget_kwargs)
    run_prefix(ctx)
    close_prefix(ctx)
    out = finish(ctx, judge(ctx))
    return {"status": out["status"], "attempt_status": out["attempt_status"], "persistence": out["persistence"],
            "state": ctx.rec["state"], "detail": ctx.rec["detail"],
            "verified_receipts": ctx.budget.verified_receipts, "dispatch_journal": dispatch_journal(ctx),
            "calls_reserved": ctx.budget.calls_reserved, "subject_pin": ctx.subject_pin, "result": out["result"]}


def exit_code(status):
    """0 closed, matching and fully persisted; 2 refused before any dispatch; 1 everything else.

    That includes persistence-failed, even when the judged attempt matched.
    """
    if status == "refused-before-dispatch":
        return 2
    return 0 if status == "baseline-matches-oracle" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--ca-repo", required=True, type=Path)
    parser.add_argument("--run-root", required=True, type=Path)
    args = parser.parse_args()
    try:
        summary = run(args.ca_repo, args.run_root)
    except CarrierRefused as exc:
        print("carrier refused: " + str(exc), file=sys.stderr)
        return 2
    print(json.dumps({k: summary[k] for k in ("status", "attempt_status", "persistence", "state", "detail",
                                              "verified_receipts", "dispatch_journal", "calls_reserved")},
                     sort_keys=True))
    return exit_code(summary["status"])


if __name__ == "__main__":
    sys.exit(main())
