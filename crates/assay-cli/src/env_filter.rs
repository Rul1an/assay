//! Environment variable filtering for sandbox security.
//!
//! By default, the sandbox scrubs sensitive environment variables to prevent
//! credential leakage to untrusted MCP servers and agents. This module
//! implements the filtering logic described in ADR-001.
//!
//! # Security Model
//!
//! - **Default**: Scrub known sensitive patterns (API keys, tokens, secrets)
//! - **Allow**: Explicitly allow specific variables through the filter
//! - **Passthrough**: Danger mode - pass all variables (not recommended)

use std::collections::{HashMap, HashSet};

/// Default patterns to scrub from environment.
/// These cover common cloud providers, AI/ML APIs, and generic secret patterns.
const DEFAULT_SCRUB_PATTERNS: &[&str] = &[
    // Cloud providers
    "AWS_*",
    "AZURE_*",
    "GCP_*",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_CLOUD_*",
    "DIGITALOCEAN_*",
    "LINODE_*",
    "VULTR_*",
    "CLOUDFLARE_*",
    // AI/ML APIs
    "OPENAI_*",
    "ANTHROPIC_*",
    "HF_*",
    "HUGGING*",
    "REPLICATE_*",
    "COHERE_*",
    "MISTRAL_*",
    "GROQ_*",
    "TOGETHER_*",
    "FIREWORKS_*",
    "DEEPSEEK_*",
    "PERPLEXITY_*",
    // Dev tools & CI
    "GITHUB_*",
    "GITLAB_*",
    "BITBUCKET_*",
    "CODECOV_*",
    "CIRCLECI_*",
    "TRAVIS_*",
    "NPM_*",
    "CARGO_REGISTRY_*",
    "PYPI_*",
    "DOCKER_*",
    // Generic secret patterns (suffix)
    "*_TOKEN",
    "*_SECRET",
    "*_KEY",
    "*_PASSWORD",
    "*_CREDENTIAL",
    "*_CREDENTIALS",
    "*_API_KEY",
    "*_AUTH",
    "*_PRIVATE_KEY",
    "*_ACCESS_KEY",
    "*_SECRET_KEY",
    // Database & connection strings
    "*_DATABASE_URL",
    "*_CONNECTION_STRING",
    "*_DSN",
    "DATABASE_URL",
    "REDIS_URL",
    "MONGODB_*",
    "POSTGRES_*",
    "MYSQL_*",
    // SOTA/Agent upgrades
    "SSH_*",
    "GPG_*",
    "SOPS_*",
    "VAULT_*",
    "KUBECONFIG",
    "KUBE_*",
    "1PASSWORD_*",
    "OP_*",
    "PASS_*",
    "*_SESSION",
    "*_COOKIE",
    "*_BEARER",
    "*_JWT",
];

/// Variables that are always safe to pass through.
/// These are essential for basic process operation.
const SAFE_BASE_PATTERNS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_*",
    "TERM",
    "TMPDIR",
    "TMP",
    "TEMP",
    "XDG_*",
    "PWD",
    "OLDPWD",
    "SHLVL",
    "HOSTNAME",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "COLORTERM",
    "COLUMNS",
    "LINES",
    // Rust/Cargo build vars (not secrets)
    "CARGO",
    "CARGO_HOME",
    "CARGO_MANIFEST_DIR",
    "CARGO_PKG_*",
    "RUSTUP_HOME",
    "RUST_BACKTRACE",
    "RUST_LOG",
    // Common dev tools (non-secret)
    "EDITOR",
    "VISUAL",
    "PAGER",
    "LESS",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "NO_COLOR",
    "FORCE_COLOR",
];

/// Filtering mode for environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvMode {
    /// Scrub sensitive patterns (default, secure)
    #[default]
    Scrub,
    /// Pass all environment variables (danger!)
    Passthrough,
}

/// Environment filter configuration.
#[derive(Debug, Clone)]
pub struct EnvFilter {
    mode: EnvMode,
    /// Additional variables to explicitly allow (overrides scrub patterns)
    explicit_allow: HashSet<String>,
}

/// Result of filtering environment variables.
#[derive(Debug, Clone)]
pub struct EnvFilterResult {
    /// The filtered environment to pass to the child process
    pub filtered_env: HashMap<String, String>,
    /// Keys that were scrubbed (for reporting, values NOT included)
    pub scrubbed_keys: Vec<String>,
    /// Total number of variables passed through
    pub passed_count: usize,
}

impl EnvFilter {
    /// Create a new filter with default scrub mode.
    pub fn default_scrub() -> Self {
        Self {
            mode: EnvMode::Scrub,
            explicit_allow: HashSet::new(),
        }
    }

