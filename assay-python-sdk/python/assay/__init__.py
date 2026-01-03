from ._native import Policy, CoverageAnalyzer
import json

__all__ = ["Policy", "CoverageAnalyzer", "analyze_coverage"]

def analyze_coverage(policy_path: str, traces: list, threshold: float = 80.0) -> dict:
    """
    High-level helper to analyze coverage.

    Args:
        policy_path: Path to assay policy file (.yaml)
        traces: List of traces (each trace is a list of tool call dicts)
        threshold: Minimum coverage percentage (default 80.0)

    Returns:
        dict: Coverage report object
    """
    policy = Policy.from_file(policy_path)
    analyzer = CoverageAnalyzer(policy)
    report_json = analyzer.analyze(traces, threshold)
    return json.loads(report_json)
