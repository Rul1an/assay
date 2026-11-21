//! The shared reader for user-authored documents (ADR-052). Hidden and unstable: it carries no
//! stability promise until the first format has landed on it.
//!
//! std-only: it parses YAML and allocates. Not on by default, so the eBPF build never sees it.
//!
//! # What it is for
//!
//! serde drops a key a type has no field for. A reader built on a plain `#[derive(Deserialize)]`
//! therefore accepts a misspelled key and loads the part it was meant to set as absent. Each
//! reader can guard against that for itself; this module is the place where the guard holds for
//! every reader that uses it, without each one remembering.
//!
//! - A document type is declared through [`closed!`](crate::closed). Its grammar has no way to
//!   write `flatten`, `untagged` or any other serde attribute, and its expansion refuses a field
//!   whose type is not itself closed.
//! - [`load`](crate::doc::load) is the one function that turns text into such a type. It names
//!   every key no part of the type takes, by its path as the file spells it, and refuses the file.
//!
//! # How `load` reads
//!
//! Twice, for two different questions.
//!
//! 1. *Which keys does nothing take?* The text is parsed into a tree, and the tree is walked
//!    along a description of the type that `closed!` generates from the same declaration as the
//!    type itself. Every key of a mapping that the description has no field for is collected,
//!    in file order. A `strict struct` also rejects unknown fields when an existing typed
//!    YAML reader deserializes it directly.
//! 2. *What are the values?* The text is read into the type by the YAML crate directly, not
//!    through the tree. A tree has already decided that `1.0` is a number; read from the text, a
//!    string field takes it as the string `"1.0"`. Formats such as the MCP policy that already
//!    distinguish these values declare `root(yaml_types)` and read the checked tree instead.
//!
//! The walk follows the type, so it sees the shapes `serde_ignored` cannot: a part selected by a
//! tag key is checked against the variant the tag names.
//!
//! # What it does not do
//!
//! - It does not render. [`Refusal`](crate::doc::Refusal) has no `Display` on purpose: its text
//!   quotes the file, and a consumer prints it through the one escaper it already uses for
//!   untrusted text.
//! - A closed type still implements `serde::Deserialize`, so an entry point of another format
//!   crate can read one without the key check. ADR-052 bans the YAML entry points outside this
//!   module; a JSON entry point is not banned.
//! - `ClosedValue` can be implemented by hand. Only this module and `closed!` are meant to.
//!
//! # The grammar is the whole language
//!
//! ```
//! use assay_common::closed;
//! use assay_common::doc::{load, Opaque, Refusal};
//!
//! closed! {
//!     /// A policy file.
//!     root pub struct Policy {
//!         version: String,
//!         #[default]
//!         tools: Vec<String>,
//!         #[key = "match"]
//!         matcher: Option<Matcher>,
//!         schema: Option<Opaque<serde_json::Value>>,
//!     }
//! }
//! closed! {
//!     pub struct Matcher {
//!         mode: Mode,
//!     }
//! }
//! closed! {
//!     pub enum Mode {
//!         "exact" => Exact,
//!         "glob" => Glob,
//!     }
//! }
//!
//! let policy: Policy = load("version: \"1\"\nmatch:\n  mode: glob\n").unwrap();
//! assert_eq!(policy.matcher.unwrap().mode, Mode::Glob);
//!
//! let refused = load::<Policy>("version: \"1\"\nmatch:\n  mode: glob\n  modes: exact\n");
//! assert!(matches!(refused, Err(Refusal::UnknownKeys(keys)) if keys == ["match.modes"]));
//! ```
//!
//! Each of the following is refused by the compiler. Every refused block has a twin that differs
//! in the one place named and compiles, so a block cannot pass by failing for another reason.
//!
//! A field whose type is not closed. The twin wraps it in `Opaque`:
//!
//! ```compile_fail
//! assay_common::closed! { root pub struct Doc { body: serde_json::Value } }
//! ```
//! ```
//! use assay_common::doc::Opaque;
//! assay_common::closed! { root pub struct Doc { body: Opaque<serde_json::Value> } }
//! ```
//!
//! A serde attribute on a field. The twin has none:
//!
//! ```compile_fail
//! assay_common::closed! { pub struct Inner { a: String } }
//! assay_common::closed! { root pub struct Doc { #[serde(flatten)] inner: Inner } }
//! ```
//! ```
//! assay_common::closed! { pub struct Inner { a: String } }
//! assay_common::closed! { root pub struct Doc { inner: Inner } }
//! ```
//!
//! A serde attribute on the type. The twin has none:
//!
//! ```compile_fail
//! assay_common::closed! { #[serde(deny_unknown_fields)] pub struct Doc { a: String } }
//! ```
//! ```
//! assay_common::closed! { pub struct Doc { a: String } }
//! ```
//!
//! An enum without the spelling of each variant, which is how an untagged or renamed enum would
//! have to be written. The twin spells them:
//!
//! ```compile_fail
//! assay_common::closed! { pub enum Mode { Exact, Glob } }
//! ```
//! ```
//! assay_common::closed! { pub enum Mode { "exact" => Exact, "glob" => Glob } }
//! ```
//!
//! A tagged variant that holds something other than a closed struct. serde would read the rest of
//! the mapping into the map, past the walk. The twin holds a closed struct:
//!
//! ```compile_fail
//! use std::collections::BTreeMap;
//! assay_common::closed! { pub enum Rule tagged "type" { "any" => Any(BTreeMap<String, String>) } }
//! ```
//! ```
//! assay_common::closed! { pub struct Any { tool: String } }
//! assay_common::closed! { pub enum Rule tagged "type" { "any" => Any(Any) } }
//! ```
//!
//! A raw identifier as a field name without a key. `stringify!` would give the walk `r#type` where
//! serde reads `type`. The twin names the key:
//!
//! ```compile_fail
//! assay_common::closed! { pub struct Doc { r#type: String } }
//! ```
//! ```
//! assay_common::closed! { pub struct Doc { #[key = "type"] kind: String } }
//! ```
//!
//! A type that is not a document root cannot be loaded. The twin is declared `root`:
//!
//! ```compile_fail
//! assay_common::closed! { pub struct Part { a: String } }
//! let _ = assay_common::doc::load::<Part>("a: x\n");
//! ```
//! ```
//! assay_common::closed! { root pub struct Part { a: String } }
//! let _ = assay_common::doc::load::<Part>("a: x\n");
//! ```

#[cfg(feature = "closed-doc")]
mod field_projection;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::prelude::v1::*;

use serde_yaml::Value;

/// A whole file: a declared root or a list of closed values.
pub trait ClosedDoc: ClosedValue + serde::de::DeserializeOwned {
    #[doc(hidden)]
    const PRESERVE_YAML_TYPES: bool = false;
    #[doc(hidden)]
    const ALLOW_X_EXTENSIONS: bool = false;
}

impl<T: ClosedValue + serde::de::DeserializeOwned> ClosedDoc for Vec<T> {}
impl<T: ClosedValue + serde::de::DeserializeOwned> ClosedDoc for BTreeMap<String, T> {}

/// Anything a closed type may hold: a scalar, a collection of closed values, a type declared
/// through `closed!`, or an [`Opaque`] payload that is open on purpose.
pub trait ClosedValue {
    /// What the walk expects at a node of this type.
    #[doc(hidden)]
    fn __shape() -> Shape;
}

/// A struct declared through `closed!`: the only thing a tagged variant may hold.
pub trait ClosedStruct: ClosedValue {}

