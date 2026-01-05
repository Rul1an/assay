use crate::cli::args::InitCiArgs;
use std::fs;
use std::path::PathBuf;

pub fn cmd_init_ci(args: InitCiArgs) -> anyhow::Result<i32> {
    let (content, default_path) = match args.provider.as_str() {
        "github" => (
            crate::templates::CI_WORKFLOW_YML,
            ".github/workflows/assay.yml",
        ),
        "gitlab" => (
            r#"stages:
  - test

assay-check:
  stage: test
  image: ubuntu:latest
  script:
    - curl -sSL https://assay.dev/install.sh | sh
    - assay validate --config assay.yaml --trace-file traces.jsonl
"#,
            ".gitlab-ci.yml",
        ),
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
