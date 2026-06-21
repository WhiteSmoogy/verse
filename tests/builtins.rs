//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn converts_values_to_strings() {
    assert_eq!(
        eval(r#""value=" + str(42)"#),
        Value::String("value=42".into())
    );
}

#[test]
fn rejects_message_assigned_to_string() {
    let error = check_source(
        r#"
Greeting<localizes>:message = "Hello"
Text:string = Greeting
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Text` is annotated as `string` but expression has type `message`")
    );
}

#[test]
fn evaluates_localize_message_function() {
    let source = r#"
Greeting<localizes>:message = "Hello"
Localize(Greeting)
"#;

    assert_eq!(eval(source), Value::String("Hello".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_message_function() {
    let source = r#"
First<localizes>:message = "Hello"
Second<localizes>:message = "Verse"
Separator<localizes>:message = ", "
Joined:message = Join(array{First, Second}, Separator)
Localize(Joined)
"#;

    assert_eq!(eval(source), Value::String("Hello, Verse".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_string_function() {
    let source = r#"
Joined:string = Join(array{"A", "B", "C"}, " + ")
Joined
"#;

    assert_eq!(eval(source), Value::String("A + B + C".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_message_function_with_string_literals() {
    let source = r#"
Joined:message = Join(array{"A", "B", "C"}, " + ")
Localize(Joined)
"#;

    assert_eq!(eval(source), Value::String("A + B + C".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_localize_non_message_argument() {
    let error = check_source("Localize(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `message`, got `int`")
    );
}

#[test]
fn evaluates_to_string_number_function() {
    let source = "ToString(42)";

    assert_eq!(eval(source), Value::String("42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_float_function() {
    let source = "ToString(42.5)";

    assert_eq!(eval(source), Value::String("42.5".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_string_function() {
    let source = r#"ToString("Ready")"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_char_array_function() {
    let source = r#"
Text:[]char = array{'O', 'K'}
ToString(Text)
"#;

    assert_eq!(eval(source), Value::String("OK".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_char_escape_literals_and_to_string() {
    let source = r#"
LineFeed:char = '\n'
Text := ToString(LineFeed)
if (Text.Length = 1 and Text[0] = '\n'):
    Text
else:
    ToString('x')
"#;

    assert_eq!(eval(source), Value::String("\n".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn rejects_join_non_string_or_message_array() {
    let error = check_source(r#"Join(array{1, 2}, ", ")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (array<int>, string)")
    );
}

#[test]
fn rejects_to_string_logic_argument() {
    let error = check_source("ToString(true)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (bool)")
    );
}

#[test]
fn rejects_to_string_rational_argument() {
    let error = check_source("if (Value := 1 / 2). ToString(Value) else. \"\"")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (rational)")
    );
}

#[test]
fn runtime_errors_on_to_string_rational_argument() {
    let error = Interpreter::new()
        .eval_source("ToString(1 / 2)")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`ToString` expected `float`, `int`, `[]char`, or `char`, got rational")
    );
}

#[test]
fn rejects_to_string_message_argument() {
    let error = check_source(
        r#"
Greeting<localizes>:message = "Hello"
ToString(Greeting)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (message)")
    );
}

#[test]
fn evaluates_official_random_functions() {
    let source = r#"
using { /Verse.org/Random }
FixedInt:int = GetRandomInt(5, 5)
OutOfOrder:int = GetRandomInt(10, 8)
FixedFloat:float = GetRandomFloat(2.0, 2.0)
Shuffled:[]int = Shuffle(array{35})
Bounds := if (OutOfOrder >= 8 and OutOfOrder <= 10). 0 else. 1000
Rounded := if (Value := Round[FixedFloat]). Value else. 0
ShuffledValue := if (Value := Shuffled[0]). Value else. 0
FixedInt + Rounded + ShuffledValue + Bounds
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_random_functions_with_ordinary_named_arguments() {
    let source = r#"
using { /Verse.org/Random }
FixedInt:int = GetRandomInt(High := 40, Low := 40)
FixedFloat:float = GetRandomFloat(High := 2.0, Low := 2.0)
Shuffled:[]int = Shuffle(Input := array{0})
Rounded := if (Value := Round[FixedFloat]). Value else. 0
ShuffledValue := if (Value := Shuffled[0]). Value else. 0
FixedInt + Rounded + ShuffledValue
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_get_random_int_argument_type_mismatch() {
    let error = check_source("GetRandomInt(0.0, 1.0)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `int`, got `float`")
    );
}

#[test]
fn rejects_get_random_float_argument_type_mismatch() {
    let error = check_source(r#"GetRandomFloat("low", 1.0)"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `float`, got `string`")
    );
}

#[test]
fn rejects_shuffle_non_array_argument() {
    let error = check_source("Shuffle(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `array`, got `int`")
    );
}

#[test]
fn rejects_shuffle_extra_arguments() {
    let error = check_source("Shuffle(1, 2)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Shuffle` expected 1 arguments, got 2")
    );
}

