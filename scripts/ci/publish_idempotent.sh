#!/usr/bin/env bash
set -euo pipefail

echo "📦 Starting Idempotent Publisher..."

# Crates published in dependency order.
#
# The protocol adapters (a2a, acp, ucp) are intentionally source/workspace-internal
# for this release line, per ADR-026's distribution freeze. They have no published
# dependent, so nothing on crates.io needs them to exist.
#
# assay-adapter-api is not in that group and publishes from v4.0.0. It was grouped
# with them and marked `publish = false` while `assay-core` and `assay-sim` -- both
# published -- carried a hard dependency on it. That only ever resolved because a
# historical 3.x predates the flag, so published assay-core has been resolving an
# abandoned 3.2.3 rather than the source it was built against.
#
# As of v3.11.2 the Assay-Runner crates publish too. They are marked as
# internal/experimental substrate in their package descriptions; they exist on
# crates.io so that `assay-cli` (which depends on them when the `runner`
# feature is enabled, the default) can resolve them at publish time. Their
# semver tracks the Assay workspace and they are not guaranteed standalone
# product crates.
CRATES=(
  "assay-common"
  "assay-registry"
  "assay-canonical"
  # Before assay-evidence, not after it. `assay-evidence` dev-depends on this crate for
  # `tests/claim_gate_parity.rs` (#2084), and `cargo publish` resolves dev-dependencies from the
  # registry like any other. With this crate at position 10, v4.0.0 failed here with "failed to
  # select a version for the requirement `assay-runner-schema = ^4.0.0`" -- and because this script
  # is `set -e`, the nine crates after it never published at all.
  "assay-runner-schema"
  "assay-evidence"
  # After assay-evidence (its only internal dependency) and before assay-core, which requires it.
  # Published from v4.0.0: see the note in its manifest.
  "assay-adapter-api"
  "assay-core"
  "assay-metrics"
  "assay-policy"
  "assay-mcp-server"
  "assay-monitor"
  "assay-runner-linux"
  "assay-runner-core"
  "assay-sim"
  "assay-cli"
)

# The list above is hand-kept, so it is checked against the manifests before anything is published.
#
# One rule, three clauses, and each clause exists because a v4.0.0 publish failed on it:
#
#   1. every crate in this list is publishable            (assay-adapter-api carried publish = false)
#   2. internal dependencies publish before their users   (assay-runner-schema was six places late)
#   3. internal dependencies this script does NOT publish
#      resolve to a version crates.io actually has        (assay-adapter-api ^4.0.0 did not exist)
#
# They are one function rather than three checks because they answer one question -- will this
# sequence of publishes resolve -- and because a rule split across three places drifts in three
# directions.
#
# A publish order is a topological sort someone wrote by hand, and v4.0.0 is what happens when the
# graph moves and the list does not: a dev-dependency added in #2084 put `assay-runner-schema` ahead
# of `assay-evidence` in the graph while it stayed six places behind it here. CLAUDE.md documents
# that edge. Nothing compared the two.
#
# Dev-dependencies count, and that is the point: the edge that broke this was dev-only, and
# `cargo publish` does not care -- a versioned dev-dependency must resolve from the registry.
#
# Checked before the first upload, because publishing is not reversible. An order discovered to be
# wrong at crate three has already put two crates on crates.io that cannot be taken back.
validate_publish_order() {
  python3 - "${CRATES[@]}" <<'ORDER_CHECK'
import json, pathlib, re, sys

import urllib.error, urllib.request

order = sys.argv[1:]
position = {name: i for i, name in enumerate(order)}
problems = []
unpublishable = []


def requirement_for(manifest_text, dep):
    """The version requirement a manifest states for `dep`, or None if it states none."""
    for line in manifest_text.splitlines():
        if line.startswith(f"{dep} ") or line.startswith(f"{dep}="):
            m = re.search(r'version\s*=\s*"([^"]+)"', line)
            return m.group(1) if m else None
    return None


def published_versions(crate):
    """Versions of `crate` on crates.io, via the sparse index. Raises if it cannot be reached."""
    prefix = {1: "1", 2: "2", 3: f"3/{crate[0]}"}.get(len(crate), f"{crate[:2]}/{crate[2:4]}")
    url = f"https://index.crates.io/{prefix}/{crate}"
    req = urllib.request.Request(url, headers={"User-Agent": "assay-publish-order-check"})
    with urllib.request.urlopen(req, timeout=30) as fh:
        body = fh.read().decode("utf-8")
    return [json.loads(line)["vers"] for line in body.splitlines() if line.strip()]

for name in order:
    manifest = pathlib.Path("crates") / name / "Cargo.toml"
    if not manifest.is_file():
        problems.append(f"{name}: no crates/{name}/Cargo.toml")
        continue
    text = manifest.read_text(encoding="utf-8")

    # Clause 1: a crate this script publishes must be publishable. `assay-adapter-api` carried
    # `publish = false` while two published crates hard-depended on it, which is how a stale 3.2.3
    # ended up satisfying every `assay-core` on crates.io.
    if re.search(r"^publish\s*=\s*false\s*$", text, re.M):
        problems.append(
            f"{name} is in the publish list and its manifest says `publish = false`; "
            "cargo will refuse it"
        )

    # Clause 2 and 3: every internal dependency this manifest names, in any dependency table. A
    # mention in a comment cannot match: the name has to start a key.
    for dep in sorted(set(re.findall(r"^(assay-[a-z0-9-]+)\s*=", text, re.M))):
        if dep == name:
            continue
        if dep not in position:
            # An internal dependency this script never publishes. Skipping it is what let v4.0.0
            # get this far: `assay-core` required `assay-adapter-api = ^4.0.0`, a crate held at
            # 3.2.3 on crates.io because it is workspace-internal for this release line, and the
            # release-prep bump moved the pin with everything else. `cargo publish` refused.
            #
            # So the requirement is checked against what is actually published. Unreachable index
            # is a hard failure: "could not check" and "fine" must not be spelled the same way.
            unpublishable.append((name, dep, requirement_for(text, dep)))
            continue
        if position[dep] > position[name]:
            problems.append(
                f"{name} (position {position[name]}) depends on {dep} "
                f"(position {position[dep]}), which publishes later and will not resolve"
            )

for name, dep, req in unpublishable:
    if req is None:
        continue
    try:
        available = published_versions(dep)
    except (urllib.error.URLError, OSError, TimeoutError) as exc:
        problems.append(
            f"{name} depends on {dep} {req}, which this script does not publish, and the crates.io "
            f"index could not be reached to check it ({exc}). This is a could-not-check, not a pass."
        )
        continue
    major = req.split(".")[0].lstrip("^~=")
    if not any(v.split(".")[0] == major for v in available):
        newest = available[-1] if available else "none"
        problems.append(
            f"{name} requires {dep} = \"{req}\", which this script does not publish and crates.io "
            f"does not have (newest published: {newest}). Pin it to a published version."
        )

if problems:
    print("publish order does not respect the dependency graph:", file=sys.stderr)
    for p in problems:
        print(f"  - {p}", file=sys.stderr)
    sys.exit(1)
print(f"publish order respects the dependency graph ({len(order)} crates).")
ORDER_CHECK
}

