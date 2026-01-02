use assay_core::on_error::log_fail_safe;
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use tracing_subscriber::fmt::MakeWriter;

#[test]
fn test_failsafe_emits_structured_log() {
    // Capture stdout/stderr in a buffer
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let buffer_clone = buffer.clone();

    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(move || MockWriter(buffer_clone.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        log_fail_safe("Test reason", Some("test_config.yaml"));
    });

    let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();

    // Verification: Check for JSON structure and fields
    assert!(output.contains("\"event\":\"assay.failsafe.triggered\""));
    assert!(output.contains("\"reason\":\"Test reason\""));
    assert!(output.contains("\"config_path\":\"test_config.yaml\""));
    assert!(output.contains("\"action\":\"allowed\""));
    assert!(output.contains("\"timestamp\""));
}

struct MockWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
