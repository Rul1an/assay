#!/usr/bin/env python3
"""Mutation check for the octets consumer: a rule that no case exercises is decoration.

Each rule in `consume_octets` is silenced in turn by disabling its guard, and the run is re-executed.
A rule is KILLED when at least one case changes verdict. Two conditions are required for a kill to
count, and the second is the one that is easy to lose: the mutation must NOT blind the positive
control, so case C (the clean resolution over the untouched record) must still reach `recomputed`
under every mutation that is not the digest rule itself. A mutation that turns everything red proves
the case set reacts to damage, not that the rule discriminates.

Scope: this mutates the octets consumer in `resolve.py` only. Cases A and B run through the published
June consumer, which this experiment must not modify, so they are outside the mutation set.

Usage: python3 mutate.py    # writes runs/mutation.json, exit 0 iff every rule is killed
"""
from __future__ import annotations

import copy
import json
import pathlib
import sys

import resolve as ref

HERE = pathlib.Path(__file__).parent

# Each entry disables exactly one guard by making its condition unreachable.
MUTATIONS = {
    "malformed_ref": ("if not ref.get(\"digest\") or not ref.get(\"canonicalization\"):", "if False:"),
    "unresolvable_digest_only": ("if locator is None:", "if False:"),
    "unresolved_ref": ("if locator not in octet_store:", "if False:"),
    "unsupported_canonicalization": ("if not isinstance(canon, str) or canon != OCTETS_PROFILE:", "if False:"),
    "digest_mismatch": ("if recomputed != ref[\"digest\"]:", "if False:"),
    "unknown_schema": ("if spec is None:", "if False:"),
    "redacted_projection_incomplete": ("if incomplete:", "if False:"),
}

def octet_case_ids(build=ref.build_cases) -> tuple:
    """Derived, never hand-listed. A literal roster here would restate machine state and go stale the
    moment a case is added, which is exactly how a rule stays unexercised while the report reads green."""
    return tuple(c["id"] for c in build() if c["mode"] == "octets_consumer")


OCTET_CASES = octet_case_ids()


def _baseline() -> dict:
    cases = ref.build_cases()
    out = {}
    for c in cases:
        if c["id"] in OCTET_CASES:
            out[c["id"]] = ref.consume_octets(c["ref"], {"gate-record": c["octets"]})["verdict"]
    return out


def _mutated_consume(old: str, new: str):
    src = (HERE / "resolve.py").read_text()
    if src.count(old) != 1:
        raise SystemExit(f"mutation target not unique or missing: {old!r}")
    ns: dict = {"__file__": str(HERE / "resolve.py"), "__name__": "resolve_mutant"}
    exec(compile(src.replace(old, new), "<mutant>", "exec"), ns)  # noqa: S102  local mutation harness
    return ns["consume_octets"], ns["build_cases"]


def main() -> int:
    base = _baseline()
    if base["C_octets_profile_named"] != "recomputed":
        print("baseline control is not clean; refusing to report kills")
        return 1

    report = {"baseline": base, "rules": {}, "all_killed": None}
    for rule, (old, new) in MUTATIONS.items():
        consume, build = _mutated_consume(old, new)
        got = {}
        for c in build():
            if c["mode"] == "octets_consumer":
                try:
                    got[c["id"]] = consume(c["ref"], {"gate-record": c["octets"]})["verdict"]
                except Exception as exc:  # a rule whose removal crashes is still exercised
                    got[c["id"]] = f"raised:{type(exc).__name__}"
        changed = sorted(k for k in base if got.get(k) != base[k])
        # The control must survive every mutation except the one that removes the digest rule the
        # control itself depends on. Otherwise the mutation blinded the control and the kill is void.
        control_ok = got.get("C_octets_profile_named") == "recomputed" or rule == "digest_mismatch"
        report["rules"][rule] = {
            "killed_by": changed,
            "killed": bool(changed),
            "control_preserved": control_ok,
            "valid_kill": bool(changed) and control_ok,
        }

    report["all_killed"] = all(r["valid_kill"] for r in report["rules"].values())
    (HERE / "runs").mkdir(exist_ok=True)
    (HERE / "runs" / "mutation.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    for rule, r in report["rules"].items():
        mark = "killed " if r["valid_kill"] else "SURVIVED"
        print(f"{mark} {rule:34s} by={','.join(r['killed_by']) or '(none)':60s} control={r['control_preserved']}")
    print(f"\nall_killed={report['all_killed']}  ({len(MUTATIONS)} rules)")
    return 0 if report["all_killed"] else 1


if __name__ == "__main__":
    sys.exit(main())
