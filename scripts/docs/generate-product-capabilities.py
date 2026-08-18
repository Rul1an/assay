#!/usr/bin/env python3

import argparse
import json
import re
import sys
from pathlib import Path


MATURITY_VALUES = {"stable", "beta", "experimental", "verifier-only", "planned"}
AXIS_VALUES = {"observation", "policy_decision", "outcome"}
CAPABILITY_FIELDS = {
    "id",
    "label",
    "summary",
    "maturity",
    "introduced_release",
    "protocols",
    "platforms",
    "enforcement_points",
    "limitations",
    "non_claims",
    "claims",
}
ID_PATTERN = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")
COMMIT_SHA_PATTERN = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")
DIGEST_PATTERN = re.compile(r"sha256:[0-9a-f]{64}\Z")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate Assay product capability views")
    parser.add_argument(
        "--source", type=Path, default=Path("docs/data/product-capabilities.v0.json")
    )
    parser.add_argument(
        "--public-output", type=Path, default=Path("docs/reference/product-support.md")
    )
    parser.add_argument(
        "--proof-output", type=Path, default=Path("docs/generated/product-claim-proof.md")
    )
    return parser.parse_args()


def require_non_empty_string(value: object, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} must be a non-empty string")
    return value


def require_string_list(value: object, field: str) -> list[str]:
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise ValueError(f"{field} must be an array of strings")
    return value


def require_id(value: object, field: str) -> str:
    identifier = require_non_empty_string(value, field)
    if ID_PATTERN.fullmatch(identifier) is None:
        raise ValueError(f"{field} must match [a-z0-9-]+")
    return identifier


def proof_has_immutable_identity(proof: dict) -> bool:
    run_id = proof.get("run_id")
    if isinstance(run_id, int) and not isinstance(run_id, bool) and run_id > 0:
        return True
    commit_sha = proof.get("commit_sha")
    if isinstance(commit_sha, str) and COMMIT_SHA_PATTERN.fullmatch(commit_sha):
        return True
    digest = proof.get("digest")
    return isinstance(digest, str) and DIGEST_PATTERN.fullmatch(digest) is not None


def load_manifest(path: Path) -> dict:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or manifest.get("schema") != "assay.product-capabilities.v0":
        raise ValueError("schema must be assay.product-capabilities.v0")
    capabilities = manifest.get("capabilities")
    if not isinstance(capabilities, list):
        raise ValueError("capabilities must be an array")

    capability_ids: set[str] = set()
    claim_ids: set[str] = set()
    for capability in capabilities:
        if not isinstance(capability, dict):
            raise ValueError("each capability must be an object")
        missing = CAPABILITY_FIELDS.difference(capability)
        if missing:
            raise ValueError(f"capability is missing fields: {', '.join(sorted(missing))}")
        capability_id = require_id(capability["id"], "capability id")
        if capability_id in capability_ids:
            raise ValueError(f"duplicate capability id: {capability_id}")
        capability_ids.add(capability_id)
        for field in ("label", "summary", "introduced_release"):
            require_non_empty_string(capability[field], f"capability {capability_id} {field}")
        if capability["maturity"] not in MATURITY_VALUES:
            raise ValueError(f"capability {capability_id} has unknown maturity")
        for field in ("protocols", "platforms", "enforcement_points", "limitations", "non_claims"):
            require_string_list(capability[field], f"capability {capability_id} {field}")
        claims = capability["claims"]
        if not isinstance(claims, list):
            raise ValueError(f"capability {capability_id} claims must be an array")
        for claim in claims:
            if not isinstance(claim, dict):
                raise ValueError(f"capability {capability_id} claim must be an object")
            claim_id = require_id(claim.get("id"), "claim id")
            if claim_id in claim_ids:
                raise ValueError(f"duplicate claim id: {claim_id}")
            claim_ids.add(claim_id)
            if claim.get("axis") not in AXIS_VALUES:
                raise ValueError(f"claim {claim_id} has unknown axis")
            has_proofs = "proofs" in claim
            has_gap = "gap" in claim
            if has_proofs == has_gap:
                raise ValueError(f"claim {claim_id} must have exactly one disposition")
            if has_proofs:
                proofs = claim["proofs"]
                if not isinstance(proofs, list) or not proofs:
                    raise ValueError(f"claim {claim_id} proofs must be a non-empty array")
                if capability["maturity"] == "planned":
                    raise ValueError(f"planned capability {capability_id} cannot carry current proof")
                for proof in proofs:
                    if not isinstance(proof, dict) or not proof_has_immutable_identity(proof):
                        raise ValueError(
                            f"claim {claim_id} proof must include an immutable identity"
                        )
                    if "artifact" in proof and "digest" not in proof:
                        raise ValueError(f"claim {claim_id} artifact requires a digest")
            else:
                gap = claim["gap"]
                issue = gap.get("issue") if isinstance(gap, dict) else None
                if not isinstance(issue, str) or not issue.isdigit():
                    raise ValueError(f"claim {claim_id} gap.issue must contain digits")
        claims.sort(key=lambda item: item["id"])
    capabilities.sort(key=lambda item: item["id"])
    return manifest


def join_values(values: list[str]) -> str:
    return ", ".join(values) if values else "None declared"


def render_public(manifest: dict) -> str:
    lines = [
        "<!-- Generated by scripts/docs/generate-product-capabilities.py; do not edit. -->",
        "# Product Support",
        "",
        "This matrix states bounded product support and explicit evidence gaps. A row is not certification or universal host compatibility.",
        "",
    ]
    for capability in manifest["capabilities"]:
        lines.extend(
            [
                f"## {capability['label']} (`{capability['id']}`)",
                "",
                capability["summary"],
                "",
                f"- Maturity: `{capability['maturity']}`",
                f"- Introduced: `{capability['introduced_release']}`",
                f"- Platforms: {join_values(capability['platforms'])}",
                f"- Protocols: {join_values(capability['protocols'])}",
                f"- Enforcement points: {join_values(capability['enforcement_points'])}",
                f"- Limitations: {join_values(capability['limitations'])}",
                f"- Non-claims: {join_values(capability['non_claims'])}",
                "",
                "| Claim | Axis | Evidence state |",
                "|---|---|---|",
            ]
        )
        for claim in capability["claims"]:
            disposition = "proof-backed" if "proofs" in claim else f"gap #{claim['gap']['issue']}"
            lines.append(f"| `{claim['id']}` | `{claim['axis']}` | {disposition} |")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def render_proof(manifest: dict) -> str:
    lines = [
        "<!-- Generated by scripts/docs/generate-product-capabilities.py; do not edit. -->",
        "# Product Claim Proof Index",
        "",
        "Proof identities are checked for shape offline; this generator does not fetch them.",
        "",
    ]
    for capability in manifest["capabilities"]:
        lines.append(f"## `{capability['id']}`")
        lines.append("")
        for claim in capability["claims"]:
            lines.append(f"### `{claim['id']}` (`{claim['axis']}`)")
            if "gap" in claim:
                lines.append(f"- Gap issue: `#{claim['gap']['issue']}`")
            else:
                for proof in claim["proofs"]:
                    rendered = ", ".join(
                        f"`{key}={proof[key]}`" for key in sorted(proof) if key != "url"
                    )
                    lines.append(f"- Proof: {rendered}")
            lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def main() -> int:
    args = parse_args()
    try:
        manifest = load_manifest(args.source)
        write_text(args.public_output, render_public(manifest))
        write_text(args.proof_output, render_proof(manifest))
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
