#!/usr/bin/env bash
# Self-test for scripts/ci/lib/editor-plugin-install-commands.sh.
#
# The extractor owns parsing of the Claude Code plugin install fence. Guards must
# not reimplement that awk. This battery drives the sourceable function and the
# executable entry against fixtures, so a later semantic change cannot hide behind
# a guard that still happens to pass.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LIB="$ROOT/scripts/ci/lib/editor-plugin-install-commands.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cases=0
failures=0

if [ ! -f "$LIB" ]; then
  printf 'FAIL: extractor missing: %s\n' "$LIB"
  exit 1
fi

# shellcheck source=scripts/ci/lib/editor-plugin-install-commands.sh
source "$LIB"

write_recipe() {
  cat > "$TMP/recipe.md"
}

run_exe() {
  bash "$LIB" "$TMP/recipe.md" 2>"$TMP/err"
}

run_fn() {
  editor_plugin_install_commands "$TMP/recipe.md" 2>"$TMP/err"
}

expect_stdout() {
  local label="$1" expected="$2"
  local got
  cases=$((cases + 1))
  if ! got="$(run_exe)"; then
    failures=$((failures + 1))
    printf 'FAIL: %s — executable extractor exited non-zero\n%s\n' "$label" "$(cat "$TMP/err")"
    return
  fi
  if [ "$got" != "$expected" ]; then
    failures=$((failures + 1))
    printf 'FAIL: %s — executable stdout mismatch\nexpected:\n%s\ngot:\n%s\n' \
      "$label" "$expected" "$got"
    return
  fi
  cases=$((cases + 1))
  if ! got="$(run_fn)"; then
    failures=$((failures + 1))
    printf 'FAIL: %s — sourced function exited non-zero\n%s\n' "$label" "$(cat "$TMP/err")"
    return
  fi
  if [ "$got" != "$expected" ]; then
    failures=$((failures + 1))
    printf 'FAIL: %s — sourced stdout mismatch\nexpected:\n%s\ngot:\n%s\n' \
      "$label" "$expected" "$got"
    return
  fi
  printf 'ok   %s\n' "$label"
}

expect_empty() {
  local label="$1"
  expect_stdout "$label" ""
}

expect_fail() {
  local label="$1" expected="$2"
  local got case_failed=0
  cases=$((cases + 1))
  if got="$(run_exe)"; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — executable extractor accepted input it should reject\n%s\n' \
      "$label" "$got"
  elif [ -n "$got" ]; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — executable extractor emitted stdout before rejecting\n%s\n' \
      "$label" "$got"
  elif ! grep -Fq -- "$expected" "$TMP/err"; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — executable rejected, but not for the stated reason (wanted %s)\n%s\n' \
      "$label" "$expected" "$(cat "$TMP/err")"
  fi

  if got="$(run_fn)"; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — sourced extractor accepted input it should reject\n%s\n' \
      "$label" "$got"
  elif [ -n "$got" ]; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — sourced extractor emitted stdout before rejecting\n%s\n' \
      "$label" "$got"
  elif ! grep -Fq -- "$expected" "$TMP/err"; then
    failures=$((failures + 1))
    case_failed=1
    printf 'FAIL: %s — sourced function rejected, but not for the stated reason (wanted %s)\n%s\n' \
      "$label" "$expected" "$(cat "$TMP/err")"
  fi
  if [ "$case_failed" -eq 0 ]; then
    printf 'ok   %s\n' "$label"
  fi
}

echo '== baseline: plugin bash fence commands are emitted =='
write_recipe <<'MD'
# Title

## Install the Claude Code plugin

```bash
# comment should be skipped
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-mcp-server --version 5.1.0 --locked

assay version
assay-mcp-server --version
# assay-editor-plugin-install-commands:end
```

## Install Assay's review tools

```bash
cargo install --path crates/assay-mcp-server --locked
```
MD
expect_stdout 'plugin fence commands; path-elsewhere omitted' \
  $'cargo install assay-cli --version 5.1.0 --locked\ncargo install assay-mcp-server --version 5.1.0 --locked\nassay version\nassay-mcp-server --version'

echo '== comments and blank lines in the fence are not commands =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
# cargo install assay-cli --version 5.1.0 --locked
   # indented comment

cargo install assay-mcp-server --version 5.1.0 --locked
# assay-editor-plugin-install-commands:end
```
MD
expect_stdout 'comment skip keeps only non-comment lines' \
  'cargo install assay-mcp-server --version 5.1.0 --locked'

echo '== H2 scoping: only the exact plugin heading =='
write_recipe <<'MD'
## Install the Claude Code Plugin

```bash
cargo install assay-cli --version 5.1.0 --locked
```

## Install the Claude Code plugin extra

```bash
cargo install assay-mcp-server --version 5.1.0 --locked
```
MD
expect_fail 'wrong H2 text is not exactly one plugin heading' \
  'expected exactly one plugin install heading'

write_recipe <<'MD'
## Something else

```bash
cargo install assay-cli --version 5.1.0 --locked
assay version
```
MD
expect_fail 'other H2 is not exactly one plugin heading' \
  'expected exactly one plugin install heading'

echo '== fence scoping: only the exact ```bash opener =='
write_recipe <<'MD'
## Install the Claude Code plugin

