use super::*;
use clap::CommandFactory;
use clap::Parser;

#[test]
fn cli_debug_assert() {
    Cli::command().debug_assert();
}

#[cfg(feature = "sim")]
#[test]
fn sim_soak_parses_with_defaults() {
    let cli = Cli::try_parse_from([
        "assay", "sim", "soak", "--target", "bundle", "--report", "out.json",
    ])
    .expect("parse should succeed");

    match cli.cmd {
        Command::Sim(sim) => match sim.cmd {
            SimSub::Soak(args) => {
                assert_eq!(args.iterations, 20);
                assert_eq!(args.time_budget, 60);
                assert_eq!(args.seed, None);
                assert_eq!(args.target, "bundle");
            }
            _ => panic!("expected SimSub::Soak"),
        },
        _ => panic!("expected Command::Sim"),
    }
}

#[cfg(feature = "sim")]
#[test]
fn sim_soak_parses_explicit_values() {
    let cli = Cli::try_parse_from([
        "assay",
        "sim",
        "soak",
        "--iterations",
        "5",
        "--seed",
        "42",
        "--target",
        "scenario-a",
        "--report",
        "out.json",
        "--time-budget",
        "120",
    ])
    .expect("parse should succeed");

    match cli.cmd {
        Command::Sim(sim) => match sim.cmd {
            SimSub::Soak(args) => {
                assert_eq!(args.iterations, 5);
                assert_eq!(args.seed, Some(42));
                assert_eq!(args.target, "scenario-a");
                assert_eq!(args.time_budget, 120);
            }
            _ => panic!("expected SimSub::Soak"),
        },
        _ => panic!("expected Command::Sim"),
    }
}
