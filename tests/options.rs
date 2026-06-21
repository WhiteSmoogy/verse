//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn rejects_overload_option_and_logic_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Value:?int):int = 1
Choose(Value:logic):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rolls_back_option_failure_context_when_it_fails() {
    let source = r#"
var Total:int = 0
Maybe := option{block:
    set Total = 99
    false?
    1
}
if (Maybe?):
    0
else:
    Total + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runs_defer_in_failed_option_source() {
    let source = r#"
Maybe := option{block:
    defer:
        Err("option defer ran")
    false?
    42
}
if (Maybe?):
    0
else:
    42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );

    let error = run_source(source).expect_err("defer should run during failed option source");

    assert!(error.to_string().contains("option defer ran"));
}

#[test]
fn evaluates_if_failure_binding_option_query() {
    let source = r#"
Filled:?int = option{42}
Empty:?int = false
First := if (Value := Filled?). Value else. 0
Second := if (Value := Empty?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_option_query_without_binding() {
    let source = r#"
Filled:?int = option{1}
Empty:?int = false
First := if (Filled?). 40 else. 0
Second := if (Empty?). 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failable_option_query_inside_defer() {
    let error = check_source(
        r#"
Maybe:?int = false
defer:
    Maybe?
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`defer` block cannot contain failable expressions")
    );
}

#[test]
fn evaluates_for_failable_option_query_binding_clauses() {
    let source = r#"
Maybe:?int = option{10}
Empty:?int = false

Kept:[]int = for (I := 1..2, Value := Maybe?):
    I + Value

Dropped:[]int = for (I := 1..2, Value := Empty?):
    I + Value

if:
    First := Kept[0]
    Second := Kept[1]
then:
    Kept.Length + First + Second + Dropped.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(25));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_braced_block_sequence() {
    let source = r#"
Maybe:?int = option{
    Value := 40
    Value + 2
}
if (Value := Maybe?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_single_line_braced_block_sequence() {
    let source = r#"
Maybe:?int = option{Value := 40; Value + 2}
if (Value := Maybe?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_function_arguments_and_returns() {
    let source = r#"
Default(Value:?int)<computes>:int = if (Actual := Value?). Actual else. 7
Pack(Values:[]?int)<computes>:int =
    First := if (Value := Values[0]?). Value else. 10
    Second := if (Value := Values[1]?). Value else. 0
    First + Second
Empty()<computes>:?int = false

FromReturn := if (Value := Empty()?). Value else. 5
Default(false) + Default(option{30}) + Pack(array{false, option{40}}) + FromReturn
"#;

    assert_eq!(eval(source), Value::Int(92));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_logic_variable_as_contextual_option() {
    let error = check_source(
        r#"
Use(Value:?int):int = if (Actual := Value?). Actual else. 0
Flag:logic = false
Use(Flag)
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(
        message.contains("argument 1 expected `?int`, got `bool`"),
        "{message}"
    );
}

#[test]
fn captures_failure_in_option_braced_block_sequence() {
    let source = r#"
Values:[]int = array{40, 2}
Found:?int = option{
    First := Values[0]
    Second := Values[1]
    First + Second
}
Missing:?int = option{
    Value := Values[9]
    Value
}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_option_braced_block_sequence_when_it_fails() {
    let source = r#"
var Total:int = 0
Maybe:?int = option{
    set Total = 99
    false?
    1
}
if (Maybe?):
    0
else:
    Total + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_empty_option_false_assignment() {
    let source = r#"
var Maybe:?int = false
set Maybe = option{7}
set Maybe = false
set Maybe = option{42}
if (Value := Maybe?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn rejects_option_type_mismatch() {
    let error = check_source(r#"Maybe:?int = option{"bad"}"#).expect_err("source should fail");

    assert!(error.to_string().contains("?int"));
}

#[test]
fn rejects_unwrap_on_non_option() {
    let error = check_source("if (1?). 1 else. 0").expect_err("source should fail");
    assert!(error.to_string().contains("query operator expected"));
}

#[test]
fn rejects_empty_option_unwrap_outside_failure_context() {
    let source = r#"
Maybe:?int = false
Maybe?
"#;
    let error = run_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
}
