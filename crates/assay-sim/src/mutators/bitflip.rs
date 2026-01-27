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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seeded_bitflip_is_deterministic() {
        let data = vec![0u8; 100];
        let bf = BitFlip {
            count: 5,
            seed: Some(42),
        };

        let result1 = bf.mutate(&data).unwrap();
        let result2 = bf.mutate(&data).unwrap();

        assert_eq!(
            result1, result2,
            "same seed must produce identical mutations"
        );
        assert_ne!(result1, data, "mutations should change the data");
    }

    #[test]
    fn test_different_seeds_produce_different_mutations() {
        let data = vec![0u8; 100];
        let bf1 = BitFlip {
            count: 5,
            seed: Some(42),
        };
        let bf2 = BitFlip {
            count: 5,
            seed: Some(99),
        };

        let result1 = bf1.mutate(&data).unwrap();
        let result2 = bf2.mutate(&data).unwrap();

        assert_ne!(
            result1, result2,
            "different seeds should produce different mutations"
        );
    }

    #[test]
    fn test_seeded_bitflip_exact_bytes() {
        // Verify the exact same bytes are flipped across runs
        let data = vec![0xAA; 50];
        let bf = BitFlip {
            count: 3,
            seed: Some(12345),
        };

        let result1 = bf.mutate(&data).unwrap();
        let result2 = bf.mutate(&data).unwrap();

        // Find which positions differ
        let diffs1: Vec<usize> = result1
            .iter()
            .enumerate()
            .filter(|(i, b)| **b != data[*i])
            .map(|(i, _)| i)
            .collect();
        let diffs2: Vec<usize> = result2
            .iter()
            .enumerate()
            .filter(|(i, b)| **b != data[*i])
            .map(|(i, _)| i)
            .collect();

        assert_eq!(diffs1, diffs2, "exact same positions must be flipped");
        assert!(!diffs1.is_empty(), "at least one bit should be flipped");
    }
}
