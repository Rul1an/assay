use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::model::{Entry, Policy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EntryFingerprint {
    count: Option<u32>,
    stability_bps: Option<i64>,
    runs_seen: Option<u32>,
    risk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EntryChange {
    pub(super) pattern: String,
    pub(super) old: EntryFingerprint,
    pub(super) new: EntryFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SectionDiff {
    pub(super) added: Vec<String>,
    pub(super) removed: Vec<String>,
    pub(super) changed: Vec<EntryChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct PolicyDiff {
    pub(super) files_allow: SectionDiff,
    pub(super) files_review: SectionDiff,
    pub(super) files_deny: SectionDiff,
    pub(super) network_allow: SectionDiff,
    pub(super) network_review: SectionDiff,
    pub(super) network_deny: SectionDiff,
    pub(super) processes_allow: SectionDiff,
    pub(super) processes_review: SectionDiff,
    pub(super) processes_deny: SectionDiff,
}

impl PolicyDiff {
    fn summary_counts(&self) -> (usize, usize, usize) {
        let sections = [
            &self.files_allow,
            &self.files_review,
            &self.files_deny,
            &self.network_allow,
            &self.network_review,
            &self.network_deny,
            &self.processes_allow,
            &self.processes_review,
            &self.processes_deny,
        ];
        let added = sections.iter().map(|s| s.added.len()).sum();
        let removed = sections.iter().map(|s| s.removed.len()).sum();
        let changed = sections.iter().map(|s| s.changed.len()).sum();
        (added, removed, changed)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.summary_counts() == (0, 0, 0)
    }
}

pub(super) fn parse_existing_policy(path: &PathBuf) -> Result<Policy> {
    let raw = std::fs::read_to_string(path)?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "json" {
        return Ok(serde_json::from_str(&raw)?);
    }
    match serde_yaml::from_str(&raw) {
        Ok(p) => Ok(p),
        Err(_) => Ok(serde_json::from_str(&raw)?),
    }
}

fn entry_pattern(entry: &Entry) -> String {
    match entry {
        Entry::Simple(s) => s.clone(),
        Entry::WithMeta { pattern, .. } => pattern.clone(),
    }
}

fn fp_stability_bps(v: Option<f64>) -> Option<i64> {
    v.map(|x| (x * 10000.0).round() as i64)
}

fn entry_fingerprint(entry: &Entry) -> EntryFingerprint {
    match entry {
        Entry::Simple(_) => EntryFingerprint {
            count: None,
            stability_bps: None,
            runs_seen: None,
            risk: None,
        },
        Entry::WithMeta {
            count,
            stability,
            runs_seen,
            risk,
            ..
        } => EntryFingerprint {
            count: *count,
            stability_bps: fp_stability_bps(*stability),
            runs_seen: *runs_seen,
            risk: risk.clone(),
        },
    }
}

fn diff_entries(old: &[Entry], new: &[Entry]) -> SectionDiff {
    let old_map: BTreeMap<String, EntryFingerprint> = old
        .iter()
        .map(|e| (entry_pattern(e), entry_fingerprint(e)))
        .collect();
    let new_map: BTreeMap<String, EntryFingerprint> = new
        .iter()
        .map(|e| (entry_pattern(e), entry_fingerprint(e)))
        .collect();

    let mut out = SectionDiff::default();
    for (pattern, new_fp) in &new_map {
        match old_map.get(pattern) {
            None => out.added.push(pattern.clone()),
            Some(old_fp) if old_fp != new_fp => out.changed.push(EntryChange {
                pattern: pattern.clone(),
                old: old_fp.clone(),
                new: new_fp.clone(),
            }),
            _ => {}
        }
    }
    for pattern in old_map.keys() {
        if !new_map.contains_key(pattern) {
            out.removed.push(pattern.clone());
        }
    }
    out
}

fn diff_string_lists(old: &[String], new: &[String]) -> SectionDiff {
    let old_set: BTreeMap<String, EntryFingerprint> = old
        .iter()
        .cloned()
        .map(|s| {
            (
                s,
                EntryFingerprint {
                    count: None,
                    stability_bps: None,
                    runs_seen: None,
                    risk: None,
                },
            )
        })
        .collect();
    let new_set: BTreeMap<String, EntryFingerprint> = new
        .iter()
        .cloned()
        .map(|s| {
            (
                s,
                EntryFingerprint {
                    count: None,
                    stability_bps: None,
                    runs_seen: None,
                    risk: None,
                },
            )
        })
        .collect();

    let mut out = SectionDiff::default();
    for pattern in new_set.keys() {
        if !old_set.contains_key(pattern) {
            out.added.push(pattern.clone());
        }
    }
    for pattern in old_set.keys() {
        if !new_set.contains_key(pattern) {
            out.removed.push(pattern.clone());
        }
    }
    out
}

pub(super) fn diff_policies(old: &Policy, new: &Policy) -> PolicyDiff {
    PolicyDiff {
        files_allow: diff_entries(&old.files.allow, &new.files.allow),
        files_review: diff_entries(&old.files.needs_review, &new.files.needs_review),
        files_deny: diff_string_lists(&old.files.deny, &new.files.deny),
        network_allow: diff_entries(
            &old.network.allow_destinations,
            &new.network.allow_destinations,
        ),
        network_review: diff_entries(&old.network.needs_review, &new.network.needs_review),
        network_deny: diff_string_lists(
            &old.network.deny_destinations,
            &new.network.deny_destinations,
        ),
        processes_allow: diff_entries(&old.processes.allow, &new.processes.allow),
        processes_review: diff_entries(&old.processes.needs_review, &new.processes.needs_review),
        processes_deny: diff_string_lists(&old.processes.deny, &new.processes.deny),
    }
}

fn print_section_diff(label: &str, diff: &SectionDiff) {
    if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
        return;
    }
    eprintln!("  {}:", label);
    for v in &diff.added {
        eprintln!("    + {}", v);
    }
    for v in &diff.removed {
        eprintln!("    - {}", v);
    }
    for c in &diff.changed {
        eprintln!("    ~ {}", c.pattern);
    }
}

pub(super) fn print_policy_diff(diff: &PolicyDiff, output_path: &Path) {
    eprintln!();
    eprintln!("Policy diff ({} -> generated):", output_path.display());
    if diff.is_empty() {
        eprintln!("  (no changes)");
        return;
    }
    print_section_diff("files.allow", &diff.files_allow);
    print_section_diff("files.needs_review", &diff.files_review);
    print_section_diff("files.deny", &diff.files_deny);
    print_section_diff("network.allow_destinations", &diff.network_allow);
    print_section_diff("network.needs_review", &diff.network_review);
    print_section_diff("network.deny_destinations", &diff.network_deny);
    print_section_diff("processes.allow", &diff.processes_allow);
    print_section_diff("processes.needs_review", &diff.processes_review);
    print_section_diff("processes.deny", &diff.processes_deny);
    let (added, removed, changed) = diff.summary_counts();
    eprintln!();
    eprintln!(
        "  Summary: +{} added, -{} removed, ~{} changed",
        added, removed, changed
    );
}

#[cfg(test)]
mod tests {
    use super::diff_policies;
    use crate::cli::commands::generate::model::{Entry, Policy};

    fn e(pattern: &str, count: Option<u32>, stability: Option<f64>) -> Entry {
        Entry::WithMeta {
            pattern: pattern.to_string(),
            count,
            stability,
            runs_seen: None,
            risk: None,
            reasons: None,
        }
    }

    #[test]
    fn diff_empty_to_populated() {
        let old = Policy::default();
        let mut new = Policy::default();
        new.files.allow.push(Entry::Simple("/tmp/a".into()));
        new.network
            .allow_destinations
            .push(Entry::Simple("api.example.com:443".into()));

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.added, vec!["/tmp/a".to_string()]);
        assert_eq!(
            diff.network_allow.added,
            vec!["api.example.com:443".to_string()]
        );
    }

    #[test]
    fn diff_removed_entries() {
        let mut old = Policy::default();
        old.files.allow.push(Entry::Simple("/tmp/old".into()));
        let new = Policy::default();

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.removed, vec!["/tmp/old".to_string()]);
        assert!(diff.files_allow.added.is_empty());
    }

    #[test]
    fn diff_stability_change() {
        let mut old = Policy::default();
        old.files.allow.push(e("/tmp/file", Some(3), Some(0.70)));
        let mut new = Policy::default();
        new.files.allow.push(e("/tmp/file", Some(3), Some(0.90)));

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.changed.len(), 1);
        assert_eq!(diff.files_allow.changed[0].pattern, "/tmp/file");
    }

    #[test]
    fn diff_no_changes() {
        let mut old = Policy::default();
        old.files.allow.push(Entry::Simple("/tmp/same".into()));
        let new = old.clone();

        let diff = diff_policies(&old, &new);
        assert!(diff.is_empty());
    }
}
