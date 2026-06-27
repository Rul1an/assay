use anyhow::Context;
use assay_core::mcp::proxy::TdtProducer;
use std::path::PathBuf;

/// EXPERIMENTAL: build the opt-in tool-decision-truth carrier producer from the environment, failing
/// closed if the key material is absent or malformed. The HMAC key is read once here, moved into the
/// producer (held in memory only), and removed from this process's environment so the wrapped child
/// server - spawned with the inherited environment - cannot read it. The key is never logged or written
/// to disk.
pub(super) fn build_tdt_producer(out_path: PathBuf) -> anyhow::Result<TdtProducer> {
    const KEY_VAR: &str = "ASSAY_TDT_HMAC_KEY";
    const KEY_ID_VAR: &str = "ASSAY_TDT_HMAC_KEY_ID";

    let key = std::env::var(KEY_VAR).ok();
    let key_id = std::env::var(KEY_ID_VAR).ok();

    // Remove the key (and its id) from this process's environment immediately, before the proxy spawns
    // the wrapped child with an inherited environment, so the child cannot read the key and forge
    // carriers. The values were captured above; validation happens in the pure helper below.
    std::env::remove_var(KEY_VAR);
    std::env::remove_var(KEY_ID_VAR);

    tdt_producer_from_material(out_path, key, key_id)
}

