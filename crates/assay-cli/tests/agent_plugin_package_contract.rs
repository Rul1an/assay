#[allow(dead_code)]
#[path = "../../../tests/support/agent_golden_path.rs"]
mod agent_golden_path;

use agent_golden_path::{classify_working_directory, WorkingDirectory};
use jsonschema::{Draft, Validator};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SCHEMA_ID: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
const SCHEMA_SHA256: &str = "0a4aad95ce337878ad38802ebf0daa3fde76abe3f65400c86bcbb1ec0b3ab883";
const PACKAGE_FILES: [&str; 8] = [
    "plugin.json",
    "schemas/plugin.schema.json",
    "schemas/plugin.schema.lock.json",
    "skills/assay-golden-path/SKILL.md",
    "skills/assay-golden-path/references/agent-golden-path.json",
    "skills/assay-golden-path/assets/privileged-action-gate/mock_github_mcp.py",
    "skills/assay-golden-path/assets/privileged-action-gate/baseline-approved.json",
    "skills/assay-golden-path/assets/privileged-action-gate/policies/no-allowance.yaml",
];
const PACKAGE_DIRS: [&str; 8] = [
    "",
    "schemas",
    "skills",
    "skills/assay-golden-path",
    "skills/assay-golden-path/references",
    "skills/assay-golden-path/assets",
    "skills/assay-golden-path/assets/privileged-action-gate",
    "skills/assay-golden-path/assets/privileged-action-gate/policies",
];

struct LocalOnlyRetriever;

impl jsonschema::Retrieve for LocalOnlyRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("external schema retrieval is disabled: {uri}").into())
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("assay-cli must live below the workspace root")
        .to_path_buf()
}

fn package_root() -> PathBuf {
    workspace_root().join("packaging/agent-plugin")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(
        &std::fs::read(path)
            .unwrap_or_else(|error| panic!("read package JSON {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse package JSON {}: {error}", path.display()))
}

fn validator() -> Validator {
    let schema = read_json(&package_root().join("schemas/plugin.schema.json"));
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_retriever(LocalOnlyRetriever)
        .build(&schema)
        .expect("vendored Agent Plugins schema must compile without network access")
}

fn assert_invalid(validator: &Validator, value: &Value, context: &str) {
    assert!(
        !validator.is_valid(value),
        "schema accepted invalid manifest mutation: {context}"
    );
}

fn collect_files(root: &Path) -> BTreeSet<String> {
    let expected_dirs = PACKAGE_DIRS.into_iter().collect::<BTreeSet<_>>();
    let mut files = BTreeSet::new();

    // Traverse only compile-time allowlisted directories. Directory entries are
    // compared as names; they never become paths that this test opens.
    for relative_dir in PACKAGE_DIRS {
        let dir = root.join(relative_dir);
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read package directory {}: {error}", dir.display()))
        {
            let entry = entry.expect("read package entry");
            let file_type = entry.file_type().expect("read package entry type");
            assert!(!file_type.is_symlink(), "package must not contain symlinks");
            let name = entry.file_name();
            let name = name.to_str().expect("package names must be UTF-8");
            let relative = if relative_dir.is_empty() {
                name.to_string()
            } else {
                format!("{relative_dir}/{name}")
            };
            if file_type.is_dir() {
                assert!(
                    expected_dirs.contains(relative.as_str()),
                    "unexpected package directory: {relative}"
                );
            } else {
                assert!(file_type.is_file(), "package entries must be regular files");
                files.insert(relative);
            }
        }
    }

    files
}

#[test]
fn portable_agent_plugin_manifest_validates_offline() {
    let root = package_root();
    let schema_bytes = std::fs::read(root.join("schemas/plugin.schema.json"))
        .expect("vendored Agent Plugins schema must exist");
    assert_eq!(schema_bytes.len(), 1_805, "vendored schema byte count");
    let schema_digest = Sha256::digest(&schema_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(schema_digest, SCHEMA_SHA256, "vendored schema digest");

    let lock = read_json(&root.join("schemas/plugin.schema.lock.json"));
    assert_eq!(lock["canonical_url"], SCHEMA_ID);
    assert_eq!(lock["sha256"], SCHEMA_SHA256);
    assert_eq!(lock["bytes"], 1_805);

    let manifest = read_json(&root.join("plugin.json"));
    let validator = validator();
    if !validator.is_valid(&manifest) {
        let errors = validator
            .iter_errors(&manifest)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        panic!("portable Agent Plugin manifest is invalid:\n{errors}");
    }
    assert_eq!(manifest["$schema"], SCHEMA_ID);
    assert_eq!(manifest["name"], "assay");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["author"]["name"], "Assay");
    assert_eq!(manifest["repository"], "https://github.com/Rul1an/assay");
    assert_eq!(manifest["homepage"], "https://getassay.dev");
    assert_eq!(manifest["license"], "MIT");

    let mut missing_schema = manifest.clone();
    missing_schema
        .as_object_mut()
        .expect("manifest object")
        .remove("$schema");
    assert_invalid(&validator, &missing_schema, "missing $schema");

    let mut unknown_field = manifest.clone();
    unknown_field["commands"] = serde_json::json!([]);
    assert_invalid(&validator, &unknown_field, "unknown top-level field");

    let mut invalid_name = manifest;
    invalid_name["name"] = serde_json::json!("Assay");
    assert_invalid(&validator, &invalid_name, "uppercase plugin name");
}

#[test]
fn portable_agent_plugin_is_skills_only_and_self_contained() {
    let root = package_root();
    assert!(!root.join("mcp.json").exists(), "portable MCP is deferred");

    let actual = collect_files(&root);
    let expected = PACKAGE_FILES
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "portable package file inventory drifted");

    let skill_path = root.join("skills/assay-golden-path/SKILL.md");
    let skill = std::fs::read_to_string(&skill_path)
        .unwrap_or_else(|error| panic!("read portable skill {}: {error}", skill_path.display()));
    assert!(skill.is_ascii(), "portable skill must be ASCII");
    for forbidden in [
        "${CLAUDE_PLUGIN_ROOT}",
        "${PLUGIN_ROOT}",
        ".agents/",
        ".claude/",
        "packaging/",
        "docs/generated/",
        "examples/privileged-action-gate",
    ] {
        assert!(
            !skill.contains(forbidden),
            "portable skill leaked host/source path: {forbidden}"
        );
    }
    for required in [
        "references/agent-golden-path.json",
        "assets/privileged-action-gate",
    ] {
        assert!(
            skill.contains(required),
            "portable skill omits package-relative path: {required}"
        );
    }
}

