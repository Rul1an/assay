from .client import AssayClient
from .coverage import Coverage
from .explain import Explainer

__all__ = ["AssayClient", "Coverage", "Explainer", "validate"]

def validate(policy_path: str, traces: list) -> dict:
    """
    Validate a list of traces against a policy file.

    This is a stateless check.

    Args:
        policy_path: Path to the assay.yaml policy file.
        traces: List of trace dictionaries.

    Returns:
        dict: Coverage report.
    """
    cov = Coverage(policy_path)
    # DX: Auto-wrap single session (list of dicts) into list of sessions
    if isinstance(traces, list) and traces and isinstance(traces[0], dict):
        traces = [traces]

    return cov.analyze(traces)

