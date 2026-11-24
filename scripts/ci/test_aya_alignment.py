"""Exercise the manifests consumed by the dependency-security job."""
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class AyaAlignment(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "scripts/ci").mkdir(parents=True)
        (self.root / "crates/assay-ebpf").mkdir(parents=True)
        self.checker = self.root / "scripts/ci/check_aya_alignment.py"
        shutil.copyfile(Path(__file__).with_name(self.checker.name), self.checker)
        (self.root / "Cargo.toml").write_text('[workspace.dependencies]\naya = "=0.16.0"\naya-log = "=0.4.0"\n')

        (self.root / "Cargo.lock").write_text('[[package]]\nname = "object"\nversion = "0.40.0"\n')

    def check(self, second, expected):
        manifest = '[dependencies]\naya-ebpf = { tag = "aya-v0.16.0" }\n' + second + '\n'
        (self.root / "crates/assay-ebpf/Cargo.toml").write_text(manifest)
        result = subprocess.run([sys.executable, self.checker], capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_duplicate_object(self):
        with (self.root / "Cargo.lock").open("a") as lock:
            lock.write('[[package]]\nname = "object"\nversion = "0.39.0"\n')
        self.check("aya-log-ebpf = { tag = 'aya-v0.16.0' }", 1)

    def test_equal_tags(self):
        self.check("aya-log-ebpf = { tag = 'aya-v0.16.0' }", 0)

    def test_other_minor_single_quoted(self):
        self.check("aya-log-ebpf = { tag = 'aya-v0.13.0' }", 1)

    def test_other_patch(self):
        self.check('aya-log-ebpf = { tag = "aya-v0.16.1" }', 1)

    def test_missing_dependency(self):
        self.check("", 1)

    def test_wrong_log_line(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace("=0.4.0", "=0.3.0"))
        self.check('aya-log-ebpf = { tag = "aya-v0.16.0" }', 1)

    def test_missing_tag(self):
        self.check('aya-log-ebpf = { branch = "main" }', 1)


if __name__ == "__main__":
    unittest.main()
