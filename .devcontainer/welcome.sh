#!/bin/bash
set -e

# Verify assay is available
if ! command -v assay &> /dev/null; then
    echo "⚠️  Assay binary not found. Building from source..."
    cargo install --path crates/assay-cli --locked 2>/dev/null || cargo install assay-cli --locked
fi

VERSION=$(assay --version 2>/dev/null || echo "unknown")

cat << 'EOF'

  ╔══════════════════════════════════════════════════════════════╗
  ║                                                              ║
  ║   ┌─┐┌─┐┌─┐┌─┐┬ ┬                                          ║
  ║   ├─┤└─┐└─┐├─┤└┬┘   Policy-as-Code for AI Agents           ║
  ║   ┴ ┴└─┘└─┘┴ ┴ ┴                                           ║
  ║                                                              ║
  ╠══════════════════════════════════════════════════════════════╣
  ║                                                              ║
  ║   Quick start:                                               ║
  ║                                                              ║
  ║     make demo        Run the full break & fix demo           ║
  ║     make test        Test a safe trace against policy        ║
  ║     make fail        Test an unsafe trace (expect failure)   ║
  ║     make explore     Open the TUI evidence explorer          ║
  ║                                                              ║
  ║   Or run directly:                                           ║
  ║                                                              ║
  ║     assay run --config eval.yaml \                           ║
  ║       --trace-file traces/safe.jsonl                         ║
  ║                                                              ║
  ║   Docs:  https://assay.dev/docs                              ║
  ║   Repo:  https://github.com/Rul1an/assay                     ║
  ║                                                              ║
  ╚══════════════════════════════════════════════════════════════╝

EOF

echo "  Assay $VERSION ready."
echo ""
