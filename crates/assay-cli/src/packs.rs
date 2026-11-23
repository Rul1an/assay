pub struct Pack {
    pub name: &'static str,
    pub description: &'static str,
    pub policy_yaml: &'static str,
}

const DEFAULT_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/packs/default-policy.yaml"
));
const HARDENED_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/packs/hardened-policy.yaml"
));
const DEV_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/packs/dev-policy.yaml"
));

const DEVELOPMENT_DESCRIPTION: &str =
    "Allows all tools except the listed deny names; checks read_file paths";

pub fn list() -> &'static [Pack] {
    &[
        Pack {
            name: "default",
            description: DEVELOPMENT_DESCRIPTION,
            policy_yaml: DEFAULT_POLICY,
        },
        Pack {
            name: "hardened",
            description:
                "Tool allowlist with path checks for read_file, list_directory and search_files",
            policy_yaml: HARDENED_POLICY,
        },
        Pack {
            name: "dev",
            description: DEVELOPMENT_DESCRIPTION,
            policy_yaml: DEV_POLICY,
        },
    ]
}

pub fn get(name: &str) -> Option<&'static Pack> {
    list().iter().find(|p| p.name.eq_ignore_ascii_case(name))
}
