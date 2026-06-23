//! Executable inventory for finishing the remaining `for` FT.
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
fn evaluates_for_generator_iterable_effect_propagation() {
    assert_runtime_cases(&[
        (
            "computes function can iterate computes array source",
            r#"
Items()<computes>:[]int = array{40, 2}
Use()<computes>:int =
    Values:[]int = for (Item : Items()):
        Item
    if:
        First := Values[0]
        Second := Values[1]
    then:
        First + Second
    else:
        0
Use()
"#,
            Value::Int(42),
        ),
        (
            "reads function can iterate reads array source",
            r#"
var Seed:int = 40
Items()<reads>:[]int = array{Seed, 2}
Use()<reads>:int =
    Values:[]int = for (Item : Items()):
        Item
    if:
        First := Values[0]
        Second := Values[1]
    then:
        First + Second
    else:
        0
Use()
"#,
            Value::Int(42),
        ),
        (
            "transacts function can iterate transacts array source",
            r#"
var Seed:int = 0
Items()<transacts>:[]int =
    set Seed = 40
    array{2}
Use()<transacts>:int =
    Values:[]int = for (Item : Items()):
        Seed + Item
    if (Value := Values[0]). Value else. 0
Use()
"#,
            Value::Int(42),
        ),
        (
            "failable generator iterable succeeds in failure context",
            r#"
Items(Ok:logic)<decides><computes>:[]int =
    Ok?
    array{40, 2}
Use()<computes>:?int = option{
    Values:[]int = for (Item : Items[true]):
        Item
    First := Values[0]
    Second := Values[1]
    First + Second
}
if (Value := Use()?). Value else. 0
"#,
            Value::Int(42),
        ),
        (
            "failable generator iterable failure rolls back enclosing failure context",
            r#"
var Hits:int = 0
Items(Ok:logic)<decides><transacts>:[]int =
    set Hits += 1
    Ok?
    array{40, 2}
Use()<transacts>:?int = option{
    Values:[]int = for (Item : Items[false]):
        Item
    Values.Length
}
if (Value := Use()?). Value else. Hits
"#,
            Value::Int(0),
        ),
    ]);

    assert_check_rejects(&[
        (
            "computes function rejects transacts generator iterable source",
            r#"
Items()<transacts>:[]int = array{1}
Use()<computes>:[]int =
    for (Item : Items()):
        Item
"#,
            "function with <computes> effect cannot call function requiring <transacts> effect",
        ),
        (
            "failure context rejects no_rollback generator iterable source",
            r#"
Items():[]int = array{1}
Use()<transacts>:?[]int = option{
    for (Item : Items()):
        Item
}
"#,
            "function with `<no_rollback>` effect cannot be called in a failure context",
        ),
    ]);
}

#[test]
fn evaluates_for_body_control_flow_effect_integration() {
    assert_runtime_cases(&[
        (
            "return inside for body exits enclosing function",
            r#"
Find()<transacts>:int =
    for (Value := 1..3):
        if (Value = 2):
            return 42
        Value
    0
Find()
"#,
            Value::Int(42),
        ),
        (
            "return inside for body uses enclosing function return type",
            r#"
Make()<transacts>:[]int =
    for (Value := 1..3):
        if (Value = 2):
            return array{40, Value}
        Value
    array{}
Result := Make()
if:
    First := Result[0]
    Second := Result[1]
then:
    First + Second
else:
    0
"#,
            Value::Int(42),
        ),
        (
            "return inside nested for body exits before later iterations",
            r#"
Pick()<transacts>:int =
    for (Outer := 1..3, Inner := 1..3):
        if (Outer = 2):
            if (Inner = 1):
                return 42
        Outer * 10 + Inner
    0
Pick()
"#,
            Value::Int(42),
        ),
    ]);

    assert_check_rejects(&[
        (
            "return type mismatch inside for body is rejected",
            r#"
Make()<transacts>:[]int =
    for (Value := 1..3):
        return Value
    array{}
"#,
            "cannot return `int` from function returning `array<int>`",
        ),
        (
            "return outside function inside for body is rejected",
            r#"
for (Value := 1..3):
    return Value
"#,
            "`return` used outside a function",
        ),
        (
            "return inside for body remains forbidden in failure context",
            r#"
Use()<transacts>:?int = option{
    for (Value := 1..3):
        return Value
}
"#,
            "Explicit return out of a failure context is not allowed",
        ),
    ]);
}
