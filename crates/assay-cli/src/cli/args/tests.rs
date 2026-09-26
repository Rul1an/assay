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
fn mcp_group_accepts_canonical_paths_and_legacy_shims_are_removed() {
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

    let preflight = Cli::try_parse_from(["assay", "mcp", "preflight"])
        .expect("canonical mcp preflight command should parse");
    assert!(matches!(
        preflight.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Preflight(_)
        })
    ));

    // The legacy top-level shims were retired; only the canonical `assay mcp ...` paths remain.
    assert!(Cli::try_parse_from(["assay", "discover", "--format", "json"]).is_err());
    assert!(Cli::try_parse_from(["assay", "kill", "proc-123"]).is_err());
    assert!(Cli::try_parse_from(["assay", "tool", "keygen", "--out", "keys"]).is_err());
}

#[test]
fn mcp_preflight_defaults_to_dot_root_and_terminal_or_json() {
    let defaults =
        Cli::try_parse_from(["assay", "mcp", "preflight"]).expect("bare preflight should parse");
    match defaults.cmd {
        Command::Mcp(McpArgs {
            cmd: McpSub::Preflight(args),
        }) => {
            assert_eq!(args.policy_root.as_os_str(), ".");
            assert_eq!(args.format, PreflightFormat::Terminal);
        }
        _ => panic!("expected mcp preflight"),
    }

    let terminal = Cli::try_parse_from([
        "assay",
        "mcp",
        "preflight",
        "--policy-root",
        ".",
        "--format",
        "terminal",
    ])
    .expect("--format terminal should parse");
    assert!(matches!(
        terminal.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Preflight(PreflightArgs {
                format: PreflightFormat::Terminal,
                ..
            })
        })
    ));

    let json = Cli::try_parse_from(["assay", "mcp", "preflight", "--format", "json"])
        .expect("--format json should parse");
    assert!(matches!(
        json.cmd,
        Command::Mcp(McpArgs {
            cmd: McpSub::Preflight(PreflightArgs {
                format: PreflightFormat::Json,
                ..
            })
        })
    ));

    assert!(
        Cli::try_parse_from(["assay", "mcp", "preflight", "--format", "text"]).is_err(),
        "preflight format is terminal|json, not the global text|json pair"
    );
}