/// `closed!` implements this for declarations whose decoder accounts for every field recursively.
/// As with `ClosedValue`, hand-written implementations are assertions by their author.
#[doc(hidden)]
pub trait StrictValue: ClosedValue {}
impl<T: StrictValue> StrictValue for Vec<T> {}
impl<T: StrictValue> StrictValue for Option<T> {}
#[doc(hidden)]
pub fn __require_strict<T: StrictValue>() {}

/// The description the walk follows. Generated by `closed!` from the declaration the type itself
/// is generated from, so the two cannot name different keys.
#[doc(hidden)]
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum Shape {
    /// A scalar: nothing below it.
    Leaf,
    /// Open on purpose: nothing below it is checked.
    Open,
    /// The same node, when it is there (`Option`, `Box`).
    Same(fn() -> Shape),
    /// The decoder checks string mapping keys and then reads the value as JSON.
    Json(fn() -> Shape),
    /// A second input name for this field. Its value and positional slot stay the same.
    FieldAlias(&'static str, fn() -> Shape),
    /// Every element of a sequence.
    Seq(fn() -> Shape),
    /// Every value of a mapping whose keys are data.
    Map(fn() -> Shape),
    /// A mapping whose keys are the fields of `name`.
    Struct {
        name: &'static str,
        fields: &'static [Field],
        legacy_positional: bool,
    },
    /// A YAML tag or single JSON mapping key names the variant payload.
    External(&'static [Variant]),
    /// A mapping whose discriminator names the struct that takes the other keys.
    Tagged {
        tag: &'static str,
        variants: &'static [Variant],
    },
    /// A scalar, or a mapping read by the type.
    NameOr(fn() -> Shape),
    /// A sequence read by the first type, or a mapping read by the second.
    SeqOrMap(fn() -> Shape, fn() -> Shape),
}

/// One variant of a tagged part: the tag value as the file spells it, and the struct it selects.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Variant {
    pub text: &'static str,
    pub shape: fn() -> Shape,
}

/// One field of a closed struct: the key as the file spells it, and what it holds.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub shape: fn() -> Shape,
}

impl Field {
    fn accepts_name(&self, name: &str) -> bool {
        same(name, self.key)
            || matches!((self.shape)(), Shape::FieldAlias(alias, _) if same(name, alias))
    }
}

macro_rules! leaf {
    ($($t:ty),*) => { $( impl ClosedValue for $t { fn __shape() -> Shape { Shape::Leaf } } impl StrictValue for $t {} )* };
}
leaf!(
    chrono::DateTime<chrono::Utc>,
    u8,
    u16,
    usize,
    bool,
    u32,
    u64,
    i32,
    i64,
    f64,
    String,
    std::path::PathBuf
);

/// Preserve an explicitly authored option, including null. Used by `closed!`.
#[doc(hidden)]
pub fn __present_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl<T: ClosedValue> ClosedValue for Option<T> {
    fn __shape() -> Shape {
        Shape::Same(T::__shape)
    }
}
impl<T: ClosedValue> ClosedValue for Box<T> {
    fn __shape() -> Shape {
        Shape::Same(T::__shape)
    }
}
impl<T: ClosedValue> ClosedValue for Vec<T> {
    fn __shape() -> Shape {
        Shape::Seq(T::__shape)
    }
}
impl<T: ClosedValue> ClosedValue for BTreeSet<T> {
    fn __shape() -> Shape {
        Shape::Seq(T::__shape)
    }
}
impl<T: ClosedValue> ClosedValue for BTreeMap<String, T> {
    fn __shape() -> Shape {
        Shape::Map(T::__shape)
    }
}

impl<T: ClosedValue> ClosedValue for std::collections::HashMap<String, T> {
    fn __shape() -> Shape {
        BTreeMap::<String, T>::__shape()
    }
}

/// A payload that is open by design, such as a JSON Schema body. Nothing inside it is checked.
/// The name is the marker: every open payload in a document type can be found by searching for it.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct Opaque<T>(pub T);
impl<T> ClosedValue for Opaque<T> {
    fn __shape() -> Shape {
        Shape::Open
    }
}

mod native_json {
    pub trait Field {}
    impl Field for serde_json::Value {}
    impl Field for Option<serde_json::Value> {}
}

/// Read a JSON value or optional JSON value using its public Serde representation.
/// The document walk checks duplicate members in the authored source before typed decoding.
#[doc(hidden)]
pub fn __native_json<'de, T, D>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: native_json::Field + serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(d)
}

/// Stable diagnostic prefix for an optional field of this type.
#[doc(hidden)]
pub trait FieldErrorContext: ClosedValue {
    const CONTEXT: &'static str;
}

#[doc(hidden)]
pub fn __contextual_option<'de, T, D>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FieldErrorContext + serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(d)
        .map_err(|e| serde::de::Error::custom(format!("{}: {e}", T::CONTEXT)))
}

/// Check duplicate members while reading opaque JSON, before collecting a map loses them.
#[doc(hidden)]
pub fn __optional_json<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Option::<crate::unique_value::UniqueValue>::deserialize(deserializer)?
        .map(crate::unique_value::UniqueValue::into_native_value)
        .transpose()
        .map_err(serde::de::Error::custom)
}

/// Duplicate-checked, deliberately open JSON entries in an optional list.
#[doc(hidden)]
pub fn __optional_json_rows<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<serde_json::Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use crate::unique_value::UniqueValue;
    use serde::Deserialize;
    Option::<Vec<UniqueValue>>::deserialize(deserializer)?
        .map(|rows| {
            rows.into_iter()
                .map(UniqueValue::into_native_value)
                .collect()
        })
        .transpose()
        .map_err(serde::de::Error::custom)
}

/// Alternate wire spelling for a closed scalar, selected explicitly at the field.
pub trait TextScalar: ClosedValue + Sized {
    fn spelling(&self) -> &str;
    fn parse_spelling(text: &str) -> Result<Self, String>;
}

#[doc(hidden)]
pub mod text_scalar {
    use super::TextScalar;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::string::String;

    pub fn serialize<T: TextScalar, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(value.spelling())
    }

    pub fn deserialize<'de, T: TextScalar, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
        T::parse_spelling(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Serde normally ignores leftover fields on an internally tagged unit variant.
#[doc(hidden)]
pub fn __empty_variant<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    crate::closed! { strict struct NoFields {} }
    <NoFields as serde::Deserialize>::deserialize(d).map(|NoFields {}| ())
}

/// A future tagged variant discards its payload after YAML has checked its mapping keys.
#[doc(hidden)]
pub fn __discard_future_payload<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    struct FuturePayload;
    impl<'de> serde::de::Visitor<'de> for FuturePayload {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a mapping or an empty positional payload")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<(), A::Error> {
            <Value as serde::Deserialize>::deserialize(
                serde::de::value::MapAccessDeserializer::new(map),
            )
            .map(|_| ())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, _seq: A) -> Result<(), A::Error> {
            // The internally tagged decoder checks that the visitor consumed the
            // whole sequence. Consume zero arguments, as the unit variant did.
            Ok(())
        }
    }
    d.deserialize_any(FuturePayload)
}

/// A name, or a mapping read by a closed type.
#[derive(Debug, Clone, PartialEq)]
pub enum NameOr<T> {
    Name(String),
    Detailed(T),
}
impl<T: ClosedValue> ClosedValue for NameOr<T> {
    fn __shape() -> Shape {
        Shape::NameOr(T::__shape)
    }
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for NameOr<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for V<T> {
            type Value = NameOr<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a name, or a mapping")
            }
            fn visit_str<E: serde::de::Error>(self, name: &str) -> Result<NameOr<T>, E> {
                Ok(NameOr::Name(name.to_string()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<NameOr<T>, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(NameOr::Detailed)
            }
        }
        deserializer.deserialize_any(V(PhantomData))
    }
}

