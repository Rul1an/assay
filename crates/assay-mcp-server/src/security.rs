use crate::tools::ToolError;
use std::path::{Component, Path, PathBuf};

/// Securely resolves a user-provided path against the policy root.
///
/// Strategy:
/// 1. Lexical checks: Prevent `..` escaping root, absolute paths, and root prefixes.
/// 2. Canonicalization: If file exists, ensure the canonical path is within the canonical root (handles symlinks).
pub fn resolve_policy_path(root_canon: &Path, user_path: &str) -> Result<PathBuf, ToolError> {
    // Reject empty
    if user_path.trim().is_empty() {
        return Err(ToolError::new("E_INVALID_REQUEST", "policy path is empty"));
    }

    let up = Path::new(user_path);

    // Reject absolute paths / Windows prefixes
    if up.is_absolute() {
        return Err(ToolError::new(
            "E_PERMISSION_DENIED",
            "absolute paths are not allowed",
        ));
    }
    for c in up.components() {
        if matches!(c, Component::Prefix(_) | Component::RootDir) {
            return Err(ToolError::new(
                "E_PERMISSION_DENIED",
                "path prefixes/root are not allowed",
            ));
        }
    }

    // Lexical normalization: root + components, but never allow popping above root
    let mut out = PathBuf::from(root_canon);
    let root_len = out.components().count();

    for c in up.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(seg) => out.push(seg),
            Component::ParentDir => {
                // Prevent escaping root via ..
                if out.components().count() <= root_len {
                    return Err(ToolError::new(
                        "E_PERMISSION_DENIED",
                        "path escapes policy_root",
                    ));
                }
                out.pop();
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::new(
                    "E_PERMISSION_DENIED",
                    "absolute path not allowed",
                ));
            }
        }
    }

    // If file exists, canonicalize to detect symlink escapes
    if out.exists() {
        let canon = std::fs::canonicalize(&out)
            .map_err(|e| ToolError::new("E_POLICY_READ", &format!("canonicalize failed: {e}")))?;

        if !canon.starts_with(root_canon) {
            return Err(ToolError::new(
                "E_PERMISSION_DENIED",
                "policy resolves outside policy_root",
            ));
        }
        return Ok(canon);
    }

    // If it doesn't exist, it's safe to return the lexical path (it’s still jailed lexically)
    Ok(out)
}
