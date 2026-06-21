//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn parses_official_path_qualified_type_names_from_spatialmath_docs() {
    parse_source(
        r#"
using { /UnrealEngine.com/Temporary/SpatialMath }
using { /Verse.org/SpatialMath }

my_class := class:
    MyUnrealEngineVector:(/UnrealEngine.com/Temporary/SpatialMath:)vector3 = (/UnrealEngine.com/Temporary/SpatialMath:)vector3{}
    MyVerseVector:(/Verse.org/SpatialMath:)vector3 = (/Verse.org/SpatialMath:)vector3{}
"#,
    )
    .expect("source should parse");
}

#[test]
fn rejects_arctan_wrong_arity() {
    let error = check_source("ArcTan(0, 0, 0)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 1..=2 arguments"));
}

#[test]
fn rejects_arctan_argument_type_mismatch() {
    let error = check_source(r#"ArcTan("bad")"#).expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected `float`"));
}

#[test]
fn evaluates_recursive_functions() {
    let source = r#"
factorial(n:int):int = if (n <= 1) {
    1
} else {
    n * factorial(n - 1)
}

factorial(5)
"#;

    assert_eq!(eval(source), Value::Int(120));
}

#[test]
fn evaluates_top_level_function_overloads_by_parameter_type() {
    let source = r#"
Score(Value:int):int = 40
Score(Value:string):int = 2

Score(1) + Score("bonus")
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_function_overloads_by_parameter_type() {
    let source = r#"
Process():int =
    Format(Value:int):int = 40
    Format(Value:string):int = 2

    Format(1) + Format("bonus")

Process()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloads_with_different_effects_and_parameter_types() {
    let source = r#"
Pick(Value:int)<decides><transacts>:int = Value
Pick(Value:string):int = 2

Found := if (Value := Pick[40]). Value else. 0
Found + Pick("bonus")
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_capture_of_overloaded_function_name() {
    let error = check_source(
        r#"
Score(Value:int):int = Value
Score(Value:string):int = 1

Captured := Score
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("overloaded function `Score` must be called")
    );
}

#[test]
fn rejects_duplicate_function_overload_signature() {
    let error = check_source(
        r#"
Score(Value:int):int = Value
Score(Other:int):int = Other + 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Score`"));
}

#[test]
fn rejects_function_overload_by_effects_only() {
    let error = check_source(
        r#"
Score(Value:int):int = Value
Score(Value:int)<decides><transacts>:int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Score`"));
}

#[test]
fn rejects_overload_function_parameter_signature_distinctness() {
    let error = check_source(
        r#"
Choose(Callback:type{_(:int):int}):int = 1
Choose(Callback:type{_(:string):int}):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_void_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Value:void):int = 1
Choose(Value:int):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_named_defaults_with_overlapping_empty_call() {
    let error = check_source(
        r#"
Choose(?X:int = 1):int = X
Choose(?Y:int = 2):int = Y
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn parses_function_definitions() {
    let program = parse_source("add(a:int, b:int):int = a + b").expect("source should parse");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn evaluates_localized_message_binding() {
    let source = r#"
Greeting<localizes>:message = "Hello, world!"
Greeting
"#;

    assert_eq!(eval(source), Value::String("Hello, world!".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Message
    );
}

#[test]
fn evaluates_public_localized_message_binding() {
    let source = r#"
Tip<public><localizes>:message := "Use the button."
str(Tip)
"#;

    assert_eq!(eval(source), Value::String("Use the button.".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_localized_message_function() {
    let source = r#"
ScoreText<localizes>(Score:int):message = "Score: {Score}"
ScoreText(42)
"#;

    assert_eq!(eval(source), Value::String("Score: 42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Message
    );
}

#[test]
fn rejects_localizes_function_without_message_return() {
    let error = check_source(r#"Bad<localizes>():int = 42"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`localizes` function specifier requires a `message` return type")
    );
}

#[test]
fn evaluates_typed_bindings_and_functions() {
    let source = r#"
x: number := 40
add(a: number, b: number): number = a + b
add(x, 2)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn rejects_float_return_from_int_function() {
    let error = check_source("Bad():int = 1.5").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function is annotated to return `int` but body has type `float`")
    );
}

#[test]
fn evaluates_nested_named_function_values() {
    let source = r#"
MakeAdder(X:int) = {
    Add(Y:int):int = X + Y
    Add
}

MakeAdder(40)(2)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_function_type_annotations() {
    let source = r#"
Double(X:int):int = X * 2
Fn:type{_(:int):int} = Double
Fn(21)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_function_parameters_with_type_annotations() {
    let source = r#"
Apply(F:type{_(:int):int}, Value:int):int = F(Value)
Double(X:int):int = X * 2
Apply(Double, 21)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_optional_function_type_annotations() {
    let source = r#"
Default():int = 40
Custom():int = 42
var Handler:?type{_():int} = false
set Handler = option{Custom}
if (Fn := Handler?). Fn() else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn checks_function_type_with_effect_specifiers() {
    let source = r#"
Pick(X:int)<decides><transacts>:int = X
Handler:type{_(:int)<decides><transacts>:int} = Pick
if (Value := Handler[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_type_effect_hierarchy_assignability() {
    let source = r#"
Compute()<computes>:int = 42
Handler:type{_()<transacts>:int} = Compute
Handler()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_type_effect_hierarchy_to_computes() {
    let source = r#"
Stable()<converges>:int = 42
Handler:type{_()<computes>:int} = Stable
Handler()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_definitions_with_effect_specifiers() {
    let source = r#"
Double(X:int)<computes>:int = X * 2
Double(21)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn checks_effect_propagation_allows_computes_chain() {
    let source = r#"
Base()<computes>:int = 40
Derived()<computes>:int = Base() + 2
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["computes".to_string()],
            Some(Vec::new()),
            Type::Int
        )
    );
}

#[test]
fn checks_transacts_function_calling_computes_function() {
    let source = r#"
Base()<computes>:int = 42
Use()<transacts>:int = Base()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["transacts".to_string()],
            Some(Vec::new()),
            Type::Int
        )
    );
}

#[test]
fn checks_reads_function_calling_computes_function() {
    let source = r#"
Base()<computes>:int = 42
Use()<reads>:int = Base()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["reads".to_string()],
            Some(Vec::new()),
            Type::Int
        )
    );
}

#[test]
fn effect_set_models_call_capability_lattice() {
    let transacts = EffectSet::from_names(["transacts"]);
    assert!(
        transacts
            .call_allowed_effects()
            .contains(&Effect::Transacts)
    );
    assert!(transacts.call_allowed_effects().contains(&Effect::Writes));
    assert!(transacts.call_allowed_effects().contains(&Effect::Computes));
    assert!(
        transacts
            .call_allowed_effects()
            .contains(&Effect::Converges)
    );

    let reads = EffectSet::from_names(["reads"]);
    assert!(reads.call_allowed_effects().contains(&Effect::Reads));
    assert!(reads.call_allowed_effects().contains(&Effect::Computes));
    assert!(!reads.call_allowed_effects().contains(&Effect::Writes));

    let no_rollback = EffectSet::from_names(std::iter::empty::<&str>());
    assert!(no_rollback.has_no_rollback());
    assert_eq!(no_rollback.render_declared(), "<no_rollback>");
}

#[test]
fn effect_set_models_function_type_assignability() {
    let expected = EffectSet::from_names(["computes"]);
    let actual = EffectSet::from_names(["converges"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["computes"]);
    let actual = EffectSet::from_names(["transacts"]);
    assert!(!expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["transacts", "decides"]);
    let actual = EffectSet::from_names(["transacts"]);
    assert!(!expected.assignable_from(&actual));
}

#[test]
fn evaluates_function_definitions_with_name_specifiers() {
    let source = r#"
Visible<public>(X:int):int = X + 1
Visible(41)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_definitions_with_name_and_effect_specifiers() {
    let source = r#"
OnBegin<override>()<suspends>:void = print("begin")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["override".to_string(), "suspends".to_string()],
            Some(Vec::new()),
            Type::None
        )
    );
}

#[test]
fn rejects_function_type_effect_hierarchy_widening() {
    let error = check_source(
        r#"
Update()<transacts>:int = 42
Handler:type{_()<computes>:int} = Update
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/0<computes> -> int`")
    );
}

#[test]
fn rejects_computes_function_calling_transacts_function() {
    let error = check_source(
        r#"
Update()<transacts>:int = 42
Pure()<computes>:int = Update()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_varies_function_calling_transacts_function() {
    let error = check_source(
        r#"
Update()<transacts>:int = 42
ReadVarying()<varies>:int = Update()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <varies> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_computes_function_calling_native_transacts_function() {
    let error = check_source(
        r#"
Use()<computes>:float = GetRandomFloat(0.0, 1.0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_reads_function_calling_writes_function() {
    let error = check_source(
        r#"
Write()<writes>:int = 42
Read()<reads>:int = Write()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <reads> effect cannot call function requiring <writes> effect"
        )
    );
}

#[test]
fn rejects_function_typed_value_effect_mismatch_in_body() {
    let error = check_source(
        r#"
Use(Handler:type{_()<transacts>:int})<computes>:int = Handler()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_suspends_function_assigned_to_non_suspends_type() {
    let error = check_source(
        r#"
Wait()<suspends>:int = 42
Handler:type{_():int} = Wait
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("function/0<suspends> -> int"));
}

#[test]
fn rejects_unknown_function_effect_specifier() {
    let error = parse_source("Double(X:int)<custom>:int = X").expect_err("source should fail");

    assert!(error.to_string().contains("unknown effect specifier"));
}

#[test]
fn rejects_unknown_function_name_specifier() {
    let error = parse_source("Double<custom>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("unknown function specifier"));
}

#[test]
fn rejects_duplicate_function_name_specifier() {
    let error =
        parse_source("Double<public><public>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("duplicate function specifier"));
}

#[test]
fn rejects_duplicate_function_effect_specifier() {
    let error = check_source(
        r#"
Double(X:int)<computes><computes>:int = X * 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate function effect `<computes>`")
    );
}

#[test]
fn rejects_conflicting_exclusive_function_effects() {
    let error = check_source(
        r#"
Double(X:int)<computes><varies>:int = X * 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function exclusive effects cannot be combined")
    );
}

#[test]
fn evaluates_official_memory_effect_specifiers() {
    let source = r#"
ReadValue()<reads>:int = 40
AllocateValue()<allocates>:int = 1
WriteValue()<writes>:int = 1
ReadValue() + AllocateValue() + WriteValue()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unknown_function_type_effect_specifier() {
    let error =
        parse_source("Handler:type{_(:int)<custom>:int} = Double").expect_err("source should fail");

    assert!(error.to_string().contains("unknown effect specifier"));
}

#[test]
fn evaluates_comparable_function_parameter() {
    let source = r#"
Accept(Key:comparable):int = 42
Accept((1, "a"))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_function_value_assigned_to_comparable() {
    let error = check_source(
        r#"
Double(X:int):int = X * 2
Bad:comparable = Double
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("binding `Bad`"));
    assert!(error.to_string().contains("comparable"));
}

#[test]
fn rejects_function_equality_comparison() {
    let error = check_source(
        r#"
Read():int = 42
if (Read = Read). 1 else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("equality operand type `function/0 -> int` is not comparable")
    );
}

#[test]
fn rejects_function_return_type_mismatch() {
    let error = check_source(r#"bad(): number := "not a number""#).expect_err("source should fail");
    assert!(error.to_string().contains("return `number`"));
}

#[test]
fn rejects_function_type_parameter_mismatch() {
    let error = check_source(
        r#"
Stringify(Value:int):string = str(Value)
Fn:type{_(:string):string} = Stringify
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/1 -> string`")
    );
}

#[test]
fn rejects_function_type_return_mismatch() {
    let error = check_source(
        r#"
Double(X:int):int = X * 2
Fn:type{_(:int):string} = Double
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/1 -> string`")
    );
}

#[test]
fn rejects_wrong_call_arity() {
    let error = check_source(
        r#"
add(a: number, b: number): number = a + b
add(1)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected 2 arguments"));
}

#[test]
fn evaluates_named_and_default_parameters() {
    let source = r#"
Label(Message:string, ?Level:int = 1, ?Color:string = "white"):string = {
    Message + ":" + str(Level) + ":" + Color
}

Label("Start") + "|" + Label("Warn", ?Level := 2) + "|" + Label("Err", ?Color := "red", ?Level := 3)
"#;

    assert_eq!(
        eval(source),
        Value::String("Start:1:white|Warn:2:white|Err:3:red".into())
    );
}

#[test]
fn evaluates_required_named_parameters() {
    let source = r#"
Scale(?Value:int, ?Factor:int = 2):int = Value * Factor
Scale(?Value := 5) + Scale(?Factor := 3, ?Value := 5)
"#;

    assert_eq!(eval(source), Value::Int(25));
}

#[test]
fn evaluates_official_ordinary_named_argument() {
    let source = r#"
BuyMousetrap(CoinsPerMousetrap:int):int = CoinsPerMousetrap + 32
BuyMousetrap(CoinsPerMousetrap := 10)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_ordinary_named_arguments_reordered() {
    let source = r#"
Difference(Left:int, Right:int):int = Left - Right
Difference(Right := 8, Left := 50)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_ordinary_named_arguments_choose_overload() {
    let source = r#"
Pick(Value:int, Bonus:int):int = Value + Bonus
Pick(Value:string, Bonus:int):int = Bonus + 100
Pick(Bonus := 2, Value := 40)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_missing_required_named_parameter() {
    let error = check_source(
        r#"
Scale(?Value:int, ?Factor:int = 2):int = Value * Factor
Scale()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("missing required argument `?Value`")
    );
}

#[test]
fn rejects_unknown_named_argument() {
    let error = check_source(
        r#"
Scale(?Value:int):int = Value
Scale(?Other := 1)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unknown named argument `?Other`")
    );
}

#[test]
fn rejects_unknown_ordinary_named_argument() {
    let error = check_source(
        r#"
Scale(Value:int):int = Value
Scale(Other := 1)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown named argument `Other`"));
}

#[test]
fn rejects_duplicate_ordinary_named_argument() {
    let error = check_source(
        r#"
Scale(Value:int):int = Value
Scale(1, Value := 2)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate argument for parameter `Value`")
    );
}

#[test]
fn rejects_named_argument_type_mismatch() {
    let error = check_source(
        r#"
Scale(?Value:int):int = Value
Scale(?Value := "bad")
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument `?Value` expected `int`")
    );
}

#[test]
fn rejects_duplicate_named_argument() {
    let error = check_source(
        r#"
Scale(?Value:int):int = Value
Scale(?Value := 1, ?Value := 2)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate argument"));
}

#[test]
fn rejects_positional_after_named_argument() {
    let error = parse_source(
        r#"
Scale(Value:int, ?Factor:int = 2):int = Value * Factor
Scale(?Factor := 3, 5)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("positional arguments cannot follow named arguments")
    );
}

#[test]
fn rejects_positional_parameter_after_named_parameter() {
    let error = parse_source("Bad(?Value:int, Other:int):int = Value + Other")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("positional parameters cannot follow named parameters")
    );
}

#[test]
fn evaluates_return_statements() {
    let source = r#"
ClampLocal(Value:int):int = {
    if (Value < 0) {
        return 0
    }
    Value
}

ClampLocal(-5) + ClampLocal(7)
"#;

    assert_eq!(eval(source), Value::Int(7));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_bare_return_from_void_function() {
    let source = r#"
Noop():void =
    return
Noop()
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn evaluates_return_from_all_branches() {
    let source = r#"
Sign(Value:int):int = if (Value < 0) {
    return -1
} else {
    return 1
}

Sign(-2) + Sign(4)
"#;

    assert_eq!(eval(source), Value::Int(0));
}

#[test]
fn warns_unreachable_statement_after_return() {
    assert_check_warning(
        r#"
Bad():int = {
    return 1
    2
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after `return`",
    );
}

#[test]
fn warns_unreachable_statement_after_bare_return() {
    assert_check_warning(
        r#"
Bad():void =
    return
    print("bad")
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after `return`",
    );
}

#[test]
fn warns_unreachable_statement_after_never_expression() {
    assert_check_warning(
        r#"
Bad():int =
    Err("fatal")
    42
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn warns_unreachable_statement_after_never_binding_initializer() {
    assert_check_warning(
        r#"
Bad():int = {
    Value:int = Err("fatal")
    42
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn warns_unreachable_statement_after_never_call_argument() {
    assert_check_warning(
        r#"
Halt()<computes> = Err("fatal")
Bad():int = {
    Print(Halt())
    42
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn warns_unreachable_statement_after_never_collection_item() {
    assert_check_warning(
        r#"
Bad():int = {
    Values:[]int = array{1, Err("fatal")}
    42
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn rejects_return_type_mismatch() {
    let error = check_source(
        r#"
Bad():int = {
    return "bad"
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot return `string`"));
}

#[test]
fn rejects_bare_return_from_non_void_function() {
    let error = check_source(
        r#"
Bad():int =
    return
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot return `none`"));
}

#[test]
fn rejects_return_outside_function() {
    let error = check_source("return 1").expect_err("source should fail");
    assert!(error.to_string().contains("outside a function"));
}

#[test]
fn rejects_bare_return_outside_function() {
    let error = check_source("return").expect_err("source should fail");
    assert!(error.to_string().contains("outside a function"));
}

#[test]
fn runtime_errors_on_return_outside_function() {
    let error = run_source("return 1").expect_err("source should fail");

    assert!(error.to_string().contains("outside a function"));
}

#[test]
fn rejects_unknown_names() {
    let error = check_source("missing + 1").expect_err("source should fail");
    assert!(error.to_string().contains("undefined name `missing`"));
}

#[test]
fn rejects_unknown_type_names() {
    let error = check_source("x: numbre := 1").expect_err("source should fail");
    assert!(error.to_string().contains("unknown type `numbre`"));
}

#[test]
fn evaluates_where_type_parameter_constructor_function() {
    let source = r#"
stack<public>(t:type) := class:
    Elements<public>:[]t = array{}
    Push<public>(NewElement:t):stack(t) =
        stack(t){ Elements := Elements + array{NewElement} }
    First<public>()<transacts><decides>:t =
        Elements[0]

CreateStack<constructor>(InitialElements:[]t where t:type) := stack(t):
    Elements := InitialElements

Scores := CreateStack(array{40, 2})
Updated := Scores.Push(100)
if (Value := Updated.First[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_user_parametric_type_wrong_arity() {
    let source = r#"
stack(t:type) := class:
    Elements:[]t = array{}

Value:stack(int, string) = stack(int){}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("parametric type `stack` expected 1 type arguments, got 2")
    );
}

#[test]
fn rejects_where_type_parameter_constructor_mismatch() {
    let source = r#"
stack(t:type) := class:
    Elements:[]t = array{}

CreateStack<constructor>(InitialElements:[]t where t:type) := stack(t):
    Elements := InitialElements

Value:stack(int) = CreateStack(array{"bad"})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("stack(int)"));
}

#[test]
fn evaluates_inline_generic_function_parameter_runtime() {
    let source = r#"
PickFirstIf(Items:[]t, Accept(L:t, R:t)<decides><transacts>:t where t:type)<decides><transacts>:t =
    Left := Items[0]
    Right := Items[1]
    Accept[Left, Right]
    Left

Smaller(Left:int, Right:int)<decides><transacts>:int =
    Left < Right
    Left

if (Value := PickFirstIf[array{2, 5}, Smaller]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_inline_function_parameter_with_anonymous_type() {
    let source = r#"
Render(Items:[]t, ToText(:[]t)<transacts>:string where t:type)<transacts>:string =
    ToText(Items)

IntArrayToString(Items:[]int)<transacts>:string =
    "ints"

Render(array{1, 2, 3}, IntArrayToString)
"#;

    assert_eq!(eval(source), Value::String("ints".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_inline_function_parameter_type_mismatch_after_inference() {
    let source = r#"
Apply(Value:t, Callback(Item:t)<transacts>:t where t:type)<transacts>:t =
    Callback(Value)

Wrong(Item:string)<transacts>:string =
    Item

Apply(1, Wrong)
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("argument 2"));
}

#[test]
fn evaluates_anonymous_generic_parameter_from_official_style() {
    let source = r#"
Const(X:t, :u where t:type, u:type):t =
    X

Const(42, "ignored")
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_anonymous_parameter_subtype_constraint() {
    let source = r#"
RequireComparable(:t where t:subtype(comparable)):int =
    1

RequireComparable("key")
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_anonymous_parameter_subtype_constraint_mismatch() {
    let source = r#"
RequireComparable(:t where t:subtype(comparable)):int =
    1

holder := class:
    Value:int = 0

RequireComparable(holder{})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `comparable`")
    );
}

#[test]
fn runtime_errors_on_subtype_comparable_constraint_argument_mismatch() {
    let source = r#"
RequireComparable(:t where t:subtype(comparable)):int =
    42

holder := class:
    Callback:type{_():int}

MakeNumber():int =
    1

RequireComparable(holder{Callback := MakeNumber})
"#;

    let error = run_source(source).expect_err("source should fail at runtime");

    assert!(
        error
            .to_string()
            .contains("must be a subtype of `comparable`")
    );
}

#[test]
fn checks_official_make_classifiable_subset_return_type() {
    let source = r#"
Make():classifiable_subset(int) = MakeClassifiableSubset(array{1, 2})
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            Vec::<String>::new(),
            Some(vec![]),
            Type::ClassifiableSubset(Box::new(Type::Int))
        )
    );
}

#[test]
fn rejects_classifiable_subset_union_in_computes_function() {
    let source = r#"
Combine(Left:classifiable_subset(int), Right:classifiable_subset(int))<computes>:classifiable_subset(int) =
    Left + Right
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn checks_official_result_constructor_inferred_types() {
    assert_eq!(
        check_source("MakeSuccess(42)").expect("source should check"),
        Type::Result(Box::new(Type::Int), Box::new(Type::Never))
    );
    assert_eq!(
        check_source(r#"MakeError("no")"#).expect("source should check"),
        Type::Result(Box::new(Type::Never), Box::new(Type::String))
    );
}

#[test]
fn rejects_official_result_get_success_parentheses_call() {
    let error = check_source(
        r#"
Outcome:result(int, string) = MakeSuccess(42)
Outcome.GetSuccess()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must be called with `[]`"));
}

#[test]
fn rejects_official_result_unknown_member() {
    let error = check_source(
        r#"
Outcome:result(int, string) = MakeSuccess(42)
Outcome.Done[]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `Done`"));
}

#[test]
fn rejects_official_result_constructor_named_argument() {
    let error = check_source("MakeSuccess(?Result := 42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("does not accept named arguments")
    );
}

#[test]
fn rejects_official_parametric_type_wrong_arity() {
    let error = check_source("Value:result(int) = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `result` expected 2 type arguments")
    );
}

#[test]
fn rejects_official_castable_subtype_parametric_type_wrong_arity() {
    let error =
        check_source("Value:castable_subtype() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `castable_subtype` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_classifiable_subset_parametric_type_wrong_arity() {
    let error =
        check_source("Value:classifiable_subset() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `classifiable_subset` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_make_classifiable_subset_named_argument() {
    let error = check_source("MakeClassifiableSubset(?InElements := array{1})")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`MakeClassifiableSubset` does not accept named arguments")
    );
}

#[test]
fn evaluates_constructor_delegation_to_parent_constructor() {
    let source = r#"
entity := class:
    Name:string
    Health:int

MakeEntity<constructor>(Name:string, Health:int) := entity:
    Name := Name
    Health := Health

character := class(entity):
    Class:string
    Level:int

MakeCharacter<constructor>(Name:string, Class:string, Level:int) := character:
    Class := Class
    Level := Level
    MakeEntity<constructor>(Name, Level * 100)

Hero := MakeCharacter("Ava", "Warrior", 4)
Hero.Name + ":" + str(Hero.Health) + ":" + Hero.Class + ":" + str(Hero.Level)
"#;

    assert_eq!(eval(source), Value::String("Ava:400:Warrior:4".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_field_initializer_after_parent_constructor_delegation() {
    let error = check_source(
        r#"
entity := class:
    Name:string

MakeEntity<constructor>(Name:string) := entity:
    Name := Name

character := class(entity):
    Level:int

character:
    MakeEntity<constructor>("Ava")
    Level := 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field initializer `Level` cannot appear after constructor delegation")
    );
}

#[test]
fn runtime_errors_on_field_initializer_after_constructor_delegation() {
    let source = r#"
entity := class:
    Name:string

MakeEntity<constructor>(Name:string) := entity:
    Name := Name

character := class(entity):
    Level:int

character:
    MakeEntity<constructor>("Ava")
    Level := 42
"#;

    let error = run_source(source).expect_err("source should fail at runtime");

    assert!(
        error
            .to_string()
            .contains("field initializer `Level` cannot appear after constructor delegation")
    );
}

#[test]
fn rejects_constructor_delegation_to_non_constructor_function() {
    let error = check_source(
        r#"
entity := class:
    Name:string

MakeEntity(Name:string):entity =
    entity:
        Name := Name

player := class(entity):
    Score:int

player:
    Score := 42
    MakeEntity<constructor>("Ava")
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expects a constructor function"));
}

#[test]
fn evaluates_overloaded_constructor_functions() {
    let source = r#"
position := struct:
    X:int
    Y:int
    Z:int

entity := class:
    Name:string
    Health:int
    Position:position

MakeEntity<constructor>(Name:string, Health:int, Position:position) := entity:
    Name := Name
    Health := Health
    Position := Position

MakeEntity<constructor>(Name:string, Position:position) := entity:
    Name := Name
    Health := 100
    Position := Position

MakeEntity<constructor>(Name:string) := entity:
    Name := Name
    Health := 100
    Position := position{X := 0, Y := 0, Z := 7}

Full := MakeEntity("Ava", 42, position{X := 1, Y := 2, Z := 3})
DefaultHealth := MakeEntity("Bea", position{X := 4, Y := 5, Z := 6})
Origin := MakeEntity("Cyd")

Full.Health + DefaultHealth.Health + Origin.Position.Z
"#;

    assert_eq!(eval(source), Value::Int(149));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_suspends_constructor_function() {
    let error = check_source(
        r#"
player := class:
    Name:string

MakePlayer<constructor>()<suspends>:player =
    player:
        Name := "Ava"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("constructor functions cannot use `<suspends>`")
    );
}

#[test]
fn checks_constructor_function_return_type_mismatch() {
    let error = check_source(
        r#"
player := class:
    Name : string

MakePlayer<constructor>():player =
    "bad"
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("return `player`"));
}

#[test]
fn evaluates_subclass_argument_runtime_type_check() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

ReadID(Item:entity):int = Item.ID

ReadID(boss{ID := 40, Threat := 2})
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unknown_member_name() {
    let error = check_source(
        r#"
Values:[]int = array{}
Values.Count
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown member `Count`"));
}

#[test]
fn checks_user_suspends_function_call_inside_async_context() {
    let source = r#"
Wait()<suspends>:void = {}
Start()<suspends>:void = Wait()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(Vec::new()),
            Type::None
        )
    );
}

#[test]
fn rejects_user_suspends_function_call_outside_async_context() {
    let error = check_source(
        r#"
Wait()<suspends>:void = {}
Start():void = Wait()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`<suspends>` effect"));
    assert!(error.to_string().contains("async context"));
}

#[test]
fn evaluates_function_signature_type_alias_annotations() {
    let source = r#"
player_map := [string]int
ScoreFor(Scores:player_map, Name:string)<decides><transacts>:int = Scores[Name]
if (Score := ScoreFor[map{"ada" => 42}, "ada"]). Score else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
