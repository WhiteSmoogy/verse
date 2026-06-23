//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;
use verse_rs::ast::{ExprKind, StmtKind, TypeName};

#[test]
fn checks_distinct_int_and_float_annotations() {
    let source = r#"
Whole:int = 40
Fraction:float = 1.5
Widened:float = Whole
Whole + if (Value := Int[Fraction]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(41));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_int_range_type_annotation_for_integer_literals() {
    let source = r#"
Small:int_range(-5, 10) = -5
Small
"#;

    assert_eq!(eval(source), Value::Int(-5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(-5, 10))
    );
}

#[test]
fn evaluates_int_range_type_alias_annotation() {
    let source = r#"
small := int_range(0, 10)
Value:small = 7
Value
"#;

    assert_eq!(eval(source), Value::Int(7));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(0, 10))
    );
}

#[test]
fn evaluates_where_int_type_literal_as_type_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(type{X:int where 0 <= X, X < 256})
"#;

    assert_eq!(
        eval("type{X:int where 0 <= X, X < 256}"),
        Value::Type(TypeName::IntRange { min: 0, max: 255 })
    );
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_where_int_type_annotation_for_integer_literals() {
    let source = r#"
Small:type{X:int where 0 <= X, X < 256} = 255
Small
"#;

    assert_eq!(eval(source), Value::Int(255));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(0, 255))
    );
}

#[test]
fn evaluates_where_int_type_alias_annotation() {
    let source = r#"
positive := type{X:int where X > 0}
Value:positive = 1
Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(1, i64::MAX))
    );
}

#[test]
fn evaluates_where_float_type_literal_as_type_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(type{X:float where 0.0 <= X, X <= 1.0})
"#;

    assert_eq!(
        eval("type{X:float where 0.0 <= X, X <= 1.0}"),
        Value::Type(TypeName::FloatRange(
            FloatRange::new(0.0, 1.0).expect("valid float range")
        ))
    );
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_where_float_type_annotation_for_float_literals() {
    let source = r#"
Unit:type{X:float where 0.0 <= X, X <= 1.0} = 0.5
Unit
"#;

    assert_eq!(eval(source), Value::Float(0.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::FloatRange(FloatRange::new(0.0, 1.0).expect("valid float range"))
    );
}

#[test]
fn evaluates_where_float_type_alias_annotation() {
    let source = r#"
non_negative := type{X:float where 0.0 <= X, X <= Inf}
Value:non_negative = 1.5
Value
"#;

    assert_eq!(eval(source), Value::Float(1.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::FloatRange(FloatRange::new(0.0, f64::INFINITY).expect("valid float range"))
    );
}

#[test]
fn evaluates_nat_type_annotation_for_non_negative_integer_literals() {
    let source = r#"
Small:nat = 0
Large:nat = 42
Large
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(0, i64::MAX))
    );
}

#[test]
fn evaluates_nat_as_first_class_type_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(nat)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_sized_nat_type_annotations_for_integer_literals() {
    for (source, expected_value, expected_type) in [
        (
            "Value:nat8 = 255\nValue",
            255,
            Type::IntRange(IntRange::new(0, 255)),
        ),
        (
            "Value:nat16 = 65535\nValue",
            65_535,
            Type::IntRange(IntRange::new(0, 65_535)),
        ),
        (
            "Value:nat32 = 4294967295\nValue",
            4_294_967_295,
            Type::IntRange(IntRange::new(0, 4_294_967_295)),
        ),
        (
            "Value:nat64 = 9223372036854775807\nValue",
            i64::MAX,
            Type::IntRange(IntRange::new(0, i64::MAX)),
        ),
    ] {
        assert_eq!(eval(source), Value::Int(expected_value));
        assert_eq!(
            check_source(source).expect("source should check"),
            expected_type
        );
    }
}

#[test]
fn evaluates_sized_nat_as_first_class_type_values() {
    let source = r#"
Use(Kind:type):int = 10
Use(nat8) + Use(nat16) + Use(nat32) + Use(nat64) + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_sized_int_type_annotations_for_integer_literals() {
    for (source, expected_value, expected_type) in [
        (
            "Value:int8 = -128\nValue",
            -128,
            Type::IntRange(IntRange::new(-128, 127)),
        ),
        (
            "Value:int8 = 127\nValue",
            127,
            Type::IntRange(IntRange::new(-128, 127)),
        ),
        (
            "Value:int16 = -32768\nValue",
            -32_768,
            Type::IntRange(IntRange::new(-32_768, 32_767)),
        ),
        (
            "Value:int16 = 32767\nValue",
            32_767,
            Type::IntRange(IntRange::new(-32_768, 32_767)),
        ),
        (
            "Value:int32 = -2147483648\nValue",
            -2_147_483_648,
            Type::IntRange(IntRange::new(-2_147_483_648, 2_147_483_647)),
        ),
        (
            "Value:int32 = 2147483647\nValue",
            2_147_483_647,
            Type::IntRange(IntRange::new(-2_147_483_648, 2_147_483_647)),
        ),
        (
            "Value:int64 = -9223372036854775808\nValue",
            i64::MIN,
            Type::IntRange(IntRange::new(i64::MIN, i64::MAX)),
        ),
        (
            "Value:int64 = 9223372036854775807\nValue",
            i64::MAX,
            Type::IntRange(IntRange::new(i64::MIN, i64::MAX)),
        ),
    ] {
        assert_eq!(eval(source), Value::Int(expected_value));
        assert_eq!(
            check_source(source).expect("source should check"),
            expected_type
        );
    }
}

#[test]
fn evaluates_sized_int_as_first_class_type_values() {
    let source = r#"
Use(Kind:type):int = 10
Use(int8) + Use(int16) + Use(int32) + Use(int64) + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_numeric_alias_type_alias_annotations() {
    let source = r#"
small_int := int8
full_int := int64
wide_nat := nat16
A:small_int = -1
B:full_int = -9223372036854775808
C:wide_nat = 2
C
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::IntRange(IntRange::new(0, 65_535))
    );
}

#[test]
fn rejects_unexposed_float_width_alias_annotations() {
    for source in [
        "Value:float16 = 1.5",
        "Value:float32 = 1.5",
        "Value:float64 = 1.5",
        "Value:float128 = 1.5",
    ] {
        let error = check_source(source).expect_err("source should fail");
        assert!(
            error.to_string().contains("unknown type"),
            "expected unknown type error in {error}"
        );
    }
}

#[test]
fn rejects_type_alias_conflicting_with_reserved_float_width_names() {
    for source in [
        "float16 := float",
        "float32 := float",
        "float64 := float",
        "float128 := float",
    ] {
        let error = check_source(source).expect_err("source should fail");
        assert!(
            error
                .to_string()
                .contains("conflicts with builtin type name"),
            "expected reserved-name conflict in {error}"
        );
    }
}

#[test]
fn rejects_integer_literal_outside_int_range_annotation() {
    let error = check_source("Value:int_range(0, 10) = 11").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 10)`")
    );
}

#[test]
fn rejects_negative_integer_literal_assigned_to_nat() {
    let error = check_source("Value:nat = -1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 9223372036854775807)`")
    );
}

#[test]
fn rejects_integer_literal_outside_sized_int_annotations() {
    for (source, expected) in [
        ("Value:int8 = -129", "int_range(-128, 127)"),
        ("Value:int8 = 128", "int_range(-128, 127)"),
        ("Value:int16 = -32769", "int_range(-32768, 32767)"),
        ("Value:int16 = 32768", "int_range(-32768, 32767)"),
        (
            "Value:int32 = -2147483649",
            "int_range(-2147483648, 2147483647)",
        ),
        (
            "Value:int32 = 2147483648",
            "int_range(-2147483648, 2147483647)",
        ),
    ] {
        let error = check_source(source).expect_err("source should fail");
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in {error}"
        );
    }
}

#[test]
fn rejects_integer_literal_outside_sized_nat_annotations() {
    for (source, expected) in [
        ("Value:nat8 = 256", "int_range(0, 255)"),
        ("Value:nat16 = 65536", "int_range(0, 65535)"),
        ("Value:nat32 = 4294967296", "int_range(0, 4294967295)"),
    ] {
        let error = check_source(source).expect_err("source should fail");
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in {error}"
        );
    }
}

#[test]
fn rejects_negative_integer_literal_assigned_to_sized_nat() {
    let error = check_source("Value:nat8 = -1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 255)`")
    );
}

