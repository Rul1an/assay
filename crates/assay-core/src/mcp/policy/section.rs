//! One section of a policy file, read into a reader's own type.

use serde::de::DeserializeOwned;

/// A policy section as one reader takes it, and every key under it that the reader's type did not
/// take.
#[derive(Debug)]
pub struct PolicySection<T> {
    /// `None` when the file does not have the key, or sets it to null.
    pub value: Option<T>,
    /// Each key under the section that `T` has no field for, as a dotted path from the policy root
    /// (`kill_switch.kill_children`, `runtime_monitor.rules.0.severity`), in file order.
    pub unread: Vec<String>,
}

/// Reads the root key `key` of a policy file into `T`, and names every key under it that `T` did
/// not take.
///
/// [`super::McpPolicy`] holds every key any Assay reader accepts, with each default filled in, so
/// it cannot say what one reader acts on. A reader whose `T` holds only the keys it acts on can
/// refuse everything in [`PolicySection::unread`]. The file's bytes are read, and the section
/// typed, beside the policy loader's own stages, so the two cannot see different keys.
///
/// Declared in a module under `#![forbid(dead_code)]`, `T` then also proves the other half: a field
/// of `T` that nothing reads is a compile error, because `derive(Deserialize)` writes fields and
/// does not read them.
///
/// ```
/// #![forbid(dead_code)]
/// #[derive(serde::Deserialize)]
/// struct KillSwitch {
///     enabled: bool,
///     grace_period_ms: u64,
/// }
///
/// let file = b"kill_switch:\n  enabled: true\n  grace_period_ms: 500\n  kill_children: true\n";
/// let section =
///     assay_core::mcp::policy::read_section::<KillSwitch>(file, "kill_switch").unwrap();
/// let kill_switch = section.value.unwrap();
/// assert!(kill_switch.enabled);
/// assert_eq!(kill_switch.grace_period_ms, 500);
/// assert_eq!(section.unread, ["kill_switch.kill_children"]);
/// ```
///
/// The same type with a field that nothing reads does not compile. This example differs from the
/// one above only in that `grace_period_ms` is never read, so it is the pinned negative control for
/// the paragraph above: if a future `serde` or `rustc` counted the derive as a read, this example
/// would compile and the doctest would fail.
///
/// ```compile_fail
/// #![forbid(dead_code)]
/// #[derive(serde::Deserialize)]
/// struct KillSwitch {
///     enabled: bool,
///     grace_period_ms: u64,
/// }
///
/// let file = b"kill_switch:\n  enabled: true\n  grace_period_ms: 500\n  kill_children: true\n";
/// let section =
///     assay_core::mcp::policy::read_section::<KillSwitch>(file, "kill_switch").unwrap();
/// let kill_switch = section.value.unwrap();
/// assert!(kill_switch.enabled);
/// assert_eq!(section.unread, ["kill_switch.kill_children"]);
/// ```
pub fn read_section<T: DeserializeOwned>(
    policy_file: &[u8],
    key: &str,
) -> anyhow::Result<PolicySection<T>> {
    let (value, unread) = super::legacy::read_section(policy_file, key)?;
    Ok(PolicySection {
        value,
        unread: unread.iter().map(|path| dotted(key, path)).collect(),
    })
}

/// `serde_ignored` writes a `?` segment for each `Option` it passes through; a key path has none.
fn dotted(key: &str, path: &str) -> String {
    std::iter::once(key)
        .chain(
            path.split('.')
                .filter(|segment| !segment.is_empty() && *segment != "?"),
        )
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct Match {
        #[serde(default)]
        path_globs: Vec<String>,
        #[serde(default)]
        not: Option<Box<Match>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct Rule {
        id: String,
        #[serde(rename = "match")]
        matcher: Match,
    }

    #[derive(Debug, serde::Deserialize)]
    struct Monitor {
        #[serde(default)]
        rules: Vec<Rule>,
    }

    #[test]
    fn unread_keys_are_named_by_their_path_from_the_root() {
        let file = b"version: '2.0'\nruntime_monitor:\n  provider: ebpf\n  rules:\n    - id: a\n      severity: high\n      match:\n        path_globs: ['/x']\n        dest_globs: ['10.0.0.0/8']\n        not:\n          path_globs: ['/x/y']\n          dest_globs: ['10.1.0.0/16']\n";
        let section = read_section::<Monitor>(file, "runtime_monitor").expect("reads");
        let monitor = section.value.expect("present");
        assert_eq!(monitor.rules[0].id, "a");
        assert_eq!(monitor.rules[0].matcher.path_globs, ["/x"]);
        let not = monitor.rules[0].matcher.not.as_ref().expect("not");
        assert_eq!(not.path_globs, ["/x/y"]);
        assert!(not.not.is_none());
        assert_eq!(
            section.unread,
            [
                "runtime_monitor.provider",
                "runtime_monitor.rules.0.severity",
                "runtime_monitor.rules.0.match.dest_globs",
                "runtime_monitor.rules.0.match.not.dest_globs",
            ]
        );
    }

    #[test]
    fn an_absent_or_null_section_has_no_value_and_nothing_unread() {
        for file in [&b"version: '2.0'\n"[..], b"runtime_monitor: null\n"] {
            let section = read_section::<Monitor>(file, "runtime_monitor").expect("reads");
            assert!(section.value.is_none());
            assert!(section.unread.is_empty());
        }
    }

    #[test]
    fn a_section_of_the_wrong_shape_is_an_error_naming_the_key() {
        let error = read_section::<Monitor>(b"runtime_monitor: [1, 2]\n", "runtime_monitor")
            .expect_err("a list is not a monitor section");
        assert!(
            format!("{error:#}").contains("\"runtime_monitor\""),
            "{error:#}"
        );
        assert!(read_section::<Monitor>(b"- a\n", "runtime_monitor").is_err());
    }
}
