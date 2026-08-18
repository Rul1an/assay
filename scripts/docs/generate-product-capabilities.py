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


def validate_proof(proof: object, claim_id: str) -> dict:
    if not isinstance(proof, dict):
        raise ValueError(f"claim {claim_id} proof must be an object")
    if "run_id" in proof:
        run_id = proof["run_id"]
        if not isinstance(run_id, int) or isinstance(run_id, bool) or run_id <= 0:
            raise ValueError(f"claim {claim_id} proof run_id must be a positive integer")
    if "commit_sha" in proof:
        commit_sha = proof["commit_sha"]
        if not isinstance(commit_sha, str) or COMMIT_SHA_PATTERN.fullmatch(commit_sha) is None:
            raise ValueError(f"claim {claim_id} proof commit_sha must be immutable")
    if "digest" in proof:
        digest = proof["digest"]
        if not isinstance(digest, str) or DIGEST_PATTERN.fullmatch(digest) is None:
            raise ValueError(f"claim {claim_id} proof digest must be sha256:<64 lowercase hex>")
    if "artifact" in proof:
        require_non_empty_string(proof["artifact"], f"claim {claim_id} proof artifact")
        if "digest" not in proof:
            raise ValueError(f"claim {claim_id} artifact requires a digest")
    if not proof_has_immutable_identity(proof):
        raise ValueError(f"claim {claim_id} proof must include an immutable identity")
    return proof


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
                    validate_proof(proof, claim_id)
            else:
                gap = claim["gap"]
                issue = gap.get("issue") if isinstance(gap, dict) else None
                if not isinstance(issue, str) or re.fullmatch(r"[1-9][0-9]*", issue) is None:
                    raise ValueError(f"claim {claim_id} gap.issue must be a positive issue number")
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
        "Proof identities are shape-checked offline; generation does not fetch or verify proof content, URLs, artifacts, or issue state.",
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
            if "proofs" in claim:
                disposition = (
                    "proof-backed "
                    f"([identities](../generated/product-claim-proof.md#claim-{claim['id']}))"
                )
            else:
                issue = claim["gap"]["issue"]
                disposition = (
                    f"[gap #{issue}](https://github.com/Rul1an/assay/issues/{issue})"
                )
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
            lines.append(f'<a id="claim-{claim["id"]}"></a>')
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
