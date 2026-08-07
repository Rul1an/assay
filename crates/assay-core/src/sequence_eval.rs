//! One evaluation of the sequence-rule language, and a record of what it evaluated.
//!
//! Two things live here, and the second is the reason the first moved.
//!
//! **One implementation.** The rule language had two evaluators. `assay-metrics`'
//! `sequence_valid` handled `Require`, `Before` and `Blocklist` and resolved no aliases;
//! `assay-mcp-server`'s `check_sequence` handled all eight variants and did resolve them.
//! The same suite YAML therefore got two answers, and the silent one was the pass: a
//! `never_after` rule, which is the shape a credential-read-then-egress policy is written
//! in, fell through the metric's `_` arm and reported a clean run. `assay-metrics` cannot
//! call `assay-mcp-server` (the dependency runs the other way), so the shared home is here.
//!
//! **A record, not a verdict.** Each rule yields a [`RuleEvaluation`] naming the rule, the
//! call indices it read, and what it found. A consumer recomputes the conclusion from the
//! carried span rather than accepting a severity, which is what ADR-042 requires of a claim:
//! bounded, and checkable by someone who does not trust the producer. Nothing here
//! aggregates: there is no score, no whole-run verdict, and a caller that wants one has to
//! write the reduction itself and own it.
//!
//! [`RuleOutcome::NotExercised`] is the member that makes the record worth carrying. A
//! `before` rule whose `then` tool never appears passes without its antecedent ever firing,
//! and a rule kind this build does not implement passes for a different reason entirely.
//! Both used to be indistinguishable from a rule that ran and held. They are separate values
//! now, for the same reason [`crate::metrics_api::Exercised`] exists one layer down.

use crate::model::{Policy, SequenceRule};

/// What one rule found. Deliberately three values: a rule that did not run is not a rule
/// that passed, and folding them loses the distinction this module exists to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOutcome {
    /// The rule's antecedent fired and the constraint held.
    Held,
    /// The rule's antecedent fired and the constraint did not hold.
    Violated,
    /// The rule never got a chance to decide. Either its antecedent never fired, or this
    /// build has no implementation for the rule kind. `reason` says which.
    NotExercised,
}

impl RuleOutcome {
    /// The stable string for this value. It reaches `details` in a `MetricResult` and any
    /// evidence projection built over one, so it is an interface rather than a `Debug` view.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Violated => "violated",
            Self::NotExercised => "not_exercised",
        }
    }
}

/// One rule's evaluation against one call sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvaluation {
    /// Stable identity for this rule within a policy: kind plus its operands, so a consumer
    /// can key on it across runs without depending on list position.
    pub rule_id: String,
    /// The rule kind, as written in the policy vocabulary.
    pub kind: &'static str,
    pub outcome: RuleOutcome,
    /// The call indices this rule actually read to reach its outcome. Empty when the rule
    /// read nothing, which is the honest span for a kind that is not implemented.
    pub spanned: Vec<usize>,
    /// Why, in the producer's words. Present for `Violated` and `NotExercised`; a held rule
    /// needs no prose, and inventing one would invite readers to parse it.
    pub reason: Option<String>,
}

impl RuleEvaluation {
    fn held(rule_id: String, kind: &'static str, spanned: Vec<usize>) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::Held,
            spanned,
            reason: None,
        }
    }
    fn violated(rule_id: String, kind: &'static str, spanned: Vec<usize>, reason: String) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::Violated,
            spanned,
            reason: Some(reason),
        }
    }
    fn not_exercised(rule_id: String, kind: &'static str, reason: String) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::NotExercised,
            spanned: Vec::new(),
            reason: Some(reason),
        }
    }

    /// Whether this evaluation should fail the run.
    pub const fn is_violation(&self) -> bool {
        matches!(self.outcome, RuleOutcome::Violated)
    }
}

fn resolve(policy: Option<&Policy>, tool: &str) -> Vec<String> {
    match policy {
        Some(p) => p.resolve_alias(tool),
        None => vec![tool.to_string()],
    }
}

fn matches_any(name: &str, targets: &[String]) -> bool {
    targets.iter().any(|t| t == name)
}

fn indices_matching(names: &[String], targets: &[String]) -> Vec<usize> {
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| matches_any(n, targets))
        .map(|(i, _)| i)
        .collect()
}

/// Evaluate every rule against the ordered tool-call names, returning one record per rule.
///
/// Every rule is evaluated. An earlier caller returned on the first violation, which made the
/// records for later rules unobtainable rather than empty — a reader could not tell a rule
/// that held from one that was never reached. Callers that want fail-fast semantics reduce
/// over the result; the reduction is theirs to state.
pub fn evaluate_rules(
    rules: &[SequenceRule],
    actual_names: &[String],
    policy: Option<&Policy>,
) -> Vec<RuleEvaluation> {
    rules
        .iter()
        .map(|r| evaluate_rule(r, actual_names, policy))
        .collect()
}

