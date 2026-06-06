# ADR-035: Sandbox-the-Agent Evidence Path

## Status
Proposed (June 2026)

Depends on ADR-034 (Assay / Runner / Harness contract seam).

## Context

Coding agents and other autonomous agents run shell commands, edit files, and reach
the network with broad blast radius. Editor-native permission prompts are
self-reported and editor-specific, so they do not give an independent, portable
record of what an agent actually did.

Assay already ships `assay sandbox` (Landlock-based), which runs a command under
containment and, with `--profile`, emits a content-addressed evidence profile of the
observed effects (filesystem operations, executed programs, counters, containment
degradations) with a deterministic `sandbox_<sha256-prefix>` run id, plus a suggested
policy and a human report.

## Decision

Document and support running a coding agent under `assay sandbox -- <agent command>`
as a first-class governance path that produces an independent, deterministic record
of the agent's observed effects, with optional inline enforcement (`--enforce`,
`--fail-closed`) and an observe-only mode (`--dry-run`).

Frame the evidence dimensions around the three controls that matter most for an
autonomous agent: network egress, file writes, and configuration protection.

Consume the record via the contract seam (ADR-034). Promoting the evidence profile
into the canonical evidence bundle consumed by `assay evidence lint` / `diff`, and a
matching Assay-Harness recipe, gate, and report, is the next slice; until then the
evidence profile is consumed directly.

## Consequences

- An independent, cross-editor, deterministic record of an agent run, most valuable
  in CI and cloud-autonomous runs where an audit trail beats an interactive prompt.
- A documented governance workflow over already-shipping code; no new subsystem.
- A follow-up is needed to converge the evidence-profile artifact with the canonical
  evidence-bundle format behind the contract seam.

## Best-practice basis (2026)

- Landlock / seccomp for lightweight unprivileged in-process containment.
- microVM / gVisor for isolating genuinely untrusted code; Assay composes on top.
- Independent observation as the cross-check on self-reported permissions.
- The three mandatory controls for autonomous agents: network egress, file-write
  restrictions, configuration protection.

## Non-claims

- Landlock is not VM-level isolation. For untrusted code, run the agent inside a
  microVM or gVisor and use Assay for the record and the gate on top.
- Assay does not prevent prompt injection. There is no deterministic prevention for
  prompt injection; the only provable defense is environment isolation. Assay
  observes, records, and gates.
- The evidence profile records observed effects from Assay's vantage; it is not a
  proof of intent.

## References

- `crates/assay-cli/src/cli/commands/sandbox.rs`
- `docs/guides/coding-agent-governance.md`
- ADR-034 (contract seam)
