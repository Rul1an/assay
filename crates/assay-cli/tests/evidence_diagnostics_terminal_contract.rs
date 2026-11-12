//! A bundle that fails verification is the bundle whose contents are least trusted, and the error
//! names things from it: a tar member's path, for one. `tests/evidence_show_terminal_contract.rs`
//! pins the table of a bundle that verified; this pins what the commands print when it did not.
//!
//! The contract, on the bytes written to stdout and stderr: no control byte except the newline
//! that ends a line, no C1 control, and no line the archive wrote for itself.

use assert_cmd::Command;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::path::Path;
use tempfile::tempdir;

/// A member path with a colour sequence, an OSC 52 clipboard write, a bell, an 8-bit CSI, and a
/// newline followed by the line `evidence show` prints for a bundle that verified.
const HOSTILE_MEMBER: &str =
    "evil-\x1b[31mRED\x1b[0m\x1b]52;c;cHduZWQ=\x07\u{9b}31m\nVerified:    \u{2705} OK";

fn write_archive(path: &Path, member: &str) {
    let file = std::fs::File::create(path).expect("create archive");
    let mut builder = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    let body = b"{}";
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, member, &body[..])
        .expect("append member");
    builder
        .into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip");
}

fn assert_terminal_safe(stream: &[u8], context: &str) {
    for (index, &byte) in stream.iter().enumerate() {
        assert!(
            byte >= 0x20 || byte == b'\n',
            "{context}: byte {index} is the control byte {byte:#04x}:\n{}",
            String::from_utf8_lossy(stream)
        );
        assert_ne!(byte, 0x7f, "{context}: byte {index} is DEL");
    }
    let text = String::from_utf8_lossy(stream);
    assert!(
        !text.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)),
        "{context}: a C1 control reached the terminal:\n{text}"
    );
    assert!(
        !text.lines().any(|line| line.starts_with("Verified:")),
        "{context}: the archive wrote a status line of its own:\n{text}"
    );
}

#[test]
fn a_bundle_that_fails_verification_cannot_write_to_the_terminal_through_the_error() {
    let dir = tempdir().expect("tempdir");
    let hostile = dir.path().join("hostile.tar.gz");
    let plain = dir.path().join("plain.tar.gz");
    write_archive(&hostile, HOSTILE_MEMBER);
    write_archive(&plain, "plain-member");

    // `lint` reports that verification failed without the cause, so it has no member name to
    // print; it is held to the byte contract all the same.
    for (command, names_member) in [
        (vec!["evidence", "verify"], true),
        (vec!["evidence", "show"], true),
        (vec!["evidence", "lint"], false),
        (vec!["evidence", "lint", "--format", "json"], false),
    ] {
        let label = command.join(" ");
        // Control: the same refusal for a harmless member name, so the hostile case is known to
        // reach the message that names the member.
        let control = Command::cargo_bin("assay")
            .expect("assay binary")
            .args(&command)
            .arg(&plain)
            .output()
            .expect("run assay");
        assert_ne!(
            control.status.code(),
            Some(0),
            "{label}: a foreign member must be refused"
        );
        let control_text = format!(
            "{}{}",
            String::from_utf8_lossy(&control.stdout),
            String::from_utf8_lossy(&control.stderr)
        );
        assert!(
            !names_member || control_text.contains("plain-member"),
            "{label}: the refusal no longer names the member, so this test proves nothing:\n{control_text}"
        );

        let output = Command::cargo_bin("assay")
            .expect("assay binary")
            .args(&command)
            .arg(&hostile)
            .output()
            .expect("run assay");
        assert_eq!(
            output.status.code(),
            control.status.code(),
            "{label}: the hostile name must be refused the same way"
        );
        assert_terminal_safe(&output.stderr, &format!("{label} stderr"));
        assert_terminal_safe(&output.stdout, &format!("{label} stdout"));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !names_member || text.contains("evil-"),
            "{label}: the refusal should still name the member, readable part included:\n{text}"
        );
    }
}
