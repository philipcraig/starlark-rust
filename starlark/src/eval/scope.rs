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

use crate::{
    environment::names::MutableNames,
    syntax::ast::{AstExpr, AstStmt, Expr, Stmt, Visibility},
};
use std::collections::HashMap;

pub(crate) struct Scope<'a> {
    module: &'a MutableNames,
    // The first locals is the anon-slots for load() and comprehensions at the module-level
    // The rest are anon-slots for functions (which include their comprehensions)
    locals: Vec<ScopeNames>,
    unscopes: Vec<Unscope>,
}

#[derive(Default)]
struct Unscope(Vec<(String, Option<usize>)>);

#[derive(Default, Debug)]
pub(crate) struct ScopeNames {
    /// The number of slots this scope uses, including for parameters and `parent`.
    /// The next required slot would be at index `used`.
    pub used: usize,
    /// The names that are in this scope
    pub mp: HashMap<String, usize>,
    /// Slots to copy from the parent. (index in parent, index in child).
    /// Module-level identifiers are not copied over, to avoid excess copying.
    pub parent: Vec<(usize, usize)>,
}

impl ScopeNames {
    fn copy_parent(&mut self, parent: usize, name: &str) -> usize {
        assert!(self.get_name(name).is_none()); // Or we'll be overwriting our variable
        let res = self.add_name(name);
        self.parent.push((parent, res));
        res
    }

    fn next_slot(&mut self) -> usize {
        let res = self.used;
        self.used += 1;
        res
    }

    fn add_name(&mut self, name: &str) -> usize {
        match self.mp.get(name) {
            Some(v) => *v,
            None => {
                let slot = self.next_slot();
                self.mp.insert(name.to_owned(), slot);
                slot
            }
        }
    }

    fn add_scoped(&mut self, name: &str, unscope: &mut Unscope) -> usize {
        let slot = self.next_slot();
        let undo = match self.mp.get_mut(name) {
            Some(v) => {
                let old = *v;
                *v = slot;
                Some(old)
            }
            None => {
                self.mp.insert(name.to_owned(), slot);
                None
            }
        };
        unscope.0.push((name.to_owned(), undo));
        slot
    }

    fn unscope(&mut self, unscope: Unscope) {
        for (name, v) in unscope.0.iter().rev() {
            match v {
                None => {
                    self.mp.remove(name);
                }
                Some(v) => *self.mp.get_mut(name).unwrap() = *v,
            }
        }
    }

    fn get_name(&self, name: &str) -> Option<usize> {
        self.mp.get(name).copied()
    }
}

pub(crate) enum Slot {
    Module(usize), // Top-level module scope
    Local(usize),  // Local scope, always mutable
}

impl<'a> Scope<'a> {
    pub fn enter_module(module: &'a MutableNames, code: &AstStmt) -> Self {
        let mut locals = HashMap::new();
        Stmt::collect_defines(code, &mut locals);
        let mut module_private = ScopeNames::default();
        for (x, vis) in locals {
            match vis {
                Visibility::Public => module.add_name(x),
                Visibility::Private => module_private.add_name(x),
            };
        }
        Self {
            module,
            locals: vec![module_private],
            unscopes: Vec::new(),
        }
    }

    // Number of module slots I need, number of local anon slots I need
    pub fn exit_module(mut self) -> (usize, usize) {
        assert!(self.locals.len() == 1);
        assert!(self.unscopes.is_empty());
        let scope = self.locals.pop().unwrap();
        assert!(scope.parent.is_empty());
        (self.module.slot_count(), scope.used)
    }

    pub fn enter_def<'s>(&mut self, params: impl Iterator<Item = &'s str>, code: &AstStmt) {
        let mut names = ScopeNames::default();
        for p in params {
            // Subtle invariant: the slots for the params must be ordered and at the
            // beginning
            names.add_name(p);
        }
        let mut locals = HashMap::new();
        Stmt::collect_defines(code, &mut locals);
        // Note: this process introduces non-determinism, as the defines are collected in different orders each time
        for x in locals.into_iter() {
            names.add_name(x.0);
        }
        self.locals.push(names);
    }

    // Which slots to grab from the current scope to the parent scope, size of your
    // self scope Future state: Should return the slots to use from the parent
    // scope
    pub fn exit_def(&mut self) -> ScopeNames {
        self.locals.pop().unwrap()
    }

    pub fn enter_compr(&mut self) {
        self.unscopes.push(Unscope::default());
    }

    pub fn add_compr(&mut self, var: &AstExpr) {
        let mut locals = HashMap::new();
        Expr::collect_defines_lvalue(var, &mut locals);
        for k in locals.into_iter() {
            self.locals
                .last_mut()
                .unwrap()
                .add_scoped(k.0, self.unscopes.last_mut().unwrap());
        }
    }

    pub fn exit_compr(&mut self) {
        self.locals
            .last_mut()
            .unwrap()
            .unscope(self.unscopes.pop().unwrap());
    }

    pub fn get_name(&mut self, name: &str) -> Option<Slot> {
        // look upwards to find the first place the variable occurs
        // then copy that variable downwards
        for i in (0..self.locals.len()).rev() {
            if let Some(mut v) = self.locals[i].get_name(name) {
                for j in (i + 1)..self.locals.len() {
                    v = self.locals[j].copy_parent(v, name);
                }
                return Some(Slot::Local(v));
            }
        }
        match self.module.get_name(name) {
            None => None,
            Some(v) => Some(Slot::Module(v)),
        }
    }

    pub fn get_name_or_panic(&mut self, name: &str) -> Slot {
        self.get_name(name).unwrap_or_else(|| {
            panic!(
                "Scope::get_name, internal error, entry missing from scope table `{}`",
                name
            )
        })
    }
}