#[test]
fn rejects_non_literal_int_narrowing_to_int_range() {
    let error = check_source(
        r#"
Base:int = 7
Small:int_range(0, 10) = Base
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 10)`")
    );
}

#[test]
fn rejects_non_literal_int_narrowing_to_nat() {
    let error = check_source(
        r#"
Base:int = 7
Small:nat = Base
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 9223372036854775807)`")
    );
}

#[test]
fn rejects_empty_int_range_type_annotation() {
    let error = parse_source("Value:int_range(10, 0) = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("int_range minimum cannot be greater than maximum")
    );
}

#[test]
fn rejects_integer_literal_outside_where_int_type_annotation() {
    let error = check_source("Value:type{X:int where 0 <= X, X < 256} = 256")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `int_range(0, 255)`")
    );
}

#[test]
fn rejects_float_literal_outside_where_float_type_annotation() {
    let error = check_source("Value:type{X:float where 0.0 <= X, X <= 1.0} = 1.5")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `float_range(0.0, 1.0)`")
    );
}

#[test]
fn rejects_non_literal_float_narrowing_to_float_range() {
    let error = check_source(
        r#"
Base:float = 0.5
Small:type{X:float where 0.0 <= X, X <= 1.0} = Base
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `float_range(0.0, 1.0)`")
    );
}

#[test]
fn rejects_unsupported_where_int_type_comparison() {
    let error = parse_source("type{X:int where X = 1}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `<`, `<=`, `>`, or `>=` in `where` clause")
    );
}

#[test]
fn rejects_empty_where_int_type_range() {
    let error =
        parse_source("Value:type{X:int where X < 0, X > 10} = 1").expect_err("source should fail");

    assert!(error.to_string().contains("where type range is empty"));
}

#[test]
fn rejects_empty_where_float_type_range() {
    let error = parse_source("Value:type{X:float where X < 0.0, X > 1.0} = 0.5")
        .expect_err("source should fail");

    assert!(error.to_string().contains("where type range is empty"));
}

#[test]
fn rejects_strict_where_float_infinity_bound() {
    let error = parse_source("type{X:float where X > Inf}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("float cannot be strictly greater than infinity")
    );
}

#[test]
fn rejects_non_numeric_where_type_literal() {
    let error =
        parse_source("Value:type{X:string where X > 0} = \"bad\"").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("constrained `where` type literals currently support only `int` or `float`")
    );
}

#[test]
fn exposes_type_variables_from_type_parameters() {
    let program =
        parse_source("Accept(Value:t where t:subtype(comparable)):t = Value").expect("parse");
    let StmtKind::Let { expr, .. } = &program.statements[0].kind else {
        panic!("expected function binding");
    };
    let ExprKind::Function { params, .. } = &expr.kind else {
        panic!("expected function expression");
    };

    let variables = TypeVariable::from_type_params(&params[0].type_params);
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name, "t");
    assert!(variables[0].explicit);
    assert_eq!(variables[0].bounds.positive, Some(TypeName::Comparable));
    assert_eq!(variables[0].bounds.negative, None);
}

#[test]
fn exposes_type_variables_from_type_parameter_bounds() {
    let program =
        parse_source("Accept(Value:t where t:type(int, comparable)):t = Value").expect("parse");
    let StmtKind::Let { expr, .. } = &program.statements[0].kind else {
        panic!("expected function binding");
    };
    let ExprKind::Function { params, .. } = &expr.kind else {
        panic!("expected function expression");
    };

    let variables = TypeVariable::from_type_params(&params[0].type_params);
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name, "t");
    assert!(variables[0].explicit);
    assert_eq!(variables[0].bounds.negative, Some(TypeName::Int));
    assert_eq!(variables[0].bounds.positive, Some(TypeName::Comparable));
}

#[test]
fn evaluates_type_parameter_type_bounds() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
Use(Item:t where t:type(child_item, base_item)):int = Item.Value
Use(child_item{}) + Use(base_item{})
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_parameter_type_bounds_upper_mismatch() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
other_item := class:
    Value:int = 1
Use(Item:t where t:type(child_item, base_item)):int = 42
Use(other_item{})
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type argument `other_item` for `t` must be a subtype of `base_item`"),
        "{error}"
    );
}

#[test]
fn rejects_type_parameter_type_bounds_lower_mismatch() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
grandchild_item := class(child_item):
    GrandchildValue:int = 0
Use(Item:t where t:type(child_item, base_item)):int = 42
Use(grandchild_item{})
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "type argument `grandchild_item` for `t` must be a supertype of `child_item`"
        ),
        "{error}"
    );
}

#[test]
fn evaluates_type_value_type_bounds_annotation() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
Kind:type(child_item, base_item) = child_item
Value:Kind = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_value_type_bounds_alias_annotation() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
bounded := type(child_item, base_item)
Kind:bounded = child_item
Value:Kind = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_value_type_bounds_annotation_upper_mismatch() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
other_item := class:
    Value:int = 1
Kind:type(child_item, base_item) = other_item
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `type(child_item, base_item)`"),
        "{error}"
    );
}

#[test]
fn rejects_type_value_type_bounds_annotation_lower_mismatch() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
grandchild_item := class(child_item):
    GrandchildValue:int = 0
Kind:type(child_item, base_item) = grandchild_item
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `type(child_item, base_item)`"),
        "{error}"
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

    assert_eq!(eval(source), Value::Option(Some(Box::new(Value::Int(7)))));
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

    assert_eq!(eval(source), Value::Int(42));
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
VerseSource:subscribable_event(int) = external {}
SourceEvent:subscribable_event_intrnl(int) = external {}
Sticky:sticky_event(int) = external {}
Outcome:result(int, string) = external {}
Signal:signalable(int) = external {}
Waitable:awaitable(string) = external {}
AnyWaitable:awaitable() = external {}
Listener:listenable(agent) = external {}
AnyListener:listenable() = external {}
Subscription:subscribable() = external {}
Background:task(int) = external {}
Produced:generator(int) = external {}
UntypedProduced:generator() = external {}
AnyTagType:subtype(tag) = external {}
TagType:castable_subtype(tag) = external {}
ComponentType:castable_subtype(component) = external {}
EntityPrefab:concrete_subtype(castable_subtype(entity)) = external {}
ShortEntityPrefab:castable_concrete_subtype(entity) = external {}
TagSet:classifiable_subset(tag) = external {}
TagSetKey:classifiable_subset_key(tag) = external {}
TagSetVar:classifiable_subset_var(tag) = external {}
ScoreModifier:modifier(int) = external {}
ScoreStack:modifier_stack(int) = external {}
Succeeded:success_result(int) = external {}
Failed:error_result(string) = external {}
42
"#;

    assert_eq!(eval(source), Value::Int(42));
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
any_tag_type := subtype(tag)
entity_prefab_type := concrete_subtype(castable_subtype(entity))
short_entity_prefab_type := castable_concrete_subtype(entity)
tag_set_type := classifiable_subset(tag)
tag_set_key_type := classifiable_subset_key(tag)
tag_set_var_type := classifiable_subset_var(tag)
score_modifier_type := modifier(int)
score_stack_type := modifier_stack(int)
success_int_type := success_result(int)
error_string_type := error_result(string)
verse_source_type := subscribable_event(int)
source_event_type := subscribable_event_intrnl(int)
sticky_event_type := sticky_event(int)
any_listener_type := listenable()
Use(Value:task_result, AnyTag:any_tag_type, Tag:tag_type, Prefab:entity_prefab_type, ShortPrefab:short_entity_prefab_type, Tags:tag_set_type, Key:tag_set_key_type, Var:tag_set_var_type, Modifier:score_modifier_type, Stack:score_stack_type, Succeeded:success_int_type, Failed:error_string_type, VerseSource:verse_source_type, Source:source_event_type, Sticky:sticky_event_type, Listener:any_listener_type):int = 42
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(16),
            Vec::<String>::new(),
            Some(vec![
                Type::Result(
                    Box::new(Type::Task(Box::new(Type::Int))),
                    Box::new(Type::Array(Box::new(Type::String))),
                ),
                Type::Subtype(Box::new(Type::Class("tag".to_string()))),
                Type::CastableSubtype(Box::new(Type::Class("tag".to_string()))),
                Type::ConcreteSubtype(Box::new(Type::CastableSubtype(Box::new(Type::Class(
                    "entity".to_string()
                ))))),
                Type::ConcreteSubtype(Box::new(Type::CastableSubtype(Box::new(Type::Class(
                    "entity".to_string()
                ))))),
                Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string()))),
                Type::ClassifiableSubsetKey(Box::new(Type::Class("tag".to_string()))),
                Type::ClassifiableSubsetVar(Box::new(Type::Class("tag".to_string()))),
                Type::Modifier(Box::new(Type::Int)),
                Type::ModifierStack(Box::new(Type::Int)),
                Type::SuccessResult(Box::new(Type::Int)),
                Type::ErrorResult(Box::new(Type::String)),
                Type::SubscribableEvent(Box::new(Type::Int)),
                Type::SubscribableEventIntrnl(Some(Box::new(Type::Int))),
                Type::StickyEvent(Some(Box::new(Type::Int))),
                Type::Listenable(None),
            ]),
            Type::Int
        )
    );
}

