#!/usr/bin/env bash
# List handwritten Rust files >800 LOC for split-plan inventory.
# Run from repo root. Excludes auto-generated (vmlinux.rs).
# Usage: ./scripts/largest_rust_files.sh

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "# Generated on $(date -u +%Y-%m-%d) at HEAD $(git rev-parse --short HEAD)"
echo ""
echo "| LOC | File | Crate |"
echo "|-----|------|-------|"

find crates -name "*.rs" -type f ! -path "*/target/*" ! -name "vmlinux.rs" -print0 \
  | xargs -0 wc -l 2>/dev/null \
  | awk '$1 >= 800 && $2 != "total" { print $1, $2 }' \
  | sort -rn \
  | while read -r loc path; do
    # Extract crate from path: crates/assay-registry/src/foo.rs -> assay-registry
    crate=$(echo "$path" | sed -n 's|^crates/\([^/]*\)/.*|\1|p')
    file=$(echo "$path" | sed 's|^crates/[^/]*/||')
    printf "| %s | \`%s\` | %s |\n" "$loc" "$file" "$crate"
  done
