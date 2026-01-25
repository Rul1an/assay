//! Landlock compatibility check for policy enforcement.
//!
//! Landlock (as of ABI v3) is an allow-only system. It cannot strictly enforce "deny" rules
//! that are nested within an "allowed" directory tree.
//!
//! This module detects such conflicts to ensure we don't claim "Containment" when the
//! policy semantics are not actually enforced.

use crate::policy::Policy;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LandlockCompatReport {
    /// Roots that should be allowed in Landlock ruleset
    pub allowed_roots: Vec<PathBuf>,
    /// Conflicts detected (deny path inside allow root)
    pub conflicts: Vec<(PathBuf, PathBuf)>, // (allow_root, deny_path)
}

impl LandlockCompatReport {
    pub fn is_compatible(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// Compute compatibility of the policy with Landlock's allow-only model.
///
/// Checks if any effective deny paths are contained within allowed paths.
pub fn check_compatibility(policy: &Policy, cwd: &Path, tmp: &Path) -> LandlockCompatReport {
    let mut allowed_roots = Vec::new();
    let mut deny_paths: Vec<PathBuf> = Vec::new();

    // 1. Collect and Normalize Allowed Roots
    // --------------------------------------

    // Explicit allows from policy
    for path_str in &policy.fs.allow {
        // Expand and normalize
        if let Some(path) = resolve_path(path_str, cwd) {
            allowed_roots.push(path);
        }
    }

    // Implicit allows (contextual)
    // NOTE: CWD is often implicitly allowed in sandbox.rs logic.
    if let Some(p) = resolve_path_buf(cwd, cwd) {
        allowed_roots.push(p);
    }
    if let Some(p) = resolve_path_buf(tmp, cwd) {
        allowed_roots.push(p);
    }

    // 2. Collect and Normalize Deny Paths
    // -----------------------------------
    for path_str in &policy.fs.deny {
        if let Some(path) = resolve_path(path_str, cwd) {
            deny_paths.push(path);
        }
    }

    // 3. Detect Conflicts (Deny inside Allow)
    // ---------------------------------------
    let mut conflicts = Vec::new();

    for deny in &deny_paths {
        for allow in &allowed_roots {
            // Check if deny is inside allow (or equal).
            if is_subpath_or_equal(allow, deny) {
                // Conflict! Landlock will allow 'allow', ignoring 'deny'.
                conflicts.push((allow.clone(), deny.clone()));
            }
        }
    }

    // De-duplicate conflicts
    conflicts.sort();
    conflicts.dedup();

    LandlockCompatReport {
        allowed_roots,
        conflicts,
    }
}

/// Resolve a path: expand variables (simple), canonicalize if exists, normalize if not.
/// handles partial existence (e.g. /tmp/secrets where /tmp exists but secrets doesn't).
fn resolve_path(raw: &str, cwd: &Path) -> Option<PathBuf> {
    let path_str = raw.replace("${CWD}", &cwd.to_string_lossy());
    let path_buf = if let Some(stripped) = path_str.strip_prefix("~") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        if stripped.is_empty() {
            PathBuf::from(home)
        } else if stripped.starts_with(std::path::MAIN_SEPARATOR) {
            PathBuf::from(home).join(stripped.trim_start_matches(std::path::MAIN_SEPARATOR))
        } else {
            // ~user case not supported yet, treat as relative literal
            PathBuf::from(&path_str)
        }
    } else {
        PathBuf::from(&path_str)
    };

    let path = if path_buf.is_relative() {
        cwd.join(path_buf)
    } else {
        path_buf
    };

    // Try to canonicalize the full path
    if let Ok(p) = std::fs::canonicalize(&path) {
        return Some(p);
    }

    // Fallback: Resolve longest existing ancestor
    let mut current = path.clone();
    let mut components_to_append = Vec::new();

    // Pop components until we find an existing path or run out
    while !current.exists() && current.parent().is_some() {
        if let Some(file_name) = current.file_name() {
            components_to_append.push(file_name.to_os_string());
            current.pop();
        } else {
            break;
        }
    }

    // If we found an existing ancestor, canonicalize it
    let base = if current.exists() {
        std::fs::canonicalize(&current).ok().unwrap_or(current)
    } else {
        current
    };

    // Re-assemble
    let mut final_path = base;
    for component in components_to_append.into_iter().rev() {
        final_path.push(component);
    }

    // Finally normalize lexically to handle any remaining .. or . in the appended part?
    // (canonicalize handles base, appended part is raw)
    // Actually appended part came from pop(), so it shouldn't contain .. unless raw input had it and we popped it?
    // Using simple push is safer. But let's run lexical normalize pass at end just in case.
    normalize_path_lexially(&final_path)
}

fn resolve_path_buf(p: &Path, cwd: &Path) -> Option<PathBuf> {
    resolve_path(&p.to_string_lossy(), cwd)
}

/// Best-effort lexical normalization (handles .. and .)
fn normalize_path_lexially(path: &Path) -> Option<PathBuf> {
    use std::path::{Component, PathBuf};
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(component),
        }
    }
    Some(clean)
}

/// Check if path `child` is inside `parent` OR equal to it.
fn is_subpath_or_equal(parent: &Path, child: &Path) -> bool {
    child.starts_with(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    #[test]
    fn test_conflict_detection() {
        let mut policy = Policy::default();
        // Allow /tmp
        policy.fs.allow.push("/tmp".to_string());
        // Deny /tmp/secrets
        policy.fs.deny.push("/tmp/secrets".to_string());

        let cwd = Path::new("/tmp"); // Implicit allow
        let tmp = Path::new("/tmp/assay-sandbox"); // Implicit allow

        let report = check_compatibility(&policy, cwd, tmp);

        assert!(!report.is_compatible());
        assert!(!report.conflicts.is_empty());

        // We expect conflict between Allow("/tmp") and Deny("/tmp/secrets")
        let has_conflict = report
            .conflicts
            .iter()
            .any(|(a, d)| a.ends_with("tmp") && d.ends_with("secrets"));
        assert!(
            has_conflict,
            "Should detect conflict between /tmp and /tmp/secrets"
        );
    }

    #[test]
    fn test_no_conflict_disjoint() {
        let mut policy = Policy::default();
        policy.fs.allow.push("/usr".to_string());
        policy.fs.deny.push("/etc/shadow".to_string());

        let cwd = Path::new("/home/user");
        let tmp = Path::new("/tmp/sandbox");

        let report = check_compatibility(&policy, cwd, tmp);
        assert!(report.is_compatible());
    }
}
