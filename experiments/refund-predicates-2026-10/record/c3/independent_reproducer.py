"""Independent reproducer for the synthetic C3 wire contract v0 (CONTRACT.md).

Derived from CONTRACT.md and the frozen fixtures/expectations only; it imports
no reader, carrier, Assay or Corpus Adequacy code. Decoding is a hand-written
scanner (no json.loads): duplicate keys (including escaped surrogate pairs),
nonfinite numbers, depth, digit count and trailing data are refused by
construction. json.dumps is used only to write the report.

Stages run in the frozen order io -> parse -> schema -> binding; the first
failing stage names the refusal category.

Usage: python3 independent_reproducer.py PACKET
Exit 0 with one JSON report; exit 2 with "refused: CATEGORY: ..." on stderr.
"""
import hashlib
import json
import math
import sys

MAX_BYTES = 1048576
MAX_DEPTH = 32
MAX_INT_DIGITS = 4300
MAX_EVENTS = 1000
DIGITS = "0123456789"
HEX = "0123456789abcdef"
ID_CHARS = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz" + DIGITS + "_.:-")
ROOT_KEYS = frozenset(("schema", "case_id", "source_class", "operation_id", "delivery_coverage", "events"))
BASE_EVENT_KEYS = ("id", "seq", "kind", "operation_id")
KIND_FIELD = {"dispatch": None, "provider_result": "status", "delivery": "result_ref",
              "client_report": "outcome"}
STATUSES = ("committed", "rejected_final", "pending")
OUTCOMES = ("succeeded", "failed", "unknown")
TERMINAL_FOR = {"succeeded": "committed", "failed": "rejected_final"}
NON_CLAIMS = ["no-live-execution", "no-runtime-coverage-proof", "no-provider-authentication",
              "no-effect-truth", "no-C2", "no-adequacy-score"]


class Refused(Exception):
    def __init__(self, category, detail):
        super().__init__("%s: %s" % (category, detail))
        self.category = category


# ---------------------------------------------------------------- parse stage

class Scanner:
    def __init__(self, text):
        self.s = text
        self.i = 0

    def fail(self, why):
        raise Refused("parse", "%s at offset %d" % (why, self.i))

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ""

    def skip_ws(self):
        while self.i < len(self.s) and self.s[self.i] in " \t\n\r":
            self.i += 1

    def document(self):
        self.skip_ws()
        value = self.value(0)
        self.skip_ws()
        if self.i != len(self.s):
            self.fail("trailing data")
        return value

    def value(self, depth):
        c = self.peek()
        if c == "{" or c == "[":
            if depth + 1 > MAX_DEPTH:
                self.fail("container depth above %d" % MAX_DEPTH)
            return self.obj(depth + 1) if c == "{" else self.arr(depth + 1)
        if c == '"':
            return self.string()
        if c == "-" or (c and c in DIGITS):
            return self.number()
        for word, result in (("true", True), ("false", False), ("null", None)):
            if self.s.startswith(word, self.i):
                self.i += len(word)
                return result
        self.fail("unexpected token")

    def obj(self, depth):
        self.i += 1
        self.skip_ws()
        members = {}
        if self.peek() == "}":
            self.i += 1
            return members
        while True:
            if self.peek() != '"':
                self.fail("object key expected")
            key = self.string()
            if key in members:
                self.fail("duplicate key")
            self.skip_ws()
            if self.peek() != ":":
                self.fail("colon expected")
            self.i += 1
            self.skip_ws()
            members[key] = self.value(depth)
            self.skip_ws()
            c = self.peek()
            self.i += 1
            if c == "}":
                return members
            if c != ",":
                self.fail("comma or brace expected")
            self.skip_ws()

    def arr(self, depth):
        self.i += 1
        self.skip_ws()
        items = []
        if self.peek() == "]":
            self.i += 1
            return items
        while True:
            items.append(self.value(depth))
            self.skip_ws()
            c = self.peek()
            self.i += 1
            if c == "]":
                return items
            if c != ",":
                self.fail("comma or bracket expected")
            self.skip_ws()

    def hex4(self):
        code = self.s[self.i:self.i + 4].lower()
        if len(code) != 4 or any(h not in HEX for h in code):
            self.fail("bad unicode escape")
        self.i += 4
        unit = 0
        for h in code:
            unit = unit * 16 + HEX.index(h)
        return unit

    def string(self):
        self.i += 1
        out = []
        simple = {'"': '"', "\\": "\\", "/": "/", "b": "\b", "f": "\f", "n": "\n", "r": "\r", "t": "\t"}
        while True:
            if self.i >= len(self.s):
                self.fail("unterminated string")
            c = self.s[self.i]
            self.i += 1
            if c == '"':
                return "".join(out)
            if ord(c) < 0x20:
                self.fail("raw control character")
            if c != "\\":
                out.append(c)
                continue
            e = self.peek()
            self.i += 1
            if e in simple:
                out.append(simple[e])
            elif e == "u":
                unit = self.hex4()
                # An escaped surrogate pair names the same scalar as the literal character.
                if 0xD800 <= unit <= 0xDBFF and self.s.startswith("\\u", self.i):
                    mark = self.i
                    self.i += 2
                    low = self.hex4()
                    if 0xDC00 <= low <= 0xDFFF:
                        unit = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
                    else:
                        self.i = mark
                out.append(chr(unit))
            else:
                self.fail("bad escape")

    def number(self):
        start = self.i
        if self.peek() == "-":
            self.i += 1
        if self.peek() == "0":
            self.i += 1
        elif self.peek() and self.peek() in "123456789":
            self.digits()
        else:
            self.fail("bad number")
        integral = True
        if self.peek() == ".":
            integral = False
            self.i += 1
            self.digits(required=True)
        if self.peek() and self.peek() in "eE":
            integral = False
            self.i += 1
            if self.peek() and self.peek() in "+-":
                self.i += 1
            self.digits(required=True)
        token = self.s[start:self.i]
        if integral:
            return exact_integer(token, self.fail)
        value = float(token)
        if not math.isfinite(value):
            self.fail("number outside finite binary64")
        return value

    def digits(self, required=False):
        if required and not (self.peek() and self.peek() in DIGITS):
            self.fail("digit expected")
        while self.peek() and self.peek() in DIGITS:
            self.i += 1


