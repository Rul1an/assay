use anyhow::Context as _;
use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

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

    /// Force skipping Docker even if available (non-Linux hosts)
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

fn docker_available() -> bool {
    // Fast check: "docker version" should succeed if Docker is installed & running
    Command::new("docker")
        .args(["version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

fn build_ebpf(opts: BuildEbpfOpts) -> anyhow::Result<()> {
    let root = workspace_root()?;

    // Decide build mode
    let on_linux = cfg!(target_os = "linux");
    let can_use_docker = !opts.no_docker && docker_available();

    // Non-Linux hosts: default to Docker if available, otherwise skip (unless user forced docker)
    if !on_linux {
        if opts.no_docker {
            return build_ebpf_local(&root, &opts);
        }
        if opts.docker || can_use_docker {
             if !can_use_docker {
                anyhow::bail!(
                    "Docker build requested but Docker is not available/running. \
                     Start Docker Desktop and retry."
                );
            }
            return build_ebpf_docker(&root, &opts);
        }

        eprintln!(
            "Skipping eBPF build on non-Linux host (Docker not available).\n\
             Hint: install/start Docker Desktop, then run:\n\
             \n  cargo xtask build-ebpf --docker\n\
             \nOr run this in Linux CI (ubuntu-latest)."
        );
        return Ok(());
    }

    // Linux: default local; allow --docker if desired
    if opts.docker {
        if !can_use_docker {
            anyhow::bail!("Docker build requested but Docker is not available/running.");
        }
        return build_ebpf_docker(&root, &opts);
    }

    build_ebpf_local(&root, &opts)
}

fn build_ebpf_local(root: &PathBuf, opts: &BuildEbpfOpts) -> anyhow::Result<()> {
    let target_flag = format!("--target={}", opts.target);

    let mut args = vec![
        "+nightly",
        "build",
        "--package",
        "assay-ebpf",
        &target_flag,
        "-Z",
        "build-std=core",
        "--features",
        "ebpf",
    ];

    if opts.release {
        args.push("--release");
    }

    let rustflags = match std::env::var("RUSTFLAGS") {
        Ok(v) if !v.is_empty() => format!("{v} -C linker=bpf-linker"),
        _ => "-C linker=bpf-linker".to_string(),
    };

    let status = Command::new("cargo")
        .current_dir(root)
        .args(&args)
        .env("RUSTFLAGS", rustflags)
        .status()
        .context("Failed to run cargo build for ebpf (local)")?;

    if !status.success() {
        anyhow::bail!("Failed to build eBPF program (local)");
    }

    // Deterministic artifact copy
    let src = resolve_ebpf_output(root, &opts.target, opts.release)
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

fn resolve_ebpf_output(
    root: &std::path::Path,
    target: &str,
    release: bool,
) -> anyhow::Result<PathBuf> {
    let profile = if release { "release" } else { "debug" };

    // 1) Preferred: target/<triple>/<profile>/<bin-name>
    // Cargo bin name is usually "assay-ebpf" (package name).
    let preferred = root
        .join("target")
        .join(target)
        .join(profile)
        .join("assay-ebpf");
    if preferred.exists() {
        return Ok(preferred);
    }

    // 2) Some toolchains put it under deps/ with hashing or different naming.
    // Fallback: pick newest file that starts with "assay_ebpf" or "assay-ebpf"
    let deps_dir = root.join("target").join(target).join(profile).join("deps");
    if deps_dir.is_dir() {
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for ent in std::fs::read_dir(&deps_dir)? {
            let ent = ent?;
            let p = ent.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let looks_like = name.starts_with("assay_ebpf") || name.starts_with("assay-ebpf");
            if !looks_like {
                continue;
            }
            let mt = ent
                .metadata()?
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if best.as_ref().map(|(t, _)| mt > *t).unwrap_or(true) {
                best = Some((mt, p));
            }
        }
        if let Some((_, p)) = best {
            return Ok(p);
        }
    }

    anyhow::bail!(
        "No eBPF artifact found. Looked in: {} and {}",
        preferred.display(),
        deps_dir.display()
    );
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

    // Setup dependencies (using cache) - SKIP if using builder image
    if !opts.docker_image.contains("assay-ebpf-builder") {
        // We need nightly for -Z build-std, so install it first
        script.push_str("rustup toolchain install nightly; ");
        script.push_str("rustup component add rust-src --toolchain nightly >/dev/null 2>&1 || true; ");
        script.push_str("if ! command -v bpf-linker > /dev/null; then echo 'Installing bpf-linker...'; ");

        // Install dependencies for bpf-linker
        script.push_str("apt-get update && apt-get install -y llvm-dev libclang-dev build-essential git; ");

        script.push_str("cargo install bpf-linker --locked; fi; ");
    }

    script.push_str(r#"export RUSTFLAGS="${RUSTFLAGS:-} -C linker=bpf-linker"; "#);



    script.push_str("cargo +nightly build --package assay-ebpf ");
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
    script.push_str(&format!("chown {}:{} /work/target/assay-ebpf.o || true; ", uid, gid));
    script.push_str(&format!("chown -R {}:{} /work/target-ebpf || true; ", uid, gid));

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
