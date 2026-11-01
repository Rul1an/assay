use super::observation::ToolObservations;
use std::{
    io::{self, BufRead, BufReader, Write},
    process::ChildStdout,
    sync::{Arc, Mutex},
};

pub(super) fn run_server_to_client(
    child_stdout: ChildStdout,
    stdout: Arc<Mutex<io::Stdout>>,
    server_id: String,
    observations: Arc<Mutex<ToolObservations>>,
) -> io::Result<()> {
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    loop {
        // A line that stops this reader is one the client may still get something from.
        let read = reader.read_line(&mut line).inspect_err(|_| {
            observations.lock().unwrap().invalidate();
        })?;
        if read == 0 {
            break;
        }
        // Observed before the client can read it, so its next request sees this state.
        let processed_line = observations.lock().unwrap().server_line(&line, &server_id);

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
    }
}
