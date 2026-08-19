use clap::Args;

/// Ask for one node of the CLI contract. Empty path is the top level.
#[derive(Args, Debug, Clone)]
pub struct DescribeArgs {
    /// Command path to descend into. Omit to list the top-level surface.
    pub path: Vec<String>,
}
