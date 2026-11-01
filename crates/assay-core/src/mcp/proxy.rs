mod client;
mod decisions;
mod frame;
mod request_receipts;
mod session_stop;
pub use session_stop::ProxyRunReport;
mod server;
mod tools;

#[cfg(all(test, target_os = "linux"))]
mod lifecycle_tests;

use self::client::{run_client_to_server, DeferredOutputs};
use self::server::run_server_to_client;
use super::decision::{DecisionEmitter, DecisionEvent, FileDecisionEmitter, NullDecisionEmitter};
use super::policy::McpPolicy;
use super::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
};

/// Creates a caller's outputs and returns the decision log and request receipts handles among them.
/// See [`McpProxy::run_with_deferred_outputs`].
pub type OutputCommit =
    Box<dyn FnOnce() -> io::Result<(Option<std::fs::File>, Option<std::fs::File>)> + Send>;

/// Emits to the decision log once it exists, and to `before` until then. No decision is made before
/// the commit, since a frame that could lead to one commits first.
struct DeferredDecisionEmitter {
    log: Arc<OnceLock<Arc<FileDecisionEmitter>>>,
    before: Arc<dyn DecisionEmitter>,
}

impl DecisionEmitter for DeferredDecisionEmitter {
    fn emit(&self, event: &DecisionEvent) {
        match self.log.get() {
            Some(log) => log.emit(event),
            None => self.before.emit(event),
        }
    }
}

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
    /// EXPERIMENTAL (unstable): opt-in tool-decision-truth carrier producer. `None` disables it. The
    /// secret HMAC key lives inside [`TdtProducer`], whose manual `Debug` redacts it, so deriving `Debug`
    /// on this config does not leak the key.
    pub tdt_producer: Option<TdtProducer>,
}

/// EXPERIMENTAL (unstable): opt-in producer of `assay.tool_decision_truth.v0` carriers, written
/// append-only as NDJSON (one carrier per line) to a sink kept separate from — never folded into — the
/// decision log. The decision log is operational decision output; a carrier is a content-addressed
/// evidence record with its own HMAC/key semantics and claim ceiling, so the two stay distinct. The
/// producer is evidence-only: it takes no runtime action on the verdict and never enforces or blocks.
///
/// The HMAC key is held in memory only and is never logged or persisted; the manual `Debug` impl redacts
/// it so it cannot leak through a `{:?}` of the surrounding [`ProxyConfig`].
#[derive(Clone)]
pub struct TdtProducer {
    out_path: std::path::PathBuf,
    key: Vec<u8>,
    key_id: String,
}

impl TdtProducer {
    /// Build a producer from a sink path and key material. The caller is responsible for failing closed
    /// (before constructing this) when the producer is enabled but the key/key_id is missing or
    /// malformed; this type does not re-validate beyond what `tool_decision_truth::args_digest` enforces
    /// at digest time.
    pub fn new(out_path: std::path::PathBuf, key: Vec<u8>, key_id: String) -> Self {
        Self {
            out_path,
            key,
            key_id,
        }
    }

    /// Append-only NDJSON sink for minted carriers.
    pub(crate) fn out_path(&self) -> &std::path::Path {
        &self.out_path
    }

    /// HMAC key for `args_digest` (secret; never logged or persisted).
    pub(crate) fn key(&self) -> &[u8] {
        &self.key
    }

    /// Stable key identifier bound into the digest prefix.
    pub(crate) fn key_id(&self) -> &str {
        &self.key_id
    }
}

impl std::fmt::Debug for TdtProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TdtProducer")
            .field("out_path", &self.out_path)
            .field("key", &"<redacted>")
            .field("key_id", &self.key_id)
            .finish()
    }
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
            // Opt-in producer is wired by the caller after validation (env key, fail-closed), never
            // from the raw CLI/config struct.
            tdt_producer: None,
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
    child_status: Option<std::process::ExitStatus>,
    kill_requested: bool,
    cleanup_attempted: bool,
    client_traffic: Arc<session_stop::ClientTraffic>,
}