#[test]
fn checks_err_as_never_returning_function() {
    let source = r#"
Value:int = Err("fatal")
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_err_function() {
    let error = Interpreter::new()
        .eval_source(r#"Err("fatal stop")"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("fatal stop"));
}

#[test]
fn evaluates_official_print_function() {
    let source = r#"
Greeting<localizes>:message = "Hello"
Print("Ready")
Print(Greeting)
Print(ToDiagnostic("diag"))
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn evaluates_official_print_ordinary_named_arguments() {
    let source = r#"
Print(Message := "Ready", Duration := 1.5, Color := MakeColorFromSRGB(Red := 1.0, Green := 0.0, Blue := 0.0))
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn rejects_print_non_text_argument() {
    let error = check_source("Print(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (int)")
    );
}

#[test]
fn rejects_print_bad_duration_argument() {
    let error =
        check_source(r#"Print("Ready", ?Duration := "slow")"#).expect_err("source should fail");

    assert!(error.to_string().contains("no overload matches"));
}

#[test]
fn evaluates_to_diagnostic_function() {
    let source = r#"
Entry:diagnostic = ToDiagnostic(42)
Entry
"#;

    assert!(matches!(eval(source), Value::Diagnostic(_)));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("diagnostic".into())
    );
}

#[test]
fn evaluates_diagnostic_concatenation() {
    let source = r#"
Entry:diagnostic = ToDiagnostic("ready") + "!" + ToDiagnostic(7)
Result:diagnostic = ">" + Entry
Result
"#;

    assert_eq!(eval(source), Value::Diagnostic(">ready!7".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("diagnostic".into())
    );
}

#[test]
fn rejects_diagnostic_assigned_to_string() {
    let error =
        check_source(r#"Text:string = ToDiagnostic("ready")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `string` but expression has type `diagnostic`")
    );
}

#[test]
fn rejects_diagnostic_plus_non_string_or_diagnostic() {
    let error = check_source("ToDiagnostic(1) + 2").expect_err("source should fail");

    assert!(error.to_string().contains("diagnostics"));
}

#[test]
fn evaluates_get_seconds_since_epoch_function() {
    let source = "GetSecondsSinceEpoch()";
    let value = eval(source);

    assert!(matches!(value, Value::Number(seconds) if seconds > 0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn get_seconds_since_epoch_is_stable_within_eval_transaction() {
    let source = r#"
First := GetSecondsSinceEpoch()
Second := GetSecondsSinceEpoch()
if (First = Second). true else. false
"#;

    assert_eq!(eval(source), Value::Bool(true));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Bool
    );
}

#[test]
fn rejects_get_seconds_since_epoch_arguments() {
    let error = check_source("GetSecondsSinceEpoch(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn evaluates_concatenate_and_shuffle_value_copy_semantics() {
    let source = r#"
row := []int
grid := []row

var Rows:grid = array{array{1, 2}}
Combined:grid = Concatenate(array{Rows, array{array{3, 4}}})
Flattened:[]int = Concatenate(array{array{5, 6}})
Shuffled:grid = Shuffle(Rows)
if:
    set Rows[0][1] = 9
then:
    {}
else:
    {}

if:
    CombinedValue := Combined[0][1]
    FlattenedValue := Flattened[1]
    ShuffledValue := Shuffled[0][1]
    RowsValue := Rows[0][1]
then:
    CombinedValue * 1000 + FlattenedValue * 100 + ShuffledValue * 10 + RowsValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(2629.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
