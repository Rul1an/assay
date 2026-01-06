use super::audit::{AuditEvent, AuditLog};
use super::jsonrpc::JsonRpcRequest;
use super::policy::{make_deny_response, McpPolicy, PolicyDecision, PolicyState};
use std::{
    io::{self, BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

#[derive(Clone, Debug, Default)]
pub struct ProxyConfig {
    pub dry_run: bool,
    pub verbose: bool,
    pub audit_log_path: Option<std::path::PathBuf>,
}

pub struct McpProxy {
    child: Child,
    policy: McpPolicy,
    config: ProxyConfig,
}

impl McpProxy {
    pub fn spawn(
        command: &str,
        args: &[String],
        policy: McpPolicy,
        config: ProxyConfig,
    ) -> io::Result<Self> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // protocol blijft op stdout
            .spawn()?;

        Ok(Self {
            child,
            policy,
            config,
        })
    }

    pub fn run(mut self) -> io::Result<i32> {
        let mut child_stdin = self.child.stdin.take().expect("child stdin");
        let child_stdout = self.child.stdout.take().expect("child stdout");

        let stdout = Arc::new(Mutex::new(io::stdout()));
        let policy = self.policy.clone();
        let config = self.config.clone();

        // Thread A: server -> client passthrough
        let stdout_a = stdout.clone();
        let t_server_to_client = thread::spawn(move || -> io::Result<()> {
            let mut reader = BufReader::new(child_stdout);
            let mut line = String::new();

            while reader.read_line(&mut line)? > 0 {
                let mut out = stdout_a.lock().unwrap();
                out.write_all(line.as_bytes())?;
                out.flush()?;
                line.clear();
            }
            Ok(())
        });

        // Thread B: client -> server passthrough with Policy Check
        let stdout_b = stdout.clone();
        let t_client_to_server = thread::spawn(move || -> io::Result<()> {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            let mut line = String::new();

            let mut state = PolicyState::default();
            let mut audit_log = AuditLog::new(config.audit_log_path.as_deref());

            while reader.read_line(&mut line)? > 0 {
                // 1. Try Parse as MCP Request
                match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(req) => {
                        // 2. Check Policy
                        match policy.check(&req, &mut state) {
                            PolicyDecision::Allow => {
                                // Log Allow
                                if config.verbose && req.is_tool_call() {
                                    let tool = req
                                        .tool_params()
                                        .map(|p| p.name)
                                        .unwrap_or_else(|| "unknown".to_string());
                                    eprintln!("[assay] ALLOW {}", tool);
                                }

                                if req.is_tool_call() {
                                    let tool = req.tool_params().map(|p| p.name);
                                    audit_log.log(&AuditEvent {
                                        timestamp: chrono::Utc::now().to_rfc3339(),
                                        decision: "allow".to_string(),
                                        tool,
                                        reason: None,
                                        request_id: req.id.clone(),
                                        agentic: None,
                                    });
                                }
                                // forward
                            }
                            PolicyDecision::Deny {
                                tool,
                                reason,
                                contract,
                            } => {
                                // Log Decision
                                let decision_str =
                                    if config.dry_run { "would_deny" } else { "deny" };

                                if config.verbose {
                                    eprintln!(
                                        "[assay] {} {} (reason: {})",
                                        decision_str.to_uppercase(),
                                        tool,
                                        reason
                                    );
                                }

                                audit_log.log(&AuditEvent {
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    decision: decision_str.to_string(),
                                    tool: Some(tool.clone()),
                                    reason: Some(reason.clone()),
                                    request_id: req.id.clone(),
                                    agentic: Some(contract.clone()),
                                });

                                if config.dry_run {
                                    // DRY RUN: Forward anyway
                                    // Fallthrough to forward logic below
                                } else {
                                    // BLOCK: Send error response
                                    let id = req.id.unwrap_or(serde_json::Value::Null);
                                    let response_json = make_deny_response(
                                        id,
                                        "Content blocked by policy",
                                        contract,
                                    );

                                    let mut out = stdout_b.lock().unwrap();
                                    out.write_all(response_json.as_bytes())?;
                                    out.flush()?;

                                    line.clear();
                                    continue; // Skip forwarding
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Passthrough non-request (or parse error)
                        // eprintln!("[assay] DEBUG: Parse failed for line: {}", line.trim());
                    }
                }

                // 3. Forward
                child_stdin.write_all(line.as_bytes())?;
                child_stdin.flush()?;
                line.clear();
            }
            Ok(())
        });

        // Wacht tot client->server eindigt (stdin closed)
        t_client_to_server.join().expect("client->server thread")?;

        // Server->client thread kan nog even lopen; join best-effort
        let _ = t_server_to_client.join();

        // Wacht op child exit
        let status = self.child.wait()?;
        Ok(status.code().unwrap_or(1))
    }
}
