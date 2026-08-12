//! Private CONFIG flag writers shared by the Linux loader setters.
//!
//! Not a public API and not a generic loader: only the independently configurable
//! flag keys that must not alias share this one put path so tests can drive the
//! same writers the production setters use.

use crate::MonitorError;
use assay_common::{KEY_DEDUP_OPEN_PATHS, KEY_EMIT_OBSERVED_CONNECT};

pub(crate) trait FlagConfigSink {
    fn put_flag(&mut self, key: u32, value: u32) -> Result<(), MonitorError>;
}

pub(crate) fn apply_emit_observed_connect(
    sink: &mut impl FlagConfigSink,
    enabled: bool,
) -> Result<(), MonitorError> {
    sink.put_flag(KEY_EMIT_OBSERVED_CONNECT, u32::from(enabled))
}

pub(crate) fn apply_dedup_open_paths(
    sink: &mut impl FlagConfigSink,
    enabled: bool,
) -> Result<(), MonitorError> {
    sink.put_flag(KEY_DEDUP_OPEN_PATHS, u32::from(enabled))
}

#[cfg(test)]
mod tests {
    use super::{apply_dedup_open_paths, apply_emit_observed_connect, FlagConfigSink};
    use crate::MonitorError;
    use assay_common::{KEY_DEDUP_OPEN_PATHS, KEY_EMIT_OBSERVED_CONNECT};
    use std::collections::HashMap;

    #[derive(Default)]
    struct RecordingSink {
        values: HashMap<u32, u32>,
    }

    impl FlagConfigSink for RecordingSink {
        fn put_flag(&mut self, key: u32, value: u32) -> Result<(), MonitorError> {
            self.values.insert(key, value);
            Ok(())
        }
    }

    /// M2: the real apply helpers must write distinct CONFIG keys. If
    /// `apply_dedup_open_paths` were mutated to use `KEY_EMIT_OBSERVED_CONNECT`,
    /// opposite values cannot both be retained.
    #[test]
    fn real_flag_setters_write_distinct_config_keys() {
        let mut sink = RecordingSink::default();
        apply_emit_observed_connect(&mut sink, true).expect("emit observed-connect write");
        apply_dedup_open_paths(&mut sink, false).expect("dedup open-paths write");

        assert_eq!(
            sink.values.get(&KEY_EMIT_OBSERVED_CONNECT),
            Some(&1),
            "set_emit_observed_connect wiring must retain its value when dedup is also set"
        );
        assert_eq!(
            sink.values.get(&KEY_DEDUP_OPEN_PATHS),
            Some(&0),
            "set_dedup_open_paths wiring must retain its value when observed-connect is also set"
        );
    }
}
