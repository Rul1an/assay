#!/bin/bash
# E2E Test Script for assay-registry
# SPEC: SPEC-Pack-Registry-v1
#
# This script runs end-to-end tests using wiremock to simulate registry responses.
#
# Prerequisites:
# - wiremock installed (brew install wiremock or similar)
# - cargo build -p assay-registry
#
# Usage:
#   ./e2e.sh [--verbose]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REVIEW_KIT_DIR="$(dirname "$SCRIPT_DIR")"
WIREMOCK_PORT="${WIREMOCK_PORT:-8089}"
VERBOSE="${1:-}"

log() {
    echo "[E2E] $*"
}

log_verbose() {
    if [[ "$VERBOSE" == "--verbose" ]]; then
        echo "[E2E] $*"
    fi
}

cleanup() {
    if [[ -n "${WIREMOCK_PID:-}" ]]; then
        log "Stopping wiremock (PID: $WIREMOCK_PID)"
        kill "$WIREMOCK_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# Check prerequisites
if ! command -v wiremock &>/dev/null; then
    log "ERROR: wiremock not found. Install with: brew install wiremock"
    exit 1
fi

log "Starting E2E tests..."

# Start wiremock
log "Starting wiremock on port $WIREMOCK_PORT..."
wiremock --port "$WIREMOCK_PORT" --root-dir "$REVIEW_KIT_DIR/wiremock-stubs" &
WIREMOCK_PID=$!
sleep 2

# Verify wiremock is running
if ! curl -s "http://localhost:$WIREMOCK_PORT/__admin" >/dev/null; then
    log "ERROR: wiremock failed to start"
    exit 1
fi
log "Wiremock running on http://localhost:$WIREMOCK_PORT"

# Run tests
log ""
log "=== Test 1: Successful pack fetch (200) ==="
RESPONSE=$(curl -s -w "\n%{http_code}" "http://localhost:$WIREMOCK_PORT/packs/test-pack/1.0.0")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [[ "$HTTP_CODE" == "200" ]]; then
    log "PASS: Got 200 OK"
    log_verbose "Body: $BODY"
else
    log "FAIL: Expected 200, got $HTTP_CODE"
    exit 1
fi

log ""
log "=== Test 2: Cache hit (304 Not Modified) ==="
RESPONSE=$(curl -s -w "\n%{http_code}" -H 'If-None-Match: "sha256:abc123"' "http://localhost:$WIREMOCK_PORT/packs/test-pack/1.0.0")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

if [[ "$HTTP_CODE" == "304" ]]; then
    log "PASS: Got 304 Not Modified"
else
    log "FAIL: Expected 304, got $HTTP_CODE"
    exit 1
fi

log ""
log "=== Test 3: Revoked pack (410 Gone) ==="
RESPONSE=$(curl -s -w "\n%{http_code}" "http://localhost:$WIREMOCK_PORT/packs/revoked-pack/1.0.0")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [[ "$HTTP_CODE" == "410" ]]; then
    log "PASS: Got 410 Gone"
    if echo "$BODY" | grep -q "safe_version"; then
        log "PASS: Response contains safe_version"
    else
        log "FAIL: Response missing safe_version"
        exit 1
    fi
else
    log "FAIL: Expected 410, got $HTTP_CODE"
    exit 1
fi

log ""
log "=== Test 4: Rate limited (429) ==="
RESPONSE=$(curl -s -w "\n%{http_code}" "http://localhost:$WIREMOCK_PORT/packs/rate-limited/1.0.0")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

if [[ "$HTTP_CODE" == "429" ]]; then
    log "PASS: Got 429 Too Many Requests"
else
    log "FAIL: Expected 429, got $HTTP_CODE"
    exit 1
fi

log ""
log "=== Test 5: Keys manifest ==="
RESPONSE=$(curl -s -w "\n%{http_code}" "http://localhost:$WIREMOCK_PORT/keys")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [[ "$HTTP_CODE" == "200" ]]; then
    log "PASS: Got 200 OK"
    if echo "$BODY" | grep -q "Ed25519"; then
        log "PASS: Keys manifest contains Ed25519 key"
    else
        log "FAIL: Keys manifest missing Ed25519 key"
        exit 1
    fi
else
    log "FAIL: Expected 200, got $HTTP_CODE"
    exit 1
fi

log ""
log "=== All E2E tests passed ==="
