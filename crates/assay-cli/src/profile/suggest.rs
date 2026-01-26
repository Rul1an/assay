use super::{generalize, ProfileReport};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize)]
pub struct SuggestConfig {
    pub widen_dirs_to_glob: bool,
}

/// Generated policy suggestion (deterministic struct).
#[derive(Debug, Clone, Default, Serialize)]
pub struct PolicySuggestion {
    pub api_version: u32,
    pub extends: Vec<String>,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    pub env: EnvPolicy,
    pub processes: ProcessPolicy,
    pub meta: MetaPolicy,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FsPolicy {
    pub allow: BTreeSet<String>,
    pub deny: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetPolicy {
    pub allow: BTreeSet<String>,
    pub deny: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EnvPolicy {
    pub allow: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcessPolicy {
    pub allow: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetaPolicy {
    pub notes: Vec<String>,
    pub counters: BTreeMap<String, u64>,
}

pub fn build_policy_suggestion(report: &ProfileReport, cfg: SuggestConfig) -> PolicySuggestion {
    let mut out = PolicySuggestion {
        api_version: 1,
        ..PolicySuggestion::default()
    };

    // Default base packs (future: configurable)
    out.extends.push("pack:deny-all".to_string());
    out.extends.push("pack:mcp-server-minimal".to_string());

    // Counters
    out.meta.counters = report.agg.counters.clone();

    // Notes
    out.meta.notes = report.agg.notes.clone();

    // Environment: only keys are collected. Exclude SAFE_BASE to minimize policy noise.
    for k in report.agg.env_provided.keys() {
        if !crate::env_filter::matches_any_pattern(k, crate::env_filter::SAFE_BASE_PATTERNS) {
            out.env.allow.insert(k.clone());
        }
    }

    // Execs: generalize command paths
    for cmd in report.agg.execs.keys() {
        let p = PathBuf::from(cmd);
        let g = generalize::generalize_path(
            &p,
            &report.config.cwd,
            report.config.home.as_deref(),
            report.config.assay_tmp.as_deref(),
        );
        out.processes.allow.insert(g.rendered);
    }

    // FS: Generalize paths
    for (op, raw_path, _backend) in &report.agg.fs {
        let p = PathBuf::from(raw_path);
        let g = generalize::generalize_path(
            &p,
            &report.config.cwd,
            report.config.home.as_deref(),
            report.config.assay_tmp.as_deref(),
        );

        match op {
            super::events::FsOp::Read => {
                out.fs.allow.insert(g.rendered);
            }
            super::events::FsOp::Exec => {
                out.fs.allow.insert(g.rendered);
            }
            super::events::FsOp::Write => {
                out.fs.allow.insert(g.rendered);
            }
        }
    }

    // Default deny for net if not already set (hardening)
    out.net.deny.insert("*:*".to_string());

    // Counters & Notes
    out.meta.counters = report.agg.counters.clone();
    out.meta.notes = report.agg.notes.clone();

    // Heuristic: If we allowed ${ASSAY_TMP}/..., maybe just allow ${ASSAY_TMP}/** once?
    if cfg.widen_dirs_to_glob && report.config.assay_tmp.is_some() {
        let tmp_prefix = "${ASSAY_TMP}/";
        let has_tmp = out.fs.allow.iter().any(|p| p.starts_with(tmp_prefix));
        if has_tmp {
            // Remove individual tmp files
            out.fs.allow.retain(|p| !p.starts_with(tmp_prefix));
            // Add broad allow
            out.fs.allow.insert("${ASSAY_TMP}/**".to_string());
        }
    }

    out
}