validate_publish_order

CRATESIO_PUBLISH_WAIT_ATTEMPTS="${CRATESIO_PUBLISH_WAIT_ATTEMPTS:-36}"
CRATESIO_PUBLISH_WAIT_DELAY_SECONDS="${CRATESIO_PUBLISH_WAIT_DELAY_SECONDS:-10}"

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

wait_for_cratesio_version() {
  local crate="$1"
  local ver="$2"
  local code
  local i

  echo "Waiting for ${crate}@${ver} to appear in the crates.io API..."
  for ((i = 1; i <= CRATESIO_PUBLISH_WAIT_ATTEMPTS; i++)); do
    code="$(cratesio_status "$crate" "$ver")"
    case "$code" in
      200)
        echo "✅ ${crate}@${ver} is visible in crates.io."
        return 0
        ;;
      403)
        echo "⚠️  crates.io API returned 403 while confirming ${crate}@${ver}; cargo publish succeeded, so continuing."
        return 0
        ;;
      404|429|500|502|503|504|000)
        echo "⏳ ${crate}@${ver} not visible yet (status ${code}, attempt ${i}/${CRATESIO_PUBLISH_WAIT_ATTEMPTS})."
        sleep "${CRATESIO_PUBLISH_WAIT_DELAY_SECONDS}"
        ;;
      *)
        echo "❌ Unexpected HTTP status '${code}' while waiting for ${crate}@${ver}."
        return 1
        ;;
    esac
  done

  echo "❌ ${crate}@${ver} did not become visible after ${CRATESIO_PUBLISH_WAIT_ATTEMPTS} attempts."
  return 1
}

try_publish() {
  local crate="$1"
  local ver="$2"

  # Attempt publish; treat "already exists" as success for idempotency.
  # Using mktemp avoids pipefail issues with tee + grep.
  local log
  log="$(mktemp)"
  set +e
  cargo publish --package "$crate" --verbose 2>&1 | tee "$log"
  local rc="${PIPESTATUS[0]}"
  set -e

  if [ "$rc" -eq 0 ]; then
    if ! wait_for_cratesio_version "$crate" "$ver"; then
      rm -f "$log"
      return 1
    fi
    rm -f "$log"
    return 0
  fi

  if grep -qiE "already exists on crates\.io|is already uploaded|crate .* already exists" "$log"; then
    echo "✅ ${crate} already on crates.io — skipping."
    rm -f "$log"
    return 0
  fi

  # Public crate publishing is a release-truth contract. A missing Trusted
  # Publishing grant must fail the release instead of silently creating
  # crates.io drift.
  # Error: "The provided access token is not valid for crate `name`"
  if grep -qiE "token.*not valid for crate|provided access token.*not valid" "$log"; then
    echo "❌ Token not valid for ${crate}; configure crates.io Trusted Publishing before releasing."
    rm -f "$log"
    return 1
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
      try_publish "$crate" "$ver"
      return 0
      ;;
    403)
      echo "⚠️  crates.io API returned 403 (likely WAF/Cloudflare). Falling back to publish-attempt idempotency..."
      try_publish "$crate" "$ver"
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
          if try_publish "$crate" "$ver"; then
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
