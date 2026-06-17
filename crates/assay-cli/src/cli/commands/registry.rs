//! `assay registry` dispatch. Narrow in this slice: only `supply-chain-conformance`.

use crate::cli::args::{RegistryArgs, RegistrySub};

pub async fn run(args: RegistryArgs) -> anyhow::Result<i32> {
    match args.sub {
        RegistrySub::SupplyChainConformance(a) => super::supply_chain_conformance::run(a).await,
    }
}
