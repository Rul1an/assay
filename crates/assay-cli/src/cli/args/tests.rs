use super::*;
use clap::CommandFactory;
use clap::Parser;

#[test]
fn cli_debug_assert() {
    Cli::command().debug_assert();
}

#[test]
fn visible_top_level_commands_have_descriptions() {
    let missing: Vec<_> = Cli::command()
        .get_subcommands()
        .filter(|cmd| !cmd.is_hide_set())
        .filter(|cmd| cmd.get_about().is_none())
        .map(|cmd| cmd.get_name().to_string())
        .collect();

    assert!(
        missing.is_empty(),
        "visible top-level commands without descriptions: {}",
        missing.join(", ")
    );
}

#[test]
fn trust_card_command_accepts_canonical_and_legacy_names() {
    let canonical = Cli::try_parse_from([
        "assay",
        "trust-card",
        "generate",
        "bundle.tar.gz",
        "--out-dir",
        "trustcard",
    ])
    .expect("canonical trust-card command should parse");
    assert!(matches!(canonical.cmd, Command::TrustCard(_)));

    let legacy = Cli::try_parse_from([
        "assay",
        "trustcard",
        "generate",
        "bundle.tar.gz",
        "--out-dir",
        "trustcard",
    ])
    .expect("legacy trustcard alias should parse");
    assert!(matches!(legacy.cmd, Command::TrustCard(_)));
}

#[test]
fn mcp_group_accepts_canonical_paths_and_legacy_shims_are_hidden() {
    let visible: Vec<_> = Cli::command()
        .get_subcommands()
        .filter(|cmd| !cmd.is_hide_set())
        .map(|cmd| cmd.get_name().to_string())
        .collect();

    assert!(visible.contains(&"mcp".to_string()));
    assert!(!visible.contains(&"discover".to_string()));
    assert!(!visible.contains(&"kill".to_string()));
    assert!(!visible.contains(&"tool".to_string()));

    let discover = Cli::try_parse_from(["assay", "mcp", "discover", "--format", "json"])
        .expect("canonical mcp discover command should parse");
    assert!(matches!(
        discover.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Discover(_)
        })
    ));

    let kill = Cli::try_parse_from(["assay", "mcp", "kill", "proc-123"])
        .expect("canonical mcp kill command should parse");
    assert!(matches!(
        kill.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Kill(_)
        })
    ));

    let tool = Cli::try_parse_from(["assay", "mcp", "tool", "keygen", "--out", "keys"])
        .expect("canonical mcp tool command should parse");
    assert!(matches!(
        tool.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Tool(_)
        })
    ));

    let legacy_discover = Cli::try_parse_from(["assay", "discover", "--format", "json"])
        .expect("legacy discover shim should parse");
    assert!(matches!(legacy_discover.cmd, Command::Discover(_)));

    let legacy_kill =
        Cli::try_parse_from(["assay", "kill", "proc-123"]).expect("legacy kill shim should parse");
    assert!(matches!(legacy_kill.cmd, Command::Kill(_)));

    let legacy_tool = Cli::try_parse_from(["assay", "tool", "keygen", "--out", "keys"])
        .expect("legacy tool shim should parse");
    assert!(matches!(legacy_tool.cmd, Command::Tool(_)));
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
