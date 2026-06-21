//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_bindings_and_blocks() {
    let source = r#"
x := 10
{
    y := 5
    x + y
}
"#;

    assert_eq!(eval(source), Value::Int(15));
}

#[test]
fn evaluates_public_data_specifier_on_constant() {
    let source = r#"
Answer<public>:int = 42
Answer
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_typed_binding_as_opaque_value() {
    let source = r#"
ExternalValue:float = external {}
str(ExternalValue)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_scoped_external_digest_binding() {
    let source = r#"
texture_2d := class:
    ID:int = 0

MyTexture<scoped {MyProject}>:texture_2d = external {}
str(MyTexture)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_scoped_digest_class_binding() {
    let source = r#"
material := class:
    ID:int = 0

concrete_material<scoped {ParameterizedMaterialsTest}> := class<final><public>(material):
    var Specular:float = external {}

Material := concrete_material{}
str(Material.Specular)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_native_localized_external_class_field_default() {
    let source = r#"
text_base<native><public> := class:
    DefaultText<native><localizes><public>:message = external {}

Text := text_base{}
str(Text.DefaultText)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_external_class_field_default_as_opaque_value() {
    let source = r#"
concrete_material := class:
    var Specular:float = external {}

Material := concrete_material{}
str(Material.Specular)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_external_binding_without_type_annotation() {
    let error = check_source(r#"ExternalValue := external {}"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`external {}` requires an explicit type annotation")
    );
}

#[test]
fn rejects_scoped_data_specifier_without_scope_block() {
    let error = parse_source(r#"ExternalValue<scoped>:float = external {}"#)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `{` after `scoped` data specifier")
    );
}

#[test]
fn rejects_duplicate_scoped_data_specifier() {
    let error = parse_source(r#"ExternalValue<scoped {A}><scoped {B}>:float = external {}"#)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate data specifier `scoped`")
    );
}

#[test]
fn rejects_non_empty_external_braces() {
    let error = parse_source(r#"ExternalValue:float = external { Value := 1 }"#)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `}` after `external {`")
    );
}

#[test]
fn rejects_localizes_data_specifier_without_message_annotation() {
    let error =
        check_source(r#"Tip<localizes>:string = "Not a message""#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`localizes` data specifier requires a `message` annotation")
    );
}

#[test]
fn rejects_duplicate_data_specifier() {
    let error =
        parse_source(r#"Tip<public><public>:message = "Hi""#).expect_err("source should fail");

    assert!(error.to_string().contains("duplicate data specifier"));
}

#[test]
fn rejects_unknown_data_specifier() {
    let error =
        parse_source(r#"Tip<visible_to_editor>:message = "Hi""#).expect_err("source should fail");

    assert!(error.to_string().contains("unknown data specifier"));
}

#[test]
fn rejects_unsupported_data_specifier() {
    let error = parse_source(r#"Tip<constructor>:message = "Hi""#).expect_err("source should fail");

    assert!(error.to_string().contains("unsupported data specifier"));
}

#[test]
fn evaluates_native_data_specifier_on_external_binding() {
    let source = r#"
ExternalValue<native>:float = external {}
str(ExternalValue)
"#;

    assert_eq!(eval(source), Value::String("<external>".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_external_classifiable_subset_contains_as_empty_runtime_subset() {
    let source = r#"
Set:classifiable_subset(tag) = external {}
TagType:castable_subtype(tag) = external {}
if (Set.Contains[TagType]). 0 else. 42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_external_classifiable_subset_contains_any_and_all_empty_runtime_subset() {
    let source = r#"
Set:classifiable_subset(tag) = external {}
TagType:castable_subtype(tag) = external {}
SomeTags:[]castable_subtype(tag) = array{TagType}
NoTags:[]castable_subtype(tag) = array{}
Any := if (Set.ContainsAny[SomeTags]). 0 else. 20
All := if (Set.ContainsAll[NoTags]). 22 else. 0
Any + All
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn rejects_external_empty_classifiable_subset_contains_outside_failure_context() {
    let source = r#"
Set:classifiable_subset(tag) = external {}
TagType:castable_subtype(tag) = external {}
Set.Contains[TagType]
"#;
    assert_failable_context_error(source);
}

#[test]
fn rejects_non_constant_scalar_case_pattern() {
    let error = check_source(
        r#"
case (2):
    1 + 1 => 42
    _ => 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must be an `int`"));
}

#[test]
fn evaluates_data_member_defaults_with_converges_calls() {
    let source = r#"
Make()<converges>:int = 20
Score()<converges>:int = 2

record := struct:
    Value:int = Make()

taggable := interface:
    Score:int = Score()

thing := class(taggable):
    Value:int = Make()

record{}.Value + thing{}.Value + thing{}.Score
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_recursive_data_member_default_construction() {
    run_source(
        r#"
node := class:
    Value : int = 0
"#,
    )
    .expect("initial type should run");

    let error = run_source(
        r#"
node := class:
    Child : node = node{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field default `node.Child` recursively constructs `node`")
    );
}

#[test]
fn rejects_data_member_default_no_rollback_call_through_run_source() {
    let error = run_source(
        r#"
Make():int = 42

bad := class:
    Value:int = Make()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <converges> effect cannot call function requiring <no_rollback> effect"
    ));
}

#[test]
fn evaluates_verse_style_constant_definitions() {
    let source = r#"
Answer:int = 40
if (Answer = 40) {
    Answer + 2
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Int(42));
}
