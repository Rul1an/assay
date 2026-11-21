//! One section of a policy file, read into a reader's own type.

use serde::de::DeserializeOwned;

/// Loaded policy fields before defaults. YAML anchors are resolved and root extension keys removed.
/// Only the policy loader makes one, from the shared reader's checked source.
pub struct PolicySource(pub(super) assay_common::doc::Source);

impl PolicySource {
    /// Projects a reader's section before defaults obscure which fields the author supplied.
    ///
    /// [`super::McpPolicy`] contains normalized enforcement settings. The source retains
    /// declared sections such as `sequences` for readers that use them. A reader whose `T`
    /// holds only the fields it acts on can refuse everything in [`PolicySection::unread`].
    ///
    /// ```
    /// use assay_core::mcp::policy::McpPolicy;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct Switch { enabled: bool }
    ///
    /// let bytes = b"version: '2.0'\nkill_switch: {enabled: true, kill_children: true}\n";
    /// let (_, source) = McpPolicy::from_slice_with_source(bytes)?;
    /// let section = source.section::<Switch>("kill_switch")?;
    /// assert!(section.value.unwrap().enabled);
    /// assert_eq!(section.unread, ["kill_switch.kill_children"]);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// This is the route for a caller of `read_section(bytes, key)`, removed in 7.0: load the
    /// policy, then project the section. A file that holds only the section loads. A file with
    /// root keys outside the policy grammar is refused before a source exists.
    ///
    /// A root extension key such as `x-example` is different: the policy grammar accepts it, and
    /// the loaded source removes it, so its section reads as absent. `read_section` returned it.
    /// A reader that needs an extension section reads its own YAML source:
    ///
    /// ```
    /// use assay_core::mcp::policy::McpPolicy;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct Example { enabled: bool }
    ///
    /// let bytes = b"version: '2.0'\nx-example: {enabled: true}\n";
    /// let (_, source) = McpPolicy::from_slice_with_source(bytes)?;
    /// let section = source.section::<Example>("x-example")?;
    /// assert!(section.value.is_none(), "the loaded source has no root extension keys");
    /// assert!(section.unread.is_empty());
    ///
    /// let own: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
    /// let example: Example = serde_yaml::from_value(own["x-example"].clone())?;
    /// assert!(example.enabled);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// Declared in a module under `#![forbid(dead_code)]`, `T` then also proves the other half: a
    /// field of `T` that nothing reads is a compile error, because `derive(Deserialize)` writes
    /// fields and does not read them.
    ///
    /// ```
    /// #![forbid(dead_code)]
    /// use assay_core::mcp::policy::McpPolicy;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct KillSwitch {
    ///     enabled: bool,
    ///     grace_period_ms: u64,
    /// }
    ///
    /// let file = b"kill_switch:\n  enabled: true\n  grace_period_ms: 500\n  kill_children: true\n";
    /// let (_, source) = McpPolicy::from_slice_with_source(file).unwrap();
    /// let section = source.section::<KillSwitch>("kill_switch").unwrap();
    /// let kill_switch = section.value.unwrap();
    /// assert!(kill_switch.enabled);
    /// assert_eq!(kill_switch.grace_period_ms, 500);
    /// assert_eq!(section.unread, ["kill_switch.kill_children"]);
    /// ```
    ///
    /// The same type with a field that nothing reads does not compile. This example differs from
    /// the one above only in that `grace_period_ms` is never read, so it is the pinned negative
    /// control for the paragraph above: if a future `serde` or `rustc` counted the derive as a
    /// read, this example would compile and the doctest would fail.
    ///
    /// ```compile_fail
    /// #![forbid(dead_code)]
    /// use assay_core::mcp::policy::McpPolicy;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct KillSwitch {
    ///     enabled: bool,
    ///     grace_period_ms: u64,
    /// }
    ///
    /// let file = b"kill_switch:\n  enabled: true\n  grace_period_ms: 500\n  kill_children: true\n";
    /// let (_, source) = McpPolicy::from_slice_with_source(file).unwrap();
    /// let section = source.section::<KillSwitch>("kill_switch").unwrap();
    /// let kill_switch = section.value.unwrap();
    /// assert!(kill_switch.enabled);
    /// assert_eq!(section.unread, ["kill_switch.kill_children"]);
    /// ```
    pub fn section<T: DeserializeOwned>(&self, key: &str) -> anyhow::Result<PolicySection<T>> {
        let section = self
            .0
            .section::<T>(key)
            .map_err(|error| super::McpPolicyError {
                kind: super::McpPolicyErrorKind::Structure,
                source: anyhow::Error::new(error).context(format!("policy key {key:?}")),
            })?;
        Ok(match section {
            Some((value, unread)) => PolicySection::new(key, Some(value), unread),
            None => PolicySection::new(key, None, Vec::new()),
        })
    }
}