    /// Create a filter that passes all variables (danger!).
    pub fn passthrough() -> Self {
        Self {
            mode: EnvMode::Passthrough,
            explicit_allow: HashSet::new(),
        }
    }

    /// Add explicit allow list (overrides scrub patterns).
    pub fn with_allowed(mut self, keys: &[String]) -> Self {
        self.explicit_allow.extend(keys.iter().cloned());
        self
    }

    /// Filter environment variables according to the configured mode.
    pub fn filter(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        match self.mode {
            EnvMode::Passthrough => EnvFilterResult {
                filtered_env: env.clone(),
                scrubbed_keys: vec![],
                passed_count: env.len(),
            },
            EnvMode::Scrub => self.filter_scrub(env),
        }
    }

    /// Filter from current process environment.
    pub fn filter_current(&self) -> EnvFilterResult {
        let env: HashMap<String, String> = std::env::vars().collect();
        self.filter(&env)
    }

    fn filter_scrub(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        let mut filtered = HashMap::new();
        let mut scrubbed = Vec::new();

        for (key, value) in env {
            // Check explicit allow first (highest priority)
            if self.explicit_allow.contains(key) {
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // Check if it matches scrub patterns (Security Priority)
            // Done BEFORE safe base to prevent leaks like "LC_SECRET" (matches LC_* and *_SECRET)
            if matches_any_pattern(key, DEFAULT_SCRUB_PATTERNS) {
                scrubbed.push(key.clone());
                continue;
            }

            // Check if it's a safe base variable
            if matches_any_pattern(key, SAFE_BASE_PATTERNS) {
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // Default: allow through (unknown variables are passed)
            filtered.insert(key.clone(), value.clone());
        }

        // Sort scrubbed keys for deterministic output
        scrubbed.sort();

        EnvFilterResult {
            passed_count: filtered.len(),
            filtered_env: filtered,
            scrubbed_keys: scrubbed,
        }
    }
}

/// Check if a key matches any of the given glob patterns.
fn matches_any_pattern(key: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| matches_glob(key, pattern))
}

/// Simple wildcard matching supporting only `*` wildcard.
///
/// Patterns:
/// - `PREFIX_*` matches keys starting with `PREFIX_`
/// - `*_SUFFIX` matches keys ending with `_SUFFIX`
/// - `*CONTAINS*` matches keys containing substring (multi-wildcard matches disjoint parts)
/// - `EXACT` matches exactly `EXACT`
fn matches_glob(key: &str, pattern: &str) -> bool {
    // Handle patterns with wildcards
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();

        match parts.len() {
            // Single `*` splits into 2 parts
            2 => {
                let (prefix, suffix) = (parts[0], parts[1]);

                if prefix.is_empty() && suffix.is_empty() {
                    // Pattern is just "*" - matches everything
                    true
                } else if prefix.is_empty() {
                    // Pattern is "*SUFFIX" - matches if key ends with suffix
                    key.ends_with(suffix)
                } else if suffix.is_empty() {
                    // Pattern is "PREFIX*" - matches if key starts with prefix
                    key.starts_with(prefix)
                } else {
                    // Pattern is "PREFIX*SUFFIX" - matches if key starts with prefix and ends with suffix
                    key.starts_with(prefix)
                        && key.ends_with(suffix)
                        && key.len() >= prefix.len() + suffix.len()
                }
            }
            // No `*` - exact match (handled below)
            1 => key == pattern,
            // Multiple `*` - not supported, fall back to contains check
            _ => {
                // For patterns like "*FOO*", check if all non-empty parts are contained
                parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .all(|p| key.contains(p))
            }
        }
    } else {
        // No wildcard - exact match
        key == pattern
    }
}

