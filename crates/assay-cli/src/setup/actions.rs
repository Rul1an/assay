use super::plan::SetupAction;
use std::os::unix::fs::PermissionsExt;

pub async fn execute_action(action: SetupAction) -> anyhow::Result<()> {
    match action {
        SetupAction::Mkdir {
            path,
            mode,
            owner: _,
            group: _,
        } => {
            if path.exists() {
                // Idempotent check
                return Ok(());
            }
            std::fs::create_dir_all(&path)?;
            #[cfg(unix)]
            {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
            }
            Ok(())
        }
        SetupAction::InstallBinary { from, to, mode } => {
            if let Some(p) = to.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            std::fs::copy(from, &to)?;
            #[cfg(unix)]
            {
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode))?;
            }
            Ok(())
        }
        SetupAction::SetCaps { path, caps } => {
            #[cfg(target_os = "linux")]
            {
                let caps_str = format!("{}+ep", caps.join(","));
                let status = std::process::Command::new("setcap")
                    .arg(&caps_str)
                    .arg(path)
                    .status()?;
                if !status.success() {
                    // Try to detect common failure reasons
                    if std::process::Command::new("which")
                        .arg("setcap")
                        .output()
                        .is_err()
                    {
                        anyhow::bail!("missing 'setcap' command. Install libcap-progs.");
                    }
                    anyhow::bail!("setcap failed (exit code {:?}). Need sudo?", status.code());
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = path;
                let _ = caps;
            }
            Ok(())
        }
        SetupAction::Verify { description } => {
            eprintln!("Verifying: {}", description);
            Ok(())
        }
    }
}

pub async fn execute_plan(plan: super::plan::SetupPlan) -> anyhow::Result<()> {
    for action in plan.actions {
        execute_action(action).await?;
    }
    Ok(())
}
