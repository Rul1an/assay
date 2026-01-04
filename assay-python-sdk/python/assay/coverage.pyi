from typing import List, Dict, Any, Optional
from ._native import Policy, CoverageAnalyzer

class Coverage:
    policy: Policy
    analyzer: CoverageAnalyzer
    def __init__(self, policy_path: str) -> None: ...
    def analyze(self, traces: List[Any], min_coverage: float = 80.0) -> Dict[str, Any]: ...