/// A decoded policy section and the paths its deserializer reported as ignored.
#[derive(Debug)]
pub struct PolicySection<T> {
    /// `None` when the file does not have the key, or sets it to null.
    pub value: Option<T>,
    /// Paths reported as ignored while decoding `T`, in file order and relative to the policy
    /// root (for example, `kill_switch.kill_children`). A derived struct reports unknown fields
    /// here. Types that buffer their input, such as an untagged enum, can discard fields without
    /// reporting them. An empty list therefore cannot establish that such a type used every key.
    pub unread: Vec<String>,
}

impl<T> PolicySection<T> {
    fn new(key: &str, value: Option<T>, unread: Vec<String>) -> Self {
        Self {
            value,
            unread: unread.iter().map(|path| dotted(key, path)).collect(),
        }
    }
}

/// The section's key in front of a path under it. The path is already a key path
/// ([`assay_common::ignored_keys::key_path`]): no `?` for an `Option`, and a key spelled `?` kept.
fn dotted(key: &str, path: &str) -> String {
    format!("{key}.{path}")
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

    /// The section as the loaded policy's source projects it: only a file the policy reader
    /// accepted has one.
    fn monitor(file: &[u8]) -> anyhow::Result<PolicySection<Monitor>> {
        let (_, source) = super::super::McpPolicy::from_slice_with_source(file)?;
        source.section::<Monitor>("runtime_monitor")
    }

    #[test]
    fn unread_keys_are_named_by_their_path_from_the_root() {
        let file = b"version: '2.0'\nruntime_monitor:\n  provider: ebpf\n  rules:\n    - id: a\n      type: file_open\n      severity: high\n      match:\n        path_globs: ['/x']\n        dest_globs: ['10.0.0.0/8']\n        not:\n          path_globs: ['/x/y']\n          dest_globs: ['10.1.0.0/16']\n";
        let section = monitor(file).expect("reads");
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
                "runtime_monitor.rules.0.type",
                "runtime_monitor.rules.0.severity",
                "runtime_monitor.rules.0.match.dest_globs",
                "runtime_monitor.rules.0.match.not.dest_globs",
            ]
        );
    }

    /// A key is named as the file spells it. serde_ignored marks each `Option` it passes through
    /// with a `?`, which a key spelled `?` cannot be told apart from in the rendered path. The
    /// policy grammar takes any tool name as a `tool_pins` key, `?` among them.
    #[test]
    fn a_key_spelled_question_mark_is_named_as_written() {
        #[derive(Debug, serde::Deserialize)]
        struct Pin {
            server_id: String,
        }
        let hash = |c: char| c.to_string().repeat(64);
        let file = format!(
            "version: '2.0'\ntool_pins:\n  \"?\":\n    server_id: s\n    tool_name: t\n    schema_hash: {}\n    meta_hash: {}\n",
            hash('a'),
            hash('b')
        );
        let (_, source) =
            super::super::McpPolicy::from_slice_with_source(file.as_bytes()).expect("loads");
        let section = source
            .section::<std::collections::BTreeMap<String, Option<Pin>>>("tool_pins")
            .expect("reads");
        let pins = section.value.expect("present");
        assert_eq!(pins["?"].as_ref().expect("pin").server_id, "s");
        assert_eq!(
            section.unread,
            [
                "tool_pins.?.tool_name",
                "tool_pins.?.schema_hash",
                "tool_pins.?.meta_hash",
            ]
        );
    }

    #[test]
    fn an_absent_or_null_section_has_no_value_and_nothing_unread() {
        for file in [&b"version: '2.0'\n"[..], b"runtime_monitor: null\n"] {
            let section = monitor(file).expect("reads");
            assert!(section.value.is_none());
            assert!(section.unread.is_empty());
        }
    }

    /// The policy reader accepted the section; the reader's own type does not fit it.
    #[test]
    fn a_section_of_the_wrong_shape_is_an_error_naming_the_key() {
        let (_, source) =
            super::super::McpPolicy::from_slice_with_source(b"runtime_monitor: {enabled: true}\n")
                .expect("loads");
        let error = source
            .section::<Vec<u8>>("runtime_monitor")
            .expect_err("a mapping is not a list");
        assert!(
            format!("{error:#}").contains("\"runtime_monitor\""),
            "{error:#}"
        );
    }
}