def exact_integer(token, fail):
    """Chunked accumulation: never depends on the host int/str digit limit."""
    negative = token.startswith("-")
    digits = token[1:] if negative else token
    if len(digits) > MAX_INT_DIGITS:
        fail("integer above %d digits" % MAX_INT_DIGITS)
    value = 0
    for at in range(0, len(digits), 9):
        chunk = digits[at:at + 9]
        part = 0
        for d in chunk:
            part = part * 10 + DIGITS.index(d)
        value = value * 10 ** len(chunk) + part
    return -value if negative else value


def parse(raw):
    """Bytes to an arbitrary JSON value; no schema knowledge."""
    if len(raw) > MAX_BYTES:
        raise Refused("parse", "input above %d bytes" % MAX_BYTES)
    if raw.startswith(b"\xef\xbb\xbf"):
        raise Refused("parse", "byte order mark")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise Refused("parse", "invalid UTF-8 at byte %d" % exc.start) from None
    return Scanner(text).document()


# --------------------------------------------------------------- schema stage

def ident(v):
    return type(v) is str and 0 < len(v) <= 128 and all(ch in ID_CHARS for ch in v)


def schema_check(doc):
    def need(ok, what):
        if not ok:
            raise Refused("schema", what)

    need(type(doc) is dict and set(doc) == ROOT_KEYS, "root keys")
    need(doc["schema"] == "refund.synthetic-c3.v0", "schema value")
    need(ident(doc["case_id"]), "case_id")
    need(doc["source_class"] == "synthetic_fixture", "source_class")
    need(ident(doc["operation_id"]), "operation_id")
    need(doc["delivery_coverage"] in ("complete", "incomplete"), "delivery_coverage")
    events = doc["events"]
    need(type(events) is list and 1 <= len(events) <= MAX_EVENTS, "events count")
    for n, ev in enumerate(events):
        where = "event %d" % n
        need(type(ev) is dict and type(ev.get("kind")) is str and ev["kind"] in KIND_FIELD, where + " kind")
        extra = KIND_FIELD[ev["kind"]]
        need(set(ev) == set(BASE_EVENT_KEYS) | ({extra} if extra else set()), where + " keys")
        need(ident(ev["id"]), where + " id")
        need(ident(ev["operation_id"]), where + " operation_id")
        need(type(ev["seq"]) is int and 1 <= ev["seq"] <= MAX_EVENTS, where + " seq")
        if extra == "status":
            need(ev["status"] in STATUSES, where + " status")
        elif extra == "result_ref":
            need(ident(ev["result_ref"]), where + " result_ref")
        elif extra == "outcome":
            need(ev["outcome"] in OUTCOMES, where + " outcome")


