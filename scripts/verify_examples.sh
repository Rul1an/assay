#!/bin/bash
set -e

echo "Verifying examples..."

# function-calling-agent
echo "Checking examples/agent-function-calling..."
cd examples/agent-function-calling
cargo check
cd ../..

# web-search-agent
echo "Checking examples/web-search-agent..."
cd examples/web-search-agent
cargo check
cd ../..

echo "All examples verified!"