```sh
cargo install assay-cli --version 5.1.0 --locked
```

```
assay version
```

~~~bash
assay-mcp-server --version
~~~
MD
expect_fail 'non-bash fences are not exactly one bash fence' \
  'expected exactly one bash fence in the plugin section'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
# assay-editor-plugin-install-commands:end
```
MD
expect_stdout 'exact ```bash fence is extracted' 'assay version'

echo '== duplicate matching H2 is not exactly one heading =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
```

## Other

```bash
ignored
```

## Install the Claude Code plugin

```bash
assay-mcp-server --version
# assay-editor-plugin-install-commands:end
```
MD
expect_fail 'duplicate plugin H2 concatenates both fences' \
  'expected exactly one plugin install heading'

echo '== split commands across two plugin H2s is a heading failure =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
cargo install assay-cli --version 5.1.0 --locked
assay version
```

## Install the Claude Code plugin

```bash
cargo install assay-mcp-server --version 5.1.0 --locked
assay-mcp-server --version
# assay-editor-plugin-install-commands:end
```
MD
expect_fail 'split CLI/MCP commands across two plugin H2s' \
  'expected exactly one plugin install heading'

echo '== split commands across two bash fences is a fence failure =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
cargo install assay-cli --version 5.1.0 --locked
assay version
```

```bash
cargo install assay-mcp-server --version 5.1.0 --locked
assay-mcp-server --version
# assay-editor-plugin-install-commands:end
```
MD
expect_fail 'split CLI/MCP commands across two bash fences' \
  'expected exactly one bash fence in the plugin section'

echo '== the plugin bash fence must close before a later fenced block =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
cargo install assay-cli --version 5.1.0 --locked
cargo install assay-mcp-server --version 5.1.0 --locked
assay version
assay-mcp-server --version
# assay-editor-plugin-install-commands:end

### Update stale state

```sh
claude plugin update assay@assay --scope local
```
MD
expect_fail 'later fenced block cannot close the plugin bash fence' \
  'plugin bash fence is not closed at its own boundary'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
# assay-editor-plugin-install-commands:end
MD
expect_fail 'plugin bash fence left open at end of file' \
  'plugin bash fence is not closed at its own boundary'

echo '== only the explicit marker can establish the plugin bash fence boundary =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
# assay-editor-plugin-install-commands:end

Continue with the verification notes below.

## Verify the installation
Run both version probes before continuing.
```
MD
expect_fail 'later bare fence cannot close across an apparent next H2' \
  'plugin bash fence end marker must immediately precede its close'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
```
MD
expect_fail 'plugin bash fence end marker is required' \
  'expected exactly one plugin bash fence end marker'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
# assay-editor-plugin-install-commands:end
# assay-editor-plugin-install-commands:end
```
MD
expect_fail 'plugin bash fence end marker must be unique' \
  'expected exactly one plugin bash fence end marker'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
# assay-editor-plugin-install-commands:end
assay version
```
MD
expect_fail 'plugin bash fence end marker cannot move away from close' \
  'plugin bash fence end marker must immediately precede its close'

write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version

## shell comment
assay-mcp-server --version
# assay-editor-plugin-install-commands:end
```
MD
expect_stdout 'in-fence H2-shaped shell comment remains supported' \
  $'assay version\nassay-mcp-server --version'

echo '== quoted and continued lines are emitted as standalone text =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
cargo install "assay-cli" --version 5.1.0 --locked
cargo install assay-cli \
  --version 5.1.0 --locked
# assay-editor-plugin-install-commands:end
```
MD
expect_stdout 'extractor does not join or unquote cargo lines' \
  $'cargo install "assay-cli" --version 5.1.0 --locked\ncargo install assay-cli \\\n--version 5.1.0 --locked'

echo '== missing plugin H2 is not exactly one heading =='
write_recipe <<'MD'
# no plugin heading

```bash
cargo install assay-cli --version 5.1.0 --locked
```
MD
expect_fail 'missing plugin H2 is not exactly one heading' \
  'expected exactly one plugin install heading'

echo '== recipe byte ceiling =='
write_recipe <<'MD'
## Install the Claude Code plugin

```bash
assay version
# assay-editor-plugin-install-commands:end
```
MD
bytes="$(wc -c < "$TMP/recipe.md")"
printf '%*s' "$((65536 - bytes))" '' >> "$TMP/recipe.md"
cases=$((cases + 1))
if ! run_exe >/dev/null; then
  failures=$((failures + 1))
  printf 'FAIL: recipe at 65536 bytes rejected\n%s\n' "$(cat "$TMP/err")"
else
  printf 'ok   recipe at 65536 bytes accepted\n'
fi
printf ' ' >> "$TMP/recipe.md"
expect_fail 'recipe over 65536 bytes rejected' 'recipe exceeds 65536-byte limit'

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'editor plugin install extractor self-test: %s of %s case(s) failed\n' \
    "$failures" "$cases"
  exit 1
fi
printf 'editor plugin install extractor self-test: %s case(s) PASS\n' "$cases"
