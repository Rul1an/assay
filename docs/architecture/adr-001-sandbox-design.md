# ADR 001: Assay Sandbox Architecture (SOTA Refined)

**Date**: 2026-01-25
**Status**: Accepted

## Context
Assay needs a "one command" sandbox experience (`assay sandbox -- cmd`) that works immediately for developers (DX) while providing robust security (SOTA). Running the entire agent as root is unsafe; requiring sudo for every run causes friction. We need a path that works unprivileged by default but can upgrade to full kernel enforcement seamlessly.

## Decisions

### 1. Unified Backend-Agnostic Flow
The sandbox follows one uniform execution flow, regardless of the active backend (BPF, Landlock, or Ptrace):
1.  **Parse Policy**: Resolve extends, merges, and variable expansion.
2.  **Spawn Child**: Always create the child process as the unprivileged user.
3.  **Attach Backend**: Apply constraints (Landlock) or attach probes (BPF/Ptrace) *before* execution resumes.
4.  **Collect & Classify**: Stream uniform `SandboxEvent` stream to the CLI for classification.
5.  **Exit Decision**: Determine final exit code based on policy violations.

### 2. Backend Strategy: Containment vs. Enforcement
We define distinct tiers of protection to set clear user expectations:
*   **BPF-LSM (Full Fidelity)**: Requires `assay-bpf` helper + capabilities.
    *   *Capabilities*: Deep enforcement (socket, file, process), high-fidelity telemetry, signal blocking.
*   **Landlock (Baseline Containment)**: Rootless fallback (Kernel 5.13+).
    *   *Capabilities*: Best-effort containment. FS restricted to CWD (read-only default) and System (read).
    *   *Network*: Network restriction planned (Landlock ABI v4). v0.1 reports NET:audit.
    *   *UX*: Explicitly labeled as "Containment Mode" vs "Enforcement Mode".
*   **Ptrace (Audit Only)**: Last resort.
    *   *Capabilities*: Violation detection only (no blocking). Slow.

### 3. Privileged Helper Security ("Narrow Waist")
The `assay-bpf` helper is privileged but dumb. It trusts *nothing* from the CLI:
*   **API Boundary**: Only accepts `Attach(spec)`, `UpdateMaps(policy)`, and `Detach()`.
*   **No Arbitrary Execution**: Helper *never* accepts file paths or commands to execute. It only attaches to PIDs provided by the unprivileged parent.
*   **Caps over Root**: Prefer `cap_bpf`, `cap_perfmon`, `cap_sys_resource` over full root/setuid.

### 4. Policy Semantics & Merge Logic
Merge priority is deterministic to prevent "open by accident" flaws:
*   **Deny Wins**: A deny rule in *any* layer supersedes all allows.
*   **Union Strategy**: `allows` are additive; `denies` are additive.
*   **Defaults**: The default policy is `mcp-server-minimal` (Deny Shell, Deny Secrets, Deny Outbound).

### 5. Input/Output Hygiene (SOTA)
*   **Environment Filtering**: Planned for PR4/PR5. v0.1 inherits env.
*   **Future**: MCP-aware proxy for structured tool I/O interception.

## Interfaces

### Unified Event Schema
```rust
enum SandboxEvent {
  File { op, path, result, pid, ts, backend_meta },
  Net  { dest, proto, result, pid, ts, backend_meta },
  Proc { path, argv0, result, pid, ts, backend_meta },
}
```

### Backend Type (v0.1)
v0.1 uses `BackendType` enum; trait-based backend planned once BPF helper lands.
```rust
enum BackendType {
  Landlock,
  NoopAudit,
  Bpf, // Future
}
```
