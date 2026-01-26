//! Environment variable filtering for sandbox security.
//!
//! By default, the sandbox scrubs sensitive environment variables to prevent
//! credential leakage to untrusted MCP servers and agents. This module
//! implements the filtering logic described in ADR-001 and Phase 6 Hardening.
//!
//! # Security Model
//!
//! - **Scrub (Default)**: Remove known sensitive patterns (API keys, tokens, secrets)
//! - **Strict**: Only allow safe base variables (PATH, HOME, etc) + explicit allows
//! - **Passthrough**: Danger mode - pass all variables (not recommended)
//!
//! Additionally, "Execution Influence" variables (like LD_PRELOAD) can be stripped
//! independently or as part of Strict mode.

use std::collections::{HashMap, HashSet};

/// Environment filtering mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvMode {
    /// Pattern-based scrub: remove known secrets, pass unknown
    #[default]
    Scrub,
    /// Strict: only SAFE_BASE + explicit allows, scrub everything else
    Strict,
    /// Passthrough: no filtering (danger!)
    Passthrough,
}

/// Result of environment filtering
#[derive(Debug, Clone)]
pub struct EnvFilterResult {
    pub filtered_env: HashMap<String, String>,
    pub passed_count: usize,
    pub scrubbed_keys: Vec<String>,
    pub exec_influence_stripped: Vec<String>,
    pub exec_influence_allowed: Vec<String>, // Explicitly allowed (warn)
}

/// Environment filter configuration
#[derive(Debug, Clone)]
pub struct EnvFilter {
    pub mode: EnvMode,
    pub strip_exec_influence: bool,
    pub enforce_safe_path: bool,
    pub explicit_allow: HashSet<String>,
}

// ============================================================================
// Pattern Definitions
// ============================================================================

/// Safe base patterns that always pass in strict mode
pub const SAFE_BASE_PATTERNS: &[&str] = &[
    // System essentials
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "LOGNAME",
    // Locale & terminal
    "LANG",
    "LC_*",
    "TERM",
    "COLORTERM",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "NO_COLOR",
    "FORCE_COLOR",
    // Temp directories (will be overwritten with scoped path)
    "TMPDIR",
    "TMP",
    "TEMP",
    // XDG directories
    "XDG_*",
    // Rust development
    "RUST_LOG",
    "RUST_BACKTRACE",
    "RUST_LIB_BACKTRACE",
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "RUSTUP_HOME",
    // Editor & tools
    "EDITOR",
    "VISUAL",
    "PAGER",
    "LESS",
    "LESSCHARSET",
    // Timezone
    "TZ",
];

/// Secret patterns to scrub in Scrub mode
pub const SECRET_SCRUB_PATTERNS: &[&str] = &[
    // Generic credential patterns
    "*_TOKEN",
    "*_SECRET",
    "*_KEY",
    "*_PASSWORD",
    "*_CREDENTIAL*",
    "*_API_KEY",
    "*_APIKEY",
    "*_AUTH",
    "*_BEARER",
    // Cloud providers
    "AWS_*",
    "OPENAI_*",
    "ANTHROPIC_*",
    "AZURE_*",
    "GCP_*",
    "GOOGLE_*",
    "DIGITALOCEAN_*",
    "LINODE_*",
    "VULTR_*",
    "CLOUDFLARE_*",
    "HEROKU_*",
    "VERCEL_*",
    "NETLIFY_*",
    "FLY_*",
    // Version control & CI
    "GITHUB_*",
    "GITLAB_*",
    "BITBUCKET_*",
    "CI_*",
    "CIRCLE_*",
    "TRAVIS_*",
    "JENKINS_*",
    "BUILDKITE_*",
    "CODEBUILD_*",
    // Databases & storage
    "DATABASE_URL",
    "*_DATABASE_URL",
    "*_CONNECTION_STRING",
    "*_CONN_STR",
    "REDIS_*",
    "MONGO_*",
    "MYSQL_*",
    "POSTGRES_*",
    "PGPASSWORD",
    "PGUSER",
    // Security tools
    "SSH_*",
    "GPG_*",
    "VAULT_*",
    "KUBECONFIG",
    "DOCKER_*",
    // Package managers
    "NPM_TOKEN",
    "NPM_AUTH_TOKEN",
    "YARN_*",
    "PIP_*",
    "PYPI_*",
    "CARGO_REGISTRY_TOKEN",
    "GEM_*",
    "NUGET_*",
    // Misc
    "*_PRIVATE_KEY",
    "*_SIGNING_KEY",
    "*_ENCRYPTION_KEY",
    "SLACK_*",
    "DISCORD_*",
    "TWILIO_*",
    "SENDGRID_*",
    "STRIPE_*",
    "SENTRY_*",
    "DATADOG_*",
    "NEWRELIC_*",
];

