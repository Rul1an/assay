"""Strict offline consumer for C2 baseline-only observation evidence.

Pattern adapted, with attribution, from refund-c3-mutation-2026-10/consumer.py (not imported from there).

It never re-derives C2 semantics and imports neither the reader nor the reproducer. Expected values come only
from the frozen literal oracle (expectations.json). Identity fields are equality-checked against the frozen
fixture bytes; semantic fields are type/enum-checked and then compared, never forced to match.

Every entry point takes a `Loaded` value, which only `load_evidence` creates, and only after CA's public
`execution_observation.load_observation` has validated the canonical bytes. Blobs are read through
`read_blob(digest) -> bytes`; the consumer verifies every blob digest it reads. Each observed slot must carry
invocation evidence showing exit 0, empty stderr and stdout bytes equal to the raw report, all bound to the
same step, vector and source. Abnormal, missing, unsafe or misaligned evidence is retained but never promoted.
"""
import hashlib
import json

REPORT_SCHEMA = "refund.c2-report.v0"
RESULT_SCHEMA = "refund.c2-baseline-result.v0"
NON_CLAIMS = ["no-live-execution", "no-provider-semantics", "no-exactly-once", "no-refund-safety",
              "no-custody-authentication", "no-runtime-coverage-proof", "no-adequacy-score"]
SELECTORS = ("operations", "epoch", "closure_contradicted", "unattributed_blocking_effect_ids")
ROOT_KEYS = frozenset(("schema", "case_id", "packet_sha256", "source_class", "custody", "evidence_basis",
                       "effect_coverage", "epoch", "closure_contradicted", "unattributed_blocking_effect_ids",
                       "operations", "non_claims"))
ROW_KEYS = frozenset(("operation_id", "result", "reason", "committed_effect_ids", "late_effect_ids",
                      "commit_unobserved_effect_ids", "reversed_effect_ids", "workflow_keys", "key_hint_effect_ids"))
ROW_LISTS = ("committed_effect_ids", "late_effect_ids", "commit_unobserved_effect_ids", "reversed_effect_ids",
             "workflow_keys", "key_hint_effect_ids")
RESULTS = frozenset(("ESTABLISHED", "CONTRADICTED", "NOT_ESTABLISHED"))
REASONS = frozenset(("at_most_one_committed", "multiple_committed_effects", "closure_contradicted", "epoch_open",
                     "incomplete_effect_coverage", "unattributed_effect_in_scope", "pending_at_closure"))
EPOCHS = frozenset(("open", "closed"))
IDENTITY = ("case_id", "packet_sha256", "source_class", "custody", "effect_coverage")
IDENTITY_LABEL = {"packet_sha256": "packet digest"}  # result documents carry no digest-named strings
EVIDENCE_KEYS = frozenset(("schema", "dispatch_sha256", "invocation_id", "step_id", "vector_id", "source_sha256",
                           "stdout_sha256", "stdout_size", "stderr_sha256", "stderr_size", "returncode", "abnormal",
                           "selector_presence"))
ROLES = (("build", None), ("baseline", None), ("control", "positive"), ("ordinary", None))


class ContractFailure(Exception):
    """Output or evidence that cannot be measured; never counted as a semantic result."""


class FrozenInputError(ValueError):
    """Frozen oracle/fixtures are inconsistent; refuse before reading evidence."""


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def strict_json(raw, error=ContractFailure):
    def pairs(items):
        out = {}
        for key, value in items:
            if key in out:
                raise error("duplicate key")
            out[key] = value
        return out

    def constant(name):
        raise error("nonfinite constant " + name)

    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=pairs, parse_constant=constant)
    except (UnicodeDecodeError, ValueError, RecursionError) as exc:
        if isinstance(exc, error):
            raise
        raise error("invalid JSON: " + type(exc).__name__) from None


class Loaded:
    """Evidence that passed the public CA codec; construct only via load_evidence."""
    __slots__ = ("_token", "doc", "kind", "raw", "read_blob")

    def __init__(self, doc, kind, raw, read_blob, token):
        if token is not _TOKEN:
            raise TypeError("use load_evidence")
        self.doc, self.kind, self.raw, self.read_blob, self._token = doc, kind, raw, read_blob, token


