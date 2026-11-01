# block-buffer affected-range disposition, 2026-10-27

**Closed 2026-10-31: the exception this record justified has been removed.**

## Closure, 2026-10-31

GHSA-qwgh-2vcv-g2f7 first reported every version below 0.12.1. The global GitHub
advisory (updated 2026-10-30) and the OSV record (modified 2026-10-30) now give the
affected range as `>= 0.11.0, < 0.12.1`, so 0.10.4 is outside it. The advisory is
not withdrawn: 0.11.x and 0.12.0 stay affected.

The removal rests on a scan, not on the record alone. The required `OSV Cargo.lock`
job (pinned action v2.7.0, both lockfiles) ran on two throwaway trees on 2026-10-31,
each with the exception deleted from `dd3a79a44`:

| Tree | `block-buffer` in `Cargo.lock` | in `fuzz/Cargo.lock` | Result |
| --- | --- | --- | --- |
| Lockfiles unchanged (`db995b43d`) | 0.10.4, 0.12.1 | 0.12.1 | No issues found |
| Control, 0.12.1 pinned to 0.12.0 (`98935750b`) | 0.10.4, 0.12.0 | 0.12.0 | Job failed; finding for 0.12.0 in both lockfiles, none for 0.10.4 |

The control is what makes the clean result mean something: the same scanner, with
the same configuration, still reports this advisory where it applies. Both runs
filtered only RUSTSEC-2024-0370.

Removed with the exception: its `osv-scanner.toml` entry, the two-artifact allowlist
in `check-osv-cargo-lock-gate.py`, and that allowlist's cases in
`test-osv-cargo-lock-gate.sh`. They bounded an ignore that no longer exists. An
affected `block-buffer` now fails the scanner directly.

This follows the upstream record. If the range widens again, the finding returns
and the job goes red, which needs a fresh decision. A new exception needs its own
review and its own bound; the removed allowlist is in `ebf43ab2a`. Nothing here
establishes exploitability in Assay or the absence of unrelated defects in 0.10.4.

The rest of this record is the disposition as it stood until closure.

## Disposition, 2026-10-27

The advisory then reported every version below 0.12.1. The described inline-cursor
corruption is absent from the exact crates.io 0.10.4 artifact. This was a narrow
affected-range disposition, not a general memory-safety claim.

### Artifact boundary

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

### Enforcement and expiry

OSV 2.7.0 can ignore an advisory ID or all vulnerabilities of a package, but cannot
express just this advisory/package/version tuple. The disposition therefore paired
an expiring ID exception in root `osv-scanner.toml` with a closed artifact allowlist
in `check-osv-cargo-lock-gate.py`: every block-buffer record in both lockfiles had to
match a reviewed version, its checksum, and the crates.io registry source. Expiry was
set to 2026-11-10.

### Bounded Linux reproduction

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