/// Execution-influence variables (code injection risk)
pub const EXEC_INFLUENCE_PATTERNS: &[&str] = &[
    // Dynamic linker (Linux)
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "LD_DEBUG",
    "LD_DEBUG_OUTPUT",
    "LD_PROFILE",
    "LD_PROFILE_OUTPUT",
    "LD_BIND_NOW",
    "LD_BIND_NOT",
    "LD_DYNAMIC_WEAK",
    "LD_HWCAP_MASK",
    "LD_ORIGIN_PATH",
    "LD_ASSUME_KERNEL",
    "LD_POINTER_GUARD",
    "LD_PREFER_MAP_32BIT_EXEC",
    "LD_SHOW_AUXV",
    "LD_USE_LOAD_BIAS",
    "LD_VERBOSE",
    "LD_WARN",
    // Dynamic linker (macOS)
    "DYLD_*",
    // Python
    "PYTHONPATH",
    "PYTHONSTARTUP",
    "PYTHONHOME",
    "PYTHONUSERBASE",
    "PYTHONEXECUTABLE",
    "PYTHONWARNINGS",
    "PYTHONDONTWRITEBYTECODE",
    "PYTHONHASHSEED",
    "PYTHONINSPECT",
    "PYTHONIOENCODING",
    "PYTHONNOUSERSITE",
    "PYTHONOPTIMIZE",
    "PYTHONUNBUFFERED",
    "PYTHONVERBOSE",
    "PYTHONMALLOC",
    "PYTHONCOERCECLOCALE",
    "PYTHONDEVMODE",
    "PYTHONFAULTHANDLER",
    "PYTHONTRACEMALLOC",
    "PYTHONPROFILEIMPORTTIME",
    "PYTHONBREAKPOINT",
    // Node.js
    "NODE_OPTIONS",
    "NODE_PATH",
    "NODE_EXTRA_CA_CERTS",
    "NODE_REDIRECT_WARNINGS",
    "NODE_DEBUG",
    "NODE_DEBUG_NATIVE",
    "NODE_PENDING_DEPRECATION",
    "NODE_PENDING_PIPE_INSTANCES",
    "NODE_PRESERVE_SYMLINKS",
    "NODE_PRESERVE_SYMLINKS_MAIN",
    "NODE_REPL_EXTERNAL_MODULE",
    "NODE_REPL_HISTORY",
    "NODE_TLS_REJECT_UNAUTHORIZED",
    "NODE_V8_COVERAGE",
    // Ruby
    "RUBYOPT",
    "RUBYLIB",
    "RUBYPATH",
    "RUBYSHELL",
    "RUBY_GC_*",
    "RUBY_THREAD_*",
    // Perl
    "PERL5LIB",
    "PERL5OPT",
    "PERLLIB",
    "PERL_MM_OPT",
    "PERL_MB_OPT",
    // Java
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "JAVA_OPTIONS",
    "CLASSPATH",
    "JDK_JAVA_OPTIONS",
    // Rust
    "RUSTC_WRAPPER",
    "RUSTDOC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_ENCODED_RUSTDOCFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_RUSTDOCFLAGS",
    // C/C++ build
    "CC",
    "CXX",
    "CPP",
    "CFLAGS",
    "CXXFLAGS",
    "CPPFLAGS",
    "LDFLAGS",
    "LIBS",
    "AR",
    "AS",
    "LD",
    "NM",
    "OBJCOPY",
    "OBJDUMP",
    "RANLIB",
    "STRIP",
    "PKG_CONFIG",
    "PKG_CONFIG_PATH",
    "PKG_CONFIG_LIBDIR",
    "CMAKE_*",
    "MAKEFLAGS",
    "MFLAGS",
    "MAKELEVEL",
    // Shell behavior
    "BASH_ENV",
    "ENV",
    "ZDOTDIR",
    "CDPATH",
    "GLOBIGNORE",
    "BASH_XTRACEFD",
    "PS4",
    "PROMPT_COMMAND",
    "FIGNORE",
    "HOSTFILE",
    "INPUTRC",
    "MAILPATH",
    "TIMEFORMAT",
    // Git (selective: hooks can execute code)
    "GIT_EXEC_PATH",
    "GIT_TEMPLATE_DIR",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_ASKPASS",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "SSH_ASKPASS",
    "SUDO_ASKPASS",
    "GIT_EXTERNAL_DIFF",
    "GIT_DIFF_OPTS",
    "GIT_PAGER",
    "GIT_EDITOR",
    "GIT_SEQUENCE_EDITOR",
    // Misc execution control
    "SHELL", // Note: this is also in SAFE_BASE, strip_exec wins if conflicting
    "EXECIGNORE",
    "IFS",
];

