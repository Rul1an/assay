use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum SetupAction {
    Mkdir {
        path: PathBuf,
        mode: u32,
        owner: String,
        group: String,
    },
    InstallBinary {
        from: PathBuf,
        to: PathBuf,
        mode: u32,
    },
    SetCaps {
        path: PathBuf,
        caps: Vec<String>,
    },
    Verify {
        description: String,
    },
}

#[derive(Debug, Default)]
pub struct SetupPlan {
    pub actions: Vec<SetupAction>,
    pub requires_sudo: bool,
}

use crate::cli::args::SetupArgs;

pub fn generate_plan(args: &SetupArgs) -> SetupPlan {
    let mut plan = SetupPlan::default();

    // We reuse probing logic (from diagnostics or just simple checks)
    // Here we can be idempotent: check if target state exists.

    // 1. Runtime Dir
    if !args.runtime_dir.exists() {
        plan.actions.push(SetupAction::Mkdir {
            path: args.runtime_dir.clone(),
            mode: 0o770,
            owner: "root".into(),
            group: "assay".into(), // ideally we create this group too, but for v0.1 keep simple
        });
        plan.requires_sudo = true;
    }

    // 2. Install Helper
    if let Some(src) = &args.helper_from {
        let dest = args.prefix.join("assay-bpf");

        // Idempotency: only install if missing or different?
        // For security binaries, "always overwrite on request" is safer to ensure version match.
        // User explicitly asked for install via flags.

        plan.actions.push(SetupAction::InstallBinary {
            from: src.clone(),
            to: dest.clone(),
            mode: 0o755,
        });

        if cfg!(target_os = "linux") {
            plan.actions.push(SetupAction::SetCaps {
                path: dest,
                caps: vec![
                    "cap_bpf".to_string(),
                    "cap_perfmon".to_string(),
                    "cap_sys_resource".to_string(),
                ],
            });
        }
        plan.requires_sudo = true;
    }

    plan
}