/// Format the env filter result for banner display.
pub fn format_banner(result: &EnvFilterResult, mode: EnvMode) -> String {
    match mode {
        EnvMode::Passthrough => {
            format!("⚠ passthrough ({} vars, DANGER)", result.passed_count)
        }
        EnvMode::Scrub => {
            if result.scrubbed_keys.is_empty() {
                format!("clean ({} vars)", result.passed_count)
            } else {
                format!(
                    "scrubbed ({} passed, {} removed)",
                    result.passed_count,
                    result.scrubbed_keys.len()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ========== Glob matching tests ==========

    #[test]
    fn test_glob_exact_match() {
        assert!(matches_glob("PATH", "PATH"));
        assert!(!matches_glob("PATH", "HOME"));
        assert!(!matches_glob("MY_PATH", "PATH"));
    }

    #[test]
    fn test_glob_prefix_wildcard() {
        // PREFIX_* pattern
        assert!(matches_glob("AWS_SECRET_ACCESS_KEY", "AWS_*"));
        assert!(matches_glob("AWS_", "AWS_*")); // Edge: just prefix
        assert!(!matches_glob("BAWS_FOO", "AWS_*")); // No match
        assert!(!matches_glob("aws_secret", "AWS_*")); // Case sensitive
    }

    #[test]
    fn test_glob_suffix_wildcard() {
        // *_SUFFIX pattern
        assert!(matches_glob("GITHUB_TOKEN", "*_TOKEN"));
        assert!(matches_glob("MY_API_TOKEN", "*_TOKEN"));
        assert!(matches_glob("_TOKEN", "*_TOKEN")); // Edge: just suffix
        assert!(!matches_glob("TOKEN", "*_TOKEN")); // Must have underscore
        assert!(!matches_glob("TOKEN_VALUE", "*_TOKEN")); // Wrong position
    }

    #[test]
    fn test_glob_contains_wildcard() {
        // *CONTAINS* pattern (multiple wildcards)
        assert!(matches_glob("HUGGINGFACE_TOKEN", "HUGGING*"));
        assert!(matches_glob("HUGGING", "HUGGING*"));
    }

    #[test]
    fn test_glob_prefix_suffix_wildcard() {
        // PREFIX*SUFFIX pattern
        assert!(matches_glob("CARGO_PKG_NAME", "CARGO_PKG_*"));
        assert!(matches_glob("LC_ALL", "LC_*"));
    }

    // ========== Filter tests ==========

    #[test]
    fn test_default_scrubs_secrets() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/user"),
            ("OPENAI_API_KEY", "sk-secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
            ("MY_APP_TOKEN", "token123"),
            ("GITHUB_TOKEN", "ghp_xxx"),
            ("NORMAL_VAR", "value"),
        ]);

        let result = EnvFilter::default_scrub().filter(&env);

        // Safe vars should pass
        assert!(result.filtered_env.contains_key("PATH"));
        assert!(result.filtered_env.contains_key("HOME"));
        assert!(result.filtered_env.contains_key("NORMAL_VAR"));

        // Secrets should be scrubbed
        assert!(!result.filtered_env.contains_key("OPENAI_API_KEY"));
        assert!(!result.filtered_env.contains_key("AWS_SECRET_ACCESS_KEY"));
        assert!(!result.filtered_env.contains_key("MY_APP_TOKEN"));
        assert!(!result.filtered_env.contains_key("GITHUB_TOKEN"));

        // Verify scrubbed list
        assert_eq!(result.scrubbed_keys.len(), 4);
        assert!(result.scrubbed_keys.contains(&"OPENAI_API_KEY".to_string()));
        assert!(result
            .scrubbed_keys
            .contains(&"AWS_SECRET_ACCESS_KEY".to_string()));
        assert!(result.scrubbed_keys.contains(&"MY_APP_TOKEN".to_string()));
        assert!(result.scrubbed_keys.contains(&"GITHUB_TOKEN".to_string()));
    }

    #[test]
    fn test_explicit_allow_overrides_scrub() {
        let env = make_env(&[
            ("OPENAI_API_KEY", "sk-secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
        ]);

        let result = EnvFilter::default_scrub()
            .with_allowed(&["OPENAI_API_KEY".to_string()])
            .filter(&env);

        // Explicitly allowed should pass through
        assert!(result.filtered_env.contains_key("OPENAI_API_KEY"));
        assert_eq!(
            result.filtered_env.get("OPENAI_API_KEY"),
            Some(&"sk-secret".to_string())
        );

        // Non-allowed secret still scrubbed
        assert!(!result.filtered_env.contains_key("AWS_SECRET_ACCESS_KEY"));
        assert!(result
            .scrubbed_keys
            .contains(&"AWS_SECRET_ACCESS_KEY".to_string()));
    }

    #[test]
    fn test_passthrough_allows_all() {
        let env = make_env(&[
            ("OPENAI_API_KEY", "sk-secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
            ("PATH", "/usr/bin"),
        ]);

        let result = EnvFilter::passthrough().filter(&env);

        assert_eq!(result.filtered_env.len(), 3);
        assert!(result.scrubbed_keys.is_empty());
        assert!(result.filtered_env.contains_key("OPENAI_API_KEY"));
        assert!(result.filtered_env.contains_key("AWS_SECRET_ACCESS_KEY"));
    }

    #[test]
    fn test_safe_base_always_passes() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/user"),
            ("USER", "testuser"),
            ("SHELL", "/bin/bash"),
            ("LANG", "en_US.UTF-8"),
            ("LC_ALL", "C"),
            ("TERM", "xterm-256color"),
            ("XDG_CONFIG_HOME", "/home/user/.config"),
            ("RUST_LOG", "debug"),
            ("RUST_BACKTRACE", "1"),
        ]);

        let result = EnvFilter::default_scrub().filter(&env);

        // All safe base vars should pass
        for key in env.keys() {
            assert!(
                result.filtered_env.contains_key(key),
                "Safe base var {} should pass through",
                key
            );
        }
        assert!(result.scrubbed_keys.is_empty());
    }

    #[test]
    fn test_unknown_vars_pass_through() {
        let env = make_env(&[
            ("MY_CUSTOM_VAR", "value"),
            ("APP_DEBUG", "true"),
            ("SOME_SETTING", "123"),
        ]);

        let result = EnvFilter::default_scrub().filter(&env);

        // Unknown vars that don't match scrub patterns should pass
        assert_eq!(result.filtered_env.len(), 3);
        assert!(result.scrubbed_keys.is_empty());
    }

    #[test]
    fn test_database_url_scrubbed() {
        let env = make_env(&[
            ("DATABASE_URL", "postgres://user:pass@host/db"),
            ("REDIS_URL", "redis://localhost"),
            ("MY_DATABASE_URL", "mysql://..."),
        ]);

        let result = EnvFilter::default_scrub().filter(&env);

        assert!(!result.filtered_env.contains_key("DATABASE_URL"));
        assert!(!result.filtered_env.contains_key("REDIS_URL"));
        assert!(!result.filtered_env.contains_key("MY_DATABASE_URL"));
        assert_eq!(result.scrubbed_keys.len(), 3);
    }

    #[test]
    fn test_multiple_allow() {
        let env = make_env(&[
            ("OPENAI_API_KEY", "sk-1"),
            ("ANTHROPIC_API_KEY", "sk-2"),
            ("GITHUB_TOKEN", "ghp-3"),
        ]);

        let result = EnvFilter::default_scrub()
            .with_allowed(&[
                "OPENAI_API_KEY".to_string(),
                "ANTHROPIC_API_KEY".to_string(),
            ])
            .filter(&env);

        assert!(result.filtered_env.contains_key("OPENAI_API_KEY"));
        assert!(result.filtered_env.contains_key("ANTHROPIC_API_KEY"));
        assert!(!result.filtered_env.contains_key("GITHUB_TOKEN"));
    }

    // ========== Banner formatting tests ==========

    #[test]
    fn test_banner_scrubbed() {
        let result = EnvFilterResult {
            filtered_env: HashMap::new(),
            scrubbed_keys: vec!["FOO".to_string(), "BAR".to_string()],
            passed_count: 10,
        };

        let banner = format_banner(&result, EnvMode::Scrub);
        assert!(banner.contains("scrubbed"));
        assert!(banner.contains("10 passed"));
        assert!(banner.contains("2 removed"));
    }

    #[test]
    fn test_banner_passthrough() {
        let result = EnvFilterResult {
            filtered_env: HashMap::new(),
            scrubbed_keys: vec![],
            passed_count: 25,
        };

        let banner = format_banner(&result, EnvMode::Passthrough);
        assert!(banner.contains("passthrough"));
        assert!(banner.contains("25 vars"));
        assert!(banner.contains("DANGER"));
    }

    #[test]
    fn test_scrub_priority_over_safe_base() {
        // "LC_*" is in SAFE_BASE_PATTERNS
        // "*_SECRET" is in DEFAULT_SCRUB_PATTERNS
        // "LC_SECRET" matches BOTH. It MUST BE SCRUBBED.
        let env = make_env(&[
            ("LC_ALL", "C"),          // Safe only
            ("LC_SECRET", "leak me"), // Safe + Scrub -> Scrub
            ("MY_SECRET", "secret"),  // Scrub only
        ]);

        let result = EnvFilter::default_scrub().filter(&env);

        assert!(result.filtered_env.contains_key("LC_ALL"));
        assert!(!result.filtered_env.contains_key("MY_SECRET"));

        // Critical check: Priority
        assert!(
            !result.filtered_env.contains_key("LC_SECRET"),
            "LC_SECRET should be scrubbed even if LC_* is safe"
        );
        assert!(result.scrubbed_keys.contains(&"LC_SECRET".to_string()));
    }

    #[test]
    fn test_banner_clean() {
        let result = EnvFilterResult {
            filtered_env: HashMap::new(),
            scrubbed_keys: vec![],
            passed_count: 15,
        };

        let banner = format_banner(&result, EnvMode::Scrub);
        assert!(banner.contains("clean"));
        assert!(banner.contains("15 vars"));
    }
}