/// Historical constraints were decoded through Serde's buffered content. Keep that
/// behavior within the two legacy unions: a struct could be supplied as a sequence
/// of fields, including when nested inside a list. The walk still checks its members.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum LegacyNameOr<T> {
    Name(String),
    Detailed(T),
}

impl<T: ClosedValue> ClosedValue for LegacyNameOr<T> {
    fn __shape() -> Shape {
        NameOr::<T>::__shape()
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum LegacySeqOrMap<S, M> {
    Seq(S),
    Map(M),
}

impl<S: ClosedValue, M: ClosedValue> ClosedValue for LegacySeqOrMap<S, M> {
    fn __shape() -> Shape {
        SeqOrMap::<S, M>::__shape()
    }
}

/// A sequence read by one closed type, or a mapping read by another. The kind of node decides.
#[derive(Debug, Clone, PartialEq)]
pub enum SeqOrMap<S, M> {
    Seq(S),
    Map(M),
}
impl<S: ClosedValue, M: ClosedValue> ClosedValue for SeqOrMap<S, M> {
    fn __shape() -> Shape {
        Shape::SeqOrMap(S::__shape, M::__shape)
    }
}
impl<'de, S: serde::Deserialize<'de>, M: serde::Deserialize<'de>> serde::Deserialize<'de>
    for SeqOrMap<S, M>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<S, M>(PhantomData<(S, M)>);
        impl<'de, S: serde::Deserialize<'de>, M: serde::Deserialize<'de>> serde::de::Visitor<'de>
            for V<S, M>
        {
            type Value = SeqOrMap<S, M>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list, or a mapping")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                seq: A,
            ) -> Result<Self::Value, A::Error> {
                S::deserialize(serde::de::value::SeqAccessDeserializer::new(seq)).map(SeqOrMap::Seq)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                M::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(SeqOrMap::Map)
            }
        }
        deserializer.deserialize_any(V(PhantomData))
    }
}

/// Why a document was refused. The text in it quotes the file and is not escaped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The text is not one YAML document.
    Syntax {
        line: Option<usize>,
        column: Option<usize>,
        message: String,
    },
    /// Keys no part of the document type takes: each as a dotted path from the root
    /// (`rules.0.matcher.modes`), as the file spells it, in file order.
    UnknownKeys(Vec<String>),
    /// A sequence where the type takes a mapping. serde would read it by position.
    NotAMapping {
        /// The dotted path of the node; empty for the root.
        path: String,
        /// The type that takes a mapping there.
        expected: &'static str,
    },
    /// The keys are known and a value does not fit the type, or a required key is missing.
    Shape {
        line: Option<usize>,
        column: Option<usize>,
        message: String,
    },
}

/// A root extension key for formats that allow named YAML anchors in their own section.
pub fn is_extension_key(key: &str) -> bool {
    key.starts_with("x-")
}

/// The one place text becomes a document.
///
/// Refuses, in this order: text that is not one YAML document; a sequence where the type takes a
/// mapping; every key no part of `T` takes; a value that does not fit.
pub fn load<T: ClosedDoc>(text: &str) -> Result<T, Refusal> {
    Input::parse(text)?.read()
}

/// Decode the authored text first to retain typed diagnostics and their source locations.
/// Return unread field paths in file order so the caller can choose its warning or refusal.
#[cfg(feature = "closed-doc")]
pub fn read_reporting_unknowns<T: ClosedDoc>(
    text: &str,
) -> Result<(T, Vec<String>), serde_yaml::Error> {
    let document = crate::ignored_keys::decode_before_walk(text)?;
    let mut tree = field_projection::read::<T>(text)?;
    remove_root_extensions::<T>(&mut tree);
    let mut walk = Walk {
        projected_keys: true,
        text_keys: true,
        ..Default::default()
    };
    walk.node(&tree, T::__shape());
    Ok((document, walk.unknown))
}

/// Read a `strict root` whose macro-declared parts reject unknown fields during decoding.
/// Reading the original text retains scalar spellings and located decoder errors.
/// A nested declaration that relies on the field walk cannot enter a strict document.
///
/// ```compile_fail
/// use assay_common::closed;
/// closed! { struct Inner { label: String } }
/// closed! { strict root struct Pack { inner: Inner } }
/// ```
/// ```
/// use assay_common::{closed, doc::load_strict};
/// closed! { strict part struct Inner { label: String } }
/// closed! { strict root struct Pack { inner: Inner } }
/// assert!(load_strict::<Pack>("inner: {label: yes, typo: 1}").is_err());
/// ```
pub fn load_strict<T: ClosedDoc + StrictValue>(text: &str) -> Result<T, Refusal> {
    decode_text(text)
}

/// Reads a closed document and retains the resolved input for consumers that must distinguish
/// an explicitly written field from its default. Both views originate in the same YAML tree
/// for roots declared with `yaml_types`. [`load_with_checked_source`] keeps the tree as a
/// [`Source`], from which a consumer can project a section.
pub fn load_with_source<T: ClosedDoc>(text: &str) -> Result<(T, Value), Refusal> {
    load_with_checked_source(text).map(|(document, source)| (document, source.0))
}

/// [`load_with_source`], with the checked tree kept as a [`Source`]. The one place a source is
/// made.
pub fn load_with_checked_source<T: ClosedDoc>(text: &str) -> Result<(T, Source), Refusal> {
    let tree = checked_tree::<T>(Input::parse(text)?.tree)?;
    Ok((decode(text, &tree)?, Source(tree)))
}

/// The checked tree of a document [`load_with_checked_source`] accepted: YAML anchors resolved
/// and, for a root that declares `x_extensions`, the extension keys removed. The grammar the
/// document was loaded with decides what it accepted, so a source holds what that grammar accepted,
/// and a section projected from it is part of such a document. Only this module makes one.
///
/// ```
/// use assay_common::closed;
/// use assay_common::doc::{load_with_checked_source, Source};
/// closed! { root struct Doc { section: Option<Inner> } }
/// closed! { struct Inner { kept: bool, extra: bool } }
/// #[derive(serde::Deserialize)]
/// struct Reader { kept: bool }
/// let (_, source): (Doc, Source) =
///     load_with_checked_source("section: {kept: true, extra: false}").unwrap();
/// let (reader, unread) = source.section::<Reader>("section").unwrap().unwrap();
/// assert!(reader.kept);
/// assert_eq!(unread, ["extra"]);
/// ```
///
/// A tree that the reader did not check cannot become one. This differs from the example above
/// only in building the source itself:
///
/// ```compile_fail
/// use assay_common::doc::Source;
/// let source = Source(serde_yaml::Value::Null);
/// ```
#[derive(Debug, Clone)]
pub struct Source(Value);

impl Source {
    /// The checked tree, to read.
    pub fn tree(&self) -> &Value {
        &self.0
    }

    /// The value under the root `key` decoded into the caller's own `T`, and the paths its
    /// deserializer reported as ignored, relative to the key and in file order. `None` when the
    /// key is absent or null. A derived struct reports unknown fields there. A type that buffers
    /// its input, such as an untagged enum, can discard fields without reporting them, so an empty
    /// list cannot establish that such a type used every key. Decoded from the checked tree, so
    /// scalars keep the YAML types it gave them.
    pub fn section<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<(T, Vec<String>)>, serde_yaml::Error> {
        match self.0.get(key) {
            Some(value) if !value.is_null() => crate::ignored_keys::decode(value).map(Some),
            _ => Ok(None),
        }
    }
}

