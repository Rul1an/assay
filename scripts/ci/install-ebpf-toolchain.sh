#!/usr/bin/env bash
set -euo pipefail

toolchain="${ASSAY_EBPF_RUST_TOOLCHAIN:-nightly-2026-01-01}"
bpf_linker_version="${ASSAY_BPF_LINKER_VERSION:-0.10.3}"

rustup toolchain install "${toolchain}" --profile minimal --component rust-src

if ! command -v bpf-linker >/dev/null 2>&1 \
    || ! bpf-linker --version | grep -Fq "bpf-linker ${bpf_linker_version}"; then
    rustup run "${toolchain}" cargo install \
        bpf-linker \
        --version "${bpf_linker_version}" \
        --locked \
        --force
fi

echo "eBPF toolchain: ${toolchain}"
echo "bpf-linker: $(bpf-linker --version)"
