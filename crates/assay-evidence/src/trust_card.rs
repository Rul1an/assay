//! Trust Card (T1b): deterministic render layer over [`TrustBasis`](crate::TrustBasis).
//!
//! Semantics come only from `generate_trust_basis`; this module maps and serializes.

use crate::trust_basis::{TrustBasis, TrustBasisClaim};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Trust Card schema: `5` is the ten-claim surface after the eval, inventory,
/// and decision receipt boundary claims became visible.
pub const TRUST_CARD_SCHEMA_VERSION: u32 = 5;

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
    md_cell(&note_text(note))
}

fn note_text(note: &Option<String>) -> String {
    let trimmed = note.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    match trimmed {
        Some(s) => s.to_string(),
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

fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\r' | '\n' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

const TRUST_CARD_HTML_CSP: &str = concat!(
    "default-src 'none'; ",
    "style-src 'unsafe-inline'; ",
    "script-src 'none'; ",
    "connect-src 'none'; ",
    "img-src 'none'; ",
    "font-src 'none'; ",
    "object-src 'none'; ",
    "base-uri 'none'; ",
    "form-action 'none'"
);

const TRUST_CARD_HTML_CSS: &str = r#":root{
  color-scheme:light dark;
  --ink:#18211f;
  --muted:#5e6b66;
  --line:#d8dfdc;
  --paper:#fbfaf5;
  --panel:#ffffff;
  --accent:#2f6f62;
  --soft:#edf4f1;
  --shadow:0 18px 50px rgba(24,33,31,.08);
}
*{box-sizing:border-box}
body{
  margin:0;
  background:
    radial-gradient(circle at 10% 0%,rgba(47,111,98,.12),transparent 32rem),
    linear-gradient(135deg,#fbfaf5 0%,#eef5f1 100%);
  color:var(--ink);
  font-family:Georgia,"Iowan Old Style","New York",serif;
  line-height:1.5;
}
main{max-width:1120px;margin:0 auto;padding:48px 24px}
section{background:var(--panel);border:1px solid var(--line);border-radius:18px;box-shadow:var(--shadow);padding:28px;margin:24px 0}
h1{font-size:clamp(2rem,6vw,3rem);line-height:1.02;margin:0 0 10px;letter-spacing:-.03em}
h2{font-size:1.375rem;margin:0 0 16px}
.lede{color:var(--muted);max-width:760px}
.meta{display:inline-block;background:var(--soft);border:1px solid var(--line);border-radius:999px;padding:6px 12px;color:var(--accent);font:600 .8125rem ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.skip-link{position:absolute;left:1rem;top:1rem;transform:translateY(-200%);background:var(--panel);border:2px solid var(--accent);border-radius:999px;color:var(--ink);padding:.5rem .875rem;text-decoration:none}
.skip-link:focus-visible{transform:none;outline:3px solid var(--accent);outline-offset:3px}
.table-wrap{overflow-x:auto;border:1px solid var(--line);border-radius:14px;background:var(--panel);scrollbar-gutter:stable}
.table-wrap:focus-visible{outline:3px solid var(--accent);outline-offset:4px}
table{width:100%;border-collapse:collapse;font-size:.875rem;background:var(--panel)}
caption{padding:14px 12px;text-align:left;color:var(--muted);font-size:.875rem;border-bottom:1px solid var(--line)}
th,td{border-bottom:1px solid var(--line);padding:12px;text-align:left;vertical-align:top}
th{font:700 .75rem ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);background:var(--soft)}
td code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.8125rem;word-break:break-word}
.level{font-weight:700;color:var(--accent)}
ul{margin:0;padding-left:22px}
.footnote{color:var(--muted);font-size:.8125rem}
.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}
@media (prefers-color-scheme: dark){
  :root{--ink:#edf5f1;--muted:#a8bbb3;--line:#344943;--paper:#111917;--panel:#17211e;--accent:#8ed8c4;--soft:#21322d;--shadow:0 18px 50px rgba(0,0,0,.35)}
  body{background:radial-gradient(circle at 10% 0%,rgba(142,216,196,.14),transparent 32rem),linear-gradient(135deg,#101816 0%,#182622 100%)}
}
@media (forced-colors: active){
  body{background:Canvas;color:CanvasText}
  section,.table-wrap,.meta{border:1px solid CanvasText;box-shadow:none}
  .skip-link,.table-wrap:focus-visible{outline:3px solid Highlight}
  th{background:Canvas;color:CanvasText}
  .level,.meta{color:CanvasText}
}
@media (max-width:760px){
  main{padding:28px 14px}
  section{padding:18px;border-radius:14px}
  th,td{padding:10px}
}
@media print{
  body{background:#fff;color:#000}
  main{max-width:none;padding:0}
  section{box-shadow:none;border-color:#999;break-inside:avoid}
  .skip-link{display:none}
  .table-wrap{overflow:visible}
}
"#;

/// Secondary single-file human view. JSON remains canonical; HTML adds no claim semantics.
pub fn trust_card_to_html(card: &TrustCard) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n");
    out.push_str("<html lang=\"en\">\n");
    out.push_str("<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    out.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
    out.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"");
    out.push_str(TRUST_CARD_HTML_CSP);
    out.push_str("\">\n");
    out.push_str("<title>Assay Trust Card</title>\n");
    out.push_str("<style>\n");
    out.push_str(TRUST_CARD_HTML_CSS);
    out.push_str("</style>\n");
    out.push_str("</head>\n");
    out.push_str("<body>\n");
    out.push_str("<a class=\"skip-link\" href=\"#claims-heading\">Skip to claims</a>\n");
    out.push_str("<main>\n");
    out.push_str("<header>\n");
    out.push_str("<p class=\"meta\">Trust Card schema ");
    out.push_str(&card.schema_version.to_string());
    out.push_str("</p>\n");
    out.push_str("<h1>Trust card</h1>\n");
    out.push_str("<p class=\"lede\">Static projection of the canonical trustcard.json artifact. This page renders the same claim rows and non-goals for review; it does not add scores, badges, or a second classifier.</p>\n");
    out.push_str("</header>\n");
    out.push_str(
        "<section aria-labelledby=\"claims-heading\" aria-describedby=\"claims-description\">\n",
    );
    out.push_str("<h2 id=\"claims-heading\">Claims</h2>\n");
    out.push_str("<p id=\"claims-description\" class=\"lede\">Rows are rendered from the canonical Trust Card claim list. Reviewers should key by stable claim id, not row position.</p>\n");
    out.push_str("<div class=\"table-wrap\" role=\"region\" aria-label=\"Scrollable Trust Card claims table\" tabindex=\"0\">\n");
    out.push_str("<table>\n");
    out.push_str("<caption>Trust Basis claim rows from trustcard.json</caption>\n");
    out.push_str("<thead><tr><th scope=\"col\">id</th><th scope=\"col\">level</th><th scope=\"col\">source</th><th scope=\"col\">boundary</th><th scope=\"col\">note</th></tr></thead>\n");
    out.push_str("<tbody>\n");
    for claim in &card.claims {
        let id = serde_cell_string(&claim.id);
        let level = serde_cell_string(&claim.level);
        let source = serde_cell_string(&claim.source);
        let boundary = serde_cell_string(&claim.boundary);
        let note = note_text(&claim.note);
        out.push_str("<tr data-claim-id=\"");
        out.push_str(&html_escape(&id));
        out.push_str("\"><td data-label=\"id\"><code>");
        out.push_str(&html_escape(&id));
        out.push_str("</code></td><td data-label=\"level\" class=\"level\">");
        out.push_str(&html_escape(&level));
        out.push_str("</td><td data-label=\"source\"><code>");
        out.push_str(&html_escape(&source));
        out.push_str("</code></td><td data-label=\"boundary\"><code>");
        out.push_str(&html_escape(&boundary));
        out.push_str("</code></td><td data-label=\"note\">");
        out.push_str(&html_escape(&note));
        out.push_str("</td></tr>\n");
    }
    out.push_str("</tbody>\n");
    out.push_str("</table>\n");
    out.push_str("</div>\n");
    out.push_str("</section>\n");
    out.push_str("<section aria-labelledby=\"non-goals-heading\">\n");
    out.push_str("<h2 id=\"non-goals-heading\">Non-goals</h2>\n");
    out.push_str("<ul>\n");
    for line in &card.non_goals {
        out.push_str("<li>");
        out.push_str(&html_escape(line));
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");
    out.push_str("</section>\n");
    out.push_str("<p class=\"footnote\">Canonical source of truth: trustcard.json. Markdown and HTML are deterministic projections.</p>\n");
    out.push_str("</main>\n");
    out.push_str("</body>\n");
    out.push_str("</html>\n");
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
        assert_eq!(html.matches("data-claim-id=").count(), 10);
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
}
