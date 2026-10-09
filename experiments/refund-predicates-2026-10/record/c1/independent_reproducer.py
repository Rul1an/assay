"""Independent reproducer for the synthetic C1 packet contract v0 (CONTRACT.md).

Derived from CONTRACT.md and the frozen fixtures/expectations only. It imports
no reader, producer, Assay or Corpus Adequacy code and does not use the json
module: decoding is a hand-written byte-level scanner, so duplicate keys,
nonfinite literals, depth and trailing data are rejected by construction
rather than by parser hooks. Supplementary parse convention (CONTRACT-ADDENDUM):
fraction/exponent tokens must be finite binary64; integer tokens are exact and
limited to 4300 decimal digits; protected money fields stay strict integers.

Usage: python3 independent_reproducer.py PACKET
Exit 0 with one JSON report on stdout; exit 2 with "refused: ..." on stderr.
"""
import hashlib
import sys

LIMIT = 1048576
MAX_DEPTH = 32
MAX_INT_DIGITS = 4300
DIGITS = "0123456789"
HEX_LOWER = DIGITS + "abcdef"
ID_CHARS = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz" + DIGITS + "_.:-")
PROTECTED = ("merchant", "payment_id", "amount", "exponent", "currency", "action")
APPROVED_PROFILE = (("currency", "EUR"), ("exponent", 2), ("action", "refund"))
NON_CLAIMS = (
    "no-live-execution",
    "no-runtime-coverage-proof",
    "no-issuer-authentication",
    "approval-digest-not-verified-against-content",
    "no-C2-or-C3",
    "no-adequacy-score",
)


class Refused(Exception):
    pass


def integer_from_digits(text, fail):
    """Exact integer from a grammar-checked token, bounded at MAX_INT_DIGITS.

    Accumulates in 9-digit chunks so the result never depends on the host's
    int/str conversion limit (sys.set_int_max_str_digits, PYTHONINTMAXSTRDIGITS).
    """
    negative = text.startswith("-")
    digits = text[1:] if negative else text
    if len(digits) > MAX_INT_DIGITS:
        fail("integer token above %d digits" % MAX_INT_DIGITS)
    value = 0
    for at in range(0, len(digits), 9):
        chunk = digits[at:at + 9]
        part = 0
        for d in chunk:
            part = part * 10 + DIGITS.index(d)
        value = value * 10 ** len(chunk) + part
    return -value if negative else value


class Scanner:
    def __init__(self, text):
        self.s = text
        self.i = 0

    def fail(self, why):
        raise Refused("%s at offset %d" % (why, self.i))

    def skip_ws(self):
        while self.i < len(self.s) and self.s[self.i] in " \t\n\r":
            self.i += 1

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ""

    def document(self):
        self.skip_ws()
        value = self.value(0)
        self.skip_ws()
        if self.i != len(self.s):
            self.fail("trailing data")
        return value

    def value(self, depth):
        c = self.peek()
        if c == "{":
            return self.obj(depth + 1)
        if c == "[":
            return self.arr(depth + 1)
        if c == '"':
            return self.string()
        if c == "-" or (c and c in DIGITS):
            return self.number()
        for word, result in (("true", True), ("false", False), ("null", None)):
            if self.s.startswith(word, self.i):
                self.i += len(word)
                return result
        self.fail("unexpected token")

    def enter(self, depth):
        if depth > MAX_DEPTH:
            self.fail("container depth above %d" % MAX_DEPTH)
        self.i += 1
        self.skip_ws()

    def obj(self, depth):
        self.enter(depth)
        members = {}
        if self.peek() == "}":
            self.i += 1
            return members
        while True:
            if self.peek() != '"':
                self.fail("object key expected")
            key = self.string()
            if key in members:
                self.fail("duplicate key %r" % key)
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
        self.enter(depth)
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
                self.fail("raw control character in string")
            if c != "\\":
                out.append(c)
                continue
            e = self.peek()
            self.i += 1
            if e in simple:
                out.append(simple[e])
            elif e == "u":
                unit = self.hex4()
                # Join an escaped surrogate pair into one scalar so that its key
                # compares equal to the literal character (duplicate detection).
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

    def hex4(self):
        code = self.s[self.i:self.i + 4]
        if len(code) != 4 or any(h not in "0123456789abcdefABCDEF" for h in code):
            self.fail("bad unicode escape")
        self.i += 4
        unit = 0
        for h in code:
            unit = unit * 16 + "0123456789abcdef".index(h.lower())
        return unit

    def number(self):
        start = self.i
        if self.peek() == "-":
            self.i += 1
        if self.peek() == "0":
            self.i += 1
        elif self.peek() and self.peek() in "123456789":
            while self.peek() and self.peek() in DIGITS:
                self.i += 1
        else:
            self.fail("bad number")
        integral = True
        if self.peek() == ".":
            integral = False
            self.i += 1
            self.digits_required()
        if self.peek() and self.peek() in "eE":
            integral = False
            self.i += 1
            if self.peek() and self.peek() in "+-":
                self.i += 1
            self.digits_required()
        text = self.s[start:self.i]
        if integral:
            return integer_from_digits(text, self.fail)
        value = float(text)
        if value != value or value in (float("inf"), float("-inf")):
            self.fail("number outside finite binary64")
        return value

    def digits_required(self):
        if not (self.peek() and self.peek() in DIGITS):
            self.fail("digit expected")
        while self.peek() and self.peek() in DIGITS:
            self.i += 1


