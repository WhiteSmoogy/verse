//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn checks_distinct_int_and_float_annotations() {
    let source = r#"
Whole:int = 40
Fraction:float = 1.5
Widened:float = Whole
Whole + if (Value := Int[Fraction]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(41.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_typed_programs() {
    let source = r#"
factorial(n:int):int = if (n <= 1) {
    1
} else {
    n * factorial(n - 1)
}

factorial(5)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_comparable_type_annotations() {
    let source = r#"
Key:comparable = option{7}
Key
"#;

    assert_eq!(
        eval(source),
        Value::Option(Some(Box::new(Value::Number(7.0))))
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Comparable
    );
}

#[test]
fn evaluates_comparable_type_alias_annotation() {
    let source = r#"
key := comparable
Key:key = 42
Key
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Comparable
    );
}

#[test]
fn rejects_binding_type_mismatch() {
    let error = check_source(r#"x: number := "not a number""#).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `number`"));
}

#[test]
fn evaluates_official_parametric_type_annotations() {
    let source = r#"
Done:event() = external {}
PayloadEvent:event(int) = external {}
Outcome:result(int, string) = external {}
Signal:signalable(int) = external {}
Waitable:awaitable(string) = external {}
AnyWaitable:awaitable() = external {}
Listener:listenable(agent) = external {}
Subscription:subscribable() = external {}
Background:task(int) = external {}
Produced:generator(int) = external {}
UntypedProduced:generator() = external {}
TagType:castable_subtype(tag) = external {}
ComponentType:castable_subtype(component) = external {}
EntityPrefab:concrete_subtype(castable_subtype(entity)) = external {}
TagSet:classifiable_subset(tag) = external {}
ScoreModifier:modifier(int) = external {}
ScoreStack:modifier_stack(int) = external {}
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_parametric_type_alias_annotations() {
    let source = r#"
task_result := result(task(int), []string)
Value:task_result = external {}
tag_type := castable_subtype(tag)
entity_prefab_type := concrete_subtype(castable_subtype(entity))
tag_set_type := classifiable_subset(tag)
score_modifier_type := modifier(int)
score_stack_type := modifier_stack(int)
Use(Value:task_result, Tag:tag_type, Prefab:entity_prefab_type, Tags:tag_set_type, Modifier:score_modifier_type, Stack:score_stack_type):int = 42
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(6),
            Vec::<String>::new(),
            Some(vec![
                Type::Result(
                    Box::new(Type::Task(Box::new(Type::Int))),
                    Box::new(Type::Array(Box::new(Type::String))),
                ),
                Type::CastableSubtype(Box::new(Type::Class("tag".to_string()))),
                Type::ConcreteSubtype(Box::new(Type::CastableSubtype(Box::new(Type::Class(
                    "entity".to_string()
                ))))),
                Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string()))),
                Type::Modifier(Box::new(Type::Int)),
                Type::ModifierStack(Box::new(Type::Int)),
            ]),
            Type::Int
        )
    );
}

#[test]
fn evaluates_official_make_classifiable_subset_annotation() {
    let source = r#"
Subset:classifiable_subset(int) = MakeClassifiableSubset(array{1, 2, 3})
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_official_classifiable_subset_contains_members() {
    let source = r#"
Use(Set:classifiable_subset(tag), TagType:castable_subtype(tag), TagTypes:[]castable_subtype(tag)):void =
    if:
        Set.Contains[TagType]
        Set.ContainsAny[TagTypes]
        Set.ContainsAll[TagTypes]
    then:
        {}
    else:
        {}
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(3),
            Vec::<String>::new(),
            Some(vec![
                Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string()))),
                Type::CastableSubtype(Box::new(Type::Class("tag".to_string()))),
                Type::Array(Box::new(Type::CastableSubtype(Box::new(Type::Class(
                    "tag".to_string()
                ))))),
            ]),
            Type::None
        )
    );
}

#[test]
fn checks_make_classifiable_subset_of_castable_subtypes_returns_element_subset() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Make():classifiable_subset(tag) = MakeClassifiableSubset(array{TagType})
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            Vec::<String>::new(),
            Some(vec![]),
            Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string())))
        )
    );
}

#[test]
fn evaluates_classifiable_subset_accepts_subclass_runtime_members() {
    let source = r#"
entity := class:
    ID:int

character := class(entity):
    Health:int

Hero := character{ID := 40, Health := 2}
Set:classifiable_subset(entity) = MakeClassifiableSubset(array{Hero})
Hero.ID + Hero.Health
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_classifiable_subset_contains_runtime_success() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{TagType})
if (Set.Contains[TagType]). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_classifiable_subset_contains_any_and_all_runtime() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{TagType})
TagTypes:[]castable_subtype(tag) = array{TagType}
Any := if (Set.ContainsAny[TagTypes]). 20 else. 0
All := if (Set.ContainsAll[TagTypes]). 22 else. 0
Any + All
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_classifiable_subset_union_runtime() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Left:classifiable_subset(tag) = MakeClassifiableSubset(array{})
Right:classifiable_subset(tag) = MakeClassifiableSubset(array{TagType})
Combined:classifiable_subset(tag) = Left + Right
if (Combined.Contains[TagType]). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_classifiable_subset_union_type() {
    let source = r#"
Combine(Left:classifiable_subset(int), Right:classifiable_subset(int))<transacts>:classifiable_subset(int) =
    Left + Right
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(2),
            vec!["transacts".to_string()],
            Some(vec![
                Type::ClassifiableSubset(Box::new(Type::Int)),
                Type::ClassifiableSubset(Box::new(Type::Int)),
            ]),
            Type::ClassifiableSubset(Box::new(Type::Int))
        )
    );
}

