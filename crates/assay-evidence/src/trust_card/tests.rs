use super::*;
use crate::bundle::BundleWriter;
use crate::bundle::VerifyLimits;
use crate::trust_basis::{
    generate_trust_basis, to_canonical_json_bytes, TrustBasisClaim, TrustBasisOptions,
    TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource,
};
use crate::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::io::Cursor;

/// Must match `TrustBasis::claims` order from [`crate::trust_basis::generate_trust_basis`].
const FROZEN_TRUST_BASIS_CLAIM_ID_ORDER: [TrustClaimId; 10] = [
    TrustClaimId::BundleVerified,
    TrustClaimId::SigningEvidencePresent,
    TrustClaimId::ProvenanceBackedClaimsPresent,
    TrustClaimId::DelegationContextVisible,
    TrustClaimId::AuthorizationContextVisible,
    TrustClaimId::ContainmentDegradationObserved,
    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
    TrustClaimId::ExternalDecisionReceiptBoundaryVisible,
    TrustClaimId::ExternalInventoryReceiptBoundaryVisible,
    TrustClaimId::AppliedPackFindingsPresent,
];

fn markdown_table_id_column(md: &str) -> Vec<String> {
    let mut lines = md.lines();
    let mut after_sep = false;
    let mut out = Vec::new();
    for line in lines.by_ref() {
        if line.contains("| id | level | source | boundary | note |") {
            continue;
        }
        if line.contains("| --- | --- | --- | --- | --- |") {
            after_sep = true;
            continue;
        }
        if !after_sep {
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        if line.starts_with("##") {
            break;
        }
        if !line.starts_with('|') {
            continue;
        }
        let parts: Vec<&str> = line
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() >= 5 {
            out.push(parts[0].to_string());
        }
    }
    out
}

fn html_table_claim_ids(html: &str) -> Vec<String> {
    html.split("data-claim-id=\"")
        .skip(1)
        .map(|tail| tail.split_once('"').expect("claim id attribute").0)
        .map(str::to_string)
        .collect()
}

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:trust-card", run_id, seq, payload);
    event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    event
}

fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("bundle should finish");
    buffer
}

#[test]
fn trust_card_markdown_note_strips_crlf_for_single_line_cell() {
    let card = TrustCard {
        schema_version: TRUST_CARD_SCHEMA_VERSION,
        claims: vec![TrustBasisClaim {
            id: TrustClaimId::BundleVerified,
            level: TrustClaimLevel::Verified,
            source: TrustClaimSource::BundleVerification,
            boundary: TrustClaimBoundary::BundleWide,
            note: Some("a\r\nb".to_string()),
        }],
        non_goals: TRUST_CARD_NON_GOALS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    let md = trust_card_to_markdown(&card);
    assert!(
        !md.contains('\r'),
        "markdown table must not contain raw carriage returns"
    );
}

#[test]
fn trust_card_non_goals_match_const_golden() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_golden",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    assert_eq!(card.non_goals.len(), 3);
    for (a, b) in card.non_goals.iter().zip(TRUST_CARD_NON_GOALS.iter()) {
        assert_eq!(a, b);
    }
}

#[test]
fn trust_card_json_always_exactly_ten_frozen_claim_ids_once_in_trust_basis_order() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_seven",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);

    assert_eq!(card.claims.len(), 10);
    let ids: Vec<TrustClaimId> = card.claims.iter().map(|c| c.id).collect();
    assert_eq!(ids, FROZEN_TRUST_BASIS_CLAIM_ID_ORDER);
}

#[test]
fn trust_card_json_top_level_has_only_schema_version_claims_non_goals() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_keys",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let v: serde_json::Value =
        serde_json::from_slice(&trust_card_to_canonical_json_bytes(&card).expect("json"))
            .expect("parse");
    let obj = v.as_object().expect("object");
    let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["claims", "non_goals", "schema_version"],
        "trustcard.json must not gain section_id, hashes, display_order, summary, etc."
    );
}

