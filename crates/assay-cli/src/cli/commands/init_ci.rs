use crate::cli::args::InitCiArgs;
use std::fs;
use std::path::PathBuf;

pub fn cmd_init_ci(args: InitCiArgs) -> anyhow::Result<i32> {
    let (content, default_path) = match args.provider.as_str() {
        "github" => (
            crate::templates::CI_WORKFLOW_YML,
            ".github/workflows/assay.yml",
        ),
        "gitlab" => (crate::templates::GITLAB_CI_YML, ".gitlab-ci.yml"),
        _ => anyhow::bail!(
            "Unknown provider: {}. Supported: github, gitlab",
            args.provider
        ),
    };

    let target = args.out.unwrap_or_else(|| PathBuf::from(default_path));

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    if target.exists() {
        println!("⚠  {} already exists. Skipping.", target.display());
        return Ok(0);
    }

    fs::write(&target, content)?;
    println!("✓ Created CI workflow: {}", target.display());

    // Hint next steps
    if args.provider == "github" {
        println!("\nNext: Commit this file to enable GitHub Actions.");
    }

    Ok(0)
}
