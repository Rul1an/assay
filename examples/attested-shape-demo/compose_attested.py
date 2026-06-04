#!/usr/bin/env python3
"""Attested-claim composition SHAPE — SYNTHETIC DEMONSTRATOR ONLY.

================================ READ THIS FIRST ===============================
This file is a synthetic demonstrator. It does NOT implement, verify, or assert
any real attestation mechanism. The "attested" claim basis it explores is NOT a
basis recognised by the current claim-class schema
(`assay.observability.claim_class_cell.v0` defines: reported, measured, derived,
inferred). Nothing here is a commitment, a stable contract, or a statement that
such a basis will be added. It exists purely to illustrate the *shape* a
consumer-side composition rule could take if an attested basis were ever
explored — so the idea can be reasoned about on engineering merit alone.

Every envelope and digest below is fabricated demo data. The verification result
is decided by a flag in the synthetic envelope, not by any cryptographic check.
===============================================================================

What it shows: a claim cell that cites an attestation is only as strong as the
attestation it can actually bind to. The composition rule here degrades an
"attested" cell to a conservative basis unless a verifiable envelope is present
AND that envelope binds to the same subject digest the cell refers to. This is
the same evidence-first discipline the measured/derived cells already use:
absence of verifiable evidence degrades the claim; it never silently upgrades it.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

DEMO_BANNER = "SYNTHETIC DEMONSTRATOR — not a real attestation mechanism"

# Strength lattice, weakest first. The composed strength can never exceed what
# the synthetic evidence supports.
_STRENGTH_ORDER = ("absent", "weak", "partial", "strong")


def _at_most(declared: str, ceiling: str) -> str:
    di = _STRENGTH_ORDER.index(declared) if declared in _STRENGTH_ORDER else 0
    ci = _STRENGTH_ORDER.index(ceiling) if ceiling in _STRENGTH_ORDER else 0
    return declared if di <= ci else ceiling


def compose(cell: dict[str, Any], envelope: dict[str, Any] | None) -> dict[str, Any]:
    """Compose a (synthetic) attested cell with a (synthetic) attestation envelope.

    Rules (illustrative, evidence-first):
    - No envelope -> the attested claim cannot be supported; basis degrades to
      `inferred` and strength is capped at `weak`.
    - Envelope present but not verified (synthetic flag) -> same conservative
      degrade; an unverifiable envelope is not evidence.
    - Envelope verified but bound to a different subject digest than the cell
      cites -> degrade: the attestation does not actually cover this subject.
    - Envelope verified AND subject digest matches -> the declared strength is
      allowed to stand (still capped by the lattice).
    """
    declared = cell.get("claim_strength", "absent")
    cited_digest = cell.get("subject_digest")
    result = {
        "demo": DEMO_BANNER,
        "claim_type": cell.get("claim_type"),
        "declared_strength": declared,
        "declared_basis": cell.get("claim_basis"),
    }

    if envelope is None:
        result.update(
            effective_strength=_at_most(declared, "weak"),
            effective_basis="inferred",
            reason="no attestation envelope supplied; cannot support an attested claim",
        )
        return result

    verified = envelope.get("verification", {}).get("status") == "verified"
    env_digest = envelope.get("subject_digest")

    if not verified:
        result.update(
            effective_strength=_at_most(declared, "weak"),
            effective_basis="inferred",
            reason=(
                "envelope present but verification status is "
                f"{envelope.get('verification', {}).get('status')!r}; "
                "an unverifiable envelope is not evidence"
            ),
        )
        return result

    if env_digest != cited_digest:
        result.update(
            effective_strength=_at_most(declared, "weak"),
            effective_basis="inferred",
            reason=(
                "envelope verifies but its subject digest does not match the "
                "digest the claim cites; the attestation does not cover this subject"
            ),
        )
        return result

    result.update(
        effective_strength=declared,
        effective_basis="attested",
        reason=(
            "envelope verifies (synthetic) and binds to the cited subject digest; "
            "declared strength stands"
        ),
    )
    return result


def render_text(report: dict[str, Any]) -> str:
    return (
        f"[{report['demo']}]\n"
        f"claim: {report['claim_type']}\n"
        f"declared: {report['declared_strength']} ({report['declared_basis']})\n"
        f"effective: {report['effective_strength']} ({report['effective_basis']})\n"
        f"reason: {report['reason']}\n"
    )


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cell", help="synthetic attested claim cell JSON")
    parser.add_argument(
        "--envelope",
        help="synthetic attestation envelope JSON (omit to show the no-evidence degrade)",
    )
    parser.add_argument("--format", choices=["text", "json"], default="text")
    return parser.parse_args()


def _run(args: argparse.Namespace) -> int:
    with open(args.cell, "r", encoding="utf-8") as handle:
        cell = json.load(handle)
    envelope = None
    if args.envelope:
        with open(args.envelope, "r", encoding="utf-8") as handle:
            envelope = json.load(handle)
    report = compose(cell, envelope)
    if args.format == "json":
        sys.stdout.write(json.dumps(report, indent=2, sort_keys=True) + "\n")
    else:
        sys.stdout.write(render_text(report))
    return 0


if __name__ == "__main__":
    raise SystemExit(_run(_parse_args()))
