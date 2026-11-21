use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=src/file_fields.c");
    println!("cargo:rerun-if-env-changed=BPF_CLANG");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("bpf") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("file_fields.bc");
    let target = env::var("TARGET").unwrap();
    let clang = env::var_os("BPF_CLANG").unwrap_or_else(|| "clang".into());
    let status = Command::new(clang)
        .args([
            "-target",
            &target,
            "-O2",
            "-g",
            "-emit-llvm",
            "-c",
            "src/file_fields.c",
            "-o",
        ])
        .arg(&out)
        .status()
        .expect("run BPF-capable clang to compile file_open field relocations");
    assert!(
        status.success(),
        "compiling file_open field relocations failed"
    );
    println!("cargo:rustc-link-arg-bin=assay-ebpf={}", out.display());
    println!("cargo:rustc-link-arg-bin=assay-ebpf=--btf");
}