# -------------------------------------------------------------- binding stage

def binding_check(doc):
    def need(ok, what):
        if not ok:
            raise Refused("binding", what)

    events = doc["events"]
    seen = {}
    for position, ev in enumerate(events, start=1):
        need(ev["operation_id"] == doc["operation_id"], "operation mismatch at %d" % position)
        need(ev["seq"] == position, "sequence out of order at %d" % position)
        need(ev["id"] not in seen, "duplicate id %s" % ev["id"])
        if ev["kind"] == "delivery":
            target = seen.get(ev["result_ref"])
            need(target is not None and target["kind"] == "provider_result",
                 "delivery %s must reference an earlier provider_result" % ev["id"])
        seen[ev["id"]] = ev
    kinds = [ev["kind"] for ev in events]
    need(kinds[0] == "dispatch" and kinds.count("dispatch") == 1, "exactly one dispatch, first")
    need("client_report" in kinds, "at least one client_report")


# ----------------------------------------------------------- interpretation

def triple(applicable, result, reason):
    return {"applicable": applicable, "result": result, "reason": reason}


def judge(outcome, terminals, complete):
    """Return (definite_support, uncertainty_calibration) for one report."""
    if outcome == "unknown":
        support = triple(False, None, "not_definite_report")
        if len(terminals) == 2:
            calibration = triple(True, "ESTABLISHED", "conflicting_terminal_evidence")
        elif not complete:
            calibration = triple(True, "NOT_ESTABLISHED", "incomplete_visibility")
        elif terminals:
            calibration = triple(True, "CONTRADICTED", "unnecessarily_unknown")
        else:
            calibration = triple(True, "ESTABLISHED", "unresolved_support")
        return support, calibration
    calibration = triple(False, None, "not_unknown_report")
    wanted = TERMINAL_FOR[outcome]
    if terminals - {wanted}:
        support = triple(True, "CONTRADICTED", "contrary_terminal_evidence")
    elif not complete:
        support = triple(True, "NOT_ESTABLISHED", "incomplete_visibility")
    elif terminals:
        support = triple(True, "ESTABLISHED", "terminal_support")
    else:
        support = triple(True, "CONTRADICTED", "unsupported_definite_report")
    return support, calibration


def interpret(doc):
    complete = doc["delivery_coverage"] == "complete"
    status_of = {}
    delivered, results, terminals, claims = [], [], set(), []
    for ev in doc["events"]:
        kind = ev["kind"]
        if kind == "provider_result":
            status_of[ev["id"]] = ev["status"]
        elif kind == "delivery":
            delivered.append(ev["id"])
            ref = ev["result_ref"]
            if ref not in results:
                results.append(ref)
            if status_of[ref] != "pending":
                terminals.add(status_of[ref])
        elif kind == "client_report":
            support, calibration = judge(ev["outcome"], frozenset(terminals), complete)
            claims.append({
                "report_id": ev["id"],
                "report_seq": ev["seq"],
                "reported_outcome": ev["outcome"],
                "eligible_delivery_ids": list(delivered),
                "eligible_result_ids": list(results),
                "conflict": len(terminals) == 2,
                "definite_support": support,
                "uncertainty_calibration": calibration,
            })
    return claims


def report(raw):
    doc = parse(raw)
    schema_check(doc)
    binding_check(doc)
    return {
        "schema": "refund.c3-report.v0",
        "case_id": doc["case_id"],
        "operation_id": doc["operation_id"],
        "packet_sha256": hashlib.sha256(raw).hexdigest(),
        "source_class": doc["source_class"],
        "evidence_basis": "fixture_declared",
        "delivery_coverage": doc["delivery_coverage"],
        "claims": interpret(doc),
        "non_claims": list(NON_CLAIMS),
    }


def main(argv):
    try:
        if len(argv) != 2:
            raise Refused("io", "usage: independent_reproducer.py PACKET")
        try:
            with open(argv[1], "rb") as handle:
                raw = handle.read(MAX_BYTES + 1)
        except OSError as exc:
            raise Refused("io", "cannot read packet: %s" % exc.strerror) from None
        out = json.dumps(report(raw), sort_keys=True)
    except Refused as exc:
        sys.stderr.write("refused: %s\n" % exc)
        return 2
    sys.stdout.write(out + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
