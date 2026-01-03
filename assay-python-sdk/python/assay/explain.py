from ._native import TraceExplainer, Policy
import json

class Explainer:
    def __init__(self, policy_path: str):
        self.policy = Policy.from_file(policy_path)
        self.explainer = TraceExplainer(self.policy)

    def explain(self, trace: list) -> dict:
        """
        Explain a single trace of tool calls.

        Args:
            trace: List of tool call dicts.

        Returns:
            dict: Explanation object.
        """
        explanation_json = self.explainer.explain(trace)
        return json.loads(explanation_json)

__all__ = ["Explainer"]
