use super::matcher::matches_any_pattern;
use super::patterns::{EXEC_INFLUENCE_PATTERNS, SAFE_BASE_PATTERNS, SECRET_SCRUB_PATTERNS};
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