_TOKEN = object()


def load_evidence(raw, kind, codec, read_blob):
    """Validate raw canonical observation bytes with the given public CA codec."""
    if kind not in ("prefix", "final"):
        raise ValueError("unsupported observation kind")
    doc = codec.load_observation(raw, kind=kind)
    return Loaded(doc, kind, raw, read_blob, _TOKEN)


def _require_loaded(value, kinds):
    if type(value) is not Loaded:
        raise TypeError("evidence must come from load_evidence")
    if value.kind not in kinds:
        raise ContractFailure(f"expected {'/'.join(kinds)} evidence, got {value.kind}")


class Frozen:
    """Literal oracle and fixture identities."""

    def __init__(self, *, oracle, fixtures):
        self.oracle = oracle
        self.identity = {}
        for case_id, raw in fixtures.items():
            doc = strict_json(raw, FrozenInputError)
            if type(doc) is not dict or doc.get("case_id") != case_id:
                raise FrozenInputError("fixture case_id differs: " + case_id)
            self.identity[case_id] = {"case_id": case_id, "packet_sha256": sha(raw),
                                      "source_class": doc["source_class"], "custody": doc["custody"],
                                      "effect_coverage": doc["effect_coverage"]}
        if sorted(self.identity) != sorted(oracle):
            raise FrozenInputError("fixtures and oracle cover different cases")
        for case_id, expected in oracle.items():
            if set(expected) != set(SELECTORS) or not expected["operations"]:
                raise FrozenInputError("oracle entry shape: " + case_id)


def _check(condition, message):
    if not condition:
        raise ContractFailure(message)


def _str_list(value):
    return type(value) is list and all(type(v) is str for v in value)


def validate_report(raw, identity, n_rows):
    """Full-shape and identity validation of one reader stdout; returns the report."""
    doc = strict_json(raw)
    _check(type(doc) is dict and set(doc) == ROOT_KEYS, "root keys")
    _check(doc["schema"] == REPORT_SCHEMA, "schema")
    for key in IDENTITY:
        _check(doc[key] == identity[key] and type(doc[key]) is str, "identity " + IDENTITY_LABEL.get(key, key))
    _check(doc["evidence_basis"] == "fixture_declared", "evidence_basis")
    _check(doc["non_claims"] == NON_CLAIMS, "non_claims")
    _check(doc["epoch"] in EPOCHS, "epoch enum")
    _check(type(doc["closure_contradicted"]) is bool, "closure_contradicted type")
    _check(_str_list(doc["unattributed_blocking_effect_ids"]), "unattributed_blocking_effect_ids type")
    rows = doc["operations"]
    _check(type(rows) is list and len(rows) == n_rows, "row count")
    for row in rows:
        _check(type(row) is dict and set(row) == ROW_KEYS, "row keys")
        _check(type(row["operation_id"]) is str, "operation_id type")
        _check(row["result"] in RESULTS, "result enum")
        _check(row["reason"] in REASONS, "reason enum")
        for key in ROW_LISTS:
            _check(_str_list(row[key]), key + " type")
    return doc


def _aligned(doc):
    schedule, steps = doc.get("schedule", []), doc.get("steps", [])
    return len(schedule) == len(steps) and all(
        row.get("step_id") == step.get("step_id") for row, step in zip(schedule, steps))


def _blob(read_blob, digest, what):
    raw = read_blob(digest)
    _check("sha256:" + sha(raw) == digest, what + " blob digest mismatch")
    return raw


