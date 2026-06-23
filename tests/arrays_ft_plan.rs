//! Executable inventory for finishing the remaining Arrays FT.
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
fn evaluates_official_array_replace_function_surface() {
    assert_runtime_cases(&[
        (
            "replace middle range with longer array",
            r#"
Result := if (Updated := Replace[array{10, 20, 30, 40}, 1, 3, array{7, 8, 9}]). Updated else. array{}
if:
    A := Result[0]
    B := Result[1]
    C := Result[3]
    D := Result[4]
then:
    Result.Length * 10000 + A * 1000 + B * 100 + C * 10 + D
else:
    0
"#,
            Value::Int(60_830),
        ),
        (
            "replace full range with empty array",
            r#"
Result := if (Updated := Replace[array{1, 2, 3}, 0, 3, array{}]). Updated else. array{99}
Result.Length + 42
"#,
            Value::Int(42),
        ),
        (
            "failed invalid range is captured",
            r#"
if (Updated := Replace[array{1, 2, 3}, 3, 1, array{9}]):
    Updated.Length
else:
    42
"#,
            Value::Int(42),
        ),
        (
            "computes decides call is allowed in failure context",
            r#"
Use()<computes>:?[]int = option{Replace[array{1, 2}, 1, 2, array{42}]}
if:
    Values := Use()?
    Value := Values[1]
then:
    Value
else:
    0
"#,
            Value::Int(42),
        ),
    ]);

    assert_check_rejects(&[
        (
            "input must be an array",
            r#"
if (Value := Replace[1, 0, 0, array{2}]):
    0
else:
    1
"#,
            "argument `Input` expected `array`",
        ),
        (
            "replacement must match input item type",
            r#"
if (Value := Replace[array{1}, 0, 1, array{"bad"}]):
    0
else:
    1
"#,
            "argument `ElementsToReplaceWith` expected `array<int>`",
        ),
        (
            "indexes must be int",
            r#"
if (Value := Replace[array{1}, 0.0, 1, array{2}]):
    0
else:
    1
"#,
            "`Replace` StartIndex expected `int`",
        ),
    ]);
}

#[test]
#[ignore = "planned Arrays FT column: Concatenate computes effect metadata"]
fn planned_arrays_column_concatenate_computes_effect_metadata() {
    assert_runtime_cases(&[(
        "concatenate can be called from computes failure context",
        r#"
Use()<computes>:?[]int = option{
    Values := Concatenate(array{array{40}, array{2}})
    Values[0] = 40
    Values
}
if:
    Values := Use()?
    First := Values[0]
    Second := Values[1]
then:
    First + Second
else:
    0
"#,
        Value::Int(42),
    )]);
}
