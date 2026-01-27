use anyhow::Result;
use std::path::{Path, PathBuf};

pub enum CorpusCategory {
    Valid,
    Invalid,
    Crash,
}

pub struct Corpus {
    pub root: PathBuf,
}

impl Corpus {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load_valid(&self) -> Result<Vec<PathBuf>> {
        // Placeholder: walkdir self.root/valid
        Ok(Vec::new())
    }

    pub fn add(&self, _path: &Path, _category: CorpusCategory, _reason: &str) -> Result<()> {
        // Copy to root/category/hash
        Ok(())
    }
}
