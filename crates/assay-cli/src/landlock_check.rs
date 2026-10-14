//! One filesystem grant plan for Landlock preflight and ruleset construction.
//! Landlock is allow-only: overlapping allow and deny trees cannot enforce deny-wins.

use crate::policy::Policy;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum FsAccess {
    Read,
    ReadExecute,
    All,
}

#[derive(Debug, Clone)]
pub struct FsGrant {
    pub path: PathBuf,
    pub access: FsAccess,
}

#[derive(Debug, Clone, Default)]
pub struct LandlockCompatReport {
    /// The complete plan. The backend must not add filesystem grants of its own.
    pub grants: Vec<FsGrant>,
    pub conflicts: Vec<(PathBuf, PathBuf)>,
}

impl LandlockCompatReport {
    pub fn is_compatible(&self) -> bool {
        self.conflicts.is_empty()
    }
}

const SYSTEM_READ_FILES: &[&str] = &[
    "/etc/hosts",
    "/etc/resolv.conf",
    "/etc/localtime",
    "/etc/timezone",
    "/etc/ld.so.cache",
];
const SYSTEM_RUNTIME_DIRS: &[&str] = &["/usr", "/lib", "/lib64", "/bin", "/sbin"];

/// `${CWD}` and relative policy paths retain the launch directory as their base.
/// Only the implicit working-directory grant follows the selected child directory.
pub fn check_compatibility(
    policy: &Policy,
    launch_cwd: &Path,
    child_cwd: &Path,
    tmp: &Path,
) -> io::Result<LandlockCompatReport> {
    let home = std::env::var("HOME").ok();
    let user = std::env::var("USER").ok();
    let expand = |raw: &str| -> io::Result<PathBuf> {
        let mut expanded = raw.to_owned();
        if let Some(home) = &home {
            if expanded == "~" || expanded.starts_with("~/") {
                expanded = expanded.replacen('~', home, 1);
            }
            expanded = expanded.replace("${HOME}", home);
        }
        if expanded.contains("${CWD}") {
            let cwd = launch_cwd.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "${CWD} requires a UTF-8 launch directory",
                )
            })?;
            expanded = expanded.replace("${CWD}", cwd);
        }
        if let Some(user) = &user {
            expanded = expanded.replace("${USER}", user);
        }
        Ok(launch_cwd.join(expanded))
    };

    let mut grants = Vec::new();
    let mut add_grant = |path: &Path, access| -> io::Result<()> {
        match std::fs::canonicalize(launch_cwd.join(path)) {
            Ok(path) => grants.push(FsGrant { path, access }),
            // Missing paths grant nothing, including paths with a missing component before `..`.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok(())
    };
    add_grant(child_cwd, FsAccess::ReadExecute)?;
    add_grant(tmp, FsAccess::All)?;
    for raw in SYSTEM_READ_FILES {
        add_grant(Path::new(raw), FsAccess::Read)?;
    }
    for raw in SYSTEM_RUNTIME_DIRS {
        add_grant(Path::new(raw), FsAccess::ReadExecute)?;
    }
    for raw in &policy.fs.allow {
        add_grant(&expand(raw)?, FsAccess::All)?;
    }
    let mut conflicts = Vec::new();
    for raw in &policy.fs.deny {
        let deny = resolve_path(&expand(raw)?, launch_cwd)?;
        for grant in &grants {
            if deny.starts_with(&grant.path) || grant.path.starts_with(&deny) {
                conflicts.push((grant.path.clone(), deny.clone()));
            }
        }
    }
    conflicts.sort();
    conflicts.dedup();
    Ok(LandlockCompatReport { grants, conflicts })
}

/// Resolve aliases, including an existing ancestor of a not-yet-created deny leaf.
/// Permission errors and symlink loops must not silently turn into lexical paths.
fn resolve_path(path: &Path, base: &Path) -> io::Result<PathBuf> {
    let absolute = base.join(path);
    match std::fs::canonicalize(&absolute) {
        Ok(path) => return Ok(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                resolved.pop();
            }
            _ => resolved.push(component),
        }
        match std::fs::canonicalize(&resolved) {
            Ok(path) => resolved = path,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(&resolved).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unresolved symlink in filesystem deny: {}",
                            resolved.display()
                        ),
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_is_not_treated_as_a_missing_literal_deny() {
        let root = std::env::temp_dir().join(format!("assay-602-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(root.join("future"), &alias).unwrap();
        let result = resolve_path(&alias, &root);
        std::fs::remove_file(&alias).unwrap();
        std::fs::remove_dir(&root).unwrap();
        assert!(
            result.is_err(),
            "unresolved alias must not become a lexical deny"
        );
    }

    #[test]
    fn missing_allow_component_does_not_grant_its_lexical_target() {
        let cwd = std::env::current_dir().unwrap();
        let mut policy = Policy::default();
        policy
            .fs
            .allow
            .push("assay-nonexistent-602/../Cargo.toml".into());
        let report = check_compatibility(&policy, &cwd, &cwd, Path::new("/tmp/scoped")).unwrap();
        assert!(!report
            .grants
            .iter()
            .any(|g| g.path == cwd.join("Cargo.toml")));
    }

    #[test]
    fn implicit_runtime_grants_cannot_override_denies() {
        let mut policy = Policy::default();
        policy.fs.deny.push("/usr".into());
        assert!(!check_compatibility(
            &policy,
            Path::new("/opt/launch"),
            Path::new("/home/child"),
            Path::new("/tmp/scoped")
        )
        .unwrap()
        .is_compatible());
    }

    #[test]
    fn deny_ancestor_of_an_allow_is_a_conflict() {
        let mut policy = Policy::default();
        policy.fs.allow.push("/etc/hosts".into());
        policy.fs.deny.push("/etc".into());
        assert!(!check_compatibility(
            &policy,
            Path::new("/opt/launch"),
            Path::new("/home/child"),
            Path::new("/tmp/scoped")
        )
        .unwrap()
        .is_compatible());
    }

    #[test]
    fn home_variable_and_absolute_denies_agree() {
        let home = std::env::var("HOME").expect("HOME");
        let mut policy = Policy::default();
        policy.fs.allow.push("${HOME}".into());
        policy
            .fs
            .deny
            .push(format!("{home}/assay-test-allow/secret"));
        assert!(!check_compatibility(
            &policy,
            Path::new("/opt/launch"),
            Path::new("/opt/child"),
            Path::new("/tmp/scoped")
        )
        .unwrap()
        .is_compatible());
    }

    #[test]
    fn test_conflict_detection() {
        let mut policy = Policy::default();
        // Allow /tmp
        policy.fs.allow.push("/tmp".to_string());
        // Deny /tmp/secrets
        policy.fs.deny.push("/tmp/secrets".to_string());

        let cwd = Path::new("/tmp"); // Implicit allow
        let tmp = Path::new("/tmp/assay-sandbox"); // Implicit allow

        let report = check_compatibility(&policy, Path::new("/opt/launch"), cwd, tmp).unwrap();

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

        let report = check_compatibility(&policy, Path::new("/opt/launch"), cwd, tmp).unwrap();
        assert!(report.is_compatible());
    }
}
