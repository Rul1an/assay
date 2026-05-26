#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

limit=600
files=(
  "crates/assay-evidence/src/trust_basis/tests.rs"
  "crates/assay-adapter-a2a/src/adapter_impl/tests.rs"
  "crates/assay-evidence/src/trust_card.rs"
  "crates/assay-cli/tests/evidence_test.rs"
  "crates/assay-cli/tests/receipt_schema_registry_test.rs"
  "crates/assay-core/tests/mcp_transport_compat.rs"
  "crates/assay-core/src/storage/store.rs"
  "crates/assay-core/src/vcr/mod.rs"
  "crates/assay-cli/src/cli/commands/profile.rs"
)

failed=0
for f in "${files[@]}"; do
  lines=$(wc -l < "$f")
  printf '%4d %s\n' "$lines" "$f"
  if [ "$lines" -ge "$limit" ]; then
    echo "LOC gate failed: $f has $lines lines (must be < $limit)" >&2
    failed=1
  fi
done

if [ "$failed" -ne 0 ]; then
  exit 1
fi

echo "LOC gate passed: all target files are < $limit lines."
