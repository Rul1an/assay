#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

workspace_aya_minor="$(
  sed -nE 's/^aya = "([0-9]+\.[0-9]+)(\.[0-9]+)?".*/\1/p' Cargo.toml
)"
workspace_aya_log_minor="$(
  sed -nE 's/^aya-log = "([0-9]+\.[0-9]+)(\.[0-9]+)?".*/\1/p' Cargo.toml
)"
ebpf_tag_minors="$(
  sed -nE 's/^aya-(ebpf|log-ebpf) = \{ .*tag = "aya-v([0-9]+\.[0-9]+)(\.[0-9]+)?".*/\2/p' \
    crates/assay-ebpf/Cargo.toml | sort -u
)"

if [[ -z "$workspace_aya_minor" || -z "$workspace_aya_log_minor" || -z "$ebpf_tag_minors" ]]; then
  echo "FAIL: could not read aya/aya-log workspace versions or aya-ebpf git tags" >&2
  exit 1
fi

if [[ "$(wc -l <<<"$ebpf_tag_minors" | tr -d ' ')" != "1" ]]; then
  echo "FAIL: aya-ebpf and aya-log-ebpf must use the same aya git tag" >&2
  echo "$ebpf_tag_minors" >&2
  exit 1
fi

ebpf_tag_minor="$ebpf_tag_minors"
if [[ "$workspace_aya_minor" != "$ebpf_tag_minor" ]]; then
  echo "FAIL: workspace aya $workspace_aya_minor must align with aya-ebpf tag $ebpf_tag_minor" >&2
  echo "Update aya, aya-log, aya-ebpf, and aya-log-ebpf as one compatibility line." >&2
  exit 1
fi

case "$ebpf_tag_minor" in
  0.13) expected_aya_log_minor="0.2" ;;
  0.14) expected_aya_log_minor="0.3" ;;
  *)
    echo "FAIL: unknown aya/eBPF compatibility line $ebpf_tag_minor; update this guard with the matching aya-log line" >&2
    exit 1
    ;;
esac

if [[ "$workspace_aya_log_minor" != "$expected_aya_log_minor" ]]; then
  echo "FAIL: workspace aya-log $workspace_aya_log_minor must be $expected_aya_log_minor for aya $ebpf_tag_minor" >&2
  exit 1
fi

echo "PASS: aya workspace deps align with assay-ebpf git tag aya-v${ebpf_tag_minor}.x"
