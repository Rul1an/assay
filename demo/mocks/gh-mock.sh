#!/bin/bash
# Mock script for GitHub CLI in video demos
# Usage: alias gh="./demo/mocks/gh-mock.sh"

CMD="$1"
SUB="$2"

if [[ "$CMD" == "run" && "$SUB" == "watch" ]]; then
    # Simulate CI run output
    echo "✓ Set up job"
    sleep 0.2
    echo "✓ Run actions/checkout@v4"
    sleep 0.2
    echo "✓ Install Rust"
    sleep 0.5
    echo "○ Run cargo test"
    sleep 0.5
    echo "Running tests..."
    sleep 1
    echo "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"
    sleep 0.5
    echo "Error: Process completed with exit code 101."
    sleep 0.5
    echo "❌ Run failed with exit code 1"
    exit 1
fi

# Fallback
if command -v gh >/dev/null; then
    exec gh "$@"
else
    echo "gh: command not found (mock fallback failed)"
    exit 127
fi
