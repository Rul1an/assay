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
# Returns 000 on network failure.
cratesio_status() {
  local crate="$1"
  local ver="$2"
  local url="https://crates.io/api/v1/crates/${crate}/${ver}"

  # Time out quickly; return code even if body is empty.
  # If curl itself fails, echo 000 (standard convention).
  curl -sS --connect-timeout 10 --max-time 20 -o /dev/null -w "%{http_code}" "$url" || echo "000"
}

# Publish wrapper that handles "already exists" race condition
cargo_publish_race_safe() {
  local crate="$1"
  local out

  set +e
  out="$(cargo publish --package "$crate" --verbose 2>&1)"
  local rc=$?
  set -e

  if [ $rc -eq 0 ]; then
    echo "$out"
    return 0
  fi

  if echo "$out" | grep -q "already exists"; then
    echo "✅ ${crate} already published (race condition ignored) — continuing."
    return 0
  fi

  echo "$out"
  return $rc
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
      if cargo_publish_race_safe "$crate"; then
        echo "Sleeping 45s for index propagation..."
        sleep 45
        return 0
      else
        return 1
      fi
      ;;
    429|500|502|503|504|000)
      echo "⚠️  crates.io returned ${code} for ${crate}@${ver} — retrying with backoff..."
      # simple backoff retries
      for i in 1 2 3 4 5; do
        sleep $((i*10))
        code="$(cratesio_status "$crate" "$ver")"
        if [[ "$code" == "200" ]]; then
          echo "✅ ${crate}@${ver} appears published now — continuing."
          return 0
        fi
        if [[ "$code" == "404" ]]; then
          echo "⬆️  still not found — attempting publish (try $i)..."
          if cargo_publish_race_safe "$crate"; then
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

# Ensure python3 exists
command -v python3 >/dev/null 2>&1 || { echo "python3 missing"; exit 1; }

for c in "${CRATES[@]}"; do
  publish_one "$c"
  sleep 10
done

echo "🎉 Idempotent publishing complete."
