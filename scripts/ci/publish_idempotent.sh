#!/usr/bin/env bash
set -euo pipefail

echo "📦 Starting Idempotent Publisher..."

# Crates published in dependency order
CRATES=(
  "assay-common"
  "assay-core"
  "assay-metrics"
  "assay-policy"
  "assay-mcp-server"
  "assay-monitor"
  "assay-cli"
)

# Get version from the crate's Cargo.toml (no jq)
crate_version() {
  local crate="$1"
  python3 - <<'PY' "$crate"
import sys, pathlib, re
crate = sys.argv[1]
# workspace layout: crates/<name>/Cargo.toml OR <name>/Cargo.toml (fallback)
# Note: assay-python-sdk is not in this list, but generalized logic handles crates/ vs root/
candidates = [
  pathlib.Path("crates")/crate/"Cargo.toml",
  pathlib.Path(crate)/"Cargo.toml",
]
for p in candidates:
  if p.exists():
    txt = p.read_text(encoding="utf-8")
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', txt)
    if not m:
      raise SystemExit(f"version not found in {p}")
    print(m.group(1))
    raise SystemExit(0)
raise SystemExit(f"Cargo.toml not found for {crate}")
PY
}

# Query crates.io for a specific crate+version; print HTTP status
cratesio_status() {
  local crate="$1"
  local ver="$2"
  local url="https://crates.io/api/v1/crates/${crate}/${ver}"

  # Don't use -f; we WANT 404 as data, not a fatal error.
  # Capture status code; body discarded.
  curl -sS -o /dev/null -w "%{http_code}" "$url"
}

publish_one() {
  local crate="$1"
  local ver
  ver="$(crate_version "$crate")"

  echo "Checking ${crate}@${ver}..."

  local code
  code="$(cratesio_status "$crate" "$ver" || true)"

  case "$code" in
    200)
      echo "✅ ${crate}@${ver} already on crates.io — skipping."
      return 0
      ;;
    404)
      echo "⬆️  ${crate}@${ver} not found — publishing..."
      cargo publish --package "$crate" --verbose
      echo "Sleeping 45s for index propagation..."
      sleep 45
      return 0
      ;;
    429|500|502|503|504)
      echo "⚠️  crates.io returned ${code} for ${crate}@${ver} — retrying with backoff..."
      # simple backoff retries
      for i in 1 2 3 4 5; do
        sleep $((i*10))
        code="$(cratesio_status "$crate" "$ver" || true)"
        if [[ "$code" == "200" ]]; then
          echo "✅ ${crate}@${ver} appears published now — continuing."
          return 0
        fi
        if [[ "$code" == "404" ]]; then
          echo "⬆️  still not found — attempting publish (try $i)..."
          if cargo publish --package "$crate" --verbose; then
            echo "Sleeping 45s for index propagation..."
            sleep 45
            return 0
          fi
        fi
      done
      echo "❌ Failed to publish ${crate}@${ver} after retries."
      return 1
      ;;
    *)
      echo "❌ Unexpected HTTP status '${code}' for ${crate}@${ver}"
      return 1
      ;;
  esac
}

# Ensure python3 exists (ubuntu-latest should have it, but keep it deterministic)
command -v python3 >/dev/null 2>&1 || { echo "python3 missing"; exit 1; }

for c in "${CRATES[@]}"; do
  publish_one "$c"
  # Optional: small delay to reduce indexing race pain
  sleep 10
done

echo "🎉 Idempotent publishing complete."