/// Decode an already resolved YAML tree with a document that declares YAML-typed scalars.
pub fn read_yaml_tree<T: ClosedDoc>(tree: Value) -> Result<T, Refusal> {
    const { assert!(T::PRESERVE_YAML_TYPES) };
    T::deserialize(checked_tree::<T>(tree)?).map_err(shape_error)
}

/// Parsed YAML input for a format whose historic list and mapping forms have different types.
pub struct Input<'a> {
    text: &'a str,
    tree: Value,
}

impl<'a> Input<'a> {
    pub fn parse(text: &'a str) -> Result<Self, Refusal> {
        Self::parse_bytes(text.as_bytes())
    }

    /// Preserve the YAML decoder's error location when the caller has file bytes.
    pub fn parse_bytes(bytes: &'a [u8]) -> Result<Self, Refusal> {
        let tree = crate::yaml_input::parse_tree(bytes).map_err(syntax_error)?;
        let text = std::str::from_utf8(bytes).map_err(|error| Refusal::Syntax {
            line: None,
            column: None,
            message: error.to_string(),
        })?;
        Ok(Self { text, tree })
    }

    pub fn source(&self) -> &Value {
        &self.tree
    }

    pub fn read<T: ClosedDoc>(self) -> Result<T, Refusal> {
        decode(self.text, checked_tree::<T>(self.tree)?)
    }

    /// Report all unknown keys before a malformed section, for loaders that promise this order.
    pub fn read_unknown_keys_first<T: ClosedDoc>(
        self,
        name_key: fn(&Value) -> String,
    ) -> Result<T, Refusal> {
        decode(
            self.text,
            check_tree::<T>(
                self.tree,
                Walk {
                    name_unknown: Some(name_key),
                    ..Default::default()
                },
            )?,
        )
    }

    /// Read a dialect that historically decodes resolved YAML scalars through JSON.
    /// A non-JSON mapping key is refused even inside an opaque label.
    pub fn read_json_projection<T: ClosedDoc>(self) -> Result<T, Refusal> {
        let tree = check_mixed_projection::<T>(self.tree)?;
        serde_json::to_value(tree)
            .and_then(serde_json::from_value)
            .map_err(json_shape_error)
    }
}

/// Input for documents whose mapping keys are strings even when YAML would resolve
/// their spelling as a number. Logical duplicate keys are refused during decoding.
pub struct JsonInput(serde_json::Value);

impl JsonInput {
    pub fn parse(text: &str) -> Result<Self, Refusal> {
        parse_string_keys::<false, false, false>(text).map(Self)
    }

    pub fn source(&self) -> &serde_json::Value {
        &self.0
    }

    /// Check the declared grammar against the same projection used for the typed read.
    pub fn read<T: ClosedDoc>(&self) -> Result<T, Refusal> {
        Self::read_value::<T>(&self.0)
    }

    /// Read the same declaration when a policy is already inline in a decoded config.
    pub fn read_value<T: ClosedDoc>(source: &serde_json::Value) -> Result<T, Refusal> {
        let tree = serde_yaml::to_value(source).map_err(syntax_error)?;
        checked_tree::<T>(tree)?;
        T::deserialize(source).map_err(json_shape_error)
    }

    pub fn into_source(self) -> serde_json::Value {
        self.0
    }

    /// Keep explicit nulls and authored fields when inlining a file into another document.
    pub fn into_checked_source<T: ClosedDoc>(self) -> Result<serde_json::Value, Refusal> {
        self.read::<T>()?;
        Ok(self.0)
    }
}

fn parse_string_keys<const YAML_SHAPE: bool, const BARE_ROOT: bool, const ROOT_EXTENSIONS: bool>(
    text: &str,
) -> Result<serde_json::Value, Refusal> {
    serde_yaml::from_str::<
        crate::unique_value::StringKeyValue<YAML_SHAPE, BARE_ROOT, ROOT_EXTENSIONS>,
    >(text)
    .map(|value| value.0)
    .map_err(syntax_error)
}

/// Check dynamic map keys as strings, then decode scalar fields from their
/// original YAML spelling. For example, the string keys `16` and `0x10` differ.
pub fn load_with_string_keys<T: ClosedDoc>(text: &str) -> Result<T, Refusal> {
    let projection = if T::ALLOW_X_EXTENSIONS {
        parse_string_keys::<true, false, true>(text)?
    } else {
        parse_string_keys::<true, false, false>(text)?
    };
    decode_string_key_projection(text, projection)
}

/// Read a text-keyed document whose transport requires an untagged mapping root.
pub fn load_bare_mapping_with_string_keys<T: ClosedDoc>(text: &str) -> Result<T, Refusal> {
    let projection = parse_string_keys::<true, true, false>(text)?;
    if !projection.is_object() {
        return Err(Refusal::NotAMapping {
            path: String::new(),
            expected: "mapping",
        });
    }
    decode_string_key_projection(text, projection)
}

fn decode_string_key_projection<T: ClosedDoc>(
    text: &str,
    projection: serde_json::Value,
) -> Result<T, Refusal> {
    let tree = serde_yaml::to_value(projection).map_err(syntax_error)?;
    check_mixed_projection::<T>(tree)?;
    decode_text(text)
}

fn json_shape_error(error: serde_json::Error) -> Refusal {
    Refusal::Shape {
        line: None,
        column: None,
        message: error.to_string(),
    }
}

fn syntax_error(e: serde_yaml::Error) -> Refusal {
    let (line, column) = position(&e);
    Refusal::Syntax {
        line,
        column,
        message: e.to_string(),
    }
}

fn checked_tree<T: ClosedDoc>(tree: Value) -> Result<Value, Refusal> {
    check_tree::<T>(tree, Walk::default())
}

// These readers check and decode different representations of mapping keys.
// A numeric key can be a Serde field index in YAML but a field name in JSON.
fn check_mixed_projection<T: ClosedDoc>(tree: Value) -> Result<Value, Refusal> {
    check_tree::<T>(
        tree,
        Walk {
            skip_positional_tags: true,
            ..Default::default()
        },
    )
}

fn remove_root_extensions<T: ClosedDoc>(tree: &mut Value) {
    if T::ALLOW_X_EXTENSIONS {
        // The YAML parser has already resolved references to anchors in these entries.
        if let Some(root) = tree.as_mapping_mut() {
            root.retain(|key, _| !key.as_str().is_some_and(is_extension_key));
        }
    }
}

fn check_tree<T: ClosedDoc>(mut tree: Value, mut walk: Walk) -> Result<Value, Refusal> {
    remove_root_extensions::<T>(&mut tree);
    let shape = T::__shape();
    // Resolving YAML first can turn an empty file into null. Serde accepts null as a
    // struct with all defaults, but the typed YAML root promises an actual mapping.
    if T::PRESERVE_YAML_TYPES && !tree.is_mapping() {
        if let Shape::Struct { name, .. } = shape {
            return Err(Refusal::NotAMapping {
                path: String::new(),
                expected: name,
            });
        }
    }
    // Source projections index the resolved YAML tree by its original keys.
    walk.text_keys = !T::PRESERVE_YAML_TYPES;
    walk.node(&tree, shape);
    if let Some((path, expected)) = walk.not_a_mapping {
        if walk.name_unknown.is_none() || walk.unknown.is_empty() {
            return Err(Refusal::NotAMapping { path, expected });
        }
    }
    if !walk.unknown.is_empty() {
        return Err(Refusal::UnknownKeys(walk.unknown));
    }
    Ok(tree)
}

