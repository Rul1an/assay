# Research Report: State of the Art Runtime Security (Jan 2026)

## Overview
This document summarizes research into bleeding-edge runtime security practices and threat vectors relevant to Assay's architecture (eBPF monitor + Policy Engine), conducted in January 2026.

## Key Findings

### 1. eBPF TOCTOU (Time-of-Check Time-of-Use)
**Risk Level**: Critical
**Description**: File monitoring based on syscall entry probes (like `sys_enter_openat`) is vulnerable to TOCTOU. An attacker can change the file path or swap a symlink after the eBPF probe reads the path but before the kernel locks the inode.
**SOTA Mitigation**:
- Use LSM (Linux Security Modules) hooks (e.g., `security_file_open`) instead of syscalls. LSM hooks fire after path resolution, mitigating path-based race conditions.
- **Assay Gap**: Current implementation usage of `openat` syscall probe is vulnerable to this. (Action: Document as known limitation, verify path traversal).

### 2. "Confused Deputy" & Prompt Injection
**Risk Level**: High
**Description**: Autonomous agents can be tricked via indirect prompt injection to execute tools against policy intent.
**SOTA Mitigation**:
- Strict structural validation (JSON Schema).
- "Refuse-by-default" policies.
- **Assay Coverage**: Policy V2 uses schema validation, but "Logic Bypass" (e.g., tool allowed but used maliciously) remains a risk if policy is too permissive.

### 3. Policy Bypass by Non-Enforcement
**Risk Level**: Catastrophic
**Description**: A common implementation failure is loading a policy but failing to wire it effectively into the blocking path.
**Assay Finding**: `assay monitor` currently loads V2 policy config validation (`runtime_monitor.rules`) into memory but **fails to apply these rules** against the incoming event stream in the main loop. This renders the runtime protection ineffective.

### 4. Edge Cases in Path Matching
**Risk Level**: Medium
**Description**: Attackers use path obfuscation (`../`, `./`, `//`) to bypass string-based or glob-based matching.
**SOTA Best Practice**: Canonicalize paths before matching.
**Assay Gap**: `events.rs` reads raw strings from user memory. Needs normalization before glob check.

## Action Plan
1. **Fix Enforcement**: Wire `runtime_monitor` rules into the `monitor.rs` event loop immediately.
2. **Implement Path Normalization**: Ensure paths are cleaned before glob matching.
3. **Hardening**: Use `globset` for robust matching.
4. **Future**: Explore LSM hooks for v2.0.
