//! EXPERIMENTAL: project a verified skill supply-chain bundle into a CycloneDX 1.6 AI-BOM (ASBOM).
//!
//! This is a view over VERIFIED evidence, never a best-effort extractor: the bundle is verified with
//! the same gate as `verify-skill-supply-chain` first, and nothing is written if any carrier fails.
//! The projection is an INVENTORY, not an assertion of safety: the reviewed skill is a `file`
//! component, declared packages/skills are `library` components, services are `service` entries, and
//! the bounded verdict + coverage + non-claims travel as namespaced `properties`. There is deliberately
//! no trust score and no `vulnerabilities`/VEX block, because the carrier asserts neither CVEs nor
//! safety — only review sufficiency at the reviewed boundary.

use super::skill_supply_chain::CARRIER_EVENT_TYPE;
use crate::exit_codes;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::fs::File;
use std::path::PathBuf;

const CDX_PROPERTY_NS: &str = "assay:skill_supply_chain";

#[derive(Debug, clap::Args, Clone)]
pub struct ProjectSkillBomArgs {
    /// Evidence bundle (.tar.gz) with skill supply-chain carrier events
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Where to write the CycloneDX BOM JSON (stdout if omitted)
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}

pub fn cmd_project_skill_bom(args: ProjectSkillBomArgs) -> Result<i32> {
    let file = match File::open(&args.bundle) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: cannot open bundle {}: {e}", args.bundle.display());
            return Ok(exit_codes::EXIT_CONFIG_ERROR);
        }
    };
    let reader = match assay_evidence::bundle::BundleReader::open(file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: bundle integrity verification failed: {e}");
            return Ok(exit_codes::EXIT_CONFIG_ERROR);
        }
    };
    let events = reader
        .events_vec()
        .context("failed to read bundle events")?;

    // Verify FULLY before projecting, with the SAME semantics as `verify-skill-supply-chain`: this
    // includes duplicate-root rejection, so a bundle that fails verification (e.g. ambiguous roots) is
    // never projected into a BOM with duplicate bom-refs. Projection is a view over verified evidence.
    let report = super::verify_skill_supply_chain::build_report(&events);
    if !report.ok {
        eprintln!("error: skill supply-chain verification failed; refusing to project unverified evidence");
        for check in report.checks.iter().filter(|c| !c.ok) {
            eprintln!("  fail: {} — {}", check.id, check.detail);
        }
        return Ok(exit_codes::EXIT_CONFIG_ERROR);
    }

    let carriers: Vec<&Value> = events
        .iter()
        .filter(|e| e.type_ == CARRIER_EVENT_TYPE)
        .map(|e| &e.payload)
        .collect();

    let bom = project_bom(&carriers);
    let json = serde_json::to_string_pretty(&bom)?;
    match &args.out {
        Some(path) => {
            fs::write(path, format!("{json}\n"))
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Projected CycloneDX AI-BOM to {}", path.display());
        }
        None => println!("{json}"),
    }
    Ok(exit_codes::OK)
}