fn decode<'de, T: ClosedDoc>(
    text: &str,
    tree: impl serde::Deserializer<'de, Error = serde_yaml::Error>,
) -> Result<T, Refusal> {
    if T::PRESERVE_YAML_TYPES {
        T::deserialize(tree).map_err(shape_error)
    } else {
        decode_text(text)
    }
}

fn decode_text<T: ClosedDoc>(text: &str) -> Result<T, Refusal> {
    from_text(text).map_err(shape_error)
}

fn from_text<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, serde_yaml::Error> {
    serde_yaml::from_str(text)
}

fn shape_error(error: serde_yaml::Error) -> Refusal {
    let (line, column) = position(&error);
    Refusal::Shape {
        line,
        column,
        message: error.to_string(),
    }
}

fn position(error: &serde_yaml::Error) -> (Option<usize>, Option<usize>) {
    let at = error.location();
    (
        at.as_ref().map(|l| l.line()),
        at.as_ref().map(|l| l.column()),
    )
}

/// Whether text from the file is the text a declaration names. Exactly, as serde matches a key
/// and a variant: no case folding and no trimming. The walk compares in four places (a key with
/// a field, a key with the tag key twice, a tag's value with a variant), and all four come here,
/// because a walk that is more lenient than serde anywhere calls a key known that serde drops.
fn same(in_file: &str, declared: &str) -> bool {
    in_file == declared
}

/// The dotted path of a key: each mapping key as written and each sequence index.
fn dotted(path: &[String]) -> String {
    path.join(".")
}

/// A mapping key as text. A key that is a string is itself; a number or a boolean is what it
/// says; anything else has no spelling a path can carry.
fn key_text(key: &Value, text_keys: bool) -> String {
    match key {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_string(),
        Value::Tagged(tagged) if text_keys => key_text(&tagged.value, text_keys),
        Value::Tagged(_) | Value::Sequence(_) | Value::Mapping(_) => "<complex key>".to_string(),
    }
}

#[derive(Default)]
struct Walk {
    projected_keys: bool,
    name_unknown: Option<fn(&Value) -> String>,
    text_keys: bool,
    in_positional_variant: bool,
    in_tagged_mapping: bool,
    json_keys: bool,
    skip_positional_tags: bool,
    path: Vec<String>,
    unknown: Vec<String>,
    not_a_mapping: Option<(String, &'static str)>,
}

impl Walk {
    fn key<'a>(&self, key: &'a Value) -> (&'a Value, String) {
        if self.projected_keys {
            if let Value::Sequence(parts) = key {
                if let [native, Value::String(spelling)] = parts.as_slice() {
                    return (native, spelling.clone());
                }
            }
        }
        (key, key_text(key, self.text_keys))
    }

    // Inside a buffered argument Serde reads an unsigned integer identifier as
    // a declaration index. Quoted integers are names and retain their spelling.
    fn member<'a, T>(
        ordinal: bool,
        key: &Value,
        text: Option<&str>,
        declarations: &'a [T],
        accepts: fn(&T, &str) -> bool,
    ) -> Option<&'a T> {
        if ordinal {
            if let Some(index) = key.as_u64() {
                return declarations.get(usize::try_from(index).ok()?);
            }
        }
        declarations
            .iter()
            .find(|item| text.is_some_and(|text| accepts(item, text)))
    }

    fn node(&mut self, raw: &Value, shape: Shape) {
        // A YAML tag (`!thing`) does not change which keys the node has.
        let node = match raw {
            Value::Tagged(tagged) => &tagged.value,
            plain => plain,
        };
        match shape {
            Shape::External(variants) => match raw {
                Value::Tagged(tagged) => {
                    if let Some(variant) = variants.iter().find(|v| tagged.tag == v.text) {
                        self.node(&tagged.value, (variant.shape)());
                    }
                }
                Value::Mapping(entries) => {
                    for (key, child) in entries {
                        let (key, text) = self.key(key);
                        if let Some(variant) = Self::member(
                            self.in_positional_variant,
                            key,
                            key.as_str(),
                            variants,
                            |variant, text| same(text, variant.text),
                        ) {
                            self.child(text, child, (variant.shape)());
                        }
                    }
                }
                _ => {}
            },
            Shape::Leaf | Shape::Open => {}
            Shape::Same(inner) | Shape::FieldAlias(_, inner) => self.node(raw, inner()),
            Shape::Json(inner) => {
                let outer = self.json_keys;
                self.json_keys = true;
                self.node(raw, inner());
                self.json_keys = outer;
            }
            Shape::Seq(item) => {
                if let Value::Sequence(items) = node {
                    for (index, child) in items.iter().enumerate() {
                        self.child(index.to_string(), child, item());
                    }
                }
            }
            Shape::Map(value) => {
                if let Value::Mapping(entries) = node {
                    for (key, child) in entries {
                        self.child(self.key(key).1, child, value());
                    }
                }
            }
            Shape::Struct {
                name,
                fields,
                legacy_positional,
            } => {
                if let (true, Value::Sequence(items)) =
                    (legacy_positional || self.in_positional_variant, node)
                {
                    self.positional_fields(items, fields, 0);
                } else {
                    self.fields(node, name, fields, None);
                }
            }
            Shape::Tagged { tag, variants } => {
                let discriminator = match node {
                    Value::Mapping(entries) => entries
                        .iter()
                        .find(|(key, _)| key.as_str().is_some_and(|key| same(key, tag)))
                        .map(|(_, value)| value),
                    Value::Sequence(items) if !self.skip_positional_tags => items.first(),
                    _ => None,
                };
                let Some(variant) = discriminator.and_then(|key| {
                    Self::member(
                        self.in_positional_variant || self.in_tagged_mapping,
                        key,
                        key.as_str(),
                        variants,
                        |variant, text| same(text, variant.text),
                    )
                }) else {
                    return;
                };
                if let Shape::Struct { name, fields, .. } = (variant.shape)() {
                    if let Value::Sequence(items) = node {
                        // Serde buffers positional variant arguments. Nested structs in
                        // those arguments can themselves be supplied in field order.
                        let outer = self.in_positional_variant;
                        self.in_positional_variant = true;
                        self.positional_fields(&items[1..], fields, 1);
                        self.in_positional_variant = outer;
                    } else {
                        // Serde buffers the contents of an internally tagged mapping. A
                        // nested discriminator can therefore arrive as a variant index.
                        let outer = self.in_tagged_mapping;
                        self.in_tagged_mapping = !self.skip_positional_tags;
                        self.fields(node, name, fields, Some(tag));
                        self.in_tagged_mapping = outer;
                    }
                }
            }
            Shape::NameOr(detailed) => {
                let mut inner = detailed();
                while let Shape::Same(next) = inner {
                    inner = next();
                }
                if matches!(node, Value::Mapping(_))
                    || (matches!(node, Value::Sequence(_))
                        && matches!(
                            inner,
                            Shape::Struct {
                                legacy_positional: true,
                                ..
                            }
                        ))
                {
                    self.node(node, inner);
                }
            }
            Shape::SeqOrMap(seq, map) => match node {
                Value::Sequence(_) => self.node(node, seq()),
                Value::Mapping(_) => self.node(node, map()),
                _ => {}
            },
        }
    }

    fn positional_fields(&mut self, items: &[Value], fields: &[Field], offset: usize) {
        for (index, (item, field)) in items.iter().zip(fields).enumerate() {
            self.child((index + offset).to_string(), item, (field.shape)());
        }
    }

    fn child(&mut self, segment: String, node: &Value, shape: Shape) {
        self.path.push(segment);
        self.node(node, shape);
        self.path.pop();
    }

    /// The one place a mapping's keys meet a struct's fields.
    fn fields(
        &mut self,
        node: &Value,
        name: &'static str,
        fields: &'static [Field],
        tag: Option<&'static str>,
    ) {
        match node {
            Value::Mapping(entries) => {
                for (key, child) in entries {
                    let (key, text) = self.key(key);
                    if tag.is_some_and(|tag| same(&text, tag)) {
                        continue;
                    }
                    match Self::member(
                        !self.json_keys && (self.in_positional_variant || self.in_tagged_mapping),
                        key,
                        Some(&text),
                        fields,
                        Field::accepts_name,
                    ) {
                        Some(field) => self.child(text, child, (field.shape)()),
                        None => {
                            self.path
                                .push(self.name_unknown.map_or(text, |name| name(key)));
                            self.unknown.push(dotted(&self.path));
                            self.path.pop();
                        }
                    }
                }
            }
            Value::Sequence(_) => {
                // The first one found is the one reported.
                let at = dotted(&self.path);
                self.not_a_mapping.get_or_insert((at, name));
            }
            // A scalar or nothing: the typed read says what is wrong with it, if anything.
            _ => {}
        }
    }
}

