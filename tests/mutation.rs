//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_writes_function_mutable_assignment() {
    let source = r#"
Bump()<writes>:int =
    var Total:int = 0
    set Total += 42
    Total

Bump()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_writes_function_container_slot_assignment() {
    let source = r#"
ReplaceFirst()<writes>:int =
    var Values:[]int = array{1}
    if:
        set Values[0] = 42
        Value := Values[0]
    then:
        Value
    else:
        0

ReplaceFirst()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_mutable_assignment_in_function_without_write_effect() {
    let error = check_source(
        r#"
Bump():int =
    var Total:int = 0
    set Total += 1
    Total
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<writes>` or `<transacts>` effect")
    );
}

#[test]
fn rejects_mutable_assignment_in_reads_function() {
    let error = check_source(
        r#"
Bump()<reads>:int =
    var Total:int = 0
    set Total += 1
    Total
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<writes>` or `<transacts>` effect")
    );
}

#[test]
fn rejects_container_slot_assignment_in_function_without_write_effect() {
    let error = check_source(
        r#"
ReplaceFirst():int =
    var Values:[]int = array{1}
    if:
        set Values[0] = 2
    then:
        Values.Length
    else:
        0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<writes>` or `<transacts>` effect")
    );
}

#[test]
fn evaluates_set_expression_in_if_condition() {
    let source = r#"
var TeamSize:int = 0
Sizes:[string]int = map{"red" => 42}
Result := if (set TeamSize = Sizes["red"]):
    TeamSize
else:
    0
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_set_expression_assignment_without_write_effect() {
    let error = check_source(
        r#"
var Total:int = 0
Bad():int =
    if (set Total = 42):
        Total
    else:
        0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<writes>` or `<transacts>` effect")
    );
}

#[test]
fn rejects_set_expression_to_immutable_binding() {
    let error = check_source(
        r#"
Total:int = 0
if (set Total = 42):
    1
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable binding `Total`")
    );
}

#[test]
fn rejects_plain_set_expression_as_only_if_condition() {
    let error = check_source(
        r#"
var Total:int = 0
if (set Total = 42):
    1
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`if` condition must contain at least one failable expression")
    );
}

#[test]
fn evaluates_public_field_with_protected_var_assignment_inside_subclass() {
    let source = r#"
weapon := class:
    var<protected> Ammo<public>:int = 10

rifle := class(weapon):
    Reload()<transacts>:void =
        set Self.Ammo = 15

Gun := rifle{}
Gun.Reload()
Gun.Ammo
"#;

    assert_eq!(eval(source), Value::Int(15));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_private_var_assignment_from_subclass_with_protected_read() {
    let error = check_source(
        r#"
base_counter := class:
    var<private> Value<protected>:int = 0

child_counter := class(base_counter):
    Bump()<transacts>:void =
        set Self.Value = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Value` is private to class `base_counter`")
    );
}

#[test]
fn rejects_non_access_var_field_specifier() {
    let error = parse_source(
        r#"
counter := class:
    var<final> Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported var field specifier `final`")
    );
}

#[test]
fn evaluates_var_declaration_expression() {
    let source = r#"
Initial := (var Total:int = 40)
set Total += 2
Initial + Total
"#;

    assert_eq!(eval(source), Value::Int(82));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_var_expression_without_type_annotation() {
    let error = parse_source("Value := (var Temp = 1)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected explicit type annotation after `var` name")
    );
}

#[test]
fn rejects_var_expression_colon_equal_initializer() {
    let error = parse_source("Value := (var Temp:int := 1)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `=` after variable declaration")
    );
}

#[test]
fn rejects_var_expression_type_mismatch() {
    let error = check_source(r#"Value := (var Temp:int = "bad")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Temp` is annotated as `int` but expression has type `string`")
    );
}

#[test]
fn rejects_var_expression_as_set_target() {
    let error = parse_source("set (var Temp:int = 0) = 1").expect_err("source should fail");

    assert!(error.to_string().contains("expected assignment target"));
}

#[test]
fn evaluates_compound_assignments_to_slots_and_fields() {
    let source = r#"
score := class:
    var Value:int = 10

var Values:[]int = array{10, 20}
if:
    set Values[0] *= 3
then:
    {}
else:
    {}
Item := score{}
set Item.Value -= 8
if (Value := Values[0]). Value + Item.Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(32));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_assignment_to_immutable_bindings() {
    let error = check_source(
        r#"
x := 1
set x = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("immutable binding `x`"));
}

#[test]
fn rejects_var_without_explicit_type() {
    let error = parse_source("var Score = 0").expect_err("source should fail");
    assert!(error.to_string().contains("explicit type"));
}