/// Pure fail-closed validator: turn optional key material into a [`TdtProducer`], or a startup error
/// naming exactly what is missing or malformed. Split out from the environment read so the fail-closed
/// contract is testable without mutating process-global state.
fn tdt_producer_from_material(
    out_path: PathBuf,
    key: Option<String>,
    key_id: Option<String>,
) -> anyhow::Result<TdtProducer> {
    let key = key.ok_or_else(|| {
        anyhow::anyhow!(
            "--tool-decision-truth-out is set but ASSAY_TDT_HMAC_KEY is missing; the tool-decision-truth producer fails closed. Set the HMAC key in the environment (never on the command line)."
        )
    })?;
    let key_id = key_id.ok_or_else(|| {
        anyhow::anyhow!(
            "--tool-decision-truth-out is set but ASSAY_TDT_HMAC_KEY_ID is missing; the tool-decision-truth producer fails closed."
        )
    })?;
    if key.is_empty() {
        anyhow::bail!(
            "ASSAY_TDT_HMAC_KEY is empty; the tool-decision-truth producer fails closed."
        );
    }
    // key_id must match the digest-prefix charset the carrier binds (`[A-Za-z0-9._-]`, non-empty), so the
    // minted args_digest is well-formed and verifiable downstream. This mirrors the core `args_digest`
    // guard, but here it fails closed loudly at startup instead of silently dropping carriers per call.
    let key_id_ok = !key_id.is_empty()
        && key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !key_id_ok {
        anyhow::bail!(
            "ASSAY_TDT_HMAC_KEY_ID is empty or malformed (allowed characters: A-Z a-z 0-9 . _ -); the tool-decision-truth producer fails closed."
        );
    }
    // Fail closed at startup when the opted-in sink cannot be opened. Otherwise a run could proceed with
    // `--tool-decision-truth-out` enabled while minting no carriers, which is a half-configured producer.
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .with_context(|| {
            format!(
                "tool-decision-truth sink is not writable at {}; the producer fails closed",
                out_path.display()
            )
        })?;
    Ok(TdtProducer::new(out_path, key.into_bytes(), key_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn tdt_producer_fails_closed_when_key_missing() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            None,
            Some("kid-v0".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ASSAY_TDT_HMAC_KEY is missing"), "got: {err}");
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_empty() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some(String::new()),
            Some("kid-v0".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ASSAY_TDT_HMAC_KEY is empty"), "got: {err}");
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_id_missing() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some("k".into()),
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("ASSAY_TDT_HMAC_KEY_ID is missing"),
            "got: {err}"
        );
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_id_malformed() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some("k".into()),
            Some("bad:id".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed"), "got: {err}");
    }

    #[test]
    fn tdt_producer_accepts_valid_material_and_debug_redacts_key() {
        let dir = tempdir().unwrap();
        let producer = tdt_producer_from_material(
            dir.path().join("carriers.ndjson"),
            Some("super-secret-key".into()),
            Some("kid-v0".into()),
        )
        .expect("valid material");
        let dbg = format!("{producer:?}");
        assert!(
            dbg.contains("<redacted>"),
            "Debug must redact the key: {dbg}"
        );
        assert!(
            !dbg.contains("super-secret-key"),
            "Debug must not leak the key: {dbg}"
        );
    }

    #[test]
    fn tdt_producer_fails_closed_when_sink_cannot_be_opened() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("missing-parent").join("carriers.ndjson");
        let err =
            tdt_producer_from_material(out, Some("super-secret-key".into()), Some("kid-v0".into()))
                .unwrap_err()
                .to_string();
        assert!(
            err.contains("tool-decision-truth sink is not writable"),
            "got: {err}"
        );
    }

    #[test]
    fn producer_carrier_line_roundtrips_import_verify_project() {
        use crate::cli::args::ProjectOtelArgs;
        use crate::cli::commands::evidence::tool_decision_truth::{
            cmd_tool_decision_truth, ToolDecisionTruthArgs,
        };
        use crate::cli::commands::evidence::verify_tool_decision_truth::{
            cmd_verify_tool_decision_truth, VerifyFormat, VerifyToolDecisionTruthArgs,
        };
        use crate::cli::commands::project_otel;
        use assay_core::mcp::policy::McpPolicy;
        use assay_core::mcp::tool_decision_truth as tdt_core;
        use assay_core::mcp::tool_decision_truth::DecisionEvidence;

        let policy: McpPolicy = serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap();

        // Build a carrier exactly as the live producer does (same builder + same producer arguments) and
        // write it as one NDJSON line, mirroring the producer sink.
        let carrier = tdt_core::build_classified_record(
            &policy,
            "deploy",
            &json!({"env": "prod", "trace": "ZZSENTINELRAWZZ"}),
            0,
            b"producer-test-key-v0",
            "fixture-kid-v0",
            "authoritative_boundary",
            "call-0",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .expect("carrier builds");

        let dir = tempdir().unwrap();
        let sink = dir.path().join("carriers.ndjson");
        std::fs::write(
            &sink,
            format!("{}\n", serde_json::to_string(&carrier).unwrap()),
        )
        .unwrap();

        // Extract one line; PR9a imports a single carrier JSON (multi-carrier import is out of scope).
        let body = std::fs::read_to_string(&sink).unwrap();
        let line = body.lines().next().unwrap();
        let carrier_json = dir.path().join("carrier.json");
        std::fs::write(&carrier_json, line).unwrap();

        let bundle = dir.path().join("tdt.tar.gz");
        let code = cmd_tool_decision_truth(ToolDecisionTruthArgs {
            carrier: carrier_json,
            bundle_out: bundle.clone(),
            run_id: "producer-roundtrip".to_string(),
            import_time: Some("2026-06-19T00:00:00Z".to_string()),
        })
        .expect("import runs");
        assert_eq!(code, 0, "import of a producer carrier line should succeed");

        let vcode = cmd_verify_tool_decision_truth(VerifyToolDecisionTruthArgs {
            bundle: bundle.clone(),
            format: VerifyFormat::Json,
        })
        .expect("verify runs");
        assert_eq!(
            vcode, 0,
            "verify should report ok for a producer-emitted carrier"
        );

        let proj = dir.path().join("projection.json");
        let pcode = project_otel::run(ProjectOtelArgs {
            capability_surface: None,
            evidence_bundle: Some(bundle),
            observation_health: None,
            enforcement_health: None,
            out: Some(proj.clone()),
        })
        .expect("project runs");
        assert_eq!(pcode, 0, "projection over verified evidence should succeed");
        let projection = std::fs::read_to_string(&proj).unwrap();
        assert!(
            projection.contains("assay.tdt."),
            "projection should carry tdt identity attributes: {projection}"
        );
        assert!(
            !projection.contains("ZZSENTINELRAWZZ"),
            "projection must not carry raw arguments"
        );
    }
}
