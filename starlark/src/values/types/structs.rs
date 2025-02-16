/*
 * Copyright 2018 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The struct type, an associative-map created with `struct()`.
//!
//! This struct type is related to both the [dictionary](crate::values::dict) and the
//! [record](crate::values::record) types, all being associative maps.
//!
//! * Like a record, a struct is immutable, fields can be referred to with `struct.field`, and
//!   it uses strings for keys.
//! * Like a dictionary, the struct is untyped, and manipulating structs from Rust is ergonomic.
//!
//! The `struct()` function creates a struct. It accepts keyword arguments, keys become
//! struct field names, and values become field values.
//!
//! ```
//! # starlark::assert::is_true(r#"
//! ip_address = struct(host='localhost', port=80)
//! ip_address.port == 80
//! # "#);
//! ```

use crate::{
    collections::SmallMap,
    environment::{Globals, GlobalsStatic},
    values::{
        comparison::{compare_small_map, equals_small_map},
        error::ValueError,
        AllocValue, ComplexValue, Freezer, Heap, SimpleValue, StarlarkValue, Value, ValueLike,
        Walker,
    },
};
use gazebo::any::AnyLifetime;
use std::{
    cmp::Ordering,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

impl<T> StructGen<T> {
    /// The result of calling `type()` on a struct.
    pub const TYPE: &'static str = "struct";

    /// Create a new [`Struct`].
    pub fn new(fields: SmallMap<String, T>) -> Self {
        Self { fields }
    }
}

starlark_complex_value!(pub Struct);

/// The result of calling `struct()`.
#[derive(Clone, Default, Debug)]
pub struct StructGen<T> {
    /// The fields in a struct.
    pub fields: SmallMap<String, T>,
}

/// A builder to create a `Struct` easily.
pub struct StructBuilder<'v>(&'v Heap, SmallMap<String, Value<'v>>);

impl<'v> StructBuilder<'v> {
    /// Create a new [`StructBuilder`] with a given capacity.
    pub fn with_capacity(heap: &'v Heap, capacity: usize) -> Self {
        Self(heap, SmallMap::with_capacity(capacity))
    }

    /// Create a new [`StructBuilder`].
    pub fn new(heap: &'v Heap) -> Self {
        Self(heap, SmallMap::new())
    }

    /// Add an element to the underlying [`Struct`].
    pub fn add(&mut self, key: impl Into<String>, val: impl AllocValue<'v>) {
        self.1.insert(key.into(), self.0.alloc(val));
    }

    /// Finish building and produce a [`Struct`].
    pub fn build(self) -> Struct<'v> {
        Struct { fields: self.1 }
    }
}

impl<'v> ComplexValue<'v> for Struct<'v> {
    fn freeze(self: Box<Self>, freezer: &Freezer) -> Box<dyn SimpleValue> {
        let mut frozen = SmallMap::with_capacity(self.fields.len());

        for (k, v) in self.fields.into_iter_hashed() {
            frozen.insert_hashed(k, v.freeze(freezer));
        }
        box FrozenStruct { fields: frozen }
    }

    unsafe fn walk(&mut self, walker: &Walker<'v>) {
        self.fields.values_mut().for_each(|v| walker.walk(v))
    }
}

impl<'v, T: ValueLike<'v>> StarlarkValue<'v> for StructGen<T>
where
    Self: AnyLifetime<'v>,
{
    starlark_type!(Struct::TYPE);

    fn get_members(&self) -> Option<&'static Globals> {
        static RES: GlobalsStatic = GlobalsStatic::new();
        RES.members(crate::stdlib::structs::struct_members)
    }

    fn to_json(&self) -> String {
        let mut s = "{".to_owned();
        s += &self
            .fields
            .iter()
            .map(|(k, v)| format!("\"{}\":{}", k, v.to_json()))
            .collect::<Vec<String>>()
            .join(",");
        s += "}";
        s
    }

    fn collect_repr(&self, r: &mut String) {
        r.push_str("struct(");
        for (i, (name, value)) in self.fields.iter().enumerate() {
            if i != 0 {
                r.push_str(", ");
            }
            r.push_str(name);
            r.push('=');
            value.collect_repr(r);
        }
        r.push(')');
    }

    fn equals(&self, other: Value<'v>) -> anyhow::Result<bool> {
        match Struct::from_value(other) {
            None => Ok(false),
            Some(other) => equals_small_map(&self.fields, &other.fields, |x, y| x.equals(*y)),
        }
    }

    fn compare(&self, other: Value<'v>) -> anyhow::Result<Ordering> {
        match Struct::from_value(other) {
            None => ValueError::unsupported_with(self, "cmp()", other),
            Some(other) => compare_small_map(&self.fields, &other.fields, |x, y| x.compare(*y)),
        }
    }

    fn get_attr(&self, attribute: &str, _heap: &'v Heap) -> anyhow::Result<Value<'v>> {
        match self.fields.get(attribute) {
            Some(v) => Ok(v.to_value()),
            None => Err(ValueError::OperationNotSupported {
                op: attribute.to_owned(),
                typ: self.to_repr(),
            }
            .into()),
        }
    }

    fn get_hash(&self) -> anyhow::Result<u64> {
        let mut s = DefaultHasher::new();
        for (k, v) in self.fields.iter() {
            k.hash(&mut s);
            s.write_u64(v.get_hash()?);
        }
        Ok(s.finish())
    }

    fn has_attr(&self, attribute: &str) -> bool {
        self.fields.contains_key(attribute)
    }

    fn dir_attr(&self) -> Vec<String> {
        self.fields.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::assert;

    #[test]
    fn test_to_json() {
        assert::pass(
            r#"
struct(key = None).to_json() == '{"key":null}'
struct(key = True).to_json() == '{"key":true}'
struct(key = False).to_json() == '{"key":false}'
struct(key = 42).to_json() == '{"key":42}'
struct(key = 'value').to_json() == '{"key":"value"}'
struct(key = 'value"').to_json() == '{"key":"value\\\""}'
struct(key = 'value\\').to_json() == '{"key":"value\\\\"}'
struct(key = 'value/').to_json() == '{"key":"value\\/"}'
struct(key = 'value\u0008').to_json() == '{"key":"value\\b"}'
struct(key = 'value\u000C').to_json() == '{"key":"value\\f"}'
struct(key = 'value\\n').to_json() == '{"key":"value\\n"}'
struct(key = 'value\\r').to_json() == '{"key":"value\\r"}'
struct(key = 'value\\t').to_json() == '{"key":"value\\t"}'
struct(foo = 42, bar = "some").to_json() == '{"foo":42,"bar":"some"}'
struct(foo = struct(bar = "some")).to_json() == '{"foo":{"bar":"some"}}'
struct(foo = ["bar/", "some"]).to_json() == '{"foo":["bar\\/","some"]}'
struct(foo = [struct(bar = "some")]).to_json() == '{"foo":[{"bar":"some"}]}'
"#,
        );
    }
}
