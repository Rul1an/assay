from typing import Any, Dict, List

from .client import AssayClient
from .coverage import Coverage
from .explain import Explainer

__all__ = ["AssayClient", "Coverage", "Explainer", "validate"]


def validate(policy_path: str, traces: List[Any]) -> Dict[str, Any]: ...
