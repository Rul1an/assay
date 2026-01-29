#!/bin/bash
# CI guardrail: Ensure no enterprise pack content in OSS repo
# This script prevents accidental inclusion of commercial content in the open source repository.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

errors=0

echo "=== Open Core Boundary Check ==="
echo ""

# 1. Check that packs/enterprise/ contains no pack.yaml files
echo "Checking packs/enterprise/ for pack content..."
if find "$REPO_ROOT/packs/enterprise" -name "pack.yaml" -o -name "*.yaml" ! -name "README.md" 2>/dev/null | grep -q .; then
    echo "ERROR: Found pack files in packs/enterprise/"
    echo "       Enterprise packs should be distributed via registry, not in OSS repo."
    find "$REPO_ROOT/packs/enterprise" -name "pack.yaml" -o -name "*.yaml" ! -name "README.md" 2>/dev/null
    errors=$((errors + 1))
else
    echo "OK: packs/enterprise/ contains no pack content"
fi
echo ""

# 2. Check for enterprise pack references in open packs
echo "Checking for enterprise pack references in open packs..."
if grep -rn "eu-ai-act-pro\|soc2-pro\|hipaa-pro" "$REPO_ROOT/packs/open" 2>/dev/null | grep -v README; then
    echo "ERROR: Found enterprise pack references in open packs"
    errors=$((errors + 1))
else
    echo "OK: No enterprise pack references in open packs"
fi
echo ""

# 3. Check for PRO- prefixed rule IDs in open packs
echo "Checking for PRO- rule IDs in open packs..."
if grep -rn "id: PRO-" "$REPO_ROOT/packs/open" 2>/dev/null; then
    echo "ERROR: Found PRO- prefixed rule IDs in open packs"
    echo "       PRO- prefix is reserved for enterprise packs."
    errors=$((errors + 1))
else
    echo "OK: No PRO- rule IDs in open packs"
fi
echo ""

# 4. Check that all open packs have LICENSE files
echo "Checking LICENSE files in open packs..."
for pack_dir in "$REPO_ROOT/packs/open"/*/; do
    if [ -d "$pack_dir" ]; then
        pack_name=$(basename "$pack_dir")
        if [ ! -f "$pack_dir/LICENSE" ]; then
            echo "ERROR: Missing LICENSE file in packs/open/$pack_name/"
            errors=$((errors + 1))
        else
            echo "OK: packs/open/$pack_name/ has LICENSE"
        fi
    fi
done
echo ""

# 5. Check that open pack licenses are OSS-compatible
echo "Checking open pack licenses are OSS-compatible..."
for license_file in "$REPO_ROOT/packs/open"/*/LICENSE; do
    if [ -f "$license_file" ]; then
        pack_name=$(basename "$(dirname "$license_file")")
        if grep -q "Apache License" "$license_file" || grep -q "MIT License" "$license_file"; then
            echo "OK: packs/open/$pack_name/ has OSS license"
        else
            echo "WARNING: packs/open/$pack_name/ license may not be OSS-compatible"
            echo "         Expected Apache-2.0 or MIT"
        fi
    fi
done
echo ""

# Summary
echo "=== Summary ==="
if [ $errors -eq 0 ]; then
    echo "All checks passed."
    exit 0
else
    echo "Found $errors error(s)."
    echo "See ADR-016 for open core boundary definition."
    exit 1
fi
