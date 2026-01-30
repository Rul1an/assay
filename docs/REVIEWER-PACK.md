# Reviewer pack – GitHub Actions & release

Pack voor security/review van workflows, permissions, secrets en release-provenance. Gebaseerd op GitHub’s “secure use” richtlijnen (pinnen op SHA, permissions, fork safety, caches, concurrency, untrusted input, `pull_request_target`-valkuilen).

---

## Wat de GitHub MCP (Cursor) wel/niet levert

De **user-github** MCP in Cursor heeft **geen** toegang tot de Actions-API, branch protection, rulesets, secrets of environments. Daardoor:

| Gewenst voor risk-rating | Via deze MCP? | Hoe wel? |
|---------------------------|---------------|----------|
| Repo/org Actions-instellingen (workflow permissions, allowed actions, SHA-pinning) | **Ja (gh CLI)** | `gh api repos/.../actions/permissions` en `.../actions/permissions/workflow`; zie sectie 2 |
| Branch protection / rulesets / required checks / CODEOWNERS | **Ja (gh CLI)** | `gh api repos/.../branches/main/protection`; zie sectie 2 |
| Lijst secrets + scope, environments + approvals | **Deels (gh CLI)** | `gh secret list` (namen); `gh api .../environments`; zie sectie 3 |
| Self-hosted runner posture (ephemeral/persistent, netwerk, fork-PR) | **Nee** | Documentatie + handmatig bij de runner-host |
| Workflow-run links (PR-run, main-run, release-run) | **Ja (gh CLI)** | `gh run list`; zie sectie 4 |
| Repo-metadata, releases, recente PR’s | **Ja (MCP)** | Zie hieronder |

**Via gh CLI opgehaald (Rul1an/assay):**

- **Actions permissions** (`gh api repos/Rul1an/assay/actions/permissions`): `enabled: true`, `allowed_actions: "all"`, **`sha_pinning_required: false`**.
- **Workflow default permissions** (`gh api repos/Rul1an/assay/actions/permissions/workflow`): **`default_workflow_permissions: "read"`** (read-only default), `can_approve_pull_request_reviews: false`.
- **Branch protection (main)** (`gh api repos/Rul1an/assay/branches/main/protection`): **Branch not protected** (404). Geen required checks op main.
- **Rulesets** (`gh api repos/Rul1an/assay/rulesets`): **`[]`** — geen repo rulesets. Bevestigt dat er naast branch protection ook geen rulesets elders gelden (UI kan rulesets tonen; hier is de lijst leeg).
- **Org-niveau:** Rul1an is een **user**-account (geen org). `gh api orgs/Rul1an/actions/permissions` geeft 404; er is geen org-level override voor allowed actions of SHA-pinning. Repo-instellingen zijn dus leidend.
- **Secrets** (`gh secret list -R Rul1an/assay`): leeg (geen repo-secrets zichtbaar, of geen rechten). Uit YAML: BENCHER_PROJECT, BENCHER_API_TOKEN (verwacht repo); GITHUB_TOKEN is automatisch.
- **Environments** (`gh api repos/Rul1an/assay/environments`): **github-pages** (branch_policy, custom_branch_policies: true), **pypi** (geen protection_rules). Geen `environment:` in release/workflow YAML voor publish; pypi-environment bestaat maar wordt niet gebruikt voor approval gate.

**Via MCP wel verkregen (Rul1an/assay):**

