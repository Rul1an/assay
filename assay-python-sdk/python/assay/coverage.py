from ._native import CoverageAnalyzer, Policy
import json

class Coverage:
    """
    Analyzes policy coverage for a set of traces.

    This class loads an Assay policy and checks if provided traces satisfy
    the policy's rules. It can calculate coverage percentages and identify
    violations.

    Attributes:
        policy_path (str): The file path to the policy (e.g., 'assay.yaml').
    """
    def __init__(self, policy_path: str):
        """
        Initialize the Coverage analyzer.

        Args:
            policy_path (str): Path to the assay policy YAML file.

        Raises:
            FileNotFoundError: If policy_path does not exist.
            ValueError: If the policy file is invalid.
        """
        self.policy = Policy.from_file(policy_path)
        self.analyzer = CoverageAnalyzer(self.policy)

    def analyze(self, traces: list, min_coverage: float = 80.0) -> dict:
        """
        Analyze coverage for a list of traces and check against a minimum threshold.

        This method processes a list of tool execution traces, matches them against
        the loaded policy, and produces a detailed report including pass/fail status,
        coverage percentage, and specific rule violations.

        Args:
            traces (list): A list of traces. Each trace is a list of tool call dictionaries.
                           Example: `[[{"tool": "search", "args": {"q": "foo"}}], ...]`
            min_coverage (float, optional): The minimum pass percentage. Defaults to 80.0.

        Returns:
            dict: A detailed coverage report dictionary containing:
                  - `tool_coverage` (dict): Tool coverage metrics.
                  - `rule_coverage` (dict): Rule coverage metrics.
                  - `high_risk_gaps` (list): Blocklisted tools never seen in traces.
                  - `policy_violations` (list): Policy violations found during analysis.
                  - `policy_warnings` (list): Policy warnings (for example unconstrained tools).
                  - `overall_coverage_pct` (float): Overall coverage percentage.
                  - `meets_threshold` (bool): Whether coverage met the threshold.
                  - `threshold` (float): The threshold that was checked.
        """
        report_json = self.analyzer.analyze(traces, min_coverage)
        return json.loads(report_json)

__all__ = ["Coverage"]
