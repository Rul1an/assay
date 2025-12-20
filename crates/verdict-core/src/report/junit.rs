use crate::model::{TestResultRow, TestStatus};
use std::path::Path;

pub fn write_junit(suite: &str, results: &[TestResultRow], out: &Path) -> anyhow::Result<()> {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(r#"<testsuite name="{}">"#, escape(suite)));
    xml.push('\n');

    for r in results {
        xml.push_str(&format!(r#"  <testcase name="{}">"#, escape(&r.test_id)));
        match r.status {
            TestStatus::Pass => {}
            TestStatus::Warn | TestStatus::Flaky => {
                xml.push_str(&format!(
                    r#"<system-out>WARNING: {}</system-out>"#,
                    escape(&r.message)
                ))
            }
            TestStatus::Fail => {
                xml.push_str(&format!(r#"<failure message="{}"/>"#, escape(&r.message)))
            }
            TestStatus::Error => {
                xml.push_str(&format!(r#"<error message="{}"/>"#, escape(&r.message)))
            }
        }
        xml.push_str("</testcase>\n");
    }

    xml.push_str("</testsuite>\n");
    std::fs::write(out, xml)?;
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TestStatus;
    use crate::model::TestResultRow;

    #[test]
    fn test_junit_output_structure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("junit.xml");

        let results = vec![
            TestResultRow {
                test_id: "test_pass".into(),
                status: TestStatus::Pass,
                message: "ok".into(),
                score: Some(1.0),
                cached: false,
                details: serde_json::Value::Null,
                duration_ms: Some(10),
            },
            TestResultRow {
                test_id: "test_warn".into(),
                status: TestStatus::Warn,
                message: "almost".into(),
                score: Some(0.5),
                cached: false,
                details: serde_json::Value::Null,
                duration_ms: Some(10),
            },
            TestResultRow {
                test_id: "test_fail".into(),
                status: TestStatus::Fail,
                message: "bad".into(),
                score: Some(0.0),
                cached: false,
                details: serde_json::Value::Null,
                duration_ms: Some(10),
            },
        ];

        write_junit("demo", &results, &path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains(r#"<testsuite name="demo">"#));
        // Pass
        assert!(content.contains(r#"<testcase name="test_pass">"#));
        // Warn (system-out)
        assert!(content.contains(r#"<testcase name="test_warn">"#));
        assert!(content.contains(r#"<system-out>WARNING: almost</system-out>"#));
        // Fail
        assert!(content.contains(r#"<testcase name="test_fail">"#));
        assert!(content.contains(r#"<failure message="bad"/>"#));
    }
}
