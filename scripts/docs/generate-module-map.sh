#!/usr/bin/env bash
# Generate module structure map for each crate
set -euo pipefail

OUTPUT_DIR="docs/generated"
OUTPUT_FILE="$OUTPUT_DIR/module-map.mermaid"

mkdir -p "$OUTPUT_DIR"

echo "Generating module structure map..."

# Start Mermaid diagram
cat > "$OUTPUT_FILE" << 'EOF'
flowchart TB
    subgraph assay_cli["assay-cli"]
        direction TB
        cli_main["main.rs"]
        cli_dispatch["dispatch"]
        cli_commands["commands/"]
        cli_args["args.rs"]
        cli_main --> cli_dispatch
        cli_dispatch --> cli_commands
        cli_dispatch --> cli_args
    end

    subgraph assay_core["assay-core"]
        direction TB
        core_lib["lib.rs"]
        core_engine["engine/"]
        core_storage["storage/"]
        core_trace["trace/"]
        core_mcp["mcp/"]
        core_report["report/"]
        core_providers["providers/"]
        core_lib --> core_engine
        core_lib --> core_storage
        core_lib --> core_trace
        core_lib --> core_mcp
        core_lib --> core_report
        core_lib --> core_providers
    end

    subgraph assay_metrics["assay-metrics"]
        direction TB
        metrics_lib["lib.rs"]
        metrics_must_contain["must_contain"]
        metrics_semantic["semantic"]
        metrics_regex["regex_match"]
        metrics_schema["json_schema"]
        metrics_args["args_valid"]
        metrics_sequence["sequence_valid"]
        metrics_lib --> metrics_must_contain
        metrics_lib --> metrics_semantic
        metrics_lib --> metrics_regex
        metrics_lib --> metrics_schema
        metrics_lib --> metrics_args
        metrics_lib --> metrics_sequence
    end

    subgraph assay_mcp_server["assay-mcp-server"]
        direction TB
        mcp_main["main.rs"]
        mcp_server["server"]
        mcp_proxy["proxy"]
        mcp_policy["policy"]
        mcp_main --> mcp_server
        mcp_server --> mcp_proxy
        mcp_proxy --> mcp_policy
    end

    subgraph assay_monitor["assay-monitor"]
        direction TB
        mon_lib["lib.rs"]
        mon_events["events"]
        mon_ebpf["ebpf_loader"]
        mon_lib --> mon_events
        mon_lib --> mon_ebpf
    end

    subgraph assay_evidence["assay-evidence"]
        direction TB
        ev_lib["lib.rs"]
        ev_bundle["bundle"]
        ev_events["cloud_events"]
        ev_jcs["jcs"]
        ev_lib --> ev_bundle
        ev_lib --> ev_events
        ev_lib --> ev_jcs
    end

    %% Cross-crate dependencies
    assay_cli --> assay_core
    assay_cli --> assay_metrics
    assay_cli --> assay_evidence
    assay_mcp_server --> assay_core
    assay_mcp_server --> assay_policy
    assay_monitor --> assay_ebpf
    core_engine --> metrics_lib
    core_mcp --> assay_policy
EOF

echo "Generated: $OUTPUT_FILE"

# Also generate a simple text summary
SUMMARY_FILE="$OUTPUT_DIR/module-summary.txt"
cat > "$SUMMARY_FILE" << 'EOF'
# Assay Module Summary

## Crate Overview

| Crate | Purpose | Key Modules |
|-------|---------|-------------|
| assay-cli | CLI interface | commands/, args.rs, dispatch |
| assay-core | Core evaluation engine | engine/, storage/, trace/, mcp/, report/ |
| assay-metrics | Metric implementations | must_contain, semantic, regex_match, json_schema |
| assay-mcp-server | MCP proxy server | server, proxy, policy |
| assay-monitor | Runtime monitoring | events, ebpf_loader |
| assay-evidence | Evidence bundles | bundle, cloud_events, jcs |
| assay-policy | Policy compilation | compiler, tier1, tier2 |
| assay-ebpf | eBPF programs | lsm hooks |
| assay-sim | Attack simulation | scenarios |
| assay-common | Shared types | exports |

EOF

echo "Generated: $SUMMARY_FILE"
