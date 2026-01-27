pub mod bitflip;
pub mod inject;
pub mod truncate;

use anyhow::Result;

pub trait Mutator {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>>;
}
