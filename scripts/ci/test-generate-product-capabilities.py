#!/usr/bin/env python3

import hashlib
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
                "target_release": None,
                "protocol_versions": [],
                "profile_versions": [],
                "platforms": ["linux-x86_64"],
                "enforcement_points": ["cli"],
                "limitations": [],
                "non_claims": ["No universal host compatibility claim."],
                "claims": [
                    {
                        "id": "published-install",
                        "axis": "observation",
                        "proofs": [
                            {
                                "kind": "github-actions-artifact",
                                "url": "https://example.invalid/latest",
                            }
                        ],
                    }
                ],
            }
        ],
    }


def run_generator(manifest: dict, root: Path) -> subprocess.CompletedProcess[str]:
    source = root / "capabilities.json"
    source.write_text(json.dumps(manifest), encoding="utf-8")
    return run_generator_source(source, root)


def run_generator_source(source: Path, root: Path) -> subprocess.CompletedProcess[str]:
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
        claim["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 32122877879}
        ]
        manifest["capabilities"][0]["claims"].append(dict(claim))
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate claim id: published-install", result.stderr)

    def test_rejects_duplicate_capability_ids(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        manifest["capabilities"].append(
            json.loads(json.dumps(manifest["capabilities"][0]))
        )
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate capability id: install-to-evidence", result.stderr)

    def test_rejects_unknown_proof_fields(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1, "status": "certified"}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof has unknown fields: status", result.stderr)

    def test_rejects_unknown_fields_at_every_manifest_level(self) -> None:
        cases = []
        root_manifest = minimal_manifest()
        root_manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        root_manifest["status"] = "certified"
        cases.append((root_manifest, "manifest has unknown fields: status"))

        capability_manifest = minimal_manifest()
        capability_manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        capability_manifest["capabilities"][0]["certification"] = "certified"
        cases.append(
            (capability_manifest, "capability has unknown fields: certification")
        )

        claim_manifest = minimal_manifest()
        claim_manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        claim_manifest["capabilities"][0]["claims"][0]["status"] = "certified"
        cases.append((claim_manifest, "claim has unknown fields: status"))

        gap_manifest = minimal_manifest()
        claim = gap_manifest["capabilities"][0]["claims"][0]
        claim.pop("proofs")
        claim["gap"] = {"issue": "2486", "status": "closed"}
        cases.append((gap_manifest, "gap has unknown fields: status"))

        for manifest, message in cases:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as tmp:
                result = run_generator(manifest, Path(tmp))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(message, result.stderr)

    def test_rejects_duplicate_json_object_keys(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        raw = json.dumps(manifest).replace(
            '"run_id": 1', '"run_id": 0, "run_id": 1'
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "capabilities.json"
            source.write_text(raw, encoding="utf-8")
            result = run_generator_source(source, root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate JSON object key: run_id", result.stderr)

    def test_profile_readers_pin_policy_links_and_both_versions(self) -> None:
        manifest = json.loads(
            (REPO_ROOT / "docs/data/product-capabilities.v0.json").read_text(
                encoding="utf-8"
            )
        )
        capabilities = {entry["id"]: entry for entry in manifest["capabilities"]}
        capability = capabilities["privileged-mcp-action-profile-readers"]
        claim = capability["claims"][0]
        compatibility = (
            REPO_ROOT / "docs/profiles/compatibility.md"
        ).read_text(encoding="utf-8")
        v0 = (
            REPO_ROOT / "docs/profiles/privileged-mcp-action/v0.md"
        ).read_text(encoding="utf-8")
        v1 = (
            REPO_ROOT / "docs/profiles/privileged-mcp-action/v1.md"
        ).read_text(encoding="utf-8")

        self.assertEqual(capability["maturity"], "verifier-only")
        self.assertEqual(capability["introduced_release"], "5.4.0")
        self.assertIsNone(capability["target_release"])
        self.assertEqual(capability["enforcement_points"], ["cli"])
        self.assertEqual(
            capability["profile_versions"],
            [
                {"profile": "privileged-mcp-action", "version": "v0"},
                {"profile": "privileged-mcp-action", "version": "v1"},
            ],
        )
        self.assertNotIn("proofs", claim)
        self.assertEqual(claim["gap"], {"issue": "2574"})
        self.assertIn("[`../compatibility.md`](../compatibility.md)", v0)
        self.assertIn("[`../compatibility.md`](../compatibility.md)", v1)
        self.assertIn("`CHANGELOG.md` is the single announcement surface", compatibility)
        self.assertIn(
            "Announcement and removal MUST NOT be in the same release",
            compatibility,
        )
        self.assertIn("#2487", compatibility)
        self.assertNotRegex(compatibility, r"current\s*\+\s*previous")
        self.assertNotRegex(compatibility, r"(?i)calendar (year|month|quarter|window)")

    def test_published_mcp_capabilities_list_all_shipped_protocol_versions(self) -> None:
        manifest = json.loads(
            (REPO_ROOT / "docs/data/product-capabilities.v0.json").read_text(
                encoding="utf-8"
            )
        )
        capabilities = {entry["id"]: entry for entry in manifest["capabilities"]}
        expected = [
            {"protocol": "mcp", "version": "2024-11-05", "transport": "stdio"},
            {"protocol": "mcp", "version": "2025-06-18", "transport": "stdio"},
            {"protocol": "mcp", "version": "2025-11-25", "transport": "stdio"},
        ]

        self.assertEqual(capabilities["published-mcp-server"]["protocol_versions"], expected)
        self.assertEqual(
            capabilities["published-release-golden-path"]["protocol_versions"], expected
        )

    def test_published_binary_proofs_are_release_assets(self) -> None:
        manifest = json.loads(
            (REPO_ROOT / "docs/data/product-capabilities.v0.json").read_text(
                encoding="utf-8"
            )
        )
        capabilities = {entry["id"]: entry for entry in manifest["capabilities"]}

        for capability_id in ("published-cli", "published-mcp-server"):
            with self.subTest(capability=capability_id):
                proofs = capabilities[capability_id]["claims"][0]["proofs"]
                self.assertTrue(proofs)
                self.assertTrue(
                    all(proof["kind"] == "release-asset" for proof in proofs)
                )

    def test_published_release_golden_path_has_a_receipt_for_each_target(self) -> None:
        manifest = json.loads((REPO_ROOT / "docs/data/product-capabilities.v0.json").read_bytes())
        row = next(c for c in manifest["capabilities"] if c["id"] == "published-release-golden-path")
        targets = {
            "darwin-aarch64": "aarch64-apple-darwin",
            "darwin-x86_64": "x86_64-apple-darwin",
            "linux-aarch64": "aarch64-unknown-linux-gnu",
            "linux-x86_64": "x86_64-unknown-linux-gnu",
            "windows-x86_64": "x86_64-pc-windows-msvc",
        }
        self.assertCountEqual(row["platforms"], targets.keys())
        self.assertEqual(row["introduced_release"], "5.3.0")
        self.assertEqual(row["maturity"], "stable")
        self.assertIsNone(row["target_release"])
        self.assertEqual(row["enforcement_points"], ["cli", "mcp-proxy"])
        self.assertEqual(row["profile_versions"], [{"profile": "privileged-mcp-action", "version": "v1"}])
        (claim,) = row["claims"]
        self.assertEqual((claim["id"], claim["axis"]), ("release-install-to-verified-evidence", "outcome"))
        head = "081473c115ce7f03626b34f878daae2a598630c8"
        names = [f"published-release-golden-path-{t}-v8.0.0-{head}" for t in targets.values()]
        self.assertCountEqual([p["artifact"] for p in claim["proofs"]], names)
        root = REPO_ROOT / "docs/experiments/published-journey-v8.0.0"
        table = (root / "README.md").read_text()
        for target in targets.values():
            artifact = f"published-release-golden-path-{target}-v8.0.0-{head}"
            proof = next(p for p in claim["proofs"] if p["artifact"] == artifact)
            self.assertEqual(proof["kind"], "github-actions-artifact")
            self.assertEqual((proof["run_id"], proof["commit_sha"]), (45188464669, head))
            artifact_id = proof["url"].rsplit("/", 1)[1]
            raw = (root / target / "receipt.json").read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            self.assertIn(f"| [{target}]({target}/receipt.json) | {artifact_id} | `{digest}` |", table)
            receipt = json.loads(raw)
            self.assertEqual(receipt["status"], "completed")
            self.assertEqual(receipt["exit_code"], 0)
            self.assertEqual([s["exit_code"] for s in receipt["stage_records"]], [0] * 11)
            self.assertEqual(receipt["cli_before_sha256"], receipt["cli_after_sha256"])
            online = (root / target / "verify.json").read_bytes()
            self.assertEqual((root / target / "verify-offline.json").read_bytes(), online)
            report = json.loads(online)
            self.assertEqual(report["profile"], "privileged-mcp-action/v1")
            self.assertEqual(report["verdict"], "valid")


    def test_rejects_proof_without_kind(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [{"run_id": 1}]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof is missing fields: kind", result.stderr)

    def test_rejects_unknown_proof_kind(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "generic-digest", "run_id": 1}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof has unknown kind", result.stderr)

    def test_renders_digest_subject_by_proof_kind(self) -> None:
        manifest = minimal_manifest()
        claim = manifest["capabilities"][0]["claims"][0]
        claim["proofs"] = [
            {
                "kind": "release-asset",
                "artifact": "assay.tar.gz",
                "digest": "sha256:" + "a" * 64,
            },
            {
                "kind": "github-actions-artifact",
                "artifact": "journey-proof",
                "digest": "sha256:" + "b" * 64,
            },
        ]

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = run_generator(manifest, root)
            self.assertEqual(result.returncode, 0, result.stderr)
            proof = (root / "proof.md").read_text(encoding="utf-8")

        self.assertIn("release_asset=assay.tar.gz", proof)
        self.assertIn("release_asset_digest=sha256:" + "a" * 64, proof)
        self.assertIn("run_artifact=journey-proof", proof)
        self.assertIn("run_artifact_digest=sha256:" + "b" * 64, proof)

    def test_writes_both_views_in_shared_id_order(self) -> None:
        manifest = minimal_manifest()
        first = manifest["capabilities"][0]
        first["id"] = "z-capability"
        first["claims"][0]["id"] = "z-claim"
        first["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "commit_sha": "a" * 40}
        ]
        earlier_claim = json.loads(json.dumps(first["claims"][0]))
        earlier_claim["id"] = "b-claim"
        earlier_claim["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 2}
        ]
        first["claims"].append(earlier_claim)
        second = json.loads(json.dumps(first))
        second["id"] = "a-capability"
        second["label"] = "A capability"
        second["claims"] = [
            {
                "id": "a-claim",
                "axis": "observation",
                "proofs": [
                    {
                        "kind": "github-actions-artifact",
                        "digest": "sha256:" + "b" * 64,
                    }
                ],
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
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": True}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof run_id must be a positive integer", result.stderr)

    def test_rejects_markdown_unsafe_identifier(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["id"] = "bad|identifier"
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("capability id must match [a-z0-9-]+", result.stderr)

    def test_rejects_mutable_commit_ref(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "commit_sha": "main"}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("proof commit_sha must be immutable", result.stderr)

    def test_rejects_artifact_without_digest(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {"kind": "release-asset", "artifact": "assay.tar.gz", "run_id": 1}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("artifact requires a digest", result.stderr)

    def test_rejects_invalid_digest_even_with_valid_run_id(self) -> None:
        manifest = minimal_manifest()
        manifest["capabilities"][0]["claims"][0]["proofs"] = [
            {
                "kind": "release-asset",
                "artifact": "assay.tar.gz",
                "digest": "latest",
                "run_id": 1,
            }
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
        proof_claim["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]
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

    def test_renders_versioned_protocols_profiles_and_nonclaims_in_both_views(self) -> None:
        manifest = minimal_manifest()
        capability = manifest["capabilities"][0]
        capability["protocol_versions"] = [
            {"protocol": "mcp", "version": "2025-11-25", "transport": "stdio"}
        ]
        capability["profile_versions"] = [
            {"profile": "privileged-mcp-action", "version": "v0"}
        ]
        capability["claims"][0]["proofs"] = [
            {"kind": "github-actions-artifact", "run_id": 1}
        ]

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = run_generator(manifest, root)
            self.assertEqual(result.returncode, 0, result.stderr)
            public = (root / "public.md").read_text(encoding="utf-8")
            proof = (root / "proof.md").read_text(encoding="utf-8")

        for output in (public, proof):
            self.assertIn("mcp 2025-11-25 over stdio", output)
            self.assertIn("privileged-mcp-action/v0", output)
            self.assertIn("No universal host compatibility claim.", output)

    def test_planned_capability_has_target_not_introduced_release(self) -> None:
        manifest = minimal_manifest()
        capability = manifest["capabilities"][0]
        capability["maturity"] = "planned"
        capability["target_release"] = "5.4.0"
        capability["claims"][0].pop("proofs")
        capability["claims"][0]["gap"] = {"issue": "2486"}

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            rejected = run_generator(manifest, root)
            capability["introduced_release"] = None
            accepted = run_generator(manifest, root)
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            public = (root / "public.md").read_text(encoding="utf-8")

        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("planned capability install-to-evidence cannot have introduced_release", rejected.stderr)
        self.assertIn("Target release: `5.4.0`", public)
        self.assertNotIn("Introduced:", public)

    def test_rejects_protocol_or_profile_without_version(self) -> None:
        manifest = minimal_manifest()
        capability = manifest["capabilities"][0]
        capability["protocol_versions"] = [{"protocol": "mcp", "transport": "stdio"}]
        with tempfile.TemporaryDirectory() as tmp:
            protocol_result = run_generator(manifest, Path(tmp))

        capability["protocol_versions"] = []
        capability["profile_versions"] = [{"profile": "privileged-mcp-action"}]
        with tempfile.TemporaryDirectory() as tmp:
            profile_result = run_generator(manifest, Path(tmp))

        self.assertNotEqual(protocol_result.returncode, 0)
        self.assertIn("protocol version must contain protocol, version and transport", protocol_result.stderr)
        self.assertNotEqual(profile_result.returncode, 0)
        self.assertIn("profile version must contain profile and version", profile_result.stderr)


if __name__ == "__main__":
    unittest.main()
