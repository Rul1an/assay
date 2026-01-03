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
        yaml.dump(data, open(tmp.name, 'w'))
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
