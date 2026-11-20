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
//!    in file order. This is the only unknown-key check: the generated types do not use
//!    `deny_unknown_fields`.
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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::prelude::v1::*;

use serde_yaml::Value;

/// A whole file. Only `closed! { root ... }` implements it.
pub trait ClosedDoc: ClosedValue + serde::de::DeserializeOwned {
    #[doc(hidden)]
    const PRESERVE_YAML_TYPES: bool = false;
    #[doc(hidden)]
    const ALLOW_X_EXTENSIONS: bool = false;
}

/// Anything a closed type may hold: a scalar, a collection of closed values, a type declared
/// through `closed!`, or an [`Opaque`] payload that is open on purpose.
pub trait ClosedValue {
    /// What the walk expects at a node of this type.
    #[doc(hidden)]
    fn __shape() -> Shape;
}

/// A struct declared through `closed!`: the only thing a tagged variant may hold.
pub trait ClosedStruct: ClosedValue {}

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
    /// A mapping whose `tag` key names the struct that takes the other keys.
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

macro_rules! leaf {
    ($($t:ty),*) => { $( impl ClosedValue for $t { fn __shape() -> Shape { Shape::Leaf } } )* };
}
leaf!(bool, u32, u64, i64, f64, String, std::path::PathBuf);

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
    decode(text, checked_tree::<T>(text)?)
}

/// Reads a closed document and retains the resolved input for consumers that must distinguish
/// an explicitly written field from its default. Both views originate in the same YAML tree
/// for roots declared with `yaml_types`.
pub fn load_with_source<T: ClosedDoc>(text: &str) -> Result<(T, Value), Refusal> {
    let tree = checked_tree::<T>(text)?;
    Ok((decode(text, &tree)?, tree))
}

fn checked_tree<T: ClosedDoc>(text: &str) -> Result<Value, Refusal> {
    let tree = serde_yaml::from_str(text).map_err(syntax_error)?;
    check_tree::<T>(tree)
}

