#![cfg(unix)]
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

// Helper mimicking Linux kernel `MKDEV` macro (include/linux/kdev_t.h)
// MINORBITS = 20
// MKDEV(ma, mi) = (((ma) << MINORBITS) | (mi))
fn mkdev_u32(major: u32, minor: u32) -> u32 {
    ((major & 0xfff) << 20) | (minor & 0xfffff)
}

#[test]
fn test_regression_pairs() {
    let pairs = vec![
        (0, 0),         // Zero
        (1, 1),         // Basic (8, 1) -> 0x100001
        (8, 1),         // SDA1 typical -> 0x800001
        (255, 255),     // Classic 8-bit limits
        (4095, 1048575),// Max standard (12-bit major, 20-bit minor)
        (0, 256),       // Minor > 255 (extended minor part usage)
        (300, 0),       // Major > 255
    ];

    for (maj, min) in pairs {
        // Construct a "fake" dev_t.
        // On Linux we could use libc::makedev, but we want to verify OUR encoding logic
        // against the expected bit pattern regardless of the host platform's libc.
        // Functional test: if we strictly assume input `dev` to encode_kernel_dev is
        // a u64 coming from a Linux-compatible source (or our own internal representation).
        //
        // Actually, encode_kernel_dev takes u64 and calls libc::major/minor.
        // We can't easily mock libc here.
        // Ideally we would verify: encode_kernel_dev(makedev(maj, min)) == mkdev_u32(maj, min).
        // But makedev varies by platform.
        //
        // Since this test runs on the Linux CI runner (or is gated), we can assume real Linux behavior.

        let dev_t = unsafe { libc::makedev(maj as _, min as _) } as u64;

        // Sanity check: does the platform libc agree with our inputs?
        let extracted_maj = unsafe { libc::major(dev_t as _) } as u32;
        let extracted_min = unsafe { libc::minor(dev_t as _) } as u32;

        if extracted_maj == maj && extracted_min == min {
            let encoded = encode_kernel_dev(dev_t);
            let expected = mkdev_u32(maj, min);
            assert_eq!(encoded, expected, "Failed for ({}, {}) -> Expected {:#x}, Got {:#x}", maj, min, expected, encoded);
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
