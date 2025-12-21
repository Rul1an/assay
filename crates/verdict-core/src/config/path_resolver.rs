use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct PathResolver {
    base_dir: PathBuf,
}

impl PathResolver {
    pub fn new(config_path: &Path) -> Self {
        let base_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Self { base_dir }
    }

    pub fn resolve_opt_str(&self, p: &mut Option<String>) {
        let Some(s) = p.as_mut() else { return };
        if s.trim().is_empty() {
            return;
        }

        let pb = PathBuf::from(&*s);
        if pb.is_absolute() {
            return;
        }

        let joined = self.join_clean(&pb);
        *s = joined.to_string_lossy().to_string();
    }

    pub fn resolve_str(&self, s: &mut String) {
        if s.trim().is_empty() {
            return;
        }
        let pb = PathBuf::from(&*s);
        if pb.is_absolute() {
            return;
        }

        let joined = self.join_clean(&pb);
        *s = joined.to_string_lossy().to_string();
    }

    fn join_clean(&self, rel: &Path) -> PathBuf {
        let joined = self.base_dir.join(rel);

        let mut out = PathBuf::new();
        for c in joined.components() {
            use std::path::Component::*;
            match c {
                CurDir => {}
                ParentDir => {
                    out.pop();
                }
                RootDir | Prefix(_) | Normal(_) => out.push(c.as_os_str()),
            }
        }
        out
    }
}
