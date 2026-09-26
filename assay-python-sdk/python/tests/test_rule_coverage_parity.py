"""#3166/#3181 parity (Python leg): the same rule_parity fixture as
`crates/assay-cli/tests/coverage_rule_parity.rs`, through the real PyO3 SDK.

Trace A is [Search, Create, Notify], trace B is [Search]. The union triggers
seven of the eight rules; only `never_after_read_forbidden_post` (whose
trigger `Read` never appears) stays untriggered. Before #3166 the SDK counted
evaluated rules and read 7/8 with the wrong rule untriggered; the evaluator
says 5/8 on B alone.
"""
import json
import os

from assay import Coverage

FIXTURE_DIR = os.path.join(
    os.path.dirname(__file__),
    "..",
    "..",
    "..",
    "crates",
    "assay-cli",
    "tests",
    "fixtures",
    "coverage",
    "rule_parity",
)


def load_fixture_traces():
    """Group the fixture JSONL into the SDK's list-of-lists shape."""
    groups = {}
    with open(os.path.join(FIXTURE_DIR, "traces.jsonl")) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            event = json.loads(line)
            groups.setdefault(event["test_id"], []).append(
                {"tool": event["tool"], "args": {}}
            )
    # A before B: file order is already grouped, dict preserves insertion.
    return list(groups.values())


def test_rule_coverage_parity_with_rust_legs():
    assert sorted(os.listdir(FIXTURE_DIR)) == ["policy.yaml", "traces.jsonl"]

    cov = Coverage(os.path.join(FIXTURE_DIR, "policy.yaml"))
    traces = load_fixture_traces()
    assert [[c["tool"] for c in t] for t in traces] == [
        ["Search", "Create", "Notify"],
        ["Search"],
    ]

    report = cov.analyze(traces, min_coverage=80.0)

    rule = report["rule_coverage"]
    assert rule["total_rules"] == 8
    assert rule["rules_triggered"] == 7
    assert rule["coverage_pct"] == 87.5
    assert rule["untriggered_rules"] == ["never_after_read_forbidden_post"]

    tool = report["tool_coverage"]
    assert tool["total_tools_in_policy"] == 6
    assert tool["tools_seen_in_traces"] == 3
    assert tool["coverage_pct"] == 50.0
