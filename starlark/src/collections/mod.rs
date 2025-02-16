/*
 * Copyright 2019 The Starlark in Rust Authors.
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

//! Defines [`SmallMap`] and [`SmallSet`] - collections with deterministic iteration and small memory footprint.
//!
//! These structures use vector backed storage if there are only a few elements, and [`IndexMap`](indexmap::IndexMap)
//! for larger collections. The API mirrors standard Rust collections.

pub use crate::collections::{
    hash::{BorrowHashed, Hashed, SmallHashResult},
    small_map::SmallMap,
    small_set::SmallSet,
};

mod hash;
mod idhasher;
mod small_map;
mod small_set;
mod vec_map;
