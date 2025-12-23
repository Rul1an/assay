from __future__ import annotations

from typing import List


class VerdictError(Exception):
    """Base class for all Verdict SDK errors."""


class ConfigError(VerdictError):
    """Configuration file is invalid."""


class BaselineNotFoundError(VerdictError):
    def __init__(self, name: str, available: list[str]):
        super().__init__(f"Baseline '{name}' not found. Available: {available}")
        self.name = name


class BaselineIncompatibleError(VerdictError):
    def __init__(self, baseline: str, missing: list[tuple[str, str]]):
        missing_str = str(missing[:5]) + ("..." if len(missing) > 5 else "")
        super().__init__(
            f"Baseline '{baseline}' is incompatible with current config. "
            f"Missing metrics in baseline: {missing_str}. "
            "Re-create baseline via save_baseline()."
        )
        self.baseline = baseline
        self.missing = missing


class PromptIntegrityError(VerdictError):
    def __init__(
        self, prompt: str, version: int, expected_hash: str, actual_hash: str
    ) -> None:
        super().__init__(
            "Prompt integrity check failed for {}_v{}: expected {}, got {}".format(
                prompt, version, expected_hash, actual_hash
            )
        )


class JudgeError(VerdictError):
    """Network/provider failure calling the judge."""


class JudgeParseError(JudgeError):
    """Failed to parse judge response."""

    def __init__(self, message: str, *, snippet: str = "", raw: Any = None):
        super().__init__(message + (f" | snippet={snippet!r}" if snippet else ""))
        self.snippet = snippet
        self.raw = raw


class RegressionError(VerdictError):
    def __init__(self, result: "CompareResult") -> None:
        self.result = result
        super().__init__(result.summary)