- **Repo:** `default_branch`: main, `allow_forking`: true, `web_commit_signoff_required`: false, `has_pages`: true, `visibility`: public.
- **Releases (voor release-run bewijs):** [v2](https://github.com/Rul1an/assay/releases/tag/v2), [v2.12.0](https://github.com/Rul1an/assay/releases/tag/v2.12.0), [v2.11.0](https://github.com/Rul1an/assay/releases/tag/v2.11.0) – open een release en controleer “This workflow run” / link naar de Release workflow.
- **Recente PR’s (voor PR-run + cache-hit):** [PR #65](https://github.com/Rul1an/assay/pull/65), [PR #64](https://github.com/Rul1an/assay/pull/64), [PR #63](https://github.com/Rul1an/assay/pull/63) – tab **Checks** toont workflow runs en job summary (cache-hit).
- **CODEOWNERS:** Bestand `.github/CODEOWNERS` **bestaat niet** in de repo (MCP get_file_contents).

**Run-links zelf samenstellen:**

- **PR-run:** Ga naar een PR (bv. [PR #65](https://github.com/Rul1an/assay/pull/65)) → tab **Checks** → klik op een workflow (bv. CI) voor de run-URL.
- **Main-run:** [Actions](https://github.com/Rul1an/assay/actions) → filter op workflow “CI” of “Smoke” → open laatste run op `main`.
- **Release-run:** [Releases](https://github.com/Rul1an/assay/releases) → open een release (bv. v2) → link “This workflow run” of via [Actions](https://github.com/Rul1an/assay/actions) filter op “Release”.

---

## 1. De echte YAML’s (onmisbaar)

Hier zit het grootste deel van de risico’s en verbeteringen. Onderstaand de **volledige inhoud** van alle workflow-bestanden.

**Overzicht:**

| Bestand | Triggers | Belangrijk voor review |
|--------|----------|-------------------------|
| `ci.yml` | push (main, debug/**), pull_request, workflow_dispatch | permissions, self-hosted fork guard, concurrency |
| `release.yml` | push tags v*, workflow_dispatch | contents: write, OIDC crates.io/PyPI, release job |
| `action-tests.yml` | push main, pull_request, workflow_dispatch | Geen expliciete permissions (inherited) |
| `action-v2-test.yml` | push (assay-action), workflow_dispatch | permissions contents + security-events |
| `assay-security.yml` | push (paths), pull_request, workflow_dispatch | security-events: write, SARIF upload |
| `baseline-gate-demo.yml` | pull_request (paths) | cache key, base_ref, geen permissions |
| `docs.yml` | push main (paths), workflow_dispatch | contents: write (Pages deploy) |
| `kernel-matrix.yml` | push (main, debug), pull_request (paths) | self-hosted + fork guard, actions: write |
| `parity.yml` | push/pull_request (paths) | cache, geen permissions |
| `perf_main.yml` | push main, schedule | secrets BENCHER_*, checks: write |
| `perf_pr.yml` | pull_request | same-repo guard, secrets BENCHER_* |
| `smoke-install.yml` | push main, pull_request, workflow_dispatch | contents: read, checks: write |

**Geen reusable workflows (`workflow_call`) in dit repo.**
**Geen composite actions in `.github/actions/`** – de Assay Action zit in `assay-action/action.yml` (aparte action, geen workflow).

---

### 1.1 `ci.yml`

```yaml
name: CI

on:
  push:
    branches: [ main, "debug/**" ]
  pull_request:
    paths-ignore:
      - "docs/**"
      - "**.md"
      - ".gitignore"
  workflow_dispatch:

jobs:
  clippy:
    name: Clippy (deny warnings)
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Swatinem/rust-cache
        uses: Swatinem/rust-cache@v2
      - name: Install Linux deps (for build scripts)
        if: runner.os == 'Linux'
        shell: bash
        run: |
          set -euo pipefail
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends clang libsqlite3-dev || {
            sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
              -o Acquire::Retries=10 \
              -o Acquire::http::Timeout=60 \
              -o Acquire::https::Timeout=60 \
              -o Acquire::CompressionTypes::Order::=gz \
              -o Acquire::ForceIPv4=true \
              -o Acquire::Languages=none
            sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends clang libsqlite3-dev
          }
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - run: cargo clippy --workspace --all-targets -- -D warnings

  open-core-boundary:
    name: Open Core Boundary Check
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Check open core boundary
        run: ./scripts/ci/check-open-core-boundary.sh

  perf:
    name: Criterion benches (store + suite)
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Swatinem/rust-cache
        id: rust-cache
        uses: Swatinem/rust-cache@v2
      - name: Install Linux deps (for build scripts)
        shell: bash
        run: |
          set -euo pipefail
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends clang libsqlite3-dev || {
            sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
              -o Acquire::Retries=10 \
              -o Acquire::http::Timeout=60 \
              -o Acquire::https::Timeout=60 \
              -o Acquire::CompressionTypes::Order::=gz \
              -o Acquire::ForceIPv4=true \
              -o Acquire::Languages=none
            sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends clang libsqlite3-dev
          }
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - name: Run Criterion benches
        run: cargo bench -p assay-core -p assay-cli --no-fail-fast -- --quick
      - name: Upload Criterion report
        uses: actions/upload-artifact@v4
        with:
          name: criterion-report
          path: target/criterion/
          retention-days: 5
      - name: Prove cache hit (job summary)
        if: always()
        run: |
          echo "cache-hit=${{ steps.rust-cache.outputs.cache-hit }}"
          echo "cache-hit=${{ steps.rust-cache.outputs.cache-hit }}" >> "$GITHUB_STEP_SUMMARY"

  test:
    name: Build + Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    permissions:
      contents: read
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    env:
      PYO3_USE_ABI3_FORWARD_COMPATIBILITY: 1

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.12'

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Install mold (Linux)
        if: runner.os == 'Linux'
        shell: bash
        run: |
          set -euo pipefail
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends mold clang libsqlite3-dev || {
            sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
              -o Acquire::Retries=10 \
              -o Acquire::http::Timeout=60 \
              -o Acquire::https::Timeout=60 \
              -o Acquire::CompressionTypes::Order::=gz \
              -o Acquire::ForceIPv4=true \
              -o Acquire::Languages=none
            sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends mold clang libsqlite3-dev
          }

      - name: Sccache
        uses: mozilla-actions/sccache-action@v0.0.6

      - name: Rust cache
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            . -> target
          cache-directories: |
            ~/.sccache
          cache-on-failure: true

      - name: Configure Environment
        shell: bash
        run: |
          {
            echo "RUSTC_WRAPPER=sccache"
            echo "SCCACHE_DIR=$HOME/.sccache"
            echo "SCCACHE_GHA_ENABLED=false"
            echo "CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse"
          } >> "$GITHUB_ENV"

      - name: Configure Linker (Linux)
        if: runner.os == 'Linux'
        shell: bash
        run: |
          echo "RUSTFLAGS=-C linker=clang -C link-arg=-fuse-ld=mold" >> "$GITHUB_ENV"

      - name: Test Workspace (Linux)
        if: runner.os == 'Linux'
        run: cargo test --locked --workspace --exclude assay-ebpf --exclude assay-it

      - name: Test Workspace (Cross-Platform)
        if: runner.os != 'Linux'
        run: cargo test --locked --workspace --exclude assay-ebpf --exclude assay-it --exclude assay-monitor --exclude assay-cli

  ebpf-smoke-ubuntu:
    name: eBPF monitor smoke (Linux - Ubuntu)
    needs: [test]
    runs-on: ubuntu-latest
    concurrency:
      group: ${{ github.workflow }}-${{ github.ref }}-ebpf-smoke-ubuntu
      cancel-in-progress: true
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rust-src
      - name: Install Dependencies
        shell: bash
        run: |
          set -euo pipefail
          sudo bash scripts/ci/apt_ports_failover.sh
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            build-essential libssl-dev pkg-config llvm-dev libclang-dev clang libsqlite3-dev
          if ! command -v bpf-linker >/dev/null 2>&1; then
            cargo install bpf-linker --locked
          fi
      - name: Verify LSM blocking (CI Mode - Ubuntu soft skip)
        shell: bash
        env:
          STRICT_LSM_CHECK: "0"
        run: |
          set -euo pipefail
          chmod +x scripts/verify_lsm_docker.sh
          sudo -E env "PATH=$PATH" ./scripts/verify_lsm_docker.sh --ci-mode
      - name: Fix log permissions
        if: always()
        shell: bash
        run: |
          sudo chown -R "$(id -u)":"$(id -g)" /tmp/assay-lsm-verify || true
          ls -l /tmp/assay-lsm-verify || true
      - name: Upload verification logs
        if: always()
        uses: actions/upload-artifact@v4
        continue-on-error: true
        with:
          name: ci-smoke-logs-ubuntu
          path: /tmp/assay-lsm-verify/
          if-no-files-found: ignore

  ebpf-smoke-self-hosted:
    name: eBPF monitor smoke (Linux - Self-Hosted)
    needs: [test]
    if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.fork == false
    runs-on: [self-hosted]
    timeout-minutes: 60
    concurrency:
      group: ${{ github.workflow }}-${{ github.ref }}-ebpf-smoke-self-hosted
      cancel-in-progress: true
    permissions:
      contents: read
    steps:
      - name: Pre-clean workspace ownership
        shell: bash
        run: |
          sudo chown -R "$(id -u)":"$(id -g)" "$GITHUB_WORKSPACE" || true
          sudo chown -R "$(id -u)":"$(id -g)" "$(dirname "$GITHUB_WORKSPACE")" || true
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rust-src
      - name: Install Dependencies
        shell: bash
        run: |
          set -euo pipefail
          set +e
          MIRRORS=(
            "https://mirror.gofoss.xyz/ubuntu-ports"
            "http://ports.ubuntu.com/ubuntu-ports"
          )
          switch_mirror() {
            local m="$1"
            if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then
              sudo sed -i \
                -e "s|http://ports.ubuntu.com/ubuntu-ports|${m}|g" \
                -e "s|https://ports.ubuntu.com/ubuntu-ports|${m}|g" \
                /etc/apt/sources.list.d/ubuntu.sources 2>/dev/null || true
            fi
            if [ -f /etc/apt/sources.list ]; then
              sudo sed -i \
                -e "s|http://ports.ubuntu.com/ubuntu-ports|${m}|g" \
                -e "s|https://ports.ubuntu.com/ubuntu-ports|${m}|g" \
                /etc/apt/sources.list 2>/dev/null || true
            fi
          }
          apt_update() {
            sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
              -o Acquire::Queue-Mode=access \
              -o Acquire::Retries=10 \
              -o Acquire::http::Timeout=60 \
              -o Acquire::https::Timeout=60 \
              -o Acquire::http::Pipeline-Depth=0 \
              -o Acquire::https::Pipeline-Depth=0 \
              -o Acquire::CompressionTypes::Order::=gz \
              -o Acquire::ForceIPv4=true \
              -o Acquire::Languages=none
          }
          ok=0
          for m in "${MIRRORS[@]}"; do
            echo "Trying Ubuntu Ports mirror: $m"
            switch_mirror "$m"
            if apt_update; then
              ok=1
              break
            fi
          done
          if [ "$ok" -ne 1 ]; then
            echo "ERROR: apt-get update failed on all mirrors"
            exit 1
          fi
          set -e
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            build-essential libssl-dev pkg-config llvm-dev libclang-dev clang libsqlite3-dev
          sudo chown -R "$(whoami)":"$(id -gn)" ~/.cargo || true
          if ! command -v bpf-linker >/dev/null 2>&1; then
            cargo install bpf-linker --locked
          fi
      - name: Verify LSM blocking (CI Mode - strict on self-hosted)
        shell: bash
        env:
          STRICT_LSM_CHECK: "1"
        run: |
          set -euo pipefail
          chmod +x scripts/verify_lsm_docker.sh
          sudo -E env "PATH=$PATH" ./scripts/verify_lsm_docker.sh --ci-mode
      - name: Fix log permissions
        if: always()
        shell: bash
        run: |
          sudo chown -R "$(id -u)":"$(id -g)" /tmp/assay-lsm-verify || true
          ls -l /tmp/assay-lsm-verify || true
      - name: Upload verification logs
        if: always()
        uses: actions/upload-artifact@v4
        continue-on-error: true
        with:
          name: ci-smoke-logs-self-hosted
          path: /tmp/assay-lsm-verify/
          if-no-files-found: ignore
```

**Reviewpunten:**
- Actions: `@v4` / `@v2` (geen SHA-pin).
- Self-hosted job: expliciete fork-guard `if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.fork == false`.
- Geen `pull_request_target`; alle jobs `contents: read`.
- Caches: Swatinem/rust-cache, sccache; geen user-controlled cache keys.

---

### 1.2 `release.yml`

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:
    inputs:
      version:
        description: 'Version tag (e.g., v1.1.0)'
        required: true

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: assay
            archive: tar.gz
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact: assay
            archive: tar.gz
            cross: true
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Install cross-compilation tools
        if: matrix.cross
        shell: bash
        run: |
          set -euo pipefail
          command -v python3 >/dev/null 2>&1 || {
            sudo DEBIAN_FRONTEND=noninteractive apt-get update -y
            sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3
          }
          sudo dpkg --add-architecture arm64
          . /etc/os-release
          CODENAME="${VERSION_CODENAME:-noble}"
          if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then
            sudo python3 scripts/ci/fix_apt_sources.py
          fi
          if [ -f /etc/apt/sources.list ]; then
            sudo sed -i -E '
              /^deb(-src)?[[:space:]]+\[.*\][[:space:]]+/b
              s|^(deb(-src)?[[:space:]]+)(http(s)?://[^[:space:]]*ubuntu\.com/ubuntu)|\1[arch=amd64] \3|
            ' /etc/apt/sources.list
          fi
          sudo tee /etc/apt/sources.list.d/ubuntu-ports-arm64.list >/dev/null <<EOF
          deb [arch=arm64] https://mirror.gofoss.xyz/ubuntu-ports ${CODENAME} main universe restricted multiverse
          deb [arch=arm64] https://mirror.gofoss.xyz/ubuntu-ports ${CODENAME}-updates main universe restricted multiverse
          deb [arch=arm64] https://mirror.gofoss.xyz/ubuntu-ports ${CODENAME}-security main universe restricted multiverse
          deb [arch=arm64] https://mirror.gofoss.xyz/ubuntu-ports ${CODENAME}-backports main universe restricted multiverse
          EOF
          sudo bash scripts/ci/apt_ports_failover.sh
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            gcc-aarch64-linux-gnu libc6-dev-arm64-cross linux-libc-dev-arm64-cross pkg-config
          {
            echo "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc"
            echo "CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc"
            echo "AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar"
          } >> "$GITHUB_ENV"
      - name: Cache cargo
        uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ matrix.target }}
      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }} --package assay-cli
      - name: Get version
        id: version
        shell: bash
        run: |
          if [ "${{ github.event_name }}" = "workflow_dispatch" ]; then
            V="${{ github.event.inputs.version }}"
          else
            V="${GITHUB_REF#refs/tags/}"
          fi
          echo "version=$V" >> "$GITHUB_OUTPUT"
      - name: Package (Unix)
        if: matrix.archive == 'tar.gz'
        shell: bash
        run: |
          VERSION="${{ steps.version.outputs.version }}"
          ARCHIVE_NAME="assay-${VERSION}-${{ matrix.target }}"
          mkdir -p "dist/${ARCHIVE_NAME}"
          cp "target/${{ matrix.target }}/release/${{ matrix.artifact }}" "dist/${ARCHIVE_NAME}/"
          cp README.md LICENSE "dist/${ARCHIVE_NAME}/" 2>/dev/null || true
          cd dist
          tar -czvf "${ARCHIVE_NAME}.tar.gz" "${ARCHIVE_NAME}"
          shasum -a 256 "${ARCHIVE_NAME}.tar.gz" > "${ARCHIVE_NAME}.tar.gz.sha256"
      - name: Package (Windows)
        if: matrix.archive == 'zip'
        shell: pwsh
        run: |
          $VERSION = "${{ steps.version.outputs.version }}"
          $ARCHIVE_NAME = "assay-${VERSION}-${{ matrix.target }}"
          New-Item -ItemType Directory -Force -Path "dist\${ARCHIVE_NAME}"
          Copy-Item "target\${{ matrix.target }}\release\${{ matrix.artifact }}" "dist\${ARCHIVE_NAME}\"
          Copy-Item README.md, LICENSE "dist\${ARCHIVE_NAME}\" -ErrorAction SilentlyContinue
          Compress-Archive -Path "dist\${ARCHIVE_NAME}" -DestinationPath "dist\${ARCHIVE_NAME}.zip"
          $hash = (Get-FileHash "dist\${ARCHIVE_NAME}.zip" -Algorithm SHA256).Hash.ToLower()
          "${hash}  ${ARCHIVE_NAME}.zip" | Out-File -Encoding ASCII "dist\${ARCHIVE_NAME}.zip.sha256"
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: assay-${{ matrix.target }}
          path: dist/assay-*
          retention-days: 7

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - name: Get version
        id: version
        run: |
          if [ "${{ github.event_name }}" = "workflow_dispatch" ]; then
            V="${{ github.event.inputs.version }}"
          else
            V="${GITHUB_REF#refs/tags/}"
          fi
          echo "version=$V" >> "$GITHUB_OUTPUT"
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Prepare release assets
        run: |
          mkdir -p release
          find artifacts -type f \( -name "*.tar.gz" -o -name "*.zip" -o -name "*.sha256" \) -exec cp {} release/ \;
          ls -la release/
      - name: Generate release notes
        id: notes
        run: |
          cat > release_notes.md << 'EOF'
          ## Assay ${{ steps.version.outputs.version }}
          ...
          EOF
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          name: Assay ${{ steps.version.outputs.version }}
          body_path: release_notes.md
          draft: false
          prerelease: ${{ contains(steps.version.outputs.version, '-rc') || contains(steps.version.outputs.version, '-beta') }}
          files: release/*
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

  verify-lsm-blocking:
    name: Verify LSM Enforcement
    needs: [build]
    if: github.event_name == 'workflow_dispatch'
    runs-on: [self-hosted]
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Verify LSM blocking (CI gate)
        run: |
          chmod +x scripts/verify_lsm_docker.sh
          sudo -E env "PATH=$PATH" ./scripts/verify_lsm_docker.sh --enforce-lsm
      - name: Fix Runner Permissions (Cleanup)
        if: always()
        run: |
          sudo chown -R "$(whoami)":"$(id -gn)" . || true
      - name: Upload verification logs
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: lsm-verification-logs
          path: /tmp/assay-lsm-verify/

  publish-crates:
    name: Publish to crates.io
    needs: release
    runs-on: ubuntu-latest
    if: "!contains(github.ref, '-rc') && !contains(github.ref, '-beta')"
    permissions:
      id-token: write
      contents: read
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Authenticate with crates.io
        id: auth
        uses: rust-lang/crates-io-auth-action@v1
      - name: Publish Crates (Idempotent)
        run: |
          chmod +x scripts/ci/publish_idempotent.sh
          ./scripts/ci/publish_idempotent.sh
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}

  wheels:
    name: Build Wheels
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: macos-14
            target: aarch64-apple-darwin
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: '3.12'
      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          working-directory: assay-python-sdk
          target: ${{ matrix.target }}
          args: --release --out dist --locked
          sccache: 'true'
          manylinux: auto
      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.os }}-${{ matrix.target }}
          path: assay-python-sdk/dist/*.whl

  publish-pypi:
    name: Publish to PyPI
    needs: [release, wheels]
    runs-on: ubuntu-latest
    if: "!contains(github.ref, '-rc') && !contains(github.ref, '-beta')"
    permissions:
      id-token: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist
      - name: Publish to PyPI
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          packages-dir: dist
          skip-existing: true
```

**Reviewpunten:**
- Crates.io en PyPI: OIDC-first (geen long-lived secrets in repo).
- Release job is enige met `contents: write`; alleen op tag/workflow_dispatch (geen PRs).
- `softprops/action-gh-release@v2`, `pypa/gh-action-pypi-publish@release/v1`: tag/branch-refs, geen SHA.

---

### 1.3 Overige workflows (pad + kern)

| Bestand | Volledige inhoud |
|--------|-------------------|
| **action-tests.yml** | Zie `.github/workflows/action-tests.yml` – geen top-level `permissions`; jobs gebruiken default. Fork-PR draait wel (geen self-hosted, geen secrets). |
| **action-v2-test.yml** | Zie `.github/workflows/action-v2-test.yml` – `permissions: contents: read; security-events: write`; gebruikt `./assay-action` (lokaal). |
| **assay-security.yml** | Zie `.github/workflows/assay-security.yml` – `security-events: write`, `actions: read`, `contents: read`; `github/codeql-action/upload-sarif@v4`; curl-get assay install script. |
| **baseline-gate-demo.yml** | Zie `.github/workflows/baseline-gate-demo.yml` – `actions/cache@v4` met hash van eval + traces; geen permissions; `github.base_ref` alleen voor fetch-simulatie. |
| **docs.yml** | Zie `.github/workflows/docs.yml` – `permissions: contents: write`; mkdocs gh-deploy; alleen push main (paths) of workflow_dispatch. |
| **kernel-matrix.yml** | Zie `.github/workflows/kernel-matrix.yml` – `if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.fork == false` op matrix-test; `runs-on: [self-hosted, linux, assay-bpf-runner]`; `actions: write` voor download-artifact. |
| **parity.yml** | Zie `.github/workflows/parity.yml` – `actions/cache@v4` op cargo paths; geen expliciete permissions. |
| **perf_main.yml** | Zie `.github/workflows/perf_main.yml` – `secrets.BENCHER_PROJECT`, `BENCHER_API_TOKEN`, `GITHUB_TOKEN`; `bencherdev/bencher@main`; `checks: write`. |
| **perf_pr.yml** | Zie `.github/workflows/perf_pr.yml` – `if: github.event.pull_request.head.repo.full_name == github.repository`; zelfde Bencher-secrets. |
| **smoke-install.yml** | Zie `.github/workflows/smoke-install.yml` – `contents: read`, `checks: write`, `actions: read`; `dorny/test-reporter@v1`. |

Voor letterlijke YAML van elk bestand: open de genoemde paden in de repo.

---

## 2. Repo/org-instellingen (screenshots of tekst) — minimale set

Deze sectie vul je aan met screenshots of korte beschrijvingen. Zij bepalen of workflows “least privilege by default” zijn en of PR’s van forks ooit high-privilege kunnen raken.

- **Actions → General**
  - [x] Workflow permissions: **Read** (read-only default) — gh: `default_workflow_permissions: "read"`.
  - [ ] **Fork PR policy (Actions → General):** Niet via API. Nodig: (1) Draaien fork-PR workflows? (2) Read-only of write token? (3) Secrets geblokkeerd? Screenshot: Settings → Actions → General → Fork pull request workflows. (bv. “Run workflows from fork PRs” met read-only token of uitgeschakeld.)

- **Allowed actions**
  - [ ] **Allow all actions** / **Allow [org] and verified creators** / **Allow [org] and specific actions**?
  Aanbevolen: allowlist of “verified creators” i.p.v. alles.

- **Rulesets / Branch protection (main)**
  *(SHA-pinning: indien beschikbaar in org/repo, policy aanzetten zodat alleen actions met volledige SHA zijn toegestaan.)*
  - [ ] **Branch protection:** gh: **main niet protected** (404). **Rulesets:** gh: **geen** (`[]`). Als dat bewust is: bevestigen en documenteren; anders branch protection of ruleset toevoegen voor required checks.
  - [ ] **Required status checks** (indien later): welke jobs groen vóór merge? **Require signed commits?** **Require linear history?** **Restrict force-push**?
  - [ ] **CODEOWNERS / required reviews** (voor `.github/workflows/**`, `release.yml`, `assay-action/**`)? *`.github/CODEOWNERS` bestaat niet (MCP-check).*
  - [ ] **GHAS:** aan/uit? **Code scanning** (CodeQL)? **Secret scanning** (push protection)? **Dependency review**?

- **Environments**
  - [ ] Bestaan er environments (bv. `production`, `staging`)? Zo ja: approval gates, deployment branches, en welke jobs gebruiken ze (geen in huidige YAML’s)?

*Plaats hier screenshots of één regels per punt.*

---

## 3. Secrets & environments (wat mag waar)

*Secrets: `gh secret list -R Rul1an/assay` was leeg (geen zichtbare repo-secrets of geen rechten). Environments: via `gh api repos/Rul1an/assay/environments` — zie hieronder.*

- **Secrets (alleen namen), scope (uit YAML-analyse)**
  - **GITHUB_TOKEN** – repo, automatisch; gebruikt in o.a. release (gh-release), perf (Bencher GitHub integration).
  - **BENCHER_PROJECT** – repo; alleen in `perf_main.yml` en `perf_pr.yml`.
  - **BENCHER_API_TOKEN** – repo; idem.
  - **CARGO_REGISTRY_TOKEN** – niet als repo secret opgeslagen; komt van `rust-lang/crates-io-auth-action` (OIDC) output in release workflow.
- **Environments (gh API):** **github-pages** (branch_policy, custom_branch_policies: true), **pypi** (geen protection_rules). In de workflow-YAML wordt geen `environment:` gezet voor release/publish; pypi-environment bestaat dus wel maar wordt niet gebruikt als approval gate. Releases/publicaties hangen niet achter een Environment met approvals.

- **Environments & approvals (SOTA 2026 — human-in-the-loop)**
  Voor een serieuze review: (1) **Welke jobs moeten approvals hebben?** Aanbevolen: PyPI publish, crates.io publish, GitHub Release (Create Release). (2) **Wie mag workflow_dispatch voor release uitvoeren?** Alleen maintainers met write; eventueel Environment "release" met required reviewers zodat workflow_dispatch pas na approval de release-job draait. (3) **Environment gates:** Voeg `environment: pypi` toe aan publish-pypi job en `environment: release` (of crates) aan publish-crates en release job; configureer in Settings → Environments de benodigde reviewers. Zo blijft OIDC behouden en komt er een extra menselijke check bovenop.

- **OIDC vs static**
  - **crates.io:** OIDC via `rust-lang/crates-io-auth-action` (`id-token: write`); geen static CARGO_REGISTRY_TOKEN in repo.
  - **PyPI:** Trusted publishing via `pypa/gh-action-pypi-publish` (`id-token: write`); geen PyPI token in repo.
  - **Bencher:** static token `BENCHER_API_TOKEN` (repo secret); alleen same-repo PR of push (perf_pr heeft fork-guard).

- **Self-hosted runners (Multipass)**
  Runners draaien op **Multipass** (Ubuntu VM's). Infra: `infra/bpf-runner/` (`setup_local_multipass.sh`, `cloud-init.yaml`, `register_local.sh`). VM: `assay-bpf-runner`, Ubuntu 24.04, 4 vCPU/8G RAM/20G disk; **persistent** (geen ephemeral per job). Netwerk: standaard egress; updates handmatig of via cloud-init. Runner-versie in `register_local.sh`: actions-runner 2.311.0.
  **Risk-rating (SOTA):** (1) **Ephemeral vs long-lived:** Long-lived (persistent VM); ephemeral per job zou beter zijn. (2) **Runner groups:** Repo-level = alleen dit repo; controleren in Settings → Runners. (3) **Egress:** Geen allowlist in scripts; outbound firewall overwegen. (4) **Cleanup/hardening:** Geen automatische image reset; kernel-matrix doet workspace cleanup; docker socket exposure beperken; outbound firewall niet geconfigureerd — aan te raden voor productie.
  - **ci.yml:** job `ebpf-smoke-self-hosted` op `runs-on: [self-hosted]`; alleen non-fork PR of push (`if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.fork == false`).
  - **kernel-matrix.yml:** job `matrix-test` op `runs-on: [self-hosted, linux, assay-bpf-runner]`;zelfde fork-guard.
  - **release.yml:** job `verify-lsm-blocking` op `[self-hosted]`; alleen bij `workflow_dispatch` (geen PR).
  Geen aparte “environments” voor runners in de YAML’s; labels bepalen welke runner.

---

## 4. 2–3 run-links (PR, main, release)

Workflow-run IDs zijn **niet** via MCP op te halen. Gebruik onderstaande pagina's om zelf run-URL's te maken.

- **1× PR-run (liefst met cache-hit):** Open een recente PR → tab **Checks** → klik op workflow (bv. CI). Voorbeeld PR's: [PR #65](https://github.com/Rul1an/assay/pull/65), [PR #64](https://github.com/Rul1an/assay/pull/64). In job summary: `cache-hit=true/false`.

- **1× main-run:** [Actions](https://github.com/Rul1an/assay/actions) → filter op workflow CI of Smoke install → open laatste run op `main`.

- **1× release-run (tag):** [Releases](https://github.com/Rul1an/assay/releases) → open bv. [v2](https://github.com/Rul1an/assay/releases/tag/v2) of [v2.12.0](https://github.com/Rul1an/assay/releases/tag/v2.12.0) → link "This workflow run" of via Actions filter op Release.

- **Permissions / skip-logica**
  - Fork PR: self-hosted jobs (ci, kernel-matrix) moeten “skipped” zijn; perf_pr “skipped” op fork (geen secrets).
  - SARIF: in action-tests wordt fork-PR-skip getest (upload SARIF alleen bij same-repo).

**Concrete run-URL’s (via `gh run list`):**

| Type | Workflow | Run-URL | Event / branch |
|------|----------|---------|-----------------|
| **Main-run** | CI | https://github.com/Rul1an/assay/actions/runs/21508433120 | push, main |
| **Release-run** | Release | https://github.com/Rul1an/assay/actions/runs/21507732295 | push, v2 (success) |
| **PR-run** | assay-action-contract-tests | https://github.com/Rul1an/assay/actions/runs/21489597010 | pull_request, feat/pack-registry-v1 |
| **PR-run** | Smoke Install (E2E) | https://github.com/Rul1an/assay/actions/runs/21489597001 | pull_request, feat/pack-registry-v1 |

**Bewijs runs (laatste 20% van review):**

- **Job summaries (cache-hit, artifacts, skipped jobs):** De run-URL's openen in de browser; per job staat de **Job summary** (incl. `cache-hit=true/false` als de workflow dat logt, bv. in CI perf-job en baseline-gate). Voor fork-PR: controleer dat self-hosted jobs (ebpf-smoke-self-hosted, matrix-test) **skipped** zijn. Logs lokaal: `gh run view <run_id> -R Rul1an/assay --log` (geen job summary in JSON; wel step output).
- **Release-run bewijs (OIDC, commit/tag):** Run [21507732295](https://github.com/Rul1an/assay/actions/runs/21507732295) (Release, tag v2):
  - **headSha:** `e65394d572d3fad649624ab3fa413be934b1d9fa` (commit die gebouwd is).
  - **Jobs:** Build x86_64, Build aarch64, Build Wheels (3x), Create Release, Verify LSM Enforcement (skipped), **Publish to crates.io** (success; step "Authenticate with crates.io" = OIDC), **Publish to PyPI** (success; trusted publishing).
  - **Bewijs OIDC:** Publish to crates.io heeft step "Authenticate with crates.io" (rust-lang/crates-io-auth-action); Publish to PyPI gebruikt pypa/gh-action-pypi-publish met `id-token: write`. Geen static tokens in repo.
  - **Tag/commit:** Event = push; ref = tag v2; release notes gegenereerd uit `steps.version.outputs.version`.

---

## 5. Release / provenance-details

- **Wat wordt er gepubliceerd?**
  - **Binaries:** Linux x86_64 en aarch64 (tar.gz + sha256) via GitHub Release.
  - **Crates:** assay-* crates naar crates.io (bij tag, exclusief -rc/-beta).
  - **Wheels:** Python wheels naar PyPI (idem); manylinux (Linux), macOS x64/arm64.

- **Artifact attestation / build provenance**
  - **Nu:** Geen SLSA/build provenance in de beschreven workflows; checksums (sha256) wel voor release-archieven.
  - **Mogelijke verbetering:** GitHub’s OIDC + artifact attestation of derde-partij (bv. Sigstore) voor binaries.

- **Downstream verificatie**
  - Gebruikers: sha256-bestanden bij release; `assay --version` na install.
  - Crates.io/PyPI: standaard checksums van het registry; geen extra attestation in deze workflows.

---

## Checklist (secure use)

- [ ] Alle third-party actions gepind op **SHA** (nu grotendeels @v1/@v2/@v4/@v5 of @main/@release/v1).
- [ ] Workflow permissions overal minimaal (contents: read waar mogelijk; contents: write alleen release/docs).
- [ ] Geen `pull_request_target` gebruikt (geen risico op secret-inject uit fork).
- [ ] Self-hosted jobs alleen bij non-fork PR of niet-PR events (fork-guard aanwezig).
- [ ] Caches: keys geen user-controlled input (hashFiles of vaste prefix).
- [ ] Concurrency: waar nodig (ebpf-smoke, kernel matrix) om dubbele runs te beperken.
- [ ] OIDC voor crates.io en PyPI (geïmplementeerd); Bencher nog static token met same-repo guard.
- [ ] Repo-instellingen: workflow permissions read-only default; allowed actions beperkt (aan te vullen met sectie 2).

Dit document kun je updaten met screenshots (sectie 2) en run-links (sectie 4) zodra die beschikbaar zijn.
