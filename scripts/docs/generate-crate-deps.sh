#!/usr/bin/env bash
# Generate Mermaid diagram of crate dependencies from Cargo.toml
set -euo pipefail

OUTPUT_DIR="docs/generated"
OUTPUT_FILE="$OUTPUT_DIR/crate-deps.mermaid"

mkdir -p "$OUTPUT_DIR"

echo "Generating crate dependency diagram..."

# Get workspace crates and their dependencies using cargo metadata
METADATA=$(cargo metadata --format-version=1 --no-deps 2>/dev/null)

# Extract workspace member names (just the package names, not full identifiers)
WORKSPACE_MEMBERS=$(echo "$METADATA" | jq -r '.packages[].name' | grep '^assay' | sort -u)

# Start Mermaid diagram
cat > "$OUTPUT_FILE" << 'EOF'
flowchart TB
    subgraph workspace["Assay Workspace"]
        direction TB
EOF

# Add nodes for each crate
for crate in $WORKSPACE_MEMBERS; do
    # Clean crate name for Mermaid (remove hyphens for node IDs)
    node_id=$(echo "$crate" | tr '-' '_')
    echo "        ${node_id}[\"${crate}\"]" >> "$OUTPUT_FILE"
done

echo "    end" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Add dependency edges
for crate in $WORKSPACE_MEMBERS; do
    node_id=$(echo "$crate" | tr '-' '_')

    # Get dependencies that are also workspace members
    deps=$(echo "$METADATA" | jq -r --arg crate "$crate" '
        .packages[] |
        select(.name == $crate) |
        .dependencies[]? |
        select(.path != null) |
        .name
    ' 2>/dev/null | sort -u || true)

    for dep in $deps; do
        dep_id=$(echo "$dep" | tr '-' '_')
        # Only add edge if dep is in workspace
        if echo "$WORKSPACE_MEMBERS" | grep -q "^${dep}$"; then
            echo "    ${node_id} --> ${dep_id}" >> "$OUTPUT_FILE"
        fi
    done
done

# Add grouping hints (no colors - let theme handle it)
cat >> "$OUTPUT_FILE" << 'EOF'

    %% Logical groupings for readability
    subgraph core["Core"]
        assay_core
        assay_metrics
        assay_policy
        assay_evidence
        assay_common
    end

    subgraph interface["Interface"]
        assay_cli
        assay_mcp_server
    end

    subgraph runtime["Runtime"]
        assay_monitor
        assay_ebpf
    end

    subgraph support["Support"]
        assay_sim
        assay_xtask
        assay_registry
    end
EOF

echo "Generated: $OUTPUT_FILE"
