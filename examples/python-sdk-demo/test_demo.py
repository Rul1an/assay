import pytest
from assay import Coverage

# Demo of using the assay_client fixture (auto-registered via pytest plugin entry point)
# Note: Installing this package via "pip install ." from the project root will also install and register the pytest plugin for this demo.

@pytest.mark.assay(trace_file="demo_trace.jsonl")
def test_search_and_summarize(assay_client):
    """
    Simulate an agent searching and summarizing.
    """
    print("Agent: Searching for apples")
    assay_client.record_trace({
        "tool": "Search",
        "args": {"query": "apples"}
    })

    print("Agent: Summarizing results")
    assay_client.record_trace({
        "tool": "Summarize",
        "args": {"text": "Apples are red."}
    })

def test_coverage():
    """
    Verify coverage of the previous run.
    """
    traces = [
        [{"tool": "Search", "args": {}}, {"tool": "Summarize", "args": {}}]
    ]
    cov = Coverage("policy.yaml")
    report = cov.analyze(traces)

    print(report)
    assert report["overall_coverage_pct"] >= 100.0  # 2/2 allowed tools covered (100%)
