#!/usr/bin/env bash
# The host supplies the client pipe; only the relay's stdout can satisfy the frame check.
set -euo pipefail
test_name=mcp::proxy::server::tests::relay_returns_while_a_host_holds_the_process_stdout_lock
log=$(mktemp)
trap 'rm -f "$log"' EXIT
cargo test --locked -p assay-core --lib "$test_name" -- --exact --ignored --nocapture --test-threads=2 |
  tee "$log"
grep -Fq 'test result: ok. 1 passed; 0 failed; 0 ignored;' "$log" || {
  echo 'The relay test did not run exactly once.' >&2
  exit 1
}
awk -v frame='{"jsonrpc":"2.0","id":1,"result":{}}' '
  { sub(/\r$/, ""); if ($0 == frame) copies++ }
  END { exit copies != 1 }
' "$log" || {
  echo 'Expected exactly one complete response frame on client stdout.' >&2
  exit 1
}