fn check_tree<T: ClosedDoc>(mut tree: Value) -> Result<Value, Refusal> {
    if T::ALLOW_X_EXTENSIONS {
        // The YAML parser has already resolved references to anchors in these entries.
        if let Some(root) = tree.as_mapping_mut() {
            root.retain(|key, _| !key.as_str().is_some_and(is_extension_key));
        }
    }
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
    let mut walk = Walk {
        // The text decoder reads tagged field names as strings. Source projections
        // index the resolved YAML tree by its original keys, including their tags.
        text_keys: !T::PRESERVE_YAML_TYPES,
        ..Default::default()
    };
    walk.node(&tree, shape);
    if let Some((path, expected)) = walk.not_a_mapping {
        return Err(Refusal::NotAMapping { path, expected });
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
    serde_yaml::from_str(text).map_err(shape_error)
}

fn shape_error(error: serde_yaml::Error) -> Refusal {
    let (line, column) = position(&error);
    Refusal::Shape {
        line,
        column,
        message: error.to_string(),
    }
}

/// Check dynamic map keys as strings, then decode scalar fields from their
/// original YAML spelling. For example, the string keys `16` and `0x10` differ.
pub fn load_with_string_keys<T: ClosedDoc>(text: &str) -> Result<T, Refusal> {
    fn project<const ROOT_EXTENSIONS: bool>(text: &str) -> Result<serde_json::Value, Refusal> {
        serde_yaml::from_str::<crate::unique_value::StringKeyValue<true, ROOT_EXTENSIONS>>(text)
            .map(|value| value.0)
            .map_err(syntax_error)
    }
    let projection = if T::ALLOW_X_EXTENSIONS {
        project::<true>(text)?
    } else {
        project::<false>(text)?
    };
    let tree = serde_yaml::to_value(projection).map_err(syntax_error)?;
    check_tree::<T>(tree)?;
    decode_text(text)
}

fn syntax_error(e: serde_yaml::Error) -> Refusal {
    let (line, column) = position(&e);
    Refusal::Syntax {
        line,
        column,
        message: e.to_string(),
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
    text_keys: bool,
    path: Vec<String>,
    unknown: Vec<String>,
    not_a_mapping: Option<(String, &'static str)>,
}

impl Walk {
    fn node(&mut self, node: &Value, shape: Shape) {
        // A YAML tag (`!thing`) does not change which keys the node has.
        let node = match node {
            Value::Tagged(tagged) => &tagged.value,
            plain => plain,
        };
        match shape {
            Shape::Leaf | Shape::Open => {}
            Shape::Same(inner) => self.node(node, inner()),
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
                        self.child(key_text(key, self.text_keys), child, value());
                    }
                }
            }
            Shape::Struct {
                name,
                fields,
                legacy_positional,
            } => {
                if let (true, Value::Sequence(items)) = (legacy_positional, node) {
                    for (index, (item, field)) in items.iter().zip(fields).enumerate() {
                        self.child(index.to_string(), item, (field.shape)());
                    }
                } else {
                    self.fields(node, name, fields, None);
                }
            }
            Shape::Tagged { tag, variants } => {
                let Value::Mapping(entries) = node else {
                    return;
                };
                let named = entries
                    .iter()
                    .find(|(key, _)| key.as_str().is_some_and(|key| same(key, tag)))
                    .and_then(|(_, value)| value.as_str());
                let selected = variants
                    .iter()
                    .find(|variant| named.is_some_and(|named| same(named, variant.text)))
                    .map(|variant| (variant.shape)());
                // No tag, or a tag no variant has: the typed read refuses the node.
                if let Some(Shape::Struct { name, fields, .. }) = selected {
                    self.fields(node, name, fields, Some(tag));
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
                    let text = key_text(key, self.text_keys);
                    if tag.is_some_and(|tag| same(&text, tag)) {
                        continue;
                    }
                    match fields.iter().find(|field| same(&text, field.key)) {
                        Some(field) => self.child(text, child, (field.shape)()),
                        None => {
                            self.path.push(text);
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
/// closed! { legacy_positional [vis] struct Name { field: Type, ... } }
/// closed! { [vis] enum Name { [#[default]] "text" => Variant, ... } }
/// closed! { [vis] enum Name tagged "key" { "text" => Variant(ClosedStruct), ... } }
/// closed! { [vis] enum Name tagged "key" { "text" => Variant { field: Type, ... }, ... } }
/// closed! { [vis] enum Name name_or { Name(String), Detailed { field: Type, ... } } }
/// ```
///
/// Legacy positional structs read either named fields or the same fields in declaration order.
/// A nested struct or unit enum can derive additional traits to preserve its public API. Doc comments are
/// allowed on types, fields and variants. Serde attributes cannot be supplied by the caller. A
/// field is read from the key that carries its name, or from the key `#[key]` gives. `#[default]`
/// takes `Default::default()` when the key is absent. `#[default = "path"]` calls the named
/// function instead. Both apply only to a missing key. Without a default, only an `Option` may be absent.
/// The crate that declares a type needs `serde` with its `derive` feature.
#[doc(hidden)]
#[macro_export]
macro_rules! closed {
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
                        shape: <$t as $crate::doc::ClosedValue>::__shape,
                    } ),* ],
                }
            }
        }
        impl $crate::doc::ClosedStruct for $name {}
        $crate::closed!(@root $kind $name);
    };
    (@key {} $f:ident) => {{
        const _: () = assert!(
            !$crate::doc::__is_raw(stringify!($f)),
            "a raw identifier needs #[key = \"...\"]"
        );
        stringify!($f)
    }};
    (@key {$k:literal} $f:ident) => { $k };
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

    ($(#[doc = $doc:literal])* root $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [root {$(#[doc = $doc])*} $v $name] [] {} {} {} $($body)*);
    };
    ($(#[doc = $doc:literal])* $(#[derive($($extra:path),+)])? $v:vis struct $name:ident { $($body:tt)* }) => {
        $crate::closed!(@fields [part {$(#[doc = $doc])* $(#[derive($($extra),+)])?} $v $name] [] {} {} {} $($body)*);
    };
    // A public enum with named fields keeps its Rust constructors and serialized form.
    ($(#[doc = $doc:literal])* $v:vis enum $name:ident tagged $tag:literal {
        $( $(#[doc = $vdoc:literal])* $text:literal => $variant:ident {
            $( $(#[default $(= $path:literal)?])? $f:ident : $t:ty ),+ $(,)?
        } ),+ $(,)?
    }) => {
        $(#[doc = $doc])*
        #[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
        #[serde(tag = $tag, deny_unknown_fields)]
        $v enum $name {
            $( $(#[doc = $vdoc])* #[serde(rename = $text)] $variant {
                $( $(#[serde(default $(= $path)?)])? $f: $t ),+
            } ),+
        }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Tagged {
                    tag: $tag,
                    variants: &[$( $crate::doc::Variant {
                        text: $text,
                        shape: || $crate::doc::Shape::Struct {
                            name: stringify!($variant),
                            legacy_positional: false,
                            fields: &[$( $crate::doc::Field {
                                key: $crate::closed!(@key {} $f),
                                shape: <$t as $crate::doc::ClosedValue>::__shape,
                            } ),+],
                        },
                    } ),+],
                }
            }
        }
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
        $( $(#[doc = $vdoc:literal])* $(#[$marker:ident])? $text:literal => $variant:ident ),+ $(,)?
    }) => {
        $( $( $crate::closed!(@unit_marker $marker); )? )+
        $($attrs)*
        #[derive(serde::Deserialize, Debug, Clone, PartialEq)]
        $v enum $name { $( $(#[doc = $vdoc])* $(#[$marker])? #[serde(rename = $text)] $variant ),+ }
        impl $crate::doc::ClosedValue for $name {
            fn __shape() -> $crate::doc::Shape {
                $crate::doc::Shape::Leaf
            }
        }
    };
    (@unit_marker default) => {};
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
