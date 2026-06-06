import pytest

from assay.claim_support import (
    BLOCKED,
    DEGRADED,
    NOT_EVALUABLE,
    SUPPORTED,
    score_claim_support,
)


def test_all_supported():
    assert score_claim_support([SUPPORTED, SUPPORTED]) == SUPPORTED


def test_empty_is_not_evaluable():
    assert score_claim_support([]) == NOT_EVALUABLE


def test_blocked_dominates():
    assert (
        score_claim_support([SUPPORTED, DEGRADED, BLOCKED, NOT_EVALUABLE]) == BLOCKED
    )


def test_not_evaluable_over_degraded():
    # An inconclusive fact is never upgraded to supported; not_evaluable wins
    # over degraded when no fact is blocked.
    assert score_claim_support([SUPPORTED, DEGRADED, NOT_EVALUABLE]) == NOT_EVALUABLE


def test_degraded_when_no_block_or_unknown():
    assert score_claim_support([SUPPORTED, DEGRADED]) == DEGRADED


def test_invalid_outcome_raises():
    with pytest.raises(ValueError):
        score_claim_support(["bogus"])
