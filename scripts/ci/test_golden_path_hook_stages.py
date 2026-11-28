#!/usr/bin/env python3
"""Exercise the golden-path hook's real stage and path selection, without its heavy battery."""
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HOOK = "published-release-golden-path-contract"


class Stages(unittest.TestCase):
    def test_real_consumer_selects_push_and_manual_but_not_commit(self):
        precommit = shutil.which("pre-commit")
        self.assertIsNotNone(precommit, "install the version printed by read-pre-commit-pin.sh")
        config = json.loads(subprocess.check_output(
            ["ruby", "-ryaml", "-rjson", "-e",
             "puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))"],
            input=(ROOT / ".pre-commit-config.yaml").read_bytes()))
        hooks = [(repo, hook) for repo in config["repos"] for hook in repo["hooks"]
                 if hook["id"] == HOOK]
        self.assertEqual(len(hooks), 1)
        repo, hook = hooks[0]
        self.assertEqual(repo["repo"], "local")
        self.assertEqual(hook["entry"], "bash scripts/ci/test-published-release-golden-path-contract.sh")
        isolated = copy.deepcopy(config)
        isolated["repos"] = [{"repo": "local", "hooks": [copy.deepcopy(hook)]}]
        # Retain every selection property. Only the work performed is a recorder stub.
        isolated["repos"][0]["hooks"][0]["entry"] = "bash hook-stub.sh"
        with tempfile.TemporaryDirectory(prefix="assay-golden-stages-") as temporary:
            tree = Path(temporary)
            env = {k: v for k, v in os.environ.items()
                   if not k.startswith(("GIT_", "PRE_COMMIT_")) and k != "SKIP"}
            env.update(PRE_COMMIT_HOME=str(tree / "cache"), GIT_CONFIG_NOSYSTEM="1",
                       GIT_CONFIG_GLOBAL=os.devnull)
            subprocess.run(["git", "init", "-q", str(tree)], check=True, env=env)
            (tree / ".pre-commit-config.yaml").write_text(json.dumps(isolated))
            (tree / "hook-stub.sh").write_text(
                '#!/usr/bin/env bash\nset -eu\nprintf "called\\n" >> calls\nexit "${STUB_EXIT:-0}"\n')
            matched = "docs/guides/installed-release-journey.md"
            unrelated = "unrelated.txt"
            (tree / matched).parent.mkdir(parents=True)
            (tree / matched).write_text("fixture\n")
            (tree / unrelated).write_text("fixture\n")
            calls = tree / "calls"
            for stage, path, code, should_run in (
                ("pre-commit", matched, 0, False),
                ("pre-push", matched, 0, True),
                ("manual", matched, 0, True),
                ("pre-push", matched, 19, True),
                ("manual", matched, 19, True),
                ("pre-push", unrelated, 0, False),
            ):
                with self.subTest(stage=stage, path=path, exit=code):
                    calls.unlink(missing_ok=True)
                    result = subprocess.run(
                        [precommit, "run", "--hook-stage", stage, "--files", path],
                        cwd=tree, env={**env, "STUB_EXIT": str(code)},
                        text=True, capture_output=True, timeout=30)
                    self.assertEqual(calls.exists(), should_run,
                                     f"{stage} selection: {result.stdout}{result.stderr}")
                    self.assertEqual(result.returncode == 0, code == 0,
                                     f"{stage} must propagate hook failure: {result.stdout}{result.stderr}")
                    if should_run:
                        self.assertEqual(calls.read_text(), "called\n")


if __name__ == "__main__":
    unittest.main()