#[test]
fn evaluates_class_type_value_as_type_annotation() {
    let source = r#"
item := class:
    Value:int = 0
Use(Kind:type):int = 42
Use(item)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_struct_type_value_as_type_annotation() {
    let source = r#"
point := struct:
    X:int = 0
Use(Kind:type):int = 42
Use(point)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_type_value_as_type_annotation() {
    let source = r#"
readable := interface:
    Read():int
Use(Kind:type):int = 42
Use(readable)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parametric_type_value_as_type_annotation() {
    let source = r#"
box(t:type) := class:
    Value:t
Use(Kind:type):int = 42
Use(box)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_builtin_type_names_as_type_values() {
    let source = r#"
Use(Kind:type):int = 3
Kind:type = int
Use(Kind) + Use(float) + Use(rational) + Use(logic) + Use(void) + Use(string) + Use(message) + Use(char) + Use(char8) + Use(char32) + Use(any) + Use(comparable) + Use(type) + Use(function)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_builtin_type_name_value_binding() {
    let source = r#"
DataTypes<public> := module:
    Kind<public>:type = int

Use(Kind:type):int = 42
Use(DataTypes.Kind)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_immutable_type_value_binding_as_type_annotation() {
    let source = r#"
token := class:
    Value:int = 0

Primitive:type = int
TokenKind:type = token

Number:Primitive = 40
Item:TokenKind = token{Value := 2}
Number + Item.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_type_value_binding_as_type_annotation() {
    let source = r#"
DataTypes<public> := module:
    token<public> := class:
        Value<public>:int = 0
    TokenKind<public>:type = token

Item:DataTypes.TokenKind = DataTypes.token{Value := 42}
Item.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_mutable_type_value_binding_as_type_annotation() {
    let source = r#"
var Kind:type = int
Value:Kind = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("mutable type value `Kind` cannot be used as a type annotation")
    );
}

#[test]
fn evaluates_function_returning_type_value() {
    let source = r#"
item := class:
    Value:int = 0
Pick():type = item
Use(Kind:type):int = 42
Use(Pick())
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_zero_arg_type_function_annotation() {
    let source = r#"
Pick():type = int
Value:Pick() = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_zero_arg_type_function_aggregate_annotation() {
    let source = r#"
item := class:
    Value:int = 42

Pick():type = item
Value:Pick() = item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_zero_arg_type_function_container_annotation() {
    let source = r#"
Pick():type = int
Values:[]Pick() = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_parameter_annotation() {
    let source = r#"
Identity(Kind:type):type = Kind
Value:Identity(int) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_as_type_value_argument() {
    let source = r#"
Identity(Kind:type):type = Kind
Use(Kind:type):int = 42
Use(Identity(int))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_as_dependent_type_value_argument() {
    let source = r#"
Identity(Kind:type):type = Kind
Pick(Kind:type, Item:Kind):Kind = Item
Pick(Identity(int), 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_value_call_with_parametric_alias_result() {
    let source = r#"
list(t:type) := []t
ListOf(Kind:type):type = list(Kind)
Pick(Kind:type, Items:Kind):Kind = Items
Picked := Pick(ListOf(int), array{40, 2})
if (Value := Picked[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_type_function_call_as_type_value_argument() {
    let source = r#"
DataTypes<public> := module:
    Identity<public>(Kind:type):type = Kind

Pick(Kind:type, Item:Kind):Kind = Item
Pick(DataTypes.Identity(int), 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_value_calls_by_arity() {
    let source = r#"
Pick():type = int
Pick(Kind:type):type = Kind
Use(Kind:type):int = 21
Use(Pick()) + Use(Pick(string))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_ordinary_overload_when_type_function_value_call_gets_value_argument() {
    let source = r#"
Pick(Kind:type):type = Kind
Pick(Value:int):int = Value
Pick(42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_named_ordinary_overload_when_type_function_value_call_gets_named_value_argument() {
    let source = r#"
Pick(Kind:type):type = Kind
Pick(Value:int):int = Value
Pick(Value := 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_ordinary_overload_when_type_function_value_call_constraint_mismatches() {
    let source = r#"
base_item := class:
    Value:int = 0
other_item := class:
    Value:int = 0

Pick(Kind:subtype(base_item)):type = Kind
Pick(Kind:type):int = 42
Pick(other_item)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_ordinary_type_returning_overload_when_type_function_value_call_gets_value_argument() {
    let source = r#"
Pick(Kind:type):type = Kind
Pick(Value:int):type = string
Use(Kind:type):int = 42
Use(Pick(7))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dynamic_type_returning_overload_when_type_function_constraint_mismatches() {
    let source = r#"
base_item := class:
    Value:int = 0
other_item := class:
    Value:int = 0

Pick(Kind:subtype(base_item)):type = Kind
Pick(Kind:type):type =
    Chosen:type = Kind
    Chosen
Use(Kind:type):int = 42
Use(Pick(other_item))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_value_calls_by_constraint() {
    let source = r#"
base_item := class:
    Value:int = 40
other_item := class:
    Value:int = 2

Pick(Kind:subtype(base_item)):type = Kind
Pick(Kind:subtype(other_item)):type = Kind
Use(Kind:type, Item:Kind):Kind = Item
Base := Use(Pick(base_item), base_item{})
Other := Use(Pick(other_item), other_item{})
Base.Value + Other.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_value_calls_by_more_specific_constraint() {
    let source = r#"
base_item := class:
    Value:int = 0
child_item := class(base_item):
    ChildValue:int = 40

Pick(Kind:subtype(base_item)):type = base_item
Pick(Kind:subtype(child_item)):type = child_item
Use(Kind:type, Item:Kind):Kind = Item
Value := Use(Pick(child_item), child_item{})
Value.ChildValue + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_value_call_where_subtype_parameter_inference() {
    let source = r#"
box(t:type) := class:
    Data:int = 0

Pick(Kind:subtype(box(t)) where t:type):type = t
Use(Kind:type, Item:Kind):Kind = Item
Use(Pick(box(int)), 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_value_call_where_subtype_parameter_inference_from_child() {
    let source = r#"
base_box(t:type) := class:
    Data:int = 0
child_box(t:type) := class(base_box(t)):
    ChildData:int = 0

Pick(Kind:subtype(base_box(t)) where t:type):type = t
Use(Kind:type, Item:Kind):Kind = Item
Use(Pick(child_box(int)), 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_parameter_container_annotation() {
    let source = r#"
Identity(Kind:type):type = Kind
Values:[]Identity(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_array_type_former_result_annotation() {
    let source = r#"
ListOf(Kind:type):type = []Kind

Values:ListOf(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_map_type_former_result_annotation() {
    let source = r#"
Table(Key:subtype(comparable), Value:type):type = [Key]Value

Scores:Table(string, int) = map{"answer" => 42}
if (Value := Scores["answer"]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_weak_map_type_former_result_annotation() {
    let source = r#"
SessionTable(Value:type):type = weak_map(session, Value)

var Global:SessionTable(int) = map{}
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_weak_map_type_former_value_call() {
    let source = r#"
SessionTable(Value:type):type = weak_map(session, Value)
Accept(Kind:type):int = 42
Accept(SessionTable(int))
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_option_type_former_result_annotation() {
    let source = r#"
Maybe(Kind:type):type = ?Kind

Value:Maybe(int) = option{42}
if (Item := Value?). Item else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_tuple_type_former_result_annotation() {
    let source = r#"
Pair(Left:type, Right:type):type = (Left, Right)

Value:Pair(int, string) = (40, "ready")
Value(0)
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_tuple_call_type_former_result_annotation() {
    let source = r#"
Pair(Left:type, Right:type):type = tuple(Left, Right)

Value:Pair(int, string) = (40, "ready")
Value(0)
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_tuple_type_former_value_call() {
    let source = r#"
Pair(Left:type, Right:type):type = (Left, Right)
Use(Kind:type, Item:Kind):Kind = Item

Value := Use(Pair(int, string), (40, "ready"))
Value(0)
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_official_parametric_result_former_annotation() {
    let source = r#"
ResultOf(Success:type, Error:type):type = result(Success, Error)

Outcome:ResultOf(int, string) = MakeSuccess(40)
if (Value := Outcome.GetSuccess[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_official_parametric_former_annotations() {
    let source = r#"
EventOf(Payload:type):type = event(Payload)
TaskOf(Payload:type):type = task(Payload)
SetOf(Item:type):type = classifiable_subset(Item)
ModifierOf(Item:type):type = modifier(Item)

Ready:EventOf(int) = external {}
Background:TaskOf(int) = external {}
TagSet:SetOf(tag) = external {}
ScoreModifier:ModifierOf(int) = external {}
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_official_parametric_former_value_call() {
    let source = r#"
ResultOf(Success:type, Error:type):type = result(Success, Error)
Use(Kind:type, Item:Kind):Kind = Item

Outcome := Use(ResultOf(int, string), MakeSuccess(40))
if (Value := Outcome.GetSuccess[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_official_modifier_stack_result_runtime_surface() {
    let source = r#"
DataTypes<public> := module:
    ModifierOf<public>(Item:type):type = modifier(Item)
    StackOf<public>(Item:type):type = modifier_stack(Item)

add := class(DataTypes.ModifierOf(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

Stack:DataTypes.StackOf(int) = external {}
Stack.AddModifier(add{Amount := 40}, 0)
Stack.Evaluate(2)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_external_aggregate_field_defaults_runtime_surface() {
    let source = r#"
DataTypes<public> := module:
    ModifierOf<public>(Item:type):type = modifier(Item)
    StackOf<public>(Item:type):type = modifier_stack(Item)

add := class(DataTypes.ModifierOf(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

holder := class:
    Stack:DataTypes.StackOf(int) = external {}

record := struct:
    Stack:DataTypes.StackOf(int) = external {}

Item := holder{}
Item.Stack.AddModifier(add{Amount := 40}, 0)
StructItem := record{}
StructItem.Stack.AddModifier(add{Amount := 20}, 0)
Item.Stack.Evaluate(2) + StructItem.Stack.Evaluate(0)
"#;

    assert_eq!(eval(source), Value::Int(62));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_official_event_result_archetype_surface() {
    let source = r#"
EventOf(Payload:type):type = event(Payload)

Ready:EventOf(int) = EventOf(int){}
Ready.Signal(42)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_official_sticky_event_result_archetype_surface() {
    let source = r#"
StickyOf(Payload:type):type = sticky_event(Payload)

Ready:StickyOf(int) = StickyOf(int){}
Before:int = if (Ready.IsSignaled[]). 1 else. 0
Ready.Signal(7)
After:int = if (Ready.IsSignaled[]). 40 else. 0
Before + After + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_bounds_former_value_call() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounded():type = type(child_item, base_item)
Use(Kind:type, Item:Kind):Kind = Item

Selected:Bounded() = Use(Bounded(), child_item)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_function_parameter_annotation() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounded():type = type(child_item, base_item)
Accept(Kind:Bounded()):int = 42

Accept(child_item)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_class_method_parameter_runtime() {
    let source = r#"
box(t:type) := class:
    Value:t

BoxOf(Kind:type):type = box(Kind)

reader := class:
    Read(Item:BoxOf(int)):int =
        Item.Value

Reader := reader{}
Reader.Read(box(int){Value := 42})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_extension_receiver_annotation() {
    let source = r#"
ListOf(Kind:type):type = []Kind

(Items:ListOf(int)).Second():int =
    if (Value := Items[1]). Value else. 0

array{40, 2}.Second()
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_extension_receiver_overload_runtime() {
    let source = r#"
box(t:type) := class:
    Value:t

BoxOf(Kind:type):type = box(Kind)

(Item:BoxOf(int)).Score():int =
    Item.Value
(Text:string).Score():int =
    0

box(int){Value := 42}.Score()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_parametric_class_surface() {
    let source = r#"
ListOf(Kind:type):type = []Kind

box(t:type) := class:
    Values:t

child_box(t:type) := class(box(ListOf(t))):
    Label:int = 1

Box:child_box(int) = child_box(int){Values := array{40, 2}}
if (Value := Box.Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_parametric_interface_surface() {
    let source = r#"
ListOf(Kind:type):type = []Kind

reader(t:type) := interface:
    Read():t

array_reader(t:type) := interface(reader(ListOf(t))):
    Marker():int = 1

int_reader := class(array_reader(int)):
    Values:[]int = array{40, 2}
    Read<override>():[]int = Values

Use(Reader:reader(ListOf(int))):int =
    Values := Reader.Read()
    if (Value := Values[1]). Value else. 0

Use(int_reader{})
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_class_parent_runtime_surface() {
    let source = r#"
base_box(t:type) := class:
    Value:t
    Read():t = Value

BoxOf(Kind:type):type = base_box(Kind)

child_box(t:type) := class(BoxOf(t)):
    Extra:int = 2

Item := child_box(int){Value := 40}
Item.Read() + Item.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_interface_parent_runtime_surface() {
    let source = r#"
reader(t:type) := interface:
    Read():t

ReaderOf(Kind:type):type = reader(Kind)

child_reader(t:type) := interface(ReaderOf(t)):
    Extra:int = 2

box := class(child_reader(int)):
    Value:int = 40
    Read<override>():int = Value

Item:child_reader(int) = box{}
Item.Read() + Item.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_class_parent_by_constraint_runtime_surface() {
    let source = r#"
base_item := class:
    Base:int = 0

other_item := class:
    Other:int = 0

base_box(t:subtype(base_item)) := class:
    BaseValue:int = 0

other_box(t:subtype(other_item)) := class:
    OtherValue:int = 40
    Read():int = OtherValue

BoxOf(Kind:subtype(base_item)):type = base_box(Kind)
BoxOf(Kind:subtype(other_item)):type = other_box(Kind)

child_box(t:subtype(other_item)) := class(BoxOf(t)):
    Extra:int = 2

Item := child_box(other_item){}
Item.Read() + Item.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_class_parent_with_constraint_supplier_runtime_surface() {
    let source = r#"
base_item := class:
    Value:int = 40
    Read():int = Value

child_item := class(base_item):
    Child:int = 2

SubtypeOfBase():type = subtype(base_item)
BoxOf(Kind:SubtypeOfBase()):type = Kind

derived_item := class(BoxOf(child_item)):
    Extra:int = 0

Item := derived_item{}
Item.Read() + Item.Child
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_interface_parent_with_dependent_constraint_supplier_runtime_surface() {
    let source = r#"
readable := interface:
    Read():int

named_readable := interface(readable):
    Label():int

SubtypeOf(Base:type):type = subtype(Base)
ReaderOf(Base:type, Kind:SubtypeOf(Base)):type = Kind

child_reader := interface(ReaderOf(readable, named_readable)):
    Extra():int = 2

box := class(child_reader):
    Read<override>():int = 40
    Label<override>():int = 0

Item:child_reader = box{}
Item.Read() + Item.Extra() + Item.Label()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_in_official_parametric_surface() {
    let source = r#"
Payload():type = int

Use(Event:event(Payload())):int = 42

Use(event(int){})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_class_result_type_value_surface() {
    let source = r#"
box(t:type) := class:
    Value:t
    Read():t = Value

BoxOf(Kind:type):type = box(Kind)
Pick(Kind:type, Item:Kind):Kind = Item

Picked := Pick(BoxOf(int), box(int){Value := 42})
Picked.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_interface_result_type_value_surface() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

ReaderOf(Kind:type):type = reader(Kind)
Pick(Kind:type, Item:Kind):Kind = Item

Picked := Pick(ReaderOf(int), box(int){Value := 42})
Picked.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_class_result_cast_surface() {
    let source = r#"
base_box(t:type) := class:
    Value:t
    Read():t = Value

child_box(t:type) := class(base_box(t)):
    Extra:int = 2

BoxOf(Kind:type):type = base_box(Kind)
Item:child_box(int) = child_box(int){Value := 42}

if (Casted := BoxOf(int)[Item]). Casted.Read() else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_interface_result_cast_surface() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

ReaderOf(Kind:type):type = reader(Kind)
Item:box(int) = box(int){Value := 42}

if (Casted := ReaderOf(int)[Item]). Casted.Read() else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_class_result_archetype_surface() {
    let source = r#"
box(t:type) := class:
    Value:t
    Read():t = Value

BoxOf(Kind:type):type = box(Kind)
Box := BoxOf(int){Value := 42}
Box.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_class_result_classifiable_subset_runtime() {
    let source = r#"
box(t:type) := class<castable>(tag):
    Marker:int = 0

BoxOf(Kind:type):type = box(Kind)

BoxKind:castable_subtype(tag) = BoxOf(int)
PlainKind:castable_subtype(tag) = box(int)

SetFromFunction:classifiable_subset(tag) = MakeClassifiableSubset(array{BoxKind})
HasPlain := if (SetFromFunction.Contains[PlainKind]). 20 else. 0

SetFromPlain:classifiable_subset(tag) = MakeClassifiableSubset(array{PlainKind})
HasFunction := if (SetFromPlain.Contains[BoxKind]). 22 else. 0

HasPlain + HasFunction
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_class_result_subtype_value_parameters_runtime() {
    let source = r#"
base_item := class:
    Value:int = 0

box(t:type) := class<concrete><castable>(base_item):
    Marker:int = 0

BoxOf(Kind:type):type = box(Kind)

UseSubtype(Kind:subtype(base_item)):int = 10
UseCastable(Kind:castable_subtype(base_item)):int = 20
UseConcrete(Kind:concrete_subtype(castable_subtype(base_item))):int = 12

UseSubtype(BoxOf(int)) + UseCastable(BoxOf(int)) + UseConcrete(BoxOf(int))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_parametric_struct_result_archetype_surface() {
    let source = r#"
point(t:type) := struct:
    Value:t

PointOf(Kind:type):type = point(Kind)
Point := PointOf(int){Value := 42}
Point.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_primitive_result_annotation() {
    let source = r#"
Pick():type = type{1}
Value:Pick() = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_primitive_value_call() {
    let source = r#"
Pick():type = type{1}
Use(Kind:type, Item:Kind):Kind = Item
Use(Pick(), 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_signed_number_result_annotation() {
    let source = r#"
PickInt():type = type{-1}
PickFloat():type = type{+1.0}
Whole:PickInt() = 42
Fraction:PickFloat() = 0.5
Whole
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_container_result_annotation() {
    let source = r#"
Pick():type = type{array{1, 2}}
Values:Pick() = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_map_result_annotation() {
    let source = r#"
Pick():type = type{map{"answer" => 42}}
Values:Pick() = map{"answer" => 42}
if (Value := Values["answer"]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_tuple_result_annotation() {
    let source = r#"
Pick():type = type{(40, "ready")}
Value:Pick() = (40, "ready")
Value(0)
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_option_result_annotation() {
    let source = r#"
Pick():type = type{option{1}}
Value:Pick() = option{42}
if (Item := Value?). Item else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_aggregate_result_annotation() {
    let source = r#"
item := class:
    Value:int = 42

Pick():type = type{item{}}
Value:Pick() = item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_literal_function_signature_result_annotation() {
    let source = r#"
Pick():type = type{_(:int):int}
Double(X:int):int = X * 2
Handler:Pick() = Double
Handler(21)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_function_type_literal_signed_non_number_result_annotation() {
    let source = r#"
Pick():type = type{+"ready"}
Value:Pick() = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("unary `+` expected `number`, got `string`"),
        "{error}"
    );
}

#[test]
fn evaluates_type_function_parametric_alias_result_annotation() {
    let source = r#"
list(t:type) := []t
ListOf(Kind:type):type = list(Kind)
Values:ListOf(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_alias_target_from_type_function_call() {
    let source = r#"
ListOf(Kind:type):type = []Kind
int_list := ListOf(int)

Values:int_list = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_type_alias_target_from_type_function_call() {
    let source = r#"
Pick():type = int
int_list := []Pick()

Values:int_list = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parametric_type_alias_target_from_type_function_call() {
    let source = r#"
ListOf(Kind:type):type = []Kind
list_alias(Kind:type) := ListOf(Kind)

Values:list_alias(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_parametric_type_alias_target_from_type_function_call() {
    let source = r#"
Identity(Kind:type):type = Kind
list_alias(Kind:type) := []Identity(Kind)

Values:list_alias(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_static_type_function_call_result_annotation() {
    let source = r#"
Pick():type = Other()
Other():type = int
Value:Pick() = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_static_type_function_call_with_type_parameter_result_annotation() {
    let source = r#"
list(t:type) := []t
ListOf(Kind:type):type = Inner(Kind)
Inner(Kind:type):type = list(Kind)
Values:ListOf(int) = array{40, 2}
if (Value := Values[1]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_annotations_by_arity() {
    let source = r#"
Pick():type = int
Pick(Kind:type):type = Kind

IntValue:Pick() = 42
TextValue:Pick(string) = "ready"
IntValue
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_annotations_by_constraint() {
    let source = r#"
base_item := class:
    Value:int = 42
other_item := class:
    Label:string = "ready"

Pick(Kind:subtype(base_item)):type = Kind
Pick(Kind:subtype(other_item)):type = Kind

Value:Pick(base_item) = base_item{}
Other:Pick(other_item) = other_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_annotations_by_more_specific_constraint() {
    let source = r#"
base_item := class:
    Value:int = 0
child_item := class(base_item):
    ChildValue:int = 42

Pick(Kind:subtype(base_item)):type = base_item
Pick(Kind:subtype(child_item)):type = child_item

Value:Pick(child_item) = child_item{}
Value.ChildValue
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_overloaded_type_function_annotations_by_more_specific_interface_constraint() {
    let source = r#"
readable := interface:
    Read():int
named_readable := interface(readable):
    Label():int
box := class(named_readable):
    Read<override>():int = 40
    Label<override>():int = 2

Pick(Kind:subtype(readable)):type = readable
Pick(Kind:subtype(named_readable)):type = named_readable

Value:Pick(box) = box{}
Value.Read() + Value.Label()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_subtype_parameter_inference() {
    let source = r#"
box(t:type) := class:
    Data:int = 0

Pick(Kind:subtype(box(t)) where t:type):type = t
Value:Pick(box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_where_subtype_parameter_inference() {
    let source = r#"
box(t:type) := class:
    Data:int = 0

BoxSubtype(Element:type):type = subtype(box(Element))
Pick(Kind:BoxSubtype(t) where t:type):type = t
Value:Pick(box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_where_subtype_parameter_inference_from_type_parameter() {
    let source = r#"
base_box(t:type) := class:
    Value:t

BoxSubtype(Element:type):type = subtype(base_box(Element))
Pick(Kind:BoxSubtype(t) where t:type):type = base_box(t)

holder(t:type) := class:
    Value:Pick(base_box(t))

Item := holder(int){Value := base_box(int){Value := 42}}
Item.Value.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_inferred_class_parent_runtime_surface() {
    let source = r#"
base_box(t:type) := class:
    Value:t
    Read():t = Value

BoxSubtype(Element:type):type = subtype(base_box(Element))
BoxOf(Kind:BoxSubtype(t) where t:type):type = base_box(t)

child_box(t:type) := class(BoxOf(base_box(t))):
    Extra:int = 2

Item := child_box(int){Value := 40}
Item.Read() + Item.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_inferred_interface_parent_runtime_surface() {
    let source = r#"
base_reader(t:type) := interface:
    Read():t

ReaderSubtype(Element:type):type = subtype(base_reader(Element))
ReaderOf(Kind:ReaderSubtype(t) where t:type):type = base_reader(t)

child_reader(t:type) := interface(ReaderOf(base_reader(t))):
    Extra():int = 2

box := class(child_reader(int)):
    Value:int = 40
    Read<override>():int = Value

Item:child_reader(int) = box{}
Item.Read() + Item.Extra()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_subtype_parameter_inference_from_child() {
    let source = r#"
base_box(t:type) := class:
    Data:int = 0
child_box(t:type) := class(base_box(t)):
    ChildData:int = 0

Pick(Kind:subtype(base_box(t)) where t:type):type = t
Value:Pick(child_box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_subtype_parameter_inference_from_interface_implementer() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

Pick(Kind:subtype(reader(t)) where t:type):type = t
Value:Pick(box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_where_subtype_parameter_inference_from_interface_child() {
    let source = r#"
base_reader(t:type) := interface:
    Read():t

child_reader(t:type) := interface(base_reader(t)):
    Label():string

Pick(Kind:subtype(base_reader(t)) where t:type):type = t
Value:Pick(child_reader(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_bounds_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Pick(Kind:type(child_item, base_item)):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounds():type = type(child_item, base_item)
Pick(Kind:Bounds()):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_forward_type_function_call_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Pick(Kind:Bounds()):type = Kind
Bounds():type = type(child_item, base_item)
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_parameterized_type_bounds_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounds(Lower:type, Upper:type):type = type(Lower, Upper)
Pick(Kind:Bounds(child_item, base_item)):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_dependent_parameterized_type_bounds_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounds(Lower:type, Upper:type):type = type(Lower, Upper)
Pick(Lower:type, Upper:type, Kind:Bounds(Lower, Upper)):type = Kind
Value:Pick(child_item, base_item, child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_subtype_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

SubtypeOfBase():type = subtype(base_item)
Pick(Kind:SubtypeOfBase()):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_parameterized_subtype_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

SubtypeOf(Base:type):type = subtype(Base)
Pick(Kind:SubtypeOf(base_item)):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_dependent_parameterized_subtype_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

SubtypeOf(Base:type):type = subtype(Base)
Pick(Base:type, Kind:SubtypeOf(Base)):type = Kind
Value:Pick(base_item, child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_call_castable_concrete_parameter_constraints() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

CastableTag():type = castable_subtype(tag)
ConcreteCastableTag():type = concrete_subtype(castable_subtype(tag))
PickCastable(Kind:CastableTag()):type = Kind
PickConcrete(Kind:ConcreteCastableTag()):type = Kind
CastableValue:PickCastable(puzzle_light) = external {}
ConcreteValue:PickConcrete(puzzle_light) = external {}
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_call_parameterized_castable_concrete_parameter_constraints() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

CastableOf(Base:type):type = castable_subtype(Base)
ConcreteCastableOf(Base:type):type = concrete_subtype(castable_subtype(Base))
PickCastable(Base:type, Kind:CastableOf(Base)):type = Kind
PickConcrete(Base:type, Kind:ConcreteCastableOf(Base)):type = Kind
CastableValue:PickCastable(tag, puzzle_light) = external {}
ConcreteValue:PickConcrete(tag, puzzle_light) = external {}
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_where_castable_parameter_inference() {
    let source = r#"
castable_box(t:type) := class<castable>:
    Data:int = 0

CastableBox(Element:type):type = castable_subtype(castable_box(Element))
Pick(Kind:CastableBox(t) where t:type):type = t
Value:Pick(castable_box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_call_where_concrete_castable_parameter_inference() {
    let source = r#"
prefab_box(t:type) := class<concrete><castable>:
    Data:int = 0

PrefabBox(Element:type):type = concrete_subtype(castable_subtype(prefab_box(Element)))
Pick(Kind:PrefabBox(t) where t:type):type = t
Value:Pick(prefab_box(int)) = 42
Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_type_bounds_former_result_annotation() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Bounded():type = type(child_item, base_item)
Kind:Bounded() = child_item
Value:Kind = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_function_subtype_parameter_constraint() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

Pick(Kind:subtype(base_item)):type = Kind
Value:Pick(child_item) = child_item{}
Value.Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_type_function_castable_concrete_parameter_constraints() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}

PickCastable(Kind:castable_subtype(tag)):type = Kind
PickConcrete(Kind:concrete_subtype(castable_subtype(tag))):type = Kind
CastableValue:PickCastable(puzzle_light) = external {}
ConcreteValue:PickConcrete(puzzle_light) = external {}
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_function_type_parameter_wrong_arity() {
    let source = r#"
Identity(Kind:type):type = Kind
Value:Identity(int, string) = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type function `Identity` expected 1 type arguments, got 2"),
        "{error}"
    );
}

#[test]
fn rejects_type_function_type_bounds_parameter_constraint_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
other_item := class:
    Value:int = 1

Pick(Kind:type(child_item, base_item)):type = Kind
Value:Pick(other_item) = external {}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type argument `other_item` for `Kind` must be a subtype of `base_item`"),
        "{error}"
    );
}

#[test]
fn rejects_type_function_subtype_parameter_constraint_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
other_item := class:
    Value:int = 1

Pick(Kind:subtype(base_item)):type = Kind
Value:Pick(other_item) = external {}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type argument `other_item` for `Kind` must be a subtype of `base_item`"),
        "{error}"
    );
}

#[test]
fn rejects_type_function_value_call_subtype_parameter_constraint_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
other_item := class:
    Value:int = 1

Pick(Kind:subtype(base_item)):type = Kind
Use(Kind:type):int = 42
Use(Pick(other_item))
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type argument `other_item` for `Kind` must be a subtype of `base_item`"),
        "{error}"
    );
}

#[test]
fn rejects_named_type_function_value_call_without_ordinary_overload() {
    let source = r#"
Identity(Kind:type):type = Kind
Use(Kind:type):int = 42
Use(Identity(Kind := int))
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type function `Identity` does not accept named type arguments"),
        "{error}"
    );
}

#[test]
fn rejects_type_function_castable_parameter_constraint_mismatch() {
    let source = r#"
plain_tag := class(tag){}

Pick(Kind:castable_subtype(tag)):type = Kind
Value:Pick(plain_tag) = external {}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error.to_string().contains(
            "type argument `plain_tag` for `Kind` must be a subtype of `castable_subtype(tag)`"
        ),
        "{error}"
    );
}

#[test]
fn rejects_overloaded_type_function_annotation_constraint_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 0
other_item := class:
    Value:int = 0
plain_item := class:
    Value:int = 0

Pick(Kind:subtype(base_item)):type = Kind
Pick(Kind:subtype(other_item)):type = Kind
Value:Pick(plain_item) = external {}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("no overload of type function `Pick` matches type arguments"),
        "{error}"
    );
}

#[test]
fn rejects_type_function_where_subtype_parameter_inference_mismatch() {
    let source = r#"
box(t:type) := class:
    Data:int = 0
other := class:
    Data:int = 0

Pick(Kind:subtype(box(t)) where t:type):type = t
Value:Pick(other) = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("could not infer type parameter `t` for type function `Pick`"),
        "{error}"
    );
}

#[test]
fn rejects_value_parameter_type_return_function_as_type_function_annotation() {
    let source = r#"
Other(Value:int):type = int
Pick():type = Other(1)
Value:Pick() = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error.to_string().contains("unknown parametric type `Pick`"),
        "{error}"
    );
}

#[test]
fn rejects_dynamic_type_literal_result_as_static_type_function_annotation() {
    let source = r#"
Make():int = 42
Pick():type = type{Make()}
Value:Pick() = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error.to_string().contains("unknown parametric type `Pick`"),
        "{error}"
    );
}

#[test]
fn rejects_zero_arg_type_function_type_arguments() {
    let source = r#"
Pick():type = int
Value:Pick(string) = 42
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type function `Pick` expected 0 type arguments, got 1"),
        "{error}"
    );
}

#[test]
fn evaluates_function_returning_struct_and_interface_type_values() {
    let source = r#"
point := struct:
    X:int = 0
readable := interface:
    Read():int
PickPoint():type = point
PickReadable():type = readable
Use(Kind:type):int = 21
Use(PickPoint()) + Use(PickReadable())
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_type_value_as_type_annotation() {
    let error = check_source(
        r#"
Use(Kind:type):int = 42
Use(42)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `type`, got `int`")
    );
}

#[test]
fn evaluates_type_literal_expression_for_primitive_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(type{1})
"#;

    assert_eq!(eval("type{1}"), Value::Type(TypeName::Int));
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_container_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(type{array{1, 2}})
"#;

    assert_eq!(
        eval("type{array{1, 2}}"),
        Value::Type(TypeName::Array(Some(Box::new(TypeName::Int))))
    );
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_class_instance_value() {
    let source = r#"
token := class{}
Use(Kind:type):int = 42
Use(type{token{}})
"#;

    assert_eq!(
        eval("token := class{}\ntype{token{}}"),
        Value::Type(TypeName::Named("token".to_string()))
    );
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_function_signature_value() {
    let source = r#"
Use(Kind:type):int = 42
Use(type{_(:float)<reads>:float})
"#;

    assert_eq!(
        eval("type{_(:float)<reads>:float}"),
        Value::Type(TypeName::FunctionSignature {
            params: vec![TypeName::Float],
            effects: vec!["reads".to_string()],
            return_type: Box::new(TypeName::Float),
        })
    );
    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_dependent_primitive_annotation() {
    let source = r#"
Pick(Kind:type, Item:Kind):Kind =
    Item

Pick(type{1}, 42)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_dependent_container_annotation() {
    let source = r#"
Pick(Kind:type, Item:Kind):Kind =
    Item

Values := Pick(type{array{1, 2}}, array{40, 2})
if (Value := Values[0]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(40));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_literal_expression_for_dependent_class_annotation() {
    let source = r#"
token := class:
    Value:int = 0

Pick(Kind:type, Item:Kind):Kind =
    Item

Picked := Pick(type{token{}}, token{Value := 42})
Picked.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_type_literal_expression_for_dependent_annotation() {
    let source = r#"
Read(Value:float)<reads>:float = Value
Accept(Kind:type, Fn:Kind):int = 42
Accept(type{_(:float)<reads>:float}, Read)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_function_returning_function_signature_type_value() {
    let source = r#"
Read(Value:float)<reads>:float = Value
MakeSignature()<converges>:type{_(:float)<reads>:float} =
    Read

Accept(Fn:type{_(:float)<reads>:float}):int = 42
Accept(MakeSignature())
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_literal_expression_dependent_annotation_mismatch() {
    let source = r#"
Pick(Kind:type, Item:Kind):Kind =
    Item

Pick(type{1}, "bad")
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("argument 2 expected `int`, got `string`")
    );
}

#[test]
fn evaluates_parametric_type_alias_annotation() {
    let source = r#"
list(t:type) := []t

Values:list(int) = array{1, 2}
if (Value := Values[0]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_constrained_parametric_type_alias_annotation() {
    let source = r#"
lookup(t:subtype(comparable), v:type) := [t]v

Scores:lookup(string, int) = map{"ready" => 42}
if (Value := Scores["ready"]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_castable_concrete_parametric_type_alias_constraints() {
    let source = r#"
base_tag := class(tag){}
child_tag := class<concrete><castable>(base_tag){}
plain_tag := class<concrete>(base_tag){}
castable_list(t:castable_subtype(base_tag)) := []t
prefab_list(t:concrete_subtype(castable_subtype(base_tag))) := []t
short_prefab_list(t:castable_concrete_subtype(base_tag)) := []t

CastableItems:castable_list(child_tag) = array{child_tag{}}
PrefabItems:prefab_list(child_tag) = array{child_tag{}}
ShortItems:short_prefab_list(child_tag) = array{child_tag{}}
CastableItems.Length + PrefabItems.Length + ShortItems.Length
"#;

    assert_eq!(eval(source), Value::Int(3));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_castable_parametric_type_alias_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
plain_tag := class<concrete>(base_tag){}
castable_list(t:castable_subtype(base_tag)) := []t

Items:castable_list(plain_tag) = array{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `castable_subtype(base_tag)`"),
        "{error}"
    );
}

#[test]
fn rejects_concrete_parametric_type_alias_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
castable_tag := class<castable>(base_tag){}
prefab_list(t:concrete_subtype(castable_subtype(base_tag))) := []t

Items:prefab_list(castable_tag) = array{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `concrete_subtype(castable_subtype(base_tag))`"),
        "{error}"
    );
}

#[test]
fn evaluates_shared_constraint_parametric_type_alias_annotation() {
    let source = r#"
pair(t&u:type) := tuple(t, u)

Value:pair(int, string) = (40, "ready")
Value(0) + Value(1).Length
"#;

    assert_eq!(eval(source), Value::Int(45));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_bounds_parametric_type_alias_annotation() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
holder(t:type(child_item, base_item)) := []t

Items:holder(child_item) = array{child_item{}}
if (Item := Items[0]). Item.Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_bounds_parametric_type_alias_upper_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
other_item := class:
    Value:int = 1
holder(t:type(child_item, base_item)) := []t

Items:holder(other_item) = array{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("type argument `other_item` for `t` must be a subtype of `base_item`"),
        "{error}"
    );
}

#[test]
fn rejects_type_bounds_parametric_type_alias_lower_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
grandchild_item := class(child_item):
    GrandchildValue:int = 0
holder(t:type(child_item, base_item)) := []t

Items:holder(grandchild_item) = array{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error.to_string().contains(
            "type argument `grandchild_item` for `t` must be a supertype of `child_item`"
        ),
        "{error}"
    );
}

#[test]
fn rejects_parametric_type_alias_wrong_arity() {
    let source = r#"
list(t:type) := []t

Values:list(int, string) = array{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("parametric type `list` expected 1 type arguments, got 2")
    );
}

#[test]
fn rejects_parametric_type_alias_constraint_mismatch() {
    let source = r#"
holder := class:
    Value:int = 0

lookup(t:subtype(comparable), v:type) := [t]v

Scores:lookup(holder, int) = map{}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `comparable`")
    );
}

#[test]
fn evaluates_parametric_type_alias_value_use() {
    let source = r#"
list(t:type) := []t

Use(Kind:type):int = 42
Kind:type = list
Use(Kind) + Use(list)
"#;

    assert_eq!(eval(source), Value::Int(84));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parametric_type_alias_call_as_type_value() {
    let source = r#"
list(t:type) := []t

Use(Kind:type):int = 42
Kind:type = list(int)
Use(Kind) + Use(list(int))
"#;

    assert_eq!(eval(source), Value::Int(84));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_parametric_type_alias_value_use() {
    let source = r#"
DataTypes<public> := module:
    list<public>(t:type) := []t

Use(Kind:type):int = 42
Alias:type = DataTypes.list
Use(Alias) + Use(DataTypes.list)
"#;

    assert_eq!(eval(source), Value::Int(84));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_make_classifiable_subset_annotation() {
    let source = r#"
Subset:classifiable_subset(int) = MakeClassifiableSubset(array{1, 2, 3})
42
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn checks_classifiable_subset_contains_members_accept_castable_supertype_queries() {
    let source = r#"
puzzle_light := class<castable>(tag):
    Strength:int = 0

Use(Set:classifiable_subset(puzzle_light), TagType:castable_subtype(tag), TagTypes:[]castable_subtype(tag)):void =
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
                Type::ClassifiableSubset(Box::new(Type::Class("puzzle_light".to_string()))),
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
fn checks_official_classifiable_subset_filter_by_type_member() {
    let source = r#"
Use(Set:classifiable_subset(tag), TagType:castable_subtype(tag))<transacts>:classifiable_subset(tag) =
    Set.FilterByType(TagType)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(2),
            vec!["transacts".to_string()],
            Some(vec![
                Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string()))),
                Type::CastableSubtype(Box::new(Type::Class("tag".to_string()))),
            ]),
            Type::ClassifiableSubset(Box::new(Type::Class("tag".to_string())))
        )
    );
}

#[test]
fn checks_classifiable_subset_filter_by_type_accepts_castable_supertype_query() {
    let source = r#"
puzzle_light := class<castable>(tag):
    Strength:int = 0

Use(Set:classifiable_subset(puzzle_light), TagType:castable_subtype(tag))<transacts>:classifiable_subset(puzzle_light) =
    Set.FilterByType(TagType)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(2),
            vec!["transacts".to_string()],
            Some(vec![
                Type::ClassifiableSubset(Box::new(Type::Class("puzzle_light".to_string()))),
                Type::CastableSubtype(Box::new(Type::Class("tag".to_string()))),
            ]),
            Type::ClassifiableSubset(Box::new(Type::Class("puzzle_light".to_string())))
        )
    );
}

#[test]
fn checks_official_classifiable_subset_var_member_surface() {
    let source = r#"
UseWrites(Var:classifiable_subset_var(tag), Set:classifiable_subset(tag), Item:tag, TagType:castable_subtype(tag))<transacts>:classifiable_subset_key(tag) =
    Var.Write(Set)
    NewKey := Var.Add(Item)
    Var.Read()
    Var.FilterByType(TagType)
    NewKey

UseDecides(Var:classifiable_subset_var(tag), Key:classifiable_subset_key(tag), TagType:castable_subtype(tag), TagTypes:[]castable_subtype(tag))<transacts><decides>:void =
    if:
        Var.Remove[Key]
        Var.Contains[TagType]
        Var.ContainsAny[TagTypes]
        Var.ContainsAll[TagTypes]
    then:
        {}
    else:
        {}

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_classifiable_subset_var_members_accept_castable_supertype_queries() {
    let source = r#"
puzzle_light := class<castable>(tag):
    Strength:int = 0

UseWrites(Var:classifiable_subset_var(puzzle_light), Set:classifiable_subset(puzzle_light), Item:puzzle_light, TagType:castable_subtype(tag))<transacts>:classifiable_subset_key(puzzle_light) =
    Var.Write(Set)
    NewKey := Var.Add(Item)
    Var.Read()
    Var.FilterByType(TagType)
    NewKey

UseDecides(Var:classifiable_subset_var(puzzle_light), Key:classifiable_subset_key(puzzle_light), TagType:castable_subtype(tag), TagTypes:[]castable_subtype(tag))<transacts><decides>:void =
    if:
        Var.Remove[Key]
        Var.Contains[TagType]
        Var.ContainsAny[TagTypes]
        Var.ContainsAll[TagTypes]
    then:
        {}
    else:
        {}

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn evaluates_get_castable_final_super_class_from_type() {
    let source = r#"
base_component := class<final_super><castable>(component):
    Value:int = 40

child_component := class(base_component):
    Extra:int = 2

Use(Kind:castable_subtype(component)):int = 42

if (Kind := GetCastableFinalSuperClassFromType[component, child_component]). Use(Kind) else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_get_castable_final_super_class_from_instance() {
    let source = r#"
base_component := class<final_super><castable>(component):
    Value:int = 40

child_component := class(base_component):
    Extra:int = 2

Use(Kind:castable_subtype(component)):int = 42
Child := child_component{}

if (Kind := GetCastableFinalSuperClass[component, Child]). Use(Kind) else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_get_castable_final_super_class_failure_for_non_castable_direct_class() {
    let source = r#"
base_component := class<final_super>(component):
    Value:int = 40

child_component := class(base_component):
    Extra:int = 2

if (Kind := GetCastableFinalSuperClassFromType[component, child_component]). 0 else. 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_classifiable_subset_var_runtime_surface() {
    let source = r#"
base_item := class:
    Value:int

child_item := class<castable>(base_item):
    Score:int

other_item := class<castable>(base_item):
    Extra:int

Child := child_item{Value := 1, Score := 2}
Other := other_item{Value := 3, Extra := 4}
ChildType:castable_subtype(base_item) = child_item
OtherType:castable_subtype(base_item) = other_item

Var:classifiable_subset_var(base_item) = MakeClassifiableSubsetVar(array{Child})
Replacement:classifiable_subset(base_item) = MakeClassifiableSubset(array{Child})
Var.Write(Replacement)
ReadSet := Var.Read()
Key := Var.Add(Other)
Removed := if (Var.Remove[Key]). 10 else. 0
NoOther := if (Var.ContainsNone[array{OtherType}]). 11 else. 0
Filtered := Var.FilterByType(ChildType)
HasChild := if (Filtered.Contains[ChildType]). 12 else. 0
OtherVar:classifiable_subset_var(base_item) = MakeClassifiableSubsetVar(array{Other})
Combined := Var + OtherVar
HasUnionOther := if (Combined.Contains[OtherType]). 9 else. 0
ReadHasChild := if (ReadSet.Contains[ChildType]). 0 else. 100
Removed + NoOther + HasChild + HasUnionOther + ReadHasChild
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_castable_instance_is_of_type_helper() {
    let source = r#"
base_item := class<castable>:
    Value:int = 0

child_item := class<castable>(base_item):
    Score:int = 0

other_item := class<castable>(base_item):
    Extra:int = 0

Base:base_item = child_item{Value := 40, Score := 2}
Matched := if (Base.IsOfType[child_item]). 40 else. 0
Missed := if (Base.IsOfType[other_item]). 0 else. 2
BaseMatched := if (Base.IsOfType[base_item]). 0 else. 100
Matched + Missed + BaseMatched
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_is_of_type_helper_with_non_castable_query_type() {
    let source = r#"
base_item := class<castable>:
    Value:int = 0

plain_item := class:
    Value:int = 0

Item := base_item{Value := 40}
Item.IsOfType[plain_item]
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("argument 1 expected `castable_subtype(any)`, got `class<plain_item>`")
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_classifiable_subset_filter_by_type_runtime() {
    let source = r#"
base_item := class:
    Value:int

child_item := class<castable>(base_item):
    Score:int

other_item := class<castable>(base_item):
    Extra:int

Child := child_item{Value := 1, Score := 41}
Set:classifiable_subset(base_item) = MakeClassifiableSubset(array{Child})
ChildFiltered := Set.FilterByType(child_item)
OtherFiltered := Set.FilterByType(other_item)
HasChild := if (ChildFiltered.Contains[child_item]). 40 else. 0
HasOther := if (OtherFiltered.Contains[other_item]). 0 else. 2
HasChild + HasOther
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(44));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_success_and_error_result_generated_classes() {
    let source = r#"
Success:success_result(int) = MakeSuccess(40)
Error:error_result(string) = MakeError("no")
AsSuccessResult:result(int, string) = Success
AsErrorResult:result(int, string) = Error

PickSuccess(Value:success_result(t) where t:type):t =
    Value.Success

PickError(Value:error_result(t) where t:type):t =
    Value.Error

GotSuccess := if (Value := AsSuccessResult.GetSuccess[]). Value else. 0
GotError := if (Reason := AsErrorResult.GetError[]). Reason.Length else. 0
GotSuccess + GotError + PickSuccess(MakeSuccess(0)) + PickError(MakeError("")).Length
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn rejects_official_success_result_assigned_from_error_result() {
    let error = check_source(r#"Success:success_result(int) = MakeError("bad")"#)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `success_result(int)`")
    );
}

#[test]
fn rejects_official_error_result_assigned_from_success_result() {
    let error = check_source("Error:error_result(string) = MakeSuccess(42)")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `error_result(string)`")
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
fn rejects_official_classifiable_subset_filter_by_type_mismatch() {
    let error = check_source(
        r#"
Set:classifiable_subset(tag) = external {}
EntityType:castable_subtype(entity) = external {}
Set.FilterByType(EntityType)
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
fn rejects_official_classifiable_subset_var_filter_by_type_mismatch() {
    let error = check_source(
        r#"
Var:classifiable_subset_var(tag) = external {}
EntityType:castable_subtype(entity) = external {}
Var.FilterByType(EntityType)
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
fn rejects_classifiable_subset_filter_by_type_in_computes_function() {
    let error = check_source(
        r#"
Use(Set:classifiable_subset(tag), TagType:castable_subtype(tag))<computes>:classifiable_subset(tag) =
    Set.FilterByType(TagType)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_classifiable_subset_var_add_in_computes_function() {
    let error = check_source(
        r#"
Use(Var:classifiable_subset_var(tag), Item:tag)<computes>:classifiable_subset_key(tag) =
    Var.Add(Item)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
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
fn rejects_type_alias_conflicting_with_official_subtype_parametric_type() {
    let error = check_source("subtype := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `subtype` conflicts with builtin type name")
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

    assert!(matches!(
        eval("if (Index := array{10, 20}.Find[20]). Index else. -1"),
        Value::Int(1)
    ));
    assert!(matches!(
        eval(r#"if (Index := "abc".Find['b']). Index else. -1"#),
        Value::Int(1)
    ));

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

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_type_alias_value_use() {
    let source = r#"
score_map := [string]int
Use(Kind:type):int = 42
Use(score_map)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_type_alias_value_use() {
    let source = r#"
DataTypes<public> := module:
    score<public> := int
    list<public>(t:type) := []t

Use(Kind:type):int = 21
Use(DataTypes.score) + Use(DataTypes.list(int))
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