#[test]
fn rejects_classifiable_subset_union_type_mismatch() {
    let source = r#"
Left:classifiable_subset(int) = MakeClassifiableSubset(array{1})
Right:classifiable_subset(string) = MakeClassifiableSubset(array{"x"})
Left + Right
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("incompatible types `int` and `string`")
    );
}

#[test]
fn evaluates_official_result_make_success_and_make_error() {
    let source = r#"
Success:result(int, string) = MakeSuccess(40)
Error:result(int, string) = MakeError("no")

GotSuccess := if (Value := Success.GetSuccess[]). Value else. 0
MissSuccess := if (Value := Error.GetSuccess[]). Value else. 1
GotError := if (Reason := Error.GetError[]). Reason.Length else. 0
MissError := if (Reason := Success.GetError[]). Reason.Length else. 1

GotSuccess + MissSuccess + GotError + MissError
"#;

    assert_eq!(eval(source), Value::Number(44.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_result_make_success_mismatched_success_type() {
    let error = check_source(r#"Outcome:result(int, string) = MakeSuccess("bad")"#)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `result(int,string)`")
    );
}

#[test]
fn rejects_official_result_make_error_mismatched_error_type() {
    let error =
        check_source("Outcome:result(int, string) = MakeError(7)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `result(int,string)`")
    );
}

#[test]
fn rejects_official_classifiable_subset_contains_type_mismatch() {
    let error = check_source(
        r#"
Set:classifiable_subset(tag) = external {}
EntityType:castable_subtype(entity) = external {}
Set.Contains[EntityType]
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `castable_subtype(tag)`")
    );
}

#[test]
fn rejects_unknown_parametric_type_annotation() {
    let error = check_source("Value:box(int) = external {}").expect_err("source should fail");

    assert!(error.to_string().contains("unknown parametric type `box`"));
}

#[test]
fn rejects_type_alias_conflicting_with_official_parametric_type() {
    let error = check_source("result := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `result` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_castable_subtype_parametric_type() {
    let error = check_source("castable_subtype := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `castable_subtype` conflicts with builtin type name")
    );
}

#[test]
fn evaluates_official_int_results_as_runtime_ints() {
    for source in [
        "array{10, 20}.Length",
        "array{10, 20}.Length()",
        r#"map{"a" => 1}.Length"#,
        r#"map{"a" => 1}.Length()"#,
        r#""abc".Length"#,
        r#""abc".Length()"#,
    ] {
        assert!(
            matches!(eval(source), Value::Int(_)),
            "`{source}` should evaluate to a runtime int"
        );
    }

    assert!(matches!(eval("array{10, 20}.Find[20]"), Value::Int(1)));
    assert!(matches!(eval(r#""abc".Find['b']"#), Value::Int(1)));

    let range_values = eval("for (I := 1..3) { I }");
    let Value::Array(items) = range_values else {
        panic!("range for should evaluate to an array");
    };
    let items = items.borrow();
    assert!(matches!(
        items.as_slice(),
        [Value::Int(1), Value::Int(2), Value::Int(3)]
    ));

    let pair_indices = eval(
        r#"
Values:[]int = array{10, 20}
for (Index -> Value : Values) { Index }
"#,
    );
    let Value::Array(items) = pair_indices else {
        panic!("pair-index for should evaluate to an array");
    };
    let items = items.borrow();
    assert!(matches!(items.as_slice(), [Value::Int(0), Value::Int(1)]));
}

#[test]
fn rejects_legacy_while_syntax() {
    let error = parse_source(
        r#"
while (true) {
    break
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected newline"));
}

#[test]
fn rejects_legacy_continue_statement() {
    let error = check_source(
        r#"
loop {
    continue
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `continue`"));
}

#[test]
fn evaluates_builtin_type_alias_annotations() {
    let source = r#"
score := int
Value:score = 42
Value
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_alias_unknown_type() {
    let error = check_source(
        r#"
bad_map := [missing]int
Scores:bad_map = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown type `missing`"));
}

#[test]
fn rejects_duplicate_type_alias() {
    let error = check_source(
        r#"
score_map := [string]int
score_map := [int]int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate type alias `score_map`")
    );
}

#[test]
fn rejects_local_type_alias() {
    let error = check_source(
        r#"
block:
    local_map := [string]int
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type aliases are only supported at module level")
    );
}

#[test]
fn rejects_type_alias_value_use() {
    let error = check_source(
        r#"
score_map := [string]int
score_map
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `score_map`"));
}

#[test]
fn rejects_cyclic_type_alias() {
    let error = check_source(
        r#"
a := []b
b := []a
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cyclic type alias"));
}
