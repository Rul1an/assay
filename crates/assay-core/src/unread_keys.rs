//! Read a YAML file into one reader's type and name every key that type did not take.
//!
//! Every Assay reader that names a key it does not act on builds the key's path with one
//! crate-internal function, so a key is spelled the same way in every refusal and warning.

use serde::de::DeserializeOwned;

/// Deserializes `text` into `T` and returns every key `T` has no field for, each as a dotted path
/// from the document root (`allowances.0.targets.0.read_only`), in file order.
///
/// A reader whose `T` holds only the keys it acts on refuses the file when the list is not empty,
/// instead of letting serde's defaults drop those keys without a word. The path names keys as the
/// file spells them: serde_ignored's own rendering writes `?` for each `Option` it passes through,
/// which a key spelled `?` cannot be told apart from, so the path is built from its segments here
/// and an `Option` adds none.
///
/// The error is the parser's or the type's, as an [`anyhow::Error`], so no YAML crate type is part
/// of this signature.
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct Credential {
///     scopes: Vec<String>,
/// }
/// #[derive(serde::Deserialize)]
/// struct Policy {
///     credential: Option<Credential>,
/// }
///
/// let text = "credential:\n  scope: [read]\n  scopes: [write]\n\"?\": 1\n";
/// let (policy, unread) = assay_core::unread_keys::from_yaml_str::<Policy>(text).unwrap();
/// assert_eq!(policy.credential.unwrap().scopes, ["write"]);
/// assert_eq!(unread, ["credential.scope", "?"]);
/// ```
///
/// What serde_ignored cannot see is not reported. A field marked `#[serde(flatten)]`, or an
/// internally tagged or untagged enum, buffers its keys before `T` reads them, so a key there that
/// `T` does not take is dropped without being named. A reader that refuses on this list must not
/// use those shapes in `T`; the example pins the limit:
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct Inner {
///     kept: u8,
/// }
/// #[derive(serde::Deserialize)]
/// struct Outer {
///     #[serde(flatten)]
///     inner: Inner,
/// }
///
/// let (outer, unread) = assay_core::unread_keys::from_yaml_str::<Outer>("kept: 1\nlost: 2\n").unwrap();
/// assert_eq!(outer.inner.kept, 1);
/// assert!(unread.is_empty(), "`lost` is not reported: {unread:?}");
/// ```
pub fn from_yaml_str<T: DeserializeOwned>(text: &str) -> anyhow::Result<(T, Vec<String>)> {
    let mut unread = Vec::new();
    // This attribute silences nothing, and is here to prove that. The closed-reader counter
    // (ADR-052, decision 5; scripts/ci/closed_reader_count.py) reports the two calls below whatever
    // is allowed at them, and its ledger fails the run if it ever comes up short for this file.
    #[allow(clippy::disallowed_methods)]
    let value = serde_ignored::deserialize(serde_yaml::Deserializer::from_str(text), |path| {
        unread.push(key_path(&path))
    })?;
    Ok((value, unread))
}

/// The keys `T`'s derived `Deserialize` asks for, after renames, or `None` when it does not ask
/// for a struct with named fields (a map, an enum, a struct with a flattened field).
///
/// A probe deserializer records the field list `deserialize_struct` is called with and stops, so
/// nothing is parsed. Coverage tests compare a reader's view type with the public type of the same
/// section: every key the reader takes is one the schema has, and every key it does not is
/// refused.
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct Rule {
///     id: String,
///     #[serde(rename = "match")]
///     matcher: Vec<String>,
/// }
///
/// assert_eq!(assay_core::unread_keys::keys_of::<Rule>(), Some(&["id", "match"][..]));
/// assert_eq!(assay_core::unread_keys::keys_of::<std::collections::BTreeMap<String, u8>>(), None);
/// ```
pub fn keys_of<T: DeserializeOwned>() -> Option<&'static [&'static str]> {
    #[derive(Debug)]
    struct Stop;
    impl std::fmt::Display for Stop {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("stop")
        }
    }
    impl std::error::Error for Stop {}
    impl serde::de::Error for Stop {
        fn custom<M: std::fmt::Display>(_: M) -> Self {
            Stop
        }
    }
    struct Probe<'a>(&'a mut Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for Probe<'_> {
        type Error = Stop;
        fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
            Err(Stop)
        }
        fn deserialize_struct<V: serde::de::Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Stop> {
            *self.0 = Some(fields);
            Err(Stop)
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum
            identifier ignored_any
        }
    }
    let mut fields = None;
    let _ = T::deserialize(Probe(&mut fields));
    fields
}

/// A serde_ignored path as a dotted key path: each map key as written and each sequence index,
/// with nothing for an `Option` or a newtype on the way.
pub(crate) fn key_path(path: &serde_ignored::Path<'_>) -> String {
    let mut segments = Vec::new();
    let mut at = path;
    loop {
        at = match at {
            serde_ignored::Path::Root => break,
            serde_ignored::Path::Seq { parent, index } => {
                segments.push(index.to_string());
                parent
            }
            serde_ignored::Path::Map { parent, key } => {
                segments.push(key.clone());
                parent
            }
            serde_ignored::Path::Some { parent }
            | serde_ignored::Path::NewtypeStruct { parent }
            | serde_ignored::Path::NewtypeVariant { parent } => parent,
        };
    }
    segments.reverse();
    segments.join(".")
}
