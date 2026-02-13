# Wave 2 Step 1 inventory (behavior freeze)

Scope:
- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-core/src/runtime/mandate_store.rs`

Scope lock:
- tests + docs + gates only
- no split/mechanical moves yet
- no perf tuning
- `demo/` untouched

## HEAD snapshot

- commit: `9587f5e9f10260a08099e91657b2ceb17b0dcc6c`
- LOC:
  - `runner.rs`: 1042
  - `mandate_store.rs`: 1046

## Public entrypoints (current)

`runner.rs`
- `pub async fn run_suite(...)`
- `pub async fn embed_text(...)`
- `pub struct RunPolicy`
- `pub struct Runner`

`mandate_store.rs`
- `pub fn open(...)`
- `pub fn memory(...)`
- `pub fn from_connection(...)`
- `pub fn upsert_mandate(...)`
- `pub fn consume_mandate(...)`
- `pub fn get_use_count(...)`
- `pub fn count_uses(...)`
- `pub fn nonce_exists(...)`
- `pub fn upsert_revocation(...)`
- `pub fn get_revoked_at(...)`
- `pub fn is_revoked(...)`
- `pub fn compute_use_id(...)`
- `pub struct AuthzReceipt`
- `pub enum AuthzError`
- `pub struct MandateMetadata`
- `pub struct ConsumeParams<'a>`
- `pub struct MandateStore`
- `pub struct RevocationRecord`

## Baseline drift counters (Step 1)

Current counts:
- `runner.rs`
  - `unwrap(`: 0
  - `expect(`: 8
  - `unsafe`: 0
  - `println!/eprintln!`: 2
  - `std::process::Command`: 0
  - `tokio::spawn`: 0
- `mandate_store.rs`
  - `unwrap(`: 84
  - `expect(`: 0
  - `unsafe`: 0
  - `println!/eprintln!`: 0
  - `std::process::Command`: 0
  - `tokio::spawn`: 0

## Drift gates (copy/paste)

```bash
set -euo pipefail

base_ref="origin/main"

count_in_ref() {
  local ref="$1"
  local file="$2"
  local pattern="$3"
  git show "${ref}:${file}" | $(command -v rg) -n "$pattern" | wc -l | tr -d ' '
}

count_in_worktree() {
  local file="$1"
  local pattern="$2"
  $(command -v rg) -n "$pattern" "$file" | wc -l | tr -d ' '
}

check_no_increase() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local before after
  before="$(count_in_ref "$base_ref" "$file" "$pattern")"
  after="$(count_in_worktree "$file" "$pattern")"
  echo "$label: before=$before after=$after"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: $label increased"
    exit 1
  fi
}

check_no_increase "crates/assay-core/src/engine/runner.rs" "unwrap\(|expect\(" "runner unwrap/expect"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "unwrap\(|expect\(" "mandate_store unwrap/expect"
check_no_increase "crates/assay-core/src/engine/runner.rs" "\bunsafe\b" "runner unsafe"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "\bunsafe\b" "mandate_store unsafe"
check_no_increase "crates/assay-core/src/engine/runner.rs" "println!|eprintln!" "runner stdout/stderr"
check_no_increase "crates/assay-core/src/engine/runner.rs" "std::process::Command" "runner process command"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "tokio::spawn" "mandate_store tokio spawn"
```
