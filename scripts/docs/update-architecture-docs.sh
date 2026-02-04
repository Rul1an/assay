#!/usr/bin/env bash
# Update architecture documentation with generated diagrams
set -euo pipefail

GENERATED_DIR="docs/generated"
ARCH_DIAGRAMS="docs/AIcontext/architecture-diagrams.md"
CODE_MAP="docs/AIcontext/code-map.md"

echo "Updating architecture documentation..."

# Get current version from Cargo.toml
VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
echo "Current version: $VERSION"

# Update version in code-map.md if it exists
if [[ -f "$CODE_MAP" ]]; then
    # Update version comment in the file structure
    sed -i.bak "s/# Version [0-9.]*$/# Version $VERSION/" "$CODE_MAP" 2>/dev/null || \
    sed -i '' "s/# Version [0-9.]*$/# Version $VERSION/" "$CODE_MAP"
    rm -f "${CODE_MAP}.bak"
    echo "Updated version in $CODE_MAP"
fi

# Check if generated files exist
if [[ ! -f "$GENERATED_DIR/crate-deps.mermaid" ]]; then
    echo "Warning: $GENERATED_DIR/crate-deps.mermaid not found, skipping diagram update"
    exit 0
fi

# Create a marker section in architecture-diagrams.md if it doesn't exist
if ! grep -q "## Generated Diagrams" "$ARCH_DIAGRAMS" 2>/dev/null; then
    cat >> "$ARCH_DIAGRAMS" << 'EOF'

## Generated Diagrams

The following diagrams are automatically generated from the codebase.

### Crate Dependencies (Auto-Generated)

<!-- BEGIN:CRATE_DEPS -->
```mermaid
flowchart TB
    note["Run 'scripts/docs/generate-crate-deps.sh' to update"]
```
<!-- END:CRATE_DEPS -->

### Module Structure (Auto-Generated)

<!-- BEGIN:MODULE_MAP -->
```mermaid
flowchart TB
    note["Run 'scripts/docs/generate-module-map.sh' to update"]
```
<!-- END:MODULE_MAP -->
EOF
    echo "Added Generated Diagrams section to $ARCH_DIAGRAMS"
fi

# Function to replace content between markers using Python (more reliable for multiline)
replace_between_markers() {
    local file="$1"
    local begin_marker="$2"
    local end_marker="$3"
    local content_file="$4"

    python3 << PYEOF
import sys

with open('$file', 'r') as f:
    content = f.read()

with open('$content_file', 'r') as f:
    new_content = f.read()

begin_marker = '$begin_marker'
end_marker = '$end_marker'

begin_idx = content.find(begin_marker)
end_idx = content.find(end_marker)

if begin_idx == -1 or end_idx == -1:
    print(f"Markers not found: {begin_marker} / {end_marker}")
    sys.exit(1)

# Find the end of the begin marker line
begin_line_end = content.find('\n', begin_idx)

# Build new content
result = content[:begin_line_end + 1]
result += '\`\`\`mermaid\n'
result += new_content
result += '\n\`\`\`\n'
result += content[end_idx:]

with open('$file', 'w') as f:
    f.write(result)

print("Updated successfully")
PYEOF
}

# Update crate dependencies diagram
if [[ -f "$GENERATED_DIR/crate-deps.mermaid" ]]; then
    replace_between_markers "$ARCH_DIAGRAMS" "<!-- BEGIN:CRATE_DEPS -->" "<!-- END:CRATE_DEPS -->" "$GENERATED_DIR/crate-deps.mermaid"
    echo "Updated crate dependencies diagram"
fi

# Update module map diagram
if [[ -f "$GENERATED_DIR/module-map.mermaid" ]]; then
    replace_between_markers "$ARCH_DIAGRAMS" "<!-- BEGIN:MODULE_MAP -->" "<!-- END:MODULE_MAP -->" "$GENERATED_DIR/module-map.mermaid"
    echo "Updated module map diagram"
fi

echo "Architecture docs updated successfully"
