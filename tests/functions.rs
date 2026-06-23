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
fn rejects_local_binding_access_specifiers() {
    for access in ["public", "internal", "protected", "private"] {
        let source = format!(
            r#"
Make():int =
    Local<{access}>:int = 42
    Local

Make()
"#
        );
        let error = check_source(&source).expect_err("source should fail");

        assert!(
            error.to_string().contains(&format!(
                "local definition `Local` cannot use access specifier `<{access}>`"
            )),
            "{error}"
        );
    }
}

#[test]
fn rejects_local_function_access_specifier() {
    let error = check_source(
        r#"
Make():int =
    Helper<private>():int = 42
    Helper()

Make()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("local definition `Helper` cannot use access specifier `<private>`"),
        "{error}"
    );
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
fn evaluates_external_function_typed_value_runtime_surface() {
    let source = r#"
Handler:type{_(:int):int} = external {}
Handler(21)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_function_primitive_return_runtime_surface() {
    let source = r#"
Make():int = external {}
Make() + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_function_dependent_type_value_return_runtime_surface() {
    let source = r#"
Pick(Kind:type):Kind = external {}
Pick(int) + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_function_dependent_type_value_aggregate_return_runtime_surface() {
    let source = r#"
box(t:type) := class:
    Value:t

Pick(Kind:type):Kind = external {}
Pick(box(int)).Value + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn evaluates_decides_computes_function_type_assignment() {
    let source = r#"
Pick(Value:int)<decides><computes>:int = Value
Handler:type{_(:int)<decides><computes>:int} = Pick
if (Value := Handler[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_function_type_effect_hierarchy_to_transacts() {
    let source = r#"
Pick(Value:int)<decides><computes>:int = Value
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
fn evaluates_decides_function_type_effect_hierarchy_to_computes() {
    let source = r#"
Pick(Value:int)<decides><converges>:int = Value
Handler:type{_(:int)<decides><computes>:int} = Pick
if (Value := Handler[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_reads_function_type_assignment() {
    let source = r#"
Pick(Value:int)<decides><reads>:int = Value
Handler:type{_(:int)<decides><reads>:int} = Pick
if (Value := Handler[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_decides_function_type_effect_hierarchy_widening() {
    let error = check_source(
        r#"
Pick(Value:int)<decides><transacts>:int = Value
Handler:type{_(:int)<decides><computes>:int} = Pick
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `function/1<decides><computes> -> int`")
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

    let reads_writes = EffectSet::from_names(["reads", "writes"]);
    assert!(reads_writes.call_allowed_effects().contains(&Effect::Reads));
    assert!(
        reads_writes
            .call_allowed_effects()
            .contains(&Effect::Writes)
    );
    assert!(
        reads_writes
            .call_required_effects()
            .contains(&Effect::Reads)
    );
    assert!(
        reads_writes
            .call_required_effects()
            .contains(&Effect::Writes)
    );

    let writes_allocates = EffectSet::from_names(["writes", "allocates"]);
    assert!(
        writes_allocates
            .call_allowed_effects()
            .contains(&Effect::Writes)
    );
    assert!(
        writes_allocates
            .call_allowed_effects()
            .contains(&Effect::Allocates)
    );
    assert!(
        writes_allocates
            .call_required_effects()
            .contains(&Effect::Writes)
    );
    assert!(
        writes_allocates
            .call_required_effects()
            .contains(&Effect::Allocates)
    );

    let varies = EffectSet::from_names(["varies"]);
    assert!(varies.call_allowed_effects().contains(&Effect::Transacts));
    assert!(varies.call_allowed_effects().contains(&Effect::Reads));
    assert!(varies.call_allowed_effects().contains(&Effect::Writes));
    assert!(varies.call_allowed_effects().contains(&Effect::Allocates));

    let no_rollback = EffectSet::from_names(std::iter::empty::<&str>());
    assert!(no_rollback.has_no_rollback());
    assert_eq!(no_rollback.render_declared(), "<no_rollback>");

    let predicts_transacts = EffectSet::from_names(["predicts", "transacts"]);
    assert_eq!(
        predicts_transacts.call_required_effects(),
        vec![Effect::Transacts]
    );
    assert_eq!(
        predicts_transacts.render_declared(),
        "<predicts><transacts>"
    );
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

    let expected = EffectSet::from_names(["transacts", "decides"]);
    let actual = EffectSet::from_names(["computes", "decides"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["computes", "decides"]);
    let actual = EffectSet::from_names(["converges", "decides"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["computes", "decides"]);
    let actual = EffectSet::from_names(["transacts", "decides"]);
    assert!(!expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["transacts"]);
    let actual = EffectSet::from_names(["reads", "writes"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["writes"]);
    let actual = EffectSet::from_names(["reads", "writes"]);
    assert!(!expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["transacts"]);
    let actual = EffectSet::from_names(["writes", "allocates"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["writes"]);
    let actual = EffectSet::from_names(["writes", "allocates"]);
    assert!(!expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["allocates"]);
    let actual = EffectSet::from_names(["writes", "allocates"]);
    assert!(!expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["transacts"]);
    let actual = EffectSet::from_names(["varies"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["varies"]);
    let actual = EffectSet::from_names(["transacts"]);
    assert!(expected.assignable_from(&actual));

    let expected = EffectSet::from_names(["predicts", "transacts"]);
    let actual = EffectSet::from_names(["transacts"]);
    assert!(expected.assignable_from(&actual));
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
fn checks_official_predicts_effect_specifier() {
    let source = r#"
Predict<native><public>(Message:string)<predicts><transacts>:void = external {}
Use()<transacts>:void = Predict("ready")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["transacts".to_string()],
            Some(Vec::new()),
            Type::None
        )
    );
}

#[test]
fn checks_native_function_with_native_struct_signature() {
    let source = r#"
point<native> := struct:
    X<native>:int = 0

UsePoint<native><public>(Value:point):point = external {}
UsePoint
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["native".to_string(), "public".to_string()],
            Some(vec![Type::Struct("point".to_string())]),
            Type::Struct("point".to_string())
        )
    );
}

#[test]
fn rejects_native_function_with_non_native_struct_parameter() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

UsePoint<native><public>(Value:point):void = external {}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "`struct point` used as a parameter/result in a native function must also be native"
        ),
        "{error}"
    );
}

#[test]
fn rejects_native_function_with_non_native_struct_return() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

MakePoint<native><public>():point = external {}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "`struct point` used as a parameter/result in a native function must also be native"
        ),
        "{error}"
    );
}

#[test]
fn checks_predicts_function_type_effect_specifier() {
    let source = r#"
Predict(Message:string)<predicts><transacts>:void = external {}
Handler:type{_(:string)<predicts><transacts>:void} = Predict
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["predicts".to_string(), "transacts".to_string()],
            Some(vec![Type::String]),
            Type::None
        )
    );
}

#[test]
fn checks_predicts_function_type_accepts_non_predicts_function() {
    let source = r#"
Predict(Message:string)<transacts>:void = external {}
Handler:type{_(:string)<predicts><transacts>:void} = Predict
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["predicts".to_string(), "transacts".to_string()],
            Some(vec![Type::String]),
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

    assert!(
        error.to_string().contains(
            "function with <computes> effect cannot call function requiring <transacts> effect"
        ),
        "{error}"
    );
}

#[test]
fn evaluates_varies_function_calling_transacts_function() {
    let source = r#"
Update()<transacts>:int = 42
ReadVarying()<varies>:int = Update()
ReadVarying()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_varies_function_calling_no_rollback_function() {
    let source = r#"
Read():int = 42
ReadVarying()<varies>:int = Read()
ReadVarying()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_computes_function_calling_native_transacts_function() {
    let error = check_source(
        r#"
Use()<computes>:float = GetRandomFloat(0.0, 1.0)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <computes> effect cannot call function requiring <transacts> effect"
        ),
        "{error}"
    );
}

#[test]
fn rejects_computes_function_calling_predicts_transacts_function() {
    let error = check_source(
        r#"
Predict(Message:string)<predicts><transacts>:void = external {}
Use()<computes>:void = Predict("ready")
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <computes> effect cannot call function requiring <transacts> effect"
        ),
        "{error}"
    );
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
fn checks_combined_memory_effect_call_capabilities() {
    let source = r#"
Read()<reads>:int = 40
ReadThenWrite()<reads><writes>:int =
    var Total:int = Read()
    set Total += 2
    Total
Use()<transacts>:int = ReadThenWrite()
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
fn rejects_writes_function_calling_reads_writes_function() {
    let error = check_source(
        r#"
ReadWrite()<reads><writes>:int = 42
Use()<writes>:int = ReadWrite()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <writes> effect cannot call function requiring <reads> effect"
        )
    );
}

#[test]
fn checks_combined_write_allocate_effect_call_capabilities() {
    let source = r#"
token := class<unique>:
    ID:int = 0

MakeAndWrite()<writes><allocates>:int =
    var Total:int = 0
    Token := token{ID := 40}
    set Total = Token.ID + 2
    Total

Use()<transacts>:int = MakeAndWrite()
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
fn rejects_writes_function_calling_writes_allocates_function() {
    let error = check_source(
        r#"
MakeAndWrite()<writes><allocates>:int = 42
Use()<writes>:int = MakeAndWrite()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <writes> effect cannot call function requiring <allocates> effect"
        ),
        "{error}"
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
        parse_source("Double<final><final>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("duplicate function specifier"));
}

#[test]
fn rejects_duplicate_function_access_specifier() {
    let error =
        parse_source("Double<public><public>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("Duplicate access levels"));
}

#[test]
fn rejects_conflicting_function_access_specifiers() {
    let error =
        check_source("Double<public><internal>(X:int):int = X").expect_err("source should fail");

    assert!(error.to_string().contains("Conflicting access levels"));
}

#[test]
fn rejects_conflicting_extension_method_access_specifiers() {
    let error = check_source("(Value:int).Bump<public><internal>():int = Value")
        .expect_err("source should fail");

    assert!(error.to_string().contains("Conflicting access levels"));
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
fn rejects_duplicate_predicts_function_effect_specifier() {
    let error = check_source(
        r#"
Predict(Message:string)<predicts><predicts>:void = external {}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate function effect `<predicts>`")
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
fn evaluates_extension_method_overloads_by_receiver_runtime() {
    let source = r#"
(Value:string).Score():int = 42
(Value:int).Score():int = 0

"ready".Score()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_class_argument() {
    let source = r#"
box(t:type) := class:
    Value:t

Read(Box:box(t) where t:type):t =
    Box.Value

Read(box(int){Value := 42})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_class_base_argument() {
    let source = r#"
base_box(t:type) := class:
    Value:t

child_box(t:type) := class(base_box(t)):
    Extra:int = 2

Read(Box:base_box(t) where t:type):t =
    Box.Value

Read(child_box(int){Value := 42})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_interface_argument() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

Use(Item:reader(t) where t:type):t =
    Item.Read()

Use(box(int){Value := 42})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_parameter_inference_from_subtype_parametric_class_type_value() {
    let source = r#"
box(t:type) := class:
    Value:t

Pick(Kind:subtype(box(t)) where t:type):t = external {}

Pick(box(int))
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_parameter_inference_from_castable_parametric_class_type_value() {
    let source = r#"
castable_box(t:type) := class<castable>:
    Value:t

Pick(Kind:castable_subtype(castable_box(t)) where t:type):t = external {}

Pick(castable_box(int))
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_parameter_inference_from_subtype_parametric_interface_type_value() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

Pick(Kind:subtype(reader(t)) where t:type):t = external {}

Pick(box(int))
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_subtype_parametric_class_type_value_inferred_return_mismatch() {
    let source = r#"
box(t:type) := class:
    Value:t

Pick(Kind:subtype(box(t)) where t:type):t = external {}

Value:int = Pick(box(string))
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `int`"));
}

#[test]
fn rejects_subtype_parametric_interface_type_value_inferred_return_mismatch() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

Pick(Kind:subtype(reader(t)) where t:type):t = external {}

Value:int = Pick(box(string))
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `int`"));
}

#[test]
fn evaluates_type_parameter_inference_from_dependent_parametric_subtype_constraint() {
    let source = r#"
box(t:type) := class:
    Value:t

Read(Box:t where t:subtype(box(k)), k:type):k =
    Box.Value

Read(box(int){Value := 42})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_dependent_parametric_subtype_constraint_inferred_return_mismatch() {
    let source = r#"
box(t:type) := class:
    Value:t

Read(Box:t where t:subtype(box(k)), k:type):k =
    Box.Value

Value:string = Read(box(int){Value := 42})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `string`"));
}

#[test]
fn evaluates_chained_dependent_parametric_subtype_constraint() {
    let source = r#"
cell(t:type) := class:
    Value:t

box(t:type) := class:
    Value:t

Read(Box:t where t:subtype(box(k)), k:subtype(cell(u)), u:type):tuple(k, u) = external {}

Read(box(cell(int)){Value := cell(int){Value := 42}})
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Tuple(vec![Type::Class("cell(int)".to_string()), Type::Int])
    );
}

#[test]
fn rejects_chained_dependent_parametric_subtype_constraint_inferred_return_mismatch() {
    let source = r#"
cell(t:type) := class:
    Value:t

box(t:type) := class:
    Value:t

Read(Box:t where t:subtype(box(k)), k:subtype(cell(u)), u:type):tuple(k, u) = external {}

Value:tuple(cell(int), string) = Read(box(cell(int)){Value := cell(int){Value := 42}})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("annotated as `tuple(cell(int), string)`")
    );
}

#[test]
fn checks_dependent_castable_type_value_constraint_inferred_return() {
    let source = r#"
puzzle_light := class<castable>(tag){}

Pick(Kind:t where t:castable_subtype(k), k:type):k = external {}

Pick(puzzle_light)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("puzzle_light".to_string())
    );
}

#[test]
fn checks_dependent_nested_concrete_type_value_constraint_inferred_return() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

Pick(Kind:t where t:concrete_subtype(castable_subtype(k)), k:type):k = external {}

Pick(puzzle_light)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("puzzle_light".to_string())
    );
}

#[test]
fn evaluates_dependent_subtype_value_parameter_annotation() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

Pick(Kind:subtype(base_item), Item:Kind):Kind =
    Item

Picked := Pick(child_item, child_item{Value := 40, Score := 2})
Picked.Value + Picked.Score
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dependent_subtype_type_value_cast() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

Cast(Kind:subtype(base_item), Item:base_item)<decides><transacts>:Kind =
    Kind[Item]

Base:base_item = child_item{Value := 40, Score := 2}
if (Picked := Cast[child_item, Base]). Picked.Value + Picked.Score else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dependent_subtype_type_value_cast_failure() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

other_item := class(base_item):
    Rank:int = 0

Cast(Kind:subtype(base_item), Item:base_item)<decides><transacts>:Kind =
    Kind[Item]

Base:base_item = other_item{Value := 40, Rank := 2}
if (Picked := Cast[child_item, Base]). Picked.Score else. 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_dependent_subtype_type_value_cast_unrelated_argument() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

other_item := class:
    Rank:int = 0

Cast(Kind:subtype(base_item), Item:other_item)<decides><transacts>:Kind =
    Kind[Item]
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("cannot cast class `other_item` to unrelated class `base_item`")
    );
}

#[test]
fn evaluates_dependent_castable_subtype_type_value_cast() {
    let source = r#"
base_item := class<castable>:
    Value:int = 0

child_item := class<castable>(base_item):
    Score:int = 0

Cast(Kind:castable_subtype(base_item), Item:base_item)<decides><transacts>:Kind =
    Kind[Item]

Base:base_item = child_item{Value := 40, Score := 2}
if (Picked := Cast[child_item, Base]). Picked.Value + Picked.Score else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dependent_type_value_parameter_annotation() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

Pick(Kind:type, Item:Kind):Kind =
    Item

Picked := Pick(child_item, child_item{Value := 40, Score := 2})
Picked.Value + Picked.Score
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_dependent_type_value_parameter_argument_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

other_item := class(base_item):
    Extra:int = 0

Pick(Kind:type, Item:Kind):Kind =
    Item

Pick(child_item, other_item{Value := 40, Extra := 2})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("argument 2 expected `child_item`, got `other_item`")
    );
}

#[test]
fn evaluates_dependent_type_value_parameter_subtype_annotation() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

Accept(Base:type, Kind:subtype(Base)):int =
    42

Accept(base_item, child_item)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_dependent_type_value_parameter_castable_return() {
    let source = r#"
base_item := class<castable>:
    Value:int = 0

child_item := class<castable>(base_item):
    Score:int = 0

Pick(Base:type, Item:Base):castable_subtype(Base) = external {}

Pick(base_item, child_item{Value := 40, Score := 2})
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::CastableSubtype(Box::new(Type::Class("base_item".to_string())))
    );
}

#[test]
fn rejects_dependent_type_value_parameter_subtype_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 0

other_base := class:
    Extra:int = 0

other_child := class(other_base):
    Score:int = 0

Accept(Base:type, Kind:subtype(Base)):int =
    42

Accept(base_item, other_child)
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("argument 2 expected"));
    assert!(error.to_string().contains("subtype(base_item)"));
}

#[test]
fn rejects_dependent_subtype_value_parameter_argument_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 0

child_item := class(base_item):
    Score:int = 0

other_item := class(base_item):
    Extra:int = 0

Pick(Kind:subtype(base_item), Item:Kind):Kind =
    Item

Pick(child_item, other_item{Value := 40, Extra := 2})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("argument 2 expected `child_item`, got `other_item`")
    );
}

#[test]
fn rejects_mismatched_parametric_class_argument_inference() {
    let source = r#"
box(t:type) := class:
    Value:t

Choose(Left:box(t), Right:box(t) where t:type):t =
    Left.Value

Choose(box(int){Value := 42}, box(string){Value := "bad"})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("argument 2 expected `box(int)`, got `box(string)`")
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
fn evaluates_shared_where_type_parameter_constraint() {
    let source = r#"
Combine(Left:t, Right:u where t&u:type):tuple(t, u) =
    (Left, Right)

Combine(40, "ready")
"#;

    let value = eval(source);
    let Value::Array(items) = value else {
        panic!("expected array-backed tuple result");
    };
    assert_eq!(
        *items.borrow(),
        vec![Value::Int(40), Value::String("ready".to_string())]
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Tuple(vec![Type::Int, Type::String])
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
fn evaluates_castable_and_concrete_type_parameter_constraints() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

UseCastable(TagType:t where t:castable_subtype(tag)):int =
    20

UseConcrete(TagType:u where u:concrete_subtype(castable_subtype(tag))):int =
    22

UseCastable(puzzle_light) + UseConcrete(puzzle_light)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_castable_any_type_parameter_constraint() {
    let source = r#"
base_item := class<castable>{}
child_item := class<castable>(base_item){}

Use(TagType:t where t:castable_subtype(any)):int =
    42

Use(child_item)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_castable_any_extension_receiver_constraint() {
    let source = r#"
base_item := class<castable>:
    Value:int = 0

child_item := class<castable>(base_item):
    Score:int = 0

(Item:t where t:castable_subtype(any)).AsQueried(Kind:castable_subtype(any))<decides><transacts>:any =
    Kind[Item]

Item := child_item{Value := 40, Score := 2}
if (Item.AsQueried[child_item]). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_castable_any_extension_receiver_constraint_mismatch() {
    let source = r#"
plain_item := class:
    Value:int = 0

(Item:t where t:castable_subtype(any)).AsQueried(Kind:castable_subtype(any))<decides><transacts>:any =
    Kind[Item]

Item := plain_item{Value := 40}
Item.AsQueried[plain_item]
"#;

    let error = check_source(source).expect_err("source should fail");
    let error = error.to_string();
    assert!(
        error.contains("argument 1 expected `castable_subtype(any)`, got `class<plain_item>`"),
        "{error}"
    );
}

#[test]
fn evaluates_castable_concrete_type_parameter_constraint() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

Use(TagType:t where t:castable_concrete_subtype(tag)):int =
    42

Use(puzzle_light)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dependent_castable_type_parameter_constraint() {
    let source = r#"
base_item := class<castable>{}
child_item := class<castable>(base_item){}

Use(TagType:t, Instance:k where t:castable_subtype(k), k:type):int =
    42

Use(child_item, base_item{})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_castable_type_parameter_constraint_mismatch() {
    let source = r#"
base_item := class{}
plain_child := class(base_item){}

Use(TagType:t where t:castable_subtype(base_item)):int =
    42

Use(plain_child)
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `castable_subtype(base_item)`")
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
fn rejects_result_constructor_no_rollback_call_from_computes_function() {
    let success_error = check_source(
        r#"
Use()<computes>:result(int, any) = MakeSuccess(42)
"#,
    )
    .expect_err("source should fail");
    assert!(success_error.to_string().contains(
        "function with <computes> effect cannot call function requiring <no_rollback> effect"
    ));

    let error_error = check_source(
        r#"
Use()<computes>:result(any, string) = MakeError("no")
"#,
    )
    .expect_err("source should fail");
    assert!(error_error.to_string().contains(
        "function with <computes> effect cannot call function requiring <no_rollback> effect"
    ));
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
fn rejects_official_castable_concrete_subtype_parametric_type_wrong_arity() {
    let error = check_source("Value:castable_concrete_subtype() = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `castable_concrete_subtype` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_subtype_parametric_type_wrong_arity() {
    let error = check_source("Value:subtype() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `subtype` expected 1 type arguments")
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
fn rejects_official_classifiable_subset_key_parametric_type_wrong_arity() {
    let error = check_source("Value:classifiable_subset_key() = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `classifiable_subset_key` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_classifiable_subset_var_parametric_type_wrong_arity() {
    let error = check_source("Value:classifiable_subset_var() = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `classifiable_subset_var` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_success_result_parametric_type_wrong_arity() {
    let error =
        check_source("Value:success_result() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `success_result` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_error_result_parametric_type_wrong_arity() {
    let error = check_source("Value:error_result() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `error_result` expected 1 type arguments")
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
fn rejects_constructor_delegation_effect_mismatch() {
    let error = check_source(
        r#"
entity := class:
    Name:string

character := class(entity):
    Level:int = 42

MakeEntity<constructor>(Name:string)<transacts>:entity =
    entity{Name := Name}

MakeCharacter<constructor>()<computes>:character =
    character:
        MakeEntity<constructor>("Ava")
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "function with <computes> effect cannot call function requiring <transacts> effect"
        ),
        "{error}"
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
