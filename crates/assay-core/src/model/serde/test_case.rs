//! Project the closed entry into the test used by evaluation and migration.
use super::{Expected, RawExpected, RawTestCase, TestCase};
use serde::{de::Error as _, Deserialize};

pub(super) fn read_test_case<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    read_expected: impl FnOnce(&str, serde_json::Value) -> Result<Expected, String>,
) -> Result<TestCase, D::Error> {
    let RawTestCase {
        id,
        input,
        expected,
        assertions,
        on_error,
        tags,
        metadata,
    } = RawTestCase::deserialize(deserializer)?;
    let extra_assertions = assertions.unwrap_or_default();

    // A missing `expected:` key stays permissive: a test may carry its checks in
    // `assertions:` instead. It resolves to the vacuous default, which the
    // `W_CFG_VACUOUS_EXPECTED` rule in `assay validate` reports when the test has
    // no assertions either. A present-but-unparsable key is a different matter and
    // is a hard error below.
    let expected_main = match expected {
        RawExpected::Present(val) => read_expected(&id, val).map_err(D::Error::custom)?,
        RawExpected::Missing => Expected::default(),
    };

    Ok(TestCase {
        id,
        input,
        expected: expected_main,
        assertions: if extra_assertions.is_empty() {
            None
        } else {
            Some(extra_assertions)
        },
        on_error,
        tags,
        metadata: metadata
            .map(|payload| payload.0.into_native_value())
            .transpose()
            .map_err(D::Error::custom)?,
    })
}