fn evaluate_rule(rule: &SequenceRule, names: &[String], policy: Option<&Policy>) -> RuleEvaluation {
    match rule {
        SequenceRule::Require { tool } => {
            let id = format!("require:{tool}");
            let targets = resolve(policy, tool);
            let hits = indices_matching(names, &targets);
            if hits.is_empty() {
                RuleEvaluation::violated(
                    id,
                    "require",
                    Vec::new(),
                    format!("required tool '{tool}' not found"),
                )
            } else {
                RuleEvaluation::held(id, "require", hits)
            }
        }

        SequenceRule::Blocklist { pattern } => {
            let id = format!("blocklist:{pattern}");
            let hits: Vec<usize> = names
                .iter()
                .enumerate()
                .filter(|(_, n)| n.contains(pattern))
                .map(|(i, _)| i)
                .collect();
            if let Some(&idx) = hits.first() {
                RuleEvaluation::violated(
                    id,
                    "blocklist",
                    hits.clone(),
                    format!(
                        "tool '{}' matches blocklist pattern '{pattern}'",
                        names[idx]
                    ),
                )
            } else {
                // A blocklist reads every name, so it is exercised even when nothing matches.
                RuleEvaluation::held(id, "blocklist", (0..names.len()).collect())
            }
        }

        SequenceRule::Before { first, then } => {
            let id = format!("before:{first}->{then}");
            let first_t = resolve(policy, first);
            let then_t = resolve(policy, then);
            let first_idx = names.iter().position(|n| matches_any(n, &first_t));
            let Some(t_idx) = names.iter().position(|n| matches_any(n, &then_t)) else {
                // The antecedent never fired. Syntactically fine, vacuous for this trace.
                return RuleEvaluation::not_exercised(
                    id,
                    "before",
                    format!("'{then}' never appeared, so the ordering was never constrained"),
                );
            };
            match first_idx {
                Some(f_idx) if f_idx > t_idx => RuleEvaluation::violated(
                    id,
                    "before",
                    vec![f_idx, t_idx],
                    format!(
                        "tool '{first}' appeared at index {f_idx} but was required before tool '{then}' (index {t_idx})"
                    ),
                ),
                Some(f_idx) => RuleEvaluation::held(id, "before", vec![f_idx, t_idx]),
                None => RuleEvaluation::violated(
                    id,
                    "before",
                    vec![t_idx],
                    format!(
                        "tool '{then}' was found (index {t_idx}) but required preceding tool '{first}' was missing"
                    ),
                ),
            }
        }

        SequenceRule::NeverAfter { trigger, forbidden } => {
            let id = format!("never_after:{trigger}->{forbidden}");
            let trig_t = resolve(policy, trigger);
            let forb_t = resolve(policy, forbidden);
            let Some(trig_idx) = names.iter().position(|n| matches_any(n, &trig_t)) else {
                return RuleEvaluation::not_exercised(
                    id,
                    "never_after",
                    format!("'{trigger}' never appeared, so nothing was forbidden"),
                );
            };
            match names
                .iter()
                .enumerate()
                .skip(trig_idx + 1)
                .find(|(_, n)| matches_any(n, &forb_t))
            {
                Some((idx, _)) => RuleEvaluation::violated(
                    id,
                    "never_after",
                    vec![trig_idx, idx],
                    format!(
                        "tool '{forbidden}' at index {idx} is forbidden after '{trigger}' (triggered at index {trig_idx})"
                    ),
                ),
                None => RuleEvaluation::held(id, "never_after", vec![trig_idx]),
            }
        }

        SequenceRule::MaxCalls { tool, max } => {
            let id = format!("max_calls:{tool}<={max}");
            let targets = resolve(policy, tool);
            let hits = indices_matching(names, &targets);
            if hits.is_empty() {
                return RuleEvaluation::not_exercised(
                    id,
                    "max_calls",
                    format!("'{tool}' never appeared, so no ceiling was tested"),
                );
            }
            let count = hits.len() as u32;
            if count > *max {
                RuleEvaluation::violated(
                    id,
                    "max_calls",
                    hits,
                    format!("tool '{tool}' exceeded max calls ({count} > {max})"),
                )
            } else {
                RuleEvaluation::held(id, "max_calls", hits)
            }
        }

        SequenceRule::Eventually { tool, within } => {
            let id = format!("eventually:{tool}@{within}");
            let targets = resolve(policy, tool);
            match names.iter().position(|n| matches_any(n, &targets)) {
                Some(idx) if (idx as u32) >= *within => RuleEvaluation::violated(
                    id,
                    "eventually",
                    vec![idx],
                    format!(
                        "tool '{tool}' appeared at index {idx} but must appear within first {within} calls"
                    ),
                ),
                Some(idx) => RuleEvaluation::held(id, "eventually", vec![idx]),
                None if (names.len() as u32) > *within => RuleEvaluation::violated(
                    id,
                    "eventually",
                    (0..names.len()).collect(),
                    format!(
                        "tool '{tool}' required within first {within} calls but not found (trace length: {})",
                        names.len()
                    ),
                ),
                // The deadline has not passed yet, so the rule has not been able to decide.
                None => RuleEvaluation::not_exercised(
                    id,
                    "eventually",
                    format!(
                        "'{tool}' has not appeared and the trace is {} call(s) long, still within the {within}-call deadline",
                        names.len()
                    ),
                ),
            }
        }

        SequenceRule::After {
            trigger,
            then,
            within,
        } => {
            let id = format!("after:{trigger}->{then}@{within}");
            let trig_t = resolve(policy, trigger);
            let then_t = resolve(policy, then);
            // Every trigger arms its own deadline, and a later trigger re-arms after an
            // earlier one was satisfied. An earlier version of this arm took only the first
            // trigger, which reported `held` for `[t, a, t, x, x]` at `within: 1`: the second
            // `t` is never answered and the trace runs two calls past its deadline. One
            // satisfied obligation does not discharge the next one.
            let mut spanned = Vec::new();
            let mut pending: Option<(usize, usize)> = None; // (trigger index, deadline)
            for (idx, name) in names.iter().enumerate() {
                if let Some((trig_idx, deadline)) = pending {
                    if matches_any(name, &then_t) {
                        spanned.push(idx);
                        pending = None;
                    } else if idx > deadline {
                        spanned.push(idx);
                        return RuleEvaluation::violated(
                            id,
                            "after",
                            spanned,
                            format!(
                                "tool '{then}' required within {within} calls after '{trigger}' (triggered at index {trig_idx}) but was not called by index {deadline}"
                            ),
                        );
                    }
                }
                if matches_any(name, &trig_t) {
                    spanned.push(idx);
                    pending = Some((idx, idx + (*within as usize)));
                }
            }
            match pending {
                Some((trig_idx, deadline)) if names.len() > deadline => {
                    RuleEvaluation::violated(
                        id,
                        "after",
                        spanned,
                        format!(
                            "tool '{then}' required within {within} calls after '{trigger}' (triggered at index {trig_idx}) but trace exceeded deadline"
                        ),
                    )
                }
                // A trigger fired but the trace has not yet reached its deadline, so the rule
                // has not been able to decide. Distinct from a trace with no trigger at all.
                Some((trig_idx, _)) => RuleEvaluation::not_exercised(
                    id,
                    "after",
                    format!(
                        "'{trigger}' fired at index {trig_idx} but the trace has not passed the {within}-call deadline"
                    ),
                ),
                None if spanned.is_empty() => RuleEvaluation::not_exercised(
                    id,
                    "after",
                    format!("'{trigger}' never appeared, so no deadline started"),
                ),
                None => RuleEvaluation::held(id, "after", spanned),
            }
        }

        SequenceRule::Sequence { tools, strict } => {
            let id = format!(
                "sequence{}:{}",
                if *strict { ":strict" } else { "" },
                tools.join(">")
            );
            let targets: Vec<Vec<String>> = tools.iter().map(|t| resolve(policy, t)).collect();
            if targets.is_empty() {
                return RuleEvaluation::not_exercised(
                    id,
                    "sequence",
                    "the rule names no tools".to_string(),
                );
            }
            let mut seq_idx = 0usize;
            let mut spanned = Vec::new();
            for (idx, name) in names.iter().enumerate() {
                if seq_idx < targets.len() && matches_any(name, &targets[seq_idx]) {
                    spanned.push(idx);
                    seq_idx += 1;
                    continue;
                }
                if *strict && !spanned.is_empty() && seq_idx < targets.len() {
                    return RuleEvaluation::violated(
                        id,
                        "sequence",
                        {
                            let mut s = spanned.clone();
                            s.push(idx);
                            s
                        },
                        format!(
                            "strict sequence violated: expected '{}' at index {idx} but found '{name}'",
                            tools[seq_idx]
                        ),
                    );
                }
                if !*strict
                    && seq_idx < targets.len()
                    && targets
                        .iter()
                        .skip(seq_idx + 1)
                        .any(|t| matches_any(name, t))
                {
                    let mut s = spanned.clone();
                    s.push(idx);
                    return RuleEvaluation::violated(
                        id,
                        "sequence",
                        s,
                        format!(
                            "sequence out of order: '{name}' at index {idx} appears before '{}'",
                            tools[seq_idx]
                        ),
                    );
                }
            }
            if spanned.is_empty() {
                // Nothing in the sequence appeared at all; the ordering was never tested.
                RuleEvaluation::not_exercised(
                    id,
                    "sequence",
                    "no tool named by the sequence appeared".to_string(),
                )
            } else {
                RuleEvaluation::held(id, "sequence", spanned)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// The demonstration in #2105: three individually-legitimate calls, one finding across
    /// them. Before this module the metric had no `never_after` arm and reported a clean run.
    #[test]
    fn never_after_catches_credential_read_then_egress() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let seq = names(&["list_dir", "read_credentials", "http_post"]);
        let ev = evaluate_rules(&rules, &seq, None);

        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[0].kind, "never_after");
        assert_eq!(ev[0].rule_id, "never_after:read_credentials->http_post");
        // The span is the whole claim: a consumer recomputes from these two indices.
        assert_eq!(ev[0].spanned, vec![1, 2]);
    }

    /// Same rule, egress before the credential read. Held, and it says which call armed it.
    #[test]
    fn never_after_holds_when_order_is_reversed() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let ev = evaluate_rules(&rules, &names(&["http_post", "read_credentials"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::Held);
        assert_eq!(ev[0].spanned, vec![1]);
    }

    /// The trigger never fires. Not a pass: nothing was forbidden, so nothing was tested.
    #[test]
    fn never_after_without_its_trigger_is_not_exercised() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let ev = evaluate_rules(&rules, &names(&["list_dir", "http_post"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
        assert!(ev[0].spanned.is_empty());
        assert!(ev[0].reason.as_deref().unwrap().contains("never appeared"));
    }

    /// A `before` rule whose `then` never appears is syntactically perfect and vacuous.
    #[test]
    fn before_without_its_consequent_is_not_exercised() {
        let rules = vec![SequenceRule::Before {
            first: "auth".into(),
            then: "write".into(),
        }];
        let ev = evaluate_rules(&rules, &names(&["auth", "read"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
    }

    /// A blocklist reads every name, so it is exercised even when nothing matches. The two
    /// no-match cases are different and the record keeps them apart.
    #[test]
    fn blocklist_is_exercised_when_nothing_matches() {
        let rules = vec![SequenceRule::Blocklist {
            pattern: "danger".into(),
        }];
        let ev = evaluate_rules(&rules, &names(&["a", "b"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::Held);
        assert_eq!(ev[0].spanned, vec![0, 1]);
    }

    /// Every rule is evaluated. An earlier caller returned on the first violation, which left
    /// later rules with no record at all rather than a record saying they were not reached.
    #[test]
    fn every_rule_gets_a_record_even_after_a_violation() {
        let rules = vec![
            SequenceRule::Require {
                tool: "missing".into(),
            },
            SequenceRule::Blocklist {
                pattern: "danger".into(),
            },
        ];
        let ev = evaluate_rules(&rules, &names(&["a"]), None);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[1].outcome, RuleOutcome::Held);
    }

    #[test]
    fn max_calls_without_the_tool_is_not_exercised() {
        let rules = vec![SequenceRule::MaxCalls {
            tool: "spend".into(),
            max: 2,
        }];
        let ev = evaluate_rules(&rules, &names(&["read"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
    }

    #[test]
    fn max_calls_violation_spans_every_offending_index() {
        let rules = vec![SequenceRule::MaxCalls {
            tool: "spend".into(),
            max: 1,
        }];
        let ev = evaluate_rules(&rules, &names(&["spend", "read", "spend"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[0].spanned, vec![0, 2]);
    }

    /// Rule ids are stable and operand-derived, so a consumer can key on one across runs
    /// without depending on the rule's position in the policy list.
    #[test]
    fn rule_ids_are_operand_derived() {
        let ev = evaluate_rules(
            &[SequenceRule::Eventually {
                tool: "audit".into(),
                within: 3,
            }],
            &names(&["audit"]),
            None,
        );
        assert_eq!(ev[0].rule_id, "eventually:audit@3");
    }
    /// Every trigger arms its own deadline. An earlier version took only the first trigger
    /// and reported `held` here: the second `t` is never answered and the trace runs two
    /// calls past its deadline. One satisfied obligation does not discharge the next.
    #[test]
    fn after_re_arms_on_every_trigger() {
        let rules = vec![SequenceRule::After {
            trigger: "t".into(),
            then: "a".into(),
            within: 1,
        }];
        let ev = evaluate_rules(&rules, &names(&["t", "a", "t", "x", "x"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
    }

    /// A trigger that fired but whose window has not closed has not decided anything, and is
    /// a different state from a trace where the trigger never fired at all.
    #[test]
    fn after_inside_its_window_is_not_exercised() {
        let rules = vec![SequenceRule::After {
            trigger: "t".into(),
            then: "a".into(),
            within: 5,
        }];
        let ev = evaluate_rules(&rules, &names(&["x", "t"]), None);
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
        assert!(ev[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("fired at index 1"));
    }
}
