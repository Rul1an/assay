use super::Mutator;
use anyhow::Result;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub struct BitFlip {
    pub count: usize,
    /// Optional seed for deterministic mutations. Uses thread_rng if None.
    pub seed: Option<u64>,
}

impl Mutator for BitFlip {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut corrupted = data.to_vec();

        let flip = |rng: &mut dyn rand::RngCore, buf: &mut Vec<u8>, count: usize| {
            for _ in 0..count {
                if buf.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..buf.len());
                buf[idx] ^= 1 << rng.gen_range(0..8u32);
            }
        };

        if let Some(seed) = self.seed {
            let mut rng = StdRng::seed_from_u64(seed);
            flip(&mut rng, &mut corrupted, self.count);
        } else {
            let mut rng = rand::thread_rng();
            flip(&mut rng, &mut corrupted, self.count);
        }

        Ok(corrupted)
    }
}
