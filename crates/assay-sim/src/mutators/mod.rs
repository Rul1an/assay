pub mod bitflip;
pub mod truncate;
pub mod inject;

use anyhow::Result;

pub trait Mutator {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>>;
}
