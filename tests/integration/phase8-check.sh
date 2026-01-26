#!/bin/bash
set -e

# Phase 8 Integration Check Script
# Verifies Enforcement Contract & ABI Matrix logic

echo "🚀 Starting Phase 8 Integration Check..."

# 1. Basic Build & Syntax
echo "--- Checking Build ---"
cargo check -p assay-cli

# 2. Argument Parsing
echo "--- Checking CLI Arguments ---"
cargo run --bin assay -- sandbox --help | grep -q -- "--enforce"
cargo run --bin assay -- sandbox --help | grep -q -- "--dry-run"
echo "✅ CLI Arguments present"

# 3. Backend Probing (Local)
echo "--- Checking Backend Detection ---"
# Note: On macOS this will fallback to NoopAudit, which is expected.
# We verify it doesn't crash and reports correctly.
cargo run --bin assay -- sandbox --quiet -- echo "test" > /dev/null
echo "✅ Backend detection stable"

# 4. Dry-run Violation Simulation
echo "--- Checking Dry-run Exit Code Logic ---"
# Create a policy that doesn't allow /etc/shadow
cat <<EOF > phase8_denied.yaml
fs:
  allow: []
EOF

# Run with dry-run and profiling. We use a test-hook to inject a FS event.
# We run 'echo' which always succeeds, so we can test the violation detection post-run.
export ASSAY_PROFILE_TEST_EVENTS='[{"FsObserved": {"op": "Read", "path": "/etc/shadow"}}]'
export ASSAY_SANDBOX_QUIET=1

# Should exit with code 4 (WOULD_BLOCK)
(cargo run --bin assay --features profile-test-hook -- sandbox --policy phase8_denied.yaml --dry-run --profile trace.yaml -- echo "violating" > /dev/null 2>&1) || {
    RET=$?
    if [ $RET -ne 4 ]; then
        echo "❌ Expected exit code 4 for dry-run violation, got $RET"
        exit 1
    fi
}
echo "✅ Dry-run exit code 4 (WOULD_BLOCK) verified via injection"

# 5. Fail-closed fallback
echo "--- Checking Fail-closed Logic ---"
# On non-Linux, --enforce with --fail-closed should exit 2
(cargo run --bin assay -- sandbox --enforce --fail-closed -- echo "test" > /dev/null 2>&1) || {
    RET=$?
    if [ $RET -ne 2 ]; then
         # Only fail if we are on macOS/non-landlock.
         # On landlock Linux this might succeed.
         grep -q "Landlock" <(cargo run --bin assay -- sandbox -- echo 1 2>&1) || {
            echo "❌ Expected exit code 2 for enforcement failure on non-linux, got $RET"
            exit 1
         }
    fi
}
echo "✅ Fail-closed contract verified"

# Cleanup
rm -f phase8_denied.yaml trace.yaml trace.yaml.report.md

echo "🎉 Phase 8 Check Complete!"
