#!/usr/bin/env python3
"""Disposable seeds must commit without admitting automatic Git maintenance."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


HELPER = Path(__file__).resolve().parent / "lib/drift-tree-snapshot.sh"


class GitLifetimeTests(unittest.TestCase):
    def test_seed_commit_does_not_start_automatic_maintenance(self):
        # Removing the helper's maintenance policy must start real maintenance.
        # Force it synchronous in this test so even the RED has safe cleanup.
        with tempfile.TemporaryDirectory(prefix="assay-drift-git-") as temporary:
            root = Path(temporary)
            repo = root / "repo"
            repo.mkdir()
            home = root / "home"
            home.mkdir()
            empty = root / "empty"
            empty.mkdir()
            trace = root / "trace.jsonl"
            # Git's loose-object heuristic samples bucket 17. Populate it with
            # real SHA-1 blobs, rather than rely on a large or random fixture.
            found = 0
            for number in range(20000):
                payload = (f"synthetic seed {number}\n".encode() * 80)[:1024]
                oid = hashlib.sha1(
                    b"blob " + str(len(payload)).encode() + b"\0" + payload
                ).hexdigest()
                if oid.startswith("17"):
                    (repo / f"blob-{found}.txt").write_bytes(payload)
                    found += 1
                    if found == 12:
                        break
            self.assertEqual(found, 12, "deterministic loose-object fixture incomplete")
            env = {
                "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
                "HOME": str(home), "TMPDIR": str(root), "LC_ALL": "C",
                "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
                "GIT_CONFIG_GLOBAL": os.devnull, "GIT_ATTR_NOSYSTEM": "1",
                "GIT_TERMINAL_PROMPT": "0", "GIT_TRACE2_EVENT": str(trace),
            }
            script = r'''
set -euo pipefail
source "$1"
repo="$2"
empty="$3"
hermetic_git "$repo" -c init.templateDir="$empty" -c init.defaultBranch=main init -q
hermetic_git "$repo" config core.hooksPath "$empty"
hermetic_git "$repo" config core.attributesFile /dev/null
hermetic_git "$repo" config user.name 'Synthetic drift fixture'
hermetic_git "$repo" config user.email 'fixture@example.invalid'
hermetic_git "$repo" config gc.auto 1
hermetic_git "$repo" config maintenance.auto true
hermetic_git "$repo" config gc.autoDetach false
hermetic_git "$repo" config maintenance.autoDetach false
hermetic_git "$repo" add -- .
hermetic_git "$repo" commit -qm seed
hermetic_git "$repo" rev-list --count HEAD
'''
            proc = subprocess.run(
                ["/bin/bash", "-c", script, "drift-git-test", str(HELPER),
                 str(repo), str(empty)],
                env=env, capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(proc.stdout.strip(), "1", "foreground seed commit missing")
            self.assertLess(trace.stat().st_size, 1024 * 1024)
            events = [json.loads(line) for line in trace.read_text().splitlines()]
            names = [e.get("name") for e in events if e.get("event") == "cmd_name"]
            self.assertIn("commit", names, "real Git trace omitted the commit")
            automatic = [n for n in names if n in ("maintenance", "gc", "repack")]
            self.assertEqual(automatic, [], "seed commit admitted automatic Git work")


if __name__ == "__main__":
    unittest.main()
