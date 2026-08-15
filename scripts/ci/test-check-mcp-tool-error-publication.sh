#!/usr/bin/env bash
# Self-test for check-mcp-tool-error-publication.py.
#
# Creates mutant copies of tools/mod.rs in a temp directory and verifies
# the guard rejects each. The production file is NEVER written — only read
# for the baseline check. A sha256 digest comparison before and after proves
# it was not modified.
set -euo pipefail

GUARD="scripts/ci/check-mcp-tool-error-publication.py"
TARGET="crates/assay-mcp-server/src/tools/mod.rs"

# Record digest before anything runs.
DIGEST_BEFORE=$(shasum -a 256 "$TARGET" | awk '{print $1}')

# The production guard must not accept caller-selected filesystem paths.
if python3 "$GUARD" "$TARGET" 2>/dev/null; then
    echo "FAIL: guard must reject positional filesystem paths" >&2
    exit 1
fi
echo "PASS: guard rejects positional filesystem paths"

# An unterminated result signature must not trigger polynomial regex backtracking.
python3 - "$GUARD" <<'PY'
import subprocess
import sys

source = "impl ToolError { pub fn result(self)->" + (" " * 200_000) + "x"
try:
    result = subprocess.run(
        [sys.executable, sys.argv[1], "--stdin"],
        input=source,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=2,
        check=False,
    )
except subprocess.TimeoutExpired:
    raise SystemExit("FAIL: guard regex exceeded adversarial-input deadline")
if result.returncode != 1:
    raise SystemExit(
        f"FAIL: malformed adversarial source returned {result.returncode}, expected 1"
    )
PY
echo "PASS: guard rejects adversarial source within deadline"

# The mutation-only stdin interface has its own fixed resource ceiling.
python3 - "$GUARD" <<'PY'
import subprocess
import sys

result = subprocess.run(
    [sys.executable, sys.argv[1], "--stdin"],
    input="x" * 1_000_001,
    text=True,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    check=False,
)
if result.returncode != 2:
    raise SystemExit(
        f"FAIL: oversized guard source returned {result.returncode}, expected 2"
    )
PY
echo "PASS: guard bounds stdin source"

# Verify the guard passes on the real file first.
if ! python3 "$GUARD"; then
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

if python3 "$GUARD" --stdin < "$TMPDIR/mut1.rs" 2>/dev/null; then
    echo "FAIL: guard must reject unbounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects unbounded repack"

# ── Mutation 2: bounded repack ────────────────────────────────────────────
# Replace `"error": self` with a bounded repack that calls bound_public_message
# but constructs a new JSON object, bypassing ToolError::Serialize.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": bound_public_message(\&self.message), "details": self.details})/' \
    "$TARGET" > "$TMPDIR/mut2.rs"

if python3 "$GUARD" --stdin < "$TMPDIR/mut2.rs" 2>/dev/null; then
    echo "FAIL: guard must reject bounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects bounded repack"

# ── Mutation 3: commented decoy ───────────────────────────────────────────
# Keep a comment containing `"error": self` but replace the real expression
# with a field repack. A naive substring check would see the comment and pass.
sed 's|"error": self|// "error": self  -- decoy comment\n             "error": serde_json::json!({"code": self.code, "message": self.message})|' \
    "$TARGET" > "$TMPDIR/mut3.rs"

if python3 "$GUARD" --stdin < "$TMPDIR/mut3.rs" 2>/dev/null; then
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

if python3 "$GUARD" --stdin < "$TMPDIR/mut4.rs" 2>/dev/null; then
    echo "FAIL: guard must reject dead/unrelated decoy mutation" >&2
    exit 1
fi
echo "PASS: guard rejects dead/unrelated decoy"

# ── Mutation 5: unreachable direct publication plus helper repack ─────────
# A presence-only guard can be satisfied by dead code while the reachable
# branch delegates to a repacking helper.
python3 - "$TARGET" "$TMPDIR/mut5.rs" <<'PY'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text()
old = '''    pub fn result(self) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(serde_json::json!({
             "allowed": false,
             "error": self
        }))?)
    }
'''
new = '''    pub fn result(self) -> anyhow::Result<Value> {
        if std::hint::black_box(false) {
            return Ok(serde_json::to_value(serde_json::json!({
                "allowed": false,
                "error": self
            }))?);
        }
        publish_repacked_error(self)
    }
'''
if old not in source:
    raise SystemExit("fixture drift: ToolError::result body not found")
source = source.replace(old, new, 1)
source += '''
fn publish_repacked_error(error: ToolError) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "allowed": false,
        "error": {"code": error.code, "message": error.message}
    }))
}
'''
Path(sys.argv[2]).write_text(source)
PY

if python3 "$GUARD" --stdin < "$TMPDIR/mut5.rs" 2>/dev/null; then
    echo "FAIL: guard must reject unreachable-direct/helper-repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects unreachable direct publication plus helper repack"

# ── Verify production file was not modified ───────────────────────────────
DIGEST_AFTER=$(shasum -a 256 "$TARGET" | awk '{print $1}')
if [ "$DIGEST_BEFORE" != "$DIGEST_AFTER" ]; then
    echo "FAIL: production file was modified (before=$DIGEST_BEFORE after=$DIGEST_AFTER)" >&2
    exit 1
fi
echo "PASS: production file unchanged (sha256=$DIGEST_BEFORE)"

echo "All self-test mutations caught."
