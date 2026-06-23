//! Executable inventory for finishing the remaining Functions overload FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn assert_runtime_cases(cases: &[(&str, &str, Value)]) {
    for (name, source, expected) in cases {
        assert_eq!(run_source(source).expect(name), *expected, "{name}");
        assert_eq!(check_source(source).expect(name), Type::Int, "{name}");
    }
}

fn assert_check_rejects(cases: &[(&str, &str, &str)]) {
    for (name, source, expected_message) in cases {
        let error = check_source(source).expect_err(name);
        assert!(
            error.to_string().contains(expected_message),
            "{name}: expected error containing `{expected_message}`, got {error}"
        );
    }
}

#[test]
fn rejects_functions_overload_column_numeric_subtype_distinctness() {
    assert_check_rejects(&[
        (
            "int and rational overloads overlap",
            r#"
Choose(Value:int):int = 1
Choose(Value:rational):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "int range and int overloads overlap",
            r#"
Choose(Value:type{X:int where 0 <= X, X < 10}):int = 1
Choose(Value:int):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "float range and float overloads overlap",
            r#"
Choose(Value:type{X:float where 0.0 <= X, X <= 1.0}):int = 1
Choose(Value:float):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "number overlaps all numeric overloads",
            r#"
Choose(Value:number):int = 1
Choose(Value:float):int = 2
"#,
            "duplicate overload `Choose`",
        ),
    ]);
}

#[test]
fn rejects_functions_overload_column_type_value_family_distinctness() {
    assert_check_rejects(&[
        (
            "subtype bases overlap through subclass type values",
            r#"
base_item := class:
    Marker:int = 0
child_item := class(base_item):
    ChildMarker:int = 0
Choose(Kind:subtype(base_item)):int = 1
Choose(Kind:subtype(child_item)):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "castable subtype overlaps plain subtype family",
            r#"
base_tag := class<abstract><unique>:
    Marker:int = 0
castable_tag := class<concrete><castable>(base_tag):
    Value:int = 0
Choose(Kind:subtype(base_tag)):int = 1
Choose(Kind:castable_subtype(base_tag)):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "concrete castable subtype overlaps castable subtype family",
            r#"
base_tag := class<abstract><unique>:
    Marker:int = 0
castable_tag := class<concrete><castable>(base_tag):
    Value:int = 0
Choose(Kind:castable_subtype(base_tag)):int = 1
Choose(Kind:concrete_subtype(castable_subtype(base_tag))):int = 2
"#,
            "duplicate overload `Choose`",
        ),
        (
            "type value exact kind and bounded type value overlap",
            r#"
base_item := class:
    Marker:int = 0
child_item := class(base_item):
    ChildMarker:int = 0
Choose(Kind:type):int = 1
Choose(Kind:type(child_item, base_item)):int = 2
"#,
            "duplicate overload `Choose`",
        ),
    ]);
}

#[test]
fn evaluates_functions_overload_column_runtime_selection_surfaces() {
    assert_runtime_cases(&[
        (
            "captured local overload preserves closure scope after selection",
            r#"
Run()<transacts>:int =
    var Total:int = 0
    Pick(Value:int)<transacts>:int =
        set Total += 40
        Total
    Pick(Value:string):int =
        2
    Pick(1) + Pick("bonus")

Run()
"#,
            Value::Int(42),
        ),
        (
            "named overload selection uses defaults without runtime fallback",
            r#"
Scale(?Value:int, ?Factor:int = 2):int = Value * Factor
Scale(?Text:string):int = 2

Scale(?Value := 20) + Scale(?Text := "bonus")
"#,
            Value::Int(42),
        ),
        (
            "method overload selection dispatches by argument type through base receiver",
            r#"
base := class:
    Pick(Value:int):int = 40
    Pick(Value:string):int = 1

child := class(base):
    Pick<override>(Value:string):int = 2

Item:base = child{}
Item.Pick(1) + Item.Pick("bonus")
"#,
            Value::Int(42),
        ),
    ]);
}
