#!/usr/bin/env bash
# Self-test for check-mcp-tool-error-publication.py.
#
# Creates four mutant copies of tools/mod.rs in a temp directory and verifies
# the guard rejects each. The production file is NEVER written — only read
# for the baseline check. A sha256 digest comparison before and after proves
# it was not modified.
set -euo pipefail

GUARD="scripts/ci/check-mcp-tool-error-publication.py"
TARGET="crates/assay-mcp-server/src/tools/mod.rs"

# Record digest before anything runs.
DIGEST_BEFORE=$(shasum -a 256 "$TARGET" | awk '{print $1}')

# Verify the guard passes on the real file first.
if ! python3 "$GUARD" "$TARGET"; then
    echo "FAIL: guard must pass on the unmodified source" >&2
    exit 1
fi
echo "PASS: guard accepts unmodified source"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# ── Mutation 1: unbounded repack ──────────────────────────────────────────
# Replace `"error": self` with a field repack that bypasses Serialize.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": self.message, "details": self.details})/' \
    "$TARGET" > "$TMPDIR/mut1.rs"

if python3 "$GUARD" "$TMPDIR/mut1.rs" 2>/dev/null; then
    echo "FAIL: guard must reject unbounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects unbounded repack"

# ── Mutation 2: bounded repack ────────────────────────────────────────────
# Replace `"error": self` with a bounded repack that calls bound_public_message
# but constructs a new JSON object, bypassing ToolError::Serialize.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": bound_public_message(\&self.message), "details": self.details})/' \
    "$TARGET" > "$TMPDIR/mut2.rs"

if python3 "$GUARD" "$TMPDIR/mut2.rs" 2>/dev/null; then
    echo "FAIL: guard must reject bounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects bounded repack"

# ── Mutation 3: commented decoy ───────────────────────────────────────────
# Keep a comment containing `"error": self` but replace the real expression
# with a field repack. A naive substring check would see the comment and pass.
sed 's|"error": self|// "error": self  -- decoy comment\n             "error": serde_json::json!({"code": self.code, "message": self.message})|' \
    "$TARGET" > "$TMPDIR/mut3.rs"

if python3 "$GUARD" "$TMPDIR/mut3.rs" 2>/dev/null; then
    echo "FAIL: guard must reject commented decoy mutation" >&2
    exit 1
fi
echo "PASS: guard rejects commented decoy"

# ── Mutation 4: dead/unrelated decoy ──────────────────────────────────────
# Add `"error": self` in a new unrelated function OUTSIDE impl ToolError,
# while replacing the real one inside result() with a field repack.
# A naive whole-file search would see the decoy and pass.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": self.message})/' \
    "$TARGET" > "$TMPDIR/mut4.rs"
cat >> "$TMPDIR/mut4.rs" << 'DECOY'
// Unrelated function — not inside impl ToolError
fn _decoy_for_guard_test() {
    let _ = serde_json::json!({"error": self});
}
DECOY

if python3 "$GUARD" "$TMPDIR/mut4.rs" 2>/dev/null; then
    echo "FAIL: guard must reject dead/unrelated decoy mutation" >&2
    exit 1
fi
echo "PASS: guard rejects dead/unrelated decoy"

# ── Verify production file was not modified ───────────────────────────────
DIGEST_AFTER=$(shasum -a 256 "$TARGET" | awk '{print $1}')
if [ "$DIGEST_BEFORE" != "$DIGEST_AFTER" ]; then
    echo "FAIL: production file was modified (before=$DIGEST_BEFORE after=$DIGEST_AFTER)" >&2
    exit 1
fi
echo "PASS: production file unchanged (sha256=$DIGEST_BEFORE)"

echo "All self-test mutations caught."
