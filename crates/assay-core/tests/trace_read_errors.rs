use assay_core::trace::upgrader::StreamUpgrader;
use std::io::{self, BufRead, Cursor, Read};

const VALID: &[u8] = b"{\"type\":\"episode_start\",\"episode_id\":\"control\",\"timestamp\":0,\"input\":{\"prompt\":\"hello\"}}\n";

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("injected read failure"))
    }
}

impl BufRead for FailingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::other("injected read failure"))
    }

    fn consume(&mut self, _: usize) {}
}

#[test]
fn both_streams_report_io_failure_instead_of_eof() {
    let bare = StreamUpgrader::new(FailingReader).next();
    let observed = StreamUpgrader::new(FailingReader).observed().next();
    for result in [bare.map(|v| v.map(|_| ())), observed.map(|v| v.map(|_| ()))] {
        let error = result.expect("a read failure is not EOF").unwrap_err();
        assert!(error.is_io());
        assert!(error.to_string().contains("injected read failure"));
    }
}

#[test]
fn both_streams_reject_invalid_utf8_after_a_valid_record() {
    let mut bytes = VALID.to_vec();
    bytes.extend_from_slice(&[0xff, b'\n']);
    let bare: Result<Vec<_>, _> = StreamUpgrader::new(Cursor::new(&bytes)).collect();
    let observed: Result<Vec<_>, _> = StreamUpgrader::new(Cursor::new(&bytes))
        .observed()
        .collect();
    assert!(
        bare.is_err(),
        "bare stream accepted unreadable trailing input"
    );
    assert!(
        observed.is_err(),
        "observed stream accepted unreadable trailing input"
    );
}

#[test]
fn both_streams_accept_clean_eof() {
    let bare: Result<Vec<_>, _> = StreamUpgrader::new(Cursor::new(VALID)).collect();
    let observed: Result<Vec<_>, _> = StreamUpgrader::new(Cursor::new(VALID)).observed().collect();
    assert_eq!(bare.unwrap().len(), 1);
    assert_eq!(observed.unwrap().len(), 1);
}

#[test]
fn matching_coverage_does_not_hide_an_unreadable_suffix() {
    let config = serde_json::from_str(
        r#"{"configVersion":1,"suite":"read-error","model":"fake","tests":[{"id":"control","input":{"prompt":"hello"},"expected":{"type":"must_contain","must_contain":["hello"]}}]}"#,
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.jsonl");
    std::fs::write(&input, VALID).unwrap();
    assert!(assay_core::trace::verify::verify_coverage(&input, &config).is_ok());

    let mut bytes = VALID.to_vec();
    bytes.extend_from_slice(&[0xff, b'\n']);
    std::fs::write(&input, bytes).unwrap();
    assert!(assay_core::trace::verify::verify_coverage(&input, &config).is_err());
}

#[test]
fn ingest_reports_read_failure_for_jsonl_and_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.jsonl");
    for extension in ["jsonl", "db"] {
        std::fs::write(&input, VALID).unwrap();
        let clean_output = dir.path().join(format!("clean.{extension}"));
        assert!(assay_core::trace::ingest::ingest_file(&input, &clean_output).is_ok());

        let mut bytes = VALID.to_vec();
        bytes.extend_from_slice(&[0xff, b'\n']);
        std::fs::write(&input, bytes).unwrap();
        let failed_output = dir.path().join(format!("failed.{extension}"));
        assert!(assay_core::trace::ingest::ingest_file(&input, &failed_output).is_err());
        // Earlier output may remain: rejection does not promise transactional rollback.
    }
}
