#!/usr/bin/env python3
"""Exercise the packaged SARIF recipient cases with a supplied Assay binary."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assay", required=True, help="Path to the binary to measure")
    parser.add_argument("--output", required=True, type=Path, help="New receipt directory")
    args = parser.parse_args()
    binary = Path(args.assay).resolve(strict=True)
    if not binary.is_file():
        parser.error("--assay must name a binary file")
    binary_digest = digest(binary)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = {
        "status": "failed",
        "binary": str(binary),
        "binary_sha256": binary_digest,
        "example_sha256": digest(Path(__file__)),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "non_claim": "Source correlation only; no producer authentication or release provenance.",
    }

    def invoke(name, arguments, expected=0):
        argv = [str(binary), *arguments]
        item = {"name": name, "argv": argv, "cwd": str(output)}
        receipt["commands"].append(item)
        # Retain bytes even when the command fails or times out.
        with (output / f"{name}.stdout").open("xb") as stdout, (
            output / f"{name}.stderr"
        ).open("xb") as stderr:
            result = subprocess.run(
                argv, cwd=output, stdout=stdout, stderr=stderr, timeout=30
            )
        item["exit"] = result.returncode
        require(result.returncode == expected, f"{name}: expected exit {expected}, got {result.returncode}")
        return (output / f"{name}.stdout").read_bytes()

    try:
        fixtures = Path(__file__).resolve().parent / "fixtures"
        pins = json.loads((fixtures / "SHA256SUMS.json").read_text(encoding="utf-8"))
        for name in ("source.tar.gz", "other-valid.tar.gz"):
            require(digest(fixtures / name) == pins[name], f"Fixture digest differs: {name}")
            shutil.copyfile(fixtures / name, output / name)
        receipt["fixture_sha256"] = pins
        receipt["version"] = invoke("version", ["--version"]).decode().strip()
        shows = {}
        for name, bundle in (("source", "source.tar.gz"), ("other", "other-valid.tar.gz")):
            shows[name] = json.loads(invoke(name, ["evidence", "show", bundle, "--format", "json"]))
            require(shows[name]["verify_mode"] == "enabled", f"{name}: verification disabled")
        source_id = shows["source"]["manifest"]["bundle_id"]
        other_id = shows["other"]["manifest"]["bundle_id"]
        require(source_id and other_id and source_id != other_id, "Control must have a distinct verified identity")

        runs = {}
        for name, extra in (
            ("clean", []),
            ("finding", ["--pack", "soc2-baseline"]),
            ("capped", ["--pack", "soc2-baseline", "--max-results", "0"]),
        ):
            document = json.loads(invoke(name, [
                "evidence", "lint", "source.tar.gz", "--format", "sarif",
                "--fail-on", "none", *extra,
            ]))
            require(document["version"] == "2.1.0" and len(document["runs"]) == 1, f"{name}: unexpected SARIF shape")
            run = document["runs"][0]
            require(run["properties"].get("bundle_id") == source_id, f"{name}: source identity missing or different")
            require(run["properties"]["bundle_id"] != other_id, f"{name}: unrelated bundle matched")
            require(run["invocations"][0]["executionSuccessful"] is True, f"{name}: lint did not complete")
            runs[name] = run

        require(runs["clean"]["results"] == [], "Expected clean control")
        findings = runs["finding"]["results"]
        rule_id = "soc2-baseline@1.0.0:invariant.decision_record"
        require(len(findings) == 1 and findings[0]["ruleId"] == rule_id, "Expected missing decision-record finding")
        require(findings[0]["properties"]["bundle_id"] == source_id, "Finding and run identity differ")
        require(any(rule["id"] == rule_id for rule in runs["finding"]["tool"]["driver"]["rules"]), "Finding lacks rule metadata")
        require(not any(event["type"] == "assay.tool.decision" for event in shows["source"]["events"]), "Fixture unexpectedly contains a decision")
        capped = runs["capped"]
        require(capped["results"] == [], "Zero cap must emit no results")
        require(capped["properties"]["truncated"] is True, "Cap must disclose truncation")
        require(capped["properties"]["appliedCap"] == 0, "Expected zero cap")
        require(capped["properties"]["truncatedCount"] == len(findings), "Incorrect dropped count")
        notes = [note for note in capped["invocations"][0]["toolExecutionNotifications"]
                 if note["descriptor"]["id"] == "ASSAY-LINT-TRUNCATED"]
        require(len(notes) == 1, "Expected one truncation notification")
        require(notes[0]["level"] == "warning", "Truncation notification must warn")
        require(notes[0]["properties"]["droppedCount"] == len(findings), "Notification dropped count differs")

        # Deliberately truncate the packaged archive; do not rewrite canonical data.
        (output / "damaged.tar.gz").write_bytes((output / "source.tar.gz").read_bytes()[:16])
        for name, bundle in (("damaged", "damaged.tar.gz"), ("missing", "absent.tar.gz")):
            raw = invoke(name, ["evidence", "lint", bundle, "--format", "sarif", "--fail-on", "none"], expected=2)
            require(raw == b"", f"{name}: refusal emitted SARIF stdout")
            require((output / f"{name}.stderr").stat().st_size > 0, f"{name}: refusal lacks diagnostic")
        receipt["source_bundle_id"] = source_id
        receipt["unrelated_bundle_id"] = other_id
        receipt["status"] = "passed"
    except (ValueError, KeyError, IndexError, TypeError, OSError, subprocess.SubprocessError) as error:
        receipt["error"] = str(error)
    finally:
        receipt["artifact_sha256"] = {
            path.name: digest(path) for path in sorted(output.iterdir()) if path.is_file()
        }
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"{receipt['status']}: {output / 'receipt.json'}")
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
