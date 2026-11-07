"""Run the fixed synthetic fixture cohort and existing CA process runner; no live calls."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from adapter import evaluate
from build_parser import MODULES, PIN, resource_gate

HERE = Path(__file__).resolve().parent
CA_PIN = "6d1ff19827744934e57c0b799baa49b525a361e7"
CHECKER = HERE / "closure_check.py"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    assay_repo, ca_repo, build_dir, output = map(Path, sys.argv[1:])
    output.mkdir(exist_ok=False)
    binary = build_dir / "target/debug/closure-parser-probe"
    assert (build_dir / "src/main.rs").read_bytes() == (
        HERE / "parser_main.rs"
    ).read_bytes()
    assert (build_dir / "Cargo.lock").read_bytes() == (HERE / "Cargo.lock").read_bytes()
    modules = {}
    for name in MODULES:
        path = f"crates/assay-core/src/mcp/{name}.rs"
        raw = subprocess.check_output(
            ["git", "-C", str(assay_repo), "show", f"{PIN}:{path}"]
        )
        local = build_dir / f"src/mcp/{name}.rs"
        assert local.read_bytes() == raw
        modules[path] = digest(local)
    # Repository cleanliness is not tool identity; compare every declared CA runtime source.
    ca_files = (
        "corpus_adequacy.py",
        "bounded_run.py",
        "execution_observation.py",
        "observation_session.py",
        "isolated_tree.py",
        "module_child.py",
    )
    for name in ca_files:
        assert (ca_repo / name).read_bytes() == subprocess.check_output(
            ["git", "-C", str(ca_repo), "show", f"{CA_PIN}:{name}"]
        )
    swap = resource_gate()
    os.environ["CLOSURE_PARSER"] = str(binary)
    os.environ["CLOSURE_PARSER_SHA256"] = digest(binary)
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    results = []
    fixtures = sorted((HERE / "fixtures").glob("*.json"))
    for path in fixtures:
        case = json.loads(path.read_bytes())
        result = evaluate(case["input"], str(binary))
        assert [result["outcome"], result["reason"]] == case["expected"], path
        results.append(
            {"case": path.stem, "fixture_sha256": digest(path), "result": result}
        )
    receipt = {
        "assay_commit": PIN,
        "ca_commit": CA_PIN,
        "modules": modules,
        "rustc": subprocess.check_output(
            ["rustc", "--version", "--verbose"], text=True
        ),
        "features": subprocess.check_output(
            [
                "cargo",
                "tree",
                "--locked",
                "--offline",
                "-e",
                "features",
                "--manifest-path",
                str(build_dir / "Cargo.toml"),
            ],
            text=True,
        ),
        "wrapper_sha256": digest(HERE / "parser_main.rs"),
        "lock_sha256": digest(HERE / "Cargo.lock"),
        "parser_sha256": digest(binary),
        "adapter_sha256": digest(HERE / "adapter.py"),
        "checker_sha256": digest(CHECKER),
        "swap_before": swap.strip(),
        "claim_scope": "synthetic Assay-declared MCP call-result closure",
        "results": results,
    }
    (output / "result.json").write_text(json.dumps(receipt, indent=2) + "\n")
    with tempfile.TemporaryDirectory(prefix="closure-ca-") as tmp:
        root = Path(tmp)
        (root / "probe").mkdir()
        shutil.copy2(HERE / "adapter.py", root / "probe/adapter.py")
        checker_dir = root / "probe"
        shutil.copy2(CHECKER, checker_dir / "closure_check.py")
        shutil.copytree(HERE / "fixtures", root / "fixtures")
        vectors = {
            "vectors": [
                {"vector_id": p.stem, "path": "fixtures/" + p.name} for p in fixtures
            ]
        }
        (root / "vectors.json").write_text(json.dumps(vectors))
        manifest = json.loads((HERE / "manifest.json").read_bytes())
        (root / "manifest.json").write_text(json.dumps(manifest))
        run = subprocess.run(
            [
                sys.executable,
                str(ca_repo / "corpus_adequacy.py"),
                str(root / "manifest.json"),
                "--json",
            ],
            check=False,
            capture_output=True,
            timeout=120,
        )
        (output / "ca-report.json").write_bytes(run.stdout)
        (output / "ca-stderr.txt").write_bytes(run.stderr)
        report = json.loads(run.stdout)
        assert run.returncode == 0 and report["adequate"] is True, report
        assert report["control_status"] == "killed" and report["unproved"] == 0
        assert all(
            row["verdict"] in ("killed", "control-killed", "control-unchanged")
            for row in report["mutants"]
        )
    print(
        f"{len(results)} native cases checked; CA declared mutations and controls passed"
    )


if __name__ == "__main__":
    main()
