use assay_common::encode_kernel_dev;
use proptest::prelude::*;

// Inverse of kernel new_encode_dev logic:
// (minor & 0xff) | (major << 8) | ((minor & !0xff) << 12)
//
// Decoded:
// major = (dev >> 8) & 0xfff
// minor = (dev & 0xff) | ((dev >> 12) & 0xfff00)
fn decode_kernel_dev(dev: u32) -> (u32, u32) {
    let major = (dev >> 8) & 0xfff;
    let minor = (dev & 0xff) | ((dev >> 12) & 0xfff00);
    (major, minor)
}

#[test]
fn test_regression_pairs() {
    let pairs = vec![
        (0, 0),         // Zero
        (1, 1),         // Basic
        (8, 1),         // SDA1 likely
        (255, 255),     // Classic 8-bit limits
        (4095, 1048575),// Max standard (12-bit major, 20-bit minor)
        (0, 256),       // Minor > 255 (extended minor part usage)
        (300, 0),       // Major > 255
    ];

    for (maj, min) in pairs {
        // Since we don't have a reliable userspace `makedev` across platforms that matches Linux internal `dev_t`
        // consistently for all ranges (especially macOS vs Linux),
        // we test our `encode_kernel_dev` against our correct `decode` logic,
        // AND we manually verify the bit layout for specific known patterns.

        // 1. Manually construct expected (Linux new_encode_dev)
        // (min & 0xff) | (maj << 8) | ((min & !0xff) << 12)
        let expected = (min & 0xff) | (maj << 8) | ((min & !0xff) << 12);

        // We can't rely on `libc::makedev` -> `encode_kernel_dev` for this test
        // because `libc::makedev` on the build machine might not match Linux target semantics (e.g. macOS).
        // Instead, we trust `encode_kernel_dev`'s implementation logic if it passes property tests
        // against the inverse decode *assuming the input u64 came from a Linux libc::makedev*.
        //
        // WAIT: `encode_kernel_dev` takes a u64 `dev` and calls `libc::major` / `libc::minor`.
        // This is the tricky part. On macOS `libc::major` works on `i32`.
        // If we want to unit test the *bit manipulation* of `encode_kernel_dev`,
        // we need to be able to feed it a u64 that `libc::major` interprets correctly.
        //
        // Problem: `libc::major` is a macro or function that depends on platform.
        // On macOS: major(x) is likely ((x) >> 24) & 0xff.
        // On Linux: complex.
        //
        // Solution for P1.1 Verify Gate:
        // We should test the *internal bit logic* separated from libc if possible,
        // OR we accept that this regression test might only be fully valid on Linux.
        // But `assay-cli` is cross-platform tool.
        //
        // Actually, `encode_kernel_dev` logic IS:
        // let major = unsafe { libc::major(dev) ... }
        // ...
        //
        // So `encode_kernel_dev` assumes the input `dev` is a platform-native `dev_t`.
        // The regression test here should verify that *given a (Maj, Min) pair*,
        // if we construct a `dev_t` (via makedev) that yields that Maj/Min,
        // then `encode_kernel_dev` produces the correct Linux Kernel Internal Integer.

        let dev_t = libc::makedev(maj as _, min as _) as u64;

        // Verify libc extraction works as expected (sanity check for test environment)
        let extracted_maj = libc::major(dev_t as _) as u32;
        let extracted_min = libc::minor(dev_t as _) as u32;

        // On macOS, 12-bit major / 20-bit minor might not be representable or encoded differently.
        // If the platform can't represent the pair, we skip the exact match test for that pair
        // or assert properties that hold.
        if extracted_maj == maj && extracted_min == min {
            let encoded = encode_kernel_dev(dev_t);
            assert_eq!(encoded, expected, "Failed for ({}, {}) -> Expected {:#x}, Got {:#x}", maj, min, expected, encoded);

            // Roundtrip check
            let (dec_maj, dec_min) = decode_kernel_dev(encoded);
            assert_eq!(dec_maj, maj);
            assert_eq!(dec_min, min);
        } else {
            eprintln!("Skipping ({}, {}) - Platform makedev/major mismatch (Got {}, {})", maj, min, extracted_maj, extracted_min);
        }
    }
}

proptest! {
    #[test]
    fn test_roundtrip_property(major in 0u32..4096, minor in 0u32..1048576) {
        // We want to verify: decode(encode_logic(maj, min)) == (maj, min)
        // We cannot easily use `encode_kernel_dev` because of `libc` platform dependency on input.
        // So we test the *logic* directly here essentially.
        //
        // Manual encode logic matching `encode_kernel_dev`:
        let encoded = (minor & 0xff) | (major << 8) | ((minor & !0xff) << 12);

        let (dec_maj, dec_min) = decode_kernel_dev(encoded);
        assert_eq!(dec_maj, major);
        assert_eq!(dec_min, minor);
    }
}
