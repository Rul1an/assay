#!/usr/bin/env python3
"""Render the mutation-detection matrix and verification-cost curve as Markdown tables.

Stdlib only. Reads results/matrix.json and results/cost.json (produced by the Rust harnesses)
and writes results/matrix.md and results/cost.md.

Usage:
    python3 aggregate.py [--results-dir DIR]
"""
import argparse
import json
import os


def render_cost(cost):
    rows = cost["rows"]
    out = []
    out.append("# Verification + signing cost curve\n")
    out.append(f"Profile: `{cost['profile']}` · payload {cost['payload_bytes_per_event']} bytes/event\n")
    out.append("| events | verify ms (median) | reps | compressed bytes | gzip ratio | bytes/event | inclusion-proof hashes |")
    out.append("| --- | --- | --- | --- | --- | --- | --- |")
    for r in rows:
        out.append(
            f"| {r['events']:,} | {r['verify_ms_median']:.3f} | {r['verify_reps']} | "
            f"{r['compressed_bytes']:,} | {r['gzip_ratio']:.4f} | "
            f"{r['bytes_per_event_compressed']:.2f} | {r['inclusion_proof_hashes']} |"
        )
    fit = cost["fit"]
    out.append("")
    out.append("## Linear fit (verify_ms ~ a + b·events)\n")
    out.append(f"- slope: {fit['slope_ms_per_event']:.6f} ms/event ({fit['ms_per_1k_events']:.3f} ms per 1k events)")
    out.append(f"- intercept: {fit['intercept_ms']:.4f} ms")
    out.append(f"- r²: {fit['r2']:.6f}")
    dsse = cost["dsse"]
    out.append("")
    out.append("## DSSE over the run anchor\n")
    out.append(f"- sign: {dsse['sign_ms_median']:.4f} ms (median, {dsse['reps']} reps)")
    out.append(f"- verify: {dsse['verify_ms_median']:.4f} ms (median, {dsse['reps']} reps)")
    out.append("")
    return "\n".join(out)


def render_matrix(matrix):
    out = []
    out.append("# Mutation-detection matrix\n")
    out.append(f"Threat model: {matrix['threat_model']}\n")
    out.append(f"Base events: {matrix['base_events']}\n")
    out.append("| class | layer | detected | no-op | manifest-meta-only | bypass | dominant verifier code |")
    out.append("| --- | --- | --- | --- | --- | --- | --- |")
    for c in matrix["classes"]:
        if c.get("layer") == "internal-verifier":
            out.append(
                f"| {c['class']} | internal-verifier | {c['detected']} | {c['noop']} | "
                f"{c['manifest_meta_only']} | {c['bypass']} | `{c['dominant_code']}` |"
            )
        else:
            out.append(
                f"| {c['class']} | {c['layer']} | "
                f"{'anchor-detected' if c.get('detected_by_anchor') else 'NOT detected'} | - | - | - | "
                f"run_root change |"
            )
    gate = matrix["gate"]
    out.append("")
    out.append("## Gate\n")
    out.append(f"- event-evidence bypasses: **{gate['total_bypass']}**")
    out.append(f"- manifest-meta-only (documented limitation): {gate.get('total_manifest_meta_only', 0)}")
    out.append("")
    out.append("## Documented limitation\n")
    out.append(matrix.get("documented_limitation", ""))
    out.append("")
    return "\n".join(out)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--results-dir", default=os.path.join(here, "results"))
    args = ap.parse_args()

    with open(os.path.join(args.results_dir, "cost.json")) as f:
        cost = json.load(f)
    with open(os.path.join(args.results_dir, "matrix.json")) as f:
        matrix = json.load(f)

    cost_md = render_cost(cost)
    matrix_md = render_matrix(matrix)

    with open(os.path.join(args.results_dir, "cost.md"), "w") as f:
        f.write(cost_md)
    with open(os.path.join(args.results_dir, "matrix.md"), "w") as f:
        f.write(matrix_md)

    print(matrix_md)
    print(cost_md)


if __name__ == "__main__":
    main()
