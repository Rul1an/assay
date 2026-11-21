use assay_registry::Lockfile;
use serde_json::Value;

#[test]
fn specified_lockfile_entries_survive_a_round_trip() {
    let spec =
        include_str!("../../../docs/architecture/SPEC-Pack-Registry-v1.md").replace("\r\n", "\n");
    let section = spec.split_once("### 8.2 Lockfile Format").unwrap().1;
    let example = section
        .split_once("```yaml\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    let absent = example
        .lines()
        .filter(|line| !line.trim_start().starts_with("namespace:"))
        .collect::<Vec<_>>()
        .join("\n");

    for yaml in [
        example.to_owned(),
        example.replace("namespace: null", "namespace: orgs/acme"),
        absent,
    ] {
        let original: Value = serde_yaml::from_str(&yaml).unwrap();
        let lockfile = Lockfile::parse(&yaml).unwrap();
        let saved: Value = serde_yaml::from_str(&lockfile.to_yaml().unwrap()).unwrap();
        assert_eq!(saved, original);
    }
}
