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

use crate::{
    collections::SmallMap,
    stdlib,
    values::{
        structs::FrozenStruct, AllocFrozenValue, FrozenHeap, FrozenHeapRef, FrozenValue, Value,
    },
};
use gazebo::prelude::*;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use std::{collections::HashMap, mem, sync::Arc};

pub use crate::stdlib::LibraryExtension;

/// The global values available during execution.
#[derive(Clone, Dupe, Debug)]
pub struct Globals(Arc<GlobalsData>);

#[derive(Debug)]
struct GlobalsData {
    heap: FrozenHeapRef,
    variables: HashMap<String, FrozenValue>,
}

/// Used to build a [`Globals`] value.
#[derive(Debug)]
pub struct GlobalsBuilder {
    // The heap everything is allocated in
    heap: FrozenHeap,
    // Normal top-level variables, e.g. True/hash
    variables: HashMap<String, FrozenValue>,
    // Set to Some when we are in a struct builder, otherwise None
    struct_fields: Option<SmallMap<String, FrozenValue>>,
}

impl Globals {
    /// Create an empty [`Globals`], with no functions in scope.
    pub fn new() -> Self {
        GlobalsBuilder::new().build()
    }

    /// Create a [`Globals`] following the
    /// [Starlark standard](https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions).
    pub fn standard() -> Self {
        GlobalsBuilder::standard().build()
    }

    /// Create a [`Globals`] combining those functions in the Starlark standard plus
    /// all those defined in [`LibraryExtension`].
    pub fn extended() -> Self {
        GlobalsBuilder::extended().build()
    }

    /// Create a [`Globals`] combining those functions in the Starlark standard plus
    /// all those given in the [`LibraryExtension`] arguments.
    pub fn extended_by(extensions: &[LibraryExtension]) -> Self {
        GlobalsBuilder::extended_by(extensions).build()
    }

    /// This function is only safe if you first call `heap` and keep a reference to it.
    /// Therefore, don't expose it on the public API.
    pub(crate) fn get<'v>(&'v self, name: &str) -> Option<Value<'v>> {
        self.get_frozen(name).map(FrozenValue::to_value)
    }

    /// This function is only safe if you first call `heap` and keep a reference to it.
    /// Therefore, don't expose it on the public API.
    pub(crate) fn get_frozen(&self, name: &str) -> Option<FrozenValue> {
        self.0.variables.get(name).copied()
    }

    /// Get all the names defined in this environment.
    pub fn names(&self) -> Vec<String> {
        self.0.variables.keys().cloned().collect()
    }

    pub(crate) fn heap(&self) -> &FrozenHeapRef {
        &self.0.heap
    }

    /// Print information about the values in this object.
    pub fn describe(&self) -> String {
        self.0
            .variables
            .iter()
            .map(|(name, val)| val.to_value().describe(name))
            .join("\n")
    }
}

impl GlobalsBuilder {
    /// Create an empty [`GlobalsBuilder`], with no functions in scope.
    pub fn new() -> Self {
        Self {
            heap: FrozenHeap::new(),
            variables: HashMap::new(),
            struct_fields: None,
        }
    }

    /// Create a [`GlobalsBuilder`] following the
    /// [Starlark standard](https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions).
    pub fn standard() -> Self {
        stdlib::standard_environment()
    }

    /// Create a [`GlobalsBuilder`] combining those functions in the Starlark standard plus
    /// all those defined in [`LibraryExtension`].
    pub fn extended() -> Self {
        Self::extended_by(LibraryExtension::all())
    }

    /// Create a [`GlobalsBuilder`] combining those functions in the Starlark standard plus
    /// all those defined in [`LibraryExtension`].
    pub fn extended_by(extensions: &[LibraryExtension]) -> Self {
        let mut res = Self::standard();
        for x in extensions {
            x.add(&mut res);
        }
        res
    }

