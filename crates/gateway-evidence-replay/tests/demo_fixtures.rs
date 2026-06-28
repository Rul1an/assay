use std::fs;

use gateway_evidence_replay::schema::{Ceiling, Reason, Status};
use gateway_evidence_replay::verify_json_str;
use serde::Deserialize;

const DEMO_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/gateway-path-v0/demo/"
);

#[derive(Debug, Deserialize)]
struct ExpectedFile {
    cases: Vec<ExpectedCase>,
}

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    file: String,
    status: Status,
    ceiling: Option<Ceiling>,
    reasons: Vec<Reason>,
}

#[test]
fn demo_fixtures_replay_expected_verdicts() {
    let expected: ExpectedFile =
        serde_json::from_str(&fs::read_to_string(format!("{DEMO_DIR}expected.json")).unwrap())
            .expect("expected verdicts parse");
    assert_eq!(expected.cases.len(), 4);

    for case in expected.cases {
        let body = fs::read_to_string(format!("{DEMO_DIR}{}", case.file)).expect("demo fixture");
        let got = verify_json_str(&body);
        assert_eq!(got.status, case.status, "{}", case.file);
        assert_eq!(got.ceiling, case.ceiling, "{}", case.file);
        assert_eq!(got.reasons, case.reasons, "{}", case.file);
    }
}
