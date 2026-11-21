//! Collect just the structure needed by the field walk after the typed decoder succeeds.
//! In particular, the contents of an unread field need not be a JSON-compatible value.
use super::Shape;
use serde::de::{
    value::MapAccessDeserializer, value::SeqAccessDeserializer, DeserializeSeed, IgnoredAny,
    MapAccess, SeqAccess, Visitor,
};
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;
use std::{collections::VecDeque, fmt, prelude::v1::*};

/// Serde exposes a YAML scalar as either its typed value or its authored string.
/// Only a buffered non-string scalar key needs both views.
pub(super) fn read<T: super::ClosedDoc>(text: &str) -> Result<Value, serde_yaml::Error> {
    let mut keys = Keys::default();
    let root = || Node::Declared(T::__shape());
    let tree = root().read(&mut keys, crate::ignored_keys::yaml(text))?;
    if keys.scalars.is_empty() {
        return Ok(tree);
    }
    drop(tree);
    keys.index = 0;
    keys.spelling = true;
    root().read(&mut keys, crate::ignored_keys::yaml(text))
}

#[derive(Default)]
struct Keys {
    index: usize,
    scalars: VecDeque<(usize, Value)>,
    spelling: bool,
}

impl Keys {
    fn next<'de, A: MapAccess<'de>>(&mut self, map: &mut A) -> Result<Option<Value>, A::Error> {
        let index = self.index;
        if self.spelling && self.scalars.front().is_some_and(|(at, _)| *at == index) {
            let Some(spelling) = map.next_key::<String>()? else {
                return Ok(None);
            };
            let (_, native) = self.scalars.pop_front().expect("matched front");
            self.index += 1;
            return Ok(Some(Value::Sequence(vec![native, Value::String(spelling)])));
        }
        let mut key = map.next_key::<Value>()?;
        if let Some(value) = &key {
            if !self.spelling && matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null) {
                self.scalars.push_back((index, value.clone()));
            }
            self.index += 1;
        }
        // Reserve pairs for the private (native, spelling) representation.
        if matches!(key, Some(Value::Sequence(_))) {
            key = key.map(|value| Value::Sequence(vec![value]));
        }
        Ok(key)
    }
}

struct Reader<'a>(Node, &'a mut Keys);

impl Node {
    fn read<'de, D: Deserializer<'de>>(self, keys: &mut Keys, d: D) -> Result<Value, D::Error> {
        Reader(self, keys).deserialize(d)
    }
}

enum Node {
    Declared(Shape),
    Buffered,
}

impl<'de> DeserializeSeed<'de> for Reader<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        match self.0 {
            Node::Declared(Shape::Open | Shape::Leaf) => {
                IgnoredAny::deserialize(d).map(|_| Value::Null)
            }
            Node::Declared(Shape::Same(inner) | Shape::FieldAlias(_, inner)) => {
                Node::Declared(inner()).read(self.1, d)
            }
            Node::Declared(Shape::Json(_)) => {
                let value = crate::unique_value::StringKeyValue::<false>::deserialize(d)?;
                serde_yaml::to_value(value.0).map_err(serde::de::Error::custom)
            }
            // The discriminator can follow its payload. Serde has already buffered these
            // scalar keys with their YAML types, including numeric field indices.
            Node::Declared(Shape::Tagged { .. } | Shape::External(_)) => {
                d.deserialize_any(Reader(Node::Buffered, self.1))
            }
            _ => d.deserialize_any(self),
        }
    }
}

impl<'de> Visitor<'de> for Reader<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a decoded document field")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        if let Node::Declared(Shape::NameOr(inner) | Shape::SeqOrMap(_, inner)) = self.0 {
            return Node::Declared(inner()).read(self.1, MapAccessDeserializer::new(map));
        }
        let mut entries = serde_yaml::Mapping::new();
        if matches!(self.0, Node::Buffered) {
            while let Some(key) = self.1.next(&mut map)? {
                let value = map.next_value_seed(Reader(Node::Buffered, self.1))?;
                if !entries.contains_key(&key) {
                    entries.insert(key, value);
                }
            }
        } else {
            while let Some(key) = map.next_key::<String>()? {
                let child = match self.0 {
                    Node::Declared(Shape::Struct { fields, .. }) => fields
                        .iter()
                        .find(|f| f.accepts_name(&key))
                        .map(|f| (f.shape)()),
                    Node::Declared(Shape::Map(inner)) => Some(inner()),
                    _ => None,
                };
                let value = map.next_value_seed(Reader(
                    Node::Declared(child.unwrap_or(Shape::Open)),
                    self.1,
                ))?;
                entries.insert(Value::String(key), value);
            }
        }
        Ok(Value::Mapping(entries))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        if let Node::Declared(Shape::SeqOrMap(inner, _)) = self.0 {
            return Node::Declared(inner()).read(self.1, SeqAccessDeserializer::new(seq));
        }
        let mut values = Vec::new();
        loop {
            let child = match self.0 {
                Node::Buffered => Node::Buffered,
                Node::Declared(Shape::Seq(inner)) => Node::Declared(inner()),
                Node::Declared(Shape::Struct { fields, .. }) => Node::Declared(
                    fields
                        .get(values.len())
                        .map_or(Shape::Open, |f| (f.shape)()),
                ),
                _ => Node::Declared(Shape::Open),
            };
            let Some(value) = seq.next_element_seed(Reader(child, self.1))? else {
                break;
            };
            values.push(value);
        }
        Ok(Value::Sequence(values))
    }

    fn visit_enum<A: serde::de::EnumAccess<'de>>(self, data: A) -> Result<Value, A::Error> {
        use serde::de::VariantAccess;
        let (tag, payload) = data.variant::<String>()?;
        if matches!(self.0, Node::Buffered) {
            Ok(Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
                tag: serde_yaml::value::Tag::new(tag),
                value: payload.newtype_variant_seed(self)?,
            })))
        } else {
            payload.newtype_variant_seed(self)
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
}
