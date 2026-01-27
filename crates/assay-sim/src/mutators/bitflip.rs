use super::Mutator;
use anyhow::Result;
use rand::Rng;

pub struct BitFlip {
    pub count: usize,
}

impl Mutator for BitFlip {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut corrupted = data.to_vec();
        let mut rng = rand::thread_rng();

        for _ in 0..self.count {
            if corrupted.is_empty() { break; }
            let idx = rng.gen_range(0..corrupted.len());
            corrupted[idx] ^= 1 << rng.gen_range(0..8);
        }

        Ok(corrupted)
    }
}
