//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_block_expressions() {
    let source = r#"
Result:int = block:
    X:int = 20
    Y:int = 22
    X + Y
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn block_expressions_scope_local_bindings() {
    let error = check_source(
        r#"
Result:int = block:
    Local:int = 42
    Local
Local
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Local`"));
}

#[test]
fn evaluates_defer_inside_block_expression() {
    let source = r#"
var CleanupLog:string = ""
Result:int = block:
    defer:
        set CleanupLog = CleanupLog + "D"
    42
str(Result) + ":" + CleanupLog
"#;

    assert_eq!(eval(source), Value::String("42:D".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_official_profile_expression_value() {
    let source = r#"
Result:int = profile("Finding a number"):
    40 + 2
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_profile_expression_with_description_variable() {
    let source = r#"
Description:string = "Merge sort"
Result:int = profile(Description):
    array{1}.Length()
Result
"#;

    assert_eq!(eval(source), Value::Number(1.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_profile_expression_wrong_argument_count() {
    let error = parse_source(
        r#"
profile("one", "two"):
    1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("profile expression expects exactly one description argument")
    );
}

#[test]
fn rejects_official_profile_expression_without_colon_block() {
    let error = parse_source(r#"profile("Finding a number")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `:` after profile description")
    );
}

#[test]
fn evaluates_computes_function_call_in_if_condition() {
    let source = r#"
IsReady()<computes>:logic = true
if (IsReady()?):
    42
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_colon_blocks() {
    let source = r#"
Truth:logic = true
if (Truth?):
    42
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_dot_blocks() {
    let source = r#"
if (false?). 0 else. 42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_if_then_block_without_then() {
    let error = parse_source(
        r#"
if:
    true?
    42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `then:`"));
}

#[test]
fn evaluates_if_optional_member_query() {
    let source = r#"
player := class:
    Name : string = "Ava"

Filled:?player = option{player{}}
Empty:?player = false

First := if (Name := Filled?.Name). Name else. "missing"
Second := if (Name := Empty?.Name). Name else. "none"
First + ":" + Second
"#;

    assert_eq!(eval(source), Value::String("Ava:none".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_question_mark_for_ordinary_parameter() {
    let error = check_source(
        r#"
Scale(Value:int):int = Value
Scale(?Value := 1)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parameter `Value` is not a named parameter")
    );
}

#[test]
fn rejects_unreachable_statement_after_break() {
    let error = check_source(
        r#"
Bad():void =
    loop:
        break
        Value:int = 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unreachable code after `break`"));
}

#[test]
fn rejects_unreachable_statement_after_never_if_initializer() {
    let error = check_source(
        r#"
Bad(Ready:logic):int = {
    Value:int = if (Ready?) {
        Err("left")
    } else {
        Err("right")
    }
    42
}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unreachable code after never-returning expression")
    );
}

#[test]
fn checks_reachable_statement_after_partially_never_if_initializer() {
    let source = r#"
Good(Ready:logic):int = {
    Value:int = if (Ready?) {
        Err("fatal")
    } else {
        40
    }
    Value + 2
}
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (Some(1), Vec::new(), Some(vec![Type::Bool]), Type::Int)
    );
}

#[test]
fn evaluates_subtype_comparable_constraint_for_inline_function_parameter() {
    let source = r#"
CountComparable(Items:[]t, Compare(L:t, R:t)<decides><transacts>:t where t:subtype(comparable))<transacts>:int =
    Items.Length

SameInt(Left:int, Right:int)<decides><transacts>:int =
    Left = Right
    Left

CountComparable(array{1, 1}, SameInt)
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_subtype_comparable_constraint_for_inline_function_parameter() {
    let source = r#"
Run(Items:[]t, Compare(L:t, R:t)<decides><transacts>:t where t:subtype(comparable))<transacts>:int =
    Items.Length

holder := class:
    Callback:type{_():int}

MakeNumber():int =
    1

Same(Left:holder, Right:holder)<decides><transacts>:holder =
    Left

Run(array{holder{Callback := MakeNumber}}, Same)
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `comparable`")
    );
}

#[test]
fn evaluates_loop_with_break() {
    let source = r#"
var i: int = 1
var total: int = 0
loop {
    if (i > 5) {
        break
    }
    set total += i
    set i += 1
}
total
"#;

    assert_eq!(eval(source), Value::Number(15.0));
}

#[test]
fn evaluates_loop_colon_blocks() {
    let source = r#"
var I:int = 0
loop:
    if (I = 3):
        break
    set I += 1
I
"#;

    assert_eq!(eval(source), Value::Number(3.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_loop_dot_break_blocks() {
    let source = r#"
var I:int = 0
loop:
    set I += 1
    if (I = 3). break
I
"#;

    assert_eq!(eval(source), Value::Number(3.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_defer_on_scope_exit_in_lifo_order() {
    let source = r#"
var CleanupLog:string = ""
{
    defer:
        set CleanupLog = CleanupLog + "A"
    defer:
        set CleanupLog = CleanupLog + "B"
    set CleanupLog = CleanupLog + "C"
}
CleanupLog
"#;

    assert_eq!(eval(source), Value::String("CBA".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_defer_before_return() {
    let source = r#"
var CleanupLog:string = ""
WithCleanup()<transacts>:int = {
    defer:
        set CleanupLog = CleanupLog + "D"
    return 42
}
Result:int = WithCleanup()
Result + if (CleanupLog = "D"). 0 else. 100
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_defer_before_loop_break() {
    let source = r#"
var CleanupLog:string = ""
loop:
    defer:
        set CleanupLog = CleanupLog + "D"
    break
CleanupLog
"#;

    assert_eq!(eval(source), Value::String("D".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_empty_defer_blocks() {
    let error = check_source("defer {}").expect_err("source should fail");

    assert!(error.to_string().contains("cannot be empty"));
}

#[test]
fn rejects_return_inside_defer() {
    let error = check_source(
        r#"
Bad():int = {
    defer:
        return 1
    0
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("return"));
    assert!(error.to_string().contains("defer"));
}

#[test]
fn rejects_break_inside_defer() {
    let error = check_source(
        r#"
loop:
    defer:
        break
    break
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("break"));
    assert!(error.to_string().contains("defer"));
}

#[test]
fn rejects_defer_expression_syntax() {
    let error = parse_source("defer 1").expect_err("source should fail");

    assert!(error.to_string().contains("expected `:` or `{`"));
}

#[test]
fn rejects_direct_suspends_call_inside_defer() {
    let error = check_source(
        r#"
Wait()<suspends>:void = {}
Run()<suspends>:void =
    defer:
        Wait()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`defer` block cannot contain suspend expressions")
    );
}

#[test]
fn evaluates_for_ranges() {
    let source = r#"
var total: int = 0
for (i := 1..5) {
    set total = total + i
}
total
"#;

    assert_eq!(eval(source), Value::Number(15.0));
}

#[test]
fn evaluates_for_range_iterable_clause() {
    let source = r#"
var Total:int = 0
for (I : 1..3) {
    set Total += I
}
Total
"#;

    assert_eq!(eval(source), Value::Number(6.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_standalone_range_expression() {
    let error = check_source(
        r#"
Range := 1..3
Range
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("range expressions are only valid in `for` expressions")
    );
}

#[test]
fn rejects_range_expression_as_function_argument() {
    let error = check_source(
        r#"
Use(Value:int):int = Value
Use(1..3)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("range expressions are only valid in `for` expressions")
    );
}

#[test]
fn evaluates_for_colon_blocks() {
    let source = r#"
Doubled:[]int = for (I := 1..5):
    I * 2
if (Value := Doubled[4]). Doubled.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(15.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_expressions_as_arrays() {
    let source = r#"
Doubled:[]int = for (I := 1..5) {
    I * 2
}
if (Value := Doubled[4]). Doubled.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(15.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_dot_blocks() {
    let source = r#"
Doubled:[]int = for (I := 1..5). I * 2
if (Value := Doubled[4]). Doubled.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(15.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_intermediate_bindings() {
    let source = r#"
Doubled:[]int = for (X := 1..5, Y := X * 2):
    Y
if (Value := Doubled[4]). Doubled.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(15.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_boolean_filter_clauses() {
    let source = r#"
Values:[]int = for (X := 1..5, X <> 3):
    X
if (Value := Values[2]). Values.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(8.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_do_blocks() {
    let source = r#"
Values:[]int = for:
    X := 1..5
    X <> 3
    Y := X * 2
do:
    Y
if (Value := Values[2]). Values.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_do_blocks_with_multiple_generators() {
    let source = r#"
Pairs:[]int = for:
    X := 1..2
    Y := 1..3
do:
    X * 10 + Y
if (Value := Pairs[5]). Pairs.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(29.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_for_do_block_without_do() {
    let error = parse_source(
        r#"
for:
    X := 1..3
    X
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `do:`"));
}

#[test]
fn evaluates_for_arrays() {
    let source = r#"
var total: int = 0
for (item : array{2, 4, 8}) {
    set total = total + item
}
total
"#;

    assert_eq!(eval(source), Value::Number(14.0));
}

#[test]
fn evaluates_for_strings() {
    let source = r#"
Letters := for (Letter : "abc") {
    Letter
}
if:
    Letters = array{'a', 'b', 'c'}
    First := Letters[0]
then:
    First
else:
    'z'
"#;

    assert_eq!(eval(source), Value::Char('a'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char
    );
}

#[test]
fn rejects_break_outside_loops() {
    let error = check_source("break").expect_err("source should fail");
    assert!(error.to_string().contains("outside a loop"));
}

#[test]
fn rejects_break_inside_for() {
    let error = check_source(
        r#"
for (i := 1..3) {
    break
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("outside a loop"));
}

#[test]
fn rejects_loop_with_only_break() {
    let error = check_source(
        r#"
loop {
    break
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("non-break statement"));
}

#[test]
fn rejects_colon_block_without_newline() {
    let error = parse_source("if (true): 1").expect_err("source should fail");

    assert!(error.to_string().contains("expected newline after `:`"));
}

#[test]
fn rejects_unindented_colon_block() {
    let error = parse_source(
        r#"
if (true):
1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected indented block"));
}

#[test]
fn rejects_legacy_for_in_syntax() {
    let error = parse_source(
        r#"
for item in array{1, 2} {
    item
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `(`"));
}
