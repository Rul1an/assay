#!/usr/bin/env python3
"""Adequacy of THIS repository's own corpora, plus the drift guards behind it.

    python3 conformance/tests/test_corpus_adequacy_own_corpora.py

#192's first obligation is to run the tool on ourselves and publish the result
whether or not it flatters us, because offering a corpus audit we have not
survived is a claim without cover.
"""

from __future__ import annotations

import glob
import json
import sys
import unittest
from pathlib import Path

# The tool lives in its own organisation now: corpus-adequacy/corpus-adequacy.
# It is NOT vendored here, because two implementations of a measurement drift and
# the copy that drifts is the one that stops measuring. Expect it as a sibling
# checkout, and skip loudly rather than pretending the corpora were measured.
_TOOL = Path(__file__).resolve().parents[3] / "corpus-adequacy"
if _TOOL.is_dir():
    sys.path.insert(0, str(_TOOL))
try:
    import corpus_adequacy as ca  # noqa: E402
except ImportError:  # pragma: no cover - environment without the sibling checkout
    ca = None

REPO = Path(__file__).resolve().parents[2]
MANIFEST = REPO / "conformance/adequacy/mcp-jsonrpc-id.manifest.json"
PACK = REPO / "examples/mcp-jsonrpc-id-conformance"
OBS_MANIFEST = REPO / "conformance/adequacy/observed-effect-drift-consumer.manifest.json"
OBS_CONSUMER = REPO.parent / "observed-effect-v0/observed-effect-drift-consumer-2026-06/independent_consumer.py"
RGE_MANIFEST = REPO / "conformance/adequacy/rge-bench.manifest.json"
RGE_IMPL = REPO.parent / "rge-bench/ref_example.py"


@unittest.skipIf(ca is None,
                 "corpus-adequacy not found as a sibling checkout; clone "
                 "https://github.com/corpus-adequacy/corpus-adequacy next to this repository")
class DerivedVectorsDoNotDrift(unittest.TestCase):
    """The adequacy vectors are a projection of the pack; keep them one thing."""

    def test_the_derived_vectors_still_match_the_pack(self):
        derived = json.loads((REPO / "conformance/adequacy/mcp-jsonrpc-id.vectors.json")
                             .read_text())["vectors"]
        fresh = []
        for f in sorted(glob.glob(str(PACK / "vectors" / "*.json"))):
            d = json.loads(Path(f).read_text())
            fresh.append({"vector_id": d["id"], "message": d["message"]})
        self.assertEqual(derived, fresh,
                         "the pack's vectors moved; regenerate mcp-jsonrpc-id.vectors.json")

    def test_the_manifest_lives_outside_the_pack(self):
        # Adding files inside the pack breaks its own SHA256SUMS guard. Verified by
        # trying it: check.py reproduce returned PackError until they were moved out.
        for p in PACK.rglob("adequacy*"):
            self.fail("adequacy artifact inside the checksummed pack: %s" % p)


@unittest.skipIf(ca is None, "corpus-adequacy not found as a sibling checkout")
class OwnCorpusAdequacy(unittest.TestCase):
    def test_mcp_jsonrpc_id_scores_the_id_type_arms_not_only_presence(self):
        # 4 of 4 was a score over the presence/null arms alone. The positive
        # control is a string id and RequestId is string | number, so the type
        # arms belong in the denominator. Two of them are isolated; four are not.
        rep = ca.run(MANIFEST)
        self.assertEqual(rep["killed"], 6)
        self.assertEqual(rep["survived"], 4)
        self.assertEqual(rep["score_percent"], 60.0)
        self.assertFalse(rep["adequate"])

    def test_every_out_of_scope_declaration_carries_a_reason(self):
        # Blocking review on #2538: an unreasoned exclusion is a percentage target
        # wearing a different coat.
        import json as _json
        m = _json.loads(MANIFEST.read_text())
        for mut in m["mutants"]["error_response_id"]:
            if mut.get("scope") == "out_of_scope":
                self.assertTrue(str(mut.get("reason", "")).strip(), mut["label"])

    def test_the_report_names_the_denominator_and_the_ratio(self):
        rep = ca.run(MANIFEST)
        self.assertEqual(rep["declared_total"], 17)
        self.assertEqual(rep["out_of_scope_ratio"], 0.7)
        self.assertIn("author-declared", rep["score_means"])

    def test_the_out_of_scope_count_is_stated_rather_than_hidden(self):
        # The honest half of the result: seven envelope rules no vector exercises.
        # If this number silently drops to zero, someone removed the disclosure
        # rather than adding vectors. Re-checked by applying each envelope mutant
        # to the three published messages: none of them moves an outcome.
        rep = ca.run(MANIFEST)
        self.assertEqual(rep["unexercised_out_of_scope"], 7)

    def test_every_in_scope_mutant_is_an_id_rule(self):
        # Guards the scope split itself: in-scope must mean the id contradiction
        # the pack's README declares, not whatever happens to be killable.
        m = json.loads(MANIFEST.read_text())
        in_scope = {x["label"] for x in m["mutants"]["error_response_id"]
                    if x.get("scope", "declared") == "declared"}
        self.assertEqual(in_scope, {
            "6 MCP tolerates an omitted id",
            "7 MCP RequestId excludes null",
            "8 JSON-RPC requires id to be present",
            "9 JSON-RPC permits a null id",
            "11 MCP RequestId includes string",
            "12 JSON-RPC accepts a string id",
            "13 MCP RequestId includes number",
            "14 a bool is not an MCP RequestId",
            "15 JSON-RPC accepts a number id",
            "16 a bool is not a JSON-RPC number id",
        }, "in-scope is the id-field rules, including the type arms the control exists for")


@unittest.skipIf(ca is None, "corpus-adequacy not found as a sibling checkout")
@unittest.skipUnless(OBS_CONSUMER.is_file(),
                     "observed-effect-v0 not found as a sibling checkout")
class ObservedEffectDeclarationIsTheConsumer(unittest.TestCase):
    def test_the_score_is_over_recompute_and_profile_not_only_merge(self):
        # 4 of 5 was merge-policy only. The 14 case names announce the rest.
        # This number is supposed to be the lower, true one; do not tune it up.
        rep = ca.run(OBS_MANIFEST)
        self.assertEqual(rep["killed"], 14)
        self.assertEqual(rep["survived"], 9)
        self.assertEqual(rep["score_percent"], 60.9)
        self.assertFalse(rep["adequate"])


@unittest.skipIf(ca is None, "corpus-adequacy not found as a sibling checkout")
@unittest.skipUnless(RGE_IMPL.is_file(), "rge-bench not found as a sibling checkout")
class RgeBenchDeclarationMatchesTheImplementation(unittest.TestCase):
    def test_the_score_is_over_the_rules_ref_example_has(self):
        # 30 of 30 was the hand-written table. The strength ladder, the
        # conjuncts, and the fallthroughs were discriminable and undeclared.
        rep = ca.run(RGE_MANIFEST)
        self.assertEqual(rep["killed"], 51)
        self.assertEqual(rep["survived"], 3)
        self.assertEqual(rep["score_percent"], 94.4)
        self.assertFalse(rep["adequate"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
