//! One line at a time, for the human table of a verify command.
//!
//! Those tables print strings the bundle's producer chose (a run id inside a check id, a tool
//! name, a recorded verdict) next to the command's own summary lines. Whatever a line is built
//! from, [`ReportLines::line`] writes it as exactly one line with no terminal control sequence: a
//! newline or an escape inside a field becomes U+FFFD. The rule sits in the writer, so a renderer
//! cannot forget it for one field. `--format json` does not come through here; it is data.

use assay_evidence::sanitize::sanitize_terminal_line;
use std::io::{self, Write};

pub(super) struct ReportLines<W: Write> {
    out: W,
}

impl<W: Write> ReportLines<W> {
    pub(super) fn new(out: W) -> Self {
        Self { out }
    }

    /// Write `text` as one line.
    pub(super) fn line(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        writeln!(
            self.out,
            "{}",
            sanitize_terminal_line(text.as_ref(), usize::MAX)
        )
    }

    /// Write an empty line.
    pub(super) fn blank(&mut self) -> io::Result<()> {
        writeln!(self.out)
    }
}

#[cfg(test)]
pub(super) mod testing {
    /// The contract a rendered table is held to: `lines` lines, no control byte but the newline
    /// that ends each, no C1 control.
    pub(crate) fn assert_table(bytes: &[u8], lines: usize) {
        for (index, &byte) in bytes.iter().enumerate() {
            assert!(
                (byte >= 0x20 && byte != 0x7f) || byte == b'\n',
                "byte {index} is the control byte {byte:#04x}:\n{}",
                String::from_utf8_lossy(bytes)
            );
        }
        let text = std::str::from_utf8(bytes).expect("a table is UTF-8");
        assert!(
            !text.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)),
            "a C1 control reached the table:\n{text}"
        );
        assert_eq!(text.lines().count(), lines, "a field added lines:\n{text}");
    }

    /// Colour, a clipboard write, a bell, an 8-bit CSI, and a newline before `forged`.
    pub(crate) fn hostile(forged: &str) -> String {
        format!("x\x1b[31mRED\x1b[0m\x1b]52;c;cHduZWQ=\x07\u{9b}31m\n{forged}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_one_line_whatever_it_holds() {
        let mut out = Vec::new();
        let mut lines = ReportLines::new(&mut out);
        lines
            .line(format!("check {}", testing::hostile("OK: yes")))
            .unwrap();
        lines.blank().unwrap();
        lines.line("plain").unwrap();
        testing::assert_table(&out, 3);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("check x"), "{text}");
        assert!(!text.lines().any(|line| line == "OK: yes"), "{text}");
    }
}
