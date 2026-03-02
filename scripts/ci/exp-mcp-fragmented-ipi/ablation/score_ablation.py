#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load(path):
    with Path(path).open(encoding="utf-8") as handle:
        return json.load(handle)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    modes = ["wrap_only", "sequence_only", "combined"]
    summary = {
        "schema_version": "exp_mcp_fragmented_ipi_ablation_summary_v1",
        "modes": {},
        "notes": [
            "Per-mode summaries are produced by score_runs.py",
            "Causal attribution compares wrap_only, sequence_only, and combined",
        ],
    }

    for mode in modes:
        path = Path(args.root) / mode / "summary.json"
        summary["modes"][mode] = load(path) if path.exists() else {"missing": True}

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")


if __name__ == "__main__":
    main()
