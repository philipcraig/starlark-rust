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

//! Implementation of `enum` function.
use crate as starlark;
use crate::{
    environment::GlobalsBuilder,
    values::{enumeration::EnumType, Value},
};

#[starlark_module]
pub fn global(builder: &mut GlobalsBuilder) {
    fn r#enum(args: Vec<Value>) -> Value<'v> {
        // Every Value must either be a field or a value (the type)
        EnumType::new(args, heap)
    }
}

#[cfg(test)]
mod tests {
    use crate::assert;

    #[test]
    fn test_enum() {
        assert::pass(
            r#"
enum_type = enum("option1", "option2", True)
x = enum_type("option1")
assert_eq(x.value, "option1")
assert_eq(enum_type(True).value, True)
"#,
        );
        assert::fails(
            r#"
enum_type = enum("option1", "option2", True)
enum_type(False)"#,
            &["Unknown enum element", "`False`", "option1"],
        );
        assert::fails(
            r#"
enum_type = enum("option1", "option2", True)
enum_type("option3")"#,
            &["Unknown enum element", "`option3`"],
        );
        assert::fails(
            r#"
enum_type = enum("option1", "option1")
enum_type("option3")"#,
            &["distinct", "option1"],
        );
        assert::pass(
            r#"
enum_type = enum("option1","option2")
def foo(x: enum_type.type) -> "enum_type":
    return x
foo(enum_type("option1"))"#,
        );
        assert::pass(
            r#"
v = [enum("option1","option2")]
def foo(x: v[0].type) -> "enum":
    return x
foo(v[0]("option1"))"#,
        );
        assert::pass(
            r#"
enum_type = enum("option1","option2")
assert_eq([enum_type[i].value for i in range(len(enum_type))], ["option1","option2"])
assert_eq(enum_type("option2").index, 1)
assert_eq([x.value for x in enum_type], ["option1","option2"])"#,
        );
        assert::pass(
            r#"
enum_type = enum("option1","option2")
x = enum_type("option1")
assert_eq(str(enum_type), "enum(\"option1\", \"option2\")")
assert_eq(str(x), "\"option1\"")
"#,
        );
        assert::pass(
            r#"
enum_type = enum("option1","option2")
repr(enum_type) # Check it is finite
"#,
        );
    }
}
