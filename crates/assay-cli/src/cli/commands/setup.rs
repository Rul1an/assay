use crate::cli::args::SetupArgs;
use crate::diagnostics::{probe_system, SystemStatus};
use crate::setup::{execute_plan, generate_plan, SetupAction};

pub async fn run(args: SetupArgs) -> anyhow::Result<i32> {
    // 1. Preflight Diagnostics
    let report = probe_system();

    // Banner
    eprintln!("Assay Setup (Phase 2)");
    eprintln!("─────────────────────");
    eprintln!("Platform: {}", report.platform);
    eprintln!("Status:   {:?}", report.status);

    // 2. Generate Plan
    let plan = generate_plan(&args);

    if plan.actions.is_empty() {
        if report.status == SystemStatus::Ready {
            eprintln!("\nSystem is READY. No actions needed.");
        } else if args.helper_from.is_none() {
            eprintln!("\nSystem is {:?}. Helper missing.", report.status);
            eprintln!("To enable enforcement, build the helper and run:");
            eprintln!("  sudo assay setup --apply --helper-from <PATH_TO_GENERATED_BINARY>");
            eprintln!("\nNothing to do (no --helper-from provided).");
        }
        return Ok(0);
    }

    // 3. Display Plan
    eprintln!(
        "\nSetup Plan ({}):",
        if args.apply { "Executing" } else { "Dry Run" }
    );
    for action in &plan.actions {
        match action {
            SetupAction::Mkdir { path, mode, .. } => {
                eprintln!("  [MKDIR]   {} ({:o})", path.display(), mode)
            }
            SetupAction::InstallBinary { from, to, .. } => {
                eprintln!("  [INSTALL] {} -> {}", from.display(), to.display())
            }
            SetupAction::SetCaps { path, caps } => {
                eprintln!("  [SETCAP]  {} {}", path.display(), caps.join(","))
            }
            SetupAction::Verify { description } => eprintln!("  [VERIFY]  {}", description),
        }
    }

    if plan.requires_sudo && !args.apply {
        eprintln!("\nNote: This plan requires root privileges.");
    }
    eprintln!();

    // 4. Execution
    if args.apply {
        #[cfg(unix)]
        if plan.requires_sudo && unsafe { libc::geteuid() } != 0 {
            eprintln!("Error: Plan requires root permissions.");
            eprintln!("Please re-run with sudo:");
            eprintln!("  sudo assay setup --apply ...");
            return Ok(2);
        }

        eprintln!("Applying actions...");
        if let Err(e) = execute_plan(plan).await {
            eprintln!("FAILED: {}", e);
            return Ok(2);
        }
        eprintln!("Setup complete.");

        // Post-verification
        let new_report = probe_system();
        eprintln!("New Status: {:?}", new_report.status);
    } else {
        eprintln!("Dry run complete. Use --apply to execute.");
    }

    Ok(0)
}
