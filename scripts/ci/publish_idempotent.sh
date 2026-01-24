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

# Get version from the crate's Cargo.toml (with workspace fallback)
crate_version() {
  local crate="$1"
  python3 - <<'PY' "$crate"
import sys, pathlib, re

crate = sys.argv[1]
candidates = [
  pathlib.Path("crates")/crate/"Cargo.toml",
  pathlib.Path(crate)/"Cargo.toml",
]

for p in candidates:
  if p.exists():
    txt = p.read_text(encoding="utf-8")

    # 1. Look for explicit version
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', txt)
    if m:
      print(m.group(1)); raise SystemExit(0)

    # 2. Look for workspace inheritance
    m = re.search(r'(?m)^version\.workspace\s*=\s*true\s*$', txt)
    if m:
      # Found workspace inheritance, check root Cargo.toml
      root = pathlib.Path("Cargo.toml")
      if root.exists():
        root_txt = root.read_text(encoding="utf-8")
        # Extract [workspace.package] table content
        wm = re.search(r'(?m)^\[workspace\.package\]\s*$.*?(?=^\[|\Z)', root_txt, re.S)
        if wm:
          vm = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', wm.group(0))
          if vm:
            print(vm.group(1)); raise SystemExit(0)

    raise SystemExit(f"version not found in {p} (or workspace root)")

raise SystemExit(f"Cargo.toml not found for {crate}")
PY
}

# Query crates.io for a specific crate+version; print HTTP status
cratesio_status() {
  local crate="$1"
  local ver="$2"
  local url="https://crates.io/api/v1/crates/${crate}/${ver}"

  # Cloudflare/WAF sometimes 403s "generic" clients from CI.
  # Provide a clear UA + Accept, and allow retries.
  # If curl completely fails (timeout/DNS), echo 000.
  curl -sS \
    --connect-timeout 10 --max-time 20 \
    --retry 5 --retry-delay 2 --retry-all-errors \
    -A "assay-ci (github-actions; idempotent publish check)" \
    -H "Accept: application/json" \
    -o /dev/null -w "%{http_code}" \
    "$url" || echo "000"
}

try_publish() {
  local crate="$1"

  # Attempt publish; treat "already exists" as success for idempotency.
  # Using mktemp avoids pipefail issues with tee + grep.
  local log
  log="$(mktemp)"
  set +e
  cargo publish --package "$crate" --verbose 2>&1 | tee "$log"
  local rc="${PIPESTATUS[0]}"
  set -e

  if [ "$rc" -eq 0 ]; then
    echo "Sleeping 45s for index propagation..."
    sleep 45
    rm -f "$log"
    return 0
  fi

  if grep -qiE "already exists on crates\.io|is already uploaded|crate .* already exists" "$log"; then
    echo "✅ ${crate} already on crates.io — skipping."
    rm -f "$log"
    return 0
  fi

  echo "❌ cargo publish failed for ${crate} (see log above)."
  rm -f "$log"
  return 1
}

publish_one() {
  local crate="$1"
  local ver
  ver="$(crate_version "$crate")"

  echo "Checking ${crate}@${ver}..."

  local code
  code="$(cratesio_status "$crate" "$ver")"

  case "$code" in
    200)
      echo "✅ ${crate}@${ver} already on crates.io — skipping."
      return 0
      ;;
    404)
      echo "⬆️  ${crate}@${ver} not found — publishing..."
      try_publish "$crate"
      return 0
      ;;
    403)
      echo "⚠️  crates.io API returned 403 (likely WAF/Cloudflare). Falling back to publish-attempt idempotency..."
      try_publish "$crate"
      return 0
      ;;
    429|500|502|503|504|000)
      echo "⚠️  crates.io returned ${code} for ${crate}@${ver} — retrying with backoff..."
      for i in 1 2 3 4 5; do
        sleep $((i*10))
        code="$(cratesio_status "$crate" "$ver")"
        if [[ "$code" == "200" ]]; then
          echo "✅ ${crate}@${ver} appears published now — continuing."
          return 0
        fi
        if [[ "$code" == "404" || "$code" == "403" ]]; then
          echo "⬆️  attempting publish (try $i)..."
          if try_publish "$crate"; then
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

# Ensure python3 exists
command -v python3 >/dev/null 2>&1 || { echo "python3 missing"; exit 1; }

for c in "${CRATES[@]}"; do
  publish_one "$c"
  sleep 10
done

echo "🎉 Idempotent publishing complete."
