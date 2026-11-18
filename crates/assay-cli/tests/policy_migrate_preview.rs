use std::{fs, process::Command};

#[test]
fn check_previews_the_migration_without_writing_either_path() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("policy.yaml");
    let output = dir.path().join("out.yaml");
    let original = "# retain the original bytes\nversion: \"1.0\"\nallow: [Search]\n";
    fs::write(&input, original).unwrap();
    fs::write(&output, "existing output").unwrap();

    let run = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .env_remove("ASSAY_STRICT_DEPRECATIONS")
            .args(["policy", "migrate", "--input", "policy.yaml"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    let preview = run(&["--dry-run"]);
    assert!(!preview.is_empty());
    for args in [&["--check"][..], &["--check", "--output", "out.yaml"]] {
        let checked = run(args);
        assert_eq!(fs::read_to_string(&input).unwrap(), original);
        assert_eq!(fs::read_to_string(&output).unwrap(), "existing output");
        assert_eq!(checked, preview);
    }
    run(&["--output", "out.yaml"]);
    assert_eq!(fs::read_to_string(&input).unwrap(), original);
    assert_eq!(
        fs::read_to_string(&output).unwrap().trim_end(),
        String::from_utf8(preview).unwrap().trim_end()
    );
}
