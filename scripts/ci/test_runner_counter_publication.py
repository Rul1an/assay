#!/usr/bin/env python3
"""Pause the real outside-control loop after redirection, before writing count 2."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest


class CounterPublicationTest(unittest.TestCase):
    def test_reader_keeps_complete_previous_count_during_next_write(self):
        source = Path(__file__).with_name("runner-spike-kernel-only-acceptance.sh").read_text()
        # Execute the production loop, excluding Linux/cgroup/capture setup.
        start = source.index('  while [ "$i" -lt 6000 ]')
        end = source.index('\n  done', start) + len('\n  done')
        loop = source[start:end]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "marker"
            marker.touch()
            script = '''
set -euo pipefail
i=0
echo() {
  if [ "$i" -eq 2 ]; then
    touch "$OUTSIDE_DIR/paused"
    read -r release
  fi
  builtin echo "$@"
}
sleep() {
  if [ "$i" -eq 2 ]; then touch "$OUTSIDE_DIR/stop"; fi
}
''' + loop
            env = dict(os.environ, OUTSIDE_DIR=directory,
                       OUTSIDE_EXEC_MARKER="/usr/bin/true", OUTSIDE_OPEN_MARKER=str(marker))
            with subprocess.Popen(["bash", "-c", script], env=env, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
                try:
                    deadline = time.monotonic() + 5
                    while not (root / "paused").exists():
                        if process.poll() is not None or time.monotonic() >= deadline:
                            self.fail("producer did not reach second-write barrier")
                        time.sleep(0.01)
                    during_write = (root / "count").read_text()
                    stdout, stderr = process.communicate("continue\n", timeout=5)
                    self.assertEqual(process.returncode, 0, stdout + stderr)
                    self.assertEqual(during_write, "1\n", "reader saw an incomplete published count")
                    self.assertEqual((root / "count").read_text(), "2\n")
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.communicate()


if __name__ == "__main__":
    unittest.main()
