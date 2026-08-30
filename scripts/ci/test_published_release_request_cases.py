#!/usr/bin/env python3
"""Behavioral release-request contracts; fake binaries are not released-host proof."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/published_release_proxy_phase.py"
LIBRARY = ROOT / "scripts/ci/lib/published-release-capture.sh"
INIT = {"jsonrpc": "2.0", "id": 1, "method": "initialize"}
CALL = {"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {
    "name": "github.add_deploy_key", "arguments": {"owner": "acme", "repo": "prod-app"}}}

FAKE = r'''
import json, pathlib, sys
root = pathlib.Path(__file__).parent
mode = (root / "mode").read_text()
def arg(name): return pathlib.Path(sys.argv[sys.argv.index(name) + 1])
decisions = arg("--enforcement-decision-out")
observations = arg("--denied-call-observation-out")
policy = arg("--enforce-policy")
with (root / "observed.jsonl").open("a") as h:
    h.write(json.dumps({"argv": sys.argv, "cwd": str(pathlib.Path.cwd()),
                        "policy": policy.name}) + "\n")
requests = [json.loads(sys.stdin.readline())]
if requests[0]["method"] == "initialize":
    requests.append(json.loads(sys.stdin.readline()))
if mode == "needs-open-input":
    import select
    if select.select([sys.stdin], [], [], 0.1)[0]:
        print(json.dumps({"jsonrpc": "2.0", "id": 1, "result": {}}))
        raise SystemExit(0)
if mode == "stall":
    import time
    time.sleep(5)
unsupported = requests[-1]["method"] == "unsupported_for_probe"
allow = policy.name == "allow.yaml" and mode != "allow-to-deny"
decision = {"schema": "assay.enforcement_decision.v0", "decision": "allow" if allow else "deny",
            "reason": "allow" if allow else "no_declared_allowance",
            "tool": {"name": "github.add_deploy_key", "action_class": "github_deploy_key"},
            "action": {"target": {"provider": "github", "owner": "acme", "repo": "prod-app"}}}
if not unsupported or mode == "unsupported-decision":
    if mode == "wrong-decision": decision["decision"] = "deny"
    if mode == "wrong-decision-reason": decision["reason"] = "no_declared_allowance"
    if mode == "wrong-decision-schema": decision["schema"] = "other"
    if mode == "wrong-tool": decision["tool"]["name"] = "other"
    if mode == "wrong-target": decision["action"]["target"]["repo"] = "other"
    decisions.write_text(json.dumps(decision) + "\n")
    if mode == "duplicate-decision":
        decisions.write_text(decisions.read_text() * 2)
    if mode == "allow-observation" or not allow:
        observations.write_text('{"schema":"assay.denied_call_observation.v0"}\n')
if mode == "empty-decision": decisions.write_text("")
if mode == "empty-observation": observations.write_text("")
reply = {"jsonrpc": "2.0", "id": 9}
if unsupported or not allow:
    reply["error"] = {"code": -31997 if unsupported else -31999,
                      "data": {"origin": "assay-proxy", "reason": "method_not_allowlisted" if unsupported else "no_declared_allowance"}}
else:
    reply["result"] = {"isError": False, "content": [{"type": "text", "text": "forwarded-ok (mock; no real GitHub call)"}]}
if mode == "wrong-id": reply["id"] = 10
if mode == "wrong-error": reply["error"]["code"] = -32601
if mode == "wrong-origin": reply["error"]["data"]["origin"] = "other"
if mode == "wrong-reason": reply["error"]["data"]["reason"] = "other"
if mode == "string-is-error": reply["result"]["isError"] = "false"
if any(r.get("method") == "initialize" for r in requests):
    print(json.dumps({"jsonrpc": "2.0", "id": 1, "result": {}}))
print(json.dumps(reply))
if mode == "duplicate-reply": print(json.dumps(reply))
if mode == "junk": print("not JSON")
if mode == "nonzero": raise SystemExit(23)
'''


class ReleasedRequestCases(unittest.TestCase):
    def run_case(self, case, mode="normal"):
        root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="request cases ")))
        (root / "mode").write_text(mode)
        fake = root / "assay-mcp-server"
        fake.write_text(f"#!{sys.executable}\n" + FAKE)
        fake.chmod(0o755)
        result_dir = root / "result"
        result_dir.mkdir()
        request = CALL if case != "unsupported" else {
            "jsonrpc": "2.0", "id": 9, "method": "unsupported_for_probe"}
        process = subprocess.run(
            [sys.executable, "-I", str(HELPER), "--policy", "deny" if case == "deny" else "allow",
             "--expect", case, "--timeout-seconds", "2"], input=(json.dumps(INIT) + "\n" if case == "allow" else "") + json.dumps(request) + "\n",
            cwd=result_dir, env={**os.environ, "PATH": f"{root}:/usr/bin:/bin"},
            capture_output=True, text=True, timeout=10)
        observed = json.loads((root / "observed.jsonl").read_text())
        recorded = json.loads((result_dir / "commands.ndjson").read_text())
        self.assertEqual(recorded["argv"], observed["argv"])
        self.assertEqual(recorded["exit_code"], {"nonzero": 23, "stall": 124}.get(mode, 0))
        return process, result_dir, observed

    def test_three_outcomes_and_actual_policy_selection(self):
        for case in ("allow", "deny", "unsupported"):
            with self.subTest(case=case):
                p, root, observed = self.run_case(case)
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assertEqual(observed["policy"], "no-allowance.yaml" if case == "deny" else "allow.yaml")
                if case == "unsupported":
                    self.assertFalse((root / "decisions.ndjson").exists())
                    self.assertFalse((root / "denied-observations.ndjson").exists())
                else:
                    self.assertEqual(json.loads((root / "decisions.ndjson").read_text())["decision"], case)

    def test_allow_to_deny_is_not_a_successful_allow_probe(self):
        p, _, _ = self.run_case("allow", "allow-to-deny")
        self.assertNotEqual(p.returncode, 0, "policy denial satisfied the allow gate")

    def test_waits_for_the_response_before_closing_stdin(self):
        p, _, _ = self.run_case("allow", "needs-open-input")
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_missing_response_still_has_a_bounded_deadline(self):
        p, _, _ = self.run_case("allow", "stall")
        self.assertEqual(p.returncode, 124, p.stderr)

    def test_unsupported_must_not_leave_any_evidence_artifact(self):
        for mode in ("unsupported-decision", "empty-decision", "empty-observation"):
            with self.subTest(mode=mode):
                p, _, _ = self.run_case("unsupported", mode)
                self.assertNotEqual(p.returncode, 0, "unsupported request retained evidence")

    def test_wire_and_policy_records_must_match_the_single_fixture_call(self):
        for case, modes in (
            ("allow", ("wrong-id", "duplicate-reply", "duplicate-decision", "wrong-tool",
                       "wrong-decision", "wrong-decision-reason", "wrong-decision-schema",
                       "wrong-target", "allow-observation", "string-is-error", "junk", "nonzero")),
            ("unsupported", ("wrong-id", "duplicate-reply", "wrong-error", "wrong-origin", "wrong-reason", "junk")),
        ):
            for mode in modes:
                with self.subTest(case=case, mode=mode):
                    p, _, _ = self.run_case(case, mode)
                    self.assertNotEqual(p.returncode, 0, f"{case}/{mode} falsely passed")


class RequestCaseIntegration(unittest.TestCase):
    def run_phase(self, mode="normal", library=None):
        root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="request integration ")))
        (root / "mode").write_text(mode)
        fake = root / "assay-mcp-server"
        fake.write_text(f"#!{sys.executable}\n" + FAKE)
        fake.chmod(0o755)
        cli = root / "assay"
        cli.write_text(f"#!{sys.executable}\n" + r'''
import json, pathlib, sys
root = pathlib.Path(__file__).parent
with (root / "cli-observed.jsonl").open("a") as h:
    h.write(json.dumps(sys.argv) + "\n")
if sys.argv[1:4] == ["evidence", "import", "privileged-mcp-action"]:
    assert "--denied-observations" not in sys.argv
    pathlib.Path(sys.argv[sys.argv.index("--bundle-out") + 1]).write_bytes(b"fake bundle")
else:
    assert sys.argv[1:3] == ["evidence", "verify-privileged-mcp-action"]
    assert sys.argv[-2:] == ["--profile-version", "v1"]
    claims = {name: {"status": "incomplete"} for name in
              ("caller_visible_denial", "upstream_delivery", "external_side_effect")}
    claims["policy_decision_recorded"] = {"status": "confirmed", "source_class": "producer_reported"}
    if (root / "mode").read_text() == "overclaim":
        claims["external_side_effect"]["status"] = "confirmed"
    print(json.dumps({"schema": "assay.privileged_mcp_action.verify.report.v0",
                      "bundle_integrity": "pass", "verdict": "valid", "claims": claims}))
''')
        cli.chmod(0o755)
        results = root / "results"
        results.mkdir()
        (results / "commands.ndjson").write_text('{"name":"earlier-deny-control"}\n')
        source = root / "capture.sh"
        source.write_text(LIBRARY.read_text() if library is None else library)
        env = {**os.environ, "PATH": f"{root}:/usr/bin:/bin", "PYTHON_BIN": sys.executable,
               "JQ_BIN": shutil.which("jq"), "harness_root": str(ROOT), "results": str(results),
               "commands_file": str(results / "commands.ndjson"), "init_request": json.dumps(INIT),
               "call_request": json.dumps(CALL), "workflow_run_id": "123", "workflow_run_attempt": "1"}
        process = subprocess.run(["bash", "-euc", 'source "$1"; fail() { echo "$*" >&2; exit 1; }; run_published_release_extra_request_cases',
                                  "request-test", str(source)], env=env, capture_output=True, text=True, timeout=20)
        return process, root, results

    def test_cases_are_isolated_captured_and_allow_bundle_verified(self):
        p, root, results = self.run_phase()
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertTrue((results / "allow/produced.bundle.tar.gz").is_file())
        self.assertTrue((results / "allow/verify.json").is_file())
        self.assertTrue((results / "unsupported/proxy.jsonl").is_file())
        wire = [json.loads(line) for line in (results / "unsupported/proxy.jsonl").read_text().splitlines()]
        self.assertEqual([row["id"] for row in wire], [9])
        self.assertFalse((results / "unsupported/decisions.ndjson").exists())
        observed = [json.loads(line) for line in (root / "observed.jsonl").read_text().splitlines()]
        self.assertEqual([row["cwd"] for row in observed],
                         [str(results.resolve() / name) for name in ("allow", "unsupported")])
        records = [json.loads(line) for line in (results / "commands.ndjson").read_text().splitlines()]
        self.assertEqual(records[0], {"name": "earlier-deny-control"})
        self.assertEqual([r["argv"] for r in records if r.get("name") == "proxy-enforce"],
                         [r["argv"] for r in observed])
        cli_observed = [json.loads(line) for line in (root / "cli-observed.jsonl").read_text().splitlines()]
        self.assertEqual([r["argv"][1:] for r in records if r.get("name", "").startswith("allow-")],
                         [r[1:] for r in cli_observed])

    def test_request_failure_and_overclaim_cannot_pass_the_driver_phase(self):
        for mode in ("allow-to-deny", "unsupported-decision", "overclaim"):
            with self.subTest(mode=mode):
                p, _, _ = self.run_phase(mode)
                self.assertNotEqual(p.returncode, 0, f"{mode} falsely passed")


if __name__ == "__main__":
    unittest.main()
