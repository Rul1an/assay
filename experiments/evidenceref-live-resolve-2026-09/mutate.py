#!/usr/bin/env python3
"""Mutation check for the octets consumer: a rule that no case exercises is decoration.

Each rule in `consume_octets` is silenced in turn by disabling its guard, and the run is re-executed.
A rule is KILLED when at least one case changes verdict. Three conditions are required for the kill
to count: the mutation moved a case, the positive control survived it, and the control was shown able
to fail at all. Case C, the clean resolution over the untouched record, must still reach `recomputed`
under EVERY mutation, with no per-rule exception; an earlier version exempted the digest rule here and
that exemption was dead code that only removed the control where blinding is most plausible. A
mutation that turns everything red proves the case set reacts to damage, not that the rule
discriminates.

Scope: this mutates the octets consumer in `resolve.py` only. Cases A and B run through the published
June consumer, which this experiment must not modify, so they are outside the mutation set.

A kill is decided in exactly one place, `kill_verdict`, and rule identity is not one of its inputs.

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


def control_survived(observations: dict) -> bool:
    """Did the positive control survive this mutation?

    The observations are the only input. Rule identity is deliberately absent from the signature,
    so a per-rule exemption cannot be written here without changing it, and a test can say so.
    An earlier version exempted the digest rule at the call site on the theory that silencing it
    blinds the control; measured, it does not, and the exemption only removed the control from the
    one rule where blinding is most plausible.
    """
    return observations.get("C_octets_profile_named") == "recomputed"


def kill_verdict(changed: bool, control_ok: bool, probe_fired: bool) -> bool:
    """The single place a kill is decided.

    A kill counts only when the mutation moved a case, the control survived it, AND the control
    was shown able to fail at all. Rule identity is deliberately not a parameter: an earlier
    version exempted one rule from the control check here, which is how a control acquires a
    hole that no test can see.
    """
    return bool(changed) and bool(control_ok) and bool(probe_fired)


def blinded_control_probe() -> dict:
    """Must-fail probe: a mutation that blinds the positive control has to be refused, never counted.

    Without it the control check is itself untested, and `control_preserved: true` on every row
    would be indistinguishable from a control that cannot go false. This mutation makes the clean
    case non-clean, so every rule must come back with valid_kill false.
    """
    consume, build = _mutated_consume(
        'return _v("recomputed", "octets_match_address_under_declared_profile_and_schema_complete")',
        'return _v("digest_mismatch", "blinded-control probe")',
    )
    got = {}
    for c in build():
        if c["mode"] == "octets_consumer":
            try:
                got[c["id"]] = consume(c["ref"], {"gate-record": c["octets"]})["verdict"]
            except Exception as exc:
                got[c["id"]] = f"raised:{type(exc).__name__}"
    control = got.get("C_octets_profile_named")
    return {
        "control_verdict_under_probe": control,
        "control_detected_as_blinded": control != "recomputed",
    }


def main() -> int:
    # Computed before any rule is scored: a report whose control cannot fail scores nothing.
    probe = blinded_control_probe()
    probe_fired = probe["control_detected_as_blinded"]
    base = _baseline()
    if base["C_octets_profile_named"] != "recomputed":
        print("baseline control is not clean; refusing to report kills")
        return 1

    report = {
        "baseline": base,
        "rules": {},
        "all_killed": None,
        "blinded_control_probe": probe,
    }
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
        crashed = sorted(k for k in changed if str(got.get(k)).startswith("raised:"))
        # The control must survive EVERY mutation, with no exception; see control_survived.
        control_ok = control_survived(got)
        report["rules"][rule] = {
            "killed_by": changed,
            "crash_kills": crashed,
            "verdict_kills": sorted(set(changed) - set(crashed)),
            "killed": bool(changed),
            "control_preserved": control_ok,
            "valid_kill": kill_verdict(bool(changed), control_ok, probe_fired),
        }

    report["all_killed"] = all(r["valid_kill"] for r in report["rules"].values())
    if not probe_fired:
        print("BLINDED-CONTROL PROBE DID NOT FIRE: the control cannot go false; refusing to score")
    (HERE / "runs").mkdir(exist_ok=True)
    (HERE / "runs" / "mutation.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    for rule, r in report["rules"].items():
        mark = "killed " if r["valid_kill"] else "SURVIVED"
        how = "crash" if r["crash_kills"] and not r["verdict_kills"] else "verdict"
        print(f"{mark} {rule:34s} {how:7s} by={','.join(r['killed_by']) or '(none)':52s} control={r['control_preserved']}")
    print(f"\nblinded-control probe: control reads {probe['control_verdict_under_probe']!r}, "
          f"detected={probe['control_detected_as_blinded']}")
    print(f"all_killed={report['all_killed']}  ({len(MUTATIONS)} rules)")
    return 0 if report["all_killed"] else 1


if __name__ == "__main__":
    sys.exit(main())
