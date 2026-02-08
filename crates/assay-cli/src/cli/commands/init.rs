use crate::cli::args::InitArgs;
use crate::exit_codes;
use std::path::{Path, PathBuf};

pub async fn run(args: InitArgs) -> anyhow::Result<i32> {
    if args.list_presets {
        for p in crate::packs::list() {
            println!("{}\t{}", p.name, p.description);
        }
        return Ok(exit_codes::OK);
    }

    // --from-trace: generate policy + config from existing trace
    if let Some(trace_path) = &args.from_trace {
        return run_from_trace(&args, trace_path);
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
    let pack = crate::packs::get(&args.preset)
        .ok_or_else(|| anyhow::anyhow!("unknown preset '{}'. Use --list-presets.", args.preset))?;

    // Write policy file (respecting existing)
    let policy_path = Path::new("policy.yaml");
    if policy_path.exists() {
        println!("   Skipped {} (exists)", policy_path.display());
    } else {
        std::fs::write(policy_path, pack.policy_yaml)
            .map_err(|e| anyhow::anyhow!("failed to write {}: {}", policy_path.display(), e))?;
        println!(
            "   Created {} (preset: {})",
            policy_path.display(),
            pack.name
        );
    }

    let config_template = if args.hello_trace {
        crate::templates::HELLO_EVAL_YAML
    } else {
        crate::templates::EVAL_CONFIG_DEFAULT_YAML
    };
    write_file_if_missing(&args.config, config_template)?;

    let hello_trace_path = args
        .hello_trace
        .then(|| hello_trace_path_for_config(&args.config));
    if let Some(path) = &hello_trace_path {
        write_file_if_missing(path, crate::templates::HELLO_TRACES_JSONL)?;
    }

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

        let provider = args.ci.as_deref().unwrap_or("github");
        match provider {
            "gitlab" => {
                write_file_if_missing(
                    Path::new(".gitlab-ci.yml"),
                    crate::templates::GITLAB_CI_YML,
                )?;
            }
            _ => {
                write_file_if_missing(
                    Path::new(".github/workflows/assay.yml"),
                    crate::templates::CI_WORKFLOW_YML,
                )?;
            }
        }
    }

    println!("✅  Initialization complete.");
    if args.hello_trace {
        let hello_trace = hello_trace_path
            .as_ref()
            .expect("hello trace path must exist when --hello-trace is set");
        println!(
            "   Note: hello trace uses demo prompt/response text only; treat real traces as potentially sensitive."
        );
        println!(
            "   Next: assay validate --config {} --trace-file {}",
            args.config.display(),
            hello_trace.display()
        );
    } else {
        println!("   Next: assay validate");
    }
    Ok(exit_codes::OK)
}

fn hello_trace_path_for_config(config_path: &Path) -> PathBuf {
    match config_path.parent() {
        Some(parent) if parent.as_os_str().is_empty() || parent == Path::new(".") => {
            PathBuf::from("traces/hello.jsonl")
        }
        Some(parent) => parent.join("traces/hello.jsonl"),
        None => PathBuf::from("traces/hello.jsonl"),
    }
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

// ─────────────────────────────────────────────────────────────────────────────
// init --from-trace: Generate policy + config from existing trace
// ─────────────────────────────────────────────────────────────────────────────

fn run_from_trace(args: &InitArgs, trace_path: &std::path::Path) -> anyhow::Result<i32> {
    use super::generate;
    use super::heuristics::HeuristicsConfig;

    if !trace_path.exists() {
        anyhow::bail!("trace file not found: {}", trace_path.display());
    }

    let trace_pathbuf = trace_path.to_path_buf();
    println!("🔍 Generating policy from trace: {}", trace_path.display());

    // 1. Read and aggregate events
    let events = generate::read_events(&trace_pathbuf)?;
    if events.is_empty() {
        anyhow::bail!("no events found in trace file: {}", trace_path.display());
    }
    let agg = generate::aggregate(&events);
    println!(
        "   Aggregated {} unique entries from {} events",
        agg.total(),
        events.len()
    );

    // 2. Generate policy
    let heur_cfg = HeuristicsConfig::default();
    let policy = generate::generate_from_trace("generated", &agg, args.heuristics, &heur_cfg);
    let policy_yaml = generate::serialize(&policy, "yaml")?;

    // Count entries for summary
    let allow_count = policy.files.allow.len()
        + policy.network.allow_destinations.len()
        + policy.processes.allow.len();
    let review_count = policy.files.needs_review.len()
        + policy.network.needs_review.len()
        + policy.processes.needs_review.len();
    let deny_count = policy.files.deny.len()
        + policy.network.deny_destinations.len()
        + policy.processes.deny.len();

    // 3. Write policy.yaml
    let policy_path = Path::new("policy.yaml");
    if policy_path.exists() {
        println!("   Skipped policy.yaml (exists)");
    } else {
        std::fs::write(policy_path, &policy_yaml)?;
        println!(
            "   Created policy.yaml ({} allow, {} needs_review, {} deny)",
            allow_count, review_count, deny_count
        );
    }

    // 4. Write eval.yaml config
    let config_content = r#"configVersion: 1
suite: "generated"
model: "trace"
tests:
  - id: "generated_from_trace"
    input:
      prompt: "__generated_from_trace__"
    expected:
      type: regex_match
      pattern: ".*"
      flags: ["s"]
"#
    .to_string();
    write_file_if_missing(&args.config, &config_content)?;

    // 5. Gitignore
    if args.gitignore {
        write_file_if_missing(Path::new(".gitignore"), crate::templates::GITIGNORE)?;
    }

    // 6. CI scaffolding (reuse existing logic)
    if args.ci.is_some() {
        println!("🏗️  Generating CI scaffolding...");
        let provider = args.ci.as_deref().unwrap_or("github");
        match provider {
            "gitlab" => {
                write_file_if_missing(
                    Path::new(".gitlab-ci.yml"),
                    crate::templates::GITLAB_CI_YML,
                )?;
            }
            _ => {
                write_file_if_missing(
                    Path::new(".github/workflows/assay.yml"),
                    crate::templates::CI_WORKFLOW_YML,
                )?;
            }
        }
    }

    println!("\n✅  Initialization complete.");
    println!(
        "\n   Next: assay validate --config {} --trace-file {}",
        args.config.display(),
        trace_path.display()
    );
    println!(
        "   CI:   assay ci --config {} --trace-file {}",
        args.config.display(),
        trace_path.display()
    );
    println!("\n   Tip: For EU AI Act compliance scanning, add: --pack eu-ai-act-baseline");

    Ok(exit_codes::OK)
}
