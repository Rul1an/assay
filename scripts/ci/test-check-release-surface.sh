#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts/ci" "$TMP/.github" "$TMP/docs/getting-started" "$TMP/docs/reference/cli" \
  "$TMP/docs/python-sdk" "$TMP/docs/use-cases" "$TMP/docs/AIcontext" "$TMP/docs/guides" \
  "$TMP/examples/mcp-quickstart" "$TMP/crates/assay-x" "$TMP/bin"
cp "$ROOT/scripts/ci/check-release-surface.sh" "$TMP/scripts/ci/"
cp "$ROOT/scripts/ci/read-assay-release-tag.sh" "$TMP/scripts/ci/"
cp "$ROOT/.pre-commit-config.yaml" "$TMP/"
printf '%s\n' 'v5.1.0' > "$TMP/.github/assay-release-tag"
cat > "$TMP/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/assay-x"]
[workspace.package]
version = "5.2.0"
[workspace.dependencies]
assay-x = { version = "5.2.0", path = "crates/assay-x" }
TOML
cat > "$TMP/crates/assay-x/Cargo.toml" <<'TOML'
[package]
name = "assay-x"
version.workspace = true
TOML
cat > "$TMP/Cargo.lock" <<'LOCK'
[[package]]
name = "assay-x"
version = "5.2.0"
LOCK
cat > "$TMP/bin/assay" <<'EOF_BIN'
#!/usr/bin/env bash
printf 'assay 5.2.0\n'
EOF_BIN
chmod +x "$TMP/bin/assay"
cat > "$TMP/docs/getting-started/installation.md" <<'DOC'
assay 5.1.0
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-cli --version 5.1.0 --locked
pip install assay-it
assay-v5.1.0-x86_64-pc-windows-msvc.zip
DOC
cat > "$TMP/docs/getting-started/index.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
pip install assay-it
DOC
cat > "$TMP/docs/getting-started/ci-integration.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-cli --version 5.1.0 --locked
uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
supported examples
DOC
cat > "$TMP/docs/getting-started/quickstart.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
uses: Rul1an/assay-action@f0c2125a73621830bcdf0b98355382c810df058b
DOC
cat > "$TMP/docs/reference/cli/index.md" <<'DOC'
# assay 5.1.0
cargo install assay-cli --version 5.1.0 --locked
DOC
printf '%s\n' 'pip install assay-it' > "$TMP/docs/python-sdk/index.md"
cat > "$TMP/docs/use-cases/air-gapped.md" <<'DOC'
curl -fLO https://github.com/Rul1an/assay/releases/download/v5.1.0/assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz
curl -fLO https://github.com/Rul1an/assay/releases/download/v5.1.0/assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
assay-v5.1.0-x86_64-unknown-linux-gnu/assay
No runtime image is shipped.
DOC
cat > "$TMP/docs/use-cases/ci-gate.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
uses: github/codeql-action/upload-sarif@d1ba80a13dd99fba24a470575428917156a28b43
DOC
cat > "$TMP/docs/AIcontext/user-flows.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
Install the SDK with pip install assay-it.
uses: github/codeql-action/upload-sarif@d1ba80a13dd99fba24a470575428917156a28b43
DOC
printf '%s\n' 'Historical correction: pip install assay-it.' > "$TMP/docs/migration-v1.2.md"
rge_claim='reproduction is digest-scoped and does not carry forward: v1 71-vector digest `sha256:1111111111111111111111111111111111111111111111111111111111111111` and current v2 digest `sha256:2222222222222222222222222222222222222222222222222222222222222222` (95 vectors) each carry one reported **independent implementation**; JM-Lab reported the v2 95/95 reproduction on 2026-08-24.'
broad_rge_claim='neutral, externally reproduced conformance kit for evidence reviewability'
cat > "$TMP/README.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
Current release: [`v5.1.0`](https://github.com/Rul1an/assay/releases/tag/v5.1.0)
Claude and Cursor config-path only.
DOC
printf '%s\n' "- [RGE-Bench](https://github.com/rge-bench/rge-bench) — $rge_claim" >> "$TMP/README.md"
printf '%s\n' "- [RGE-Bench](https://github.com/rge-bench/rge-bench): $rge_claim" > "$TMP/llms.txt"
cat > "$TMP/examples/mcp-quickstart/README.md" <<'DOC'
Assay CLI via the release installer.
For v5.4.0, run this from a source checkout. Published CLI archives do not carry this quickstart.
DOC
printf '%s\n' 'Current release: [`v5.1.0`](https://github.com/Rul1an/assay/releases/tag/v5.1.0)' > "$TMP/docs/index.md"
printf '%s\n' 'Codex uses .codex/config.toml.' > "$TMP/docs/guides/editor-mcp-recipe.md"
(
  cd "$TMP"
  git init -q
  git add -- .github/assay-release-tag Cargo.toml Cargo.lock crates docs examples scripts
)

run_check() {
  (cd "$TMP" && ASSAY_BIN="$TMP/bin/assay" bash scripts/ci/check-release-surface.sh)
}

check_release_hook_selector() {
  python3 - "$TMP/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
start = next(i for i, line in enumerate(lines) if line.strip() == "- id: release-surface")
end = next((i for i in range(start + 1, len(lines)) if lines[i].lstrip().startswith("- id:")), len(lines))
files_lines = [line.strip().removeprefix("files: ") for line in lines[start:end] if line.strip().startswith("files: ")]
if len(files_lines) != 1:
    raise SystemExit("release-surface hook must have one files selector")
pattern = re.compile(files_lines[0])
for path in (
    ".github/assay-release-tag",
    "scripts/ci/read-assay-release-tag.sh",
    "llms.txt",
    "examples/mcp-quickstart/README.md",
    "docs/getting-started/ci-integration.md",
    "docs/use-cases/air-gapped.md",
    "docs/use-cases/ci-gate.md",
):
    if not pattern.search(path):
        raise SystemExit(f"release-surface hook omits {path}")
PY
}

if ! run_check >"$TMP/baseline.out" 2>&1; then
  cat "$TMP/baseline.out" >&2
  echo "FAIL: release surface must allow workspace 5.2.0 to lead published release v5.1.0" >&2
  exit 1
fi
check_release_hook_selector

mutation_count=0
mutate_and_expect_failure() {
  local name="$1" file="$2" sed_expr="$3" diagnostic="$4"
  local original="$TMP/$file" backup="$TMP/$file.$name"
  cp "$original" "$backup"
  sed -i.bak -e "$sed_expr" "$original"
  rm -f "$original.bak"
  if run_check >"$TMP/$name.out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$TMP/$name.out" || {
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    cat "$TMP/$name.out" >&2
    exit 1
  }
  mv "$backup" "$original"
  mutation_count=$((mutation_count + 1))
  echo "PASS: $name"
}

mutate_rge_pair_and_expect_failure() {
  local name="$1" mode="$2" diagnostic="$3"
  local readme_backup="$TMP/README.md.$name" llms_backup="$TMP/llms.txt.$name"
  cp "$TMP/README.md" "$readme_backup"
  cp "$TMP/llms.txt" "$llms_backup"

  case "$mode" in
    replace)
      sed -i.bak -e "s#^- \[RGE-Bench\].*#- [RGE-Bench](https://github.com/rge-bench/rge-bench) — $broad_rge_claim#" "$TMP/README.md"
      sed -i.bak -e "s#^- \[RGE-Bench\].*#- [RGE-Bench](https://github.com/rge-bench/rge-bench): $broad_rge_claim#" "$TMP/llms.txt"
      rm -f "$TMP/README.md.bak" "$TMP/llms.txt.bak"
      ;;
    append)
      printf '%s\n' "- [RGE-Bench](https://github.com/rge-bench/rge-bench) — $broad_rge_claim" >> "$TMP/README.md"
      printf '%s\n' "- [RGE-Bench](https://github.com/rge-bench/rge-bench): $broad_rge_claim" >> "$TMP/llms.txt"
      ;;
    *)
      echo "FAIL: unknown RGE mutation mode: $mode" >&2
      exit 1
      ;;
  esac

  if run_check >"$TMP/$name.out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$TMP/$name.out" || {
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    cat "$TMP/$name.out" >&2
    exit 1
  }
  mv "$readme_backup" "$TMP/README.md"
  mv "$llms_backup" "$TMP/llms.txt"
  mutation_count=$((mutation_count + 1))
  echo "PASS: $name"
}

