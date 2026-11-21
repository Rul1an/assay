import pytest
import tempfile
import os
import yaml
from assay import Coverage, Explainer

@pytest.fixture
def policy_file():
    data = {
        "version": "1",
        "name": "test_policy",
        "tools": {
            "allow": ["ToolA", "ToolB"]
        }
    }
    with tempfile.NamedTemporaryFile(suffix=".yaml", delete=False) as tmp:
        with open(tmp.name, 'w') as f:
            yaml.dump(data, f)
        path = tmp.name
    yield path
    if os.path.exists(path):
        os.remove(path)

def test_coverage_wrapper(policy_file):
    cov = Coverage(policy_file)

    traces = [
        [{"tool": "ToolA", "args": {}}],
        [{"tool": "ToolB", "args": {}}]
    ]

    report = cov.analyze(traces, min_coverage=80.0)
    assert report["overall_coverage_pct"] == 100.0
    assert report["meets_threshold"] is True

def test_explainer_wrapper(policy_file):
    exp = Explainer(policy_file)

    trace = [{"tool": "ToolA", "args": {}}]

    explanation = exp.explain(trace)
    assert len(explanation["steps"]) == 1
    assert explanation["steps"][0]["tool"] == "ToolA"


@pytest.mark.parametrize("wrapper", [Coverage, Explainer])
def test_policy_file_admission(wrapper, tmp_path):
    path = tmp_path / "policy.yaml"
    with pytest.raises(FileNotFoundError):
        wrapper(str(path))

    source = b'version: "1"\n#'
    path.write_bytes(source.ljust(1_000_000, b"x"))
    wrapper(str(path))
    with path.open("ab") as handle:
        handle.write(b"x")
    with pytest.raises(ValueError, match="1000000"):
        wrapper(str(path))

    path.write_text('version: "1"\ntools: {allowed: [read]}\n')
    with pytest.raises(ValueError, match=r"tools\.allowed"):
        wrapper(str(path))


def test_policy_aliases_keep_distinct_quoted_names(tmp_path):
    path = tmp_path / "policy.yaml"
    path.write_text('version: "1"\naliases: {0x10: [read], 16: [write]}\n')
    with pytest.raises(ValueError, match="duplicate"):
        Coverage(str(path))

    path.write_text('version: "1"\naliases: {"0x10": [read], "16": [write]}\n')
    report = Coverage(str(path)).analyze([[{"tool": "read"}, {"tool": "write"}]])
    assert report["tool_coverage"]["total_tools_in_policy"] == 4
    assert report["tool_coverage"]["unexpected_tools"] == []
