#![no_main]

use assay_core::mcp::policy::McpPolicy;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = core::str::from_utf8(data) {
        let _ = serde_yaml::from_str::<assay_core::model::Policy>(input);
        let _ = serde_yaml::from_str::<McpPolicy>(input);
    }
});
