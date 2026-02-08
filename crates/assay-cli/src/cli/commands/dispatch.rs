use super::super::args::*;
use crate::exit_codes::EXIT_SUCCESS;

pub async fn dispatch(cli: Cli, legacy_mode: bool) -> anyhow::Result<i32> {
    match cli.cmd {
        Command::Init(args) => super::init::run(args).await,
        Command::Run(args) => super::run::run(args, legacy_mode).await,
        Command::Ci(args) => super::ci::run(args, legacy_mode).await,
        Command::Validate(args) => super::validate::run(args, legacy_mode).await,
        Command::Fix(args) => super::fix::run(args, legacy_mode).await,
        Command::Doctor(args) => super::doctor::run(args, legacy_mode).await,
        Command::Watch(args) => super::watch::run(args, legacy_mode).await,
        Command::Import(args) => super::import::cmd_import(args),
        Command::Quarantine(args) => super::quarantine::run(args).await,
        Command::Trace(args) => super::trace::cmd_trace(args, legacy_mode).await,
        Command::Calibrate(args) => super::calibrate::cmd_calibrate(args).await,
        Command::Baseline(args) => match args.cmd {
            BaselineSub::Report(report_args) => {
                super::baseline::cmd_baseline_report(report_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Record(record_args) => {
                super::baseline::cmd_baseline_record(record_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Check(check_args) => {
                super::baseline::cmd_baseline_check(check_args).map(|_| EXIT_SUCCESS)
            }
        },
        Command::Migrate(args) => super::migrate::cmd_migrate(args),
        Command::Coverage(args) => super::coverage::cmd_coverage(args).await,
        Command::Explain(args) => super::explain::run(args).await,
        Command::Demo(args) => super::demo::cmd_demo(args).await,
        Command::InitCi(args) => super::init_ci::cmd_init_ci(args),
        Command::Mcp(args) => super::mcp::run(args).await,
        Command::Discover(args) => super::discover::run(args).await,
        Command::Kill(args) => super::kill::run(args).await,
        Command::Monitor(args) => super::monitor::run(args).await,
        Command::Policy(args) => super::policy::run(args).await,
        Command::Generate(args) => super::generate::run(args),
        Command::Record(args) => super::record::run(args).await,
        Command::Profile(args) => super::profile::run(args),
        Command::Sandbox(args) => super::sandbox::run(args).await,
        Command::Evidence(args) => super::evidence::run(args),
        Command::Bundle(args) => super::bundle::run(args, legacy_mode).await,
        Command::Replay(args) => super::replay::run(args, legacy_mode).await,
        #[cfg(feature = "sim")]
        Command::Sim(args) => super::sim::run(args),
        Command::Setup(args) => super::setup::run(args).await,
        Command::Tool(args) => Ok(super::tool::cmd_tool(args.cmd)),
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_SUCCESS)
        }
    }
}
