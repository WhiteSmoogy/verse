//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn rejects_overload_tuple_and_flattened_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Pair:tuple(int, int)):int = 1
Choose(A:int, B:int):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_destructured_tuple_default_overlapping_single_arg_call() {
    let error = check_source(
        r#"
Choose((Value:int, ?Bonus:int = 1)):int = Value + Bonus
Choose(Value:int):int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_destructured_tuple_named_defaults_overlapping_flattened_call() {
    let error = check_source(
        r#"
Choose((Value:int, ?Bonus:int = 1)):int = Value + Bonus
Choose(Value:int, ?Bonus:int = 2):int = Value + Bonus
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn expands_tuple_arguments_for_function_calls() {
    let source = r#"
Add(A:int, B:int):int = A + B
Args := (40, 2)
Add(Args)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn packs_flattened_arguments_for_tuple_parameters() {
    let source = r#"
Add(Pair:tuple(int, int)):int = Pair(0) + Pair(1)
Args := (10, 20)
Add(40, 2) + Add(Args)
"#;

    assert_eq!(eval(source), Value::Int(72));
}

#[test]
fn evaluates_external_tuple_as_runtime_value() {
    let source = r#"
Pair:tuple(int, int) = external {}
Pair(0)
Pair(1)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_destructured_tuple_parameters() {
    let source = r#"
Add(A:int, (B:int, C:int), D:int):int = A + B + C + D
Pair := (2, 3)
Args := (1, Pair, 4)
Add(1, Pair, 4) + Add(Args)
"#;

    assert_eq!(eval(source), Value::Int(20));
}

#[test]
fn evaluates_nested_destructured_tuple_parameters() {
    let source = r#"
Sum(A:int, (B:int, (C:int, D:int)), E:int):int = A + B + C + D + E
Inner := (3, 4)
Middle := (2, Inner)
Args := (1, Middle, 5)
Sum(1, Middle, 5) + Sum(Args)
"#;

    assert_eq!(eval(source), Value::Int(30));
}

#[test]
fn evaluates_destructured_tuple_named_default_subparameters() {
    let source = r#"
Add((Left:int, ?Right:int = 2)):int = Left + Right
Add(40) + Add(40, ?Right := 1) + Add((20, 22))
"#;

    assert_eq!(eval(source), Value::Int(125));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_destructured_tuple_named_default_subparameters_choose_overload() {
    let source = r#"
Pick((Value:int, ?Bonus:int = 2)):int = Value + Bonus
Pick((Value:string, ?Bonus:int = 2)):int = Bonus + 100
Pick(40) + Pick("text", ?Bonus := 1)
"#;

    assert_eq!(eval(source), Value::Int(143));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_duplicate_destructured_tuple_parameter_names() {
    let error = parse_source(
        r#"
Bad((Value:int, Value:int)):int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate parameter name"));
}

#[test]
fn rejects_destructured_tuple_named_subparameter_type_mismatch() {
    let error = check_source(
        r#"
Add((Left:int, ?Right:int = 2)):int = Left + Right
Add(40, ?Right := "bad")
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument `?Right` expected `int`, got `string`")
    );
}

#[test]
fn rejects_missing_required_destructured_tuple_named_subparameter() {
    let error = check_source(
        r#"
Add((Left:int, ?Right:int)):int = Left + Right
Add(40)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("missing required argument `?Right`")
    );
}

#[test]
fn rejects_positional_argument_for_destructured_tuple_named_subparameter() {
    let error = check_source(
        r#"
Add((Left:int, ?Right:int = 2)):int = Left + Right
Add(40, 2)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("positional argument does not match any positional parameter")
    );
}

#[test]
fn rejects_positional_destructured_tuple_subparameter_after_named_subparameter() {
    let error = parse_source(
        r#"
Bad((?Right:int = 2, Left:int)):int = Left + Right
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("positional parameters cannot follow named parameters")
    );
}

#[test]
fn rejects_destructured_tuple_parameter_type_mismatch() {
    let error = check_source(
        r#"
Add((Left:int, Right:int)):int = Left + Right
Add("bad", 2)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple argument item 1 expected `int`, got `string`")
    );
}

#[test]
fn evaluates_tuple_index_with_dynamic_int() {
    let source = r#"
Pair := (40, 2)
Index:int = 1
Pair(0) + Pair(Index)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_tuple_index_from_failable_int_result() {
    let source = r#"
Values:[]int = array{10, 20}
Pair := (40, 2)
Index := if (Value := Values.Find[20]). Value else. 0
Pair(0) + Pair(Index)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_tuple_type_annotations() {
    let source = r#"
Pair:tuple(int, string) = (1, "one")
Pair(1)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn checks_parenthesized_tuple_type_annotations() {
    let source = r#"
Pair:(int, string) = (1, "one")
Pair(1)
"#;

    assert_eq!(eval(source), Value::String("one".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_parenthesized_tuple_return_type_annotations() {
    let source = r#"
MakePair():(int, int) = (40, 2)
Pair := MakePair()
Pair(0) + Pair(1)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parenthesized_tuple_type_alias_annotations() {
    let source = r#"
pair_type := (int, int)
Pair:pair_type = (40, 2)
Pair(0) + Pair(1)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_single_element_parenthesized_tuple_type_annotation() {
    let error = parse_source("Value:(int) = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple type expects at least two element types")
    );
}

#[test]
fn rejects_empty_parenthesized_tuple_type_annotation() {
    let error = parse_source("Value:() = ()").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple type expects at least two element types")
    );
}

#[test]
fn evaluates_tuple_keys_in_maps() {
    let source = r#"
Grid:[tuple(int, int)]string = map{
    (0, 0) => "origin",
    (1, 0) => "east",
}
if (Value := Grid[(1, 0)]). Value else. ""
"#;

    assert_eq!(eval(source), Value::String("east".into()));
}

#[test]
fn rejects_tuple_index_out_of_bounds() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(2)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("out of bounds"));
}

#[test]
fn rejects_float_tuple_index() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(1.0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("tuple index expected `int`"));
}

#[test]
fn rejects_tuple_type_mismatch() {
    let error =
        check_source(r#"Pair:tuple(int, string) = (1, 2)"#).expect_err("source should fail");

    assert!(error.to_string().contains("tuple(int, string)"));
}

#[test]
fn evaluates_tuple_value_in_option_braced_block() {
    let source = r#"
Maybe:?tuple(int, string) = option{
    Value := (40, "two")
    Value
}
if (Pair := Maybe?). Pair(0) + ToString(Pair(1)).Length else. 0
"#;

    assert_eq!(eval(source), Value::Int(43));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_tuple_items() {
    let source = r#"
Pair:tuple(?int, ?int) = (false, option{41})
First := if (Value := Pair(0)?). Value else. 1
Second := if (Value := Pair(1)?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
