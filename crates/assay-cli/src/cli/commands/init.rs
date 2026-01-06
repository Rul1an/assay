use crate::cli::args::InitArgs;
use crate::cli::commands::exit_codes;
use std::path::Path;

pub async fn run(args: InitArgs) -> anyhow::Result<i32> {
    if args.list_packs {
        for p in crate::packs::list() {
            println!("{}\t{}", p.name, p.description);
        }
        return Ok(exit_codes::OK);
    }

    println!("🔍 Scanning project for MCP configurations...");

    let mut found_config = false;

    // 1. Detect Config Files
    if Path::new("claude_desktop_config.json").exists() {
        println!("   ✨ Detected Claude Desktop config");
        found_config = true;
    } else if let Some(home) = dirs::home_dir() {
        // Check standard macOS path
        let mac_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");
        if mac_path.exists() {
            println!("   ✨ Detected Claude Desktop config (global)");
            // We could offer to import it, but for now just acknowledging it is good DX
        }
    }

    if Path::new("mcp.json").exists() {
        println!("   ✨ Detected mcp.json");
        found_config = true;
    }

    // 2. Detect Package Type (Node/Python)
    if Path::new("package.json").exists() {
        println!("   📦 Detected Node.js project");
        found_config = true;
    } else if Path::new("pyproject.toml").exists() || Path::new("requirements.txt").exists() {
        println!("   🐍 Detected Python project");
        found_config = true;
    }

    if !found_config {
        println!("   ℹ️  No specific MCP config found, initializing generic project.");
    }

    println!("\n🏗️  Generating Assay Policy & Config...");

    // Write Policy Pack
    let pack = crate::packs::get(&args.pack)
        .ok_or_else(|| anyhow::anyhow!("unknown pack '{}'. Use --list-packs.", args.pack))?;

    // Write policy file (respecting existing)
    let policy_path = Path::new("policy.yaml");
    if policy_path.exists() {
        println!("   Skipped {} (exists)", policy_path.display());
    } else {
        std::fs::write(policy_path, pack.policy_yaml)
            .map_err(|e| anyhow::anyhow!("failed to write {}: {}", policy_path.display(), e))?;
        println!("   Created {} (pack: {})", policy_path.display(), pack.name);
    }

    write_file_if_missing(&args.config, crate::templates::ASSAY_CONFIG_DEFAULT_YAML)?;

    // 2. Gitignore
    if args.gitignore {
        write_file_if_missing(Path::new(".gitignore"), crate::templates::GITIGNORE)?;
    }

    // 3. CI Scaffolding
    // Handle the boolean flag or the provider string if we upgrade the arg
    if args.ci.is_some() {
        println!("🏗️  Generating CI scaffolding...");
        write_file_if_missing(Path::new("ci-eval.yaml"), crate::templates::CI_EVAL_YAML)?;
        write_file_if_missing(
            Path::new("schemas/ci_answer.schema.json"),
            crate::templates::CI_SCHEMA_JSON,
        )?;
        write_file_if_missing(
            Path::new("traces/ci.jsonl"),
            crate::templates::CI_TRACES_JSONL,
        )?;

        // TODO: Use the provider value to select template
        write_file_if_missing(
            Path::new(".github/workflows/assay.yml"),
            crate::templates::CI_WORKFLOW_YML,
        )?;
    }

    println!("✅  Initialization complete. Run 'assay audit' to test.");
    Ok(exit_codes::OK)
}

fn write_file_if_missing(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(path, content)?;
        println!("   Created {}", path.display());
    } else {
        println!("   Skipped {} (exists)", path.display());
    }
    Ok(())
}
