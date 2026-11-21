use std::{env, fs::File, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=src/file_fields.c");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("bpf") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("file_fields.bc");
    let target = match env::var("TARGET").as_deref() {
        Ok("bpfel-unknown-none") => "bpfel-unknown-none",
        Ok("bpfeb-unknown-none") => "bpfeb-unknown-none",
        _ => panic!("expected a little- or big-endian BPF target"),
    };
    let status = Command::new("clang")
        .args([
            "-target",
            target,
            "-O2",
            "-g",
            "-emit-llvm",
            "-c",
            "src/file_fields.c",
            "-o",
            "-",
        ])
        .stdout(File::create(&out).expect("create file_fields.bc in Cargo output directory"))
        .status()
        .expect("run BPF-capable clang to compile file_open field relocations");
    assert!(
        status.success(),
        "compiling file_open field relocations failed"
    );
    println!("cargo:rustc-link-arg-bin=assay-ebpf={}", out.display());
    println!("cargo:rustc-link-arg-bin=assay-ebpf=--btf");
}
