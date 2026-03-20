#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let _ = assay_core::replay::verify_bundle(Cursor::new(data));
});
