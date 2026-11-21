//! ADR-052: keep the field inventory visible in document projections.
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::{fs, path::Path};

// Compare tokens rather than lines: formatting, comments and literal text do not change a match.
// Descend into macro bodies too; the MCP policy projections are themselves macro-generated.
fn ident(token: &TokenTree, name: &str) -> bool {
    matches!(token, TokenTree::Ident(id) if id == name)
}

fn punct(token: &TokenTree, ch: char) -> bool {
    matches!(token, TokenTree::Punct(p) if p.as_char() == ch)
}

fn violations(source: TokenStream) -> Vec<(usize, &'static str)> {
    fn scan(tokens: Vec<TokenTree>, found: &mut Vec<(usize, &'static str)>) {
        for (i, token) in tokens.iter().enumerate() {
            if let [keyword, discard, colon, ..] = &tokens[i..] {
                if ident(keyword, "let") && ident(discard, "_") && punct(colon, ':') {
                    found.push((keyword.span().start().line, "typed discard"));
                }
            }
            let TokenTree::Group(group) = token else {
                continue;
            };
            let inner: Vec<_> = group.stream().into_iter().collect();
            if let [fields @ .., first, second] = &inner[..] {
                // A record's rest is either its sole pattern or follows a field and comma.
                // Struct updates have an expression after the dots. Error diagnostics may
                // select fields from Refusal: those fields describe a failure, not input.
                let refusal = matches!(&tokens[..i], [.., ty, a, b, TokenTree::Ident(_)]
                    if ident(ty, "Refusal") && punct(a, ':') && punct(b, ':'));
                let after_type = matches!(tokens[..i].last(), Some(TokenTree::Ident(_)))
                    || tokens[..i].last().is_some_and(|last| punct(last, '>'));
                // In macro_rules a repetition can supply the fields and their trailing commas.
                let after_fields = fields.last().is_none_or(|last| punct(last, ','))
                    || matches!(fields, [.., dollar, TokenTree::Group(_), op]
                        if punct(dollar, '$') && (punct(op, '*') || punct(op, '+') || punct(op, '?')));
                if group.delimiter() == Delimiter::Brace
                    && after_type
                    && !refusal
                    && punct(first, '.')
                    && punct(second, '.')
                    && after_fields
                {
                    found.push((first.span().start().line, "rest pattern"));
                }
            }
            scan(inner, found);
        }
    }
    let mut found = Vec::new();
    scan(source.into_iter().collect(), &mut found);
    found
}

#[test]
fn document_projections_name_their_fields() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut errors = Vec::new();
    for file in [
        "assay-cli/src/cli/commands/evidence/skill_supply_chain_capture.rs",
        "assay-cli/src/cli/commands/generate/document.rs",
        "assay-cli/src/cli/commands/profile_types/document.rs",
        "assay-cli/src/policy.rs",
        "assay-common/src/enforce_document.rs",
        "assay-common/src/trust_document.rs",
        "assay-core/src/config.rs",
        "assay-core/src/config/no_effect.rs",
        "assay-core/src/mcp/policy/document.rs",
        "assay-core/src/mcp/policy/types.rs",
        "assay-core/src/model/args_policy_file.rs",
        "assay-core/src/model/policy_file.rs",
        "assay-core/src/model/serde/test_case.rs",
        "assay-evidence/src/store/config.rs",
        "assay-mcp-server/src/tools/policy_decide.rs",
        "assay-registry/src/lockfile_next/types.rs",
    ] {
        let source =
            fs::read_to_string(crates.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        for (line, reason) in violations(source.parse().unwrap()) {
            errors.push(format!("{file}:{line}: {reason}"));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn detects_both_discards_even_inside_a_macro_definition() {
    let source = r#"macro_rules! projection { () => {
        let Policy { caller: Caller { id }, .. } = input;
        let /* explicit type */ _: &Caller = caller;
        let Self { $( $field, )* .. } = other;
    } }"#;
    let found = violations(source.parse().unwrap());
    assert_eq!(
        found,
        [
            (2, "rest pattern"),
            (3, "typed discard"),
            (4, "rest pattern")
        ]
    );
}

#[test]
fn permits_named_delegation_diagnostics_ranges_and_literals() {
    let source = r#"let Policy { caller, allowances: _allowance_gate } = input;
        match err { Refusal::Shape { message, .. } => f(message), _ => () }
        let all = { .. }; let slice = &buffer[..]; let more = Policy { caller, ..base };
        let text = "let _: Caller = caller; Policy { .. }"; // let _: Caller = caller;
    "#;
    assert!(violations(source.parse().unwrap()).is_empty());
}
