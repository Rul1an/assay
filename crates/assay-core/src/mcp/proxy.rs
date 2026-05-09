use super::audit::{AuditEvent, AuditLog};
mod decisions;
mod server;
mod tools;

use self::decisions::{emit_decision, extract_tool_call_id, handle_allow, map_policy_code};
use self::server::run_server_to_client;
use super::decision::{
    reason_codes, Decision, DecisionEmitter, FileDecisionEmitter, NullDecisionEmitter,
};
use super::jsonrpc::JsonRpcRequest;
use super::policy::{make_deny_response, McpPolicy, PolicyDecision, PolicyState};
use super::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

/// Validated proxy configuration.
///
/// Use `ProxyConfig::try_from_raw()` to create from CLI/config input.
#[derive(Clone, Debug)]
pub struct ProxyConfig {
    pub dry_run: bool,
    pub verbose: bool,
    /// NDJSON log for mandate lifecycle events (audit trail)
    pub audit_log_path: Option<std::path::PathBuf>,
    pub server_id: String,
    /// NDJSON log for tool decision events (high volume)
    pub decision_log_path: Option<std::path::PathBuf>,
    /// CloudEvents source URI (validated, required when logging enabled)
    pub event_source: Option<String>,
}

/// Raw config as provided by CLI/config files before validation.
#[derive(Clone, Debug, Default)]
pub struct ProxyConfigRaw {
    pub dry_run: bool,
    pub verbose: bool,
    pub audit_log_path: Option<std::path::PathBuf>,
    pub server_id: String,
    pub decision_log_path: Option<std::path::PathBuf>,
    pub event_source: Option<String>,
}

impl ProxyConfig {
    /// Create validated config from raw input.
    ///
    /// Fails if:
    /// - Logging is enabled but event_source is missing
    /// - event_source is not a valid absolute URI (scheme://...)
    pub fn try_from_raw(raw: ProxyConfigRaw) -> anyhow::Result<Self> {
        let logging_enabled = raw.audit_log_path.is_some() || raw.decision_log_path.is_some();

        let event_source = raw
            .event_source
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if logging_enabled && event_source.is_none() {
            anyhow::bail!(
                "event_source is required when logging is enabled (e.g. --event-source assay://org/app)"
            );
        }

        if let Some(ref src) = event_source {
            validate_event_source(src)?;
        }

        Ok(ProxyConfig {
            dry_run: raw.dry_run,
            verbose: raw.verbose,
            audit_log_path: raw.audit_log_path,
            server_id: raw.server_id,
            decision_log_path: raw.decision_log_path,
            event_source,
        })
    }
}

/// Validate event_source URI (must be absolute with scheme://).
fn validate_event_source(s: &str) -> anyhow::Result<()> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("event_source must be absolute URI with scheme (e.g. assay://org/app)");
    }
    if s.chars().any(|c| c.is_whitespace()) {
        anyhow::bail!("event_source must not contain whitespace");
    }

    // Require scheme://...
    let Some(pos) = s.find("://") else {
        anyhow::bail!("event_source must be absolute URI with scheme (e.g. assay://org/app)");
    };
    if pos == 0 {
        anyhow::bail!("event_source must have scheme before :// (e.g. assay://org/app)");
    }

    // Validate scheme charset (RFC 3986: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ))
    let scheme = &s[..pos];
    let mut chars = scheme.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => anyhow::bail!("event_source URI scheme must start with a letter"),
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
        anyhow::bail!("event_source URI scheme contains invalid characters");
    }

    Ok(())
}

pub struct McpProxy {
    child: Child,
    policy: McpPolicy,
    config: ProxyConfig,
    /// Cache of tool identities discovered during tools/list
    identity_cache: Arc<Mutex<HashMap<String, super::identity::ToolIdentity>>>,
    /// Cache of bounded tool-definition bindings discovered during tools/list
    tool_definition_cache: Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>,
}

