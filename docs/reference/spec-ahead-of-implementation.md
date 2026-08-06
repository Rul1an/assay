# Spec ahead of implementation

Mechanisms that are **specified, typed, or documented in this repository but not yet exercised end to
end**. One line each, with what is missing and what would close it.

The practice is borrowed from `draft-schrock-ep-authorization-receipts-07` §11.5, which names in the
draft itself the normative mechanisms its reference implementation does not yet exercise, and tells
implementers to *"treat the text as normative and the reference implementation as incomplete on these
three points, not the reverse."* Naming the gap is cheaper than having a user find it.

**What this list is not.** Not a roadmap, not a wishlist, not a backlog. Every entry describes
something that already exists in the tree and does not yet do the whole job it names. An entry leaves
when a test exercises it end to end, not when someone intends to finish it.

**A "reserved" enum variant belongs here.** A value that can be deserialized but is never emitted is
spec ahead of implementation, even though nothing is broken. A consumer writing a match arm for it is
writing dead code and cannot tell.

---

## Open

| mechanism | where | what is missing | closed by |
|---|---|---|---|
| `IrreversibilityClass` | `assay-mcp-server/src/side_effect.rs` | The type, the ordering, `demands_evidence` and `unevidenced_and_consequential` are implemented and unit-tested, and **no producer populates the field**. Every emitted side-effect record leaves it absent, which the type correctly reads as *unknown* rather than as `two_way` | a classifier mapping an action to a class at the point the decision record is built |
| `decode_connect_sockaddr` | `assay-monitor/src/events.rs` | Implemented and tested, and **deliberately not wired**. It decodes the `sys_enter_connect` tracepoint payload, which is read from userspace memory and therefore unsound as a refutation input. Documented at the definition | nothing — this is intentional, and the entry exists so the gap is not mistaken for an oversight |
| connect6 egress enforcement | `assay-ebpf/src/socket_lsm.rs` | The IPv6 hook mirrors connect4 and is correct by construction, but `loader.rs` populates only `CIDR_RULES_V4` and the monitor refuses IPv6 policies outright, so the path has never executed | populating the v6 map, or removing the hook |
| `NetworkEnforcement::NotApplicable` | `assay-cli/src/cli/commands/monitor_next/enforcement_health.rs` | Reserved, never emitted by the connect4 producer. A consumer cannot distinguish "reserved" from "possible but unseen" | a producer whose scope makes the variant reachable |
| manifest-drift **vectors** | `assay-mcp-server/tests/mcp_manifest_drift_fixtures.rs` | The drift *gate* is production (`proxy/enforce.rs` c3, `manifest_drifted_since_approval`). These committed **vectors** have no producer or consumer, per their own header | a producer emitting the drift record in that shape |

---

## Closed, kept briefly so the list is falsifiable

| mechanism | was | closed |
|---|---|---|
| Side-effect ladder verifier (Eb) | `side_effect_fixtures.rs` declared *"no producer/verifier yet"* | 2026-08-06 — `check_audit_record` + `promote_with_audit_record`, reachable via `assay evidence verify-side-effects --audit-import`. **The header still said otherwise for hours after it shipped**, which is the argument for keeping this file |
| `SideEffectLevel::Verified` from the CLI | reachable only from library code | 2026-08-06 — `--audit-import`, three CLI tests |
| Probe attachment record | the monitor could not report which probes attached | 2026-08-06 — `ProbeAttachment` reconciled against `EXPECTED_PROBES`, verified on a live kernel |
| `assay.enforcement_health.v1` producer | header said *"No producer wires it up yet"* | **already closed before this list existed.** `sandbox/child.rs` emits it on both the active and failed Landlock paths. Header corrected |
| tool-decision establish path | header says items are *"not yet wired into the binary's run path"* with `allow(dead_code)` | **already closed.** All three public functions — `establish_action`, `establish_path`, `build_manifest_establish_record` — have production callers. What remains deferred to Increment 2 is narrower than the header implies |

---

## How to use it

- **Adding a mechanism before its producer is fine.** A carrier with fixtures and no producer is a
  legitimate way to pin a contract. Add the entry in the same change.
- **Do not cite an entry as a limitation of the design.** These are unfinished, not unsound. Where
  something is intentionally unwired, the row says so and says why.
- **Verify every row against the tree before adding it.** Three of the six rows in the first draft of
  this file were false or imprecise, and all three came from believing a stale source-header comment
  instead of grepping for callers. The list is only worth its cost if each row is checked, and the
  irony of a spec-ahead list assembled from stale prose is the reason this bullet exists.
- **An entry that goes stale is worse than no entry.** This file went stale once before it existed:
  the Eb row above sat wrong in a source header while the verifier was already shipping. Check the
  list when closing anything on it.
