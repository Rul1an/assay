use anyhow::Context as _;
use clap::Parser;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const DEFAULT_EBPF_RUST_TOOLCHAIN: &str = "nightly-2026-01-01";
const DEFAULT_BPF_LINKER_VERSION: &str = "0.10.3";

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(Parser)]
enum Cmd {
    /// Build the eBPF bytecode
    BuildEbpf(BuildEbpfOpts),
    /// Build the Docker builder image
    BuildImage,
}

#[derive(Parser)]
struct BuildEbpfOpts {
    /// Set the endianness of the BPF target
    #[clap(default_value = "bpfel-unknown-none", long)]
    target: String,

    /// Build release target
    #[clap(long)]
    release: bool,

    /// Force building eBPF inside Docker (also works on Linux)
    #[clap(long)]
    docker: bool,

    /// Disable Docker builds and Docker fallback (forces local)
    #[clap(long)]
    no_docker: bool,

    /// Docker image to use for eBPF builds
    #[clap(long, default_value = "assay-ebpf-builder:latest")]
    docker_image: String,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    match opts.cmd {
        Cmd::BuildEbpf(opts) => build_ebpf(opts),
        Cmd::BuildImage => build_image(),
    }
}

fn workspace_root() -> anyhow::Result<PathBuf> {
    // CARGO_MANIFEST_DIR points to crates/assay-xtask
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = xtask_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .context("Failed to resolve workspace root from CARGO_MANIFEST_DIR")?;
    Ok(root.to_path_buf())
}

fn get_host_user_group() -> Option<(String, String)> {
    #[cfg(target_os = "linux")]
    {
        let uid = Command::new("id").arg("-u").output().ok()?;
        let gid = Command::new("id").arg("-g").output().ok()?;
        let uid_str = String::from_utf8(uid.stdout).ok()?.trim().to_string();
        let gid_str = String::from_utf8(gid.stdout).ok()?.trim().to_string();
        Some((uid_str, gid_str))
    }
    #[cfg(not(target_os = "linux"))]
    None
}

