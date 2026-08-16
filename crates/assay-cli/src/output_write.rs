//! One write/flush policy for machine documents on stdout (and other output targets).
//!
//! A partial or absent document is not a clean success. Any write or flush failure,
//! including `ErrorKind::BrokenPipe`, is `EXIT_INFRA_ERROR`. Callers render first,
//! then hand the finished bytes to this module; they do not map the error themselves.

use std::io::{self, Write};

use crate::exit_codes::{EXIT_INFRA_ERROR, EXIT_SUCCESS};

/// Write a fully rendered machine document plus a trailing newline, then flush.
pub(crate) fn write_document(writer: &mut impl Write, rendered: &str) -> io::Result<()> {
    writer.write_all(rendered.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Map an output-write result to an exit code. A write failure is an infra/output
/// problem (`EXIT_INFRA_ERROR`) for every target, including stdout BrokenPipe.
pub(crate) fn map_write_result(target: &str, result: io::Result<()>) -> i32 {
    match result {
        Ok(()) => EXIT_SUCCESS,
        Err(error) => {
            eprintln!("[infra_error] cannot write output ({target}): {error}");
            EXIT_INFRA_ERROR
        }
    }
}

/// Write a rendered JSON document to stdout. Returns `EXIT_SUCCESS` only when the
/// write and flush both succeed; callers then apply their own command exit.
pub(crate) fn write_stdout_json(rendered: &str) -> i32 {
    map_write_result("stdout", write_document(&mut io::stdout(), rendered))
}

#[cfg(test)]
mod tests {
    use super::{map_write_result, write_document};
    use crate::exit_codes::{EXIT_INFRA_ERROR, EXIT_SUCCESS};
    use std::io::{self, Error, ErrorKind, Write};

    struct FailWrite(ErrorKind);

    impl Write for FailWrite {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(Error::new(self.0, "injected write failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_failure_maps_to_infra_error_for_any_target() {
        assert_eq!(
            map_write_result(
                "stdout",
                Err(Error::new(ErrorKind::BrokenPipe, "pipe closed"))
            ),
            EXIT_INFRA_ERROR
        );
        assert_eq!(
            map_write_result(
                "/tmp/x.json",
                Err(Error::new(ErrorKind::PermissionDenied, "nope"))
            ),
            EXIT_INFRA_ERROR
        );
        assert_eq!(map_write_result("stdout", Ok(())), EXIT_SUCCESS);
    }

    #[test]
    fn write_document_failures_share_the_one_mapping() {
        for kind in [
            ErrorKind::BrokenPipe,
            ErrorKind::PermissionDenied,
            ErrorKind::Other,
        ] {
            let mut writer = FailWrite(kind);
            let result = write_document(&mut writer, r#"{"ok":true}"#);
            assert_eq!(
                map_write_result("stdout", result),
                EXIT_INFRA_ERROR,
                "{kind:?} must not be swallowed or remapped"
            );
        }
    }

    #[test]
    fn write_document_appends_newline_and_maps_success() {
        let mut buf = Vec::new();
        let result = write_document(&mut buf, r#"{"ok":true}"#);
        assert_eq!(map_write_result("stdout", result), EXIT_SUCCESS);
        assert_eq!(buf, b"{\"ok\":true}\n");
    }
}
