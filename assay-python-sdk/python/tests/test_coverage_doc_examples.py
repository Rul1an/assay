"""Execute the documented Coverage examples against the real installed SDK."""
from pathlib import Path
import re

import pytest
from assay import Coverage


@pytest.mark.parametrize("page", ["entry-points", "user-flows", "quick-reference"])
def test_coverage_documented_example(page, tmp_path, monkeypatch):
    root = Path(__file__).resolve().parents[3]
    path = root / "docs" / "AIcontext" / f"{page}.md"
    text = path.read_text(encoding="utf-8")
    policies = [
        block for block in re.findall(r"```yaml\n(.*?)```", text, re.S)
        if "name: coverage-example" in block
    ]
    examples = [
        block for block in re.findall(r"```python\n(.*?)```", text, re.S)
        if "from assay import" in block and "Coverage" in block
    ]
    assert len(policies) == 1, f"{path}: expected one example policy"
    assert len(examples) == 1, f"{path}: expected one Coverage example"
    monkeypatch.chdir(tmp_path)
    (tmp_path / "policy.yaml").write_text(policies[0], encoding="utf-8")

    scope = {}
    exec(compile(examples[0], str(path), "exec"), scope)
    assert isinstance(scope["coverage"], Coverage)
    report = scope["report"]
    assert isinstance(report, dict)
    assert report["meets_threshold"] is True
    assert report["overall_coverage_pct"] == 100.0

    # The same real analyzer must discriminate an uncovered allowed tool.
    uncovered = scope["coverage"].analyze([], min_coverage=80.0)
    assert uncovered["meets_threshold"] is False
    assert uncovered["overall_coverage_pct"] == 0.0


def test_user_guide_pytest_example(tmp_path, monkeypatch):
    root = Path(__file__).resolve().parents[3]
    path = root / "docs" / "guides" / "user-guide.md"
    examples = [
        block for block in re.findall(r"```python\n(.*?)```", path.read_text(encoding="utf-8"), re.S)
        if "from assay import validate" in block and "def test_agent_logic" in block
    ]
    assert len(examples) == 1, f"{path}: expected one validate pytest example"
    monkeypatch.chdir(tmp_path)
    (tmp_path / "assay.yaml").write_text(
        "version: '1'\nname: user-guide-example\ntools:\n  allow: [read_file]\n",
        encoding="utf-8",
    )

    scope = {}
    exec(compile(examples[0], str(path), "exec"), scope)
    example = scope["test_agent_logic"]
    example([{"tool": "read_file", "args": {"path": "report.txt"}}])
    # A non-empty report dict is truthy even when coverage is below the threshold.
    # Execute the documented assertion on both outcomes, not just its definition.
    with pytest.raises(AssertionError):
        example([])


def _sdk_overview():
    root = Path(__file__).resolve().parents[3]
    path = root / "docs" / "python-sdk" / "index.md"
    return path, path.read_text(encoding="utf-8")


def test_sdk_overview_imports():
    import ast

    path, text = _sdk_overview()
    imports = []
    for block in re.findall(r"```python\n(.*?)```", text, re.S):
        imports.extend(
            node for node in ast.walk(ast.parse(block))
            if isinstance(node, ast.ImportFrom) and node.module == "assay"
        )
    assert imports, "SDK overview must import the real SDK"
    for node in imports:
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), "exec"), {})


def test_sdk_overview_record_validate_coverage(tmp_path, monkeypatch):
    import json

    path, text = _sdk_overview()
    monkeypatch.chdir(tmp_path)
    (tmp_path / "policy.yaml").write_text(
        "version: '1'\nname: sdk-overview\ntools:\n  allow: [read_file]\n",
        encoding="utf-8",
    )
    scope = {}
    for heading in ["Record Traces", "Validate", "Coverage Analysis"]:
        section = text.split(f" {heading}\n", 1)[1].split("\n##", 1)[0]
        blocks = re.findall(r"```python\n(.*?)```", section, re.S)
        assert len(blocks) == 1, heading
        exec(compile(blocks[0], str(path), "exec"), scope)
        if heading == "Record Traces":
            recorded = [json.loads(line) for line in (tmp_path / "traces.jsonl").read_text().splitlines()]
            assert len(recorded) == 1
            assert recorded[0]["tool"] == "read_file"
        else:
            assert scope["report"]["meets_threshold"] is True
            assert scope["report"]["overall_coverage_pct"] == 100.0
            # Re-execute the actual example against a non-covering session.
            (tmp_path / "traces.jsonl").write_text('{"tool": "other_tool", "args": {}}\n')
            exec(compile(blocks[0], str(path), "exec"), scope)
            assert scope["report"]["meets_threshold"] is False
            assert scope["report"]["overall_coverage_pct"] == 0.0
            (tmp_path / "traces.jsonl").write_text(json.dumps(recorded[0]) + "\n")


@pytest.mark.parametrize("allowed, expected_exit", [("read_file", 0), ("other_tool", 1)])
def test_sdk_overview_pytest_plugin(tmp_path, allowed, expected_exit):
    import json
    import os
    import subprocess
    import sys

    path, text = _sdk_overview()
    section = text.split("## Pytest Plugin\n", 1)[1].split("\n## ", 1)[0]
    examples = [block for block in re.findall(r"```python\n(.*?)```", section, re.S) if "def test_" in block]
    assert len(examples) == 1, f"{path}: expected one pytest example"
    (tmp_path / "test_example.py").write_text(examples[0], encoding="utf-8")
    (tmp_path / "policy.yaml").write_text(
        f"version: '1'\nname: sdk-plugin-example\ntools:\n  allow: [{allowed}]\n",
        encoding="utf-8",
    )
    env = dict(os.environ, PYTEST_DISABLE_PLUGIN_AUTOLOAD="1")
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-p", "assay.pytest_plugin", "-q", "test_example.py"],
        cwd=tmp_path, env=env, capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == expected_exit, result.stdout + result.stderr
    traces = [json.loads(line) for line in (tmp_path / "test_traces.jsonl").read_text().splitlines()]
    assert len(traces) == 1
    assert traces[0]["tool"] == "read_file"
    if expected_exit:
        assert "AssertionError" in result.stdout