fn project_bom(carriers: &[&Value]) -> Value {
    let mut components: Vec<Value> = Vec::new();
    let mut dependencies: Vec<Value> = Vec::new();

    for carrier in carriers {
        let root_name = carrier["root"]["name"].as_str().unwrap_or("unknown");
        let root_path = carrier["root"]["path"].as_str().unwrap_or("");
        let root_ref = format!("skill:{root_path}");

        let mut properties = vec![
            prop("verdict", carrier["verdict"].as_str().unwrap_or("")),
            prop("reason_codes", &join_strs(carrier.get("reason_codes"))),
        ];
        if let Some(cov) = carrier.get("coverage").and_then(Value::as_object) {
            for (k, v) in cov {
                properties.push(prop(&format!("coverage.{k}"), v.as_str().unwrap_or("")));
            }
        }
        for nc in carrier
            .get("non_claims")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            properties.push(prop("non_claim", nc));
        }
        for signal in carrier
            .get("signals")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let kind = signal.get("kind").and_then(Value::as_str).unwrap_or("");
            let sc = signal
                .get("source_class")
                .and_then(Value::as_str)
                .unwrap_or("");
            properties.push(prop("signal", &format!("{kind}:{sc}")));
        }

        components.push(json!({
            "type": "file",
            "bom-ref": root_ref,
            "name": root_name,
            "properties": properties,
        }));

        // Declared dependencies become inventory components + a dependency edge from the root.
        let mut edges: Vec<String> = Vec::new();
        let deps = carrier.get("declared_dependencies");
        for pkg in channel(deps, "packages") {
            let name = pkg.get("name").and_then(Value::as_str).unwrap_or("unknown");
            let dep_ref = format!("pkg:{name}");
            let mut comp = json!({"type": "library", "bom-ref": dep_ref, "name": name});
            // Capture treats any non-null scalar version as declared evidence (YAML parses a bare
            // `2.0` as a number), so the BOM must not drop a numeric version — coerce it to a string.
            if let Some(v) = version_string(pkg.get("version")) {
                comp["version"] = json!(v);
            }
            components.push(comp);
            edges.push(dep_ref);
        }
        for skill in channel(deps, "skills") {
            let name = skill
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let dep_ref = format!("skill-dep:{name}");
            components.push(json!({"type": "file", "bom-ref": dep_ref, "name": name}));
            edges.push(dep_ref);
        }
        for svc in channel(deps, "services") {
            let name = svc.get("name").and_then(Value::as_str).unwrap_or("unknown");
            let dep_ref = format!("service:{name}");
            components.push(json!({"type": "service", "bom-ref": dep_ref, "name": name}));
            edges.push(dep_ref);
        }
        dependencies.push(json!({"ref": root_ref, "dependsOn": edges}));
    }

    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "metadata": {
            "properties": [prop("projector", "assay-skill-supply-chain"), prop("bom_kind", "ASBOM")],
        },
        "components": components,
        "dependencies": dependencies,
    })
}

/// A declared version rendered as a string: a non-empty JSON string as-is, a JSON number stringified
/// (the bundle's JCS canonicalization has already normalized it, so `2.0` reads back as `2`). Null,
/// empty, or other types yield `None` so no `version` field is emitted.
fn version_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn channel<'a>(deps: Option<&'a Value>, name: &str) -> Vec<&'a Value> {
    deps.and_then(|d| d.get(name))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn prop(name: &str, value: &str) -> Value {
    json!({"name": format!("{CDX_PROPERTY_NS}:{name}"), "value": value})
}