def decode(raw):
    if len(raw) > LIMIT:
        raise Refused("input above %d bytes" % LIMIT)
    if raw[:3] == b"\xef\xbb\xbf":
        raise Refused("byte order mark")
    try:
        text = raw.decode("utf-8", errors="strict")
    except UnicodeDecodeError as exc:
        raise Refused("invalid UTF-8 at byte %d" % exc.start) from None
    return Scanner(text).document()


# Shape checks: each predicate returns True for a well-formed value.

def is_identifier(v):
    return type(v) is str and 1 <= len(v) <= 128 and all(c in ID_CHARS for c in v)


def is_digest(v):
    return type(v) is str and len(v) == 64 and all(c in HEX_LOWER for c in v)


def is_currency(v):
    return type(v) is str and len(v) == 3 and all("A" <= c <= "Z" for c in v)


def is_int_in(v, low, high):
    return type(v) is int and low <= v <= high


FIELD_SHAPE = {
    "merchant": is_identifier,
    "payment_id": is_identifier,
    "action": is_identifier,
    "currency": is_currency,
    "amount": lambda v: is_int_in(v, 1, 2 ** 63 - 1),
    "exponent": lambda v: is_int_in(v, 0, 9),
}


def require(condition, why):
    if not condition:
        raise Refused(why)


def exact_keys(obj, keys, where):
    require(type(obj) is dict, where + " must be an object")
    require(set(obj) == set(keys), where + " keys differ from the closed set")


def validate(doc):
    exact_keys(doc, ("schema", "case_id", "source_class", "coverage", "target_dispatch",
                     "approval", "dispatch"), "packet")
    require(doc["schema"] == "refund.synthetic-c1.v0", "schema")
    require(is_identifier(doc["case_id"]), "case_id")
    require(doc["source_class"] == "synthetic_fixture", "source_class")
    require(doc["coverage"] in ("complete", "incomplete"), "coverage")
    target = doc["target_dispatch"]
    require(target is None or is_identifier(target), "target_dispatch")

    approval = doc["approval"]
    exact_keys(approval, ("digest", "action"), "approval")
    require(is_digest(approval["digest"]), "approval.digest")
    exact_keys(approval["action"], PROTECTED, "approval.action")
    for name in PROTECTED:
        require(FIELD_SHAPE[name](approval["action"][name]), "approval.action." + name)
    for name, fixed in APPROVED_PROFILE:
        require(approval["action"][name] == fixed, "approval.action profile " + name)

    dispatch = doc["dispatch"]
    if dispatch is None:
        return
    exact_keys(dispatch, ("id", "approval_digest", "action"), "dispatch")
    require(is_identifier(dispatch["id"]), "dispatch.id")
    require(is_digest(dispatch["approval_digest"]), "dispatch.approval_digest")
    action = dispatch["action"]
    require(type(action) is dict, "dispatch.action must be an object")
    for name in PROTECTED:
        if name in action:
            require(FIELD_SHAPE[name](action[name]), "dispatch.action." + name)
    require(target is not None and target == dispatch["id"], "target_dispatch inconsistent with dispatch.id")


