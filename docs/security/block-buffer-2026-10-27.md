# block-buffer affected-range disposition, 2026-10-27

GHSA-qwgh-2vcv-g2f7 reports every version below 0.12.1. The described
inline-cursor corruption is absent from the exact crates.io 0.10.4 artifact.
This is a narrow affected-range disposition, not a general memory-safety claim.
The upstream advisory has not been corrected or withdrawn.

## Artifact boundary

| Version | crates.io SHA-256 | Disposition |
| --- | --- | --- |
| 0.10.4 | `3078c7629b62d3f0439517fa394996acacc5cbc91c5a20d8c658e77abd503a71` | Separate `pos: u8`; no `ReadBuffer` |
| 0.12.1 | `d2f6c7dbe95a6ed67ad9f18e57daf93a2f034c524b99fd2b76d18fdfeb6660aa` | Upstream patch with reset guards |

In 0.10.4, `EagerBuffer` aliases `BlockBuffer<_, Eager>`. The data buffer and
cursor occupy separate fields; compression callbacks receive immutable blocks,
not mutable access to the cursor. The `ReadBuffer` API is absent. Source files
were compared with the published crate archive, whose checksum matches crates.io.
Inactive dependency edges are not the justification: the whole lockfile is scanned.

Sources: [advisory](https://github.com/RustCrypto/utils/security/advisories/GHSA-qwgh-2vcv-g2f7),
[0.10.4 source](https://github.com/RustCrypto/utils/blob/block-buffer-v0.10.4/block-buffer/src/lib.rs),
[0.12.1 release](https://github.com/RustCrypto/utils/releases/tag/block-buffer-v0.12.1).

## Enforcement and expiry

OSV 2.7.0 can ignore an advisory ID or all vulnerabilities of a package, but cannot
express just this advisory/package/version tuple. We use an expiring ID disposition
in root `osv-scanner.toml`, paired with a closed artifact allowlist in the existing
`check-osv-cargo-lock-gate.py`. This changes scanner filtering; it is not a claim
that the scanner still reports the ignored finding.

The combined gate preserves rejection of affected artifacts: both full lockfiles
must be readable; every block-buffer record must match one of the two reviewed
versions, its checksum, canonical name, and crates.io registry source. Unknown
versions (including future releases), prereleases, substituted sources/checksums,
and duplicate vulnerable versions fail closed. No semver-range approximation or
package-wide advisory ignore is used. Other advisories remain scanned.

The required `CI` job's hardening step runs `test-osv-cargo-lock-gate.sh`. Its
control copies both actual lockfiles and executes the production checker; negative
cases exercise the same CLI. The existing workflow contract fixes scan scope to
these two lockfiles. The required CI aggregate still requires the full OSV job.
Local hook success alone is not hosted enforcement proof.

Expiry is **2026-11-10**: reassess upstream range corrections, then remove the
exception if possible. Expiry restores the scanner finding and blocks landing;
it does not silently extend the disposition. A new artifact requires fresh review.
A future change in the advisory's technical scope also requires reassessment.

## Bounded Linux reproduction

Measured on Linux aarch64, kernel 6.8.0-147-generic, with Miri from
`nightly-2026-01-01` (rustc 1.94.0-nightly `8d670b93d`, 2025-12-31).
These are isolated upstream-artifact probes, not a full Assay test-suite result.
The dependency inventory was Assay `43bfe233ad434e0ba09ec3d0ed0bc5bc1cd88b16`.

- 0.10.4: one applicable eager-buffer test passed in debug and release Miri.
- 0.12.0: both tests failed cursor invariants in debug; separate release runs each
  reported `Undefined Behavior: entering unreachable code` at `get_pos`.
- 0.12.1: both tests passed in debug and release Miri.

To replay, create an isolated library crate (outside the Assay workspace), set
`block-buffer = "=0.12.1"` in dependencies and use the following as `src/lib.rs`.
Install that pinned nightly with `miri` and `rust-src`; generate and retain the
probe Cargo.lock, then run `cargo +nightly-2026-01-01 miri test --locked --release`.
Use a separate crate with `=0.12.0` for the negative control; run the `eager_cursor`
and `read_cursor` filters separately because Miri stops at the first UB.
For `=0.10.4`, replace `array::typenum` with `generic_array::typenum` and omit the
read-buffer test, since that API does not exist. Do not call that omission a pass.

```rust
use block_buffer::array::typenum::U4;
use block_buffer::EagerBuffer;
use std::panic::{catch_unwind, AssertUnwindSafe};
#[test]
fn eager_cursor_remains_valid_after_compression_panic() {
    let mut buf = EagerBuffer::<U4>::new(&[1, 2]);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        buf.digest_blocks(&[3, 0xff], |_| panic!("simulated compression failure"));
    }));
    assert!(caught.is_err());
    assert!(buf.get_pos() < 4);
    buf.reset();
    buf.digest_blocks(&[1, 2, 3, 4], |blocks| assert_eq!(&blocks[0][..], &[1, 2, 3, 4]));
    assert_eq!(buf.get_pos(), 0);
}

#[test]
fn read_cursor_remains_valid_after_generator_panic() {
    let mut buf = block_buffer::ReadBuffer::<U4>::default();
    let caught = catch_unwind(AssertUnwindSafe(|| {
        buf.write_block(1, |block| {
            block[0] = 0xff;
            panic!("simulated block generation failure");
        }, |_| {});
    }));
    assert!(caught.is_err());
    assert!(buf.get_pos() <= 4);
}
```

This reproduces the two reported mechanisms. It does not establish exploitability
in Assay, absence of unrelated defects, or upstream endorsement of this disposition.
