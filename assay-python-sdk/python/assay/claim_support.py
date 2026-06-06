"""Claim-support scoring for Assay claim-class evidence (ADR-040).

Pure aggregation of per-fact claim-class outcomes into one claim-support verdict,
plus an optional Inspect (``inspect_ai``) scorer wrapper. Verdicts use the Assay
claim-class vocabulary: ``supported``, ``degraded``, ``blocked``, ``not_evaluable``.

The aggregation is fail-safe: ``blocked`` dominates, then ``not_evaluable`` (an
inconclusive fact is never upgraded to supported), then ``degraded``, otherwise
``supported``. Empty evidence is ``not_evaluable`` (no observation can support a
claim). This mirrors the consumer-side "observed support is the ceiling" rule.
"""

from __future__ import annotations

from typing import Iterable

SUPPORTED = "supported"
DEGRADED = "degraded"
BLOCKED = "blocked"
NOT_EVALUABLE = "not_evaluable"

_VALID = {SUPPORTED, DEGRADED, BLOCKED, NOT_EVALUABLE}


def score_claim_support(outcomes: Iterable[str]) -> str:
    """Aggregate per-fact claim-class outcomes into one claim-support verdict.

    Args:
        outcomes: per-fact outcomes, each one of ``supported``, ``degraded``,
            ``blocked``, or ``not_evaluable``.

    Returns:
        The overall verdict. Empty input returns ``not_evaluable``.

    Raises:
        ValueError: if any outcome is not a known claim-class outcome.
    """
    seen: set[str] = set()
    for outcome in outcomes:
        if outcome not in _VALID:
            raise ValueError(f"unknown claim-class outcome: {outcome!r}")
        seen.add(outcome)

    if not seen:
        return NOT_EVALUABLE
    if BLOCKED in seen:
        return BLOCKED
    if NOT_EVALUABLE in seen:
        return NOT_EVALUABLE
    if DEGRADED in seen:
        return DEGRADED
    return SUPPORTED


def claim_support_scorer():  # type: ignore[no-untyped-def]
    """Return an Inspect scorer that grades claim support from observed outcomes.

    Requires the optional ``inspect_ai`` package. The scorer reads per-fact
    claim-class outcomes from the sample metadata key ``assay_claim_outcomes``
    (a list of outcome strings), aggregates them with :func:`score_claim_support`,
    and scores ``CORRECT`` only when the aggregate is ``supported``. The verdict is
    attached as the score answer and explanation so a degraded or blocked claim is
    visible rather than silently failing.

    Raises:
        ImportError: if ``inspect_ai`` is not installed.
    """
    try:
        from inspect_ai.scorer import CORRECT, INCORRECT, Score, Target, accuracy, scorer
        from inspect_ai.solver import TaskState
    except ImportError as exc:  # pragma: no cover - exercised only without inspect_ai
        raise ImportError(
            "claim_support_scorer requires inspect_ai; install with `pip install inspect_ai`"
        ) from exc

    @scorer(metrics=[accuracy()])
    def _claim_support():  # type: ignore[no-untyped-def]
        async def score(state: TaskState, target: Target) -> Score:  # noqa: ARG001
            outcomes = state.metadata.get("assay_claim_outcomes", [])
            verdict = score_claim_support(outcomes)
            return Score(
                value=CORRECT if verdict == SUPPORTED else INCORRECT,
                answer=verdict,
                explanation=f"claim support: {verdict}",
            )

        return score

    return _claim_support()
