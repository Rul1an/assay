//! Trust Card (T1b): deterministic render layer over [`TrustBasis`](crate::TrustBasis).
//!
//! Semantics come only from `generate_trust_basis`; this module maps and serializes.

use crate::trust_basis::{TrustBasis, TrustBasisClaim};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Trust Card schema: `4` adds `external_inventory_receipt_boundary_visible`.
pub const TRUST_CARD_SCHEMA_VERSION: u32 = 4;

/// Markdown table cell when T1a leaves `note` empty (`None` or blank). Do not vary by test/renderer.
pub const TRUST_CARD_NOTE_EMPTY_PLACEHOLDER: &str = "-";

/// Frozen `non_goals` strings (JSON and Markdown). Same order as serialized output.
pub const TRUST_CARD_NON_GOALS: [&str; 3] = [
    "No aggregate trust score",
    "No safe/unsafe badge",
    "No correctness guarantees beyond stated claim boundaries",
];

/// Canonical trust card document: same `claims` serde shape as [`TrustBasis`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustCard {
    pub schema_version: u32,
    pub claims: Vec<TrustBasisClaim>,
    pub non_goals: Vec<String>,
}

/// Map a trust basis to a card: no classification, no filtering.
pub fn trust_basis_to_trust_card(basis: &TrustBasis) -> TrustCard {
    TrustCard {
        schema_version: TRUST_CARD_SCHEMA_VERSION,
        claims: basis.claims.clone(),
        non_goals: TRUST_CARD_NON_GOALS
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
    }
}

/// Pretty JSON with trailing newline, matching [`crate::to_canonical_json_bytes`] for trust basis.
pub fn trust_card_to_canonical_json_bytes(card: &TrustCard) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    card.serialize(&mut serializer)?;
    output.push(b'\n');
    Ok(output)
}

fn serde_cell_string<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("trust card cell serde") {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

fn md_cell(raw: &str) -> String {
    raw.replace('|', "\\|")
        .chars()
        .map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c })
        .collect()
}

fn note_markdown(note: &Option<String>) -> String {
    let trimmed = note.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    match trimmed {
        Some(s) => md_cell(s),
        None => TRUST_CARD_NOTE_EMPTY_PLACEHOLDER.to_string(),
    }
}

/// Secondary human view: fixed five-column table, then frozen Non-goals.
pub fn trust_card_to_markdown(card: &TrustCard) -> String {
    let mut out = String::new();
    out.push_str("# Trust card\n\n");
    out.push_str("| id | level | source | boundary | note |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for claim in &card.claims {
        let line = format!(
            "| {} | {} | {} | {} | {} |\n",
            serde_cell_string(&claim.id),
            serde_cell_string(&claim.level),
            serde_cell_string(&claim.source),
            serde_cell_string(&claim.boundary),
            note_markdown(&claim.note)
        );
        out.push_str(&line);
    }
    out.push_str("\n## Non-goals\n\n");
    for line in &card.non_goals {
        out.push_str("- ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Table + title only: substring guard for judgment language before `## Non-goals`.
#[cfg(test)]
fn markdown_body_before_non_goals(md: &str) -> &str {
    md.split_once("## Non-goals")
        .map(|(before, _)| before)
        .unwrap_or(md)
}

#[cfg(test)]
mod tests {
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
    const FROZEN_TRUST_BASIS_CLAIM_ID_ORDER: [TrustClaimId; 9] = [
        TrustClaimId::BundleVerified,
        TrustClaimId::SigningEvidencePresent,
        TrustClaimId::ProvenanceBackedClaimsPresent,
        TrustClaimId::DelegationContextVisible,
        TrustClaimId::AuthorizationContextVisible,
        TrustClaimId::ContainmentDegradationObserved,
        TrustClaimId::ExternalEvalReceiptBoundaryVisible,
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

    fn make_event(
        type_: &str,
        run_id: &str,
        seq: u64,
        payload: serde_json::Value,
    ) -> EvidenceEvent {
        let mut event =
            EvidenceEvent::new(type_, "urn:assay:test:trust-card", run_id, seq, payload);
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
    fn trust_card_json_always_exactly_nine_frozen_claim_ids_once_in_trust_basis_order() {
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

        assert_eq!(card.claims.len(), 9);
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
            serde_json::from_slice(&to_canonical_json_bytes(&basis).expect("tb json"))
                .expect("parse");
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
            table_rows, 9,
            "schema 4 adds one external inventory receipt row; no extra markdown table blocks"
        );
    }

    #[test]
    fn trust_card_schema_version_is_four() {
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
        assert_eq!(card.schema_version, 4);
        let v: serde_json::Value =
            serde_json::from_slice(&trust_card_to_canonical_json_bytes(&card).expect("json"))
                .expect("parse");
        assert_eq!(v["schema_version"], json!(4));
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
}