#[test]
fn policy_group_accepts_authoring_paths_and_legacy_shims_are_removed() {
    let visible: Vec<_> = Cli::command()
        .get_subcommands()
        .filter(|cmd| !cmd.is_hide_set())
        .map(|cmd| cmd.get_name().to_string())
        .collect();

    assert!(visible.contains(&"policy".to_string()));
    assert!(!visible.contains(&"generate".to_string()));
    assert!(!visible.contains(&"record".to_string()));

    let generate = Cli::try_parse_from([
        "assay",
        "policy",
        "generate",
        "--input",
        "trace.jsonl",
        "--dry-run",
    ])
    .expect("canonical policy generate command should parse");
    assert!(matches!(
        generate.cmd,
        Command::Policy(PolicyArgs {
            cmd: PolicyCommand::Generate(_)
        })
    ));

    let record = Cli::try_parse_from(["assay", "policy", "record", "--", "echo", "hello"])
        .expect("canonical policy record command should parse");
    assert!(matches!(
        record.cmd,
        Command::Policy(PolicyArgs {
            cmd: PolicyCommand::Record(_)
        })
    ));

    // The legacy top-level shims were retired; only the canonical `assay policy ...` paths remain.
    assert!(
        Cli::try_parse_from(["assay", "generate", "--input", "trace.jsonl", "--dry-run"]).is_err()
    );
    assert!(Cli::try_parse_from(["assay", "record", "--", "echo", "hello"]).is_err());
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

/// Every argument whose name ends in `format` advertises the values it accepts.
///
/// `tests/format_value_parser.rs` checks the same property against a hand-kept table, and that is
/// exactly how `doctor --format` escaped: it was a bare `String` with `// text|json` beside it,
/// `--format totally-invalid` printed the text report and exited 0, and every test there passed
/// because the table never named it. A list beside the thing it describes drifts silently, and in
/// the dangerous direction — the argument nobody listed is the one nobody checked.
///
/// So this derives the set from clap instead of listing it. It lives here rather than beside its
/// sibling because `assay-cli` has no library target, so only an in-crate test can walk the real
/// command tree. Adding a `--format` to a new command fails here until it carries a `value_enum`.
#[test]
fn every_format_argument_advertises_its_accepted_values() {
    use clap::CommandFactory;

    fn walk(cmd: &clap::Command, path: &[String], untyped: &mut Vec<String>) {
        for arg in cmd.get_arguments() {
            let Some(long) = arg.get_long() else { continue };
            if !long.ends_with("format") {
                continue;
            }
            if arg.get_possible_values().is_empty() {
                untyped.push(format!("assay {} --{long}", path.join(" ")));
            }
        }
        for sub in cmd.get_subcommands() {
            let mut child = path.to_vec();
            child.push(sub.get_name().to_string());
            walk(sub, &child, untyped);
        }
    }

    let cli = super::Cli::command();
    let mut untyped = Vec::new();
    walk(&cli, &[], &mut untyped);

    // A walk that finds nothing would pass silently, so prove it reached the arguments first.
    let mut total = 0usize;
    fn count(cmd: &clap::Command, total: &mut usize) {
        *total += cmd
            .get_arguments()
            .filter(|a| a.get_long().is_some_and(|l| l.ends_with("format")))
            .count();
        for sub in cmd.get_subcommands() {
            count(sub, total);
        }
    }
    count(&cli, &mut total);
    assert!(
        total > 5,
        "the walk found only {total} format arguments; it is not reaching them"
    );

    assert!(
        untyped.is_empty(),
        "these format arguments accept any string, so a typo selects a fallback silently:\n  {}",
        untyped.join("\n  ")
    );
}

#[test]
fn top_level_failure_funnel_owns_supported_machine_output_commands() {
    let policy_json = Cli::try_parse_from([
        "assay",
        "policy",
        "validate",
        "--input",
        "policy.yaml",
        "--format",
        "json",
    ])
    .expect("policy validate JSON parses");
    assert_eq!(policy_json.machine_output_verify_enabled(), Some(true));

    let policy_text =
        Cli::try_parse_from(["assay", "policy", "validate", "--input", "policy.yaml"])
            .expect("policy validate text parses");
    assert_eq!(policy_text.machine_output_verify_enabled(), None);

    let evidence_json = Cli::try_parse_from([
        "assay",
        "evidence",
        "show",
        "bundle.tar.gz",
        "--format",
        "json",
    ])
    .expect("evidence show JSON parses");
    assert_eq!(evidence_json.machine_output_verify_enabled(), Some(true));

    let evidence_json_unverified = Cli::try_parse_from([
        "assay",
        "evidence",
        "show",
        "bundle.tar.gz",
        "--format",
        "json",
        "--no-verify",
    ])
    .expect("unverified evidence show JSON parses");
    assert_eq!(
        evidence_json_unverified.machine_output_verify_enabled(),
        Some(false)
    );

    let evidence_table = Cli::try_parse_from(["assay", "evidence", "show", "bundle.tar.gz"])
        .expect("evidence show table parses");
    assert_eq!(evidence_table.machine_output_verify_enabled(), None);

    let coverage_json = Cli::try_parse_from(["assay", "coverage", "--format", "json"])
        .expect("legacy coverage JSON parses");
    assert_eq!(
        coverage_json.machine_output_verify_enabled(),
        Some(true),
        "legacy coverage JSON failures use the shared Summary envelope"
    );

    let coverage_input_json = Cli::try_parse_from([
        "assay",
        "coverage",
        "--input",
        "events.jsonl",
        "--format",
        "json",
    ])
    .expect("input-mode coverage JSON parses");
    assert_eq!(
        coverage_input_json.machine_output_verify_enabled(),
        None,
        "input mode writes coverage_report_v1 to a file and must not inherit the legacy envelope"
    );

    let coverage_text =
        Cli::try_parse_from(["assay", "coverage"]).expect("legacy coverage text parses");
    assert_eq!(coverage_text.machine_output_verify_enabled(), None);

    let run_json =
        Cli::try_parse_from(["assay", "run", "--config", "eval.yaml", "--format", "json"])
            .expect("run JSON parses");
    assert!(
        run_json.machine_output_verify_enabled().is_none(),
        "run owns its JSON error renderer and must not be rendered twice"
    );
}

#[test]
fn sandbox_enforcement_health_without_enforce_net_is_clap_usage_error() {
    let missing = Cli::try_parse_from([
        "assay",
        "sandbox",
        "--enforcement-health",
        "health.json",
        "--",
        "true",
    ]);
    let err = match missing {
        Ok(_) => panic!(
            "sandbox --enforcement-health without --enforce-net must fail clap before execution"
        ),
        Err(err) => err,
    };
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    let rendered = err.to_string();
    assert!(
        rendered.contains("--enforce-net"),
        "clap must name the missing --enforce-net requirement: {rendered}"
    );
}

#[test]
fn sandbox_enforcement_health_with_enforce_net_still_parses() {
    // Control: a live parser still accepts the coherent invocation. A dead harness
    // that always returned Err would make the rejection test look green.
    let ok = Cli::try_parse_from([
        "assay",
        "sandbox",
        "--enforce",
        "--enforce-net",
        "--enforcement-health",
        "health.json",
        "--",
        "true",
    ])
    .expect("sandbox --enforce --enforce-net --enforcement-health must still parse");
    match ok.cmd {
        Command::Sandbox(args) => {
            assert!(args.enforce);
            assert!(args.enforce_net);
            assert_eq!(
                args.enforcement_health.as_deref(),
                Some(std::path::Path::new("health.json"))
            );
            assert_eq!(args.command, ["true"]);
        }
        _ => panic!("expected sandbox command"),
    }
}

#[test]
fn sandbox_allow_audit_fallback_conflicts_with_fail_closed() {
    let err = match Cli::try_parse_from([
        "assay",
        "sandbox",
        "--enforce",
        "--fail-closed",
        "--allow-audit-fallback",
        "--",
        "true",
    ]) {
        Ok(_) => panic!("--allow-audit-fallback must conflict with --fail-closed at clap"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    let rendered = err.to_string();
    assert!(
        rendered.contains("--allow-audit-fallback") && rendered.contains("--fail-closed"),
        "clap must name both flags: {rendered}"
    );
}

#[test]
fn top_level_quiet_defaults_off_and_parses_before_the_subcommand_only() {
    use super::posture::ColorChoice;

    let bare = Cli::try_parse_from(["assay", "run", "--config", "eval.yaml"])
        .expect("bare run must parse");
    assert!(!bare.quiet, "top-level --quiet must default off");
    let (quiet, color) = super::posture::resolve_posture_with(false, None, None, None)
        .expect("bare posture must resolve without env");
    assert!(!quiet);
    assert_eq!(color, ColorChoice::Auto);

    let before = Cli::try_parse_from(["assay", "--quiet", "run", "--config", "eval.yaml"])
        .expect("--quiet before the subcommand must parse");
    assert!(before.quiet);

    let short = Cli::try_parse_from(["assay", "-q", "run", "--config", "eval.yaml"])
        .expect("-q must parse");
    assert!(short.quiet);

    // The top-level flag is not clap-global: after the subcommand it is an
    // unknown argument, so a trailing `--quiet` cannot silently reach another
    // command's local `quiet`. The error names the flag; the placement rule
    // lives in docs/reference/cli.
    let err = match Cli::try_parse_from(["assay", "run", "--config", "eval.yaml", "--quiet"]) {
        Ok(_) => panic!("trailing --quiet on run must not parse"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    let rendered = err.to_string();
    assert!(
        rendered.contains("--quiet"),
        "clap must name the misplaced flag: {rendered}"
    );
}

#[test]
fn top_level_color_defaults_unset_and_parses_all_values() {
    use super::posture::ColorChoice;

    let bare = Cli::try_parse_from(["assay", "run", "--config", "eval.yaml"])
        .expect("bare run must parse");
    assert_eq!(
        bare.color, None,
        "no --color flag means unset (env decides)"
    );

    for (spelling, expected) in [
        ("auto", ColorChoice::Auto),
        ("always", ColorChoice::Always),
        ("never", ColorChoice::Never),
    ] {
        let cli =
            Cli::try_parse_from(["assay", "--color", spelling, "run", "--config", "eval.yaml"])
                .unwrap_or_else(|_| panic!("--color {spelling} must parse"));
        assert_eq!(cli.color, Some(expected));
    }

    assert!(
        Cli::try_parse_from([
            "assay",
            "--color",
            "sometimes",
            "run",
            "--config",
            "eval.yaml"
        ])
        .is_err(),
        "--color must reject values outside auto|always|never"
    );
}

#[test]
fn tool_verify_local_quiet_is_independent_of_the_top_level_flag() {
    fn local_quiet(cli: &Cli) -> bool {
        match &cli.cmd {
            Command::Mcp(McpArgs {
                cmd: McpSub::Tool(tool),
            }) => match &tool.cmd {
                super::super::commands::tool::ToolCmd::Verify(args) => args.quiet,
                _ => panic!("expected mcp tool verify"),
            },
            _ => panic!("expected mcp command"),
        }
    }

    // The verify-level flag keeps its local meaning (suppress the error
    // text); it does not set the top-level flag.
    let local_only =
        Cli::try_parse_from(["assay", "mcp", "tool", "verify", "tool.json", "--quiet"])
            .expect("verify --quiet must parse");
    assert!(
        !local_only.quiet,
        "verify-level --quiet must not set the top-level flag"
    );
    assert!(
        local_quiet(&local_only),
        "verify-level --quiet must still set the local flag"
    );

    // The top-level flag no longer propagates: placed before the subcommand
    // it sets only itself, so `mcp tool verify` keeps printing its error
    // text (F2). There is no exception to the never-suppress rule.
    let top_only = Cli::try_parse_from(["assay", "--quiet", "mcp", "tool", "verify", "tool.json"])
        .expect("top-level --quiet with verify must parse");
    assert!(top_only.quiet);
    assert!(
        !local_quiet(&top_only),
        "top-level --quiet must not reach verify's local flag"
    );
}

#[test]
fn top_level_quiet_does_not_reach_monitor_or_sandbox_locals() {
    // `assay -q monitor --pid 1` must leave MonitorArgs.quiet false: on
    // Linux that local gates stdout VIOLATION/KILL lines (machine output).
    let monitor = Cli::try_parse_from(["assay", "-q", "monitor", "--pid", "1"])
        .expect("-q monitor must parse");
    assert!(monitor.quiet, "top-level -q must set itself");
    match &monitor.cmd {
        Command::Monitor(args) => {
            assert!(!args.quiet, "top-level -q must not set MonitorArgs.quiet")
        }
        _ => panic!("expected monitor command"),
    }

    // Same for sandbox: the top-level flag must not silence the banner.
    let sandbox = Cli::try_parse_from(["assay", "--quiet", "sandbox", "--", "true"])
        .expect("--quiet sandbox must parse");
    assert!(sandbox.quiet);
    match &sandbox.cmd {
        Command::Sandbox(args) => assert!(
            !args.quiet,
            "top-level --quiet must not set SandboxArgs.quiet"
        ),
        _ => panic!("expected sandbox command"),
    }

    // Control: each local flag keeps its own meaning after its subcommand.
    let monitor_local = Cli::try_parse_from(["assay", "monitor", "--pid", "1", "--quiet"])
        .expect("monitor --quiet must parse");
    assert!(
        !monitor_local.quiet,
        "monitor-level --quiet must not set the top-level flag"
    );
    match &monitor_local.cmd {
        Command::Monitor(args) => assert!(args.quiet),
        _ => panic!("expected monitor command"),
    }

    let sandbox_local = Cli::try_parse_from(["assay", "sandbox", "--quiet", "--", "true"])
        .expect("sandbox --quiet must parse");
    assert!(
        !sandbox_local.quiet,
        "sandbox-level --quiet must not set the top-level flag"
    );
    match &sandbox_local.cmd {
        Command::Sandbox(args) => assert!(args.quiet),
        _ => panic!("expected sandbox command"),
    }
}

#[test]
fn empty_quiet_and_color_env_count_as_unset() {
    use super::posture::{parse_color_env, parse_quiet_env, resolve_posture_with, ColorChoice};
    use std::ffi::OsStr;

    assert!(!parse_quiet_env(None).expect("unset ASSAY_QUIET is off"));
    assert!(!parse_quiet_env(Some(OsStr::new(""))).expect("empty ASSAY_QUIET counts as unset"));
    for truthy in ["1", "true", "TRUE", "yes", "Y", "on", "t"] {
        assert!(
            parse_quiet_env(Some(OsStr::new(truthy))).expect("boolish true must parse"),
            "{truthy} must enable quiet"
        );
    }
    for falsy in ["0", "false", "False", "no", "N", "off", "f"] {
        assert!(
            !parse_quiet_env(Some(OsStr::new(falsy))).expect("boolish false must parse"),
            "{falsy} must leave quiet off"
        );
    }
    assert!(
        parse_quiet_env(Some(OsStr::new("maybe"))).is_err(),
        "non-boolish ASSAY_QUIET must be rejected, not silently off"
    );

    assert_eq!(parse_color_env(None).expect("unset is fine"), None);
    assert_eq!(
        parse_color_env(Some(OsStr::new(""))).expect("empty counts as unset"),
        None
    );
    assert_eq!(
        parse_color_env(Some(OsStr::new("always"))).expect("always parses"),
        Some(ColorChoice::Always)
    );
    assert!(
        parse_color_env(Some(OsStr::new("sometimes"))).is_err(),
        "ASSAY_COLOR outside auto|always|never must be rejected"
    );

    // Resolution: explicit flag beats env beats default.
    let (quiet, _) =
        resolve_posture_with(false, None, Some(OsStr::new("1")), None).expect("env 1 enables");
    assert!(quiet);
    let (quiet, _) =
        resolve_posture_with(false, None, Some(OsStr::new("")), None).expect("empty env is unset");
    assert!(!quiet);
    let (_, color) = resolve_posture_with(false, None, None, Some(OsStr::new("never")))
        .expect("env color resolves");
    assert_eq!(color, ColorChoice::Never);
    let (_, color) = resolve_posture_with(
        false,
        Some(ColorChoice::Always),
        None,
        Some(OsStr::new("never")),
    )
    .expect("flag beats env");
    assert_eq!(color, ColorChoice::Always);
    let (_, color) = resolve_posture_with(false, None, None, None).expect("default resolves");
    assert_eq!(color, ColorChoice::Auto);
}

/// S1 colour rule (#2573): explicit `--color` beats `NO_COLOR` beats TTY
/// detection. `NO_COLOR` keeps its current meaning — present (even empty)
/// disables under `auto`.
#[test]
fn color_rule_flag_beats_no_color_beats_tty() {
    use super::posture::{resolve_color, ColorChoice::*};
    use std::ffi::OsStr;

    assert!(resolve_color(Auto, true, None));
    assert!(!resolve_color(Auto, false, None));
    assert!(!resolve_color(Auto, true, Some(OsStr::new("1"))));
    assert!(
        !resolve_color(Auto, true, Some(OsStr::new(""))),
        "present-but-empty NO_COLOR still disables (unchanged behaviour)"
    );
    assert!(!resolve_color(Auto, false, Some(OsStr::new(""))));
    assert!(resolve_color(Always, false, Some(OsStr::new("1"))));
    assert!(resolve_color(Always, true, None));
    assert!(resolve_color(Always, false, None));
    assert!(!resolve_color(Never, true, None));
    assert!(!resolve_color(Never, false, None));
    assert!(!resolve_color(Never, true, Some(OsStr::new("1"))));
}

/// F5: pin the `NO_COLOR` wiring, not just the rule. `color_enabled_from`
/// is the single seam between the live environment and `resolve_color`;
/// the injected lookup names the exact variable read, so a mutant that
/// stops reading `NO_COLOR` — or that maps present-but-empty to unset
/// (`is_none()` -> `map_or(true, is_empty)`) — turns this red, while the
/// pipe-based integration tests cannot see it.
#[test]
fn color_enabled_from_pins_no_color_wiring() {
    use super::posture::{color_enabled_from, ColorChoice::*};
    use std::ffi::OsString;

    use std::cell::RefCell;

    // The lookup records the exact variable requested: a mutant that reads
    // a different variable (or none) leaves `seen` empty and fails below.
    let seen = RefCell::new(Vec::new());
    let recording = |key: &str| {
        seen.borrow_mut().push(key.to_string());
        None
    };
    assert!(color_enabled_from(Auto, true, &recording));
    assert_eq!(
        seen.borrow().as_slice(),
        ["NO_COLOR"],
        "the seam must read exactly NO_COLOR"
    );

    let unset = |_: &str| None;
    let set = |_: &str| Some(OsString::from("1"));
    let empty_present = |_: &str| Some(OsString::from(""));

    // A TTY without NO_COLOR decorates.
    assert!(color_enabled_from(Auto, true, &unset));
    // Presence disables, even when empty (unchanged NO_COLOR meaning).
    assert!(!color_enabled_from(Auto, true, &set));
    assert!(
        !color_enabled_from(Auto, true, &empty_present),
        "present-but-empty NO_COLOR still disables"
    );
    // The flag beats the convention on both sides through the same seam.
    assert!(color_enabled_from(Always, false, &set));
    assert!(!color_enabled_from(Never, true, &unset));
}

#[test]
fn sandbox_allow_audit_fallback_parses_with_enforce() {
    let ok = Cli::try_parse_from([
        "assay",
        "sandbox",
        "--enforce",
        "--allow-audit-fallback",
        "--",
        "true",
    ])
    .expect("--enforce --allow-audit-fallback must still parse");
    match ok.cmd {
        Command::Sandbox(args) => {
            assert!(args.enforce);
            assert!(args.allow_audit_fallback);
            assert!(!args.fail_closed);
            assert_eq!(args.command, ["true"]);
        }
        _ => panic!("expected sandbox command"),
    }
}
