use assay_evidence::lint::packs::loader::{load_pack_from_file, PackError};

#[test]
fn numeric_author_keeps_its_text_and_pack_digest() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pack.yaml");
    let pack = include_str!("../packs/cicd-starter.yaml");
    let yaml = pack
        .lines()
        .map(|line| {
            if line.starts_with("author:") {
                "author: 17"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&file, &yaml).unwrap();
    let loaded = load_pack_from_file(&file).unwrap();
    assert_eq!(loaded.definition.author, "17");

    std::fs::write(&file, yaml.replace("author: 17", "author: '17'")).unwrap();
    assert_eq!(load_pack_from_file(&file).unwrap().digest, loaded.digest);
}

#[test]
fn unknown_field_reports_its_name_and_source_location() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pack.yaml");
    std::fs::write(&file, "# pack metadata\nunknown_setting: true\n").unwrap();
    let PackError::YamlParseError { message } = load_pack_from_file(&file).unwrap_err() else {
        panic!("expected a YAML parse error");
    };
    assert!(
        message.contains("unknown field `unknown_setting`"),
        "{message}"
    );
    assert!(message.contains("line 2 column 1"), "{message}");
}