// ============================================================================
// Implementation
// ============================================================================

impl Default for EnvFilter {
    fn default() -> Self {
        Self {
            mode: EnvMode::Scrub,
            strip_exec_influence: false, // PR6: default off for DX, opt-in
            enforce_safe_path: false,
            explicit_allow: HashSet::new(),
        }
    }
}

impl EnvFilter {
    /// Create a strict mode filter (only safe base + explicit allows)
    pub fn strict() -> Self {
        Self {
            mode: EnvMode::Strict,
            strip_exec_influence: true, // Strict implies strip exec influence
            enforce_safe_path: false,
            explicit_allow: HashSet::new(),
        }
    }

    /// Create a passthrough filter (danger!)
    pub fn passthrough() -> Self {
        Self {
            mode: EnvMode::Passthrough,
            strip_exec_influence: false,
            enforce_safe_path: false,
            explicit_allow: HashSet::new(),
        }
    }

    /// Enable execution-influence stripping
    pub fn with_strip_exec(mut self, strip: bool) -> Self {
        self.strip_exec_influence = strip;
        self
    }

    /// Enable strictly safe PATH
    pub fn with_safe_path(mut self, safe: bool) -> Self {
        self.enforce_safe_path = safe;
        self
    }

    /// Add explicit allow list
    pub fn with_allowed<I, S>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.explicit_allow.extend(vars.into_iter().map(Into::into));
        self
    }

    /// Filter environment variables
    pub fn filter(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        let mut result = match self.mode {
            EnvMode::Passthrough => self.filter_passthrough(env),
            EnvMode::Scrub => self.filter_scrub(env),
            EnvMode::Strict => self.filter_strict(env),
        };

        // Post-process PATH if needed
        if self.enforce_safe_path {
            let safe_default = if cfg!(target_os = "macos") {
                "/usr/bin:/bin:/usr/sbin:/sbin"
            } else {
                "/usr/bin:/bin"
            };
            result
                .filtered_env
                .insert("PATH".to_string(), safe_default.to_string());
        } else if self.mode == EnvMode::Strict {
            // In Strict Default: Sanitized PATH (strip relative/dot paths)
            if let Some(path) = result.filtered_env.get("PATH") {
                let sanitized = std::env::split_paths(path)
                    .filter(|p| {
                        // Keep only absolute paths
                        p.is_absolute()
                        // And verify no components are "." or ".." implicitly handled by canonicalization
                        // but split_paths just splits.
                        // We primarily want to block "./bin" or relative hijacking.
                    })
                    .collect::<Vec<_>>();

                // Join back. If empty, maybe fallback to safe default?
                // For now, join.
                if let Ok(new_path) = std::env::join_paths(sanitized) {
                    if let Ok(s) = new_path.into_string() {
                        result.filtered_env.insert("PATH".to_string(), s);
                    }
                }
            }
        }

        result
    }

    /// Filter from current process environment
    pub fn filter_current(&self) -> EnvFilterResult {
        let env: HashMap<String, String> = std::env::vars().collect();
        self.filter(&env)
    }

    fn filter_passthrough(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        // Passthrough: only strip exec influence if explicitly enabled
        let mut filtered = env.clone();
        let mut exec_stripped = Vec::new();
        let mut exec_allowed = Vec::new();

        if self.strip_exec_influence {
            for key in env.keys() {
                if matches_any_pattern(key, EXEC_INFLUENCE_PATTERNS) {
                    if self.explicit_allow.contains(key) {
                        exec_allowed.push(key.clone());
                    } else {
                        filtered.remove(key);
                        exec_stripped.push(key.clone());
                    }
                }
            }
        }

        exec_stripped.sort();
        exec_allowed.sort();

        EnvFilterResult {
            passed_count: filtered.len(),
            filtered_env: filtered,
            scrubbed_keys: Vec::new(),
            exec_influence_stripped: exec_stripped,
            exec_influence_allowed: exec_allowed,
        }
    }

    fn filter_scrub(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        let mut filtered = HashMap::new();
        let mut scrubbed = Vec::new();
        let mut exec_stripped = Vec::new();
        let mut exec_allowed = Vec::new();

        for (key, value) in env {
            // 1. Check explicit allow first (highest priority)
            if self.explicit_allow.contains(key) {
                // Check if it's an exec-influence var (warn but allow)
                if self.strip_exec_influence && matches_any_pattern(key, EXEC_INFLUENCE_PATTERNS) {
                    exec_allowed.push(key.clone());
                }
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // 2. Check exec-influence (if enabled)
            if self.strip_exec_influence && matches_any_pattern(key, EXEC_INFLUENCE_PATTERNS) {
                exec_stripped.push(key.clone());
                continue;
            }

            // 3. Safe Base always passes (even unlikely conflict)
            if matches_any_pattern(key, SAFE_BASE_PATTERNS) {
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // 4. Check secret patterns (scrub)
            if matches_any_pattern(key, SECRET_SCRUB_PATTERNS) {
                scrubbed.push(key.clone());
                continue;
            }

            // 4. Pass through (unknown vars allowed in Scrub mode)
            filtered.insert(key.clone(), value.clone());
        }

        scrubbed.sort();
        exec_stripped.sort();
        exec_allowed.sort();

        EnvFilterResult {
            passed_count: filtered.len(),
            filtered_env: filtered,
            scrubbed_keys: scrubbed,
            exec_influence_stripped: exec_stripped,
            exec_influence_allowed: exec_allowed,
        }
    }

    fn filter_strict(&self, env: &HashMap<String, String>) -> EnvFilterResult {
        let mut filtered = HashMap::new();
        let mut scrubbed = Vec::new();
        let mut exec_stripped = Vec::new();
        let mut exec_allowed = Vec::new();

        for (key, value) in env {
            // 1. Check explicit allow first (highest priority)
            if self.explicit_allow.contains(key) {
                // Check if it's an exec-influence var (warn but allow)
                if matches_any_pattern(key, EXEC_INFLUENCE_PATTERNS) {
                    exec_allowed.push(key.clone());
                }
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // 2. Exec-influence always stripped in strict (unless explicit allow)
            if matches_any_pattern(key, EXEC_INFLUENCE_PATTERNS) {
                exec_stripped.push(key.clone());
                continue;
            }

            // 3. Only safe base patterns pass
            if matches_any_pattern(key, SAFE_BASE_PATTERNS) {
                filtered.insert(key.clone(), value.clone());
                continue;
            }

            // 4. Everything else is scrubbed
            scrubbed.push(key.clone());
        }

        scrubbed.sort();
        exec_stripped.sort();
        exec_allowed.sort();

        EnvFilterResult {
            passed_count: filtered.len(),
            filtered_env: filtered,
            scrubbed_keys: scrubbed,
            exec_influence_stripped: exec_stripped,
            exec_influence_allowed: exec_allowed,
        }
    }

    /// Format banner line
    pub fn format_banner(&self, result: &EnvFilterResult) -> String {
        let mode_str = match self.mode {
            EnvMode::Passthrough => "⚠ passthrough",
            EnvMode::Scrub => "scrubbed",
            EnvMode::Strict => "strict",
        };

        let mut parts = vec![format!(
            "{} ({} passed, {} removed)",
            mode_str,
            result.passed_count,
            result.scrubbed_keys.len()
        )];

        if !result.exec_influence_stripped.is_empty() {
            parts.push(format!(
                "exec-influence stripped: {}",
                result.exec_influence_stripped.len()
            ));
        }

        if !result.exec_influence_allowed.is_empty() {
            parts.push(format!(
                "⚠ exec-influence ALLOWED: {}",
                result.exec_influence_allowed.join(", ")
            ));
        }

        if self.mode == EnvMode::Passthrough {
            parts.push("DANGER".to_string());
        }

        parts.join(", ")
    }
}

/// Match a key against glob patterns (PREFIX_*, *_SUFFIX, *CONTAINS*, EXACT)
pub fn matches_any_pattern(key: &str, patterns: &[&str]) -> bool {
    for pattern in patterns {
        if matches_pattern(key, pattern) {
            return true;
        }
    }
    false
}

fn matches_pattern(key: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let has_prefix_wildcard = pattern.starts_with('*');
    let has_suffix_wildcard = pattern.ends_with('*');

    match (has_prefix_wildcard, has_suffix_wildcard) {
        (true, true) => {
            // *CONTAINS*
            let inner = &pattern[1..pattern.len() - 1];
            key.contains(inner)
        }
        (true, false) => {
            // *_SUFFIX
            let suffix = &pattern[1..];
            key.ends_with(suffix)
        }
        (false, true) => {
            // PREFIX_*
            let prefix = &pattern[..pattern.len() - 1];
            key.starts_with(prefix)
        }
        (false, false) => {
            // EXACT
            key == pattern
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern("AWS_SECRET", "AWS_*"));
        assert!(matches_pattern("GITHUB_TOKEN", "*_TOKEN"));
        assert!(matches_pattern("MY_SECRET_KEY", "*SECRET*"));
        assert!(matches_pattern("PATH", "PATH"));
        assert!(!matches_pattern("PATH", "PATHX"));
        assert!(!matches_pattern("XPATH", "PATH"));
    }

    #[test]
    fn test_scrub_mode_removes_secrets() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/user"),
            ("AWS_SECRET_ACCESS_KEY", "secret"),
            ("GITHUB_TOKEN", "ghp_xxx"),
            ("CUSTOM_VAR", "value"),
        ]);

        let result = EnvFilter::default().filter(&env);

        assert!(result.filtered_env.contains_key("PATH"));
        assert!(result.filtered_env.contains_key("HOME"));
        assert!(result.filtered_env.contains_key("CUSTOM_VAR")); // Unknown passes in Scrub
        assert!(!result.filtered_env.contains_key("AWS_SECRET_ACCESS_KEY"));
        assert!(!result.filtered_env.contains_key("GITHUB_TOKEN"));
        assert_eq!(result.scrubbed_keys.len(), 2);
    }

    #[test]
    fn test_strict_mode_blocks_unknown() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/user"),
            ("CUSTOM_VAR", "value"),
            ("MY_CONFIG", "config"),
        ]);

        let result = EnvFilter::strict().filter(&env);

        assert!(result.filtered_env.contains_key("PATH"));
        assert!(result.filtered_env.contains_key("HOME"));
        assert!(!result.filtered_env.contains_key("CUSTOM_VAR")); // Blocked in Strict
        assert!(!result.filtered_env.contains_key("MY_CONFIG"));
        assert_eq!(result.scrubbed_keys.len(), 2);
    }

    #[test]
    fn test_strict_with_explicit_allow() {
        let env = make_env(&[("PATH", "/usr/bin"), ("CUSTOM_VAR", "value")]);

        let result = EnvFilter::strict()
            .with_allowed(["CUSTOM_VAR"])
            .filter(&env);

        assert!(result.filtered_env.contains_key("PATH"));
        assert!(result.filtered_env.contains_key("CUSTOM_VAR")); // Explicitly allowed
    }

    #[test]
    fn test_exec_influence_stripped() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("LD_PRELOAD", "/tmp/evil.so"),
            ("PYTHONPATH", "/tmp/evil"),
            ("NODE_OPTIONS", "--require=/tmp/evil.js"),
        ]);

        let result = EnvFilter::default().with_strip_exec(true).filter(&env);

        assert!(result.filtered_env.contains_key("PATH"));
        assert!(!result.filtered_env.contains_key("LD_PRELOAD"));
        assert!(!result.filtered_env.contains_key("PYTHONPATH"));
        assert!(!result.filtered_env.contains_key("NODE_OPTIONS"));
        assert_eq!(result.exec_influence_stripped.len(), 3);
    }

    #[test]
    fn test_exec_influence_allowed_with_warning() {
        let env = make_env(&[("PATH", "/usr/bin"), ("LD_PRELOAD", "/tmp/needed.so")]);

        let result = EnvFilter::default()
            .with_strip_exec(true)
            .with_allowed(["LD_PRELOAD"])
            .filter(&env);

        assert!(result.filtered_env.contains_key("LD_PRELOAD")); // Allowed
        assert_eq!(result.exec_influence_allowed, vec!["LD_PRELOAD"]); // But warned
        assert!(result.exec_influence_stripped.is_empty());
    }

    #[test]
    fn test_passthrough_mode() {
        let env = make_env(&[
            ("PATH", "/usr/bin"),
            ("AWS_SECRET", "secret"),
            ("LD_PRELOAD", "/tmp/lib.so"),
        ]);

        let result = EnvFilter::passthrough().filter(&env);

        assert_eq!(result.filtered_env.len(), 3); // Everything passes
        assert!(result.scrubbed_keys.is_empty());
    }

    #[test]
    fn test_banner_format() {
        let env = make_env(&[("PATH", "/usr/bin"), ("AWS_SECRET", "secret")]);

        let filter = EnvFilter::default();
        let result = filter.filter(&env);
        let banner = filter.format_banner(&result);

        assert!(banner.contains("scrubbed"));
        assert!(banner.contains("1 passed"));
        assert!(banner.contains("1 removed"));
    }
}
