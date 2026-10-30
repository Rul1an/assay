"""Real CLI/wrapper and cold receiver, with a synthetic request and no model."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

KIT = Path(__file__).resolve().parent

@unittest.skipUnless(os.environ.get('ASSAY_BIN'), 'set ASSAY_BIN to the Linux source-built executable')
class CaptureTests(unittest.TestCase):
    def test_wrapper_outputs_replay_without_oracle_and_damage_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); raw = root/'capture'; raw.mkdir(); (raw/'oracle').mkdir()
            command = [sys.executable, '-I', str(KIT/'wrap_entry.py'), str(raw), os.environ['ASSAY_BIN']]
            request = b'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"policy_denied","arguments":{"attempt":1}}}\n'
            run = subprocess.run(command, input=request, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(len((raw/'requests.ndjson').read_bytes().splitlines()), 1)
            cold = root/'cold'; cold.mkdir()
            for name in ('session_stop.py','record_hook.py','protocol.json'): shutil.copyfile(KIT/name, cold/name)
            for name in ('decisions.ndjson','session-context.json','session-stop.json'): shutil.copyfile(raw/name, cold/name)
            original = {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in raw.iterdir() if p.is_file()}
            def replay():
                return subprocess.run([sys.executable, str(cold/'session_stop.py'), str(cold)], capture_output=True, timeout=5, cwd=cold)
            checked = replay()
            self.assertEqual(checked.returncode, 0, checked.stderr)
            self.assertEqual(json.loads(checked.stdout)['status'], 'closed')
            (cold/'decisions.ndjson').write_bytes((cold/'decisions.ndjson').read_bytes().replace(b'policy_denied',b'policy_changed'))
            damaged = replay()
            self.assertEqual(damaged.returncode, 1)
            self.assertEqual(json.loads(damaged.stdout)['status'], 'unknown')
            self.assertEqual(original, {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in raw.iterdir() if p.is_file()})
            self.assertFalse((cold/'oracle').exists())

if __name__ == '__main__': unittest.main()
