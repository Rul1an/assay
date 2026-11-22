use super::observation::ToolObservations;
use super::request_receipts::Capture;
use crate::mcp::unique_json::{parse_server_frame, FrameDirection};
use std::{
    io::{self, BufReader, Write},
    process::ChildStdout,
    sync::{Arc, Mutex},
};

pub(super) fn run_server_to_client(
    child_stdout: ChildStdout,
    stdout: Arc<Mutex<io::Stdout>>,
    server_id: String,
    observations: Arc<Mutex<ToolObservations>>,
    capture: Arc<Capture>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
) -> io::Result<()> {
    #[cfg(unix)]
    let child_stdout = crate::mcp::live_input::polled_file(
        std::fs::File::from(std::os::fd::OwnedFd::from(child_stdout)),
        &stopped,
    );
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    while !stopped.load(std::sync::atomic::Ordering::Acquire) {
        // A line that stops this reader is one the client may still get something from.
        let read = FrameDirection::Server
            .read_line(&mut reader, &mut line)
            .inspect_err(|_| {
                observations.lock().unwrap().invalidate();
            })?;
        if read == 0 {
            break;
        }
        if line.trim().is_empty() {
            line.clear();
            continue;
        }
        let processed_line = match parse_server_frame(&line) {
            Ok(frame) => {
                capture.observe_server_frame(&line, &frame);
                observations
                    .lock()
                    .unwrap()
                    .accepted_server_frame(&line, &server_id, frame)
            }
            Err(_) => {
                observations.lock().unwrap().invalidate();
                serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid upstream frame"}}).to_string() + "\n"
            }
        };

        let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
        out.write_all(processed_line.as_bytes())?;
        out.flush()?;
        line.clear();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_contract_server_loop_enrichment_inputs_remain_private() {
        fn assert_send<T: Send>() {}

        assert_send::<Arc<Mutex<ToolObservations>>>();
        assert_send::<Arc<Capture>>();
    }
}
