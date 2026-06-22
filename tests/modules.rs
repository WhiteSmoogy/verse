//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_official_using_statement() {
    let source = r#"
using { /Verse.org/Verse }
Abs(-42)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_common_official_using_headers() {
    let source = r#"
using { /Fortnite.com/Devices }
using { /Verse.org/Random }
using { /Verse.org/Simulation }
using { /UnrealEngine.com/Temporary/Diagnostics }
42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn using_remains_available_as_non_statement_identifier() {
    let source = r#"
using := 40
using + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_relative_using_module_path() {
    let error = parse_source("using { Verse.org/Verse }").expect_err("source should fail");

    assert!(error.to_string().contains("expected `}`"));
}

#[test]
fn rejects_empty_using_module_path() {
    let error = parse_source("using { / }").expect_err("source should fail");

    assert!(error.to_string().contains("module path component"));
}

#[test]
fn rejects_unsupported_using_module_path() {
    let error = check_source(
        r#"
using { /Localhost/MyModule }
42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unsupported module path"));
}

#[test]
fn rejects_using_inside_block() {
    let error = check_source(
        r#"
{
    using { /Verse.org/Verse }
    42
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("module level"));
}

#[test]
fn evaluates_module_constant_member_access() {
    let source = r#"
DataTypes<public> := module:
    Answer<public>:int = 42

DataTypes.Answer
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_default_internal_module_constant_member_access_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    Answer:int = 42

DataTypes.Answer
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `Answer` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_private_or_protected_access_specifiers_outside_classes() {
    let cases = [
        (
            "module data",
            r#"
DataTypes<public> := module:
    Hidden<private>:int = 42

0
"#,
        ),
        (
            "module function",
            r#"
Hidden<protected>():int = 42
Hidden()
"#,
        ),
        (
            "module class",
            r#"
hidden := class<private>:
    Value:int = 0

0
"#,
        ),
        (
            "module type alias",
            r#"
DataTypes<public> := module:
    hidden_map<private> := [string]int

0
"#,
        ),
        (
            "module parametric type",
            r#"
DataTypes<public> := module:
    box<private>(t:type) := class:
        Value:t

0
"#,
        ),
        (
            "module extension method",
            r#"
(Value:int).Hidden<protected>():int = Value
41.Hidden()
"#,
        ),
    ];

    for (label, source) in cases {
        let error = check_source(source).expect_err("source should fail");
        assert!(
            error
                .to_string()
                .contains("Access levels protected and private are only allowed inside classes"),
            "{label}: {error}"
        );
    }
}

#[test]
fn rejects_public_type_alias_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := []secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_data_annotation_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    Item<public>:secret = external {}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Item` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_inferred_data_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class<concrete>:
        Value:int = 0
    Leaked<public> := secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Leaked` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_inferred_data_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class<concrete>:
        Value:int = 0
    Leaked<public> := secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Leaked` is public but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_function_inferred_return_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class<concrete>:
        Value:int = 0
    Make<public>() = secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Make` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_function_inferred_return_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class<concrete>:
        Value:int = 0
    Make<public>() = secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Make` is public but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_function_signature_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    Reveal<public>(Item:[]secret):int = 42

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_parametric_type_argument_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    box<public>(t:type) := class:
        Item<public>:t
    Item<public>:box(secret) = external {}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Item` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_parametric_type_public_field_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    box<public>(t:type) := class:
        Item<public>:secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.box.Item` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_function_subtype_constraint_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    Reveal<public>(Item:t where t:subtype(secret)):int = 42

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_interface_parent_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    base := interface:
        Ping():int
    exposed<public> := interface(base):
        Pong():int

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed` is public but depends on `DataTypes.base`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_public_field_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := class:
        Item<public>:secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Item` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_protected_field_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := class:
        Item<protected>:secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Item` is protected but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_protected_field_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class:
        Value:int = 0
    exposed<public> := class:
        Item<protected>:secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Item` is protected but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_protected_method_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := class:
        Reveal<protected>(Item:secret):int = 42

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Reveal` is protected but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_public_method_inferred_return_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class<concrete>:
        Value:int = 0
    exposed<public> := class:
        Item:secret
        Make<public>() = Item

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Make` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_protected_method_inferred_return_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class<concrete>:
        Value:int = 0
    exposed<public> := class:
        Item:secret
        Make<protected>() = Item

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Make` is protected but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_interface_protected_field_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := interface:
        Item<protected>:secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Item` is protected but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_interface_public_method_inferred_return_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class<concrete>:
        Value:int = 0
    exposed<public> := interface:
        Item:secret
        Make<public>() = Item

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Make` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_interface_protected_method_inferred_return_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class<concrete>:
        Value:int = 0
    exposed<public> := interface:
        Item:secret
        Make<protected>() = Item

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed.Make` is protected but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_extension_method_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    (Item:secret).Reveal<public>():int = 42

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_extension_method_inferred_return_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret := class<concrete>:
        Value:int = 0
    (Value:int).Reveal<public>() = secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_extension_method_inferred_return_exposing_scoped_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    secret<scoped{DataTypes}> := class<concrete>:
        Value:int = 0
    (Value:int).Reveal<public>() = secret{}

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn rejects_public_class_base_exposing_internal_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    base := class:
        Value:int = 0
    exposed<public> := class(base):
        Item<public>:int = 42

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed` is public but depends on `DataTypes.base`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_type_alias_exposing_internal_child_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    Hidden := module:
        secret<public> := class:
            Value:int = 0
    exposed<public> := []DataTypes.Hidden.secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed` is public but depends on `DataTypes.Hidden.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_public_type_alias_exposing_scoped_child_module_type() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    Hidden<scoped{DataTypes}> := module:
        secret<public> := class:
            Value:int = 0
    exposed<public> := []DataTypes.Hidden.secret

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.exposed` is public but depends on `DataTypes.Hidden.secret`, which is scoped"),
        "{error}"
    );
}

#[test]
fn allows_public_class_internal_field_with_internal_module_type() {
    let source = r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := class:
        Item:secret

0
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn allows_public_class_internal_method_with_internal_module_type() {
    let source = r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    exposed<public> := class:
        Reveal(Item:secret):int = 42

0
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_duplicate_type_alias_and_parametric_type_access_specifiers() {
    let cases = [
        ("type alias", "score_map<public><public> := [string]int"),
        (
            "parametric type",
            r#"
box<public><public>(t:type) := class:
    Value:t
"#,
        ),
    ];

    for (label, source) in cases {
        let error = parse_source(source).expect_err(label);
        assert!(
            error.to_string().contains("Duplicate access levels"),
            "{label}: {error}"
        );
    }
}

#[test]
fn rejects_conflicting_type_alias_and_parametric_type_access_specifiers() {
    let cases = [
        ("type alias", "score_map<public><internal> := [string]int"),
        (
            "parametric type",
            r#"
box<public><internal>(t:type) := class:
    Value:t
"#,
        ),
    ];

    for (label, source) in cases {
        let error = check_source(source).expect_err(label);
        assert!(
            error.to_string().contains("Conflicting access levels"),
            "{label}: {error}"
        );
    }
}

#[test]
fn evaluates_module_class_qualified_type_and_archetype() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Left<public>:int = 0
        Forward<public>:int = 0

Location:DataTypes.tile_coordinate = DataTypes.tile_coordinate{Left := 40, Forward := 2}
Location.Left + Location.Forward
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_class_exported_by_class_public_specifier() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate := class<public><concrete>:
        Left:int = 40
        Forward:int = 2
        Sum<public>():int = Left + Forward

Location:DataTypes.tile_coordinate = DataTypes.tile_coordinate{}
Location.Sum()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_qualified_class_base_runtime_surface() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Value<public>:int = 40

tile_coordinate := class<concrete>:
    Value<public>:int = 1

child := class<concrete>(DataTypes.tile_coordinate):
    Extra:int = 2

Child := child{}
if (Base := DataTypes.tile_coordinate[Child]):
    Base.Value + Child.Extra
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
fn evaluates_module_qualified_interface_parent_runtime_surface() {
    let source = r#"
Contracts<public> := module:
    readable<public> := interface:
        Value:int

readable := interface:
    Other:int

child_readable := interface(Contracts.readable):
    Extra():int

entry := class<concrete>(child_readable):
    Value<override>:int = 42
    Extra<override>():int = 0

entry{}.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_qualified_parametric_class_base_runtime_surface() {
    let source = r#"
DataTypes<public> := module:
    box<public>(t:type) := class:
        Value<public>:t

box(t:type) := class:
    Value<public>:t

int_child := class(DataTypes.box(int)):
    Extra:int = 2

Child := int_child{Value := 40}
Read(Base:DataTypes.box(int)):int =
    Base.Value

Read(Child) + Child.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_path_qualified_module_type_annotation_and_archetype() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Value<public>:int = 42

Location:(DataTypes:)tile_coordinate = (DataTypes:)tile_coordinate{}
Location.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_path_qualified_parametric_module_type_annotation_and_archetype() {
    let source = r#"
DataTypes<public> := module:
    box<public>(t:type) := class:
        Value<public>:t

Item:(DataTypes:)box(int) = (DataTypes:)box(int){Value := 42}
Item.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_default_internal_module_parametric_type_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    box(t:type) := class:
        Value:t

Value:DataTypes.box(int) = external {}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `box` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_internal_class_field_access_outside_defining_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Left:int = 42

Location:DataTypes.tile_coordinate = DataTypes.tile_coordinate{}
Location.Left
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Left` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_internal_class_method_call_outside_defining_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    counter<public> := class<concrete>:
        Hidden():int = 42

Counter:DataTypes.counter = DataTypes.counter{}
Counter.Hidden()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("method `Hidden` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_internal_class_field_archetype_outside_defining_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Left:int = 0

DataTypes.tile_coordinate{Left := 42}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Left` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_default_internal_module_qualified_type_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    tile_coordinate := class<concrete>:
        Left<public>:int = 0

Location:DataTypes.tile_coordinate = DataTypes.tile_coordinate{Left := 42}
Location.Left
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `tile_coordinate` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_module_class_with_internal_class_specifier_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    tile_coordinate := class<internal><concrete>:
        Left:int = 42

Location:DataTypes.tile_coordinate = DataTypes.tile_coordinate{}
Location.Left
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `tile_coordinate` is internal to module `DataTypes`")
    );
}

#[test]
fn evaluates_local_module_using_for_types_and_values() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Left<public>:int = 0
        Forward<public>:int = 0
    Origin<public>:tile_coordinate = tile_coordinate{}

using { DataTypes }
Location:tile_coordinate = tile_coordinate{Left := 40, Forward := 2}
Location.Left + Location.Forward + Origin.Left
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_cross_file_explicit_module_import() {
    let root = temp_project_dir("explicit_module");
    write_project_file(
        &root,
        "Math.verse",
        r#"
Math<public> := module:
    Double<public>(Value:int)<computes>:int = Value * 2
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Math }
Math.Double(21)
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_vproject_manifest_package_root_module_resolution() {
    let root = temp_project_dir("vproject_package_root");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = Source/main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "Source/main.verse",
        r#"
Api.Answer
"#,
    );

    let manifest = root.join("Demo.vproject");
    let entry = root.join("Source/main.verse");
    let project = SourceProject::from_manifest(&manifest).expect("manifest should load");
    assert_eq!(project.package.as_deref(), Some("DemoPackage"));
    assert_eq!(
        check_project_file(&manifest).expect("manifest project should check"),
        Type::Int
    );
    assert_eq!(
        check_project_file(&entry).expect("entry should discover manifest root"),
        Type::Int
    );
}

#[test]
fn checks_declared_package_dependency_digest_import() {
    let root = temp_project_dir("declared_package_dependency_digest_import");
    write_project_file(
        &root,
        "Shared\\Shared.vproject",
        r#"
package = Shared
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Shared\\Api.verse",
        r#"
Api<public> := module:
    Hidden:int = 7
    Answer<public>:int = 42
"#,
    );
    write_project_file(&root, "Shared\\main.verse", "false");
    write_project_file(
        &root,
        "Game\\Game.vproject",
        r#"
package = Game
dependencyPackages = Shared
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
using { Api }
Answer
"#,
    );

    let manifest = root.join("Game\\Game.vproject");
    let project = SourceProject::from_manifest(&manifest).expect("manifest should load");
    assert_eq!(project.dependencies, vec!["Shared".to_string()]);
    assert_eq!(
        check_project_file(&manifest).expect("project should check"),
        Type::Int
    );
}

#[test]
fn checks_transitive_package_dependency_api_surface() {
    let root = temp_project_dir("transitive_package_dependency_api_surface");
    write_project_file(
        &root,
        "Base\\Base.vproject",
        r#"
package = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Base\\Core.verse",
        r#"
Core<public> := module:
    token<public> := class<concrete>:
        Value<public>:int = 0
"#,
    );
    write_project_file(&root, "Base\\main.verse", "false");
    write_project_file(
        &root,
        "Shared\\Shared.vproject",
        r#"
package = Shared
dependencyPackages = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Shared\\Api.verse",
        r#"
SharedApi<public> := module:
    Make<public>():Core.token = external {}
"#,
    );
    write_project_file(&root, "Shared\\main.verse", "false");
    write_project_file(
        &root,
        "Game\\Game.vproject",
        r#"
package = Game
dependencyPackages = Shared
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
using { SharedApi }
0
"#,
    );

    let manifest = root.join("Game\\Game.vproject");
    assert_eq!(
        check_project_file(&manifest).expect("project should check"),
        Type::Int
    );
}

#[test]
fn rejects_direct_import_from_transitive_package_dependency() {
    let root = temp_project_dir("direct_import_from_transitive_package_dependency");
    write_project_file(
        &root,
        "Base\\Base.vproject",
        r#"
package = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Base\\Core.verse",
        r#"
Core<public> := module:
    token<public> := class<concrete>:
        Value<public>:int = 0
    Answer<public>:int = 42
"#,
    );
    write_project_file(&root, "Base\\main.verse", "false");
    write_project_file(
        &root,
        "Shared\\Shared.vproject",
        r#"
package = Shared
dependencyPackages = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Shared\\SharedApi.verse",
        r#"
SharedApi<public> := module:
    Read<public>():Core.token = external {}
"#,
    );
    write_project_file(&root, "Shared\\main.verse", "false");
    write_project_file(
        &root,
        "Game\\Game.vproject",
        r#"
package = Game
dependencyPackages = Shared
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
using { Core }
Answer
"#,
    );

    let manifest = root.join("Game\\Game.vproject");
    let error = check_project_file(&manifest).expect_err("project should fail");
    assert!(
        error.to_string().contains(
            "module `Core` is defined in dependency package `Base`, but package `Game` does not declare a direct dependency on `Base`"
        ),
        "{error}"
    );
}

#[test]
fn checks_direct_import_after_declaring_transitive_package_dependency() {
    let root = temp_project_dir("direct_import_after_declaring_transitive_package_dependency");
    write_project_file(
        &root,
        "Base\\Base.vproject",
        r#"
package = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Base\\Core.verse",
        r#"
Core<public> := module:
    token<public> := class<concrete>:
        Value<public>:int = 0
    Answer<public>:int = 42
"#,
    );
    write_project_file(&root, "Base\\main.verse", "false");
    write_project_file(
        &root,
        "Shared\\Shared.vproject",
        r#"
package = Shared
dependencyPackages = Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Shared\\SharedApi.verse",
        r#"
SharedApi<public> := module:
    Read<public>():Core.token = external {}
"#,
    );
    write_project_file(&root, "Shared\\main.verse", "false");
    write_project_file(
        &root,
        "Game\\Game.vproject",
        r#"
package = Game
dependencyPackages = Shared, Base
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
using { Core }
Answer
"#,
    );

    let manifest = root.join("Game\\Game.vproject");
    assert_eq!(
        check_project_file(&manifest).expect("project should check"),
        Type::Int
    );
}

#[test]
fn rejects_available_package_import_without_declared_dependency() {
    let root = temp_project_dir("undeclared_package_dependency_import");
    write_project_file(
        &root,
        "Shared\\Shared.vproject",
        r#"
package = Shared
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Shared\\Api.verse",
        r#"
Api<public> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(&root, "Shared\\main.verse", "false");
    write_project_file(
        &root,
        "Game\\Game.vproject",
        r#"
package = Game
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
using { Api }
Answer
"#,
    );

    let manifest = root.join("Game\\Game.vproject");
    let error = check_project_file(&manifest).expect_err("project should fail");
    assert!(error.to_string().contains("unsupported module path `Api`"));
}

#[test]
fn rejects_unknown_declared_package_dependency() {
    let root = temp_project_dir("unknown_declared_package_dependency");
    write_project_file(
        &root,
        "Game.vproject",
        r#"
package = Game
dependencyPackages = Missing
entry = main.verse
"#,
    );
    write_project_file(&root, "main.verse", "0");

    let manifest = root.join("Game.vproject");
    let error = check_project_file(&manifest).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("unknown dependency package `Missing`")
    );
}

#[test]
fn rejects_duplicate_declared_package_dependency() {
    let root = temp_project_dir("duplicate_declared_package_dependency");
    write_project_file(
        &root,
        "Game.vproject",
        r#"
package = Game
dependencyPackages = Shared, Shared
entry = main.verse
"#,
    );

    let manifest = root.join("Game.vproject");
    let error = SourceProject::from_manifest(&manifest).expect_err("manifest should fail");
    assert!(
        error
            .to_string()
            .contains("duplicate dependency package `Shared`")
    );
}

#[test]
fn rejects_self_declared_package_dependency() {
    let root = temp_project_dir("self_declared_package_dependency");
    write_project_file(
        &root,
        "Game.vproject",
        r#"
package = Game
dependencyPackages = Game
entry = main.verse
"#,
    );

    let manifest = root.join("Game.vproject");
    let error = SourceProject::from_manifest(&manifest).expect_err("manifest should fail");
    assert!(
        error
            .to_string()
            .contains("package `Game` cannot depend on itself")
    );
}

#[test]
fn evaluates_cross_file_scoped_members_inside_matching_package() {
    let root = temp_project_dir("cross_file_scoped_members_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{DemoPackage}>:int = 20
    score_map<scoped{DemoPackage}> := [string]int
    box<scoped{DemoPackage}>(t:type) := class:
        Value<public>:t
    Double<scoped{DemoPackage}>(Value:int)<computes>:int = Value * 2
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Scores:score_map = map{"ada" => 1}
Item:box(int) = box(int){Value := Double(11)}
Answer + Item.Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_named_scoped_access_level_inside_matching_package() {
    let root = temp_project_dir("named_scoped_access_level_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<demo_scope>:int = 18
    score_map<demo_scope> := [string]int
    box<demo_scope>(t:type) := class:
        Value<public>:t
    Double<demo_scope>(Value:int)<computes>:int = Value * 2
    (Value:int).Bump<demo_scope>():int = Value + 1

    item<public> := class:
        Value<demo_scope>:int = 5
        var<demo_scope> Count<public>:int = 0
        Score<demo_scope>():int = Value + 1

    readable<public> := interface:
        Seen<demo_scope>:int

    box_item<public> := class(readable):
        Seen<override><demo_scope>:int = 4

    demo_scope := scoped{DemoPackage}
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Scores:score_map = map{"ada" => 1}
Boxed:box(int) = box(int){Value := Double(2)}
Thing:item = item{}
set Thing.Count = 1
ReadSeen(Target:readable):int = Target.Seen
Answer + Double(3) + 1.Bump() + Thing.Value + Thing.Score() + Thing.Count + ReadSeen(box_item{})
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_named_scoped_access_level_outside_package_scope() {
    let root = temp_project_dir("named_scoped_access_level_outside_package");
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    demo_scope := scoped{DemoPackage}
    Answer<demo_scope>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn rejects_cross_file_scoped_member_without_package_scope() {
    let root = temp_project_dir("cross_file_scoped_member_no_package");
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{DemoPackage}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn rejects_cross_file_scoped_member_from_other_package() {
    let root = temp_project_dir("cross_file_scoped_member_other_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{DemoPackage}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn evaluates_cross_file_scoped_member_inside_package_path_scope() {
    let root = temp_project_dir("cross_file_scoped_member_package_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /Demo/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{/Demo}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_scoped_member_inside_absolute_module_path_scope() {
    let root = temp_project_dir("cross_file_scoped_member_absolute_module_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /Demo/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{/Demo/Package/Friend}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Friend<public> := module:
    Child<public> := module:
        Read<public>():int = Api.Answer

Friend.Child.Read()
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_scoped_member_from_unlisted_absolute_module_path_scope() {
    let root = temp_project_dir("cross_file_scoped_member_unlisted_absolute_module_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /Demo/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{/Demo/Package/Friend}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Other<public> := module:
    Child<public> := module:
        Read<public>():int = Api.Answer

Other.Child.Read()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn rejects_cross_file_scoped_member_from_unlisted_package_path_scope() {
    let root = temp_project_dir("cross_file_scoped_member_other_package_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /DemoOther/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    Answer<scoped{/Demo}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn evaluates_cross_file_scoped_extension_method_inside_matching_package() {
    let root = temp_project_dir("cross_file_scoped_extension_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Bump<scoped{DemoPackage}>():int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
20.Bump() + 20.(Ops:)Bump()
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_scoped_extension_method_without_package_scope() {
    let root = temp_project_dir("cross_file_scoped_extension_no_package");
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Bump<scoped{DemoPackage}>():int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
41.Bump()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Bump` is scoped to `Ops`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Bump` is scoped to `Ops`")
    );
}

#[test]
fn rejects_cross_file_scoped_extension_method_from_other_package() {
    let root = temp_project_dir("cross_file_scoped_extension_other_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Bump<scoped{DemoPackage}>():int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
41.Bump()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Bump` is scoped to `Ops`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Bump` is scoped to `Ops`")
    );
}

#[test]
fn evaluates_cross_file_scoped_class_members_inside_matching_package() {
    let root = temp_project_dir("cross_file_scoped_class_members_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    item<public> := class:
        Value<scoped{DemoPackage}>:int = 20
        var<scoped{DemoPackage}> Count<public>:int = 0
        Score<scoped{DemoPackage}>():int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Item:item = item{}
set Item.Count = 1
Item.Value + Item.Score() + Item.Count
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_scoped_class_member_without_package_scope() {
    let root = temp_project_dir("cross_file_scoped_class_member_no_package");
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    item<public> := class:
        Value<scoped{DemoPackage}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Item:item = item{}
Item.Value
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("field `Value` is scoped to class `Api.item`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("field `Value` is scoped to class `Api.item`")
    );
}

#[test]
fn rejects_cross_file_scoped_class_method_from_other_package() {
    let root = temp_project_dir("cross_file_scoped_class_method_other_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    item<public> := class:
        Score<scoped{DemoPackage}>():int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
item{}.Score()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("method `Score` is scoped to class `Api.item`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("method `Score` is scoped to class `Api.item`")
    );
}

#[test]
fn rejects_cross_file_scoped_class_var_assignment_without_package_scope() {
    let root = temp_project_dir("cross_file_scoped_class_var_assignment_no_package");
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    item<public> := class:
        var<scoped{DemoPackage}> Count<public>:int = 0
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Item:item = item{}
set Item.Count = 1
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("field `Count` is scoped to class `Api.item`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("field `Count` is scoped to class `Api.item`")
    );
}

#[test]
fn evaluates_cross_file_scoped_interface_field_inside_matching_package() {
    let root = temp_project_dir("cross_file_scoped_interface_field_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    readable<public> := interface:
        Value<scoped{DemoPackage}>:int

    item<public> := class(readable):
        Value<override><scoped{DemoPackage}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Target:readable = item{}
Target.Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_scoped_interface_field_without_package_scope() {
    let root = temp_project_dir("cross_file_scoped_interface_field_no_package");
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    readable<public> := interface:
        Value<scoped{DemoPackage}>:int

    item<public> := class(readable):
        Value<override><scoped{DemoPackage}>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Target:readable = item{}
Target.Value
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("field `Value` is scoped to interface `Api.readable`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("field `Value` is scoped to interface `Api.readable`")
    );
}

#[test]
fn evaluates_scoped_members_inside_descendant_module_scope() {
    let source = r#"
Api<public> := module:
    Answer<scoped{Friend}>:int = 20
    Double<scoped{Friend}>(Value:int)<computes>:int = Value * 2
    (Value:int).Bump<scoped{Friend}>():int = Value + 1

    item<public> := class:
        Value<scoped{Friend}>:int = 5
        Score<scoped{Friend}>():int = Value + 1

    readable<public> := interface:
        Seen<scoped{Friend}>:int

    box<public> := class(readable):
        Seen<override><scoped{Friend}>:int = 4

Friend<public> := module:
    Child<public> := module:
        using { Api }

        ReadSeen<public>(Target:readable):int = Target.Seen

        Read<public>():int =
            Answer + Double(2) + 1.Bump() + item{}.Value + item{}.Score() + ReadSeen(box{}) + 1

Friend.Child.Read()
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_scoped_member_from_unlisted_descendant_module_scope() {
    let source = r#"
Api<public> := module:
    Answer<scoped{Friend}>:int = 42

Other<public> := module:
    Child<public> := module:
        Read<public>():int = Api.Answer

Other.Child.Read()
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_source(source).expect_err("source run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn evaluates_internal_members_inside_descendant_module_scope() {
    let source = r#"
DataTypes<public> := module:
    Hidden:int = 20

    tile_coordinate<public> := class<concrete>:
        Left:int = 20

    Child<public> := module:
        Read<public>():int = DataTypes.Hidden + DataTypes.tile_coordinate{}.Left + 2

DataTypes.Child.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parent_scoped_module_internal_members_inside_matching_package() {
    let root = temp_project_dir("parent_scoped_module_internal_members");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer:int = 20
    Double(Value:int)<computes>:int = Value * 2

    item<public> := class:
        Value:int = 5
        Score():int = Value + 1

    readable<public> := interface:
        Seen:int

    box<public> := class(readable):
        Seen<override>:int = 4
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
ReadSeen(Target:readable):int = Target.Seen
Answer + Double(2) + item{}.Value + item{}.Score() + ReadSeen(box{}) + 3
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_parent_scoped_module_public_member_inside_matching_package() {
    let root = temp_project_dir("parent_scoped_module_public_member");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_parent_scoped_module_internal_member_from_unlisted_package() {
    let root = temp_project_dir("parent_scoped_module_internal_member_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn rejects_parent_scoped_module_public_member_from_unlisted_package() {
    let root = temp_project_dir("parent_scoped_module_public_member_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Answer` is scoped to `Api`")
    );
}

#[test]
fn evaluates_using_scoped_module_inside_matching_package() {
    let root = temp_project_dir("using_scoped_module_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn checks_using_scoped_module_inside_absolute_module_path_scope() {
    let root = temp_project_dir("using_scoped_module_absolute_module_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /Demo/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{/Demo/Package/Friend}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Friend<public> := module:
    Child<public> := module:
        using { Api }
        Read<public>():int = Answer

Friend.Child.Read()
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
}

#[test]
fn rejects_using_scoped_module_from_unlisted_package() {
    let root = temp_project_dir("using_scoped_module_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{DemoPackage}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("module `Api` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("module `Api` is scoped to `Api`")
    );
}

#[test]
fn rejects_using_scoped_module_from_unlisted_absolute_module_path_scope() {
    let root = temp_project_dir("using_scoped_module_unlisted_absolute_module_path_scope");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = /Demo/Package
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<scoped{/Demo/Package/Friend}> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Other<public> := module:
    Child<public> := module:
        using { Api }
        Read<public>():int = 0

Other.Child.Read()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("module `Api` is scoped to `Api`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("module `Api` is scoped to `Api`")
    );
}

#[test]
fn rejects_using_public_child_module_through_scoped_parent_from_unlisted_package() {
    let root = temp_project_dir("using_public_child_module_scoped_parent_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Outer<scoped{DemoPackage}> := module:
    Inner<public> := module:
        Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Outer.Inner }
0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("module `Outer.Inner` is scoped to `Outer`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("module `Outer.Inner` is scoped to `Outer`")
    );
}

#[test]
fn evaluates_internal_child_module_public_member_inside_parent_module_scope() {
    let source = r#"
Outer<public> := module:
    Inner := module:
        Answer<public>:int = 42
    Read<public>():int = Inner.Answer

Outer.Read()
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_internal_ancestor_module_public_member_outside_parent_module_scope() {
    let error = check_source(
        r#"
Outer<public> := module:
    Inner := module:
        Answer<public>:int = 42

Outer.Inner.Answer
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `Inner` is internal to module `Outer`"),
        "{error}"
    );
}

#[test]
fn rejects_using_internal_child_module_outside_parent_module_scope() {
    let error = check_source(
        r#"
Outer<public> := module:
    Inner := module:
        Answer<public>:int = 42

using { Outer.Inner }
0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("module `Outer.Inner` is internal to module `Outer`"),
        "{error}"
    );
}

#[test]
fn rejects_scoped_ancestor_module_public_member_from_unlisted_module_scope() {
    let root = temp_project_dir("scoped_ancestor_module_public_member_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Outer.verse",
        r#"
Outer<scoped{DemoPackage}> := module {}
"#,
    );
    write_project_file(
        &root,
        "Outer\\Inner\\defs.verse",
        r#"
Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Outer.Inner.Answer
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Inner` is scoped to `Outer`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Inner` is scoped to `Outer`")
    );
}

#[test]
fn evaluates_parent_scoped_class_internal_members_inside_descendant_module_scope() {
    let source = r#"
Api<public> := module:
    item<scoped{Friend}> := class<concrete>:
        Value:int = 20
        Score():int = 22

Friend<public> := module:
    Child<public> := module:
        Read<public>():int = Api.item{}.Value + Api.item{}.Score()

Friend.Child.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_scoped_class_constructor_inside_matching_package() {
    let root = temp_project_dir("scoped_class_constructor_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    token<public> := class<scoped{DemoPackage}><concrete>:
        Value<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.token{}.Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_scoped_class_constructor_from_unlisted_package() {
    let root = temp_project_dir("scoped_class_constructor_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    token<public> := class<scoped{DemoPackage}><concrete>:
        Value<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.token{}.Value
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("class constructor `Api.token` is scoped")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("class constructor `Api.token` is scoped")
    );
}

#[test]
fn evaluates_named_scoped_class_constructor_inside_matching_package() {
    let root = temp_project_dir("named_scoped_class_constructor_matching_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = DemoPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    demo_scope := scoped{DemoPackage}
    token<public> := class<demo_scope><concrete>:
        Value<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.token{}.Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_named_scoped_class_constructor_from_unlisted_package() {
    let root = temp_project_dir("named_scoped_class_constructor_unlisted_package");
    write_project_file(
        &root,
        "Demo.vproject",
        r#"
package = OtherPackage
entry = main.verse
"#,
    );
    write_project_file(
        &root,
        "Api.verse",
        r#"
Api<public> := module:
    demo_scope := scoped{DemoPackage}
    token<public> := class<demo_scope><concrete>:
        Value<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Api.token{}.Value
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("class constructor `Api.token` is scoped")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("class constructor `Api.token` is scoped")
    );
}

#[test]
fn generates_external_digest_that_project_loader_can_consume() {
    let implementation = r#"
Api<public> := module:
    Hidden:int = 7
    Answer<public>:int = 42
    Double<public>(Value:int)<computes>:int = Value * 2
"#;

    let digest = generate_digest(implementation).expect("digest should generate");
    assert!(digest.contains("Api<public> := module:"));
    assert!(digest.contains("Answer<public>:int = external {}"));
    assert!(digest.contains("Double<public>(Value:int)<computes>:int = external {}"));
    assert!(!digest.contains("Hidden"));
    check_source(&digest).expect("digest should be valid source");

    let root = temp_project_dir("external_digest_consumer");
    write_project_file(&root, "Api.verse", &digest);
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Api }
Result:int = Api.Double(21)
Api.Answer
"#,
    );
    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("digest-backed project should check"),
        Type::Int
    );

    let project_digest = generate_project_digest(root.join("Api.verse"))
        .expect("project digest should generate from project file");
    assert!(project_digest.contains("Double<public>(Value:int)<computes>:int = external {}"));
}

#[test]
fn evaluates_cross_file_folder_module_import_for_types_and_values() {
    let root = temp_project_dir("folder_module");
    write_project_file(
        &root,
        "DataTypes\\defs.verse",
        r#"
tile_coordinate := class<public><concrete>:
    Left<public>:int = 0
    Forward<public>:int = 0

Origin<public>:tile_coordinate = tile_coordinate{Left := 40, Forward := 2}
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
Location:tile_coordinate = Origin
Location.Left + Location.Forward
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_folder_module_with_explicit_public_descriptor() {
    let root = temp_project_dir("folder_module_public_descriptor");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module {}
"#,
    );
    write_project_file(
        &root,
        "DataTypes\\defs.verse",
        r#"
Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
DataTypes.Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_implicit_sibling_module_reference() {
    let root = temp_project_dir("implicit_sibling_module");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
DataTypes.Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_implicit_folder_module_reference() {
    let root = temp_project_dir("implicit_folder_module");
    write_project_file(
        &root,
        "DataTypes\\defs.verse",
        r#"
Answer<public>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
DataTypes.Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn ignores_cross_file_implicit_non_module_script() {
    let root = temp_project_dir("implicit_non_module_script");
    write_project_file(
        &root,
        "Scratch.verse",
        r#"
Err("should not load")
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
42
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_implicit_sibling_source_declarations() {
    let root = temp_project_dir("implicit_sibling_source");
    write_project_file(
        &root,
        "Helpers.verse",
        r#"
Double(Value:int)<computes>:int = Value * 2
Offset:int = 2
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Double(20) + Offset
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_implicit_sibling_source_types() {
    let root = temp_project_dir("implicit_sibling_source_types");
    write_project_file(
        &root,
        "Types.verse",
        r#"
score := int

counter := class<concrete>:
    Value:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Value:score = counter{}.Value
Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_implicit_sibling_source_extension_method() {
    let root = temp_project_dir("implicit_sibling_source_extension");
    write_project_file(
        &root,
        "Ops.verse",
        r#"
(Value:int).Bump()<computes>:int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
41.Bump()
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn ignores_cross_file_implicit_sibling_source_script_expressions() {
    let root = temp_project_dir("implicit_sibling_source_script_expression");
    write_project_file(
        &root,
        "Library.verse",
        r#"
Answer:int = 42
"#,
    );
    write_project_file(
        &root,
        "Scratch.verse",
        r#"
Err("should not load")
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
Answer
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_imported_extension_method() {
    let root = temp_project_dir("extension_module");
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Squared<public>()<computes>:int = Value * Value
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
6.Squared() + 6
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_public_signature_exposing_internal_module_type() {
    let root = temp_project_dir("cross_file_public_signature_internal_type");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    secret := class:
        Value:int = 0
    Reveal<public>(Item:secret):int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("definition `DataTypes.Reveal` is public but depends on `DataTypes.secret`, which is internal"),
        "{error}"
    );
}

#[test]
fn rejects_cross_file_imported_transacts_call_from_computes_function() {
    let root = temp_project_dir("cross_file_transacts_effect");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Update<public>()<transacts>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
Use()<computes>:int = Effects.Update()
Use()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_cross_file_imported_no_rollback_call_in_failure_context() {
    let root = temp_project_dir("cross_file_no_rollback_effect");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Read<public>():int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Read()
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn evaluates_cross_file_imported_decides_computes_call_in_failure_context() {
    let root = temp_project_dir("cross_file_decides_computes_effect");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Pick<public>(Value:int)<decides><computes>:int = Value
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Pick[42]
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn evaluates_cross_file_imported_decides_function_type_assignment() {
    let root = temp_project_dir("cross_file_decides_function_type");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Pick<public>(Value:int)<decides><computes>:int = Value
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
Handler:type{_(:int)<decides><computes>:int} = Effects.Pick
if (Value := Handler[42]). Value else. 0
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_imported_writes_call_in_failure_context() {
    let root = temp_project_dir("cross_file_writes_failure_context");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Write<public>()<writes>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Write()
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_cross_file_imported_decides_writes_call_in_failure_context() {
    let root = temp_project_dir("cross_file_decides_writes_failure_context");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Pick<public>(Value:int)<decides><writes>:int = Value
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Pick[42]
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<writes>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_cross_file_imported_decides_allocates_call_in_failure_context() {
    let root = temp_project_dir("cross_file_decides_allocates_failure_context");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Pick<public>(Value:int)<decides><allocates>:int = Value
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Pick[42]
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_cross_file_imported_allocates_call_in_failure_context() {
    let root = temp_project_dir("cross_file_allocates_failure_context");
    write_project_file(
        &root,
        "Effects.verse",
        r#"
Effects<public> := module:
    Allocate<public>()<allocates>:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Effects }
if:
    Value := Effects.Allocate()
    Value > 0
then:
    Value
else:
    0
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<allocates>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_cross_file_imported_extension_effect_mismatch() {
    let root = temp_project_dir("cross_file_extension_effect");
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Bump<public>()<transacts>:int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
Use()<computes>:int = 41.Bump()
Use()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_cross_file_using_internal_value_outside_module() {
    let root = temp_project_dir("cross_file_using_internal_value");
    write_project_file(
        &root,
        "Secrets.verse",
        r#"
Secrets<public> := module:
    Hidden:int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Secrets }
Hidden
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Secrets`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Secrets`")
    );
}

#[test]
fn rejects_cross_file_using_internal_function_outside_module() {
    let root = temp_project_dir("cross_file_using_internal_function");
    write_project_file(
        &root,
        "Secrets.verse",
        r#"
Secrets<public> := module:
    Hidden():int = 42
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Secrets }
Hidden()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Secrets`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Secrets`")
    );
}

#[test]
fn rejects_cross_file_using_internal_extension_method_outside_module() {
    let root = temp_project_dir("cross_file_using_internal_extension");
    write_project_file(
        &root,
        "Ops.verse",
        r#"
Ops<public> := module:
    (Value:int).Hidden():int = Value + 1
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { Ops }
41.Hidden()
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Ops`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `Ops`")
    );
}

#[test]
fn evaluates_cross_file_using_public_type_alias_outside_module() {
    let root = temp_project_dir("cross_file_using_public_type_alias");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    score_map<public> := [string]int
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
Scores:score_map = map{"ada" => 42}
Scores.Length + 41
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_using_internal_type_alias_outside_module() {
    let root = temp_project_dir("cross_file_using_internal_type_alias");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    score_map := [string]int
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
Scores:score_map = map{}
Scores.Length
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `score_map` is internal to module `DataTypes`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `score_map` is internal to module `DataTypes`")
    );
}

#[test]
fn evaluates_cross_file_using_public_parametric_type_outside_module() {
    let root = temp_project_dir("cross_file_using_public_parametric_type");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    box<public>(t:type) := class:
        Value<public>:t
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
Item:box(int) = box(int){Value := 42}
Item.Value
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file(&entry).expect("project should check"),
        Type::Int
    );
    assert_eq!(
        run_project_file(&entry).expect("project should run"),
        Value::Int(42)
    );
}

#[test]
fn rejects_cross_file_using_internal_parametric_type_outside_module() {
    let root = temp_project_dir("cross_file_using_internal_parametric_type");
    write_project_file(
        &root,
        "DataTypes.verse",
        r#"
DataTypes<public> := module:
    box(t:type) := class:
        Value:t
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
using { DataTypes }
Item:box(int) = external {}
"#,
    );

    let entry = root.join("main.verse");
    let error = check_project_file(&entry).expect_err("project should fail");
    assert!(
        error
            .to_string()
            .contains("member `box` is internal to module `DataTypes`")
    );

    let error = run_project_file(&entry).expect_err("project run should fail");
    assert!(
        error
            .to_string()
            .contains("member `box` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_default_internal_module_using_type_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    tile_coordinate := class<concrete>:
        Left<public>:int = 0

using { DataTypes }
Location:tile_coordinate = tile_coordinate{Left := 42}
Location.Left
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `tile_coordinate` is internal to module `DataTypes`")
    );
}

#[test]
fn evaluates_module_internal_type_alias_inside_same_module() {
    let source = r#"
DataTypes<public> := module:
    score_map := [string]int
    Total<public>(Scores:score_map):int = Scores.Length

DataTypes.Total(map{"ada" => 42})
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_public_module_type_alias_outside_module() {
    let source = r#"
DataTypes<public> := module:
    score_map<public> := [string]int

using { DataTypes }
Scores:score_map = map{"ada" => 40, "grace" => 2}
Qualified:DataTypes.score_map = Scores
Qualified.Length + 40
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_default_internal_module_using_type_alias_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    score_map := [string]int

using { DataTypes }
Scores:score_map = map{}
Scores.Length
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `score_map` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_default_internal_module_qualified_type_alias_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    score_map := [string]int

Scores:DataTypes.score_map = map{}
Scores.Length
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `score_map` is internal to module `DataTypes`")
    );
}

#[test]
fn rejects_type_alias_target_importing_internal_module_alias() {
    let error = check_source(
        r#"
Source<public> := module:
    score_map := [string]int

Target<public> := module:
    using { Source }
    local_scores := []score_map
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(
        message.contains("member `score_map` is internal to module `Source`"),
        "{message}"
    );
}

#[test]
fn evaluates_module_internal_using_for_function_signatures() {
    let source = r#"
DataTypes<public> := module:
    tile_coordinate<public> := class<concrete>:
        Left<public>:int = 0
        Forward<public>:int = 0

UtilityFunctions<public> := module:
    using { DataTypes }
    Sum<public>(Tile:tile_coordinate):int = Tile.Left + Tile.Forward

UtilityFunctions.Sum(DataTypes.tile_coordinate{Left := 40, Forward := 2})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_empty_module_declarations() {
    let source = r#"
MiniGame := module:
    MiniGameAssets<public> := module:
        Watermelon<public> := module:
            Meshes<public> := module {}

MiniGame.MiniGameAssets.Watermelon.Meshes
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Module("MiniGame.MiniGameAssets.Watermelon.Meshes".to_string())
    );
}

#[test]
fn rejects_unknown_module_member() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    Answer<public>:int = 42

DataTypes.Missing
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("module `DataTypes` has no member `Missing`")
    );
}

#[test]
fn rejects_unknown_local_using_module() {
    let error = check_source("using { MissingModule }").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported module path `MissingModule`")
    );
}

#[test]
fn rejects_local_module_definition() {
    let error = check_source(
        r#"
block:
    Local := module:
        Answer:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("module definitions are only supported at module level")
    );
}

#[test]
fn evaluates_imported_module_extension_method() {
    let source = r#"
DataTypes<public> := module:
    point<public> := class<concrete>:
        X<public>:int = 0

    (Point:point).Sum<public>():int = Point.X + 2

using { DataTypes }
point{X := 40}.Sum()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_imported_module_extension_method() {
    let source = r#"
DataTypes<public> := module:
    point<public> := class<concrete>:
        X<public>:int = 0

    (Point:point).Sum<public>():int = Point.X + 2

using { DataTypes }
point{X := 40}.(DataTypes:)Sum()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_nested_module_extension_method() {
    let source = r#"
Library<public> := module:
    Numbers<public> := module:
        (Value:int).Bump<public>():int = Value + 1

using { Library.Numbers }
40.(Library.Numbers:)Bump() + 0.(Numbers:)Bump()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_qualified_internal_module_extension_method_outside_module() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    (Value:int).Hidden():int = Value + 1

using { DataTypes }
41.(DataTypes:)Hidden()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("member `Hidden` is internal to module `DataTypes`")
    );
}

#[test]
fn evaluates_default_parameter_using_earlier_parameter() {
    let source = r#"
Offset(Value:int, ?Amount:int = Value + 1):int = Value + Amount
Offset(10) + Offset(10, ?Amount := 5)
"#;

    assert_eq!(eval(source), Value::Int(36));
}

#[test]
fn evaluates_destructured_tuple_default_subparameter_using_previous_item() {
    let source = r#"
Bonus((Base:int, ?Amount:int = Base + 2)):int = Amount
Bonus(40)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_class_method_archetype_of_defining_class() {
    let source = r#"
DataTypes<public> := module:
    counter<public> := class<concrete>:
        Value<private>:int = 0

        WithValue<public>(NewValue:int):counter =
            counter{Value := NewValue}

        Reveal<public>():int = Value

DataTypes.counter{}.WithValue(42).Reveal()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_module_constructor_function_before_class_definition() {
    let source = r#"
DataTypes<public> := module:
    MakePlayer<constructor><public>(Name:string):player =
        player:
            Name := Name

    player<public> := class:
        Name<public>:string

Hero := DataTypes.MakePlayer("Ava")
Hero.Name
"#;

    assert_eq!(eval(source), Value::String("Ava".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}