#[test]
fn trust_card_markdown_table_rows_follow_claim_id_order_not_sorted() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_md_order",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let md = trust_card_to_markdown(&card);
    let col = markdown_table_id_column(&md);
    let expected: Vec<String> = FROZEN_TRUST_BASIS_CLAIM_ID_ORDER
        .iter()
        .map(super::serde_cell_string)
        .collect();
    assert_eq!(
        col, expected,
        "table rows must follow TrustBasis order, not alphabetical or regrouped"
    );
}

#[test]
fn trust_card_html_table_rows_follow_claim_id_order_not_sorted() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_html_order",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let html = trust_card_to_html(&card);
    let ids = html_table_claim_ids(&html);
    let expected: Vec<String> = FROZEN_TRUST_BASIS_CLAIM_ID_ORDER
        .iter()
        .map(super::serde_cell_string)
        .collect();
    assert_eq!(
        ids, expected,
        "html rows must follow TrustBasis order, not alphabetical or regrouped"
    );
}

#[test]
fn trust_card_html_is_static_projection_without_remote_assets_or_script() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_html_static",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let html = trust_card_to_html(&card);
    assert!(html.starts_with("<!doctype html>\n"));
    assert_eq!(html.matches("data-claim-id=").count(), card.claims.len());
    assert!(
        !html.contains("<script"),
        "static Trust Card HTML must not require script"
    );
    assert!(
        !html.contains(r#"<link rel="stylesheet""#) && !html.contains(r#" rel="stylesheet""#),
        "static Trust Card HTML must not load external stylesheets"
    );
    assert!(
        !html.contains("http://") && !html.contains("https://"),
        "static Trust Card HTML must not contain remote asset URLs"
    );
    assert!(
        html.contains("Content-Security-Policy")
            && html.contains("script-src 'none'")
            && html.contains("connect-src 'none'")
            && html.contains("object-src 'none'")
            && html.contains("base-uri 'none'")
            && html.contains("form-action 'none'"),
        "HTML should carry a static-page CSP boundary"
    );
    assert!(
        html.contains(r#"<meta name="referrer" content="no-referrer">"#),
        "HTML should avoid leaking local artifact paths as referrers"
    );
    assert!(
        html.contains("prefers-color-scheme: dark")
            && html.contains("forced-colors: active")
            && html.contains("@media print"),
        "HTML should respect color-scheme, high-contrast, and print review modes"
    );
    assert!(
        html.contains("<caption>Trust Basis claim rows from trustcard.json</caption>")
            && html.contains(r#"<th scope="col">id</th>"#)
            && html.contains(r#"role="region""#)
            && html.contains(r#"aria-label="Scrollable Trust Card claims table""#),
        "HTML table should expose accessible labels and scroll context"
    );
    assert!(
        html.contains("Canonical source of truth: trustcard.json"),
        "HTML must name JSON as the canonical source"
    );
    assert!(
        html.contains("does not add scores, badges, or a second classifier"),
        "HTML must keep the no-score/no-badge/no-second-classifier boundary visible"
    );
}

#[test]
fn trust_card_html_escapes_notes_and_keeps_non_goals_literal() {
    let card = TrustCard {
        schema_version: TRUST_CARD_SCHEMA_VERSION,
        claims: vec![TrustBasisClaim {
            id: TrustClaimId::BundleVerified,
            level: TrustClaimLevel::Verified,
            source: TrustClaimSource::BundleVerification,
            boundary: TrustClaimBoundary::BundleWide,
            note: Some("<script>alert(\"x\")</script> & ok\r\nnext".to_string()),
        }],
        non_goals: TRUST_CARD_NON_GOALS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    let html = trust_card_to_html(&card);
    assert!(
        !html.contains("<script"),
        "HTML must escape note content, not render raw markup"
    );
    assert!(html.contains("&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; ok  next"));
    for goal in TRUST_CARD_NON_GOALS {
        assert!(
            html.contains(&format!("<li>{goal}</li>")),
            "missing frozen non-goal: {goal}"
        );
    }
}

#[test]
fn trust_card_claim_order_and_serde_match_trust_basis() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_order",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "delegated_from": "agent:x"
        }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);

    let basis_ids: Vec<_> = basis.claims.iter().map(|c| c.id).collect();
    let card_ids: Vec<_> = card.claims.iter().map(|c| c.id).collect();
    assert_eq!(basis_ids, card_ids);

    let tb_json: serde_json::Value =
        serde_json::from_slice(&to_canonical_json_bytes(&basis).expect("tb json")).expect("parse");
    let tc_json: serde_json::Value =
        serde_json::from_slice(&trust_card_to_canonical_json_bytes(&card).expect("tc json"))
            .expect("parse");
    assert_eq!(tc_json["claims"], tb_json["claims"]);
}

#[test]
fn trust_card_markdown_has_only_trust_card_title_table_and_non_goals_section() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_md_shape",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let md = trust_card_to_markdown(&card);
    assert!(md.starts_with("# Trust card\n\n"));
    assert_eq!(
        md.matches("\n## ").count(),
        1,
        "only ## Non-goals after the table; no Summary/Prose/Appendix sections"
    );
    assert!(
        md.contains("\n## Non-goals\n\n"),
        "frozen non-goals heading only"
    );
    let table_rows = md
        .lines()
        .filter(|l| l.starts_with('|') && !l.contains("| id | level |"))
        .filter(|l| !l.contains("| --- |"))
        .count();
    assert_eq!(
        table_rows, 10,
        "schema 5 adds one external decision receipt row; no extra markdown table blocks"
    );
}

#[test]
fn trust_card_schema_version_is_five() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_schema",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    assert_eq!(card.schema_version, 5);
    let v: serde_json::Value =
        serde_json::from_slice(&trust_card_to_canonical_json_bytes(&card).expect("json"))
            .expect("parse");
    assert_eq!(v["schema_version"], json!(5));
}

