//! Executable inventory for finishing the remaining Options FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn assert_runtime_cases(cases: &[(&str, &str, Value)]) {
    for (name, source, expected) in cases {
        assert_eq!(run_source(source).expect(name), expected.clone(), "{name}");
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
#[ignore = "planned Options FT column: official option type former"]
fn planned_options_column_official_option_type_former() {
    assert_runtime_cases(&[
        (
            "option type former in annotations and containers",
            r#"
Filled:option(int) = option{40}
Empty:option(int) = false
Values:[]option(int) = array{Filled, Empty, option{2}}
First := if (Value := Values[0]?). Value else. 0
Second := if (Value := Values[1]?). Value else. 0
Third := if (Value := Values[2]?). Value else. 0
First + Second + Third
"#,
            Value::Int(42),
        ),
        (
            "type function returns option type former",
            r#"
Maybe(Kind:type):type = option(Kind)
Value:Maybe(int) = option{42}
if (Actual := Value?). Actual else. 0
"#,
            Value::Int(42),
        ),
        (
            "type value parameter flows through option type former",
            r#"
Use(Kind:type, Value:option(Kind))<computes>:int =
    if (Actual := Value?). Actual else. 0
Use(int, option{42})
"#,
            Value::Int(42),
        ),
        (
            "static type literal option source can be used as option type value",
            r#"
Pick():type = type{option{1}}
Value:option(Pick()) = option{42}
if (Actual := Value?). Actual else. 0
"#,
            Value::Int(42),
        ),
    ]);

    assert_check_rejects(&[
        (
            "option type former rejects missing item type",
            "Value:option() = false",
            "option type expects 1 type argument",
        ),
        (
            "option type former rejects extra item type",
            "Value:option(int, string) = false",
            "option type expects 1 type argument",
        ),
        (
            "option type former enforces item type",
            r#"Value:option(int) = option{"bad"}"#,
            "option<int>",
        ),
    ]);
}

#[test]
fn evaluates_option_source_effect_propagation() {
    assert_runtime_cases(&[
        (
            "computes option source can call decides computes",
            r#"
Pick(Value:int)<decides><computes>:int = Value
Use()<computes>:?int = option{Pick[42]}
if (Actual := Use()?). Actual else. 0
"#,
            Value::Int(42),
        ),
        (
            "reads option source can call decides reads",
            r#"
Pick(Value:int)<decides><reads>:int = Value
Use()<reads>:?int = option{Pick[42]}
if (Actual := Use()?). Actual else. 0
"#,
            Value::Int(42),
        ),
        (
            "transacts option source can call decides transacts and roll back failed writes",
            r#"
var Total:int = 0
WriteThenFail()<decides><transacts>:int =
    set Total = 99
    false?
    1
Use()<transacts>:?int = option{WriteThenFail[]}
if (Use()?):
    0
else:
    Total + 42
"#,
            Value::Int(42),
        ),
    ]);

    assert_check_rejects(&[
        (
            "computes option source rejects transacts callee",
            r#"
Pick(Value:int)<decides><transacts>:int = Value
Use()<computes>:?int = option{Pick[42]}
"#,
            "function with <computes> effect cannot call function requiring <transacts> effect",
        ),
        (
            "option source rejects decides writes callee in failure context",
            r#"
Pick(Value:int)<decides><writes>:int = Value
Use()<transacts>:?int = option{Pick[42]}
"#,
            "function with `<writes>` effect cannot be called in a failure context",
        ),
    ]);
}
