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
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-cli --version 5.1.0 --locked
pip install assay-it
assay-v5.1.0-x86_64-pc-windows-msvc.zip
DOC
cat > "$TMP/docs/getting-started/index.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
pip install assay-it
DOC
printf '%s\n' 'supported examples' > "$TMP/docs/getting-started/ci-integration.md"
printf '%s\n' 'cargo install assay-cli --version 5.1.0 --locked' > "$TMP/docs/getting-started/quickstart.md"
cat > "$TMP/docs/reference/cli/index.md" <<'DOC'
# assay 5.1.0
cargo install assay-cli --version 5.1.0 --locked
DOC
printf '%s\n' 'pip install assay-it' > "$TMP/docs/python-sdk/index.md"
printf '%s\n' 'Build an image locally.' > "$TMP/docs/use-cases/air-gapped.md"
cat > "$TMP/docs/AIcontext/user-flows.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
Install the SDK with pip install assay-it.
DOC
printf '%s\n' 'Historical correction: pip install assay-it.' > "$TMP/docs/migration-v1.2.md"
cat > "$TMP/README.md" <<'DOC'
cargo install assay-cli --version 5.1.0 --locked
Current release: [`v5.1.0`](https://github.com/Rul1an/assay/releases/tag/v5.1.0)
Claude and Cursor config-path only.
DOC
printf '%s\n' 'Current release: [`v5.1.0`](https://github.com/Rul1an/assay/releases/tag/v5.1.0)' > "$TMP/docs/index.md"
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
  's/Build an image locally./docker pull ghcr.io\/rul1an\/assay:latest/' 'unsupported GHCR image'
mutate_and_expect_failure stale-windows-asset docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/assay-windows-x86_64.zip/' 'obsolete Windows asset name'
mutate_and_expect_failure unsupported-codex-config-path README.md \
  's/Claude and Cursor config-path only./assay mcp config-path <editor>/' \
  'config-path does not support Codex'
mutate_and_expect_failure unsupported-codex-literal README.md \
  's/Claude and Cursor config-path only./assay mcp config-path codex/' \
  'config-path does not support Codex'
mutate_and_expect_failure unsupported-codex-guide docs/guides/editor-mcp-recipe.md \
  's/Codex uses .codex\/config.toml./assay mcp config-path codex/' \
  'config-path does not support Codex'

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

mutate_and_expect_failure stale-release-readme README.md \
  's/releases\/tag\/v5.1.0/releases\/tag\/v5.0.0/' 'current release link drift'
mutate_and_expect_failure stale-release-doc-index docs/index.md \
  's/releases\/tag\/v5.1.0/releases\/tag\/v5.0.0/' 'current release link drift'
mutate_and_expect_failure stale-cli-version docs/reference/cli/index.md \
  's/# assay 5.1.0/# assay 5.0.0/' 'documented CLI version drift'

if [ "$mutation_count" -ne 25 ]; then
  echo "FAIL: expected 25 release-surface mutations, observed $mutation_count" >&2
  exit 1
fi
echo "release-surface mutations: $mutation_count observed"
