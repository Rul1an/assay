import os
from math import nan

import pytest
from verdict_sdk.errors import BaselineIncompatibleError
from verdict_sdk.evaluator import Evaluator


@pytest.fixture
def edge_setup(tmp_path):
    cwd = os.getcwd()
    os.chdir(tmp_path)
    yield tmp_path
    os.chdir(cwd)


def test_missing_baseline_metric_strict(edge_setup):
    """Scenario: Strict mode raises error on missing baseline metric."""
    tmp = edge_setup
    (tmp / "eval.yaml").write_text(
        """
version: 1
tests:
  - id: t1
    prompt: "Q"
    metrics:
      - name: regex_match
        kind: builtin
        params: {pattern: "Foo"}
"""
    )
    (tmp / "trace.jsonl").write_text('{"kind":"model","content":"Foo"}')

    # 1. Create Baseline with EMPTY metrics
    ev = Evaluator(strict=True)
    base_run = ev.run("trace.jsonl")
    object.__setattr__(base_run.tests[0], "metrics", [])
    ev.save_baseline("main", base_run)

    # 2. Compare (Should Error)
    with pytest.raises(BaselineIncompatibleError) as exc:
        ev.compare("trace.jsonl", baseline="main")

    assert "t1" in str(exc.value)


def test_missing_baseline_metric_lenient(edge_setup):
    """Scenario: Lenient mode passes if current metric passes threshold."""
    tmp = edge_setup
    (tmp / "eval.yaml").write_text(
        """
version: 1
tests:
  - id: t1
    prompt: "Q"
    metrics:
      - name: regex_match
        kind: builtin
        params: {pattern: "Foo"}
        threshold: 0.5
"""
    )
    # Trace that PASSES
    (tmp / "trace.jsonl").write_text('{"kind":"model","content":"Foo"}')

    ev = Evaluator(strict=False)  # Lenient
    base_run = ev.run("trace.jsonl")
    object.__setattr__(base_run.tests[0], "metrics", [])
    ev.save_baseline("main", base_run)

    comp = ev.compare("trace.jsonl", baseline="main")

    if not comp.passed:
        # Should NOT happen now because m1 -> regex_match is known
        print(f"DEBUG FAIL: summary={comp.summary} regressions={comp.regressions}")

    assert comp.passed is True
    assert len(comp.regressions) == 0


def test_missing_baseline_metric_lenient_fail(edge_setup):
    """Scenario: Lenient mode FAILS if current metric violates threshold."""
    tmp = edge_setup
    (tmp / "eval.yaml").write_text(
        """
version: 1
tests:
  - id: t1
    prompt: "Q"
    metrics:
      - name: regex_match
        kind: builtin
        params: {pattern: "Bar"} # Won't match Foo
        threshold: 0.5
"""
    )
    (tmp / "trace.jsonl").write_text('{"kind":"model","content":"Foo"}')

    ev = Evaluator(strict=False)
    base_run = ev.run("trace.jsonl")
    object.__setattr__(base_run.tests[0], "metrics", [])
    ev.save_baseline("main", base_run)

    comp = ev.compare("trace.jsonl", baseline="main")

    assert comp.passed is False
    # Should be regression due to threshold failure?
    assert len(comp.regressions) == 1
    # Threshold checks run even without baseline
    assert comp.regressions[0].threshold == 0.5