impl Drop for McpProxy {
    fn drop(&mut self) {
        self.finish_child();
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
            child_status: None,
            kill_requested: false,
            client_traffic: Arc::default(),
            cleanup_attempted: false,
        })
    }

    pub fn run(self) -> io::Result<i32> {
        self.run_with_report(None, None).result
    }

    /// EXPERIMENTAL: retain bounded, caller-asserted request/decision associations.
    /// The caller supplies a new private file separate from all other outputs.
    /// Write failures disable capture without changing policy or forwarding behavior.
    /// This is not execution proof or an admitted incident source.
    pub fn run_with_request_receipts(self, file: std::fs::File) -> io::Result<i32> {
        self.run_with_report(None, Some(file)).result
    }

    /// Run with outputs the caller already reserved. A `decision_log` handle receives the
    /// decision events instead of a file opened from `decision_log_path`, so a caller that
    /// created it exclusively knows the log holds only this run's events. `request_receipts`
    /// behaves as in [`Self::run_with_request_receipts`].
    pub fn run_with_reserved_outputs(
        self,
        decision_log: Option<std::fs::File>,
        request_receipts: Option<std::fs::File>,
    ) -> io::Result<i32> {
        self.run_with_report(decision_log, request_receipts).result
    }

    /// Run with a final observation after bounded child cleanup. Existing run methods
    /// preserve their result; this additive entry point also retains loss/termination data.
    pub fn run_with_report(
        self,
        decision_log: Option<std::fs::File>,
        receipts: Option<std::fs::File>,
    ) -> ProxyRunReport {
        self.run_reporting(decision_log, receipts, None)
    }

    /// Run with outputs that do not exist yet. `commit` runs just before the first client frame
    /// that is not a `server/discover` request is handled, or when the client closes without
    /// sending a frame, and never for a run of only such requests. If it fails the frame is not
    /// handled or forwarded, and the run ends with that error.
    pub fn run_with_deferred_outputs(self, commit: OutputCommit) -> ProxyRunReport {
        self.run_reporting(None, None, Some(commit))
    }

    fn run_reporting(
        mut self,
        decision_log: Option<std::fs::File>,
        receipts: Option<std::fs::File>,
        commit: Option<OutputCommit>,
    ) -> ProxyRunReport {
        let mut emitter = None;
        let committed_log = Arc::new(OnceLock::new());
        let deferred = DeferredOutputs {
            commit,
            decision_log: committed_log.clone(),
        };
        let result = self.run_inner(decision_log, receipts, deferred, &mut emitter);
        self.finish_child();
        ProxyRunReport {
            server_pid: self.child.id(),
            child_status: self.child_status,
            assay_kill_requested: self.kill_requested,
            decision_log: committed_log.get().or(emitter.as_ref()).map(|e| e.status()),
            client_probe_only: self.client_traffic.probe_only(),
            result,
        }
    }

    fn run_inner(
        &mut self,
        decision_log: Option<std::fs::File>,
        receipts: Option<std::fs::File>,
        deferred: DeferredOutputs,
        retained_emitter: &mut Option<Arc<FileDecisionEmitter>>,
    ) -> io::Result<i32> {
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
        *retained_emitter = if let Some(file) = decision_log {
            Some(Arc::new(FileDecisionEmitter::from_file(file)))
        } else if let Some(path) = &config.decision_log_path {
            Some(Arc::new(FileDecisionEmitter::new(path)?))
        } else {
            None
        };
        let decision_emitter: Arc<dyn DecisionEmitter> = match retained_emitter.as_ref() {
            Some(emitter) => emitter.clone(),
            None => Arc::new(NullDecisionEmitter),
        };
        let decision_emitter: Arc<dyn DecisionEmitter> = Arc::new(DeferredDecisionEmitter {
            log: deferred.decision_log.clone(),
            before: decision_emitter,
        });
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
        let client_traffic = self.client_traffic.clone();
        let t_client_to_server = thread::spawn(move || {
            run_client_to_server(
                child_stdin,
                stdout_b,
                policy,
                config,
                emitter_b,
                event_source_b,
                identity_cache_b,
                tool_definition_cache_b,
                receipts,
                client_traffic,
                deferred,
            )
        });

        // Wacht tot client->server eindigt (stdin closed)
        t_client_to_server
            .join()
            .map_err(|_| io::Error::other("client->server thread panicked"))??;

        // Drain normal output, but preserve a read/write failure or panic. Returning
        // here lets Drop terminate/reap the child instead of waiting on a live server
        // after its output reader has already failed.
        t_server_to_client
            .join()
            .map_err(|_| io::Error::other("server->client thread panicked"))??;

        // Wacht op child exit
        let status = self.child.wait()?;
        self.child_status = Some(status);
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
