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
                xml.push_str(&format!(r#"<skipped message="{}"/>"#, escape(&r.message)))
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
