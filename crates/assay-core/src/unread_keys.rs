//! Read a YAML file into one reader's type and name every key that type did not take.

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
pub fn from_yaml_str<T: DeserializeOwned>(text: &str) -> anyhow::Result<(T, Vec<String>)> {
    let mut unread = Vec::new();
    let value = serde_ignored::deserialize(serde_yaml::Deserializer::from_str(text), |path| {
        unread.push(dotted(&path))
    })?;
    Ok((value, unread))
}

fn dotted(path: &serde_ignored::Path<'_>) -> String {
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
