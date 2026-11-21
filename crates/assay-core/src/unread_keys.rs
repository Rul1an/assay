//! The keys a reader's type takes, and the path a key it did not take is named by.
//!
//! Every Assay reader that names a key it does not act on builds the key's path with one
//! crate-internal function, so a key is spelled the same way in every refusal and warning.

use serde::de::DeserializeOwned;

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
