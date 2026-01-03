from ._native import CoverageAnalyzer, Policy
import json

class Coverage:
    def __init__(self, policy_path: str):
        self.policy = Policy.from_file(policy_path)
        self.analyzer = CoverageAnalyzer(self.policy)

    def analyze(self, traces: list, min_coverage: float = 80.0) -> dict:
        """
        Analyze coverage for a list of traces.

        Args:
            traces: List of traces (list of tool call dicts).
            min_coverage: Minimum coverage percentage (default 80.0).

        Returns:
            dict: Coverage report.
        """
        report_json = self.analyzer.analyze(traces, min_coverage)
        return json.loads(report_json)

__all__ = ["Coverage"]
