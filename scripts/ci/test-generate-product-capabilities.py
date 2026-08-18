#!/usr/bin/env python3

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
GENERATOR = REPO_ROOT / "scripts/docs/generate-product-capabilities.py"


def minimal_manifest() -> dict:
    return {
        "schema": "assay.product-capabilities.v0",
        "capabilities": [
            {
                "id": "install-to-evidence",
                "label": "Install to evidence",
                "summary": "Install Assay and produce verifiable evidence.",
                "maturity": "stable",
                "introduced_release": "5.3.0",
                "protocols": [],
                "platforms": ["linux-x86_64"],
                "enforcement_points": ["cli"],
                "limitations": [],
                "non_claims": ["No universal host compatibility claim."],
                "claims": [
                    {
                        "id": "published-install",
                        "axis": "observation",
                        "proofs": [{"url": "https://example.invalid/latest"}],
                    }
                ],
            }
        ],
    }


def run_generator(manifest: dict, root: Path) -> subprocess.CompletedProcess[str]:
    source = root / "capabilities.json"
    source.write_text(json.dumps(manifest), encoding="utf-8")
    return subprocess.run(
        [
            "python3",
            str(GENERATOR),
            "--source",
            str(source),
            "--public-output",
            str(root / "public.md"),
            "--proof-output",
            str(root / "proof.md"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )


class ProductCapabilityGeneratorTests(unittest.TestCase):
    def test_rejects_mutable_url_only_proof(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = run_generator(minimal_manifest(), root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "claim published-install proof must include an immutable identity",
            result.stderr,
        )

    def test_rejects_claim_without_proof_or_gap(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0].pop("proofs")
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("claim published-install must have exactly one disposition", result.stderr)

    def test_rejects_duplicate_claim_ids(self) -> None:
        manifest = minimal_manifest()
        claim = manifest["capabilities"][0]["claims"][0]
        claim["proofs"] = [{"run_id": 32122877879}]
        manifest["capabilities"][0]["claims"].append(dict(claim))
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate claim id: published-install", result.stderr)

    def test_writes_both_views_in_shared_id_order(self) -> None:
        manifest = minimal_manifest()
        first = manifest["capabilities"][0]
        first["id"] = "z-capability"
        first["claims"][0]["id"] = "z-claim"
        first["claims"][0]["proofs"] = [{"commit_sha": "a" * 40}]
        earlier_claim = json.loads(json.dumps(first["claims"][0]))
        earlier_claim["id"] = "b-claim"
        earlier_claim["proofs"] = [{"run_id": 2}]
        first["claims"].append(earlier_claim)
        second = json.loads(json.dumps(first))
        second["id"] = "a-capability"
        second["label"] = "A capability"
        second["claims"] = [
            {
                "id": "a-claim",
                "axis": "observation",
                "proofs": [{"digest": "sha256:" + "b" * 64}],
            }
        ]
        manifest["capabilities"].append(second)

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = run_generator(manifest, root)
            self.assertEqual(result.returncode, 0, result.stderr)
            public = (root / "public.md").read_text(encoding="utf-8")
            proof = (root / "proof.md").read_text(encoding="utf-8")

        self.assertLess(public.index("a-capability"), public.index("z-capability"))
        self.assertLess(proof.index("a-capability"), proof.index("z-capability"))
        self.assertLess(public.index("b-claim"), public.index("z-claim"))
        self.assertLess(proof.index("b-claim"), proof.index("z-claim"))
        self.assertIn("sha256:" + "b" * 64, proof)
        self.assertNotIn("sha256:" + "b" * 64, public)

    def test_rejects_boolean_run_id(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [{"run_id": True}]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof run_id must be a positive integer", result.stderr)

    def test_rejects_markdown_unsafe_identifier(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["id"] = "bad|identifier"
        manifest["capabilities"][0]["claims"][0]["proofs"] = [{"run_id": 1}]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("capability id must match [a-z0-9-]+", result.stderr)

    def test_rejects_mutable_commit_ref(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"commit_sha": "main"}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof commit_sha must be immutable", result.stderr)

    def test_rejects_artifact_without_digest(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"artifact": "assay.tar.gz", "run_id": 1}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("artifact requires a digest", result.stderr)

    def test_rejects_invalid_digest_even_with_valid_run_id(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"artifact": "assay.tar.gz", "digest": "latest", "run_id": 1}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof digest must be sha256:<64 lowercase hex>", result.stderr)

    def test_rejects_nonpositive_gap_issue(self) -> None:
        manifest = minimal_manifest()
        claim = manifest["capabilities"][0]["claims"][0]
        claim.pop("proofs")
        claim["gap"] = {"issue": "0"}
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("gap.issue must be a positive issue number", result.stderr)

    def test_public_view_links_proofs_and_gaps_and_states_verification_boundary(self) -> None:
        manifest = minimal_manifest()
        proof_claim = manifest["capabilities"][0]["claims"][0]
        proof_claim["proofs"] = [{"run_id": 1}]
        gap_claim = {"id": "tracked-gap", "axis": "outcome", "gap": {"issue": "2486"}}
        manifest["capabilities"][0]["claims"].append(gap_claim)

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = run_generator(manifest, root)
            self.assertEqual(result.returncode, 0, result.stderr)
            public = (root / "public.md").read_text(encoding="utf-8")

        self.assertIn("does not fetch or verify proof content", public)
        self.assertIn("product-claim-proof.md#claim-published-install", public)
        self.assertIn("https://github.com/Rul1an/assay/issues/2486", public)


if __name__ == "__main__":
    unittest.main()