mutate_and_expect_failure wrong-python-package docs/getting-started/index.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-installation docs/getting-started/installation.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-sdk docs/python-sdk/index.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-flow docs/AIcontext/user-flows.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-migration docs/migration-v1.2.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-upgrade docs/python-sdk/index.md \
  's/pip install assay-it/pip install --upgrade assay/' 'unsupported Python package'
mutate_and_expect_failure wrong-python-package-short-upgrade docs/python-sdk/index.md \
  "s/pip install assay-it/pip install -U 'assay'/" 'unsupported Python package'
mutate_and_expect_failure homebrew-channel docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/brew install rul1an\/tap\/assay/' 'unsupported Homebrew channel'
mutate_and_expect_failure scoop-channel docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/scoop install assay/' 'unsupported Scoop channel'
mutate_and_expect_failure ghcr-channel docs/getting-started/ci-integration.md \
  's/supported examples/docker pull ghcr.io\/rul1an\/assay:latest/' 'unsupported GHCR image'
mutate_and_expect_failure ghcr-installation docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/docker pull ghcr.io\/rul1an\/assay:latest/' 'unsupported GHCR image'
mutate_and_expect_failure ghcr-air-gapped docs/use-cases/air-gapped.md \
  's/No runtime image is shipped./docker pull ghcr.io\/rul1an\/assay:latest/' 'unsupported GHCR image'