def judge(doc):
    """Return ((applicable, result, reason), (nfd_result, nfd_reason))."""
    dispatch = doc["dispatch"]
    complete = doc["coverage"] == "complete"
    if dispatch is None:
        if doc["target_dispatch"] is not None:
            return (True, "NOT_ESTABLISHED", "missing_dispatch_evidence"), ("NOT_ESTABLISHED", "unresolved_dispatch")
        c1 = (False, None, "no_recorded_dispatch")
    else:
        seen = set(dispatch["action"])
        expected = set(PROTECTED)
        approved = doc["approval"]["action"]
        if seen - expected:
            c1 = (True, "NOT_ESTABLISHED", "unknown_action_fields")
        elif expected - seen:
            c1 = (True, "NOT_ESTABLISHED", "missing_action_fields")
        elif dispatch["approval_digest"] != doc["approval"]["digest"]:
            c1 = (True, "CONTRADICTED", "approval_mismatch")
        elif [dispatch["action"][n] for n in PROTECTED] != [approved[n] for n in PROTECTED]:
            c1 = (True, "CONTRADICTED", "field_mismatch")
        else:
            c1 = (True, "ESTABLISHED", "exact_match")
        if c1[1] == "CONTRADICTED":
            return c1, ("CONTRADICTED", "forbidden_dispatch")
        if c1[1] == "NOT_ESTABLISHED":
            return c1, ("NOT_ESTABLISHED", "unresolved_dispatch")
    if complete:
        return c1, ("ESTABLISHED", "complete_fixture_scope")
    return c1, ("NOT_ESTABLISHED", "incomplete_coverage")


def encode(value):
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if type(value) is str:
        body = []
        for c in value:
            if c in '"\\' or ord(c) < 0x20 or ord(c) > 0x7e:
                body.append("\\u%04x" % ord(c) if ord(c) <= 0xffff else c)
            else:
                body.append(c)
        return '"' + "".join(body) + '"'
    if type(value) in (list, tuple):
        return "[" + ", ".join(encode(v) for v in value) + "]"
    if type(value) is dict:
        return "{" + ", ".join(encode(k) + ": " + encode(value[k]) for k in sorted(value)) + "}"
    raise TypeError(type(value))


def report(raw):
    doc = decode(raw)
    validate(doc)
    (applicable, result, reason), (nfd_result, nfd_reason) = judge(doc)
    return {
        "schema": "refund.c1-report.v0",
        "case_id": doc["case_id"],
        "packet_sha256": hashlib.sha256(raw).hexdigest(),
        "source_class": doc["source_class"],
        "evidence_basis": "fixture_declared",
        "c1": {"applicable": applicable, "result": result, "reason": reason},
        "no_forbidden_dispatch": {"result": nfd_result, "reason": nfd_reason},
        "non_claims": list(NON_CLAIMS),
    }


def main(argv):
    try:
        if len(argv) != 2:
            raise Refused("usage: independent_reproducer.py PACKET")
        try:
            with open(argv[1], "rb") as handle:
                raw = handle.read(LIMIT + 1)
        except OSError as exc:
            raise Refused("cannot read packet: %s" % exc.strerror) from None
        out = encode(report(raw))
    except Refused as exc:
        sys.stderr.write("refused: %s\n" % exc)
        return 2
    sys.stdout.write(out + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
