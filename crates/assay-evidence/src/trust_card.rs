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
#[cfg(test)]
mod tests;
