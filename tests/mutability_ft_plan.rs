//! Executable inventory for finishing the remaining Mutability FT.
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
fn evaluates_mutability_column_non_local_mutation_runtime() {
    assert_runtime_cases(&[
        (
            "local transacting function mutates captured variable",
            r#"
Run()<transacts>:int =
    var Total:int = 0
    Bump()<transacts>:void =
        set Total += 40
    Bump()
    set Total += 2
    Total

Run()
"#,
            Value::Int(42),
        ),
        (
            "returned transacting closure mutates captured variable",
            r#"
MakeCounter()<transacts>:type{_()<transacts>:int} =
    var Count:int = 0
    Next()<transacts>:int =
        set Count += 1
        Count
    Next

Counter := MakeCounter()
Counter() + Counter() + 39
"#,
            Value::Int(42),
        ),
        (
            "nested local function sees outer mutation between calls",
            r#"
Run()<transacts>:int =
    var Total:int = 10
    Read()<transacts>:int = Total
    set Total += 30
    Read() + 2

Run()
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
#[ignore = "planned Mutability FT column: captured aggregate mutation and rollback"]
fn evaluates_mutability_column_captured_aggregate_mutation_and_rollback() {
    assert_runtime_cases(&[
        (
            "captured array and map slots write through returned closure",
            r#"
MakeUpdater()<transacts>:type{_()<transacts>:int} =
    var Values:[]int = array{1, 2}
    var Scores:[string]int = map{"base" => 10}
    Update()<transacts>:int =
        if:
            set Values[0] = 20
            set Scores["base"] += 20
            Value := Values[0]
            Score := Scores["base"]
        then:
            Value + Score - 8
        else:
            0
    Update

MakeUpdater()()
"#,
            Value::Int(42),
        ),
        (
            "captured computes struct field mutation writes back through closure",
            r#"
point := struct<computes>:
    X:int = 0

MakeMover()<transacts>:type{_()<transacts>:int} =
    var Position:point = point{X := 10}
    Move()<transacts>:int =
        set Position.X += 32
        Position.X
    Move

MakeMover()()
"#,
            Value::Int(42),
        ),
        (
            "failed failure context rolls back captured aggregate mutations",
            r#"
Run()<transacts>:int =
    var Values:[]int = array{1}
    var Total:int = 0
    Bump()<transacts>:int =
        if:
            set Values[0] = 40
            set Total += 40
            Missing := Values[2]
        then:
            Missing
        else:
            if (Value := Values[0]). Value + Total + 40 else. 0
    Bump()

Run()
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
#[ignore = "planned Mutability FT column: non-local effect and capability checks"]
fn rejects_mutability_column_non_local_effect_and_capability_mismatches() {
    assert_check_rejects(&[
        (
            "captured mutation still requires write-capable local function effect",
            r#"
Run()<transacts>:int =
    var Total:int = 0
    Bump():void =
        set Total += 1
    Bump()
    Total

Run()
"#,
            "mutable assignment in function requires `<writes>` or `<transacts>` effect",
        ),
        (
            "outer computes function cannot call captured transacting mutator",
            r#"
Run()<computes>:int =
    var Total:int = 0
    Bump()<transacts>:void =
        set Total += 1
    Bump()
    Total

Run()
"#,
            "function with <computes> effect cannot call function requiring <transacts> effect",
        ),
        (
            "returned closure cannot erase captured mutation effect",
            r#"
MakeCounter()<transacts>:type{_():int} =
    var Count:int = 0
    Next()<transacts>:int =
        set Count += 1
        Count
    Next

MakeCounter()()
"#,
            "annotated as `function/0 -> int`",
        ),
    ]);
}