    /// Add a nested struct to the builder. If `f` adds the definition `foo`,
    /// it will end up on a struct `name`, accessible as `name.foo`.
    /// This function cannot be called recursively from inside `f`.
    pub fn struct_(&mut self, name: &str, f: impl Fn(&mut GlobalsBuilder)) {
        assert!(
            self.struct_fields.is_none(),
            "Can't recursively nest GlobalsBuilder::struct_"
        );
        self.struct_fields = Some(SmallMap::new());
        f(self);
        let fields = mem::take(&mut self.struct_fields).unwrap();
        self.set(name, FrozenStruct { fields });
    }

    /// A fluent API for modifying [`GlobalsBuilder`] and returning the result.
    pub fn with(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }

    /// A fluent API for modifying [`GlobalsBuilder`] using [`struct_`](GlobalsBuilder::struct_).
    pub fn with_struct(mut self, name: &str, f: impl Fn(&mut GlobalsBuilder)) -> Self {
        self.struct_(name, f);
        self
    }

    /// Called at the end to build a [`Globals`].
    pub fn build(self) -> Globals {
        Globals(Arc::new(GlobalsData {
            heap: self.heap.into_ref(),
            variables: self.variables,
        }))
    }

    /// Set a value in the [`GlobalsBuilder`].
    pub fn set<'v, V: AllocFrozenValue>(&'v mut self, name: &str, value: V) {
        let name = name.to_owned();
        let value = value.alloc_frozen_value(&self.heap);
        match &mut self.struct_fields {
            None => self.variables.insert(name, value),
            Some(fields) => fields.insert(name, value),
        };
    }

    /// Allocate a value using the same underlying heap as the [`GlobalsBuilder`],
    /// only intended for values that are referred to by those which are passed
    /// to [`set`](GlobalsBuilder::set).
    pub fn alloc<'v, V: AllocFrozenValue>(&'v self, value: V) -> FrozenValue {
        value.alloc_frozen_value(&self.heap)
    }
}

/// Used to create static members for a [`StarlarkValue`](crate::values::StarlarkValue).
///
/// To define a method `foo()` on your type, define
///  usually written as:
///
/// ```ignore
/// fn my_members(builder: &mut GlobalsBuilder) {
///     fn foo(me: ARef<Foo>) -> NoneType {
///         ...
///     }
/// }
///
/// impl StarlarkValue<'_> for Foo {
///     ...
///     fn get_members(&self) -> Option<&'static Globals> {
///         static RES: GlobalsStatic = GlobalsStatic::new();
///         RES.members(module_creator)
///     }
///     ...
/// }
/// ```
pub struct GlobalsStatic(OnceCell<Globals>);

impl GlobalsStatic {
    /// Create a new [`GlobalsStatic`].
    pub const fn new() -> Self {
        Self(OnceCell::new())
    }

    fn globals(&'static self, x: impl FnOnce(&mut GlobalsBuilder)) -> &'static Globals {
        self.0.get_or_init(|| GlobalsBuilder::new().with(x).build())
    }

    /// Populate the globals with a builder function. Always returns `Some`, but using this API
    /// to be a better fit for [`StarlarkValue.get_members`](crate::values::StarlarkValue::get_members).
    pub fn members(&'static self, x: impl FnOnce(&mut GlobalsBuilder)) -> Option<&'static Globals> {
        Some(self.globals(x))
    }

    /// Get a function out of the object. Requires that the function passed only set a single
    /// value. If populated via a `#[starlark_module]`, that means a single function in it.
    pub fn function(&'static self, x: impl FnOnce(&mut GlobalsBuilder)) -> FrozenValue {
        let globals = self.globals(x);
        if globals.0.variables.len() != 1 {
            panic!(
                "GlobalsBuilder.function must have exactly 1 member, you had {:?}",
                globals.names()
            );
        }
        *globals.0.variables.values().next().unwrap()
    }
}

#[test]
fn test_send_sync()
where
    Globals: Send + Sync,
{
}
