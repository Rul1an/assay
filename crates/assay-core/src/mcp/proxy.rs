mod client;
mod decisions;
mod server;
mod tools;

use self::client::run_client_to_server;
use self::server::run_server_to_client;
use super::decision::{DecisionEmitter, FileDecisionEmitter, NullDecisionEmitter};
use super::policy::McpPolicy;
use super::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io,
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
        let child_stdin = self.child.stdin.take().expect("child stdin");
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
        let config_b = config.clone();
        let emitter_b = decision_emitter.clone();
        let event_source_b = event_source.clone();
        let t_client_to_server = thread::spawn(move || {
            run_client_to_server(
                child_stdin,
                stdout_b,
                policy,
                config_b,
                emitter_b,
                event_source_b,
                identity_cache_b,
                tool_definition_cache_b,
            )
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
