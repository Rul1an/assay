from typing import List, Dict, Any
from ._native import Policy, TraceExplainer

class Explainer:
    policy: Policy
    explainer: TraceExplainer
    def __init__(self, policy_path: str) -> None: ...
    def explain(self, trace: List[Any]) -> Dict[str, Any]: ...