/// `true` when a field name is a raw identifier, which `stringify!` and serde spell differently.
#[doc(hidden)]
pub const fn __is_raw(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'r' && bytes[1] == b'#'
}

/// Declare a document type. The grammar is the whole language:
///
/// ```text
/// closed! { [root | root(x_extensions) | root(yaml_types) | root(yaml_types, x_extensions)] [vis] struct Name { [#[default] or #[default = "path"]] [#[key = "k"]] field: Type, ... } }
/// closed! { strict [vis] struct Name { field: Type, ... } }
/// closed! { legacy_positional [vis] struct Name { field: Type, ... } }
/// closed! { strict legacy_positional [vis] struct Name { field: Type, ... } }
/// closed! { [vis] enum Name { [#[default]] "text" [| "input alias"] => Variant, ... } }
/// closed! { [vis] enum Name tagged "key" { "text" => Variant(ClosedStruct), ... } }
/// closed! { [vis] enum Name tagged "key" { "text" => Variant { [#[key = "k", alias = "a"]] field: Type, ... }, "unit" => Unit } }
/// closed! { strict [vis] enum Name tagged "key" { "text" => Variant { field: Type }; future "unsupported" => Unsupported } }
/// closed! { legacy_serde [vis] enum Name tagged "key" { "text" => Variant { field: Type, ... } } }
/// closed! { [vis] enum Name external { "text" => Variant { field: Type, ... }, "unit" => Unit } }
/// closed! { [vis] enum Name name_or { Name(String), Detailed { field: Type, ... } } }
/// ```
///
/// A tagged variant can mark an open `serde_json::Value` with `#[native_json = "key"]`.
/// Direct Serde uses the native JSON decoder. Document loading checks the source for duplicates.
/// A missing field is an error unless the declaration also supplies a default.
/// Ordinary tagged fields accept the same serialization-only `#[omit_if = "predicate"]` as structs.
/// `#[contextual_option = "key"]` reads an option normally and prefixes errors with its inner
/// type's `FieldErrorContext::CONTEXT`. A direct read of the inner type keeps its own diagnostics.
///
/// Legacy positional structs read either named fields or the same fields in declaration order.
/// A nested struct or unit enum can derive additional traits to preserve its public API. Doc comments are
/// allowed on types, fields and variants. Serde attributes cannot be supplied by the caller. A
/// field is read from the key that carries its name, or from the key `#[key]` gives. `#[default]`
/// takes `Default::default()` when the key is absent. `#[default = "path"]` calls the named
/// function instead. Both apply only to a missing key. Without a default, only an `Option` may be absent.
/// `#[omit_none]` leaves absent options out of serialized output. On a nested option,
/// `#[present_option]` also distinguishes a present null from a missing key.
/// `#[omit_if = "path"]` omits a field from serialization when the predicate returns true.
/// `#[key = "name", alias = "older_name"]` accepts both spellings and serializes the canonical name.
/// A field of a tagged variant takes the same marker, after any `#[default]` and not together
/// with `#[opaque_json]`.
/// The crate that declares a type needs `serde` with its `derive` feature.
#[doc(hidden)]
#[macro_export]
macro_rules! closed {
    // Registry keys are data covered by the digest. Only the fixed wire reader constructs this
    // root. There is no caller-supplied parser, Serde fallback, or unchecked tree constructor.
    (canonical_registry $v:vis struct $name:ident) => {
        $v type $name = $crate::doc::registry::RegistryDocument;
    };
    // The field muncher: [head] [fields done] {docs} {serde attributes} {key} rest
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[doc = $doc:literal] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)* #[doc = $doc]} {$($attrs)*} {$($key)*}
            $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[default $(= $path:literal)?] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*} {$($attrs)* #[serde(default $(= $path)?)]}
            {$($key)*} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[omit_none] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*}
            {$($attrs)* #[serde(skip_serializing_if = "Option::is_none")]} {$($key)*} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[omit_if = $path:literal] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*}
            {$($attrs)* #[serde(skip_serializing_if = $path)]} {$($key)*} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[text_scalar] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*}
            {$($attrs)* #[serde(with = "assay_common::doc::text_scalar")]} {$($key)*} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        #[present_option] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*}
            {$($attrs)* #[serde(default, deserialize_with = "assay_common::doc::__present_option",
                skip_serializing_if = "Option::is_none")]} {$($key)*} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {}
        #[key = $k:literal, alias = $alias:literal] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*}
            {$($attrs)* #[serde(rename = $k, alias = $alias)]} {$k, $alias} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {}
        #[key = $k:literal] $($rest:tt)*) => {
        $crate::closed!(@fields $head [$($done)*] {$($docs)*} {$($attrs)* #[serde(rename = $k)]}
            {$k} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        $f:ident : $t:ty , $($rest:tt)*) => {
        $crate::closed!(@fields $head
            [$($done)* { {$($docs)*} {$($attrs)*} {$($key)*} $f $t }] {} {} {} $($rest)*);
    };
    (@fields $head:tt [$($done:tt)*] {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*}
        $f:ident : $t:ty) => {
        $crate::closed!(@fields $head
            [$($done)* { {$($docs)*} {$($attrs)*} {$($key)*} $f $t }] {} {} {});
    };
    (@fields [$kind:ident {$($m:tt)*} $v:vis $name:ident]
        [$( { {$($docs:tt)*} {$($attrs:tt)*} {$($key:tt)*} $f:ident $t:ty } )*] {} {} {}) => {
        #[derive(serde::Deserialize, Debug, Clone, PartialEq)]
        $($m)*
        $v struct $name { $( $($docs)* $($attrs)* $v $f : $t, )* }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Struct {
                    name: stringify!($name),
                    legacy_positional: $crate::closed!(@positional $kind),
                    fields: &[ $( $crate::doc::Field {
                        key: $crate::closed!(@key {$($key)*} $f),
                        shape: $crate::closed!(@struct_field_shape {$($key)*} $t),
                    } ),* ],
                }
            }
        }
        impl $crate::doc::ClosedStruct for $name {}
        $crate::closed!(@root $kind $name);
        $crate::closed!(@strict_struct $kind $name { $($t),* });
    };
    (@strict_struct strict_root $name:ident $fields:tt) => {
        $crate::closed!(@strict_struct strict $name $fields);
    };
    (@strict_struct strict $name:ident { $($t:ty),* }) => {
        impl $crate::doc::StrictValue for $name where $( $t: $crate::doc::StrictValue, )* {}
    };
    (@strict_struct $kind:ident $name:ident $fields:tt) => {};
    (@root strict_root $name:ident) => { impl $crate::doc::ClosedDoc for $name {} };
    (@root strict $name:ident) => {};
    (@key {} $f:ident) => {{
        const _: () = assert!(
            !$crate::doc::__is_raw(stringify!($f)),
            "a raw identifier needs #[key = \"...\"]"
        );
        stringify!($f)
    }};
    (@key {$k:literal $(, $alias:literal)?} $f:ident) => { $k };
    (@struct_field_shape {$k:literal, $alias:literal} $t:ty) => {
        || $crate::doc::Shape::FieldAlias($alias, <$t as $crate::doc::ClosedValue>::__shape)
    };
    (@struct_field_shape {$($k:literal)?} $t:ty) => { <$t as $crate::doc::ClosedValue>::__shape };
    (@root root $name:ident) => {
        impl $crate::doc::ClosedDoc for $name {}
    };
    (@root extensions $name:ident) => {
        impl $crate::doc::ClosedDoc for $name {
            const ALLOW_X_EXTENSIONS: bool = true;
        }
    };
    (@root yaml_types $name:ident) => {
        impl $crate::doc::ClosedDoc for $name {
            const PRESERVE_YAML_TYPES: bool = true;
        }
    };
    (@root yaml_extensions $name:ident) => {
        impl $crate::doc::ClosedDoc for $name {
            const PRESERVE_YAML_TYPES: bool = true;
            const ALLOW_X_EXTENSIONS: bool = true;
        }
    };
    (@root part $name:ident) => {};
    (@root legacy $name:ident) => {};
    (@positional legacy) => { true };
    (@positional $other:ident) => { false };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? strict root $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [strict_root {$(#[doc = $doc])* $(#[derive($($extra),+)])? #[serde(deny_unknown_fields)]} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? strict part $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [strict {$(#[doc = $doc])* $(#[derive($($extra),+)])? #[serde(deny_unknown_fields)]} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? strict $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [part {$(#[doc = $doc])* $(#[derive($($extra),+)])? #[serde(deny_unknown_fields)]} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? strict legacy_positional $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [legacy {$(#[doc = $doc])* $(#[derive($($extra),+)])? #[serde(deny_unknown_fields)]} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? legacy_positional $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [legacy {$(#[doc = $doc])* $(#[derive($($extra),+)])?} $v $name] [] {} {} {} $($body)*);
    };

    ($(#[doc = $doc:literal])* root(yaml_types, x_extensions) $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [yaml_extensions {$(#[doc = $doc])*} $v $name] [] {} {} {} $($body)*);
    };

    ($(#[doc = $doc:literal])* root(x_extensions) $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [extensions {$(#[doc = $doc])*} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* root(yaml_types) $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [yaml_types {$(#[doc = $doc])*} $v $name] [] {} {} {} $($body)*);
    };

    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? root $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [root {$(#[doc = $doc])* $(#[derive($($extra),+)])?} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [part {$(#[doc = $doc])* $(#[derive($($extra),+)])?} $v $name] [] {} {} {} $($body)*);
    };
    // Derive the Rust variants and the walk's field lists from the same declaration.
    (@variants $name:ident [$v:vis] {$($attr:tt)*} {$($tag:tt)*} {
        $( $(#[doc = $vdoc:literal])* $(#[empty($empty_doc:literal)])? $text:literal => $variant:ident $( {
            $( $(#[doc = $fdoc:literal])* $(#[default $(= $path:literal)?])? $(#[key = $k:literal, alias = $alias:literal])? $(#[opaque_json = $json_key:literal])? $(#[opaque_json_rows = $rows_key:literal])? $(#[native_json = $native_key:literal])? $(#[contextual_option = $context_key:literal])? $(#[omit_if = $omit:literal])? $f:ident : $t:ty ),+ $(,)?
        } )? ),+ $(,)?
        $(; future $wire:literal => $fallback:ident)?
    }) => {
        #[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
        $($attr)*
        $v enum $name {
            $( $(#[doc = $vdoc])* $(#[doc = $empty_doc] #[serde(deserialize_with = "assay_common::doc::__empty_variant")])? #[serde(rename = $text)] $variant $( {
                $( $(#[doc = $fdoc])* $(#[serde(default $(= $path)?)])? $(#[serde(rename = $k, alias = $alias)])? $(#[serde(default, rename = $json_key, deserialize_with = "assay_common::doc::__optional_json")])? $(#[serde(default, rename = $rows_key, deserialize_with = "assay_common::doc::__optional_json_rows")])? $(#[serde(rename = $native_key, deserialize_with = "assay_common::doc::__native_json")])? $(#[serde(rename = $context_key, deserialize_with = "assay_common::doc::__contextual_option")])? $(#[serde(skip_serializing_if = $omit)])? $f: $t ),+
            } )? ),+
            $(, #[serde(rename = $wire, other, deserialize_with = "assay_common::doc::__discard_future_payload")] $fallback)?
        }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                let variants: &'static [$crate::doc::Variant] = &[$( $crate::doc::Variant {
                    text: $text,
                    shape: || $crate::doc::Shape::Struct {
                        name: stringify!($variant),
                        legacy_positional: false,
                        fields: &[$($( $crate::doc::Field {
                            key: $crate::closed!(@key {$($k)? $($json_key)? $($rows_key)? $($native_key)? $($context_key)?} $f),
                            shape: $crate::closed!(@variant_field_shape {$($k, $alias)?} {$($json_key)? $($rows_key)? $($native_key)?} $t),
                        } ),+)?],
                    },
                } ),+];
                $crate::closed!(@variant_shape variants $($tag)*)
            }
        }
    };
    // An aliased variant field uses the struct rule's shape; it cannot also be opaque.
    (@variant_field_shape {$k:literal, $alias:literal} {} $t:ty) => {
        $crate::closed!(@struct_field_shape {$k, $alias} $t)
    };
    (@variant_field_shape {} {$($key:literal)?} $t:ty) => { $crate::closed!(@field_shape {$($key)?} $t) };
    (@field_shape {} $t:ty) => { <$t as $crate::doc::ClosedValue>::__shape };
    (@field_shape {$key:literal} $t:ty) => { || $crate::doc::Shape::Open };
    (@variant_shape $variants:ident) => { $crate::doc::Shape::External($variants) };
    (@variant_shape $variants:ident $tag:literal) => {
        $crate::doc::Shape::Tagged { tag: $tag, variants: $variants }
    };
    (@strict_variant { $doc:literal } {}) => {};
    (@strict_variant {} { { $( { $($json_key:literal)? } $t:ty ),+ } }) => {
        $( $crate::closed!(@strict_field { $($json_key)? } $t); )+
    };
    (@strict_field {} $t:ty) => { let _ = $crate::doc::__require_strict::<$t>; };
    (@strict_field { $key:literal } $t:ty) => {};
    ($(#[doc = $doc:literal])* strict $v:vis enum $name:ident tagged $tag:literal {
        $( $(#[doc = $vdoc:literal])* $(#[empty($empty_doc:literal)])? $text:literal => $variant:ident $( {
            $( $(#[doc = $fdoc:literal])* $(#[default $(= $path:literal)?])? $(#[key = $k:literal, alias = $alias:literal])? $(#[opaque_json = $json_key:literal])? $f:ident : $t:ty ),+ $(,)?
        } )? ),+ $(,)?
        $(; future $wire:literal => $fallback:ident)?
    }) => {
        impl $crate::doc::StrictValue for $name {}
        const _: () = {
            $( $crate::closed!(@strict_variant { $($empty_doc)? } { $( { $( { $($json_key)? } $t ),+ } )? }); )+
        };
        $crate::closed!(@variants $name [$v] {$(#[doc = $doc])* #[serde(tag = $tag, deny_unknown_fields)]} {$tag} {
            $( $(#[doc = $vdoc])* $(#[empty($empty_doc)])? $text => $variant $( { $( $(#[doc = $fdoc])* $(#[default $(= $path)?])? $(#[key = $k, alias = $alias])? $(#[opaque_json = $json_key])? $f: $t ),+ } )? ),+
            $(; future $wire => $fallback)?
        });
    };
    // Public types that historically ignored extra fields on direct Serde calls.
    // The file reader still checks every variant field against this declaration.
    ($(#[doc = $doc:literal])* legacy_serde $v:vis enum $name:ident tagged $tag:literal {
        $( $(#[doc = $vdoc:literal])* $text:literal => $variant:ident $( {
            $( $(#[default $(= $path:literal)?])? $f:ident : $t:ty ),+ $(,)?
        } )? ),+ $(,)?
    }) => {
        $crate::closed!(@variants $name [$v] {$(#[doc = $doc])* #[serde(tag = $tag)]} {$tag} { $( $(#[doc = $vdoc])* $text => $variant $( { $( $(#[default $(= $path)?])? $f: $t ),+ } )? ),+ });
    };
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident external {
        $( $(#[doc = $vdoc:literal])* $text:literal => $variant:ident $( {
            $( $(#[default $(= $path:literal)?])? $f:ident : $t:ty ),+ $(,)?
        } )? ),+ $(,)?
    }) => {
        $crate::closed!(@variants $name [$v] {$(#[doc = $doc])*} {} { $( $(#[doc = $vdoc])* $text => $variant $( { $( $(#[default $(= $path)?])? $f: $t ),+ } )? ),+ });
    };
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident tagged $tag:literal {
        $( $(#[doc = $vdoc:literal])* $text:literal => $variant:ident $( {
            $( $(#[doc = $fdoc:literal])* $(#[default $(= $path:literal)?])? $(#[key = $k:literal, alias = $alias:literal])? $(#[opaque_json = $json_key:literal])? $(#[opaque_json_rows = $rows_key:literal])? $(#[native_json = $native_key:literal])? $(#[contextual_option = $context_key:literal])? $(#[omit_if = $omit:literal])? $f:ident : $t:ty ),+ $(,)?
        } )? ),+ $(,)?
    }) => {
        $crate::closed!(@variants $name [$v] {$(#[doc = $doc])* #[serde(tag = $tag, deny_unknown_fields)]} {$tag} {
            $( $(#[doc = $vdoc])* $text => $variant $( { $( $(#[doc = $fdoc])* $(#[default $(= $path)?])? $(#[key = $k, alias = $alias])? $(#[opaque_json = $json_key])? $(#[opaque_json_rows = $rows_key])? $(#[native_json = $native_key])? $(#[contextual_option = $context_key])? $(#[omit_if = $omit])? $f: $t ),+ } )? ),+
        });
    };
    // A public name-or-map enum uses NameOr for both the walk and the typed read.
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident name_or {
        $(#[doc = $ndoc:literal])* $bare:ident(String),
        $(#[doc = $mdoc:literal])* $detailed:ident { $( $f:ident : $t:ty ),+ $(,)? } $(,)?
    }) => {
        $(#[doc = $doc])*
        #[derive(serde::Serialize, Debug, Clone, PartialEq)]
        #[serde(untagged)]
        $v enum $name {
            $(#[doc = $ndoc])* $bare(String),
            $(#[doc = $mdoc])* $detailed { $( $f: $t ),+ },
        }
        const _: () = {
            $crate::closed!(@fields [part {#[serde(deny_unknown_fields)]} Detail]
                [] {} {} {} $( $f: $t, )+);
            type Wire = $crate::doc::NameOr<Detail>;
            impl $crate::doc::ClosedValue for $name {
                fn __shape() -> $crate::doc::Shape {
                    <Wire as $crate::doc::ClosedValue>::__shape()
                }
            }
            impl<'de> serde::Deserialize<'de> for $name {
                fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                    Ok(match <Wire as serde::Deserialize>::deserialize(d)? {
                        Wire::Name(name) => Self::$bare(name),
                        Wire::Detailed(Detail { $( $f ),+ }) => Self::$detailed { $( $f ),+ },
                    })
                }
            }
        };
    };
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident tagged $tag:literal {
        $( $(#[doc = $vdoc:literal])* $text:literal => $variant:ident ( $inner:ty ) ),+ $(,)?
    }) => {
        $(#[doc = $doc])*
        #[derive(serde::Deserialize, Debug, Clone, PartialEq)]
        #[serde(tag = $tag)]
        $v enum $name { $( $(#[doc = $vdoc])* #[serde(rename = $text)] $variant($inner) ),+ }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Tagged {
                    tag: $tag,
                    variants: &[ $( $crate::doc::Variant {
                        text: $text,
                        shape: <$inner as $crate::doc::ClosedValue>::__shape,
                    } ),+ ],
                }
            }
        }
        // A variant holds a closed struct and nothing else: serde would read the rest of the
        // mapping into any other map-like type, past the walk.
        const _: fn() = || {
            fn holds_a_closed_struct<T: $crate::doc::ClosedStruct>() {}
            $( holds_a_closed_struct::<$inner>(); )+
        };
    };
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident { $($body:tt)* }) => {
        $crate::closed!(@unit {$(#[doc = $doc])* #[derive(Copy, Eq)]} $v $name { $($body)* });
    };
    ($(#[doc = $doc:literal])* #[derive($($extra:path),+)] $v:vis enum $name:ident { $($body:tt)* }) => {
        $crate::closed!(@unit {$(#[doc = $doc])* #[derive($($extra),+)]} $v $name { $($body)* });
    };
    (@unit {$($attrs:tt)*} $v:vis $name:ident {
        $( $(#[$($marker:tt)*])* $text:literal $(| $alias:literal)* => $variant:ident ),+ $(,)?
    }) => {
        $( $( $crate::closed!(@unit_marker $($marker)*); )* )+
        $($attrs)*
        #[derive(serde::Deserialize, Debug, Clone, PartialEq)]
        $v enum $name { $( $(#[$($marker)*])* #[serde(rename = $text $(, alias = $alias)*)] $variant ),+ }
        impl $crate::doc::StrictValue for $name {}
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Leaf
            }
        }
    };
    (@unit_marker default) => {};
    (@unit_marker doc = $doc:literal) => {};
}

/// A scalar with a rule, checked when the file loads:
/// `closed_scalar!(pub Name, "what it is", |text| rule)`.
#[doc(hidden)]
#[macro_export]
macro_rules! closed_scalar {
    ($(#[doc = $doc:literal])* $v:vis $name:ident, $expecting:literal, $rule:expr) => {
        $(#[doc = $doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        $v struct $name(String);
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(deserializer)?;
                let rule: fn(&str) -> bool = $rule;
                if rule(&text) {
                    Ok($name(text))
                } else {
                    Err(serde::de::Error::custom(concat!("expected ", $expecting)))
                }
            }
        }
        impl $name {
            /// The text, which passed the rule.
            $v fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Leaf
            }
        }
    };
}

#[cfg(feature = "registry-wire")]
pub mod registry;
