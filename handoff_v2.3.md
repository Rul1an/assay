Developer Handoff: Assay v2.3 (Robust CI + SOTA Inode Enforcement)

## 1. Overview

**Assay** is a high-performance Linux runtime security monitor built in Rust + eBPF (aya-rs). It enforces file/network/process policies at kernel level using LSM + tracepoints.

### Principles
*   **Kernel-first enforcement**: Decisions happen in-kernel (LSM) for correctness + performance.
*   **Fail-safe**:
    *   **Host Safety**: If monitor crashes (Exit 40), kernel enforcement stops ("Fail Open" for OS stability).
    *   **Pipeline Safety**: CI jobs fail on attach error ("Fail Closed" for security gates).
*   **CO-RE**: Compile Once, Run Everywhere on diverse kernels (target: 5.8+), validated via kernel matrix CI.

---

## 2. Architecture & Components

The codebase is organized as a Cargo workspace:

*   **`assay-cli`**: CLI, policy loading, event streaming, orchestration.
    *   *Key File*: `crates/assay-cli/src/cli/commands/monitor.rs`
*   **`assay-monitor`**: Userspace aya loader wrapper (`Monitor::load_file`, `Monitor::attach`).
*   **`assay-ebpf`**: Kernel program (LSM hooks + tracepoints).
    *   *Key Files*: `crates/assay-ebpf/src/lsm.rs`, `crates/assay-ebpf/src/main.rs`.
*   **`assay-common`**: Shared ABI between user/kernel.
    *   *Key Struct*: `InodeKey { dev, ino, gen }`.

### 2.1 CO-RE Kernel Compatibility Matrix
Assay is actively tested against the following kernels to ensure BPF/CO-RE stability:
*   **5.15 LTS** (Ubuntu 22.04) - Minimum supported target.
*   **6.6 LTS** (Ubuntu 24.04) - Recent stable.
*   **Prerequisites**: `CONFIG_BPF_LSM=y` (or `lsm=bpf` boot param) and `CONFIG_DEBUG_INFO_BTF=y`.

---

## 3. SOTA Inode Enforcement (v2.2)

Goal: prevent TOCTOU/path-race bypass by enforcing on inode identity rather than path.

### Flow
1.  **Userspace secure open**: `open(path, O_PATH | O_NOFOLLOW | O_CLOEXEC)`
    *   *Note*: `O_PATH` minimizes side effects (no content access) while enabling `fstat()`.
2.  **Derive inode identity**: `fstat(fd)` → `(dev_t, ino, gen)`
    *   `gen` (generation) may be 0 on some filesystems; fallback handling acts as mitigation.
3.  **Robust dev_t encoding**: Insert multiple keys to maximize kernel compatibility:
    *   `new_encode_dev`-style encoding (standard, matches `sb->s_dev`).
    *   `alt/old` encoding fallback: `(major << 20) | minor`.
    *   Optionally `(gen=0)` fallback.
4.  **Kernel enforcement (LSM)**: on `lsm/file_open`, reconstruct key from inode and check `DENY_INO` BPF map.

### Inode Lookup Semantics
To allow for kernel/FS differences in `dev_t` representation, the BPF program performs lookups in this order:
1.  `(dev_new, ino, gen)`
2.  `(dev_old, ino, gen)`
3.  `(dev_new, ino, 0)`  *(Fallback if gen is unstable)*
4.  `(dev_old, ino, 0)`
*Deny if any match hits.*

---

## 4. Robust CI/CD (v2.3)

Focus: resilient CI on ARM + self-hosted runners.

### 4.1 Entry Points (Single Source of Truth)
*   **Workflows**: `.github/workflows/kernel-matrix.yml`, `ci.yml`, `release.yml`.
*   **Scripts**:
    *   `scripts/ci/apt_ports_failover.sh` (Mirror reliability)
    *   `scripts/ci/verify_lsm_docker.sh` (Docker sanity)
    *   `scripts/ci/publish_idempotent.sh` (Release reliability)

### 4.2 APT & Mirrors
*   **ARM**: Dynamically switches between `ubuntu-ports` mirrors (edge.kernel.org vs ports.ubuntu.com) based on availability.
*   **Strategy**: "Install-First" (cache hit) -> "Update-Fallback" (cache miss).
*   **Settings**: `Acquire::Retries=10`, `Pipeline-Depth=0`.

### 4.3 Kernel Matrix Test
*   **Build**: Compile artifacts (CLI + eBPF) on Ubuntu ARM.
*   **Smoke Test**: Execute `deny_smoke` policy on self-hosted 5.15/6.6 runners.

### 4.4 Bleeding Edge Security
*   **Fork Gating**: Self-hosted jobs strictly blocked on forks (`fork == false`).
*   **Permissions**: `permissions: contents: read` enforced globally.

---

## 5. Learning Mode Phase 3 (v2.3): Stability Scoring

Assay can now learn “stable behavior” across repeated runs (Multi-Run).

### Profile Workflow
1.  `assay profile init`
2.  `assay profile update --run-id <id>` (Idempotent merge)
3.  `assay generate --profile profile.yaml`

### Safety Belts
*   **Confidence-Aware Gating**: Uses **Wilson Lower Bound** (95% CI) as default gate to filter noise.
*   **`--min-runs N`**: Prevents promotion if runs < N (default 1, recommended 5).
    *   *Semantics*: If `total_runs < min_runs`, items are skipped (default) or marked `needs_review` (if `--new-is-risky`).
*   **Scope Guard**: `profile update` enforces config fingerprint matching (unless `--force`).

---

## 6. Failure Modes & Safe Behavior

| Failure Scenario | Exit Code | Behavior | CI Implications |
| :--- | :--- | :--- | :--- |
| **Monitor Attach Failed** | `40` | Fatal Error | **Fail Closed** (Job fails, PR blocked) |
| **BPF Not Supported** | `40` | Error Log | **Fail Closed** (Job fails) |
| **Policy Parse Error** | `1` | Fatal Error | **Fail Closed** |
| **Cgroup Resolution Fail**| `40` | Fatal Error | **Fail Closed** |
| **Runtime Violation** | `0` | Log/Kill | **Enforcing** (Process blocked, Monitor continues) |

*System-wise behavior:* If the monitor crashes (Exit 40), kernel enforcement stops (Fail Open regarding the OS, but Fail Closed regarding the Pipeline).

---

## 7. Developer Workflows

### Quickstart (New Contributors)
```bash
# 1. Build Userspace & eBPF
cargo build -p assay-cli
cargo xtask build-ebpf

# 2. Run eBPF Monitor (Requires sudo)
sudo target/debug/assay monitor --pid $$ --duration 10s

# 3. Validating Changes
cargo test
# Optional: assay doctor (check prerequisites)
```

### Release Checklist
*   [ ] CI Green: `kernel-matrix` (5.15 + 6.6) + `ci` (Smoke).
*   [ ] Verify x86 builder image pinned by digest.
*   [ ] Tag `v2.x.x` + Update Changelog.
*   [ ] Verify smoke logs upload in artifacts.
