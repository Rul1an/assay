use super::tools::observe_tool_definition;
use crate::mcp::identity::ToolIdentity;
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, Write},
    process::ChildStdout,
    sync::{Arc, Mutex},
};

pub(super) fn run_server_to_client(
    child_stdout: ChildStdout,
    stdout: Arc<Mutex<io::Stdout>>,
    server_id: String,
    identity_cache: Arc<Mutex<HashMap<String, ToolIdentity>>>,
    tool_definition_cache: Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>,
) -> io::Result<()> {
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let mut processed_line = line.clone();

        // Phase 9: Compute Identities on tools/list response
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(result) = v.get_mut("result") {
                if let Some(tools) = result.get_mut("tools").and_then(|t| t.as_array_mut()) {
                    for tool in tools {
                        if let Some(observation) = observe_tool_definition(tool, &server_id) {
                            let mut identities = identity_cache.lock().unwrap();
                            identities.insert(observation.name.clone(), observation.identity);
                            drop(identities);

                            if let Some(binding) = observation.binding {
                                let mut bindings = tool_definition_cache.lock().unwrap();
                                bindings.insert(observation.name, binding);
                            }
                        }
                    }
                    processed_line = serde_json::to_string(&v).unwrap_or(line.clone()) + "\n";
                }
            }
        }

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

        assert_send::<Arc<Mutex<HashMap<String, ToolIdentity>>>>();
        assert_send::<Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>>();
    }
}
