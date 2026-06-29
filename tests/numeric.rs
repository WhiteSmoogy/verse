//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_arithmetic_with_precedence() {
    assert_eq!(eval("1 + 2 * 3"), Value::Int(7));
}

#[test]
fn preserves_i64_max_integer_precision() {
    let source = "9223372036854775807 + 0";

    assert_eq!(eval(source), Value::Int(i64::MAX));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_numeric_functions() {
    let source = r#"
ModValue := if (Value := Mod[-1, 3]). Value else. 0
QuotientA := if (Value := Quotient[-1, 3]). Value else. 0
QuotientB := if (Value := Quotient[10, -3]). Value else. 0
str(ModValue) + ":" + str(QuotientA) + ":" + str(QuotientB) + ":" + str(Clamp(12, 10, 0)) + ":" + str(Lerp(10, 20, 0.25))
"#;

    assert_eq!(eval(source), Value::String("2:-1:-3:10:12.5".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_official_numeric_helper_runtime_value_types() {
    fn expect_int(source: &str, expected: i64) {
        match eval(source) {
            Value::Int(actual) => assert_eq!(actual, expected),
            other => panic!("expected int {expected}, got {other:?}"),
        }
    }

    fn expect_float(source: &str, expected: f64) {
        match eval(source) {
            Value::Float(actual) => assert_eq!(actual, expected),
            other => panic!("expected float {expected}, got {other:?}"),
        }
    }

    expect_int(
        "Result := if (Value := Mod[5, 3]). Value else. 0\nResult",
        2,
    );
    expect_int(
        "Result := if (Value := Quotient[10, -3]). Value else. 0\nResult",
        -3,
    );
    expect_int("Abs(-5)", 5);
    expect_float("Abs(-5.0)", 5.0);
    expect_int("Min(10, 3)", 3);
    expect_float("Min(10.0, 3.0)", 3.0);
    expect_int("Max(7, 9)", 9);
    expect_float("Max(7.0, 9.0)", 9.0);
    expect_int("Clamp(12, 10, 0)", 10);
    expect_float("Clamp(12.0, 10.0, 0.0)", 10.0);
    expect_float("Lerp(10.0, 20.0, 0.25)", 12.5);
    expect_int(
        "Result := if (Value := 1 / 2). Ceil(Value) else. 0\nResult",
        1,
    );
    expect_int(
        "Result := if (Value := 7 / 3). Floor(Value) else. 0\nResult",
        2,
    );
    expect_int(
        "Result := if (Value := Round[2.5]). Value else. 0\nResult",
        2,
    );
    expect_int(
        "Result := if (Value := Int[-3.7]). Value else. 0\nResult",
        -3,
    );
}

#[test]
fn evaluates_official_numeric_helpers_with_ordinary_named_arguments() {
    let source = r#"
Clamped:int = Clamp(Value := -1, A := 0, B := 10)
Interpolated:int = if (Value := Round[Lerp(To := 44.0, From := 40.0, Parameter := 0.5)]). Value else. 0
Clamped + Interpolated
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_bitwise_integer_helpers() {
    let source = r#"
BitAnd(12, 10) + BitOr(12, 10) + BitXor(12, 10) + BitNot(-15)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_bitwise_helpers_with_non_int_arguments() {
    let error = check_source("BitAnd(1.0, 1)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `int`, got `float`")
    );
}

#[test]
fn evaluates_official_float_min_max_nan_semantics() {
    let source = r#"
MinNaN := if ((Min(NaN, 1.0)).IsFinite[]). 0 else. 20
MaxNaN := if ((Max(1.0, NaN)).IsFinite[]). 0 else. 22
MinNaN + MaxNaN
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_clamp_nan_ordering() {
    let source = r#"
ValueNaN:float = Clamp(NaN, 1.0, 2.0)
BoundNaN:float = Clamp(2.0, NaN, 1.0)
ValueNaNHigh:int = if (Value := Round[ValueNaN]). Value else. 0
BoundNaNHigh:int = if (Value := Round[BoundNaN]). Value else. 0
TwoNaNClamp:float = Clamp(1.0, NaN, NaN)
TwoNaNs := if (TwoNaNClamp.IsFinite[]). 0 else. 38
ValueNaNHigh + BoundNaNHigh + TwoNaNs
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_lerp_non_finite_arguments() {
    let error = run_source("Lerp(0.0, Inf, 0.5)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Lerp` expected finite arguments")
    );
}

#[test]
fn rejects_ceil_floor_non_finite_arguments_outside_failure_context() {
    let ceil_error = run_source("Ceil[NaN]").expect_err("source should fail");
    let floor_error = run_source("Floor[Inf]").expect_err("source should fail");

    assert!(
        ceil_error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
    assert!(
        floor_error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
}

#[test]
fn rejects_round_and_int_rational_arguments() {
    let round_error =
        check_source("if (Value := 1 / 2). Round[Value] else. 0").expect_err("source should fail");
    let int_error =
        check_source("if (Value := 1 / 2). Int[Value] else. 0").expect_err("source should fail");

    assert!(
        round_error
            .to_string()
            .contains("argument 1 expected `float`, got `rational`")
    );
    assert!(
        int_error
            .to_string()
            .contains("argument 1 expected `float`, got `rational`")
    );
}

#[test]
fn rejects_round_and_int_rational_expressions_outside_failure_context() {
    let round_error = run_source("Round[1 / 2]").expect_err("source should fail");
    let int_error = run_source("Int[1 / 2]").expect_err("source should fail");

    assert!(
        round_error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
    assert!(
        int_error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
}

#[test]
fn evaluates_mod_and_quotient_failure_contexts() {
    let source = r#"
ModFailure := if (Value := Mod[10, 0]). Value else. 40
QuotientFailure := if (Value := Quotient[10, 0]). Value else. 2
Captured:?int = option{Mod[10, 0]}
CapturedValue := if (Value := Captured?). Value else. 0
ModFailure + QuotientFailure + CapturedValue
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_integer_numeric_helpers_and_constants() {
    let source = r#"
CeilValue := if (Value := Ceil[1.2]). Value else. 0
FloorValue := if (Value := Floor[1.8]). Value else. 0
Abs(-5) + Min(10, 3) + Max(7, 9) + CeilValue + FloorValue + if (PiFloat > 3.0 and PiFloat < 4.0). 22 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_ceil_floor_float_failure_contexts() {
    let source = r#"
CeilSuccess:int = if (Value := Ceil[1.2]). Value else. 0
FloorSuccess:int = if (Value := Floor[1.8]). Value else. 0
CeilFailure := if (Value := Ceil[NaN]). Value else. 20
FloorFailure := if (Value := Floor[Inf]). Value else. 19
Captured:?int = option{Ceil[NaN]}
CapturedValue := if (Value := Captured?). Value else. 0
CeilSuccess + FloorSuccess + CeilFailure + FloorFailure + CapturedValue
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_round_and_int_failure_contexts() {
    let source = r#"
Rounded:int = (if (Value := Round[2.5]). Value else. 0) + (if (Value := Round[3.5]). Value else. 0)
Truncated:int = if (Value := Int[-3.7]). Value else. 0
IntFailure := if (Value := Int[NaN]). Value else. 39
RoundFailure := if (Value := Round[Inf]). Value else. 2
Rounded + Truncated + IntFailure + RoundFailure
"#;

    assert_eq!(eval(source), Value::Int(44));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_float_math_helpers() {
    let source = r#"
RoundOrZero(Value:float):int = if (Rounded := Round[Value]). Rounded else. 0
RoundOrZero(Sqrt(81)) + RoundOrZero(Sin(0)) + RoundOrZero(Cos(0)) + RoundOrZero(Tan(0)) + RoundOrZero(ArcSin(0)) + RoundOrZero(ArcCos(1)) + RoundOrZero(ArcTan(0)) + RoundOrZero(ArcTan(0, 0)) + RoundOrZero(Sinh(0)) + RoundOrZero(Cosh(0)) + RoundOrZero(Tanh(0)) + RoundOrZero(ArSinh(0)) + RoundOrZero(ArCosh(1)) + RoundOrZero(ArTanh(0)) + RoundOrZero(Exp(0)) + RoundOrZero(Ln(1)) + RoundOrZero(Log(2, 8)) + RoundOrZero(Pow(2, 5)) + Sgn(-3) + Sgn(0) + Sgn(4)
"#;

    assert_eq!(eval(source), Value::Int(47));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sgn_overloads() {
    let source = r#"
IntSign:int = Sgn(-3)
FloatSign:float = Sgn(-3.0)
NaNSign:float = Sgn(NaN)
NaNFlag := if (NaNSign.IsFinite[]). 0 else. 44
IntSign + (if (Value := Round[FloatSign]). Value else. 0) + NaNFlag
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_sgn_float_result_assigned_to_int() {
    let error = check_source("Sign:int = Sgn(-3.0)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Sign` is annotated as `int` but expression has type `float`")
    );
}

#[test]
fn rejects_sgn_rational_argument() {
    let error =
        check_source("if (Value := 1 / 2). Sgn(Value) else. 0").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (rational)")
    );
}

#[test]
fn evaluates_is_finite_number_extension_failure_contexts() {
    let source = r#"
Finite := if (Value := (12.5).IsFinite[]). Value else. 0
Infinite := if (Value := Inf.IsFinite[]). Value else. 29.5
NotNumber := if (Value := NaN.IsFinite[]). Value else. 0
TrigNaN := if (Value := Sin(Inf).IsFinite[]). Value else. 0
Finite + Infinite + NotNumber + TrigNaN
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_is_almost_zero_failure_contexts() {
    let source = r#"
Close := if ((-0.01).IsAlmostZero[0.02]). 40 else. 0
Far := if ((0.2).IsAlmostZero[0.02]). 0 else. 2
Close + Far
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_is_almost_equal_failure_contexts() {
    let source = r#"
Close := if (IsAlmostEqual[1.0, 1.01, 0.02]). 40 else. 0
Far := if (IsAlmostEqual[1.0, 1.2, 0.02]). 0 else. 2
Close + Far
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failed_is_almost_equal_outside_failure_context() {
    assert_failable_context_error("IsAlmostEqual[1.0, 1.2, 0.02]");
}

#[test]
fn rejects_failed_is_almost_zero_outside_failure_context() {
    assert_failable_context_error("(0.2).IsAlmostZero[0.02]");
}

#[test]
fn rejects_failed_is_finite_outside_failure_context() {
    assert_failable_context_error("Inf.IsFinite[]");
}

#[test]
fn rejects_is_almost_equal_with_parentheses() {
    let error = check_source("IsAlmostEqual(1.0, 1.0, 0.0)").expect_err("source should fail");

    assert!(error.to_string().contains("functions with `<decides>`"));
}

#[test]
fn rejects_is_almost_zero_with_parentheses() {
    let error = check_source("(0.0).IsAlmostZero(0.1)").expect_err("source should fail");

    assert!(error.to_string().contains("unknown member `IsAlmostZero`"));
}

#[test]
fn rejects_is_almost_zero_non_float_tolerance() {
    let error = check_source(r#"(0.0).IsAlmostZero["near"]"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`IsAlmostZero` AbsoluteTolerance expected `float`")
    );
}

#[test]
fn rejects_is_finite_with_parentheses() {
    let error = check_source("Inf.IsFinite()").expect_err("source should fail");

    assert!(error.to_string().contains("unknown member `IsFinite`"));
}

#[test]
fn rejects_decides_numeric_conversion_with_parentheses() {
    let error = check_source("Round(2.5)").expect_err("source should fail");

    assert!(error.to_string().contains("functions with `<decides>`"));
}

#[test]
fn rejects_float_ceil_floor_with_parentheses() {
    let ceil_error = check_source("Ceil(1.2)").expect_err("source should fail");
    let floor_error = check_source("Floor(1.8)").expect_err("source should fail");

    assert!(
        ceil_error
            .to_string()
            .contains("no overload matches () call")
    );
    assert!(
        floor_error
            .to_string()
            .contains("no overload matches () call")
    );
}

#[test]
fn rejects_non_decides_numeric_helper_with_brackets() {
    let error = check_source("Sqrt[4.0]").expect_err("source should fail");

    assert!(error.to_string().contains("functions without `<decides>`"));
}

#[test]
fn rejects_rational_ceil_floor_with_brackets() {
    let ceil_error =
        check_source("if (Value := 1 / 2). Ceil[Value] else. 0").expect_err("source should fail");
    let floor_error =
        check_source("if (Value := 7 / 3). Floor[Value] else. 0").expect_err("source should fail");

    assert!(
        ceil_error
            .to_string()
            .contains("no overload matches [] call")
    );
    assert!(
        floor_error
            .to_string()
            .contains("no overload matches [] call")
    );
}

#[test]
fn rejects_failed_mod_outside_failure_context() {
    assert_failable_context_error("Mod[10, 0]");
}

#[test]
fn rejects_decides_numeric_functions_with_parentheses() {
    let error = check_source("Mod(10, 3)").expect_err("source should fail");

    assert!(error.to_string().contains("functions with `<decides>`"));
}

#[test]
fn rejects_numeric_function_argument_type_mismatch() {
    let error = check_source(r#"Clamp("bad", 0, 1)"#).expect_err("source should fail");

    assert!(error.to_string().contains("no overload matches"));
}

#[test]
fn evaluates_unary_positive_operator() {
    assert_eq!(eval("+42"), Value::Int(42));
    assert_eq!(check_source("+42").expect("source should check"), Type::Int);
}

#[test]
fn rejects_unary_positive_on_non_number() {
    let error = check_source(r#"+"bad""#).expect_err("source should fail");

    assert!(error.to_string().contains("unary `+` expected `number`"));
}

#[test]
fn evaluates_rational_type_annotations() {
    let source = r#"
Half:rational = if (Value := 1 / 2). Value else. 0
Ceil(Half) + Floor(Half)
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_integer_division_as_rational() {
    let source = r#"
Value:rational = if (Result := 7 / 3). Result else. 0
Floor(Value) * 10 + Ceil(Value)
"#;

    assert_eq!(eval(source), Value::Int(23));
    assert_eq!(
        check_source("if (Value := 7 / 3). Value else. 0").expect("source should check"),
        Type::Rational
    );
}

#[test]
fn evaluates_int_subtype_of_rational() {
    let source = r#"
Whole:rational = 7
Ceil(Whole)
"#;

    assert_eq!(eval(source), Value::Int(7));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_exact_rational_arithmetic() {
    let source = r#"
if:
    First := 1 / 3
    Second := 1 / 3
    Third := 1 / 3
    First + Second + Third = 1
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
fn evaluates_rational_type_alias_annotation() {
    let source = r#"
fraction := rational
Value:fraction = if (Result := 1 / 2). Result else. 0
Ceil(Value)
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_float_literal_assigned_to_rational() {
    let error = check_source("Value:rational = 1.0").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Value` is annotated as `rational` but expression has type `float`")
    );
}

#[test]
fn rejects_rational_assigned_to_int() {
    let error = check_source("Value:int = if (Result := 1 / 1). Result else. 0")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Value` is annotated as `int` but expression has type `rational`")
    );
}

#[test]
fn rejects_array_index_with_rational() {
    let error = check_source(
        r#"
Values := array{10, 20}
if:
    Index := 1 / 1
    Value := Values[Index]
then:
    Value
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("array index expected `int`"));
}

#[test]
fn evaluates_numeric_compound_assignments() {
    let source = r#"
var Total:rational = 100
set Total -= 25
set Total *= 2
set Total /= 3
Total
"#;

    assert_eq!(eval(source), Value::Int(50));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Rational
    );
}

#[test]
fn rejects_int_divide_assignment_with_rational_result() {
    let error = check_source(
        r#"
var Total:int = 100
set Total /= 3
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign compound result `rational` to target of type `int`")
    );
}

#[test]
fn rejects_non_numeric_subtract_assignment() {
    let error = check_source(
        r#"
var Name:string = "Ava"
set Name -= "v"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("assignment target expected `number`")
    );
}

#[test]
fn rejects_int_divide_assignment_by_zero_as_rational_result() {
    let source = r#"
var Value:int = 10
set Value /= 0
"#;
    let error = run_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign compound result `rational` to target of type `int`")
    );
}

#[test]
fn rejects_array_method_rational_index() {
    let error = check_source(
        r#"
Values:[]int = array{1, 2}
if:
    Index := 1 / 1
    Result := Values.Insert[Index, array{3}]
then:
    Result.Length
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`Insert` index expected `int`"));
}

#[test]
fn evaluates_rational_map_key_annotation() {
    let source = r#"
Half:rational = if (Value := 1 / 2). Value else. 0
Equivalent:rational = if (Value := 2 / 4). Value else. 0
Scores:[rational]int = map{Half => 42}
if (Value := Scores[Equivalent]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_rational_tuple_index() {
    let error = check_source(
        r#"
Pair := (1, 2)
Index:rational = if (Value := 1 / 1). Value else. 0
Pair(Index)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("tuple index expected `int`"));
}

#[test]
fn captures_comparison_and_division_failure_in_option_literal() {
    let source = r#"
Comparison:?int = option{1 < 0}
ComparisonHit:?int = option{4 < 5}
Division:?rational = option{84 / 2}
Zero:?rational = option{84 / 0}
First := if (Value := Comparison?). Value else. 1
Hit := if (Value := ComparisonHit?). Value else. 0
Second := if (Value := Division?). Floor(Value) else. 0
Third := if (Value := Zero?). Floor(Value) else. -5
First + Hit + Second + Third
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
