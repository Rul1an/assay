use super::client_output::ClientOutput;
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
    stdout: Arc<Mutex<ClientOutput>>,
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

        // The session's own output: no process-wide stdout lock, and a write to a client
        // that stopped reading ends when the session stops (see `client_output`).
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

    #[cfg(any(unix, windows))]
    #[test]
    #[ignore = "run scripts/ci/test-mcp-relay-stdout.sh to supply and check the client pipe"]
    fn relay_returns_while_a_host_holds_the_process_stdout_lock() {
        // Library use: the host holds the process-wide stdout lock on its own thread. The relay
        // must still deliver a frame, reach the child's EOF and return, so the session can join it.
        // Let Command create the ChildStdout: on Windows an arbitrary PipeReader handle
        // does not satisfy ChildStdout's asynchronous-read contract.
        let mut upstream =
            std::process::Command::new(if cfg!(windows) { "python" } else { "python3" })
                .args(["-c", r#"print('{"jsonrpc":"2.0","id":1,"result":{}}')"#])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
        let child_stdout = upstream.stdout.take().unwrap();
        assert!(upstream.wait().unwrap().success());
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let session_out = Arc::new(Mutex::new(ClientOutput::stdout(stopped.clone()).unwrap()));
        let host_lock = io::stdout().lock();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = run_server_to_client(
                child_stdout,
                session_out,
                "srv".to_string(),
                Arc::new(Mutex::new(ToolObservations::default())),
                Capture::new(None),
                stopped,
            );
            let _ = done.send(result.map_err(|e| e.kind()));
        });
        let returned = finished.recv_timeout(std::time::Duration::from_secs(5));
        drop(host_lock);
        assert!(
            returned.is_ok(),
            "the relay did not return while a host held the process stdout lock"
        );
        assert_eq!(returned.unwrap(), Ok(()));
    }

    #[test]
    fn proxy_contract_server_loop_enrichment_inputs_remain_private() {
        fn assert_send<T: Send>() {}

        assert_send::<Arc<Mutex<ToolObservations>>>();
        assert_send::<Arc<Capture>>();
    }
}
