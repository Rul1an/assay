#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts/ci" "$TMP/docs/getting-started" "$TMP/docs/reference/cli" \
  "$TMP/docs/python-sdk" "$TMP/docs/use-cases" "$TMP/docs/AIcontext" "$TMP/docs/guides" \
  "$TMP/crates/assay-x" "$TMP/bin"
cp "$ROOT/scripts/ci/check-release-surface.sh" "$TMP/scripts/ci/"
cat > "$TMP/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/assay-x"]
[workspace.package]
version = "5.1.0"
[workspace.dependencies]
assay-x = { version = "5.1.0", path = "crates/assay-x" }
TOML
cat > "$TMP/crates/assay-x/Cargo.toml" <<'TOML'
[package]
name = "assay-x"
version.workspace = true
TOML
cat > "$TMP/Cargo.lock" <<'LOCK'
[[package]]
name = "assay-x"
version = "5.1.0"
LOCK
cat > "$TMP/bin/assay" <<'EOF_BIN'
#!/usr/bin/env bash
printf 'assay 5.1.0\n'
EOF_BIN
chmod +x "$TMP/bin/assay"
cat > "$TMP/docs/getting-started/installation.md" <<'DOC'
assay 5.1.0
assay-v5.1.0-x86_64-pc-windows-msvc.zip
DOC
printf '%s\n' 'pip install assay-it' > "$TMP/docs/getting-started/index.md"
printf '%s\n' 'supported examples' > "$TMP/docs/getting-started/ci-integration.md"
printf '%s\n' '# assay 5.1.0' > "$TMP/docs/reference/cli/index.md"
printf '%s\n' 'pip install assay-it' > "$TMP/docs/python-sdk/index.md"
printf '%s\n' 'Build an image locally.' > "$TMP/docs/use-cases/air-gapped.md"
printf '%s\n' 'Install the CLI with Cargo; install the SDK with pip install assay-it.' > "$TMP/docs/AIcontext/user-flows.md"
printf '%s\n' 'Historical correction: pip install assay-it.' > "$TMP/docs/migration-v1.2.md"
printf '%s\n' 'Claude and Cursor config-path only.' > "$TMP/README.md"
printf '%s\n' 'Codex uses .codex/config.toml.' > "$TMP/docs/guides/editor-mcp-recipe.md"
(
  cd "$TMP"
  git init -q
  git add -- Cargo.toml Cargo.lock crates docs scripts
)

run_check() {
  (cd "$TMP" && ASSAY_BIN="$TMP/bin/assay" bash scripts/ci/check-release-surface.sh)
}

run_check >/dev/null

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
  echo "PASS: $name"
}

mutate_and_expect_failure wrong-python-package docs/getting-started/index.md \
  's/pip install assay-it/pip install assay/' 'unsupported Python package'
mutate_and_expect_failure homebrew-channel docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/brew install rul1an\/tap\/assay/' 'unsupported Homebrew channel'
mutate_and_expect_failure scoop-channel docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/scoop install assay/' 'unsupported Scoop channel'
mutate_and_expect_failure ghcr-channel docs/getting-started/ci-integration.md \
  's/supported examples/docker pull ghcr.io\/rul1an\/assay:latest/' 'unsupported GHCR image'
mutate_and_expect_failure stale-windows-asset docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/assay-windows-x86_64.zip/' 'obsolete Windows asset name'
mutate_and_expect_failure unsupported-codex-config-path README.md \
  's/Claude and Cursor config-path only./assay mcp config-path <editor>/' \
  'config-path does not support Codex'

echo 'release-surface mutations: 6 observed'
