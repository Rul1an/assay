use std::path::{Component, Path, PathBuf};

/// Result of path generalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralizedPath {
    pub raw: PathBuf,
    /// The tokenized/rendered string for the policy (e.g. "${ASSAY_TMP}/foo")
    pub rendered: String,
    pub risk: RiskTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskTag {
    Low,
    Broad,
    HomeScope,
    TmpScope,
    Redacted,
}

/// Tokenize and harden a path for policy generation.
///
/// This performs:
/// 1. Normalization (removing ./ and .. components where possible).
/// 2. Scope matching (CWD, HOME, TMP).
/// 3. Redaction of unsafe/non-unicode paths.
pub fn generalize_path(
    raw: &Path,
    cwd: &Path,
    home: Option<&Path>,
    assay_tmp: Option<&Path>,
) -> GeneralizedPath {
    // 0. Redaction check for non-unicode
    let raw_str = match raw.to_str() {
        Some(s) => s,
        None => return redacted(raw, "non-unicode"),
    };

    // 0. Length check (very long paths might be fuzzing/junk)
    if raw_str.len() > 1024 {
        return redacted(raw, "length > 1024");
    }

    // 1. Normalization (best effort)
    let normalized = normalize_path(raw);
    let norm_cwd = normalize_path(cwd);
    let norm_home = home.map(normalize_path);
    let norm_tmp = assay_tmp.map(normalize_path);

    // 2. Scope Matching

    // A) Assay Scoped Tmp
    if let Some(tmp) = norm_tmp {
        if let Ok(rel) = normalized.strip_prefix(&tmp) {
            let s = format!("${{ASSAY_TMP}}/{}", rel.display());
            return GeneralizedPath {
                raw: raw.to_path_buf(),
                rendered: s,
                risk: RiskTag::TmpScope,
            };
        }
    }

    // B) XDG / User Tmp (heuristic, if we knew XDG_RUNTIME_DIR)
    // For now, let's just do CWD/HOME which are most critical.

    // C) CWD Relative
    if let Ok(rel) = normalized.strip_prefix(&norm_cwd) {
        let s = format!("./{}", rel.display());
        return GeneralizedPath {
            raw: raw.to_path_buf(),
            rendered: s,
            risk: RiskTag::Low,
        };
    }

    // D) Home Relative
    if let Some(h) = norm_home {
        if let Ok(rel) = normalized.strip_prefix(&h) {
            let s = format!("~/{}", rel.display());
            return GeneralizedPath {
                raw: raw.to_path_buf(),
                rendered: s,
                risk: RiskTag::HomeScope,
            };
        }
    }

    // E) Absolute Fallback
    GeneralizedPath {
        raw: raw.to_path_buf(),
        rendered: normalized.display().to_string(),
        risk: RiskTag::Broad,
    }
}

fn redacted(raw: &Path, reason: &str) -> GeneralizedPath {
    GeneralizedPath {
        raw: raw.to_path_buf(),
        rendered: format!(
            "${{REDACTED_PATH_TYPE_{}}}",
            reason.to_uppercase().replace(" ", "_")
        ),
        risk: RiskTag::Redacted,
    }
}

/// Normalize path components (remove . and ..) without touching FS.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop if we are not at root.
                // For absolute paths, first component is RootDir.
                // For relative paths, out might be empty.
                if let Some(last) = out.components().next_back() {
                    if last != Component::RootDir {
                        out.pop();
                    }
                }
            }
            c => out.push(c),
        }
    }
    // If empty after pop, it implies "current dir" concept or root?
    // If it was absolute, it kept root. If relative and empty, return ".".
    if out.as_os_str().is_empty() {
        if path.is_absolute() {
            // Should not happen for absolute paths unless root was popped?
            // But root is a component.
        } else {
            out.push(".");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generalize_scopes() {
        let cwd = Path::new("/app");
        let home = Path::new("/home/user");
        let tmp = Path::new("/tmp/assay-123");

        // CWD
        let p = generalize_path(Path::new("/app/src/main.rs"), cwd, Some(home), Some(tmp));
        assert_eq!(p.rendered, "./src/main.rs");
        assert_eq!(p.risk, RiskTag::Low);

        // Home
        let p = generalize_path(Path::new("/home/user/.config"), cwd, Some(home), Some(tmp));
        assert_eq!(p.rendered, "~/.config");

        // Tmp
        let p = generalize_path(Path::new("/tmp/assay-123/sock"), cwd, Some(home), Some(tmp));
        assert_eq!(p.rendered, "${ASSAY_TMP}/sock");

        // Absolute (Outside)
        let p = generalize_path(Path::new("/usr/bin/ls"), cwd, Some(home), Some(tmp));
        assert_eq!(p.rendered, "/usr/bin/ls");
    }

    #[test]
    fn test_normalization() {
        let cwd = Path::new("/app");
        let p = generalize_path(Path::new("/app/./foo/../bar"), cwd, None, None);
        assert_eq!(p.rendered, "./bar");
    }

    #[test]
    fn test_path_traversal_protection() {
        let cwd = Path::new("/app");
        // Should NOT be allowed to pop past root /
        let p = generalize_path(Path::new("/app/../../etc/passwd"), cwd, None, None);
        assert_eq!(p.rendered, "/etc/passwd");

        // Relative traversal should stay at "." if it pops too much
        let p = normalize_path(Path::new("foo/../../bar"));
        assert_eq!(p, PathBuf::from("bar")); // foo/.. -> "" then .. -> stays at "" then bar -> bar

        let p = normalize_path(Path::new("../../../secret"));
        assert_eq!(p, PathBuf::from("secret"));
    }
}