def _check_receipt(slot, step, schedule_row, read_blob):
    """The receipt and its invocation evidence must bind this step, vector and source, exit 0, empty stderr."""
    receipt = slot["receipt"]
    _check(type(receipt) is dict, "receipt missing")
    _check(receipt.get("step_id") == schedule_row["step_id"] and receipt.get("vector_id") == slot["vector_id"],
           "receipt binds another invocation")
    _check(step.get("source_sha256") is not None and receipt.get("source_sha256") == step["source_sha256"],
           "receipt source differs from step source")
    raw = _blob(read_blob, receipt["raw_sha256"], "raw")
    _check(len(raw) == receipt["raw_size"], "raw size")
    evidence = strict_json(_blob(read_blob, receipt["evidence_sha256"], "evidence"))
    _check(type(evidence) is dict and set(evidence) == EVIDENCE_KEYS, "invocation evidence keys")
    _check(evidence["schema"] == "corpus-adequacy.invocation-evidence.v0", "invocation evidence schema")
    for key in ("invocation_id", "step_id", "vector_id", "source_sha256"):
        _check(evidence[key] == receipt[key], "invocation evidence differs from receipt: " + key)
    _check(evidence["stdout_sha256"] == receipt["raw_sha256"] and evidence["stdout_size"] == len(raw),
           "stdout differs from raw report")
    _check(evidence["returncode"] == 0 and type(evidence["returncode"]) is int, "nonzero exit")
    _check(evidence["stderr_size"] == 0, "stderr on success")
    _check(evidence["stderr_sha256"] == "sha256:" + sha(b""), "stderr on success")
    _check(evidence["abnormal"] is None, "abnormal invocation")
    _check(evidence["selector_presence"] == [True, True], "selector presence")
    dispatch = strict_json(_blob(read_blob, evidence["dispatch_sha256"], "dispatch"))
    _check(type(dispatch) is dict and dispatch.get("schema") == "corpus-adequacy.invocation-dispatch.v0",
           "dispatch schema")
    # Receipt, invocation evidence and dispatch must all name the same invocation (review 48bf F3); a missing
    # dispatch invocation_id fails too. This binds the recorded bytes to each other; it authenticates no producer.
    _check(type(receipt["invocation_id"]) is str and "invocation_id" in dispatch
           and dispatch["invocation_id"] == receipt["invocation_id"], "dispatch invocation_id differs from receipt")
    for key in ("step_id", "vector_id", "source_sha256"):
        _check(dispatch.get(key) == receipt[key], "dispatch differs from receipt: " + key)
    return raw


def _differences(report, expected):
    """Root-field and per-operation differences against the literal oracle; never pooled."""
    out = []
    for key in ("epoch", "closure_contradicted", "unattributed_blocking_effect_ids"):
        if report[key] != expected[key]:
            out.append({"operation_id": None, "fields": [key], "oracle": expected[key], "observed": report[key]})
    for got, want in zip(report["operations"], expected["operations"]):
        fields = sorted(k for k in ROW_KEYS if got[k] != want[k])
        if fields:
            out.append({"operation_id": want["operation_id"], "fields": fields, "oracle": want, "observed": got})
    return out


def _invocation(slot, read_blob):
    """Bounded exit facts from digest-verified invocation evidence; None when absent or unreadable."""
    receipt = slot.get("receipt")
    if type(receipt) is not dict:
        return None
    try:
        evidence = strict_json(_blob(read_blob, receipt["evidence_sha256"], "evidence"))
        return {k: evidence[k] for k in ("returncode", "abnormal", "stdout_size", "stderr_size")}
    except (ContractFailure, KeyError, OSError, TypeError):
        return None


def _read_case(slot, step, schedule_row, read_blob, frozen):
    case_id = slot.get("vector_id")
    entry = {"case_id": case_id, "status": None, "operations": None, "projection": None, "differences": [],
             "detail": None, "invocation": _invocation(slot, read_blob)}
    state = slot.get("state")
    if state != "observed":
        entry.update(status={"abnormal": "abnormal", "not_run": "not-run"}.get(state, "missing"),
                     detail=slot.get("reason"))
        return entry
    expected = frozen.oracle[case_id]
    try:
        raw = _check_receipt(slot, step, schedule_row, read_blob)
        report = validate_report(raw, frozen.identity[case_id], len(expected["operations"]))
        _check([r["operation_id"] for r in report["operations"]] == [r["operation_id"] for r in expected["operations"]],
               "operation identity or order")
        _check(slot.get("outcome") == [report[k] for k in SELECTORS], "CA projection differs from raw report")
    except (ContractFailure, KeyError, OSError, TypeError) as exc:
        entry.update(status="contract-failure", detail=type(exc).__name__ + ": " + str(exc)[:200])
        return entry
    differences = _differences(report, expected)
    entry.update(status="differs-from-oracle" if differences else "matches-oracle",
                 operations=[[r["operation_id"], r["result"], r["reason"]] for r in report["operations"]],
                 projection=slot["outcome"], differences=differences)
    return entry


