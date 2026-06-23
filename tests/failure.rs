//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn rejects_failed_int_outside_failure_context() {
    assert_failable_context_error("Int[NaN]");
}

#[test]
fn evaluates_official_profile_expression_in_failure_context() {
    let source = r#"
if (profile("Lookup"):
    array{10}[0]
). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decision_expressions_in_failure_context() {
    let source = r#"
Both := if (5 > 0 and 30 >= 20). 20 else. 0
Either := if (0 > 0 or 2 = 2). 20 else. 0
Negated := if (not (0 > 0)). 2 else. 0
Both + Either + Negated
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn decision_expressions_short_circuit_in_failure_context() {
    let source = r#"
Values:[]int = array{}
LeftFails := if (0 > 1 and Values[0] = 1). 0 else. 40
LeftSucceeds := if (1 = 1 or Values[0] = 1). 2 else. 0
LeftFails + LeftSucceeds
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_failed_or_left_operand_before_right_operand() {
    let source = r#"
var Hits:int = 0

Observed := if ({
    set Hits += 1
    false?
    Hits
} or {
    set Hits += 10
    true?
    Hits
}). Hits else. 0

Observed + Hits
"#;

    assert_eq!(eval(source), Value::Int(20));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_failed_not_operand_when_not_succeeds() {
    let source = r#"
var Hits:int = 0

Observed := if (not {
    set Hits += 1
    false?
    Hits
}). Hits else. 99

Observed + Hits
"#;

    assert_eq!(eval(source), Value::Int(0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_return_in_failure_context() {
    let error = check_source(
        r#"
Choose():int =
    if:
        return 1
        true?
    then:
        2
    else:
        3

Choose()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("Explicit return out of a failure context is not allowed")
    );
}

#[test]
fn rejects_break_in_failure_context() {
    let error = check_source(
        r#"
var Hits:int = 0
loop:
    if:
        break
        true?
    then:
        set Hits = 1
    else:
        set Hits = 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`break` may not be used in a failure context")
    );
}

#[test]
fn evaluates_official_comparison_success_value_in_failure_context() {
    let source = r#"
Greater := if (Value := 5 > 0). Value else. 0
Equal := if (Value := 7 = 7). Value else. 0
NotEqual := if (Value := 11 <> 12). Value else. 0
LessFails := if (Value := 3 < 0). Value else. 19
Greater + Equal + NotEqual + LessFails
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_decision_expression_success_values() {
    let source = r#"
AndValue := if (Value := 1 = 1 and 40 = 40). Value else. 0
OrValue := if (Value := 0 > 1 or 2 = 2). Value else. 0
AndValue + OrValue
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_if_condition_without_failable_expression() {
    let error = check_source(
        r#"
if (true):
    1
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`if` condition must contain at least one failable expression")
    );
}

#[test]
fn rejects_if_block_without_failable_expression() {
    let error = check_source(
        r#"
if:
    Value := 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`if` condition must contain at least one failable expression")
    );
}

#[test]
fn evaluates_decides_function_bracket_calls() {
    let source = r#"
Pick(Value:int)<decides><transacts>:int = Value
if (Value := Pick[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_function_failure_in_failure_context() {
    let source = r#"
Values:[]int = array{42}

Pick(Index:int)<decides><transacts>:int = Values[Index]

Found := if (Value := Pick[0]). Value else. 0
Missing := if (Value := Pick[1]). Value else. 0
Captured:?int = option{Pick[1]}
Found + Missing + if (Value := Captured?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_function_query_failure_in_failure_context() {
    let source = r#"
Check(Ready:logic)<decides><transacts>:logic = Ready?

First := if (Check[true]). 40 else. 0
Second := if (Check[false]). 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_decides_failure_outside_failure_context() {
    let source = r#"
Values:[]int = array{42}
Pick(Index:int)<decides><transacts>:int = Values[Index]
Pick[1]
"#;
    assert_failable_context_error(source);
}

#[test]
fn rejects_decides_function_parenthesis_calls() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><transacts>:int = Value
Pick(42)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must be called with `[]`"));
}

#[test]
fn rejects_non_decides_function_bracket_calls() {
    let error = check_source(
        r#"
Double(Value:int):int = Value * 2
Double[21]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must be called with `()`"));
}

#[test]
fn rejects_decides_function_assigned_to_non_decides_type() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><transacts>:int = Value
Handler:type{_(:int):int} = Pick
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function/1<decides><transacts> -> int")
    );
}

#[test]
fn rejects_non_decides_function_assigned_to_decides_type() {
    let error = check_source(
        r#"
Double(Value:int):int = Value * 2
Handler:type{_(:int)<decides><transacts>:int} = Double
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/1<decides><transacts> -> int`")
    );
}

#[test]
fn rejects_computes_function_calling_no_rollback_function() {
    let error = check_source(
        r#"
Read():int = 42
Pure()<computes>:int = Read()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <no_rollback> effect"
    ));
}

#[test]
fn rejects_no_rollback_function_assigned_to_transacts_type() {
    let error = check_source(
        r#"
Read():int = 42
Handler:type{_()<transacts>:int} = Read
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/0<transacts> -> int`")
    );
}

#[test]
fn checks_decides_function_without_call_effect() {
    let source = r#"
Pick(Value:int)<decides>:int = Value
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_computes_function_call_in_failure_context() {
    let source = r#"
Pick(Value:int)<decides><computes>:int = Value
if (Result := Pick[42]). Result else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_reads_function_call_in_failure_context() {
    let source = r#"
Pick(Value:int)<decides><reads>:int = Value
if (Result := Pick[42]). Result else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_converges_function_call_in_failure_context() {
    let source = r#"
Pick(Value:int)<decides><converges>:int = Value
if (Result := Pick[42]). Result else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_decides_no_rollback_function_call_in_failure_context() {
    let error = check_source(
        r#"
Pick(Value:int)<decides>:int = Value
if (Result := Pick[42]). Result else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_decides_writes_function_call_in_failure_context() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><writes>:int = Value
if (Result := Pick[42]). Result else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_decides_allocates_function_call_in_failure_context() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><allocates>:int = Value
if (Result := Pick[42]). Result else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn evaluates_computes_function_call_in_failure_context() {
    let source = r#"
Read()<computes>:int = 40
if:
    Value := Read()
    Value > 0
then:
    Value + 2
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_reads_function_call_in_failure_context() {
    let source = r#"
Read()<reads>:int = 40
if:
    Value := Read()
    Value = 40
then:
    Value + 2
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_computes_function_typed_value_call_in_failure_context() {
    let source = r#"
Read()<computes>:int = 40
Handler:type{_()<computes>:int} = Read
if:
    Value := Handler()
    Value = 40
then:
    Value + 2
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_transacts_function_call_in_failure_context() {
    let source = r#"
var Total:int = 0
Next()<transacts>:int =
    set Total += 1
    Total

if:
    Value := Next()
    Value = 1
then:
    Total + 41
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_varies_function_call_in_failure_context() {
    let source = r#"
var Total:int = 0
Next()<varies>:int =
    set Total += 1
    Total

if:
    Value := Next()
    Value = 1
then:
    Total + 41
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_no_rollback_function_typed_value_call_in_failure_context() {
    let error = check_source(
        r#"
Read():int = 42
Handler:type{_():int} = Read
if:
    Value := Handler()
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_writes_function_call_in_failure_context() {
    let error = check_source(
        r#"
Write()<writes>:int = 42
if:
    Value := Write()
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_writes_function_typed_value_call_in_failure_context() {
    let error = check_source(
        r#"
Write()<writes>:int = 42
Handler:type{_()<writes>:int} = Write
if:
    Value := Handler()
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_allocates_function_call_in_failure_context() {
    let error = check_source(
        r#"
Make()<allocates>:int = 42
if:
    Value := Make()
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_allocates_function_typed_value_call_in_failure_context() {
    let error = check_source(
        r#"
Make()<allocates>:int = 42
Handler:type{_()<allocates>:int} = Make
if:
    Value := Handler()
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_decides_allocates_function_typed_value_call_in_failure_context() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><allocates>:int = Value
Handler:type{_(:int)<decides><allocates>:int} = Pick
if:
    Value := Handler[42]
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_no_rollback_function_call_in_failure_context() {
    let error = check_source(
        r#"
Read():int = 42
if:
    Value := Read()
    Value > 0
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_no_rollback_overload_call_in_failure_context() {
    let error = check_source(
        r#"
if:
    Label := ToString(42)
    Label = "42"
then:
    1
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_writes_overload_call_in_failure_context() {
    let error = check_source(
        r#"
Pick(Value:int)<writes>:int = Value
Pick(Value:string)<computes>:int = 0
if:
    Value := Pick(42)
    Value = 42
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_no_rollback_function_call_in_if_condition() {
    let error = check_source(
        r#"
IsReady():logic = true
if (IsReady()?):
    42
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn evaluates_computes_function_call_in_failure_comparison() {
    let source = r#"
Current()<computes>:int = 42
if (Current() = 42):
    42
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_no_rollback_function_call_in_failure_comparison() {
    let error = check_source(
        r#"
Current():int = 42
if (Current() = 42):
    42
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_no_rollback_argument_call_in_failure_context() {
    let error = check_source(
        r#"
Accept(Value:logic)<computes>:logic = Value
IsReady():logic = true
if (Accept(IsReady())?):
    42
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_no_rollback_native_call_in_failure_context() {
    let error = check_source(
        r#"
if:
    Values := ConcatenateMaps(map{1 => 1}, map{2 => 2})
    Values[1] = 1
then:
    42
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_special_transacts_call_in_computes_failure_context() {
    let error = check_source(
        r#"
Use()<computes>:?[]int = option{Shuffle(array{1})}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn evaluates_computes_argument_call_in_failure_context() {
    let source = r#"
Accept(Value:logic)<computes>:logic = Value
IsReady()<computes>:logic = true
if (Accept(IsReady())?):
    42
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_manual_no_rollback_effect_specifier() {
    let error = parse_source("Double(X:int)<no_rollback>:int = X").expect_err("source should fail");

    assert!(error.to_string().contains("cannot be manually specified"));
}

#[test]
fn rejects_manual_no_rollback_function_name_specifier() {
    let error = parse_source("Double<no_rollback>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("cannot be manually specified"));
}

#[test]
fn evaluates_if_then_failure_context_blocks() {
    let source = r#"
Values:[]int = array{40, 2}
if:
    Value := Values[1]
    Value > 1
then:
    Value + 40
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_then_failure_context_block_failure() {
    let source = r#"
Values:[]int = array{40, 2}
if:
    Value := Values[9]
then:
    Value
else:
    42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_set_in_if_then_failure_context_blocks() {
    let source = r#"
var Ready:logic = false
if:
    set Ready = true
    Ready?
then:
    42
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn fails_set_statement_rhs_in_if_then_failure_context_block() {
    let source = r#"
var Total:int = 7
Values:[]int = array{}
Result := if:
    set Total = Values[0]
then:
    Total
else:
    Total + 35
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_set_expression_when_if_condition_sequence_fails() {
    let source = r#"
var Total:int = 0
Values:[]int = array{}
Result := if (set Total = 99, set Total = Values[0]):
    Total
else:
    Total + 42
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_variable_assignment_when_if_failure_context_fails() {
    let source = r#"
var BreakTime:logic = false
if:
    1 > 0
    set BreakTime = true
    0 > 1
then:
    0
else:
    if (BreakTime?):
        1
    else:
        42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn commits_variable_assignment_when_if_failure_context_succeeds() {
    let source = r#"
var Total:int = 0
if:
    set Total = 42
    Total > 0
then:
    0
else:
    1
Total
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_defer_when_if_failure_context_succeeds() {
    let source = r#"
var CleanupLog:string = ""
if:
    defer:
        set CleanupLog = CleanupLog + "D"
    true?
then:
    CleanupLog + "T"
else:
    "bad"
"#;

    assert_eq!(eval(source), Value::String("DT".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn runs_defer_when_if_failure_context_fails() {
    let source = r#"
if:
    defer:
        Err("defer ran")
    false?
then:
    0
else:
    42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );

    let error = run_source(source).expect_err("failed failure context should run defer");

    assert!(error.to_string().contains("defer ran"));
}

#[test]
fn rolls_back_defer_mutations_when_if_failure_context_fails() {
    let source = r#"
var CleanupLog:string = ""
if:
    defer:
        set CleanupLog = CleanupLog + "D"
    false?
then:
    0
else:
    if (CleanupLog = ""):
        42
    else:
        0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_decides_function_body_when_it_fails() {
    let source = r#"
var Total:int = 0
Try()<decides><transacts>:int =
    set Total = 99
    false?
    1

if (Try[]):
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
fn runs_defer_when_decides_function_fails() {
    let source = r#"
Try()<decides><transacts>:int =
    defer:
        Err("decides defer ran")
    false?
    1

if (Try[]):
    0
else:
    42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );

    let error = run_source(source).expect_err("failed decides function should run defer");

    assert!(error.to_string().contains("decides defer ran"));
}

#[test]
fn rolls_back_defer_mutations_when_decides_function_fails() {
    let source = r#"
var CleanupLog:string = ""
Try()<decides><transacts>:int =
    defer:
        set CleanupLog = CleanupLog + "D"
    false?
    1

if (Try[]):
    0
else:
    if (CleanupLog = ""):
        42
    else:
        0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_optional_member_query_outside_failure_context() {
    let source = r#"
player := class:
    Name : string = "Ava"

Empty:?player = false
Empty?.Name
"#;
    assert_failable_context_error(source);
}

#[test]
fn evaluates_if_failure_condition_sequence() {
    let source = r#"
Scores:[string]int = map{"ada" => 42}
if (Score := Scores["ada"], Score > 40):
    Score
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_if_failure_binding_after_if_expression() {
    let error = check_source(
        r#"
Values:[]int = array{42}
if (Value := Values[0]):
    Value
else:
    0
Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
}

#[test]
fn warns_unreachable_failure_clause_after_never_expression() {
    assert_check_warning(
        r#"
Halt()<computes> = Err("fatal")
if:
    Halt()
    1 = 1
then:
    1
else:
    0
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn evaluates_classifiable_subset_contains_runtime_failure_context() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{})
if (Set.Contains[TagType]). 0 else. 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_classifiable_subset_contains_outside_failure_context() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{})
Set.Contains[TagType]
"#;
    assert_failable_context_error(source);
}

#[test]
fn evaluates_var_declaration_expression_in_failure_context() {
    let source = r#"
Values := array{40}
Result := if (var Picked:int = Values[0], Picked = 40):
    set Picked += 2
    Picked
else:
    0
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failable_index_inside_defer() {
    let error = check_source(
        r#"
Values := array{1}
defer:
    Values[0]
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
fn rejects_failable_if_condition_inside_defer() {
    let error = check_source(
        r#"
defer:
    if (1 = 1):
        42
    else:
        0
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
fn evaluates_computes_call_in_for_filter_failure_context() {
    let source = r#"
Keep()<computes>:logic = true
Values:[]int = for (X := 1..3, Keep()?):
    X
if (Value := Values[2]). Values.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(6));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_no_rollback_call_in_for_filter_failure_context() {
    let error = check_source(
        r#"
Keep():logic = true
Values:[]int = for (X := 1..3, Keep()?):
    X
Values.Length
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn evaluates_computes_call_in_for_binding_failure_context() {
    let source = r#"
Current()<computes>:int = 2
Values:[]int = for (X := 1..2, Y := Current()):
    X + Y
if (Value := Values[1]). Values.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(6));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_no_rollback_call_in_for_binding_failure_context() {
    let error = check_source(
        r#"
Current():int = 2
Values:[]int = for (X := 1..2, Y := Current()):
    X + Y
Values.Length
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn evaluates_for_failable_filter_clauses() {
    let source = r#"
Items:[]int = array{40, 2}
Indexes:[]int = for (Index := 0..3, Items[Index]):
    Index

if:
    First := Indexes[0]
    Second := Indexes[1]
    Item := Items[Second]
then:
    Indexes.Length + First + Second + Item
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failed_for_body_outside_failure_context() {
    let source = r#"
Values:[]int = array{10, 20}
Picked:[]int = for (I := 0..3):
    Values[I]
"#;
    assert_failable_context_error(source);
}

#[test]
fn propagates_for_body_failure_in_decides_function() {
    let source = r#"
AllMatch(Values:[]logic, Expected:[]logic)<decides><transacts>:void =
    for:
        Index -> Value : Values
        ExpectedValue := Expected[Index]
    do:
        Value = ExpectedValue

Good:int = if (AllMatch[array{true, false}, array{true, false}]). 1 else. 10
Bad:int = if (AllMatch[array{true, false}, array{true, true}]). 100 else. 2
Good + Bad
"#;

    assert_eq!(eval(source), Value::Int(3));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_for_body_failure_in_enclosing_failure_context() {
    let source = r#"
var Total:int = 0
Values:[]int = array{1}

Result:int = if:
    for (I := 0..1):
        set Total += 1
        Values[I]
then:
    99
else:
    Total

Result * 10 + Total
"#;

    assert_eq!(eval(source), Value::Int(0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_for_filter_without_failable_expression() {
    let error = check_source(
        r#"
for (X := 1..3, "bad"):
    X
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("for filter must contain at least one failable expression")
    );
}

#[test]
fn evaluates_logic_query_in_failure_context() {
    let source = r#"
First := if (true?). 40 else. 0
Second := if (false?). 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
