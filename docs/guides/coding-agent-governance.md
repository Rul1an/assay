# Coding-Agent Governance: an independent record of what the agent did

Coding agents (Claude Code, Cursor, Codex, and cloud-autonomous runners) run shell,
edit files, and reach the network with broad blast radius. Their editor-native
permission prompts are self-reported and editor-specific. Assay gives you an
independent, deterministic record of what an agent actually did, plus an optional
policy gate, by running the agent under `assay sandbox`.

## What you get

Run any command (including a coding-agent CLI) under the sandbox with profiling on:

```bash
assay sandbox \
  --profile run.profile.yaml \
  --profile-report run.report.md \
  -- <your coding-agent command> [args...]
```

This produces three artifacts:

- `run.profile.yaml` — a suggested policy derived from the observed behavior.
- `run.profile.evidence.yaml` — a content-addressed **evidence profile** of the
  observed effects (filesystem operations, executed programs, counters, and any
  containment degradations), with a deterministic run id of the form
  `sandbox_<sha256-prefix>`. Re-running the same command over the same behavior
  yields the same run id.
- `run.profile.report.md` — a human-readable summary.

Use `--profile-format json` for JSON instead of YAML.

## Enforcement modes

The sandbox uses Landlock when available:

- default: containment is active when Landlock is present.
- `--dry-run`: observe and log only, never block (exits 4 if an unauthorized action
  occurs). Best for the first run while you learn the agent's footprint.
- `--enforce`: require active enforcement; combine with `--fail-closed` to make an
  unenforceable policy fatal (exit 2) rather than degrading to audit.

Pass a policy with `--policy assay.yaml`; without one, a minimal default applies.
Environment scrubbing is on by default (`--env-strict`, `--env-strip-exec`,
`--env-allow a,b`, `--env-safe-path` to tune; `--env-passthrough` is the unsafe
escape hatch).

## The three controls

The observed effects map to the three controls that matter most for an autonomous
agent:

- **Network egress** — NET allow/deny rules; observed connection attempts.
- **File writes** — FS allow/deny rules; observed filesystem operations in the
  evidence profile.
- **Configuration protection** — deny rules over sensitive paths.

## Honest limits (read this)

- Landlock is a lightweight in-kernel containment layer. It is **not** VM-level
  isolation. For genuinely untrusted code, run the agent inside a microVM or gVisor
  and use Assay for the independent record and policy gate on top of that isolation.
- Assay does **not** prevent prompt injection. There is no deterministic prevention
  for prompt injection; the only provable defense is isolating the environment. Assay
  observes, records, and gates; it does not make an injected instruction safe.
- The evidence profile is the agent's observed effects from Assay's vantage, not a
  proof of intent.

Most useful in CI and cloud-autonomous runs, where an independent audit trail beats
an interactive permission prompt.

## Canonical evidence bundle

Add `--bundle <path>` (alongside `--profile`) to also emit a canonical evidence
bundle (`.tar.gz`, manifest + events) of the observed effects, consumable by
`assay evidence lint` / `diff`:

```bash
assay sandbox \
  --profile run.profile.yaml \
  --bundle run.bundle.tar.gz \
  -- <your coding-agent command> [args...]

assay evidence lint run.bundle.tar.gz
```

The bundle carries one CloudEvents-style event per observed filesystem operation,
executed program, and containment degradation, plus a summary event, under the
deterministic profile run id (event timestamps reflect emission time). A matching
Assay-Harness recipe, gate, and report over this bundle is tracked next (see ADR-035
and ADR-034).

See also: [Editor MCP recipe](editor-mcp-recipe.md), [ADR-035](../architecture/ADR-035-sandbox-the-agent-evidence.md).
