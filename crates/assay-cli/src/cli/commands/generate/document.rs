//! Baseline for comparing a generated policy. The output model retains its compact serialization.
use super::model::{Entry, Meta, NetSection, Policy, Section};
use anyhow::{anyhow, Result};
use assay_common::doc::{JsonInput, NameOr, Refusal};
use assay_common::unique_value::UniqueValue;
use std::{fs, io::ErrorKind, path::Path};

assay_common::closed! {
    root struct Document {
        _meta: Option<Metadata>,
        files: Rules,
        network: Network,
        processes: Rules,
    }
}
assay_common::closed! {
    struct Metadata {
        name: String,
        generated_at: String,
        profile_runs: Option<u32>,
        min_stability: Option<f64>,
        min_runs: Option<u32>,
    }
}
assay_common::closed! {
    struct Rules {
        #[default] allow: Vec<NameOr<Detail>>,
        #[default] needs_review: Vec<NameOr<Detail>>,
        #[default] deny: Vec<String>,
    }
}
assay_common::closed! {
    struct Network {
        #[default] allow_destinations: Vec<NameOr<Detail>>,
        #[default] needs_review: Vec<NameOr<Detail>>,
        #[default] deny_destinations: Vec<String>,
    }
}
assay_common::closed! {
    struct Detail {
        pattern: String,
        count: Option<u32>,
        stability: Option<f64>,
        runs_seen: Option<u32>,
        risk: Option<String>,
        reasons: Option<Vec<String>>,
    }
}

pub(super) fn read_existing(path: &Path) -> Result<Option<Policy>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    anyhow::ensure!(
        fs::metadata(path)?.is_file(),
        "expected a regular policy file"
    );
    let text = assay_core::model::read_policy_text(path)?;
    // The writer can emit block YAML under any suffix. JSON objects under .json
    // still get strict JSON decoding, including duplicate-key detection.
    let document: Document = if text.trim_start().starts_with(['{', '['])
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let UniqueValue(value) = serde_json::from_str(&text)?;
        JsonInput::read_value(&value)
    } else {
        assay_common::doc::load(&text)
    }
    .map_err(|error| match error {
        Refusal::UnknownKeys(keys) => anyhow!("unknown generated policy keys: {keys:?}"),
        Refusal::NotAMapping { path, expected } => {
            anyhow!("expected a {expected} mapping at {path:?}")
        }
        Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => anyhow!("{message}"),
        _ => anyhow!("invalid generated policy"),
    })?;
    let Document {
        _meta,
        files,
        network:
            Network {
                allow_destinations,
                needs_review,
                deny_destinations,
            },
        processes,
    } = document;
    Ok(Some(Policy {
        _meta: _meta.map(
            |Metadata {
                 name,
                 generated_at,
                 profile_runs,
                 min_stability,
                 min_runs,
             }| Meta {
                name,
                generated_at,
                profile_runs,
                min_stability,
                min_runs,
            },
        ),
        files: files.into(),
        network: NetSection {
            allow_destinations: entries(allow_destinations),
            needs_review: entries(needs_review),
            deny_destinations,
        },
        processes: processes.into(),
    }))
}

impl From<Rules> for Section {
    fn from(
        Rules {
            allow,
            needs_review,
            deny,
        }: Rules,
    ) -> Self {
        Self {
            allow: entries(allow),
            needs_review: entries(needs_review),
            deny,
        }
    }
}

fn entries(items: Vec<NameOr<Detail>>) -> Vec<Entry> {
    items
        .into_iter()
        .map(|item| match item {
            NameOr::Name(name) => Entry::Simple(name),
            NameOr::Detailed(Detail {
                pattern,
                count,
                stability,
                runs_seen,
                risk,
                reasons,
            }) => Entry::WithMeta {
                pattern,
                count,
                stability,
                runs_seen,
                risk,
                reasons,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::common::PolicyOutputFormat;
    use crate::cli::commands::generate::model::serialize;

    #[test]
    fn reads_every_field_writable_in_both_formats() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![
            Entry::Simple("007".into()),
            Entry::WithMeta {
                pattern: "pattern".into(),
                count: Some(2),
                stability: Some(0.75),
                runs_seen: Some(3),
                risk: Some("high".into()),
                reasons: Some(vec!["reason".into()]),
            },
        ];
        let section = Section {
            allow: entries.clone(),
            needs_review: entries.clone(),
            deny: vec!["deny".into()],
        };
        let policy = Policy {
            _meta: Some(Meta {
                name: "example".into(),
                generated_at: "2026-01-01".into(),
                profile_runs: Some(5),
                min_stability: Some(0.6),
                min_runs: Some(2),
            }),
            files: section.clone(),
            processes: section,
            network: NetSection {
                allow_destinations: entries.clone(),
                needs_review: entries,
                deny_destinations: vec!["localhost".into()],
            },
        };
        for (name, fmt) in [
            ("baseline.JSON", PolicyOutputFormat::Json),
            ("baseline.yaml", PolicyOutputFormat::Yaml),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, serialize(&policy, fmt).unwrap()).unwrap();
            assert_eq!(read_existing(&path).unwrap(), Some(policy.clone()));
        }
    }
}
