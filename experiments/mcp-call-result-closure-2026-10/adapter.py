"""Experimental adapter: Assay-declared MCP call-result closure, never physical effects."""

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

from closure_check import check, strict_json

LIMIT = 65536
FIELDS = {
    "session",
    "cutoff_seq",
    "selection",
    "source_class",
    "continuity",
    "coverage_through_seq",
    "trace_utf8",
    "trace_sha256",
}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def typed(value):
    if type(value) is int and -(2**63) <= value <= 2**64 - 1:
        return ["num", str(value)]
    if type(value) is str and value:
        return ["str", value]
    raise ValueError("unusable selection id")


def action(key):
    return json.dumps(key, ensure_ascii=True, separators=(",", ":"))


def evaluate(packet, parser):
    native = []

    def refusal(reason):
        return {
            "outcome": "no_claim",
            "reason": reason,
            "native": native,
            "claim_scope": "assay-declared-mcp-call-result",
        }

    try:
        if not isinstance(packet, dict) or set(packet) != FIELDS:
            raise ValueError("shape")
        for key in ("session", "source_class"):
            if not isinstance(packet[key], str) or not packet[key]:
                raise ValueError("text")
        for key, minimum in (("cutoff_seq", 1), ("coverage_through_seq", 0)):
            if type(packet[key]) is not int or packet[key] < minimum:
                raise ValueError("integer")
        if type(packet["continuity"]) is not bool:
            raise ValueError("continuity")
        raw = packet["trace_utf8"].encode("utf-8")
        if len(raw) > LIMIT or not raw or not raw.endswith(b"\n"):
            raise ValueError("trace size/framing")
        if any(not line.strip() for line in raw.split(b"\n")[:-1]):
            raise ValueError("blank row")
        if sha(raw) != packet["trace_sha256"]:
            return refusal("native_binding_mismatch")
        selected = []
        if not isinstance(packet["selection"], list) or not packet["selection"]:
            raise ValueError("empty selection")
        for item in packet["selection"]:
            if (
                set(item) != {"id", "attempt"}
                or type(item["attempt"]) is not int
                or item["attempt"] < 1
            ):
                raise ValueError("selector")
            selected.append(
                {"action_id": action(typed(item["id"])), "attempt": item["attempt"]}
            )
        if len({(x["action_id"], x["attempt"]) for x in selected}) != len(selected):
            raise ValueError("duplicate selection")
        binary_hash = sha(Path(parser).read_bytes())
        if binary_hash != os.environ["CLOSURE_PARSER_SHA256"]:
            return refusal("parser_binding_mismatch")
        run = subprocess.run(
            [parser],
            input=raw,
            capture_output=True,
            timeout=5,
            check=True,
        )
        parsed = strict_json(run.stdout)
        if "error" in parsed:
            return refusal("native_parse_refused")
        native = parsed["rows"]
        rows, blobs, attempts, outstanding = [], {}, {}, {}
        session = packet["session"]
        source_lines = raw.splitlines(keepends=True)
        for event in native:
            line = source_lines[event["seq"] - 1]
            event["source_line_utf8"] = line.decode("utf-8")
            event["line_sha256"] = sha(line)
            key = action(event["id"])
            if event["id"] is None:
                raise ValueError("no typed key")
            seq = event["seq"]
            if event["kind"] == "request":
                if event["request"] != "Valid":
                    return refusal("native_result_refused")
                attempts[key] = attempts.get(key, 0) + 1
                outstanding[key] = attempts[key]
                rows.append(
                    {
                        "seq": seq,
                        "session": session,
                        "action_id": key,
                        "attempt": attempts[key],
                        "kind": "register",
                    }
                )
            else:
                if key not in outstanding:
                    raise ValueError("orphan response")
                attempt = outstanding.pop(key)
                conclusion = event["conclusion"]
                if conclusion and conclusion.startswith("Invalid"):
                    return refusal("native_result_refused")
                # Only the reused Assay result conclusion establishes protocol-result closure.
                # NonTerminal, Incomplete and JSON-RPC errors do not establish physical pending.
                if conclusion == "Terminal":
                    blob = {
                        "seq": seq,
                        "session": session,
                        "action_id": key,
                        "attempt": attempt,
                        "state": "completed",
                    }
                    data = json.dumps(blob, separators=(",", ":"))
                    digest = sha(data.encode())
                    blobs[digest] = data
                    rows.append(dict(blob, kind="observation", evidence_sha256=digest))
        registered = {
            (x["action_id"], x["attempt"]) for x in rows if x["kind"] == "register"
        }
        if any((x["action_id"], x["attempt"]) not in registered for x in selected):
            raise ValueError("missing selection")
        trace = "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows)
        normalized = {
            "profile": "registered-action-closure.v0",
            "claim": {
                "kind": "all_selected_actions_terminal",
                "session": session,
                "cutoff_seq": packet["cutoff_seq"],
                "selection": selected,
            },
            "context": {
                "session": session,
                "trace_sha256": sha(trace.encode()),
                "source_class": packet["source_class"],
                "continuity": packet["continuity"],
                "coverage_through_seq": packet["coverage_through_seq"],
                "allowed_channels": ["native-jsonrpc"],
                "observed_channels": ["native-jsonrpc"],
            },
            "trace_utf8": trace,
            "blobs": blobs,
        }
        result = check(json.dumps(normalized).encode())
        result.update(
            native=native,
            normalized=normalized,
            source_sha256=sha(raw),
            parser_sha256=binary_hash,
            claim_scope="assay-declared-mcp-call-result",
        )
        return result
    except (ValueError, TypeError, KeyError, AttributeError, UnicodeError):
        return refusal("adapter_input_invalid")


def main():
    packet = strict_json(Path(sys.argv[1]).read_bytes())
    print(
        json.dumps(
            evaluate(packet["input"], os.environ["CLOSURE_PARSER"]), sort_keys=True
        )
    )


if __name__ == "__main__":
    main()
