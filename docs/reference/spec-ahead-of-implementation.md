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
| `decode_connect_sockaddr` | `assay-monitor/src/events.rs` | Implemented and tested. **Wired for display/rendering** in `monitor_next/output.rs` (`EVENT_CONNECT`). **Not wired as refutation input**: the `sys_enter_connect` payload is copied from userspace memory at syscall entry. Documented at the definition | nothing for the refutation gap — that is intentional |
| `connect6_hook` | `assay-ebpf` ELF; `assay-monitor/src/probes.rs` | Compiled-but-unattached IPv6 **enforcement** (`cgroup_sock_addr:connect6`, `AttachSpec::None`). Not the connect observer: `sys_enter_connect` is always attached and handles `AF_INET6`. Distinct from the CIDR-map gap below | attaching the program, or removing it from the compiled program set |
| connect6 egress enforcement | `assay-ebpf/src/socket_lsm.rs` | The IPv6 hook mirrors connect4 and is correct by construction, but `loader.rs` populates only `CIDR_RULES_V4` and the monitor refuses IPv6 policies outright, so the path has never executed. `connect6_hook` is also compiled-but-unattached enforcement (row above) | populating the v6 map, or removing the hook |
| `NetworkEnforcement::NotApplicable` | `assay-cli/src/cli/commands/monitor_next/enforcement_health.rs` | Reserved, never emitted by the connect4 producer. A consumer cannot distinguish "reserved" from "possible but unseen" | a producer whose scope makes the variant reachable |
| manifest-drift **vectors** | `assay-mcp-server/tests/mcp_manifest_drift_fixtures.rs` | The drift *gate* is production (`proxy/enforce.rs` c3, `manifest_drifted_since_approval`). These committed **vectors** have no producer or consumer, per their own header | a producer emitting the drift record in that shape |

---

## Closed, kept briefly so the list is falsifiable

| mechanism | was | closed |
|---|---|---|
| Side-effect ladder verifier (Eb) | `side_effect_fixtures.rs` declared *"no producer/verifier yet"* | 2026-08-06 — `check_audit_record` + `promote_with_audit_record`, reachable via `assay evidence verify-side-effects --audit-import`. **The header still said otherwise for hours after it shipped**, which is the argument for keeping this file |
| `SideEffectLevel::Verified` from the CLI | reachable only from library code | 2026-08-06 — `--audit-import`, three CLI tests |
| `IrreversibilityClass` producer | typed and tested with no producer populating the field | 2026-08-06 — `tool_decision::irreversibility_for(category)` mirrors `required_scope_for`, attached in `build_decision`. Unclassified tools still omit the field, and a test pins the two category tables against each other so adding to one and not the other fails |
| Probe attachment record | the monitor could not report which probes attached | 2026-08-06 — `ProbeAttachment` reconciled against `EXPECTED_PROBES`, verified on a live kernel |
| `assay_monitor_sendto` / `assay_monitor_sendmsg` attach and readback | programs were compiled but unattached, and the four `no_peer` / `non_ip` counters had no userspace readback | 2026-08-13 — both tracepoints are always attempted with independent terminal outcomes; userspace projects all four counters. Exact-head ARM attach/readback proof: [run 31669849737](https://github.com/Rul1an/assay/actions/runs/31669849737). Datagram labels still require a non-zero send-event count; this is not an io_uring `SEND` / `SENDMSG` claim |
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
