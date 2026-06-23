//! Executable inventory for finishing the Prediction/host FT.
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
fn evaluates_prediction_host_column_predicts_field_accessors() {
    assert_runtime_cases(&[
        (
            "method reads and writes non-var predicts field",
            r#"
sync_state := class:
    State<predicts>:int = 0
    Put(Value:int)<transacts>:void =
        set State = Value
    Get():int = State

Run()<transacts>:int =
    Item := sync_state{}
    Item.Put(21)
    Item.Get() * 2

Run()
"#,
            Value::Int(42),
        ),
        (
            "external member access reads and writes predicts field",
            r#"
sync_state := class:
    State<predicts>:int = 0

Run()<transacts>:int =
    Item := sync_state{}
    set Item.State = 42
    Item.State

Run()
"#,
            Value::Int(42),
        ),
        (
            "class block writes predicts field before method reads it",
            r#"
sync_state := class:
    State<predicts>:int = 0
    block:
        set State = 42
    Get():int = State

sync_state{}.Get()
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
#[ignore = "planned Prediction/host FT column: predicts extern host storage"]
fn evaluates_prediction_host_column_predicts_extern_storage() {
    assert_runtime_cases(&[
        (
            "predicts extern keeps ordinary source default without host override",
            r#"
sync_state := class:
    @predicts_extern
    State<predicts>:int = 40

sync_state{}.State + 2
"#,
            Value::Int(42),
        ),
        (
            "predicts extern field writes remain visible through later accessor reads",
            r#"
sync_state := class:
    @predicts_extern
    State<predicts>:int = 0
    Put(Value:int)<transacts>:void =
        set State = Value

Run()<transacts>:int =
    Item := sync_state{}
    Item.Put(42)
    Item.State

Run()
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
#[ignore = "planned Prediction/host FT column: predicts boundaries and ordinary fields"]
fn rejects_prediction_host_column_invalid_predicts_boundaries() {
    assert_check_rejects(&[
        (
            "ordinary immutable class field still rejects assignment",
            r#"
sync_state := class:
    State:int = 0
    Put(Value:int)<transacts>:void =
        set State = Value
"#,
            "cannot assign to immutable binding `State`",
        ),
        (
            "ordinary immutable member field still rejects assignment",
            r#"
sync_state := class:
    State:int = 0

Run()<transacts>:void =
    Item := sync_state{}
    set Item.State = 42
"#,
            "cannot assign to immutable field `State`",
        ),
        (
            "interface predicts field remains rejected",
            r#"
sync_state := interface:
    State<predicts>:int
"#,
            "`predicts` field specifier can only be used on class fields",
        ),
    ]);
}