fn has_cmd(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ebpf_rust_toolchain() -> String {
    std::env::var("ASSAY_EBPF_RUST_TOOLCHAIN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_EBPF_RUST_TOOLCHAIN.to_string())
}

fn bpf_linker_version() -> String {
    std::env::var("ASSAY_BPF_LINKER_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BPF_LINKER_VERSION.to_string())
}

fn has_pinned_bpf_linker() -> bool {
    let expected = format!("bpf-linker {}", bpf_linker_version());
    Command::new("bpf-linker")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|stdout| stdout.contains(&expected))
        .unwrap_or(false)
}

fn docker_allowed(opts: &BuildEbpfOpts) -> bool {
    // explicit override flags win
    if opts.no_docker {
        return false;
    }
    true
}

fn build_ebpf(opts: BuildEbpfOpts) -> anyhow::Result<()> {
    let root = workspace_root()?;

    if opts.docker {
        return build_ebpf_docker(&root, &opts);
    }

    // Default path: local build, with optional docker fallback
    build_ebpf_local(&root, &opts)
}

fn build_ebpf_local(root: &PathBuf, opts: &BuildEbpfOpts) -> anyhow::Result<()> {
    // Auto-fallback only when allowed and docker exists
    if !has_pinned_bpf_linker() {
        if docker_allowed(opts) && has_cmd("docker") {
            eprintln!("pinned bpf-linker not found; falling back to docker build...");
            return build_ebpf_docker(root.as_path(), opts);
        }

        anyhow::bail!(
            "pinned bpf-linker not found.\n\
             Install it with: cargo install bpf-linker --version {} --locked\n\
             Or rerun with --docker (or omit --no-docker to allow fallback).",
            bpf_linker_version()
        );
    }

    let target_flag = format!("--target={}", opts.target);
    let toolchain = ebpf_rust_toolchain();
    let toolchain_arg = format!("+{toolchain}");

    let mut args = vec![
        toolchain_arg.as_str(),
        "build",
        "--package",
        "assay-ebpf",
        &target_flag,
        "-Z",
        "build-std=core",
        "--features",
        "ebpf",
        // JSON messages on stdout (captured below); diagnostics still render
        // human-readably on stderr.
        "--message-format=json-render-diagnostics",
    ];

    if opts.release {
        args.push("--release");
    }

    let rustflags = match std::env::var("RUSTFLAGS") {
        Ok(v) if !v.is_empty() => format!("{v} -C linker=bpf-linker"),
        _ => "-C linker=bpf-linker".to_string(),
    };

    let output = Command::new("cargo")
        .current_dir(root)
        .args(&args)
        .env("RUSTFLAGS", rustflags)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("Failed to run cargo build for ebpf (local)")?;

    if !output.status.success() {
        anyhow::bail!("Failed to build eBPF program (local)");
    }

    // Deterministic artifact copy. The source path comes from cargo's own
    // artifact report for THIS invocation, never from filesystem guessing.
    let src = resolve_ebpf_artifact(&String::from_utf8_lossy(&output.stdout))
        .context("Could not locate built eBPF artifact")?;
    let dst = root.join("target").join("assay-ebpf.o");

    std::fs::create_dir_all(dst.parent().unwrap()).ok();
    std::fs::copy(&src, &dst).with_context(|| {
        format!(
            "Failed to copy eBPF artifact from {} to {}",
            src.display(),
            dst.display()
        )
    })?;

    println!("eBPF build successful (local)");
    println!("  target: {}", opts.target);
    println!("  src:    {}", src.display());
    println!("  out:    {}", dst.display());
    Ok(())
}

/// Resolve the built eBPF object from cargo's `--message-format=json` output.
///
/// Cargo emits a `compiler-artifact` message for every artifact belonging to
/// this invocation — including fingerprint-validated cached ones ("fresh") —
/// so the reported path is canonical by construction. Guessing filesystem
/// paths instead (preferred-path-if-exists, newest-by-mtime in deps/) can
/// return a stale object from an earlier build on persistent runners
/// (issue #1575).
fn resolve_ebpf_artifact(cargo_json_output: &str) -> anyhow::Result<PathBuf> {
    let mut artifact: Option<PathBuf> = None;
    for line in cargo_json_output.lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg["reason"] != "compiler-artifact" {
            continue;
        }
        let target = &msg["target"];
        if target["name"] != "assay-ebpf" {
            continue;
        }
        let is_bin = target["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|k| k == "bin"));
        if !is_bin {
            continue;
        }
        let path = msg["executable"]
            .as_str()
            .or_else(|| msg["filenames"][0].as_str())
            .map(PathBuf::from);
        if path.is_some() {
            // Keep the last match; cargo reports the final bin artifact last.
            artifact = path;
        }
    }
    artifact.context(
        "cargo build succeeded but reported no compiler-artifact for the `assay-ebpf` bin target.\n\
         Refusing to guess the artifact path from the filesystem: on persistent runners a\n\
         stale object from an earlier build could be silently treated as canonical\n\
         (see https://github.com/Rul1an/assay/issues/1575).",
    )
}

fn build_ebpf_docker(root: &std::path::Path, opts: &BuildEbpfOpts) -> anyhow::Result<()> {
    let root_str = root
        .to_str()
        .context("workspace root path is not valid utf-8")?;

    // Build command that runs inside the container.
    let mut script = String::new();
    script.push_str("set -euo pipefail; ");

    // Create target dir on host to ensure it is user-owned (not root-owned by docker mount)
    std::fs::create_dir_all(root.join("target")).ok();

    // Fix permissions: if we are on Linux, we want the container to write files as the host user
    // or at least chown them back.
    let (uid, gid) = get_host_user_group().unwrap_or(("0".into(), "0".into()));

    // Fix Docker IO/Storage issues by writing to host volume
    script.push_str("export TMPDIR=/work/.tmp; mkdir -p /work/.tmp; ");
    script.push_str("export CARGO_TARGET_DIR=/work/target-ebpf; ");

    // Ensure cargo is in PATH (standard rust image location)
    script.push_str("export PATH=\"/usr/local/cargo/bin:$PATH\"; ");

    let toolchain = ebpf_rust_toolchain();
    let bpf_linker_version = bpf_linker_version();

    // Setup dependencies (using cache) - SKIP if using builder image
    if !opts.docker_image.contains("assay-ebpf-builder") {
        // We need a pinned nightly for -Z build-std, so install it first.
        let _ = write!(
            script,
            "rustup toolchain install {toolchain} --profile minimal; "
        );
        let _ = write!(
            script,
            "rustup component add rust-src --toolchain {toolchain} >/dev/null 2>&1 || true; "
        );
        let _ = write!(
            script,
            "if ! command -v bpf-linker > /dev/null || ! bpf-linker --version | grep -Fq 'bpf-linker {bpf_linker_version}'; then echo 'Installing bpf-linker...'; "
        );

        // Install dependencies for bpf-linker
        script.push_str(
            "apt-get update && apt-get install -y llvm-dev libclang-dev build-essential git; ",
        );

        let _ = write!(
            script,
            "rustup run {toolchain} cargo install bpf-linker --version {bpf_linker_version} --locked; fi; "
        );
    }

    // Always ensure bpf-linker exists in-container (builder should already have it)
    let _ = write!(
        script,
        "if ! command -v bpf-linker >/dev/null 2>&1 || ! bpf-linker --version | grep -Fq 'bpf-linker {bpf_linker_version}'; then "
    );
    if opts.docker_image.contains("assay-ebpf-builder") {
        script.push_str("echo 'ERROR: bpf-linker missing in builder image'; exit 1; ");
    } else {
        script.push_str("echo 'Installing bpf-linker...'; ");
        script.push_str(
            "apt-get update && apt-get install -y llvm-dev libclang-dev build-essential git; ",
        );
        let _ = write!(
            script,
            "rustup run {toolchain} cargo install bpf-linker --version {bpf_linker_version} --locked; "
        );
    }
    script.push_str("fi; ");

    script.push_str(r#"export RUSTFLAGS="${RUSTFLAGS:-} -C linker=bpf-linker"; "#);

    let _ = write!(script, "cargo +{toolchain} build --package assay-ebpf ");
    script.push_str(&format!("--target {} ", opts.target));
    script.push_str("--release "); // Force release build for eBPF (LLVM strictness)
    script.push_str("-Z build-std=core ");
    script.push_str("--features ebpf ");
    // if opts.release { ... } - Removed check, always release
    script.push_str("; "); // End cargo build command

    // ✅ Deterministic copy inside Docker
    let profile = "release"; // We forced --release above
    script.push_str(&format!(
        r#"OUT="/work/target-ebpf/{t}/{p}/assay-ebpf"; "#,
        t = opts.target,
        p = profile
    ));
    // fallback via deps/ (hashed) - pick newest
    script.push_str(&format!(
        r#"if [ ! -f "$OUT" ]; then OUT="$(ls -t /work/target-ebpf/{t}/{p}/deps/assay_ebpf* 2>/dev/null | head -n1)"; fi; "#,
        t = opts.target,
        p = profile
    ));
    // Fail if not found
    script.push_str(r#"test -f "$OUT" || (echo "Could not locate built eBPF artifact inside Docker" >&2; exit 1); "#);

    // Copy to host volume mapping
    script.push_str(r#"mkdir -p /work/target; "#);
    script.push_str(r#"cp -f "$OUT" /work/target/assay-ebpf.o; "#);
    script.push_str(r#"cp -f "$OUT" /work/target/assay-ebpf.o; "#);

    // Chown the output file to match host user
    let _ = write!(
        script,
        "chown {uid}:{gid} /work/target/assay-ebpf.o || true; "
    );
    let _ = write!(script, "chown -R {uid}:{gid} /work/target-ebpf || true; ");

    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{root_str}:/work"),
            "-v",
            "assay-cargo-registry:/usr/local/cargo/registry", // Persistence!
            "-v",
            "assay-cargo-git:/usr/local/cargo/git", // Persistence!
            "-w",
            "/work",
            &opts.docker_image,
            "bash",
            "-lc",
            &script,
        ])
        .status()
        .context("Failed to run docker for ebpf build")?;

    if !status.success() {
        anyhow::bail!("Failed to build eBPF program (docker)");
    }

    println!("eBPF build successful (docker) for target {}", opts.target);
    Ok(())
}

fn build_image() -> anyhow::Result<()> {
    let root = workspace_root()?;
    let status = Command::new("docker")
        .current_dir(&root)
        .args([
            "build",
            "-t",
            "assay-ebpf-builder:latest",
            "-f",
            "docker/Dockerfile.ebpf-builder",
            ".",
        ])
        .status()
        .context("Failed to run docker build")?;

    if !status.success() {
        anyhow::bail!("Docker build failed");
    }
    println!("Successfully built assay-ebpf-builder:latest");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_ebpf_artifact;

    fn artifact_msg(name: &str, kind: &str, executable: Option<&str>, filename: &str) -> String {
        let executable = match executable {
            Some(p) => format!("\"{p}\""),
            None => "null".to_string(),
        };
        format!(
            r#"{{"reason":"compiler-artifact","package_id":"path+file:///w/crates/{name}#3.31.1","target":{{"name":"{name}","kind":["{kind}"],"crate_types":["{kind}"]}},"filenames":["{filename}"],"executable":{executable},"fresh":false}}"#
        )
    }

    #[test]
    fn picks_executable_from_bin_artifact() {
        let out = [
            artifact_msg("core", "lib", None, "/t/deps/libcore-abc.rlib"),
            artifact_msg(
                "assay-ebpf",
                "bin",
                Some("/t/bpfel-unknown-none/release/assay-ebpf"),
                "/t/bpfel-unknown-none/release/assay-ebpf",
            ),
            r#"{"reason":"build-finished","success":true}"#.to_string(),
        ]
        .join("\n");
        let path = resolve_ebpf_artifact(&out).unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/t/bpfel-unknown-none/release/assay-ebpf"
        );
    }

    #[test]
    fn falls_back_to_filenames_when_executable_is_null() {
        let out = artifact_msg(
            "assay-ebpf",
            "bin",
            None,
            "/t/bpfel-unknown-none/release/deps/assay_ebpf-1234",
        );
        let path = resolve_ebpf_artifact(&out).unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/t/bpfel-unknown-none/release/deps/assay_ebpf-1234"
        );
    }

    #[test]
    fn last_matching_bin_artifact_wins() {
        let out = [
            artifact_msg("assay-ebpf", "bin", Some("/t/old"), "/t/old"),
            artifact_msg("assay-ebpf", "bin", Some("/t/new"), "/t/new"),
        ]
        .join("\n");
        assert_eq!(
            resolve_ebpf_artifact(&out).unwrap().to_str().unwrap(),
            "/t/new"
        );
    }

    #[test]
    fn ignores_non_bin_and_other_packages() {
        let out = [
            artifact_msg("assay-ebpf", "lib", None, "/t/deps/libassay_ebpf.rlib"),
            artifact_msg(
                "assay-common",
                "bin",
                Some("/t/assay-common"),
                "/t/assay-common",
            ),
        ]
        .join("\n");
        assert!(resolve_ebpf_artifact(&out).is_err());
    }

    #[test]
    fn skips_non_json_lines_and_errors_clearly_when_absent() {
        let out = "warning: something\nnot json at all\n";
        let err = resolve_ebpf_artifact(out).unwrap_err().to_string();
        assert!(
            err.contains("assay-ebpf"),
            "error should name the target: {err}"
        );
        assert!(
            err.contains("1575"),
            "error should reference issue #1575: {err}"
        );
    }
}