fn join_classified(base: &Path, components: &[String]) -> PathBuf {
    let mut joined = base.to_path_buf();
    for component in components {
        joined.push(component);
    }
    joined
}

fn assert_stays_inside_package(package: &Path, candidate: &Path) {
    let package = package.components().collect::<Vec<_>>();
    let candidate = candidate.components().collect::<Vec<_>>();
    assert!(
        candidate.starts_with(&package),
        "resolved working_directory escaped the package"
    );
}

fn require_source_repo_relative(step: &Value) -> Vec<String> {
    match classify_working_directory(step) {
        Ok(WorkingDirectory::SourceRepoRelative(components)) => components,
        Ok(WorkingDirectory::Invocation) => {
            panic!("working_directory resolver path classified as invocation cwd")
        }
        Err(error) => panic!("{error}"),
    }
}

fn require_typed_surface<'a>(
    by_surface: &'a serde_json::Map<String, Value>,
    name: &str,
    expected_kind: &str,
) -> &'a str {
    let surface = by_surface
        .get(name)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("by_surface.{name} must be a typed path object"));
    assert_eq!(
        surface.get("kind").and_then(Value::as_str),
        Some(expected_kind),
        "by_surface.{name}.kind"
    );
    surface
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("by_surface.{name}.value"))
}

#[test]
fn portable_agent_plugin_resolves_working_directories_inside_package() {
    let package = package_root();
    let contract =
        read_json(&package.join("skills/assay-golden-path/references/agent-golden-path.json"));
    let resolver = contract
        .get("working_directory_resolver")
        .and_then(Value::as_object)
        .expect("bundled contract must publish working_directory_resolver");
    assert_eq!(
        resolver.get("operation").and_then(Value::as_str),
        Some("replace"),
        "working_directory_resolver.operation"
    );
    let canonical = resolver
        .get("canonical_root")
        .and_then(Value::as_str)
        .expect("working_directory_resolver.canonical_root");
    let by_surface = resolver
        .get("by_surface")
        .and_then(Value::as_object)
        .expect("working_directory_resolver.by_surface");
    let source = require_typed_surface(by_surface, "source", "repository_relative");
    let claude_plugin = require_typed_surface(by_surface, "claude_plugin", "host_path_template");
    let agent_plugin = require_typed_surface(by_surface, "agent_plugin", "skill_relative");
    assert_eq!(source, canonical, "source surface must echo canonical_root");
    assert!(
        claude_plugin.contains("${CLAUDE_PLUGIN_ROOT}"),
        "claude_plugin is a host path template, not a filesystem path"
    );

    let replacement = json!({ "working_directory": agent_plugin });
    let replacement_components = require_source_repo_relative(&replacement);
    let skill_root = package.join("skills/assay-golden-path");
    let target = join_classified(&skill_root, &replacement_components);
    assert_stays_inside_package(&package, &target);
    assert!(
        target.exists(),
        "resolved agent-plugin working_directory is absent from the package: {}",
        target.display()
    );

    let steps = contract["steps"]
        .as_array()
        .expect("bundled contract steps");
    let mut saw_working_directory = false;
    for step in steps {
        if step.get("working_directory").is_none() {
            continue;
        }
        saw_working_directory = true;
        let components = require_source_repo_relative(step);
        let working_directory = step["working_directory"]
            .as_str()
            .expect("working_directory string");
        assert_eq!(
            working_directory, canonical,
            "shipped replace rule accepts only an exact canonical_root match"
        );
        assert_eq!(
            components,
            require_source_repo_relative(&json!({ "working_directory": canonical })),
            "step working_directory and canonical_root must classify identically"
        );
    }
    assert!(
        saw_working_directory,
        "bundled contract has no working_directory steps"
    );
}

#[test]
fn portable_resolver_hostile_replacement_reaches_shared_classifier() {
    let error = classify_working_directory(&json!({
        "working_directory": "../escape",
    }))
    .expect_err("hostile replacement root must be rejected by classify_working_directory");
    assert!(
        error.contains("working_directory"),
        "hostile diagnostic did not name working_directory: {error}"
    );
}
