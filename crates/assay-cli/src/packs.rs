pub struct Pack {
    pub name: &'static str,
    pub description: &'static str,
    pub policy_yaml: &'static str,
}

const DEFAULT_POLICY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packs/default-policy.yaml"));
const HARDENED_POLICY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packs/hardened-policy.yaml"));
const DEV_POLICY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packs/dev-policy.yaml"));

pub fn list() -> &'static [Pack] {
    &[
        Pack {
            name: "default",
            description: "Balanced defaults (current protections)",
            policy_yaml: DEFAULT_POLICY,
        },
        Pack {
            name: "hardened",
            description: "No shell, no network, read-only FS (strict)",
            policy_yaml: HARDENED_POLICY,
        },
        Pack {
            name: "dev",
            description: "Permissive, logging-first (for local iteration)",
            policy_yaml: DEV_POLICY,
        },
    ]
}

pub fn get(name: &str) -> Option<&'static Pack> {
    list().iter().find(|p| p.name.eq_ignore_ascii_case(name))
}
