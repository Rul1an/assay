use crate::cli::args::SimSoakArgs;
use crate::exit_codes::EXIT_CONFIG_ERROR;
use anyhow::Result;

pub fn run(args: SimSoakArgs) -> Result<i32> {
    if args.time_budget == 0 {
        eprintln!("Config error: --time-budget must be > 0");
        std::process::exit(EXIT_CONFIG_ERROR);
    }
    if args.iterations == 0 {
        eprintln!("Config error: --iterations must be > 0");
        std::process::exit(EXIT_CONFIG_ERROR);
    }

    eprintln!("sim soak: not implemented yet (ADR-025 I1 Step2 PR-B2)");
    std::process::exit(EXIT_CONFIG_ERROR);
}