mutate_and_expect_failure stale-windows-asset docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/assay-windows-x86_64.zip/' 'obsolete Windows asset name'
mutate_and_expect_failure unsupported-codex-config-path README.md \
  's/Claude and Cursor config-path only./assay mcp config-path <editor>/' \
  'config-path does not support Codex'
mutate_and_expect_failure unsupported-codex-literal README.md \
  's/Claude and Cursor config-path only./assay mcp config-path codex/' \
  'config-path does not support Codex'
mutate_and_expect_failure misleading-installer-verification README.md \
  's/Claude and Cursor config-path only./Claude and Cursor config-path only. Fast path: verified release installer./' \
  'installer wording must not imply checksum or provenance verification'
mutate_and_expect_failure misleading-example-installer examples/mcp-quickstart/README.md \
  's/release installer/verified release installer/' \
  'quickstart installer wording must not imply checksum or provenance verification'
mutate_and_expect_failure extracted-archive-claim examples/mcp-quickstart/README.md \
  's/run this from a source checkout/run this from the root of an extracted CLI release archive/' \
  'quickstart must not claim assets exist in published CLI archives'
mutate_and_expect_failure unsupported-codex-guide docs/guides/editor-mcp-recipe.md \
  's/Codex uses .codex\/config.toml./assay mcp config-path codex/' \
  'config-path does not support Codex'
mutate_and_expect_failure broad-rge-claim llms.txt \
  "s#^- \[RGE-Bench\].*#- [RGE-Bench](https://github.com/rge-bench/rge-bench): $broad_rge_claim#" \
  'RGE-Bench claim must match README.md digest scope'
mutate_rge_pair_and_expect_failure broad-rge-claim-both replace \
  'RGE-Bench claim must remain digest-scoped'
mutate_rge_pair_and_expect_failure duplicate-broad-rge-claim-both append \
  'expected exactly one RGE-Bench claim'
mutate_and_expect_failure wrong-rust-package-ci docs/getting-started/ci-integration.md \
  's/cargo install assay-cli --version 5.1.0 --locked/cargo install assay/' \
  'unsupported Rust CLI package'
mutate_and_expect_failure wrong-rust-package-ci-gate docs/use-cases/ci-gate.md \
  's/cargo install assay-cli --version 5.1.0 --locked/cargo install assay/' \
  'unsupported Rust CLI package'
mutate_and_expect_failure stale-air-gap-version docs/use-cases/air-gapped.md \
  's/releases\/download\/v5.1.0/releases\/download\/v5.0.0/' \
  'current Linux release archive URL drift'
mutate_and_expect_failure obsolete-air-gap-asset docs/use-cases/air-gapped.md \
  's/assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz/assay-linux-x86_64.tar.gz/' \
  'current Linux release archive URL drift'
mutate_and_expect_failure unsupported-runtime-image docs/use-cases/air-gapped.md \
  's/No runtime image is shipped./image: internal-registry.corp\/assay:v5.1.0/' \
  'unsupported runtime image'
mutate_and_expect_failure stale-air-gap-checksum docs/use-cases/air-gapped.md \
  's/\.tar.gz\.sha256/.tar.gz.old.sha256/' \
  'current Linux release checksum URL drift'
mutate_and_expect_failure wrong-air-gap-extraction docs/use-cases/air-gapped.md \
  's/x86_64-unknown-linux-gnu\/assay/x86_64-unknown-linux-gnu\/bin\/assay/' \
  'current Linux release archive extraction drift'