fn join_strs(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::evidence::skill_supply_chain::tests::{run_import, sample_carrier};
    use serde_json::json;

    fn project(bundle: &std::path::Path, out: &std::path::Path) -> i32 {
        cmd_project_skill_bom(ProjectSkillBomArgs {
            bundle: bundle.to_path_buf(),
            out: Some(out.to_path_buf()),
        })
        .unwrap()
    }

    #[test]
    fn projects_valid_cyclonedx_1_6_bom() {
        let mut carrier = sample_carrier();
        carrier["declared_dependencies"] = json!({
            "packages": [{"name": "requests", "version": "2.0"}],
            "services": [{"name": "payments", "endpoint": "https://x"}],
            "skills": []
        });
        let (bundle, dir) = run_import(&carrier, "bom_test").unwrap();
        let out = dir.path().join("bom.json");
        assert_eq!(project(&bundle, &out), 0);

        let bom: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(bom["bomFormat"], "CycloneDX");
        assert_eq!(bom["specVersion"], "1.6");
        assert_eq!(bom["version"], 1);

        let comps = bom["components"].as_array().unwrap();
        // root file + one library + one service
        assert!(comps
            .iter()
            .any(|c| c["type"] == "file" && c["name"] == "release-notes"));
        assert!(comps
            .iter()
            .any(|c| c["type"] == "library" && c["name"] == "requests"));
        assert!(comps
            .iter()
            .any(|c| c["type"] == "service" && c["name"] == "payments"));

        // verdict rides as a namespaced property, never a trust score.
        let root = comps.iter().find(|c| c["type"] == "file").unwrap();
        let props = root["properties"].as_array().unwrap();
        assert!(props
            .iter()
            .any(|p| p["name"] == "assay:skill_supply_chain:verdict"
                && p["value"] == "review_complete"));
        // no vulnerabilities / VEX block, no score field anywhere.
        assert!(bom.get("vulnerabilities").is_none());
        let serialized = serde_json::to_string(&bom).unwrap();
        assert!(!serialized.contains("\"score\""));
        assert!(!serialized.to_lowercase().contains("\"safe\""));
    }

    #[test]
    fn refuses_to_project_incoherent_carrier() {
        // Build a bundle whose carrier bypassed the import gate (verdict/reason mismatch).
        use crate::cli::commands::evidence::skill_supply_chain::CARRIER_EVENT_TYPE;
        use assay_evidence::bundle::BundleWriter;
        use assay_evidence::types::{EvidenceEvent, ProducerMeta};
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bad.tar.gz");
        let producer = ProducerMeta {
            name: "t".into(),
            version: "0".into(),
            git: None,
        };
        let mut bad = sample_carrier();
        bad["verdict"] = json!("transitive_risk_present");
        let mut w = BundleWriter::new(std::fs::File::create(&bundle).unwrap())
            .with_producer(producer.clone());
        w.add_event(
            EvidenceEvent::new(CARRIER_EVENT_TYPE, "urn:t", "t", 0, bad).with_producer(&producer),
        );
        w.finish().unwrap();
        let out = dir.path().join("bom.json");
        assert_eq!(project(&bundle, &out), exit_codes::EXIT_CONFIG_ERROR);
        assert!(!out.exists());
    }

    #[test]
    fn refuses_to_project_bundle_with_duplicate_root_identity() {
        // Two coherent carriers for the SAME root are ambiguous: the full verifier rejects them, so
        // projection must too (no BOM with duplicate bom-refs). Regression for the verify-before-project
        // gap where per-carrier validation alone would have let this through.
        use crate::cli::commands::evidence::skill_supply_chain::CARRIER_EVENT_TYPE;
        use assay_evidence::bundle::BundleWriter;
        use assay_evidence::types::{EvidenceEvent, ProducerMeta};
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("dup.tar.gz");
        let producer = ProducerMeta {
            name: "t".into(),
            version: "0".into(),
            git: None,
        };
        let mut w = BundleWriter::new(std::fs::File::create(&bundle).unwrap())
            .with_producer(producer.clone());
        for seq in 0..2 {
            w.add_event(
                EvidenceEvent::new(CARRIER_EVENT_TYPE, "urn:t", "t", seq, sample_carrier())
                    .with_producer(&producer),
            );
        }
        w.finish().unwrap();
        let out = dir.path().join("bom.json");
        assert_eq!(project(&bundle, &out), exit_codes::EXIT_CONFIG_ERROR);
        assert!(!out.exists());
    }

    #[test]
    fn numeric_package_version_is_projected_as_a_string() {
        // Regression for the numeric-version projection loss: capture accepts a YAML numeric version as
        // declared evidence, so the BOM must render it (as a string), not drop it. The bundle's JCS
        // canonicalization renders 2.0 as 2 (and 2 == 2.0 per the numeric-equality contract edge); the
        // point of the regression is that the version is PRESENT, not the exact literal.
        let mut carrier = sample_carrier();
        carrier["declared_dependencies"] = json!({
            "packages": [{"name": "requests", "version": 2.0}],
            "services": [],
            "skills": []
        });
        let (bundle, dir) = run_import(&carrier, "ver_test").unwrap();
        let out = dir.path().join("bom.json");
        assert_eq!(project(&bundle, &out), 0);
        let bom: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let lib = bom["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "library" && c["name"] == "requests")
            .expect("library component present");
        assert_eq!(lib["version"], "2");

        // A fractional version survives JCS unchanged and is rendered verbatim.
        let mut c2 = sample_carrier();
        c2["declared_dependencies"] =
            json!({"packages": [{"name": "flask", "version": 2.5}], "services": [], "skills": []});
        let (bundle2, dir2) = run_import(&c2, "ver_test2").unwrap();
        let out2 = dir2.path().join("bom.json");
        assert_eq!(project(&bundle2, &out2), 0);
        let bom2: Value = serde_json::from_str(&std::fs::read_to_string(&out2).unwrap()).unwrap();
        let lib2 = bom2["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "library" && c["name"] == "flask")
            .expect("library component present");
        assert_eq!(lib2["version"], "2.5");
    }
}
