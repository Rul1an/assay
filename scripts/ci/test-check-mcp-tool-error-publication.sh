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

# The production guard has no caller-controlled source interface.
if python3 "$GUARD" --stdin </dev/null 2>/dev/null; then
    echo "FAIL: guard must reject stdin source selection" >&2
    exit 1
fi
echo "PASS: guard rejects stdin source selection"

# Verify the guard passes on the real file first.
if ! python3 "$GUARD"; then
    echo "FAIL: guard must pass on the unmodified source" >&2
    exit 1
fi
echo "PASS: guard accepts unmodified source"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

check_source() {
    python3 - "$GUARD" "$1" <<'PY'
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("publication_guard", sys.argv[1])
if spec is None or spec.loader is None:
    raise SystemExit("failed to load publication guard")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)
source = Path(sys.argv[2]).read_text()
raise SystemExit(0 if guard.check(source) else 1)
PY
}

# Repeated near-signatures must be handled in linear time.
python3 - "$TMPDIR/adversarial.rs" <<'PY'
import sys
from pathlib import Path

prefix = "pub fn result(self)->"
Path(sys.argv[1]).write_text("impl ToolError {" + prefix * 30_000 + "x")
PY
python3 - "$GUARD" "$TMPDIR/adversarial.rs" <<'PY'
import importlib.util
import signal
import sys
from pathlib import Path

def deadline(_signum, _frame):
    raise SystemExit("FAIL: guard exceeded adversarial-input deadline")

spec = importlib.util.spec_from_file_location("publication_guard", sys.argv[1])
if spec is None or spec.loader is None:
    raise SystemExit("failed to load publication guard")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)
signal.signal(signal.SIGALRM, deadline)
signal.alarm(2)
accepted = guard.check(Path(sys.argv[2]).read_text())
signal.alarm(0)
if accepted:
    raise SystemExit("FAIL: guard accepted repeated near-signatures")
PY
echo "PASS: guard rejects repeated near-signatures within deadline"

# ── Mutation 1: unbounded repack ──────────────────────────────────────────
# Replace `"error": self` with a field repack that bypasses Serialize.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": self.message, "details": self.details})/' \
    "$TARGET" > "$TMPDIR/mut1.rs"

if check_source "$TMPDIR/mut1.rs" 2>/dev/null; then
    echo "FAIL: guard must reject unbounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects unbounded repack"

# ── Mutation 2: bounded repack ────────────────────────────────────────────
# Replace `"error": self` with a bounded repack that calls bound_public_message
# but constructs a new JSON object, bypassing ToolError::Serialize.
sed 's/"error": self/"error": serde_json::json!({"code": self.code, "message": bound_public_message(\&self.message), "details": self.details})/' \
    "$TARGET" > "$TMPDIR/mut2.rs"

if check_source "$TMPDIR/mut2.rs" 2>/dev/null; then
    echo "FAIL: guard must reject bounded repack mutation" >&2
    exit 1
fi
echo "PASS: guard rejects bounded repack"

# ── Mutation 3: commented decoy ───────────────────────────────────────────
# Keep a comment containing `"error": self` but replace the real expression
# with a live field repack on the following line. Python makes the newline
# portable across GNU and BSD sed environments.
python3 - "$TARGET" "$TMPDIR/mut3.rs" <<'PY'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text()
old = '"error": self'
new = (
    '// "error": self  -- decoy comment\n'
    '             "error": serde_json::json!({"code": self.code, "message": self.message})'
)
if old not in source:
    raise SystemExit("fixture drift: direct publication expression not found")
Path(sys.argv[2]).write_text(source.replace(old, new, 1))
PY

if check_source "$TMPDIR/mut3.rs" 2>/dev/null; then
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

if check_source "$TMPDIR/mut4.rs" 2>/dev/null; then
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

if check_source "$TMPDIR/mut5.rs" 2>/dev/null; then
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
