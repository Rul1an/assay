#[cfg(target_os = "linux")]
use super::{normalize, output};
#[cfg(target_os = "linux")]
use assay_common::{MonitorEvent, EVENT_OPENAT};

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct ActiveRule {
    pub(crate) id: String,
    pub(crate) action: assay_core::mcp::runtime_features::MonitorAction,
    // NOTE: Despite the name, `allow` represents patterns that trigger this rule.
    pub(crate) allow: globset::GlobSet,
    pub(crate) deny: Option<globset::GlobSet>,
}

#[cfg(target_os = "linux")]
pub(crate) fn compile_globset(globs: &[String]) -> anyhow::Result<globset::GlobSet> {
    let mut b = globset::GlobSetBuilder::new();
    for g in globs {
        b.add(globset::Glob::new(g)?);
    }
    Ok(b.build()?)
}

#[cfg(target_os = "linux")]
pub(crate) fn compile_active_rules(
    runtime_config: Option<&assay_core::mcp::runtime_features::RuntimeMonitorConfig>,
) -> Vec<ActiveRule> {
    let mut rules = Vec::new();
    if let Some(cfg) = runtime_config {
        for r in &cfg.rules {
            let kind = r.rule_type.clone();
            let mc = &r.match_config;

            if !matches!(
                kind,
                assay_core::mcp::runtime_features::MonitorRuleType::FileOpen
            ) {
                continue;
            }

            match compile_globset(&mc.path_globs) {
                Ok(allow) => {
                    let deny = mc
                        .not
                        .as_ref()
                        .map(|n| compile_globset(&n.path_globs))
                        .transpose()
                        .unwrap_or(None);
                    rules.push(ActiveRule {
                        id: r.id.clone(),
                        action: r.action.clone(),
                        allow,
                        deny,
                    });
                }
                Err(e) => {
                    eprintln!("Warning: Failed to compile glob for rule {}: {}", r.id, e);
                }
            }
        }
    }
    rules
}

#[cfg(target_os = "linux")]
pub(crate) fn find_violation_rule<'a>(
    event: &MonitorEvent,
    rules: &'a [ActiveRule],
) -> Option<&'a ActiveRule> {
    if rules.is_empty() || event.event_type != EVENT_OPENAT {
        return None;
    }

    let raw = output::decode_utf8_cstr(&event.data);
    let path = normalize::normalize_path_syntactic(&raw);
    for r in rules {
        if r.allow.is_match(&path) {
            let blocked = r.deny.as_ref().map(|d| d.is_match(&path)).unwrap_or(false);
            if !blocked {
                return Some(r);
            }
        }
    }
    None
}
