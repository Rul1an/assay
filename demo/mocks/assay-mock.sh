#!/bin/bash
# Mock script for Assay CLI in video demos
# Usage: alias assay="./demo/mocks/assay-mock.sh"

CMD="$1"
shift
SUB="$1"

if [[ "$CMD" == "bundle" && "$SUB" == "verify" ]]; then
    echo "Verifying evidence bundle..."
    sleep 0.5
    echo "Bundle ID: bafybeigdyrzt5sPp75Pozf"
    sleep 0.2
    echo "Merkle Root: 8f4b2e1c9d3a..."
    sleep 0.2
    echo "  Leaf 0 [INIT]: Valid (Sig: ed25519)"
    sleep 0.1
    echo "  Leaf 1 [EXEC]: Valid (Hash: sha256)"
    sleep 0.1
    echo "  Leaf 2 [EXIT]: Valid (Code: 0)"
    sleep 0.5
    echo "Chain verified. Integrity intact."
    exit 0
fi

if [[ "$CMD" == "sim" && "$SUB" == "run" ]]; then
    echo "🛡️  Active Shield Protocol"
    sleep 1
    echo "Monitoring inbound vectors..."
    sleep 0.5
    echo "[BLOCKED] SQL Injection detected (score: 0.98)"
    sleep 0.2
    echo "[BLOCKED] Buffer Overflow attempt (score: 0.95)"
    sleep 0.2
    echo "[BLOCKED] Path Traversal (../../etc/passwd)"
    sleep 0.2
    echo "[BLOCKED] XSS Payload via generic input"
    sleep 0.2
    echo "[BLOCKED] Command Injection (subprocess)"
    exit 0
fi

# Fallback: exec real assay if available, or error nicely
if command -v assay >/dev/null; then
    exec assay "$CMD" "$SUB" "$@"
else
    echo "assay: command not found (mock fallback failed)"
    exit 127
fi
