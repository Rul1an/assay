#!/usr/bin/env python3
"""Offline C2 reader for synthetic packets (CONTRACT.md, schema refund.synthetic-c2.v0).

Per admitted operation it judges whether at most one committed effect is established from
fixture-declared effect evidence. No effect is executed and nothing is authenticated.
Stages: io -> parse -> schema -> binding; the first failing stage names the refusal category.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

MAX_BYTES = 1048576
MAX_DEPTH = 32
MAX_INT_DIGITS = 4300
MAX_AMOUNT = 2 ** 63 - 1
IDENTIFIER = re.compile(r"[A-Za-z0-9_.:-]{1,128}")
DIGEST = re.compile(r"[0-9a-f]{64}")
CURRENCY = re.compile(r"[A-Z]{3}")
STATUSES = ("pending", "committed", "failed", "reversed")
ALLOWED_PATHS = {
    ("pending",), ("pending", "committed"), ("pending", "failed"), ("pending", "committed", "reversed"),
    ("pending", "reversed"), ("committed",), ("committed", "reversed"), ("failed",), ("reversed",),
}
EVENT_FIELDS = {
    "dispatch": {"approval_digest", "workflow_key"},
    "observation": {"effect_id", "dispatch_id", "amount", "currency", "status"},
    "epoch_close": {"closer"},
}
NON_CLAIMS = ["no-live-execution", "no-provider-semantics", "no-exactly-once", "no-refund-safety",
              "no-custody-authentication", "no-runtime-coverage-proof", "no-adequacy-score"]


class Refusal(Exception):
    def __init__(self, category, message):
        super().__init__(message)
        self.category = category


def require(condition, category, message):
    if not condition:
        raise Refusal(category, message)


# ---------------------------------------------------------------- parse

class NonIntegerToken:
    """A JSON number token with a fraction and/or exponent; kept as text, never converted to binary64."""

    def __init__(self, text):
        self.text = text


def structural_depth(text):
    """Maximum container depth on the JSON text: root container 1, brackets inside strings ignored."""
    depth = deepest = 0
    in_string = escaped = False
    for char in text:
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char in "[{":
            depth += 1
            deepest = max(deepest, depth)
        elif char in "]}":
            depth -= 1
    return deepest


def integer_token(token):
    digits = token.lstrip("-")
    require(len(digits) <= MAX_INT_DIGITS, "parse", "integer token above 4300 digits")
    value = 0
    for start in range(0, len(digits), 100):  # chunked: independent of the host int/str digit limit
        chunk = digits[start:start + 100]
        value = value * 10 ** len(chunk) + int(chunk)
    return -value if token.startswith("-") else value


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "parse", "duplicate key")
        result[key] = value
    return result


def bareword(name):
    raise Refusal("parse", "nonfinite bareword " + name)


def parse(raw):
    require(len(raw) <= MAX_BYTES, "parse", "input above 1048576 bytes")
    require(not raw.startswith(b"\xef\xbb\xbf"), "parse", "byte order mark")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise Refusal("parse", "invalid UTF-8") from None
    require(structural_depth(text) <= MAX_DEPTH, "parse", "structural depth above 32")
    try:
        return json.loads(text, object_pairs_hook=unique_object, parse_int=integer_token,
                          parse_float=NonIntegerToken, parse_constant=bareword)
    except (json.JSONDecodeError, RecursionError):
        raise Refusal("parse", "invalid JSON") from None


# --------------------------------------------------------------- schema

def identifier(value):
    return type(value) is str and IDENTIFIER.fullmatch(value) is not None


def digest(value):
    return type(value) is str and DIGEST.fullmatch(value) is not None


def closed(value, keys, what):
    require(type(value) is dict and set(value) == keys, "schema", what + " keys")


def check_schema(packet):
    closed(packet, {"schema", "case_id", "source_class", "custody", "effect_coverage", "admissions", "events"}, "root")
    require(packet["schema"] == "refund.synthetic-c2.v0", "schema", "schema value")
    require(identifier(packet["case_id"]), "schema", "case_id")
    require(packet["source_class"] == "synthetic_fixture", "schema", "source_class")
    require(identifier(packet["custody"]), "schema", "custody")
    require(packet["effect_coverage"] in ("complete", "incomplete"), "schema", "effect_coverage")
    admissions, events = packet["admissions"], packet["events"]
    require(type(admissions) is list and 1 <= len(admissions) <= 100, "schema", "admissions count")
    require(type(events) is list and 1 <= len(events) <= 1000, "schema", "events count")
    for a in admissions:
        closed(a, {"admission_id", "approval_digest", "operation_id", "reissue_of"}, "admission")
        require(identifier(a["admission_id"]) and digest(a["approval_digest"]) and identifier(a["operation_id"]),
                "schema", "admission field")
        require(a["reissue_of"] is None or identifier(a["reissue_of"]), "schema", "reissue_of")
    for e in events:
        require(type(e) is dict and type(e.get("kind")) is str and e["kind"] in EVENT_FIELDS, "schema", "event kind")
        closed(e, {"id", "seq", "kind"} | EVENT_FIELDS[e["kind"]], "event")
        require(identifier(e["id"]) and type(e["seq"]) is int, "schema", "event id/seq")
        if e["kind"] == "dispatch":
            require(e["approval_digest"] is None or digest(e["approval_digest"]), "schema", "dispatch digest")
            require(e["workflow_key"] is None or identifier(e["workflow_key"]), "schema", "workflow_key")
        elif e["kind"] == "observation":
            require(identifier(e["effect_id"]) and identifier(e["dispatch_id"]), "schema", "observation ids")
            require(type(e["amount"]) is int and 1 <= e["amount"] <= MAX_AMOUNT, "schema", "amount")
            require(type(e["currency"]) is str and CURRENCY.fullmatch(e["currency"]) is not None, "schema", "currency")
            require(e["status"] in STATUSES, "schema", "status")
        else:
            require(identifier(e["closer"]), "schema", "closer")


# -------------------------------------------------------------- binding

def collapse(statuses):
    path = []
    for status in statuses:
        if not path or path[-1] != status:
            path.append(status)
    return tuple(path)


def check_binding(packet):
    events = packet["events"]
    event_ids, dispatch_ids, closes, effects = set(), set(), 0, {}
    for index, e in enumerate(events, 1):
        require(e["seq"] == index, "binding", "sequence must equal array position")
        require(e["id"] not in event_ids, "binding", "duplicate event id")
        event_ids.add(e["id"])
        if e["kind"] == "dispatch":
            dispatch_ids.add(e["id"])
        elif e["kind"] == "epoch_close":
            closes += 1
        else:
            require(e["dispatch_id"] in dispatch_ids, "binding", "observation needs an earlier dispatch")
            fixed = (e["dispatch_id"], e["amount"], e["currency"])
            first = effects.setdefault(e["effect_id"], {"fixed": fixed, "statuses": []})
            require(first["fixed"] == fixed, "binding", "conflicting immutable effect fields")
            first["statuses"].append(e["status"])
    require(closes <= 1, "binding", "more than one epoch_close")
    for effect in effects.values():
        require(collapse(effect["statuses"]) in ALLOWED_PATHS, "binding", "invalid lifecycle path")
    seen_admissions, digests, owner = {}, set(), {}
    for a in packet["admissions"]:
        require(a["admission_id"] not in seen_admissions, "binding", "duplicate admission_id")
        require(a["approval_digest"] not in digests, "binding", "duplicate approval_digest")
        if a["operation_id"] not in owner:
            require(a["reissue_of"] is None, "binding", "first admission of an operation must not be a reissue")
            owner[a["operation_id"]] = True
        else:
            target = seen_admissions.get(a["reissue_of"])
            require(target == a["operation_id"], "binding", "reissue must name an earlier admission of the operation")
        seen_admissions[a["admission_id"]] = a["operation_id"]
        digests.add(a["approval_digest"])


# ------------------------------------------------------------ semantics

def evaluate(packet, raw_sha256):
    operation_of = {a["approval_digest"]: a["operation_id"] for a in packet["admissions"]}
    dispatches, observations, close_seq = {}, {}, None
    for e in packet["events"]:
        if e["kind"] == "dispatch":
            dispatches[e["id"]] = (operation_of.get(e["approval_digest"]), e["workflow_key"])
        elif e["kind"] == "observation":
            observations.setdefault(e["effect_id"], []).append((e["seq"], e["status"], e["dispatch_id"]))
        else:
            close_seq = e["seq"]
    closure_contradicted = False
    effects = {}
    for effect_id, records in observations.items():
        path = collapse(status for _, status, _ in records)
        operation, _ = dispatches[records[0][2]]
        commit_seqs = [seq for seq, status, _ in records if status in ("committed", "reversed")]
        if close_seq is not None:
            before = [status for seq, status, _ in records if seq < close_seq]
            for seq, status, _ in records:
                if seq > close_seq and (not before or status != before[-1]):
                    closure_contradicted = True
        effects[effect_id] = {
            "operation": operation,
            "key": dispatches[records[0][2]][1],
            "counts": "committed" in path or "reversed" in path,
            "unobserved": "reversed" in path and "committed" not in path,
            "reversed": "reversed" in path,
            "pending": path[-1] == "pending",
            "late": close_seq is not None and bool(commit_seqs) and commit_seqs[0] > close_seq,
        }
    unattributed = {k: v for k, v in effects.items() if v["operation"] is None}
    blocking = sorted(k for k, v in unattributed.items() if v["counts"] or v["pending"])
    coverage = packet["effect_coverage"]
    rows = []
    for operation in sorted({a["operation_id"] for a in packet["admissions"]}):
        own = {k: v for k, v in effects.items() if v["operation"] == operation}
        committed = sorted(k for k, v in own.items() if v["counts"])
        keys = sorted({key for op, key in dispatches.values() if op == operation and key is not None})
        if len(committed) >= 2:
            result, reason = "CONTRADICTED", "multiple_committed_effects"
        elif closure_contradicted:
            result, reason = "NOT_ESTABLISHED", "closure_contradicted"
        elif close_seq is None:
            result, reason = "NOT_ESTABLISHED", "epoch_open"
        elif coverage == "incomplete":
            result, reason = "NOT_ESTABLISHED", "incomplete_effect_coverage"
        elif blocking:
            result, reason = "NOT_ESTABLISHED", "unattributed_effect_in_scope"
        elif any(v["pending"] for v in own.values()):
            result, reason = "NOT_ESTABLISHED", "pending_at_closure"
        else:
            result, reason = "ESTABLISHED", "at_most_one_committed"
        rows.append({
            "operation_id": operation, "result": result, "reason": reason,
            "committed_effect_ids": committed,
            "late_effect_ids": sorted(k for k, v in own.items() if v["counts"] and v["late"]),
            "commit_unobserved_effect_ids": sorted(k for k, v in own.items() if v["unobserved"]),
            "reversed_effect_ids": sorted(k for k, v in own.items() if v["reversed"]),
            "workflow_keys": keys,
            "key_hint_effect_ids": sorted(k for k, v in unattributed.items() if v["key"] in keys),
        })
    return {"schema": "refund.c2-report.v0", "case_id": packet["case_id"], "packet_sha256": raw_sha256,
            "source_class": packet["source_class"], "custody": packet["custody"],
            "evidence_basis": "fixture_declared", "effect_coverage": coverage,
            "epoch": "open" if close_seq is None else "closed", "closure_contradicted": closure_contradicted,
            "unattributed_blocking_effect_ids": blocking, "operations": rows, "non_claims": list(NON_CLAIMS)}


def main(argv):
    try:
        require(len(argv) == 2, "io", "usage: reader.py PACKET")
        try:
            with Path(argv[1]).open("rb") as stream:
                raw = stream.read(MAX_BYTES + 1)  # bounded: never materializes beyond limit + 1
        except OSError as exc:
            raise Refusal("io", "cannot read packet: " + type(exc).__name__) from None
        packet = parse(raw)
        check_schema(packet)
        check_binding(packet)
        report = evaluate(packet, hashlib.sha256(raw).hexdigest())
    except Refusal as exc:
        print(f"refused: {exc.category}: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
