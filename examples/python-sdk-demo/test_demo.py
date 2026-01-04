import pytest
from assay import Coverage

# Demo of using the assay_client fixture (auto-injected by lazy loading plugin)
# Note: For this local dev setup, we might need to manually install the plugin first.

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
    Verify coverage of the previous run (simulated here by manual trace construction
    or just checking policy loading for now).
    """
    traces = [
        [{"tool": "Search", "args": {}}, {"tool": "Summarize", "args": {}}]
    ]
    cov = Coverage("policy.yaml")
    report = cov.analyze(traces)

    print(report)
    assert report["overall_coverage_pct"] >= 50.0  # 2/2 allowed tools covered
