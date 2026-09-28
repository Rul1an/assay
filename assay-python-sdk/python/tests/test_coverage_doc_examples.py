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
