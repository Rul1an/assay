import os
from pathlib import Path

import pytest
from verdict_sdk.config_loader import load_config
from verdict_sdk.errors import ConfigError, RegressionError
from verdict_sdk.evaluator import Evaluator
from verdict_sdk.result import CompareResult, Regression, ResultArtifacts

# --- PHASE 0 TESTS (Config & Loader) ---


def test_config_loader_sdk_native(tmp_path):
    config_file = tmp_path / "eval.yaml"
    config_file.write_text(
        """
version: 1
suite: sdk_suite
tests:
  - id: t1
    prompt: "Prompt"
    metrics:
      - name: faithfulness
        kind: judge
        threshold: 0.8
"""
    )
    cfg = load_config(config_file)
    assert cfg.suite == "sdk_suite"
    assert len(cfg.tests) == 1
    t = cfg.tests[0]
    assert t.id == "t1"
    assert t.metrics[0].name == "faithfulness"


def test_config_loader_cli_compat(tmp_path):
    # Test normalization of legacy verdict.yaml
    config_file = tmp_path / "verdict.yaml"
    config_file.write_text(
        """
version: 1
suite: cli_suite
tests:
  - id: t_legacy
    input:
      prompt: "Legacy?"
    expected:
      type: regex_match
      pattern: "Yes"
    assertions:
      - type: trace_must_call_tool
        tool: Search
"""
    )
    cfg = load_config(config_file)
    assert cfg.suite == "cli_suite"
    t = cfg.tests[0]
    assert t.id == "t_legacy"
    # Should be mapped to 2 metrics
    assert len(t.metrics) == 2
    # Check regex_match mapping
    m1 = next(m for m in t.metrics if m.name == "regex_match")
    assert m1.kind == "builtin"
    assert m1.params["pattern"] == "Yes"
    # Check assertion mapping
    m2 = next(m for m in t.metrics if m.name == "trace_must_call_tool")
    assert m2.params["tool"] == "Search"


def test_evaluator_constructor_defaults(tmp_path):
    # Evaluator() should find eval.yaml in cwd
    old_cwd = os.getcwd()
    os.chdir(tmp_path)
    try:
        (tmp_path / "eval.yaml").write_text("version: 1\ntests: [{id: t1, prompt: p}]")
        ev = Evaluator()
        assert ev.config.tests[0].id == "t1"
    finally:
        os.chdir(old_cwd)


# --- PHASE 1 TESTS (Results) ---


def test_compare_result_logic():
    artifacts = ResultArtifacts(
        Path("run.json"), Path("r.jsonl"), None, None, None, Path("t.jsonl")
    )

    # Passing result
    res_pass = CompareResult(
        passed=True,
        exit_code=0,
        summary="OK",
        baseline="main",
        baseline_run_id="b",
        current_run_id="c",
        tests=[],
        regressions=[],
        artifacts=artifacts,
    )
    assert bool(res_pass) is True
    res_pass.raise_for_status()  # Should not raise

    # Regression result
    res_fail = CompareResult(
        passed=False,
        exit_code=1,
        summary="Regression",
        baseline="main",
        baseline_run_id="b",
        current_run_id="c",
        tests=[],
        regressions=[Regression("t1", "m1", 0.9, 0.5, -0.4)],
        artifacts=artifacts,
    )
    assert bool(res_fail) is False
    with pytest.raises(RegressionError):
        res_fail.raise_for_status()

    # Markdown output check
    md = res_fail.to_github_summary()
    assert "❌ FAIL" in md
    assert "0.900 → 0.500" in md