impl Drop for McpProxy {
    fn drop(&mut self) {
        // Best-effort cleanup
        let _ = self.child.kill();
    }
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
            identity_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_definition_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn run(mut self) -> io::Result<i32> {
        let mut child_stdin = self.child.stdin.take().expect("child stdin");
        let child_stdout = self.child.stdout.take().expect("child stdout");

        let stdout = Arc::new(Mutex::new(io::stdout()));
        let policy = self.policy.clone();
        let config = self.config.clone();
        let identity_cache_a = self.identity_cache.clone();
        let identity_cache_b = self.identity_cache.clone();
        let tool_definition_cache_a = self.tool_definition_cache.clone();
        let tool_definition_cache_b = self.tool_definition_cache.clone();

        // Initialize decision emitter (I1: always emit decision)
        let decision_emitter: Arc<dyn DecisionEmitter> =
            if let Some(path) = &config.decision_log_path {
                Arc::new(FileDecisionEmitter::new(path)?)
            } else {
                Arc::new(NullDecisionEmitter)
            };
        let event_source = config
            .event_source
            .clone()
            .unwrap_or_else(|| format!("assay://{}", config.server_id));

        // Thread A: server -> client passthrough
        let stdout_a = stdout.clone();
        let server_id_a = config.server_id.clone();
        let t_server_to_client = thread::spawn(move || {
            run_server_to_client(
                child_stdout,
                stdout_a,
                server_id_a,
                identity_cache_a,
                tool_definition_cache_a,
            )
        });

        // Thread B: client -> server passthrough with Policy Check
        let stdout_b = stdout.clone();
        let emitter_b = decision_emitter.clone();
        let event_source_b = event_source.clone();
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
                        // 2. Check Policy with Identity (Phase 9)
                        let (runtime_id, tool_definition_binding) = if req.is_tool_call() {
                            let name = req.tool_params().map(|p| p.name).unwrap_or_default();
                            let runtime_id = {
                                let cache = identity_cache_b.lock().unwrap();
                                cache.get(&name).cloned()
                            };
                            let tool_definition_binding = {
                                let cache = tool_definition_cache_b.lock().unwrap();
                                cache.get(&name).cloned()
                            };
                            (runtime_id, tool_definition_binding)
                        } else {
                            (None, None)
                        };

                        let tool_name = req.tool_params().map(|p| p.name).unwrap_or_default();
                        let tool_call_id = extract_tool_call_id(&req);

                        let policy_eval = policy.evaluate_with_metadata(
                            &tool_name,
                            &req.tool_params()
                                .map(|p| p.arguments)
                                .unwrap_or(serde_json::Value::Null),
                            &mut state,
                            runtime_id.as_ref(),
                        );

                        match policy_eval.decision {
                            PolicyDecision::Allow => {
                                handle_allow(&req, &mut audit_log, config.verbose);
                                // Emit decision event (I1: always emit)
                                if req.is_tool_call() {
                                    emit_decision(
                                        &emitter_b,
                                        &event_source_b,
                                        &tool_call_id,
                                        &tool_name,
                                        Decision::Allow,
                                        reason_codes::P_POLICY_ALLOW,
                                        None,
                                        req.id.clone(),
                                        &policy_eval.metadata,
                                        tool_definition_binding.as_ref(),
                                    );
                                }
                            }
                            PolicyDecision::AllowWithWarning { tool, code, reason } => {
                                // Log warning about allowing a tool invocation with issues
                                if config.verbose {
                                    eprintln!(
                                        "[assay] WARNING: Allowing tool '{}' with warning (code: {}, reason: {}).",
                                        tool,
                                        code,
                                        reason
                                    );
                                }
                                audit_log.log(&AuditEvent {
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    decision: "allow_with_warning".to_string(),
                                    tool: Some(tool.clone()),
                                    reason: Some(reason.clone()),
                                    request_id: req.id.clone(),
                                    agentic: None,
                                });
                                // Emit decision event (I1: always emit)
                                emit_decision(
                                    &emitter_b,
                                    &event_source_b,
                                    &tool_call_id,
                                    &tool,
                                    Decision::Allow,
                                    &code,
                                    Some(reason),
                                    req.id.clone(),
                                    &policy_eval.metadata,
                                    tool_definition_binding.as_ref(),
                                );
                                // Then proceed as a normal allow
                                handle_allow(&req, &mut audit_log, false);
                                // false = don't double log ALLOW
                            }
                            PolicyDecision::Deny {
                                tool,
                                code,
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

                                // Emit decision event (I1: always emit)
                                let reason_code = map_policy_code(&code);
                                emit_decision(
                                    &emitter_b,
                                    &event_source_b,
                                    &tool_call_id,
                                    &tool,
                                    if config.dry_run {
                                        Decision::Allow
                                    } else {
                                        Decision::Deny
                                    },
                                    &reason_code,
                                    Some(reason),
                                    req.id.clone(),
                                    &policy_eval.metadata,
                                    tool_definition_binding.as_ref(),
                                );

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

                                    let mut out = stdout_b
                                        .lock()
                                        .map_err(|e| io::Error::other(e.to_string()))?;
                                    out.write_all(response_json.as_bytes())?;
                                    out.flush()?;

                                    line.clear();
                                    continue; // Skip forwarding
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Hardening: Suspicious Unparsable JSON
                        let trimmed = line.trim();
                        if trimmed.starts_with('{')
                            && (trimmed.contains("\"method\"")
                                || trimmed.contains("\"params\"")
                                || trimmed.contains("\"tool\""))
                        {
                            eprintln!("[assay] WARNING: Suspicious unparsable JSON, forwarding anyway (potential bypass attempt?): {:.60}...", trimmed);
                        }
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
        t_client_to_server
            .join()
            .map_err(|_| io::Error::other("client->server thread panicked"))??;

        // Server->client thread kan nog even lopen; join best-effort
        let _ = t_server_to_client.join();

        // Wacht op child exit
        let status = self.child.wait()?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_source_accepts_assay_uri() {
        validate_event_source("assay://myorg/myapp").unwrap();
    }

    #[test]
    fn event_source_accepts_https_uri() {
        validate_event_source("https://example.com/agent").unwrap();
    }

    #[test]
    fn event_source_rejects_empty() {
        assert!(validate_event_source("").is_err());
        assert!(validate_event_source("   ").is_err());
    }

    #[test]
    fn event_source_rejects_whitespace() {
        assert!(validate_event_source("assay://myorg/my app").is_err());
        assert!(validate_event_source("assay://myorg/\tmyapp").is_err());
    }

    #[test]
    fn event_source_rejects_missing_scheme() {
        assert!(validate_event_source("myorg/myapp").is_err());
        assert!(validate_event_source("://myorg/myapp").is_err());
    }

    #[test]
    fn event_source_rejects_did_and_urn() {
        // We require scheme:// not just scheme:
        assert!(validate_event_source("did:example:123").is_err());
        assert!(validate_event_source("urn:example:foo").is_err());
    }

    #[test]
    fn event_source_rejects_scheme_starting_with_non_letter() {
        assert!(validate_event_source("1assay://myorg/myapp").is_err());
        assert!(validate_event_source("-assay://myorg/myapp").is_err());
    }

    #[test]
    fn event_source_rejects_scheme_with_invalid_chars() {
        assert!(validate_event_source("as_say://myorg/myapp").is_err());
        assert!(validate_event_source("as@say://myorg/myapp").is_err());
    }

    #[test]
    fn config_requires_event_source_when_logging_enabled() {
        let raw = ProxyConfigRaw {
            dry_run: false,
            verbose: false,
            audit_log_path: None,
            decision_log_path: Some(std::path::PathBuf::from("decisions.ndjson")),
            event_source: None,
            server_id: "srv".to_string(),
        };

        let err = ProxyConfig::try_from_raw(raw).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("event_source is required"));
    }

    #[test]
    fn config_allows_no_event_source_when_logging_disabled() {
        let raw = ProxyConfigRaw {
            dry_run: false,
            verbose: false,
            audit_log_path: None,
            decision_log_path: None,
            event_source: None,
            server_id: "srv".to_string(),
        };

        ProxyConfig::try_from_raw(raw).unwrap();
    }

    #[test]
    fn config_accepts_valid_event_source() {
        let raw = ProxyConfigRaw {
            dry_run: false,
            verbose: false,
            audit_log_path: None,
            decision_log_path: Some(std::path::PathBuf::from("decisions.ndjson")),
            event_source: Some("assay://myorg/myapp".to_string()),
            server_id: "srv".to_string(),
        };

        let cfg = ProxyConfig::try_from_raw(raw).unwrap();
        assert_eq!(cfg.event_source.as_deref(), Some("assay://myorg/myapp"));
    }

    #[test]
    fn config_rejects_invalid_event_source_uri() {
        let raw = ProxyConfigRaw {
            dry_run: false,
            verbose: false,
            audit_log_path: None,
            decision_log_path: Some(std::path::PathBuf::from("decisions.ndjson")),
            event_source: Some("not a uri".to_string()),
            server_id: "srv".to_string(),
        };

        assert!(ProxyConfig::try_from_raw(raw).is_err());
    }
}
