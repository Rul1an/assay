use super::Mutator;
use anyhow::Result;

pub struct Truncate {
    pub at: usize,
}

impl Mutator for Truncate {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut corrupted = data.to_vec();
        if self.at < corrupted.len() {
            corrupted.truncate(self.at);
        }
        Ok(corrupted)
    }
}