#[test]
fn trust_card_markdown_note_placeholder_is_frozen() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_note",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let md = trust_card_to_markdown(&card);
    assert!(md.contains(&format!("| {} |", TRUST_CARD_NOTE_EMPTY_PLACEHOLDER)));
}

#[test]
fn trust_card_markdown_non_goals_literal_order() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_md_ng",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let md = trust_card_to_markdown(&card);
    let idx = md.find("## Non-goals").expect("heading");
    let tail = &md[idx..];
    for goal in TRUST_CARD_NON_GOALS {
        assert!(
            tail.contains(&format!("- {goal}")),
            "missing frozen line: {goal}"
        );
    }
}

#[test]
fn trust_card_markdown_table_avoids_judgment_phrases_in_prose_scope() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_banned",
        0,
        json!({ "hits": 1 }),
    )]);
    let basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&basis);
    let md = trust_card_to_markdown(&card);
    let head = markdown_body_before_non_goals(&md);
    let lower = head.to_lowercase();
    assert!(
        !lower.contains("strong posture"),
        "unexpected judgment phrase in table/header"
    );
    assert!(
        !lower.contains(" healthy "),
        "unexpected judgment phrase in table/header"
    );
    assert!(
        !lower.contains("overall trust"),
        "unexpected judgment phrase in table/header (incl. overall-trust summaries)"
    );
    assert!(
        !lower.contains("overall trust is"),
        "no overall-trust summary sentences in table/header"
    );
    assert!(
        !lower.contains("trust posture"),
        "no trust-posture summary in table/header"
    );
    assert!(
        !lower.contains("this bundle has"),
        "no bundle-level trust narrative in table/header"
    );
    assert!(
        !lower.contains("trust is high"),
        "no high-trust summary in table/header"
    );
    assert!(
        !lower.contains("safe/unsafe"),
        "use frozen non-goals section for safe/unsafe wording, not table"
    );
}
