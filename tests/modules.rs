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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