def _unstarted(step):
    return (step.get("state") == "not_run" and step.get("application") == "not_run"
            and all(s.get("state") == "not_run" for s in step.get("slots", [])))


def _shape(doc):
    """Return 'healthy-stop', 'baseline-stop' or a reason the evidence is incomplete."""
    if doc.get("phase") != "stopped":
        return "phase " + str(doc.get("phase"))
    if not _aligned(doc):
        return "schedule and steps are misaligned"
    roles = [(r.get("kind"), r.get("control_polarity")) for r in doc["schedule"]]
    if roles != list(ROLES):
        return "schedule roles differ from build/baseline/positive control/ordinary"
    cleanup = doc.get("cleanup") or {}
    if cleanup.get("restored") is not True or cleanup.get("isolated_tree_removed") is not True:
        return "cleanup not both restored and removed"
    _, baseline, control, ordinary = doc["steps"]
    closure = doc.get("closure") or {}
    if not _unstarted(ordinary):
        return "ordinary step started"
    if (closure.get("reason") == "operator-refused" and closure.get("stop_step") == control["step_id"]
            and control.get("state") == "stopped" and control.get("application") == "not_run"
            and all(s.get("state") == "not_run" for s in control.get("slots", []))
            and baseline.get("state") == "complete"):
        return "healthy-stop"
    if (closure.get("stop_step") == baseline["step_id"] and closure.get("reason") != "operator-refused"
            and _unstarted(control)):
        return "baseline-stop"
    return "stop shape differs from the baseline-only route"


def judge_prefix(loaded, frozen):
    """Judge one stopped baseline-only prefix against the literal oracle."""
    _require_loaded(loaded, ("prefix",))
    doc = loaded.doc
    shape = _shape(doc)
    closure = doc.get("closure") or {}
    stop = {"reason": closure.get("reason"), "shape": shape}
    if shape not in ("healthy-stop", "baseline-stop"):
        return {"status": "evidence-incomplete", "stop": stop, "cases": []}
    schedule_row, step = doc["schedule"][1], doc["steps"][1]
    by_id = {s.get("vector_id"): s for s in step.get("slots", [])}
    cases = []
    for case_id in sorted(frozen.oracle):
        slot = by_id.get(case_id)
        if slot is None:
            cases.append({"case_id": case_id, "status": "missing", "operations": None, "projection": None,
                          "differences": [], "detail": "no slot", "invocation": None})
        else:
            cases.append(_read_case(slot, step, schedule_row, loaded.read_blob, frozen))
    statuses = {c["status"] for c in cases}
    if shape == "baseline-stop" or statuses - {"matches-oracle", "differs-from-oracle"}:
        status = "baseline-failed"
    elif "differs-from-oracle" in statuses:
        status = "baseline-differs-from-oracle"
    else:
        status = "baseline-matches-oracle"
    return {"status": status, "stop": stop, "cases": cases}


def final_binds_prefix(final_loaded, prefix_raw):
    """The closed final must name exactly these prefix bytes and carry no admission or consumption."""
    _require_loaded(final_loaded, ("final",))
    closure = final_loaded.doc.get("closure") or {}
    return (closure.get("prefix_sha256") == "sha256:" + sha(prefix_raw)
            and closure.get("admission_sha256") is None and closure.get("consumption_sha256") is None)


def result_document(judgement):
    """Deterministic semantic result; carries no digests, session IDs or paths."""
    return {"schema": RESULT_SCHEMA, "status": judgement["status"], "stop": judgement["stop"],
            "cases": [{k: c[k] for k in ("case_id", "status", "operations", "differences", "detail", "invocation")}
                      for c in judgement["cases"]],
            "non_claims": NON_CLAIMS}