mutate_and_expect_failure mutable-action-ci docs/getting-started/ci-integration.md \
  's/actions\/upload-artifact@[0-9a-f]*/actions\/upload-artifact@v7/' \
  'GitHub Action references must use a full commit SHA (or Rul1an/assay-action@v3)'
mutate_and_expect_failure mutable-action-ci-gate docs/use-cases/ci-gate.md \
  's/github\/codeql-action\/upload-sarif@[0-9a-f]*/github\/codeql-action\/upload-sarif@v4/' \
  'GitHub Action references must use a full commit SHA (or Rul1an/assay-action@v3)'
mutate_and_expect_failure mutable-action-quickstart docs/getting-started/quickstart.md \
  's/Rul1an\/assay-action@[0-9a-f]*/Rul1an\/assay-action@main/' \
  'GitHub Action references must use a full commit SHA (or Rul1an/assay-action@v3)'
mutate_and_expect_failure mutable-action-user-flow docs/AIcontext/user-flows.md \
  's/github\/codeql-action\/upload-sarif@[0-9a-f]*/github\/codeql-action\/upload-sarif@latest/' \
  'GitHub Action references must use a full commit SHA (or Rul1an/assay-action@v3)'
mutate_and_expect_failure missing-air-gap-archive docs/use-cases/air-gapped.md \
  's#curl -fLO https://github.com/Rul1an/assay/releases/download/v5.1.0/assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz$#archive download omitted#' \
  'current Linux release archive URL drift'

selector_backup="$TMP/.pre-commit-config.yaml.selector"
cp "$TMP/.pre-commit-config.yaml" "$selector_backup"
sed -i.bak 's/|ci-gate//' "$TMP/.pre-commit-config.yaml"
rm -f "$TMP/.pre-commit-config.yaml.bak"
if check_release_hook_selector >"$TMP/selector.out" 2>&1; then
  echo "FAIL: release-surface selector mutation was not observed" >&2
  exit 1
fi
grep -Fq 'release-surface hook omits docs/use-cases/ci-gate.md' "$TMP/selector.out"
mv "$selector_backup" "$TMP/.pre-commit-config.yaml"
mutation_count=$((mutation_count + 1))
echo "PASS: release-surface selector covers ci-gate"

for row in \
  'README.md|stale-version-readme' \
  'docs/getting-started/index.md|stale-version-getting-started' \
  'docs/getting-started/installation.md|stale-version-installation' \
  'docs/getting-started/quickstart.md|stale-version-quickstart' \
  'docs/reference/cli/index.md|stale-version-cli-reference' \
  'docs/AIcontext/user-flows.md|stale-version-user-flow'; do
  file="${row%%|*}"
  name="${row##*|}"
  mutate_and_expect_failure "$name" "$file" \
    's/assay-cli --version 5.1.0/assay-cli --version 5.0.0/g' \
    'current release-pinned install command(s)'
done

mutate_and_expect_failure wrong-workspace-binary bin/assay \
  's/assay 5.2.0/assay 5.1.0/' 'workspace is "assay 5.2.0"'
mutate_and_expect_failure wrong-published-version-output docs/getting-started/installation.md \
  's/assay 5.1.0/assay 5.2.0/' 'does not show "assay 5.1.0" as the published'

mutate_and_expect_failure stale-release-readme README.md \
  's/releases\/tag\/v5.1.0/releases\/tag\/v5.0.0/' 'current release link drift'
mutate_and_expect_failure stale-release-doc-index docs/index.md \
  's/releases\/tag\/v5.1.0/releases\/tag\/v5.0.0/' 'current release link drift'

release_backup="$TMP/README.md.duplicate-release"
cp "$TMP/README.md" "$release_backup"
printf '%s\n' 'Current release: [`v5.2.0`](https://github.com/Rul1an/assay/releases/tag/v5.2.0)' \
  >> "$TMP/README.md"
if run_check >"$TMP/duplicate-release.out" 2>&1; then
  echo "FAIL: mutation duplicate-release was not observed" >&2
  exit 1
fi
grep -Fq 'current release link drift' "$TMP/duplicate-release.out"
mv "$release_backup" "$TMP/README.md"
mutation_count=$((mutation_count + 1))
echo "PASS: duplicate-release"

mutate_and_expect_failure stale-cli-version docs/reference/cli/index.md \
  's/# assay 5.1.0/# assay 5.0.0/' 'documented CLI version drift'

if [ "$mutation_count" -ne 47 ]; then
  echo "FAIL: expected 47 release-surface mutations, observed $mutation_count" >&2
  exit 1
fi
echo "release-surface mutations: $mutation_count observed"
