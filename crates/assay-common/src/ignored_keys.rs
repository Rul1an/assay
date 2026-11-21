//! Track ignored Serde fields for readers that preserve typed decoder diagnostics.
use std::prelude::v1::*;

// Only the two concrete readers below may use the tracked grammar. Each field
// must decode directly, so buffered unions cannot silently hide ignored keys.
macro_rules! tracked_struct {
    (root $vis:vis struct $name:ident $fields:tt) => {
        tracked_struct!(@declare [root] $vis struct $name $fields);
    };
    ($vis:vis struct $name:ident $fields:tt) => {
        tracked_struct!(@declare [] $vis struct $name $fields);
    };
    (@declare [$($root:ident)?] $vis:vis struct $name:ident {
        $($(#[$($attr:tt)*])* $field:ident : $ty:ty),* $(,)?
    }) => {
        crate::closed! { $($root)? $vis struct $name { $($(#[$($attr)*])* $field: $ty,)* } }
        impl crate::ignored_keys::TrackedFields for $name where $( $ty: crate::ignored_keys::TrackedFields, )* {}
    };
}

#[path = "enforce_document.rs"]
pub mod enforce_document;
#[path = "trust_document.rs"]
pub mod trust_document;

trait TrackedFields: crate::doc::ClosedValue {}
impl TrackedFields for String {}
impl TrackedFields for bool {}
impl TrackedFields for std::path::PathBuf {}
impl<T: TrackedFields> TrackedFields for Vec<T> {}
impl<T: TrackedFields> TrackedFields for Option<T> {}

pub const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    TooLarge(crate::limits::LimitExceeded),
    Yaml(serde_yaml::Error),
    UnknownKeys(Vec<String>),
}

/// Decode a bounded, closed record. Typed errors precede unread keys, which
/// retain their spelling and every occurrence in file order.
fn read<T: crate::doc::ClosedDoc + TrackedFields>(text: &str) -> Result<T, Error> {
    if text.len() as u64 > MAX_BYTES {
        return Err(Error::TooLarge(crate::limits::LimitExceeded {
            kind: crate::limits::LimitKind::SourceBytes,
            limit: MAX_BYTES,
        }));
    }
    let (value, unread) = decode(yaml(text)).map_err(Error::Yaml)?;
    if unread.is_empty() {
        Ok(value)
    } else {
        Err(Error::UnknownKeys(unread))
    }
}

pub(crate) fn yaml(text: &str) -> serde_yaml::Deserializer<'_> {
    serde_yaml::Deserializer::from_str(text)
}

/// Use the same typed decoder for an eval document whose unread fields will be
/// discovered by the grammar. The adapter affects diagnostics for buffered YAML keys.
#[cfg(feature = "closed-doc")]
pub(crate) fn decode_before_walk<T: serde::de::DeserializeOwned>(
    text: &str,
) -> Result<T, serde_yaml::Error> {
    with_reporter(yaml(text), |_| {})
}

/// Return the ignored paths in input order, including repeated occurrences.
/// Buffered Serde shapes can hide fields. This primitive alone does not close an arbitrary type.
pub fn decode<'de, D, T>(deserializer: D) -> Result<(T, Vec<String>), D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let mut unread = Vec::new();
    let value = with_reporter(deserializer, |path| unread.push(key_path(&path)))?;
    Ok((value, unread))
}

fn with_reporter<'de, D, T>(
    deserializer: D,
    report: impl FnMut(serde_ignored::Path<'_>),
) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde_ignored::deserialize(deserializer, report)
}

/// A serde_ignored path as a dotted key path: each map key as written and each sequence index,
/// with nothing for an `Option` or a newtype on the way.
pub fn key_path(path: &serde_ignored::Path<'_>) -> String {
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
