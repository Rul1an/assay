//! The two sequence evaluators must agree on whether a trace violates a rule.
//!
//! `assay-mcp-server::tools::check_sequence` still carries its own copy of the rule language;
//! `assay_core::sequence_eval` is the shared one `assay-metrics` now calls. CLAUDE.md sanctions a
//! parity test only "when one rule cannot simply call the other", and here it can — this crate
//! already depends on `assay-core`. So this is the weaker option, held until the call-through
//! lands, because the copy's JSON violation shape is a published tool contract.
//!
//! It is not a formality. A differential over every sequence of length <= 5 on a three-symbol
//! alphabet found 213 `after` disagreements between the two, in both directions: violations the
//! shared copy reported as `held` with a span that omitted the failing call, and violations it
//! invented that the proxy allows. Both are fixed; this is what stops them coming back.
//!
//! Parity is **directional**. The shared evaluator may be stricter than the copy — it has fixes
//! the copy cannot take without changing its published JSON — but it must never be more lenient,
//! because it is the one `assay run` reports from. The stricter cases are counted so that the
//! count moving is itself a signal.
//!
//! Parity is on the **verdict**, not the prose. The copies phrase their messages differently and
//! always have, and pinning text here would fail on a wording change that breaks nothing.
//!
//! `TraceExtent::Partial` is the extent under test, because `check_sequence` validates a live
//! proxy's history-so-far: it asks whether what has happened *so far* violates the rule, with more
//! calls still possible. The metric asks the finished-run question. They are different questions
//! and only the first one is this file's business.

use assay_core::model::{Policy, SequenceRule};
use assay_core::sequence_eval::{evaluate_rules, TraceExtent};

/// Every sequence of length <= 4 over a fixed alphabet, so a divergence anywhere in the space is
/// found rather than only at the cases someone thought to write down.
fn all_traces(alphabet: &[&str], max_len: usize) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::<String>::new()];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for t in &frontier {
            for sym in alphabet {
                let mut c = t.clone();
                c.push((*sym).to_string());
                next.push(c);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

fn rules_under_test() -> Vec<SequenceRule> {
    vec![
        SequenceRule::Require { tool: "A".into() },
        SequenceRule::Blocklist {
            pattern: "B".into(),
        },
        SequenceRule::Before {
            first: "A".into(),
            then: "B".into(),
        },
        SequenceRule::NeverAfter {
            trigger: "A".into(),
            forbidden: "B".into(),
        },
        SequenceRule::MaxCalls {
            tool: "A".into(),
            max: 1,
        },
        SequenceRule::Eventually {
            tool: "A".into(),
            within: 2,
        },
        SequenceRule::After {
            trigger: "A".into(),
            then: "B".into(),
            within: 1,
        },
        SequenceRule::After {
            trigger: "A".into(),
            then: "B".into(),
            within: 2,
        },
        SequenceRule::Sequence {
            tools: vec!["A".into(), "B".into()],
            strict: false,
        },
        SequenceRule::Sequence {
            tools: vec!["A".into(), "B".into()],
            strict: true,
        },
    ]
}

#[test]
fn both_evaluators_agree_on_whether_a_rule_is_violated() {
    // No aliases: the copy and the shared evaluator resolve them through the same `Policy`
    // method, so an alias table would test that method rather than the two rule engines.
    let policy: Policy =
        serde_yaml::from_str("version: \"1\"\nsequences: []\n").expect("minimal policy parses");
    let traces = all_traces(&["A", "B", "C"], 4);
    let rules = rules_under_test();

    let mut permissive = Vec::new();
    let mut stricter = 0usize;
    for rule in &rules {
        for trace in &traces {
            let shared = evaluate_rules(
                std::slice::from_ref(rule),
                trace,
                Some(&policy),
                TraceExtent::Partial,
            );
            let shared_violates = shared[0].is_violation();
            let copy = assay_mcp_server::tools::check_sequence::validate_rules_for_parity(
                std::slice::from_ref(rule),
                trace,
                Some(&policy),
            );

            match (shared_violates, copy) {
                // The direction that is always a bug: the copy finds a violation the shared
                // evaluator misses. Whatever the two disagree about, the shared one must never
                // be the lenient side, because it is the one `assay run` reports from.
                (false, true) => permissive.push(format!(
                    "{rule:?} on {trace:?}: copy=violation shared={:?}",
                    shared[0].outcome
                )),
                // The shared evaluator is deliberately stricter in two places, both defects in
                // the copy that this crate cannot fix without changing its published JSON:
                // `after` accepted a `then` one call past its deadline and let a new trigger
                // discharge an unanswered one, and `eventually` treated a window as open for one
                // call after it closed. Counted rather than enumerated so the number moving is
                // itself the signal.
                (true, false) => stricter += 1,
                _ => {}
            }
        }
    }

    assert!(
        permissive.is_empty(),
        "the shared evaluator is more lenient than the proxy copy on {} case(s):\n{}",
        permissive.len(),
        permissive.join("\n")
    );
    assert_eq!(
        stricter, 48,
        "the shared evaluator is stricter than the copy on {stricter} cases, expected 48. \
         A change here means either a new copy defect was fixed (raise this) or a fix was \
         lost (lower it) -- both need saying out loud."
    );
}
