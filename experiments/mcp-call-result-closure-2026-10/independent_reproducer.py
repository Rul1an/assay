"""Independent finite-cohort recount from retained native bytes, not an MCP validator.

No producer/Assay imports. For these frozen fixtures only, use indexed request intervals,
not the adapter's outstanding map, normalized blobs, or checker. Unsupported extensions
raise rather than borrowing a producer verdict.
"""

import hashlib
import json
import sys
from pathlib import Path


def recount(p):
    def result(outcome, reason):
        return [outcome, reason]

    invalid = result("no_claim", "adapter_input_invalid")
    if hashlib.sha256(p["trace_utf8"].encode()).hexdigest() != p["trace_sha256"]:
        return result("no_claim", "native_binding_mismatch")
    if not p["selection"]:
        return invalid
    selectors = [
        (type(s["id"]).__name__, str(s["id"]), s["attempt"]) for s in p["selection"]
    ]
    if any(type(s["id"]) not in (int, str) for s in p["selection"]) or len(
        set(selectors)
    ) != len(selectors):
        return invalid
    events = list(enumerate(map(json.loads, p["trace_utf8"].splitlines()), 1))
    requests = [(i, e) for i, e in events if e.get("method") == "tools/call"]
    responses = [(i, e) for i, e in events if "result" in e or "error" in e]

    def key(e):
        return type(e["id"]).__name__, str(e["id"])

    # Each response must have exactly one still-unanswered preceding request.
    assignments = {}
    for pos, response in responses:
        prior = [
            (i, e)
            for i, e in requests
            if i < pos and key(e) == key(response) and i not in assignments
        ]
        if len(prior) > 1:
            return result("no_claim", "native_parse_refused")
        if not prior:
            return invalid
        assignments[prior[0][0]] = (pos, response)
    for pos, request in requests:
        earlier = [i for i, e in requests if i < pos and key(e) == key(request)]
        if any(i not in assignments or assignments[i][0] > pos for i in earlier):
            return result("no_claim", "native_parse_refused")
    request_by_pos = dict(requests)
    for pos, (_, response) in assignments.items():
        if "result" not in response:
            continue
        value = response["result"]
        legacy = (
            request_by_pos[pos]["params"]["_meta"][
                "io.modelcontextprotocol/protocolVersion"
            ]
            < "2026-07-28"
        )
        if type(value.get("resultType")) is not str and not (
            "resultType" not in value and legacy
        ):
            return result("no_claim", "native_result_refused")
    selected = []
    for typ, value, attempt in selectors:
        matching = [(i, e) for i, e in requests if key(e) == (typ, value)]
        if attempt < 1 or attempt > len(matching):
            return invalid
        selected.append(matching[attempt - 1][0])
    if p["source_class"] != "local-supervisor":
        return result("not_established", "source_unaccepted")
    if not p["continuity"] or p["coverage_through_seq"] < p["cutoff_seq"]:
        return result("not_established", "coverage_incomplete")
    for pos in selected:
        response = assignments.get(pos)
        if not response or response[0] > p["cutoff_seq"]:
            return result("not_established", "terminal_not_observed")
        value = response[1].get("result", {})
        legacy = (
            request_by_pos[pos]["params"]["_meta"][
                "io.modelcontextprotocol/protocolVersion"
            ]
            < "2026-07-28"
        )
        terminal = value.get("resultType") == "complete" or (
            "result" in response[1] and "resultType" not in value and legacy
        )
        if not terminal or any(
            value.get(k) is not None for k in ("requestState", "inputRequests")
        ):
            return result("not_established", "terminal_not_observed")
    return result("supported", "all_selected_terminal")


def main():
    root = Path(__file__).resolve().parent
    recorded = json.loads(Path(sys.argv[1]).read_bytes())
    by_id = {row["case"]: row for row in recorded["results"]}
    fixtures = sorted((root / "fixtures").glob("*.json"))
    assert set(by_id) == {p.stem for p in fixtures}
    for path in fixtures:
        row = by_id[path.stem]
        assert hashlib.sha256(path.read_bytes()).hexdigest() == row["fixture_sha256"]
        case = json.loads(path.read_bytes())
        derived = recount(case["input"])
        assert derived == case["expected"], (path, derived)
        assert derived == [row["result"]["outcome"], row["result"]["reason"]], path
    print(f"{len(fixtures)} outcomes independently re-derived from native bytes")


if __name__ == "__main__":
    main()
