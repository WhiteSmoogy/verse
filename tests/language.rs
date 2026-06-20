use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use verse_rs::{
    Interpreter, Type, Value, check_project_file, check_source, parse_source, run_project_file,
};

fn eval(source: &str) -> Value {
    let mut interpreter = Interpreter::new();
    interpreter.eval_source(source).expect("source should run")
}

fn assert_failable_context_error(source: &str) {
    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
}

fn function_shape(value_type: Type) -> (Option<usize>, Vec<String>, Option<Vec<Type>>, Type) {
    let Type::Function {
        arity,
        effects,
        param_types,
        return_type,
        ..
    } = value_type
    else {
        panic!("expected function type, got {value_type:?}");
    };
    (arity, effects, param_types, *return_type)
}

fn temp_project_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("verse_rs_{name}_{nonce}"));
    fs::create_dir_all(&dir).expect("temp project directory should be created");
    dir
}

fn write_project_file(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("project subdirectory should be created");
    }
    fs::write(path, source).expect("project file should be written");
}

#[test]
fn evaluates_arithmetic_with_precedence() {
    assert_eq!(eval("1 + 2 * 3"), Value::Number(7.0));
}

#[test]
fn evaluates_hexadecimal_integer_literals() {
    let source = "0x7F + 0xFACE";

    assert_eq!(eval(source), Value::Number(64333.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_signed_64_bit_integer_literal_bounds() {
    assert_eq!(eval("9223372036854775807"), Value::Int(i64::MAX));
    assert_eq!(eval("-9223372036854775808"), Value::Int(i64::MIN));
    assert_eq!(eval("-0x8000000000000000"), Value::Int(i64::MIN));
}

#[test]
fn rejects_integer_literal_above_signed_64_bit_range() {
    let error = parse_source("9223372036854775808").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_integer_literal_below_signed_64_bit_range() {
    let error = parse_source("-9223372036854775809").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_hex_integer_literal_above_signed_64_bit_range() {
    let error = parse_source("0x8000000000000001").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_empty_hexadecimal_integer_literal() {
    let error = parse_source("0x").expect_err("source should fail");

    assert!(error.to_string().contains("expected hexadecimal digits"));
}

#[test]
fn evaluates_inline_block_comments() {
    let source = "1<# inline comment #> + 2";

    assert_eq!(eval(source), Value::Number(3.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_multiline_block_comments() {
    let source = r#"
Value := 40
<# outer
    <# nested #>
#>
Value + 2
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_indented_comments() {
    let source = r#"
<#>
    This line is a Verse indented comment.
    This one is also ignored.
40 + 2
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unterminated_block_comments() {
    let error = parse_source("<# missing").expect_err("source should fail");

    assert!(error.to_string().contains("unterminated block comment"));
}

#[test]
fn rejects_legacy_slash_line_comments() {
    let error = parse_source("1 // not Verse").expect_err("source should fail");

    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn evaluates_block_comments_inside_string_literals() {
    let source = r#""abc<#comment#>def""#;

    assert_eq!(eval(source), Value::String("abcdef".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_nested_block_comments_inside_string_literals() {
    let source = r#""a<# outer <# inner #> still outer #>b""#;

    assert_eq!(eval(source), Value::String("ab".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_inside_interpolated_string_text() {
    let source = r#"
Value:int = 42
"a<#left#>{Value}<#right#>b"
"#;

    assert_eq!(eval(source), Value::String("a42b".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_with_braces_inside_string_interpolation() {
    let source = r#""{40 <# } ignored by comment #> + 2}""#;

    assert_eq!(eval(source), Value::String("42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_with_quotes_inside_nested_interpolation_strings() {
    let source = r#""{"a<# " ignored by comment #>b"}""#;

    assert_eq!(eval(source), Value::String("ab".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_unterminated_block_comments_inside_string_literals() {
    let error = parse_source(r#""abc<# missing""#).expect_err("source should fail");

    assert!(error.to_string().contains("unterminated block comment"));
}

#[test]
fn rejects_reserved_underscore_binding_name() {
    let error = parse_source("_ := 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn rejects_reserved_underscore_variable_name() {
    let error = parse_source("var _:int = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn rejects_reserved_underscore_parameter_name() {
    let error = parse_source("_Bad(_:int):int = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn evaluates_official_using_statement() {
    let source = r#"
using { /Verse.org/Verse }
Abs(-42)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Number(42.0));
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
        Value::Number(42.0)
    );
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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
        Value::Number(42.0)
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_extension_method_on_class_instance() {
    let source = r#"
counter := class:
    Value:int = 0

(Counter:counter).Double<public>():int = Counter.Value * 2

counter{Value := 21}.Double()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_extension_method_with_parameters_and_named_argument() {
    let source = r#"
marker := class:
    Base:int = 0

(Marker:marker).MoveMarker<public>(Offset:int, ?Scale:int = 1):int =
    Marker.Base + Offset * Scale

marker{Base := 2}.MoveMarker(20, ?Scale := 2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_extension_method_on_float_receiver() {
    let source = r#"
(X:float).AddOne<public>():float = X + 1.0

Value:float = 41.0
Value.AddOne()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_scope_extension_method_accessing_field() {
    let source = r#"
game_manager := class:
    Multiplier:int = 10

    (Score:int).ScaledScore()<computes>:int =
        Score * Multiplier

    ProcessScore(Value:int)<computes>:int =
        Value.ScaledScore()

GM := game_manager{}
GM.ProcessScore(5)
"#;

    assert_eq!(eval(source), Value::Number(50.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_scope_extension_method_accessing_self() {
    let source = r#"
counter := class:
    Base:int = 3

    (Value:int).PlusBase()<computes>:int =
        Value + Self.Base

    Use():int =
        39.PlusBase()

counter{}.Use()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_scope_extension_method_in_class_block() {
    let source = r#"
manager := class:
    var Score:int = 0

    (Value:int).Scaled()<computes>:int =
        Value * 2

    block:
        set Score = 21.Scaled()

manager{}.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_scope_extension_method_before_outer_extension() {
    let source = r#"
(Value:int).Scoped():int =
    Value + 100

manager := class:
    (Value:int).Scoped():int =
        Value + 1

    Use():int =
        41.Scoped()

manager{}.Use()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_class_scope_extension_method_outside_class_scope() {
    let error = check_source(
        r#"
manager := class:
    (Value:int).Scoped():int =
        Value + 1

    Use():int =
        41.Scoped()

41.Scoped()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unknown member `Scoped` on type `int`")
    );
}

#[test]
fn evaluates_decides_extension_method_bracket_call() {
    let source = r#"
token := class:
    Value:int = 0

(Token:token).Pick()<decides><transacts>:int = Token.Value

if (Value := token{Value := 42}.Pick[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_plain_call_to_extension_method_name() {
    let error = check_source(
        r#"
(X:int).Double():int = X * 2

Double(21)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Double`"));
}

#[test]
fn rejects_extension_method_reference_without_call() {
    let error = check_source(
        r#"
(X:int).Double():int = X * 2

Ref := 21.Double
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("extension method `Double` must be called")
    );
}

#[test]
fn rejects_local_extension_method_definition() {
    let error = check_source(
        r#"
block:
    (X:int).Double():int = X * 2
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("extension methods are only supported at module level")
    );
}

#[test]
fn rejects_extension_method_receiver_mismatch() {
    let error = check_source(
        r#"
(X:string).LenPlus():int = 0

(42).LenPlus()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unknown member `LenPlus` on type `int`")
    );
}

#[test]
fn rejects_extension_method_conflicting_with_class_method() {
    let error = check_source(
        r#"
player := class:
    Health():int = 100

(Player:player).Health():int = 50
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("extension method `Health` conflicts with class `player` method `Health`")
    );
}

#[test]
fn rejects_extension_method_conflicting_with_interface_method() {
    let error = check_source(
        r#"
rideable := interface:
    Mount():int

(Rideable:rideable).Mount():int = 50
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "extension method `Mount` conflicts with interface `rideable` method `Mount`"
        )
    );
}

#[test]
fn rejects_duplicate_extension_method_for_same_receiver_type() {
    let error = check_source(
        r#"
(X:int).Double():int = X * 2
(Y:int).Double():int = Y * 3
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate extension method `Double` for receiver type `int`")
    );
}

#[test]
fn evaluates_interface_assignment_and_method_call() {
    let source = r#"
rideable := interface():
    Mount():int

bicycle := class(rideable):
    Mount<override>():int = 42

Ride:rideable = bicycle{}
Ride.Mount()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_inheritance_requirements() {
    let source = r#"
moveable := interface():
    MoveForward():int

rideable := interface(moveable):
    Mount():int

horse := class(rideable):
    MoveForward<override>():int = 40
    Mount<override>():int = 2

Ride:rideable = horse{}
Ride.MoveForward() + Ride.Mount()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_base_and_interface_parent_list() {
    let source = r#"
moveable := interface():
    MoveForward():int

rideable := interface(moveable):
    Mount():int

horse := class:
    MoveForward():int = 40

saddle_horse := class(horse, rideable):
    Mount<override>():int = 2

Ride:rideable = saddle_horse{}
Ride.MoveForward() + Ride.Mount()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_empty_interface_declaration() {
    let source = r#"
taggable := interface

tagged := class(taggable):
    Score:int = 42

Value:taggable = tagged{}
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_class_missing_interface_method() {
    let error = check_source(
        r#"
rideable := interface():
    Mount():int

bicycle := class(rideable):
    Speed:int = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `bicycle` must be `abstract` or implement method `Mount`")
    );
}

#[test]
fn rejects_interface_method_implementation_without_override() {
    let error = check_source(
        r#"
rideable := interface():
    Mount():int

bicycle := class(rideable):
    Mount():int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate inherited class method `Mount`")
    );
}

#[test]
fn evaluates_interface_fields_and_default_method() {
    let source = r#"
triggerable := interface:
    var<protected> Triggered<public>:logic = false
    PerformAction():void
    Trigger()<transacts>:int =
        if (Triggered?):
            0
        else:
            PerformAction()
            set Triggered = true
            1

button := class(triggerable):
    PerformAction<override>():void = {}

Target:triggerable = button{}
Target.Trigger() + Target.Trigger()
"#;

    assert_eq!(eval(source), Value::Number(1.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_default_method_uses_implementer_override_dispatch() {
    let source = r#"
scorable := interface:
    Score():int
    DoubledScore():int =
        2 * Score()

player_score := class(scorable):
    Score<override>():int =
        21

Value:scorable = player_score{}
Value.DoubledScore()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_required_field_on_class_archetype() {
    let source = r#"
labeled := interface:
    Label:string

label_box := class(labeled):
    Score:int = 42

Box:labeled = label_box{Label := "ready"}
Box.Label
"#;

    assert_eq!(eval(source), Value::String("ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_builtin_cancelable_interface_implementation() {
    let source = r#"
task := class(cancelable):
    var Canceled<private>:logic = false
    Cancel<override>()<transacts>:void =
        set Canceled = true
    WasCanceled()<computes>:logic = Canceled

Concrete := task{}
Task:cancelable = Concrete
Task.Cancel()
if (Concrete.WasCanceled() = true):
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
fn evaluates_builtin_enableable_decides_interface_method() {
    let source = r#"
toggle := class(enableable):
    var Enabled<private>:logic = false
    Enable<override>()<transacts>:void =
        set Enabled = true
    Disable<override>()<transacts>:void =
        set Enabled = false
    IsEnabled<override>()<decides><transacts>:void =
        Enabled?
        {}

Widget := toggle{}
Widget.Enable()
if (Widget.IsEnabled[]):
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
fn evaluates_builtin_invalidatable_inherits_disposable_interface() {
    let source = r#"
handle := class(invalidatable):
    var Valid<private>:logic = true
    Dispose<override>()<transacts>:void =
        set Valid = false
    IsValid<override>()<decides><transacts>:void =
        Valid?
        {}

Handle := handle{}
Disposable:disposable = Handle
Disposable.Dispose()
if (Handle.IsValid[]):
    0
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_builtin_showable_interface_field() {
    let source = r#"
panel := class(showable):
    var Show<override>:?logic = false

Panel := panel{}
Showable:showable = Panel
set Showable.Show = option{true}
if (Panel.Show?):
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
fn rejects_builtin_enableable_missing_decides_method() {
    let error = check_source(
        r#"
toggle := class(enableable):
    Enable<override>()<transacts>:void = {}
    Disable<override>()<transacts>:void = {}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `toggle` must be `abstract` or implement method `IsEnabled`")
    );
}

#[test]
fn rejects_builtin_showable_field_type_mismatch() {
    let error = check_source(
        r#"
panel := class(showable):
    var Show<override>:logic = true
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Show` overrides `?bool` but has incompatible type `bool`")
    );
}

#[test]
fn rejects_interface_block_clause() {
    let error = parse_source(
        r#"
bad := interface:
    block:
        1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("interface definitions cannot contain `block`")
    );
}

#[test]
fn rejects_interface_construction() {
    let error = check_source(
        r#"
rideable := interface():
    Mount():int

rideable{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot construct value from type `interface<rideable>`")
    );
}

#[test]
fn rejects_class_as_interface_parent() {
    let error = check_source(
        r#"
base := class:
    Value:int = 0

bad := interface(base):
    Use():int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("interface parent must be an interface")
    );
}

#[test]
fn rejects_second_class_parent() {
    let error = check_source(
        r#"
first := class:
    Value:int = 0

second := class:
    Other:int = 0

bad := class(first, second):
    Extra:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("additional class parent must be an interface")
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

    expect_int("Mod[5, 3]", 2);
    expect_int("Quotient[10, -3]", -3);
    expect_int("Abs(-5)", 5);
    expect_float("Abs(-5.0)", 5.0);
    expect_int("Min(10, 3)", 3);
    expect_float("Min(10.0, 3.0)", 3.0);
    expect_int("Max(7, 9)", 9);
    expect_float("Max(7.0, 9.0)", 9.0);
    expect_int("Clamp(12, 10, 0)", 10);
    expect_float("Clamp(12.0, 10.0, 0.0)", 10.0);
    expect_float("Lerp(10.0, 20.0, 0.25)", 12.5);
    expect_int("Ceil(1 / 2)", 1);
    expect_int("Floor(7 / 3)", 2);
    expect_int("Round[2.5]", 2);
    expect_int("Int[-3.7]", -3);
}

#[test]
fn evaluates_official_numeric_helpers_with_ordinary_named_arguments() {
    let source = r#"
Clamped:int = Clamp(Value := -1, A := 0, B := 10)
Interpolated:int = if (Value := Round[Lerp(To := 44.0, From := 40.0, Parameter := 0.5)]). Value else. 0
Clamped + Interpolated
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_float_min_max_nan_semantics() {
    let source = r#"
MinNaN := if ((Min(NaN, 1.0)).IsFinite[]). 0 else. 20
MaxNaN := if ((Max(1.0, NaN)).IsFinite[]). 0 else. 22
MinNaN + MaxNaN
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_lerp_non_finite_argument() {
    let error = Interpreter::new()
        .eval_source("Lerp(0.0, Inf, 0.5)")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Lerp` expected finite arguments")
    );
}

#[test]
fn runtime_errors_on_ceil_floor_non_finite_arguments() {
    let ceil_error = Interpreter::new()
        .eval_source("Ceil[NaN]")
        .expect_err("source should fail");
    let floor_error = Interpreter::new()
        .eval_source("Floor[Inf]")
        .expect_err("source should fail");

    assert!(ceil_error.to_string().contains("`Ceil` failed"));
    assert!(floor_error.to_string().contains("`Floor` failed"));
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
fn runtime_errors_on_round_and_int_rational_arguments() {
    let round_error = Interpreter::new()
        .eval_source("Round[1 / 2]")
        .expect_err("source should fail");
    let int_error = Interpreter::new()
        .eval_source("Int[1 / 2]")
        .expect_err("source should fail");

    assert!(
        round_error
            .to_string()
            .contains("`Round` expected `float`, got rational")
    );
    assert!(
        int_error
            .to_string()
            .contains("`Int` expected `float`, got rational")
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(44.0));
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

    assert_eq!(eval(source), Value::Number(47.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn rejects_is_almost_zero_non_numeric_tolerance() {
    let error = check_source(r#"(0.0).IsAlmostZero["near"]"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`IsAlmostZero` AbsoluteTolerance expected `number`")
    );
}

#[test]
fn rejects_is_finite_with_parentheses() {
    let error = check_source("Inf.IsFinite()").expect_err("source should fail");

    assert!(error.to_string().contains("unknown member `IsFinite`"));
}

#[test]
fn rejects_failed_int_outside_failure_context() {
    assert_failable_context_error("Int[NaN]");
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
fn evaluates_unary_positive_operator() {
    assert_eq!(eval("+42"), Value::Number(42.0));
    assert_eq!(check_source("+42").expect("source should check"), Type::Int);
}

#[test]
fn rejects_unary_positive_on_non_number() {
    let error = check_source(r#"+"bad""#).expect_err("source should fail");

    assert!(error.to_string().contains("unary `+` expected `number`"));
}

#[test]
fn evaluates_bindings_and_blocks() {
    let source = r#"
x := 10
{
    y := 5
    x + y
}
"#;

    assert_eq!(eval(source), Value::Number(15.0));
}

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
fn evaluates_official_profile_expression_in_failure_context() {
    let source = r#"
if (profile("Lookup"):
    array{10}[0]
). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_profile_expression_non_string_description() {
    let error = check_source(
        r#"
profile(42):
    1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("profile description expected `string`, got `int`")
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
fn evaluates_recursive_functions() {
    let source = r#"
factorial(n:int):int = if (n <= 1) {
    1
} else {
    n * factorial(n - 1)
}

factorial(5)
"#;

    assert_eq!(eval(source), Value::Number(120.0));
}

#[test]
fn evaluates_top_level_function_overloads_by_parameter_type() {
    let source = r#"
Score(Value:int):int = 40
Score(Value:string):int = 2

Score(1) + Score("bonus")
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn rejects_overload_option_and_logic_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Value:?int):int = 1
Choose(Value:logic):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_array_and_map_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Values:[]int):int = 1
Choose(Values:[int]int):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_function_and_array_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Values:[]int):int = 1
Choose(Callback:type{_(:int):int}):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
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
fn rejects_overload_tuple_and_flattened_parameter_distinctness() {
    let error = check_source(
        r#"
Choose(Pair:tuple(int, int)):int = 1
Choose(A:int, B:int):int = 2
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
fn rejects_overload_interface_and_class_parameter_distinctness() {
    let error = check_source(
        r#"
marker := interface()

thing := class:
    ID:int = 0

Choose(Value:marker):int = 1
Choose(Value:thing):int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate overload `Choose`"));
}

#[test]
fn rejects_overload_class_subtype_parameter_distinctness() {
    let error = check_source(
        r#"
base := class:
    ID:int = 0

child := class(base):
    Extra:int = 1

Choose(Value:base):int = 1
Choose(Value:child):int = 2
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
fn evaluates_decision_expressions_in_failure_context() {
    let source = r#"
Both := if (5 > 0 and 30 >= 20). 20 else. 0
Either := if (0 > 0 or 2 = 2). 20 else. 0
Negated := if (not (0 > 0)). 2 else. 0
Both + Either + Negated
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn converts_values_to_strings() {
    assert_eq!(
        eval(r#""value=" + str(42)"#),
        Value::String("value=42".into())
    );
}

#[test]
fn evaluates_string_interpolation_official_example() {
    let source = r#""2 + 2 = {2 + 2}""#;

    assert_eq!(eval(source), Value::String("2 + 2 = 4".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_interpolation_with_bindings() {
    let source = r#"
Score:int = 40
"Score = {Score + 2}"
"#;

    assert_eq!(eval(source), Value::String("Score = 42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_interpolation_with_nested_braced_expression() {
    let source = r#""Length = {array{1, 2, 3}.Length}""#;

    assert_eq!(eval(source), Value::String("Length = 3".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_escaped_string_interpolation_braces() {
    let source = r#""\{Value\}""#;

    assert_eq!(eval(source), Value::String("{Value}".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_empty_string_interpolants() {
    let source = r#""ab{}cd""#;

    assert_eq!(eval(source), Value::String("abcd".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_multiline_string_continuation_interpolants() {
    let source = r#""This is a multi-line {
}string that continues across {
}multiple lines.""#;

    assert_eq!(
        eval(source),
        Value::String("This is a multi-line string that continues across multiple lines.".into())
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_comment_only_string_interpolants() {
    let source = r#""This is another {
    # This comment is ignored
}message""#;

    assert_eq!(
        eval(source),
        Value::String("This is another message".into())
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_line_comments_with_braces_inside_string_interpolation() {
    let source = r#""{40 # } ignored by comment
}""#;

    assert_eq!(eval(source), Value::String("40".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_unterminated_string_interpolation() {
    let error = parse_source(r#""{Value""#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unterminated string interpolation")
    );
}

#[test]
fn rejects_unescaped_closing_brace_in_string() {
    let error = parse_source(r#""}""#).expect_err("source should fail");

    assert!(error.to_string().contains("unescaped `}`"));
}

#[test]
fn rejects_invalid_string_interpolation_expression() {
    let error = parse_source(r#""Value = {1 + }""#).expect_err("source should fail");

    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn rejects_unknown_name_in_string_interpolation() {
    let error = check_source(r#""Value = {Missing}""#).expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Missing`"));
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
fn evaluates_string_value_assigned_to_message() {
    let source = r#"
Text:string = "Ready"
Label:message = Text
Label
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Message
    );
}

#[test]
fn evaluates_public_data_specifier_on_constant() {
    let source = r#"
Answer<public>:int = 42
Answer
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_native_class_binding_specifier() {
    let source = r#"
widget<native><public> := class<concrete>:
    Value:int = 42

widget{}.Value
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn rejects_message_assigned_to_string() {
    let error = check_source(
        r#"
Greeting<localizes>:message = "Hello"
Text:string = Greeting
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Text` is annotated as `string` but expression has type `message`")
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
fn evaluates_localize_message_function() {
    let source = r#"
Greeting<localizes>:message = "Hello"
Localize(Greeting)
"#;

    assert_eq!(eval(source), Value::String("Hello".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_message_function() {
    let source = r#"
First<localizes>:message = "Hello"
Second<localizes>:message = "Verse"
Separator<localizes>:message = ", "
Joined:message = Join(array{First, Second}, Separator)
Localize(Joined)
"#;

    assert_eq!(eval(source), Value::String("Hello, Verse".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_string_function() {
    let source = r#"
Joined:string = Join(array{"A", "B", "C"}, " + ")
Joined
"#;

    assert_eq!(eval(source), Value::String("A + B + C".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_join_message_function_with_string_literals() {
    let source = r#"
Joined:message = Join(array{"A", "B", "C"}, " + ")
Localize(Joined)
"#;

    assert_eq!(eval(source), Value::String("A + B + C".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_localize_non_message_argument() {
    let error = check_source("Localize(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `message`, got `int`")
    );
}

#[test]
fn evaluates_to_string_number_function() {
    let source = "ToString(42)";

    assert_eq!(eval(source), Value::String("42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_float_function() {
    let source = "ToString(42.5)";

    assert_eq!(eval(source), Value::String("42.5".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_string_function() {
    let source = r#"ToString("Ready")"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_to_string_char_array_function() {
    let source = r#"
Text:[]char = array{'O', 'K'}
ToString(Text)
"#;

    assert_eq!(eval(source), Value::String("OK".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_ascii_char_literals_and_annotations() {
    let source = r#"
Letter:char = 'a'
Letter
"#;

    assert_eq!(eval(source), Value::Char('a'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char
    );
}

#[test]
fn evaluates_char_escape_literals_and_to_string() {
    let source = r#"
LineFeed:char = '\n'
Text := ToString(LineFeed)
if (Text.Length = 1 and Text[0] = '\n'):
    Text
else:
    ToString('x')
"#;

    assert_eq!(eval(source), Value::String("\n".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_non_ascii_char32_literals_and_annotations() {
    let source = r#"
Letter:char32 = '好'
Letter
"#;

    assert_eq!(eval(source), Value::Char32('好'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char32
    );
}

#[test]
fn evaluates_hex_character_literals() {
    let source = r#"
Byte:char = 0o65
Wide:char32 = 0u00E9
Emoji:char32 = 0u1f600
ToString(Byte) + ToString(Wide) + ToString(Emoji)
"#;

    assert_eq!(eval(source), Value::String("eé😀".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_char_literals_as_map_keys() {
    let source = r#"
Scores:[char]int = map{'a' => 41}
if (Score := Scores['a']). Score + 1 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_string_escape_table_entries() {
    let source = r#""\#\<\>\&\~\'""#;

    assert_eq!(eval(source), Value::String("#<>&~'".into()));
}

#[test]
fn evaluates_char_array_annotation_from_string() {
    let source = r#"
Text:[]char = "Ready"
Text
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_string_annotation_from_char_array() {
    let source = r#"
Text:[]char = "Ready"
Label:string = Text
Label
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_char_array_function_parameter_and_return() {
    let source = r#"
Echo(Text:[]char):[]char = Text
Echo("Ready")
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_char_array_type_alias_annotations() {
    let source = r#"
text := []char
Label:text = "Ready"
Label
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn rejects_string_literal_assigned_to_char32_array() {
    let error = check_source(r#"Utf32:[]char32 = "A""#).expect_err("source should fail");

    assert!(error.to_string().contains(
        "binding `Utf32` is annotated as `array<char32>` but expression has type `string`"
    ));
}

#[test]
fn evaluates_string_indexing_as_utf8_char_units() {
    let source = r#"
Text:string = "Verse"
if (Letter := Text[0]). Letter else. 'x'
"#;

    assert_eq!(eval(source), Value::Char('V'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char
    );
}

#[test]
fn evaluates_unicode_string_length_as_utf8_units() {
    let source = r#"
Text:string = "José"
Text.Length
"#;

    assert_eq!(eval(source), Value::Number(5.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_string_index_failure_in_option_literal() {
    let source = r#"
Text:string = "ab"
option{Text[2]}
"#;

    assert_eq!(eval(source), Value::Option(None));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Option(Box::new(Type::Char))
    );
}

#[test]
fn rejects_string_index_with_non_int() {
    let error = check_source(r#"if (Letter := "abc"["0"]). Letter else. 'x'"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("string index expected `int`"));
}

#[test]
fn rejects_string_index_with_float() {
    let error = check_source(r#"if (Letter := "abc"[1.0]). Letter else. 'x'"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("string index expected `int`"));
}

#[test]
fn evaluates_string_slot_assignment() {
    let source = r#"
var Text:string = "Glorblex"
if:
    set Text[0] = 'F'
then:
    {}
else:
    {}
Text
"#;

    assert_eq!(eval(source), Value::String("Florblex".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_char_array_equality() {
    let source = r#"
if ("abc" = array{'a', 'b', 'c'}):
    1
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(1.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_string_annotation_from_char_array_literal() {
    let source = r#"
Text:string = array{'a', 'b', 'c'}
Text
"#;

    assert_eq!(eval(source), Value::String("abc".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_non_string_value_assigned_to_char_array() {
    let error = check_source("Text:[]char = 42").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Text` is annotated as `array<char>` but expression has type `int`")
    );
}

#[test]
fn rejects_string_literal_assigned_to_bare_char() {
    let error = check_source(r#"Letter:char = "A""#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char` but expression has type `string`")
    );
}

#[test]
fn rejects_char32_literal_assigned_to_char() {
    let error = check_source("Letter:char = '好'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char` but expression has type `char32`")
    );
}

#[test]
fn rejects_char_literal_assigned_to_char32() {
    let error = check_source("Letter:char32 = 'a'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char32` but expression has type `char`")
    );
}

#[test]
fn rejects_empty_character_literal() {
    let error = parse_source("Letter := ''").expect_err("source should fail");

    assert!(error.to_string().contains("empty character literal"));
}

#[test]
fn rejects_multi_character_literal() {
    let error = parse_source("Letter := 'ab'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("character literal cannot contain multiple characters")
    );
}

#[test]
fn rejects_short_hex_char_literal() {
    let error = parse_source("Letter := 0o6").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected two hexadecimal digits after `0o`")
    );
}

#[test]
fn rejects_long_hex_char_literal() {
    let error = parse_source("Letter := 0o616").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`0o` character literal expects exactly two hexadecimal digits")
    );
}

#[test]
fn rejects_long_hex_char32_literal() {
    let error = parse_source("Letter := 0u1234567").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`0u` character literal expects at most six hexadecimal digits")
    );
}

#[test]
fn rejects_invalid_hex_char32_codepoint() {
    let error = parse_source("Letter := 0u110000").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("invalid Unicode code point `0u110000`")
    );
}

#[test]
fn rejects_join_non_string_or_message_array() {
    let error = check_source(r#"Join(array{1, 2}, ", ")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (array<int>, string)")
    );
}

#[test]
fn rejects_to_string_logic_argument() {
    let error = check_source("ToString(true)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (bool)")
    );
}

#[test]
fn rejects_to_string_rational_argument() {
    let error = check_source("if (Value := 1 / 2). ToString(Value) else. \"\"")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (rational)")
    );
}

#[test]
fn runtime_errors_on_to_string_rational_argument() {
    let error = Interpreter::new()
        .eval_source("ToString(1 / 2)")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`ToString` expected `float`, `int`, `[]char`, or `char`, got rational")
    );
}

#[test]
fn rejects_to_string_message_argument() {
    let error = check_source(
        r#"
Greeting<localizes>:message = "Hello"
ToString(Greeting)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (message)")
    );
}

#[test]
fn evaluates_official_random_functions() {
    let source = r#"
using { /Verse.org/Random }
FixedInt:int = GetRandomInt(5, 5)
OutOfOrder:int = GetRandomInt(10, 8)
FixedFloat:float = GetRandomFloat(2.0, 2.0)
Shuffled:[]int = Shuffle(array{35})
Bounds := if (OutOfOrder >= 8 and OutOfOrder <= 10). 0 else. 1000
Rounded := if (Value := Round[FixedFloat]). Value else. 0
ShuffledValue := if (Value := Shuffled[0]). Value else. 0
FixedInt + Rounded + ShuffledValue + Bounds
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_random_functions_with_ordinary_named_arguments() {
    let source = r#"
using { /Verse.org/Random }
FixedInt:int = GetRandomInt(High := 40, Low := 40)
FixedFloat:float = GetRandomFloat(High := 2.0, Low := 2.0)
Shuffled:[]int = Shuffle(Input := array{0})
Rounded := if (Value := Round[FixedFloat]). Value else. 0
ShuffledValue := if (Value := Shuffled[0]). Value else. 0
FixedInt + Rounded + ShuffledValue
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_get_random_int_argument_type_mismatch() {
    let error = check_source("GetRandomInt(0.0, 1.0)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `int`, got `float`")
    );
}

#[test]
fn rejects_get_random_float_argument_type_mismatch() {
    let error = check_source(r#"GetRandomFloat("low", 1.0)"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `float`, got `string`")
    );
}

#[test]
fn rejects_shuffle_non_array_argument() {
    let error = check_source("Shuffle(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `array`, got `int`")
    );
}

#[test]
fn rejects_shuffle_extra_arguments() {
    let error = check_source("Shuffle(1, 2)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Shuffle` expected 1 arguments, got 2")
    );
}

#[test]
fn checks_err_as_never_returning_function() {
    let source = r#"
Value:int = Err("fatal")
Value
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_err_function() {
    let error = Interpreter::new()
        .eval_source(r#"Err("fatal stop")"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("fatal stop"));
}

#[test]
fn evaluates_official_print_function() {
    let source = r#"
Greeting<localizes>:message = "Hello"
Print("Ready")
Print(Greeting)
Print(ToDiagnostic("diag"))
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn evaluates_color_struct_make_color_and_named_colors() {
    let source = r#"
using { /Verse.org/Colors }
Made:color = MakeColorFromSRGB(1.0, 2.0, 3.0)
Manual:color = color{R := NamedColors.Red.R, G := NamedColors.Green.G, B := NamedColors.Blue.B}
Made.R + Made.G + Made.B + Manual.R + Manual.G + Manual.B
"#;

    assert_eq!(eval(source), Value::Float(8.0 + 128.0 / 255.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_named_colors_extended_css_keywords() {
    let source = r#"
using { /Verse.org/Colors }
Alice:color = MakeColorFromSRGBValues(240, 248, 255)
Hot:color = MakeColorFromSRGBValues(255, 105, 180)
Pale:color = MakeColorFromSRGBValues(219, 112, 147)
if (NamedColors.AliceBlue = Alice and NamedColors.Hotpink = Hot and NamedColors.PaleVioletred = Pale and NamedColors.DarkSlateGrey = NamedColors.DarkSlateGray). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_color_arithmetic_operators() {
    let source = r#"
using { /Verse.org/Colors }
Base:color = MakeColorFromSRGB(0.25, 0.5, 0.75)
Other:color = MakeColorFromSRGB(0.5, 0.25, 0.25)
Sum:color = Base + Other
Diff:color = Sum - Base
Product:color = Base * Other
ScaledLeft:color = Base * 2
ScaledRight:color = 2.0 * Other
Divided:color = if (Value := ScaledLeft / 2). Value else. Base
Sum.R + Sum.G + Sum.B + Diff.R + Diff.G + Diff.B + Product.R + Product.G + Product.B + ScaledLeft.R + ScaledRight.B + Divided.G
"#;

    assert_eq!(eval(source), Value::Float(5.4375));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_color_from_srgb_values_and_hex() {
    let source = r##"
using { /Verse.org/Colors }
FromValues:color = MakeColorFromSRGBValues(255, 128, 0)
FromShortHex:color = MakeColorFromHex("#0f8")
FromLongHex:color = MakeColorFromHex("0000ffcc")
Invalid:color = MakeColorFromHex("bad value")
FromValues.R + FromValues.B + FromShortHex.G + FromLongHex.B + Invalid.R + Invalid.G + Invalid.B
"##;

    assert_eq!(eval(source), Value::Float(3.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_srgb_and_hsv_color_tuple_helpers() {
    let source = r#"
using { /Verse.org/Colors }
RGB:tuple(float, float, float) = MakeSRGBFromColor(MakeColorFromSRGB(1.0, 0.5, 0.0))
FromHSV:color = MakeColorFromHSV(480.0, 1.0, 1.0)
HSV:tuple(float, float, float) = MakeHSVFromColor(FromHSV)
RGB(0) + RGB(1) + RGB(2) + FromHSV.G + HSV(0) + HSV(1) + HSV(2)
"#;

    assert_eq!(eval(source), Value::Float(124.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_color_helpers_with_ordinary_named_arguments() {
    let source = r#"
using { /Verse.org/Colors }
Color:color = MakeColorFromSRGB(Blue := 0.25, Red := 1.0, Green := 0.5)
RGB:tuple(float, float, float) = MakeSRGBFromColor(Color := Color)
if (RGB(0) = 1.0 and RGB(1) = 0.5 and RGB(2) = 0.25):
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
fn evaluates_official_color_alpha_struct_and_over() {
    let source = r#"
using { /Verse.org/Colors }
Front:color_alpha = MakeColorAlpha(1.0, 0.0, 0.0, 0.5)
Back:color_alpha = color_alpha{Color := MakeColorFromSRGB(0.0, 0.0, 1.0), A := 0.5}
Blended:color_alpha = Over(Front, Back)
Blended.Color.R * 3.0 + Blended.Color.B * 3.0 + Blended.A * 4.0
"#;

    assert_eq!(eval(source), Value::Float(6.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn rejects_make_color_from_srgb_values_out_of_range() {
    let error = Interpreter::new()
        .eval_source("MakeColorFromSRGBValues(256, 0, 0)")
        .expect_err("source should fail");

    assert!(error.to_string().contains("0..255"));
}

#[test]
fn rejects_over_with_zero_alpha_colors() {
    let error = Interpreter::new()
        .eval_source(
            r#"
using { /Verse.org/Colors }
Over(MakeColorAlpha(1.0, 0.0, 0.0, 0.0), MakeColorAlpha(0.0, 0.0, 1.0, 0.0))
"#,
        )
        .expect_err("source should fail");

    assert!(error.to_string().contains("`Over` failed"));
}

#[test]
fn rejects_color_operator_type_mismatch() {
    let error =
        check_source("MakeColorFromSRGB(1.0, 0.0, 0.0) + 1").expect_err("source should fail");

    assert!(error.to_string().contains("colors"));
}

#[test]
fn rejects_over_non_color_alpha_argument() {
    let error = check_source(
        r#"
using { /Verse.org/Colors }
Over(MakeColorAlpha(1.0, 0.0, 0.0, 0.5), MakeColorFromSRGB(0.0, 0.0, 1.0))
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 2 expected `color_alpha`")
    );
}

#[test]
fn evaluates_official_locale_empty_struct() {
    let source = r#"
Locale:locale = locale{}
if (Locale = locale{}):
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
fn rejects_official_locale_unknown_field() {
    let error =
        check_source("Locale := locale{Language := \"en\"}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("struct `locale` has no field `Language`")
    );
}

#[test]
fn rejects_named_color_outside_official_css3_list() {
    let error = check_source(
        r#"
using { /Verse.org/Colors }
NamedColors.RebeccaPurple
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("module `NamedColors` has no member `RebeccaPurple`")
    );
}

#[test]
fn evaluates_official_print_named_duration_and_color_arguments() {
    let source = r#"
using { /Verse.org/Colors }
Print("Ready", ?Duration := 1.5, ?Color := MakeColorFromSRGB(1.0, 0.0, 0.0))
Print(ToDiagnostic("diag"), ?Color := NamedColors.Blue)
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn evaluates_official_print_ordinary_named_arguments() {
    let source = r#"
Print(Message := "Ready", Duration := 1.5, Color := MakeColorFromSRGB(Red := 1.0, Green := 0.0, Blue := 0.0))
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn rejects_print_non_text_argument() {
    let error = check_source("Print(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("no overload matches () call with argument types (int)")
    );
}

#[test]
fn rejects_print_bad_duration_argument() {
    let error =
        check_source(r#"Print("Ready", ?Duration := "slow")"#).expect_err("source should fail");

    assert!(error.to_string().contains("no overload matches"));
}

#[test]
fn rejects_print_bad_color_argument() {
    let error = check_source(r#"Print("Ready", ?Color := 1)"#).expect_err("source should fail");

    assert!(error.to_string().contains("no overload matches"));
}

#[test]
fn evaluates_to_diagnostic_function() {
    let source = r#"
Entry:diagnostic = ToDiagnostic(42)
Entry
"#;

    assert!(matches!(eval(source), Value::Diagnostic(_)));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("diagnostic".into())
    );
}

#[test]
fn evaluates_diagnostic_concatenation() {
    let source = r#"
Entry:diagnostic = ToDiagnostic("ready") + "!" + ToDiagnostic(7)
Result:diagnostic = ">" + Entry
Result
"#;

    assert_eq!(eval(source), Value::Diagnostic(">ready!7".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("diagnostic".into())
    );
}

#[test]
fn rejects_diagnostic_assigned_to_string() {
    let error =
        check_source(r#"Text:string = ToDiagnostic("ready")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `string` but expression has type `diagnostic`")
    );
}

#[test]
fn rejects_diagnostic_plus_non_string_or_diagnostic() {
    let error = check_source("ToDiagnostic(1) + 2").expect_err("source should fail");

    assert!(error.to_string().contains("diagnostics"));
}

#[test]
fn evaluates_get_seconds_since_epoch_function() {
    let source = "GetSecondsSinceEpoch()";
    let value = eval(source);

    assert!(matches!(value, Value::Number(seconds) if seconds > 0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn get_seconds_since_epoch_is_stable_within_eval_transaction() {
    let source = r#"
First := GetSecondsSinceEpoch()
Second := GetSecondsSinceEpoch()
if (First = Second). true else. false
"#;

    assert_eq!(eval(source), Value::Bool(true));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Bool
    );
}

#[test]
fn rejects_get_seconds_since_epoch_arguments() {
    let error = check_source("GetSecondsSinceEpoch(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn evaluates_typed_bindings_and_functions() {
    let source = r#"
x: number := 40
add(a: number, b: number): number = a + b
add(x, 2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

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
fn evaluates_scientific_float_literals() {
    let source = "1.0e2 + 2.5e+1 + 5.0e-1";

    assert_eq!(eval(source), Value::Float(125.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_f64_suffixed_float_literals() {
    let source = "12.25f64 + 7.75f64";

    assert_eq!(eval(source), Value::Float(20.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn rejects_float_literal_exponent_without_digits() {
    let error = parse_source("1.0e+").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected exponent digits in float literal")
    );
}

#[test]
fn rejects_overflowing_float_literals() {
    let error = parse_source("1.7976931348623159e+308").expect_err("source should fail");

    assert!(error.to_string().contains("outside the finite f64 range"));
}

#[test]
fn rejects_float_literal_assigned_to_int() {
    let error = check_source("Whole:int = 1.5").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Whole` is annotated as `int` but expression has type `float`")
    );
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
fn evaluates_int_literal_as_float_parameter() {
    let source = r#"
Scale(Value:float):float = Value + 0.5
Scale(41)
"#;

    assert_eq!(eval(source), Value::Number(41.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_rational_type_annotations() {
    let source = r#"
Half:rational = if (Value := 1 / 2). Value else. 0
Ceil(Half) + Floor(Half)
"#;

    assert_eq!(eval(source), Value::Number(1.0));
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

    assert_eq!(eval(source), Value::Number(23.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_nested_named_function_values() {
    let source = r#"
MakeAdder(X:int) = {
    Add(Y:int):int = X + Y
    Add
}

MakeAdder(40)(2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_function_type_annotations() {
    let source = r#"
Double(X:int):int = X * 2
Fn:type{_(:int):int} = Double
Fn(21)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_function_parameters_with_type_annotations() {
    let source = r#"
Apply(F:type{_(:int):int}, Value:int):int = F(Value)
Double(X:int):int = X * 2
Apply(Double, 21)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_optional_function_type_annotations() {
    let source = r#"
Default():int = 40
Custom():int = 42
var Handler:?type{_():int} = false
set Handler = option{Custom}
Handler?()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn checks_function_type_with_effect_specifiers() {
    let source = r#"
Pick(X:int)<decides><transacts>:int = X
Handler:type{_(:int)<decides><transacts>:int} = Pick
if (Value := Handler[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_function_definitions_with_name_specifiers() {
    let source = r#"
Visible<public>(X:int):int = X + 1
Visible(41)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_decides_function_bracket_calls() {
    let source = r#"
Pick(Value:int)<decides><transacts>:int = Value
if (Value := Pick[42]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn rejects_decides_function_without_transacts_effect() {
    let error = check_source(
        r#"
Pick(Value:int)<decides>:int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<decides>` must also have `<transacts>`")
    );
}

#[test]
fn rejects_decides_abstract_class_method_without_transacts_effect() {
    let error = check_source(
        r#"
picker := class<abstract>:
    Pick()<decides>:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<decides>` must also have `<transacts>`")
    );
}

#[test]
fn rejects_decides_interface_method_without_transacts_effect() {
    let error = check_source(
        r#"
picker := interface:
    Pick()<decides>:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<decides>` must also have `<transacts>`")
    );
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
fn rejects_mutable_assignment_in_function_without_transacts_effect() {
    let error = check_source(
        r#"
Bump():int =
    var Total:int = 0
    set Total += 1
    Total
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<transacts>` effect")
    );
}

#[test]
fn rejects_class_field_assignment_in_method_without_transacts_effect() {
    let error = check_source(
        r#"
counter := class:
    var Value:int = 0

    Increment():void =
        set Value += 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<transacts>` effect")
    );
}

#[test]
fn rejects_container_slot_assignment_in_function_without_transacts_effect() {
    let error = check_source(
        r#"
ReplaceFirst():int =
    var Values:[]int = array{1}
    if:
        set Values[0] = 2
    then:
        Values.Length
    else:
        0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<transacts>` effect")
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn rejects_no_rollback_method_call_in_failure_context() {
    let error = check_source(
        r#"
reader := class:
    Read():int = 42

Item := reader{}
if:
    Value := Item.Read()
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn evaluates_computes_argument_call_in_failure_context() {
    let source = r#"
Accept(Value:logic)<computes>:logic = Value
IsReady()<computes>:logic = true
if (Accept(IsReady())?):
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
fn rejects_unknown_function_type_effect_specifier() {
    let error =
        parse_source("Handler:type{_(:int)<custom>:int} = Double").expect_err("source should fail");

    assert!(error.to_string().contains("unknown effect specifier"));
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
fn evaluates_comparable_function_parameter() {
    let source = r#"
Accept(Key:comparable):int = 42
Accept((1, "a"))
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn rejects_non_unique_class_value_assigned_to_comparable() {
    let error = check_source(
        r#"
thing := class:
    ID:int = 0

Bad:comparable = thing{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("binding `Bad`"));
    assert!(error.to_string().contains("comparable"));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_set_expression_in_if_condition() {
    let source = r#"
var TeamSize:int = 0
Sizes:[string]int = map{"red" => 42}
Result := if (set TeamSize = Sizes["red"]):
    TeamSize
else:
    0
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_map_slot_set_expression_before_failure_binding() {
    let source = r#"
var Scores:[string]int = map{}
Result := if (set Scores["ada"] = 42, Score := Scores["ada"]):
    Score
else:
    0
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_set_expression_assignment_without_transacts_effect() {
    let error = check_source(
        r#"
var Total:int = 0
Bad():int =
    if (set Total = 42):
        Total
    else:
        0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("mutable assignment in function requires `<transacts>` effect")
    );
}

#[test]
fn rejects_set_expression_to_immutable_binding() {
    let error = check_source(
        r#"
Total:int = 0
if (set Total = 42):
    1
else:
    0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable binding `Total`")
    );
}

#[test]
fn rejects_plain_set_expression_as_only_if_condition() {
    let error = check_source(
        r#"
var Total:int = 0
if (set Total = 42):
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

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
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
fn runtime_errors_on_defer_when_if_failure_context_fails() {
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

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail at runtime");

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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_array_and_map_mutations_when_failure_context_fails() {
    let source = r#"
var Values:[]int = array{1, 2}
var Scores:[string]int = map{"alice" => 10}
if:
    set Values[0] = 99
    set Scores["alice"] = 77
    false?
then:
    0
else:
    if:
        Value := Values[0]
        Score := Scores["alice"]
    then:
        Value + Score + 31
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
fn rolls_back_class_field_mutation_when_failure_context_fails() {
    let source = r#"
counter := class:
    var Value:int = 1

Counter := counter{}
if:
    set Counter.Value = 99
    false?
then:
    0
else:
    Counter.Value + 41
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_option_failure_context_when_it_fails() {
    let source = r#"
var Total:int = 0
Maybe := option{block:
    set Total = 99
    false?
    1
}
if (Maybe?):
    0
else:
    Total + 42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_defer_when_option_failure_context_fails() {
    let source = r#"
Maybe := option{block:
    defer:
        Err("option defer ran")
    false?
    42
}
if (Maybe?):
    0
else:
    42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail at runtime");

    assert!(error.to_string().contains("option defer ran"));
}

#[test]
fn runtime_errors_on_defer_when_decides_function_fails() {
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

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail at runtime");

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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn if_then_condition_bindings_do_not_escape() {
    let error = check_source(
        r#"
if:
    Value := option{42}?
then:
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
fn evaluates_if_failure_binding_array_lookup_success() {
    let source = r#"
Values:[]int = array{40, 2}
if (Value := Values[1]):
    Value + 40
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
fn evaluates_if_failure_binding_array_lookup_failure() {
    let source = r#"
Values:[]int = array{40, 2}
if (Value := Values[10]):
    Value
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_failure_binding_map_lookup_failure() {
    let source = r#"
Scores:[string]int = map{"ada" => 40}
if (Score := Scores["grace"]):
    Score
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_failure_binding_option_query() {
    let source = r#"
Filled:?int = option{42}
Empty:?int = false
First := if (Value := Filled?). Value else. 0
Second := if (Value := Empty?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_if_option_query_without_binding() {
    let source = r#"
Filled:?int = option{1}
Empty:?int = false
First := if (Filled?). 40 else. 0
Second := if (Empty?). 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn evaluates_if_optional_method_query() {
    let source = r#"
player := class:
    Name : string = "Ava"

    Label()<computes>:string =
        Self.Name + "!"

Filled:?player = option{player{}}
Empty:?player = false

First := if (Label := Filled?.Label()). Label else. "missing"
Second := if (Label := Empty?.Label()). Label else. "none"
First + ":" + Second
"#;

    assert_eq!(eval(source), Value::String("Ava!:none".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_if_failure_binding_outside_then_branch() {
    let error = check_source(
        r#"
Values:[]int = array{42}
if (Value := Values[0]):
    Value
else:
    Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
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
fn rejects_binding_type_mismatch() {
    let error = check_source(r#"x: number := "not a number""#).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `number`"));
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
fn rejects_non_verse_fun_function_literals() {
    let error = parse_source("Double := fun(X:int):int { X * 2 }").expect_err("source should fail");

    assert!(error.to_string().contains("expected `)`"));
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

    assert_eq!(eval(source), Value::Number(25.0));
}

#[test]
fn evaluates_official_ordinary_named_argument() {
    let source = r#"
BuyMousetrap(CoinsPerMousetrap:int):int = CoinsPerMousetrap + 32
BuyMousetrap(CoinsPerMousetrap := 10)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_ordinary_named_method_arguments() {
    let source = r#"
counter := class:
    Add(Left:int, Right:int):int = Left + Right

counter{}.Add(Right := 2, Left := 40)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_default_parameter_using_earlier_parameter() {
    let source = r#"
Offset(Value:int, ?Amount:int = Value + 1):int = Value + Amount
Offset(10) + Offset(10, ?Amount := 5)
"#;

    assert_eq!(eval(source), Value::Number(36.0));
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
fn expands_tuple_arguments_for_function_calls() {
    let source = r#"
Add(A:int, B:int):int = A + B
Args := (40, 2)
Add(Args)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn packs_flattened_arguments_for_tuple_parameters() {
    let source = r#"
Add(Pair:tuple(int, int)):int = Pair(0) + Pair(1)
Args := (10, 20)
Add(40, 2) + Add(Args)
"#;

    assert_eq!(eval(source), Value::Number(72.0));
}

#[test]
fn evaluates_destructured_tuple_parameters() {
    let source = r#"
Add(A:int, (B:int, C:int), D:int):int = A + B + C + D
Pair := (2, 3)
Args := (1, Pair, 4)
Add(1, Pair, 4) + Add(Args)
"#;

    assert_eq!(eval(source), Value::Number(20.0));
}

#[test]
fn evaluates_nested_destructured_tuple_parameters() {
    let source = r#"
Sum(A:int, (B:int, (C:int, D:int)), E:int):int = A + B + C + D + E
Inner := (3, 4)
Middle := (2, Inner)
Args := (1, Middle, 5)
Sum(1, Middle, 5) + Sum(Args)
"#;

    assert_eq!(eval(source), Value::Number(30.0));
}

#[test]
fn rejects_duplicate_destructured_tuple_parameter_names() {
    let error = parse_source(
        r#"
Bad((Value:int, Value:int)):int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate parameter name"));
}

#[test]
fn rejects_destructured_tuple_parameter_type_mismatch() {
    let error = check_source(
        r#"
Add((Left:int, Right:int)):int = Left + Right
Add("bad", 2)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple argument item 1 expected `int`, got `string`")
    );
}

#[test]
fn evaluates_single_array_parameter_variadic_calls() {
    let source = r#"
Sum(Items:[]int):int = {
    var Total:int = 0
    for (Item : Items) {
        set Total += Item
    }
    Total
}

Values := (10, 20, 30)
Sum(1, 2, 3) + Sum((4, 5)) + Sum(Values) + Sum(6)
"#;

    assert_eq!(eval(source), Value::Number(81.0));
}

#[test]
fn rejects_single_array_parameter_variadic_type_mismatch() {
    let error = check_source(
        r#"
Sum(Items:[]int):int = Items.Length
Sum(1, "bad")
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("array argument item 2"));
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

    assert_eq!(eval(source), Value::Number(7.0));
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

    assert_eq!(eval(source), Value::Number(0.0));
}

#[test]
fn rejects_unreachable_statement_after_return() {
    let error = check_source(
        r#"
Bad():int = {
    return 1
    2
}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unreachable code after `return`")
    );
}

#[test]
fn rejects_unreachable_statement_after_bare_return() {
    let error = check_source(
        r#"
Bad():void =
    return
    print("bad")
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unreachable code after `return`")
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
fn rejects_unreachable_statement_after_never_expression() {
    let error = check_source(
        r#"
Bad():int =
    Err("fatal")
    42
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
fn rejects_unreachable_failure_clause_after_never_expression() {
    let error = check_source(
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
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unreachable code after never-returning expression")
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
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source("return 1")
        .expect_err("source should fail");

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
fn evaluates_enum_values_and_comparison() {
    let source = r#"
direction := enum{North, East, South, West}
var Facing:direction = direction.North
set Facing = direction.East
if (Facing = direction.East) {
    42
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_enum_colon_block_definitions() {
    let source = r#"
direction := enum:
    North
    East
    South
    West

Current:direction = direction.East
if (Current = direction.East) {
    42
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_native_enum_binding_specifier() {
    let source = r#"
text_overflow_policy<native><public> := enum:
    Clip
    Ellipsis

Policy:text_overflow_policy = text_overflow_policy.Ellipsis
if (Policy = text_overflow_policy.Ellipsis) {
    42
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_session_environment_enum_values() {
    let source = r#"
Env:session_environment = session_environment.Edit
if (Env = session_environment.Edit and Env <> session_environment.Live). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_session_environment_case_expression() {
    let source = r#"
Env:session_environment = session_environment.Private
case (Env):
    session_environment.Edit => 1
    session_environment.Private => 42
    session_environment.Live => 3
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unknown_official_session_environment_value() {
    let error = check_source("session_environment.Unknown").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("enum `session_environment` has no value `Unknown`")
    );
}

#[test]
fn evaluates_official_session_environment_extension() {
    let source = r#"
Env:session_environment = GetSession().Environment()
if (Env = session_environment.Edit). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_session_environment_extension_in_case() {
    let source = r#"
case (GetSession().Environment()):
    session_environment.Edit => 42
    session_environment.Private => 2
    session_environment.Live => 3
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_session_environment_extension_arguments() {
    let error = check_source("GetSession().Environment(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn rejects_unknown_session_member() {
    let error = check_source("GetSession().Missing()").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `session` has no member `Missing`")
    );
}

#[test]
fn evaluates_official_simulation_class_optional_annotations() {
    let source = r#"
MaybeAgent:?agent = false
MaybeTeam:?team = false
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_team_type_alias_annotation() {
    let source = r#"
team_reference := ?team
MaybeTeam:team_reference = false
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_player_and_team_map_alias_annotations() {
    let source = r#"
player_map := [player]int
team_map := [team]player_map
var TeamMap:team_map = map{}
TeamMap.Length
"#;

    assert_eq!(eval(source), Value::Number(0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_entity_type_alias_annotation() {
    let source = r#"
entity_ref := entity
MaybeEntity:?entity_ref = false
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn evaluates_user_parametric_class_instance_methods() {
    let source = r#"
stack<public>(t:type) := class:
    Elements<public>:[]t = array{}
    Push<public>(NewElement:t):stack(t) =
        stack(t){ Elements := Elements + array{NewElement} }
    Peek<public>()<transacts><decides>:t =
        Elements[0]

Empty:stack(int) = stack(int){}
Filled := Empty.Push(42)
if (Value := Filled.Peek[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn checks_official_modifier_stack_member_surface() {
    let source = r#"
Use(Stack:modifier_stack(int), Modifier:modifier(int))<transacts>:void =
    block:
        First:?rational = Stack.FirstPosition
        Last:?rational = Stack.LastPosition
        Value:int = Stack.Evaluate(40)
        Modified:int = Modifier.Evaluate(Value)
        Subscription:cancelable = Stack.AddModifier(Modifier, 1)
        print("done")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(2),
            vec!["transacts".to_string()],
            Some(vec![
                Type::ModifierStack(Box::new(Type::Int)),
                Type::Modifier(Box::new(Type::Int)),
            ]),
            Type::None
        )
    );
}

#[test]
fn checks_modifier_stack_assignable_to_modifier_interface() {
    let source = r#"
AsModifier(Stack:modifier_stack(int)):modifier(int) = Stack
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::ModifierStack(Box::new(Type::Int))]),
            Type::Modifier(Box::new(Type::Int))
        )
    );
}

#[test]
fn evaluates_external_modifier_as_identity_runtime_modifier() {
    let source = r#"
Modifier:modifier(int) = external {}
Modifier.Evaluate(42)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_external_modifier_stack_as_empty_runtime_stack() {
    let source = r#"
Stack:modifier_stack(int) = external {}
NoFirst := if (Stack.FirstPosition?). 0 else. 20
NoLast := if (Stack.LastPosition?). 0 else. 22
NoFirst + NoLast + Stack.Evaluate(0)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_user_modifier_stack_ordering_and_cancel_runtime() {
    let source = r#"
add := class(modifier(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

multiply := class(modifier(int)):
    Factor:int
    Evaluate<override>(InValue:int):int =
        InValue * Factor

Stack:modifier_stack(int) = external {}
Stack.AddModifier(add{Amount := 2}, 0)
Handle:cancelable = Stack.AddModifier(multiply{Factor := 10}, 0)
BeforeCancel:int = Stack.Evaluate(4)
Handle.Cancel()
AfterCancel:int = Stack.Evaluate(4)
FirstIsZero := if (Position := Stack.FirstPosition?, Position = 0). 1 else. 0
BeforeCancel + AfterCancel + FirstIsZero
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(67));
}

#[test]
fn rolls_back_modifier_stack_add_modifier_in_failure_context() {
    let source = r#"
add := class(modifier(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

Stack:modifier_stack(int) = external {}
if:
    Stack.AddModifier(add{Amount := 40}, 0)
    false?
then:
    0
else:
    Stack.Evaluate(2)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(2));
}

#[test]
fn rejects_modifier_class_missing_evaluate_implementation() {
    let error = check_source(
        r#"
bad := class(modifier(int)):
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("must be `abstract` or implement method `Evaluate`")
    );
}

#[test]
fn checks_official_generator_type_alias_and_for_iteration() {
    let source = r#"
int_generator := generator(int)
Collect(Values:int_generator):[]int =
    for (Value : Values):
        Value
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Generator(Some(Box::new(Type::Int)))]),
            Type::Array(Box::new(Type::Int))
        )
    );
}

#[test]
fn checks_official_parameterless_generator_for_iteration() {
    let source = r#"
Collect(Values:generator()):[]any =
    for (Value : Values):
        Value
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Generator(None)]),
            Type::Array(Box::new(Type::Any))
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
fn evaluates_classifiable_subset_accepts_interface_implementer_runtime_members() {
    let source = r#"
moveable := interface:
    MoveForward():int

rideable := interface(moveable):
    Mount():int

horse := class(rideable):
    MoveForward<override>():int = 40
    Mount<override>():int = 2

Ride := horse{}
Set:classifiable_subset(moveable) = MakeClassifiableSubset(array{Ride})
Ride.MoveForward() + Ride.Mount()
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
fn evaluates_classifiable_subset_contains_runtime_failure_context() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{})
if (Set.Contains[TagType]). 0 else. 42
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
fn rejects_classifiable_subset_contains_outside_failure_context() {
    let source = r#"
TagType:castable_subtype(tag) = external {}
Set:classifiable_subset(tag) = MakeClassifiableSubset(array{})
Set.Contains[TagType]
"#;
    assert_failable_context_error(source);
}

#[test]
fn checks_official_task_await_member() {
    let source = r#"
Wait(Task:task(int))<suspends>:int = Task.Await()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["suspends".to_string()],
            Some(vec![Type::Task(Box::new(Type::Int))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_task_subtype_of_awaitable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 42
Use(Task:task(int)):int = AcceptAwaitable(Task)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Task(Box::new(Type::Int))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_spawn_expression_returns_task() {
    let source = r#"
Compute()<suspends>:int = 42
Start():task(int) = spawn{Compute()}
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            Vec::<String>::new(),
            Some(vec![]),
            Type::Task(Box::new(Type::Int))
        )
    );
}

#[test]
fn checks_official_spawn_task_can_be_awaited_in_async_context() {
    let source = r#"
Compute()<suspends>:int = 42
Start()<suspends>:int =
    Task := spawn{Compute()}
    Task.Await()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_event_await_and_signal_members() {
    let source = r#"
WaitForPayload(Event:event(int))<suspends>:int = Event.Await()
Notify(Event:event(int)):void = Event.Signal(42)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(Some(Box::new(Type::Int)))]),
            Type::None
        )
    );
}

#[test]
fn checks_official_parameterless_event_members() {
    let source = r#"
Wait(Event:event())<suspends>:void = Event.Await()
Notify(Event:event()):void = Event.Signal()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(None)]),
            Type::None
        )
    );
}

#[test]
fn evaluates_official_parameterless_event_construction() {
    let source = r#"
Done:event() = event(){}
Done.Signal()
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_payload_event_construction_and_signal() {
    let source = r#"
Payload:event(int) = event(int){}
Payload.Signal(7)
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_tuple_payload_event_construction_and_expanded_signal() {
    let source = r#"
Move:event(tuple(int, string)) = event(tuple(int, string)){}
Move.Signal(7, "go")
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_official_event_construction_with_tuple_payload_type() {
    let source = r#"
Move:event(tuple(int, string)) = event(tuple(int, string)){}
Notify():void = Move.Signal(7, "go")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (Some(0), Vec::<String>::new(), Some(vec![]), Type::None)
    );
}

#[test]
fn evaluates_official_race_ignores_unsignaled_event_await_pending_branch() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:int =
    Ready.Await()
    999
Immediate()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := race:
        WaitForReady()
        Immediate()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_awaiting_unsignaled_event_task_without_scheduler() {
    let source = r#"
Ready:event() = event(){}
WaitForReady()<suspends>:void =
    Ready.Await()
Task:task(void) = spawn{WaitForReady()}
Task.Await()
"#;

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("cannot complete without async scheduling support")
    );
}

#[test]
fn evaluates_official_event_signal_resumes_spawned_await_task() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:void =
    Ready.Await()
    set Result = 42
spawn{WaitForReady()}
Ready.Signal()
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_payload_resumes_await_binding() {
    let source = r#"
var Result:int = 0
Payload:event(int) = event(int){}
WaitForPayload()<suspends><transacts>:void =
    Value := Payload.Await()
    set Result = Value + 1
spawn{WaitForPayload()}
Payload.Signal(41)
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_resumes_waiters_fifo() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
First()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 1
Second()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 2
spawn{First()}
spawn{Second()}
Ready.Signal()
Result
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_does_not_cache_for_future_awaiters() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:void =
    Ready.Await()
    set Result = 42
Ready.Signal()
spawn{WaitForReady()}
Result
"#;

    assert_eq!(eval(source), Value::Number(0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_new_await_waits_for_next_signal() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitTwice()<suspends><transacts>:void =
    Ready.Await()
    set Result += 1
    Ready.Await()
    set Result += 10
spawn{WaitTwice()}
Ready.Signal()
AfterFirst:int = Result
Ready.Signal()
AfterFirst * 100 + Result
"#;

    assert_eq!(eval(source), Value::Number(111.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_event_construction_payload_type_mismatch() {
    let error = check_source("Bad:event(int) = event(string){}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `event(int)` but expression has type `event(string)`")
    );
}

#[test]
fn rejects_official_event_construction_with_body_entries() {
    let error =
        check_source("Bad:event(int) = event(int){Value := 1}").expect_err("source should fail");

    assert!(error.to_string().contains("expects an empty body"));
}

#[test]
fn rejects_official_event_construction_wrong_arity() {
    let error =
        check_source("Bad:event(int) = event(int, string){}").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 or 1 type arguments"));
}

#[test]
fn rejects_non_event_call_archetype_syntax() {
    let error = parse_source("Value := Foo(){}").expect_err("source should fail");

    assert!(
        error.to_string().contains("expected")
            || error.to_string().contains("unexpected")
            || error.to_string().contains("extra")
    );
}

#[test]
fn checks_official_awaitable_signalable_subscribable_members() {
    let source = r#"
Wait(Waitable:awaitable(int))<suspends>:int = Waitable.Await()
Notify(Signalable:signalable(int)):void = Signalable.Signal(42)
Handler(Value:int):void = {}
SubscribeTo(Source:subscribable(int)):cancelable = Source.Subscribe(Handler)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Subscribable(Some(Box::new(Type::Int)))]),
            Type::Interface("cancelable".into())
        )
    );
}

#[test]
fn evaluates_external_event_and_signalable_runtime_signal() {
    let source = r#"
Done:event() = external {}
Payload:event(int) = external {}
Signal:signalable(int) = external {}
Done.Signal()
Payload.Signal(7)
Signal.Signal(42)
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_subscribable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler(Value:int):void = {}
Source:subscribable(int) = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_parameterless_subscribable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler():void = {}
Source:subscribable() = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_listenable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler(Value:int):void = {}
Source:listenable(int) = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_external_subscribable_bad_callback() {
    let error = Interpreter::new()
        .eval_source("Source:subscribable(int) = external {}\nSource.Subscribe(42)")
        .expect_err("source should runtime error");

    assert!(
        error
            .to_string()
            .contains("`Subscribe` Callback expected function/1 -> void")
    );
}

#[test]
fn checks_official_listenable_exposed_awaitable_and_subscribable_members() {
    let source = r#"
Wait(Source:listenable(int))<suspends>:int = Source.Await()
Handler(Value:int):void = {}
SubscribeTo(Source:listenable(int)):cancelable = Source.Subscribe(Handler)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Listenable(Some(Box::new(Type::Int)))]),
            Type::Interface("cancelable".into())
        )
    );
}

#[test]
fn checks_official_event_subtype_of_awaitable_and_signalable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 1
AcceptSignalable(Source:signalable(int)):int = 2
Use(Event:event(int)):int = AcceptAwaitable(Event) + AcceptSignalable(Event)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(Some(Box::new(Type::Int)))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_listenable_subtype_of_awaitable_and_subscribable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 1
AcceptSubscribable(Source:subscribable(int)):int = 2
Use(Source:listenable(int)):int = AcceptAwaitable(Source) + AcceptSubscribable(Source)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Listenable(Some(Box::new(Type::Int)))]),
            Type::Int
        )
    );
}

#[test]
fn rejects_official_awaitable_await_outside_async_context() {
    let error = check_source("Wait(Waitable:awaitable(int)):int = Waitable.Await()")
        .expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_task_await_outside_async_context() {
    let error =
        check_source("Wait(Task:task(int)):int = Task.Await()").expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_event_signal_payload_type_mismatch() {
    let error = check_source(r#"Notify(Event:event(int)):void = Event.Signal("bad")"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected `int`"));
}

#[test]
fn rejects_official_event_signal_in_failure_context() {
    let error = check_source(
        r#"
Notify(Event:event(int))<decides><transacts>:void = Event.Signal(42)
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
fn rejects_official_subscribable_callback_type_mismatch() {
    let error = check_source(
        r#"
Bad(Value:string):void = {}
SubscribeTo(Source:subscribable(int)):cancelable = Source.Subscribe(Bad)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected"));
}

#[test]
fn rejects_official_awaitable_unknown_signal_member() {
    let error = check_source("Notify(Waitable:awaitable(int)):void = Waitable.Signal(42)")
        .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `Signal`"));
}

#[test]
fn rejects_official_task_unknown_signal_member() {
    let error = check_source("Notify(Task:task(int)):void = Task.Signal(42)")
        .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `Signal`"));
}

#[test]
fn rejects_official_spawn_of_non_suspends_call() {
    let error = check_source(
        r#"
Immediate():int = 42
Bad():task(int) = spawn{Immediate()}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`<suspends>` effect"));
}

#[test]
fn rejects_official_spawn_body_with_multiple_expressions() {
    let error = check_source(
        r#"
Compute()<suspends>:int = 42
Bad():task(int) = spawn{Compute(); Compute()}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("exactly one expression"));
}

#[test]
fn evaluates_official_spawn_expression_runtime_task() {
    let source = r#"
Compute()<suspends>:int = 42
spawn{Compute()}
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Task(Box::new(Type::Int))
    );

    assert!(matches!(eval(source), Value::Task(_)));
}

#[test]
fn evaluates_official_spawn_task_await_runtime() {
    let source = r#"
var Result:int = 0
Compute()<suspends>:int = 42
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Compute()}
    set Result = Task.Await()
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_official_sync_expression_tuple_result() {
    let source = r#"
ComputeScore()<suspends>:int = 42
ComputeLabel()<suspends>:string = "done"
Run()<suspends>:tuple(int, string) =
    sync:
        ComputeScore()
        ComputeLabel()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::String])
        )
    );
}

#[test]
fn checks_official_race_expression_result() {
    let source = r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run()<suspends>:int =
    race:
        Fast()
        Slow()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_rush_expression_result() {
    let source = r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run()<suspends>:int =
    rush:
        Fast()
        Slow()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_branch_expression_returns_void() {
    let source = r#"
Work()<suspends>:void = {}
Run()<suspends>:void =
    branch:
        Work()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::None
        )
    );
}

#[test]
fn rejects_official_structured_concurrency_outside_async_context() {
    let error = check_source(
        r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run():tuple(int, int) =
    sync:
        Fast()
        Slow()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_structured_concurrency_binding_after_body() {
    let error = check_source(
        r#"
F()<suspends>:int = 42
G()<suspends>:int = 0
H(Value:int):int = Value
Run()<suspends>:int =
    race:
        X := F()
        G()
    H(X)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `X`"));
}

#[test]
fn rejects_official_structured_concurrency_sibling_branch_binding_access() {
    for op in ["sync", "race", "rush", "branch"] {
        let source = format!(
            r#"
First()<suspends>:int = 1
Use(Value:int)<suspends>:int = Value
Run()<suspends>:void =
    {op}:
        X := First()
        Use(X)
"#
        );

        let error = check_source(&source).expect_err("source should fail");
        assert!(
            error.to_string().contains("undefined name `X`"),
            "{op} should isolate branch-local bindings, got {error}"
        );
    }
}

#[test]
fn rejects_official_structured_concurrency_immediate_branch() {
    for op in ["sync", "race", "rush", "branch"] {
        let source = format!(
            r#"
Wait()<suspends>:int = 1
Run()<suspends>:void =
    {op}:
        1
        Wait()
"#
        );

        let error = check_source(&source).expect_err("source should fail");
        assert!(
            error.to_string().contains("async expression"),
            "{op} should reject an immediate branch, got {error}"
        );
    }
}

#[test]
fn checks_official_structured_concurrency_async_block_branches() {
    let source = r#"
Wait()<suspends>:void = Sleep(-1.0)
Run()<suspends>:tuple(int, int) =
    sync:
        block:
            Wait()
            40
        block:
            Wait()
            2
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::Int])
        )
    );
}

#[test]
fn checks_official_structured_concurrency_branches_allow_same_local_name() {
    let sync_source = r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:tuple(int, int) =
    sync:
        X := First()
        X := Second()
"#;

    assert_eq!(
        function_shape(check_source(sync_source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::Int])
        )
    );

    for op in ["race", "rush"] {
        let source = format!(
            r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:int =
    {op}:
        X := First()
        X := Second()
"#
        );

        assert_eq!(
            function_shape(check_source(&source).expect("source should check")),
            (
                Some(0),
                vec!["suspends".to_string()],
                Some(vec![]),
                Type::Int
            )
        );
    }

    let branch_source = r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:void =
    branch:
        X := First()
        X := Second()
"#;

    assert_eq!(
        function_shape(check_source(branch_source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::None
        )
    );
}

#[test]
fn runtime_errors_on_structured_concurrency_sibling_branch_binding_access() {
    let source = r#"
First()<suspends>:int = 1
Run()<suspends>:int =
    Values := sync:
        X := First()
        X
    Values(1)
Task:task(int) = spawn{Run()}
Task.Await()
"#;

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should runtime error");

    assert!(error.to_string().contains("undefined name `X`"));
}

#[test]
fn evaluates_official_sync_expression_runtime_tuple_result() {
    let source = r#"
var Result:int = 0
ComputeScore()<suspends>:int = 42
ComputeLabel()<suspends>:string = "done"
Run()<suspends><transacts>:void =
    Values := sync:
        ComputeScore()
        ComputeLabel()
    set Result = Values(0)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_expression_waits_for_sleep_zero_branch() {
    let source = r#"
var Result:int = 0
Slow()<suspends>:int =
    Sleep(0.0)
    40
Fast()<suspends>:int = 2
Run()<suspends><transacts>:void =
    Values := sync:
        Slow()
        Fast()
    set Result = Values(0) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_expression_waits_for_positive_sleep_branch() {
    let source = r#"
var Result:int = 0
Slow()<suspends>:int =
    Sleep(0.001)
    40
Fast()<suspends>:int = 2
Run()<suspends><transacts>:void =
    Values := sync:
        Slow()
        Fast()
    set Result = Values(0) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_expression_starts_later_signal_branch() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Waiter()<suspends>:int =
    Ready.Await()
    40
Signaler()<suspends><transacts>:int =
    Ready.Signal()
    2
Run()<suspends><transacts>:void =
    Values := sync:
        Waiter()
        Signaler()
    set Result = Values(0) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_runtime_cancels_losing_immediate_branch() {
    let source = r#"
var Total:int = 0
Fast()<suspends><transacts>:int =
    set Total += 1
    1
Slow()<suspends><transacts>:int =
    set Total += 10
    2
Run()<suspends><transacts>:int =
    Winner := race:
        Fast()
        Slow()
    Winner + Total
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Number(2.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_waits_for_sleep_zero_winner_and_cancels_loser() {
    let source = r#"
var Trace:int = 0
Fast()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Slow()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 9
    2
Run()<suspends><transacts>:void =
    Winner := race:
        Fast()
        Slow()
    set Trace = Trace * 100 + Winner
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Number(140.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_waits_for_positive_sleep_winner_and_cancels_loser() {
    let source = r#"
var Trace:int = 0
Fast()<suspends><transacts>:int =
    Sleep(0.001)
    set Trace = Trace * 10 + 1
    40
Slow()<suspends><transacts>:int =
    Sleep(0.002)
    set Trace = Trace * 10 + 9
    2
Run()<suspends><transacts>:void =
    Winner := race:
        Fast()
        Slow()
    set Trace = Trace * 100 + Winner
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Number(140.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_event_signal_can_resume_winner() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Waiter()<suspends>:int =
    Ready.Await()
    40
Signaler()<suspends><transacts>:int =
    Sleep(0.0)
    Ready.Signal()
    2
Run()<suspends><transacts>:void =
    Winner := race:
        Waiter()
        Signaler()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancels_losing_sync_child_tasks() {
    let source = r#"
var Trace:int = 0
SlowChild()<suspends><transacts>:int =
    Sleep(0.001)
    set Trace = Trace * 10 + 9
    9
LosingSync()<suspends>:int =
    Values := sync:
        SlowChild()
        SlowChild()
    Values(0)
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingSync()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Number(140.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancels_losing_nested_race_child_tasks() {
    let source = r#"
var Trace:int = 0
NestedSlow()<suspends><transacts>:int =
    Sleep(0.001)
    set Trace = Trace * 10 + 9
    9
NestedNever()<suspends>:int =
    Sleep(Inf)
    7
LosingRace()<suspends>:int =
    race:
        NestedSlow()
        NestedNever()
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingRace()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Number(140.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancellation_runs_losing_branch_defer() {
    let source = r#"
var Trace:int = 0
Loser()<suspends><transacts>:int =
    defer:
        set Trace = Trace * 10 + 9
    Sleep(0.001)
    set Trace = Trace * 10 + 8
    2
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        Loser()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Number(1940.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancellation_runs_losing_sync_child_defers() {
    let source = r#"
var Trace:int = 0
SlowChild()<suspends><transacts>:int =
    defer:
        set Trace = Trace * 10 + 9
    Sleep(0.001)
    set Trace = Trace * 10 + 8
    8
LosingSync()<suspends><transacts>:int =
    Values := sync:
        SlowChild()
        SlowChild()
    Values(0)
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingSync()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Number(19940.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_runtime_continues_losing_immediate_branch() {
    let source = r#"
var Total:int = 0
Fast()<suspends><transacts>:int =
    set Total += 1
    1
Slow()<suspends><transacts>:int =
    set Total += 10
    2
Run()<suspends><transacts>:int =
    Winner := rush:
        Fast()
        Slow()
    Winner + Total
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_waits_for_sleep_zero_winner_and_continues_loser() {
    let source = r#"
var Trace:int = 0
Fast()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Slow()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 2
    2
Run()<suspends><transacts>:void =
    Winner := rush:
        Fast()
        Slow()
    set Trace = Trace * 10 + Winner
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Number(502.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_event_signal_can_resume_winner_and_continue_loser() {
    let source = r#"
var Trace:int = 0
Ready:event() = event(){}
Waiter()<suspends><transacts>:int =
    Ready.Await()
    set Trace = Trace * 10 + 1
    40
Signaler()<suspends><transacts>:int =
    Sleep(0.0)
    Ready.Signal()
    set Trace = Trace * 10 + 2
    2
Run()<suspends><transacts>:void =
    Winner := rush:
        Waiter()
        Signaler()
    set Trace = Trace * 10 + Winner
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Number(502.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_expression_runtime_returns_void_after_starting_body() {
    let source = r#"
var Total:int = 0
Work()<suspends><transacts>:void =
    set Total += 40
Run()<suspends><transacts>:int =
    branch:
        Work()
    Total + 2
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_sleep_zero_body_resumes_after_following_expression() {
    let source = r#"
var Result:int = 0
Work()<suspends><transacts>:void =
    Sleep(0.0)
    set Result = Result * 10 + 2
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = Result * 10 + 1
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_positive_sleep_body_resumes_later() {
    let source = r#"
var Result:int = 0
Work()<suspends><transacts>:void =
    Sleep(0.001)
    set Result = Result * 10 + 2
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = Result * 10 + 1
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_event_signal_resumes_started_body() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Work()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 2
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = Result * 10 + 1
    Ready.Signal()
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_unsignaled_event_does_not_block_following_expression() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Work()<suspends><transacts>:void =
    Ready.Await()
    set Result = 99
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = 42
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sleep_negative_runtime_immediate_completion() {
    let source = r#"
var Result:int = 0
Wait()<suspends><transacts>:void =
    Sleep(-1.0)
    set Result = 42
spawn{Wait()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sleep_zero_resumes_spawned_task_on_next_tick() {
    let source = r#"
var Result:int = 0
Wait()<suspends><transacts>:void =
    Sleep(0.0)
    set Result = 42
spawn{Wait()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sleep_zero_yields_each_time_until_next_tick() {
    let source = r#"
var Result:int = 0
WaitTwice()<suspends><transacts>:void =
    Sleep(0.0)
    set Result = 1
    Sleep(0.0)
    set Result = 2
spawn{WaitTwice()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterFirst * 10 + AfterSecond
"#;

    assert_eq!(eval(source), Value::Number(12.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_waits_for_sleep_zero_task_completion() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = Task.Await() + 1
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sleep_zero_task_await_runtime() {
    let source = r#"
var Result:int = 0
Wait()<suspends>:int =
    Sleep(0.0)
    42
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Wait()}
    set Result = Task.Await()
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_positive_sleep_resumes_spawned_task_after_duration() {
    let source = r#"
var Result:int = 0
Wait()<suspends><transacts>:void =
    Sleep(0.001)
    set Result = 42
spawn{Wait()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_waits_for_positive_sleep_task_completion() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    41
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = Task.Await() + 1
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_call_argument_after_multiple_yields() {
    let source = r#"
var Result:int = 0
AddOne(Value:int):int = Value + 1
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result = AddOne(Worker())
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_binary_operand_after_multiple_yields() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result = Worker() + 1
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_compound_assignment_rhs_after_multiple_yields() {
    let source = r#"
var Result:int = 1
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result += Worker()
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_call_argument_runtime() {
    let source = r#"
var Result:int = 0
AddOne(Value:int):int = Value + 1
Worker()<suspends>:int =
    Sleep(0.001)
    41
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = AddOne(Task.Await())
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_array_tuple_and_map_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    40
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    Values:[]int = array{Task.Await(), 2}
    if:
        First := Values[0]
        Second := Values[1]
        Scores:[string]int = map{"answer" => First}
        Answer := Scores["answer"]
    then:
        Pair:tuple(int, int) = (Answer, Second)
        set Result = Pair(0) + Pair(1)
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_map_key_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:string =
    Sleep(0.001)
    "answer"
Run()<suspends><transacts>:void =
    Task:task(string) = spawn{Worker()}
    Scores:[string]int = map{Task.Await() => 42}
    if (Score := Scores["answer"]):
        set Result = Score
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_bracket_argument_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    1
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    Values:[]int = array{0, 42}
    Index:int = Task.Await()
    if (Value := Values[Index]):
        set Result = Value
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_as_index_collection_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:[]int =
    Sleep(0.001)
    array{0, 42}
Run()<suspends><transacts>:void =
    Task:task([]int) = spawn{Worker()}
    Values:[]int = Task.Await()
    if (Value := Values[1]):
        set Result = Value
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_as_member_receiver_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:[]int =
    Sleep(0.001)
    array{40, 2}
Run()<suspends><transacts>:void =
    Task:task([]int) = spawn{Worker()}
    set Result = Task.Await().Length + 40
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_interpolated_string_runtime() {
    let source = r#"
var Result:string = ""
Worker()<suspends>:int =
    Sleep(0.001)
    42
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = "value {Task.Await()}"
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::String("value 42".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_official_race_ignores_sleep_inf_pending_branch() {
    let source = r#"
var Result:int = 0
Never()<suspends><transacts>:int =
    Sleep(Inf)
    999
Fast()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := race:
        Never()
        Fast()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_ignores_sleep_zero_pending_branch() {
    let source = r#"
var Result:int = 0
NextTick()<suspends><transacts>:int =
    Sleep(0.0)
    999
Immediate()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := race:
        NextTick()
        Immediate()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_ignores_positive_sleep_pending_branch() {
    let source = r#"
var Result:int = 0
Slow()<suspends><transacts>:int =
    Sleep(1.0)
    999
Immediate()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := race:
        Slow()
        Immediate()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_ignores_sleep_inf_pending_branch_for_winner() {
    let source = r#"
var Result:int = 0
Never()<suspends><transacts>:int =
    Sleep(Inf)
    999
Fast()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := rush:
        Never()
        Fast()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn runtime_errors_on_awaiting_sleep_inf_pending_task_without_scheduler() {
    let source = r#"
Never()<suspends>:void =
    Sleep(Inf)
Task:task(void) = spawn{Never()}
Task.Await()
"#;

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("cannot complete without async scheduling support")
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
fn rejects_official_event_parametric_type_wrong_arity() {
    let error =
        check_source("Value:event(int, string) = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `event` expected 0 or 1 type arguments")
    );
}

#[test]
fn rejects_official_task_parametric_type_wrong_arity() {
    let error = check_source("Value:task() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `task` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_generator_parametric_type_wrong_arity() {
    let error =
        check_source("Value:generator(int, string) = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `generator` expected 0 or 1 type arguments")
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
fn rejects_official_concrete_subtype_parametric_type_wrong_arity() {
    let error = check_source("Value:concrete_subtype(entity, entity) = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `concrete_subtype` expected 1 type arguments")
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
fn rejects_official_modifier_parametric_type_wrong_arity() {
    let error = check_source("Value:modifier() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `modifier` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_modifier_stack_parametric_type_wrong_arity() {
    let error = check_source("Value:modifier_stack(int, int) = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `modifier_stack` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_modifier_stack_add_modifier_type_mismatch() {
    let error = check_source(
        r#"
Stack:modifier_stack(int) = external {}
Modifier:modifier(string) = external {}
Stack.AddModifier(Modifier, 0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `modifier(int)`"));
}

#[test]
fn rejects_official_modifier_evaluate_type_mismatch() {
    let error = check_source(
        r#"
Modifier:modifier(int) = external {}
Modifier.Evaluate("bad")
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `int`"));
}

#[test]
fn rejects_official_modifier_stack_unknown_member() {
    let error = check_source(
        r#"
Stack:modifier_stack(int) = external {}
Stack.RemoveModifier()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `RemoveModifier`"));
}

#[test]
fn rejects_official_make_classifiable_subset_non_array_argument() {
    let error = check_source("MakeClassifiableSubset(42)").expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected `array`"));
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
    assert_eq!(eval(source), Value::Number(42.0));
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
    assert_eq!(eval(source), Value::Number(42.0));
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
fn rejects_official_generator_pair_iteration() {
    let error = check_source(
        r#"
Values:generator(int) = external {}
for (Index -> Value : Values):
    Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("pair iteration"));
}

#[test]
fn evaluates_external_official_generator_iteration_as_empty_runtime_generator() {
    let source = r#"
Values:generator(int) = external {}
Collected:[]int = for (Value : Values):
    Value
Collected.Length
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Number(0.0));
}

#[test]
fn evaluates_external_parameterless_generator_iteration_as_empty_runtime_generator() {
    let source = r#"
Values:generator() = external {}
Collected:[]any = for (Value : Values):
    Value
Collected.Length
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Number(0.0));
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
fn rejects_type_alias_conflicting_with_official_task_parametric_type() {
    let error = check_source("task := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `task` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_generator_parametric_type() {
    let error = check_source("generator := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `generator` conflicts with builtin type name")
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
fn rejects_type_alias_conflicting_with_official_modifier_parametric_type() {
    let error = check_source("modifier := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `modifier` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_modifier_stack_parametric_type() {
    let error = check_source("modifier_stack := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `modifier_stack` conflicts with builtin type name")
    );
}

#[test]
fn checks_official_player_subtype_of_agent() {
    let source = r#"
AcceptAgent(Agent:agent):int = 42
UsePlayer(Player:player):int = AcceptAgent(Player)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Class("player".into())]),
            Type::Int
        )
    );
}

#[test]
fn rejects_official_agent_assigned_to_player() {
    let error = check_source(
        r#"
UseAgent(Agent:agent):player = Agent
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("body has type `agent`"));
}

#[test]
fn checks_official_agent_and_player_subtype_of_entity() {
    let source = r#"
AcceptEntity(Entity:entity):int = 42
UseAgent(Agent:agent):int = AcceptEntity(Agent)
UsePlayer(Player:player):int = AcceptEntity(Player)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Class("player".into())]),
            Type::Int
        )
    );
}

#[test]
fn rejects_official_entity_assigned_to_agent() {
    let error = check_source(
        r#"
UseEntity(Entity:entity):agent = Entity
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("body has type `entity`"));
}

#[test]
fn user_defined_entity_does_not_accept_official_agent() {
    let error = check_source(
        r#"
entity := class:
    ID:int
AcceptEntity(Entity:entity):int = 42
UseAgent(Agent:agent):int = AcceptEntity(Agent)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1"));
    assert!(error.to_string().contains("expected `entity`"));
    assert!(error.to_string().contains("got `agent`"));
}

#[test]
fn user_defined_agent_does_not_accept_official_player() {
    let error = check_source(
        r#"
agent := class:
    ID:int
AcceptAgent(Agent:agent):int = 42
UsePlayer(Player:player):int = AcceptAgent(Player)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1"));
    assert!(error.to_string().contains("expected `agent`"));
    assert!(error.to_string().contains("got `player`"));
}

#[test]
fn checks_official_player_is_active_decides_method() {
    let source = r#"
CanUsePlayer(Player:player)<decides><transacts>:void =
    Player.IsActive[]
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["decides".to_string(), "transacts".to_string()],
            Some(vec![Type::Class("player".into())]),
            Type::None
        )
    );
}

#[test]
fn rejects_official_player_is_active_parenthesis_call() {
    let error = check_source(
        r#"
CanUsePlayer(Player:player):void =
    Player.IsActive()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("functions with `<decides>` must be called with `[]`")
    );
}

#[test]
fn rejects_official_player_is_active_arguments() {
    let error = check_source(
        r#"
CanUsePlayer(Player:player)<decides><transacts>:void =
    Player.IsActive[1]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn user_defined_player_does_not_get_official_is_active_method() {
    let source = r#"
player := class:
    ID:int
CanUsePlayer(Player:player)<decides><transacts>:void =
    Player.IsActive[]
"#;
    let error = check_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `player` has no member `IsActive`")
    );
}

#[test]
fn rejects_official_team_member_access() {
    let error = check_source("Use(Team:team):int = Team.Missing").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `team` has no member `Missing`")
    );
}

#[test]
fn rejects_official_team_value_type_mismatch() {
    let error = check_source("Team:team = false").expect_err("source should fail");

    assert!(error.to_string().contains("annotated as `team`"));
}

#[test]
fn evaluates_qualified_enum_value_declarations() {
    let source = r#"
Start:int = 0
game_state := enum:
    (game_state:)Start
    Playing

StateStart:game_state = game_state.Start
Start + if (StateStart = game_state.Start) {
    42
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_qualified_keyword_enum_value_declarations() {
    let source = r#"
keyword_enum := enum:
    (keyword_enum:)for
    Regular

Value:keyword_enum = keyword_enum.for
case (Value):
    keyword_enum.for => 42
    keyword_enum.Regular => 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_open_enum_values() {
    let source = r#"
weapon := enum<open>{Sword, Bow}
Current:weapon = weapon.Sword
Current = weapon.Sword
"#;

    assert_eq!(eval(source), Value::Bool(true));
}

#[test]
fn evaluates_exhaustive_enum_case_expressions() {
    let source = r#"
day := enum:
    Monday
    Tuesday
    Wednesday

GetDayType(D:day):string =
    case (D):
        day.Monday => "Weekday"
        day.Tuesday => "Weekday"
        day.Wednesday => "Weekday"

GetDayType(day.Monday)
"#;

    assert_eq!(eval(source), Value::String("Weekday".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_enum_case_wildcards() {
    let source = r#"
day := enum:
    Monday
    Tuesday
    Wednesday

GetDayType(D:day):string =
    case (D):
        day.Monday => "Week start"
        _ => "Midweek"

GetDayType(day.Wednesday)
"#;

    assert_eq!(eval(source), Value::String("Midweek".into()));
}

#[test]
fn evaluates_scalar_case_expressions() {
    let source = r#"
IntCase:int =
    case (2):
        1 => 10
        2 => 20
        _ => 0

LogicCase:int =
    case (false):
        true => 1
        false => 2

StringCase:int =
    case ("harvest"):
        "idle" => 1
        "harvest" => 10
        _ => 0

CharCase:int =
    case ('B'):
        'A' => 1
        'B' => 10
        _ => 0

IntCase + LogicCase + StringCase + CharCase
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_scalar_case_failure_contexts() {
    let source = r#"
Matched:?int = option{
    case (2):
        2 => 40
}
Missing:?int = option{
    case (3):
        2 => 0
}
if (Value := Matched?, not Missing?). Value + 2 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_scalar_case_in_if_failure_context() {
    let source = r#"
Pick(Value:int)<decides><transacts>:int =
    case (Value):
        7 => 42

if (Result := Pick[7]). Result else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_partial_enum_case_in_decides_function_for_matched_value() {
    let source = r#"
day := enum:
    Monday
    Tuesday

GetWeekStart(D:day)<decides><transacts>:string =
    case (D):
        day.Monday => "Week start"

if (WeekStart := GetWeekStart[day.Monday]). WeekStart else. ""
"#;

    assert_eq!(eval(source), Value::String("Week start".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_enum_map_keys_and_function_parameters() {
    let source = r#"
state := enum{Menu, Playing, Paused}
StateID:[state]int = map{
    state.Menu => 0,
    state.Playing => 1,
    state.Paused => 2,
}
IsPlaying(Value:state):logic = if (Value = state.Playing). true else. false
if (PausedID := StateID[state.Paused]). PausedID + if (IsPlaying(state.Playing)) { 40 } else { 0 } else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn rejects_non_exhaustive_closed_enum_case() {
    let error = check_source(
        r#"
day := enum:
    Monday
    Tuesday

case (day.Monday):
    day.Monday => 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("missing cases: Tuesday"));
}

#[test]
fn rejects_open_enum_case_without_wildcard() {
    let error = check_source(
        r#"
weapon := enum<open>:
    Sword
    Bow

case (weapon.Sword):
    weapon.Sword => 1
    weapon.Bow => 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("requires wildcard"));
}

#[test]
fn rejects_duplicate_enum_case_arm() {
    let error = check_source(
        r#"
day := enum:
    Monday

case (day.Monday):
    day.Monday => 1
    day.Monday => 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate case"));
}

#[test]
fn rejects_case_arm_after_wildcard() {
    let error = check_source(
        r#"
day := enum:
    Monday
    Tuesday

case (day.Monday):
    _ => 0
    day.Monday => 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("after wildcard"));
}

#[test]
fn rejects_scalar_case_without_wildcard_outside_failure_context() {
    let error = check_source(
        r#"
case (1):
    1 => 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("requires wildcard"));
}

#[test]
fn rejects_duplicate_scalar_case_arm() {
    let error = check_source(
        r#"
case (1):
    1 => 42
    1 => 0
    _ => -1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate case"));
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
fn evaluates_ignore_unreachable_case_attribute() {
    let source = r#"
status := enum:
    Active
    Inactive

case (status.Active):
    status.Active => 42
    @ignore_unreachable status.Active => 0
    @ignore_unreachable _ => -1
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unknown_case_arm_attribute() {
    let error = parse_source(
        r#"
status := enum:
    Active

case (status.Active):
    @other status.Active => 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown case arm attribute"));
}

#[test]
fn rejects_case_subject_with_unsupported_type() {
    let error = check_source(
        r#"
case (1.5):
    _ => 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("case subject must be `int`, `logic`, `string`, `char`, or enum")
    );
}

#[test]
fn rejects_unknown_enum_value() {
    let error = check_source(
        r#"
state := enum{Menu, Playing}
Bad:state = state.Paused
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("enum `state` has no value `Paused`")
    );
}

#[test]
fn rejects_mismatched_qualified_enum_value_declaration() {
    let error = check_source(
        r#"
state := enum:
    (other_state:)Menu
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must use enum name `state`"));
}

#[test]
fn rejects_enum_type_as_value() {
    let error = check_source(
        r#"
state := enum{Menu, Playing}
Bad:state = state
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("enum<state>"));
}

#[test]
fn rejects_local_enum_definition() {
    let error = check_source(
        r#"
{
    local_enum := enum{A}
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("module level"));
}

#[test]
fn rejects_enum_expression_not_direct_definition_rhs() {
    let error = check_source("enum{A}").expect_err("source should fail");

    assert!(error.to_string().contains("direct right-hand side"));
}

#[test]
fn evaluates_struct_defaults_construction_and_field_access() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

Origin := vector2{}
PlayerPos := vector2{X := 40, Y := 2}
Origin.X + PlayerPos.X + PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_colon_archetype_construction() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

PlayerPos := vector2:
    X := 40
    Y := 2

PlayerPos.X + PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_braced_archetype_semicolon_and_newline_separators() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

First := vector2{X := 20; Y := 1}
Second := vector2{
    X := 20
    Y := 1
}

First.X + First.Y + Second.X + Second.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_colon_archetype_comma_separators() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

PlayerPos := vector2:
    X := 40,
    Y := 2

PlayerPos.X + PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_dot_archetype_single_field_construction() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

PlayerPos := vector2 . X:=42
PlayerPos.X + PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_trailing_comma_in_archetype_construction() {
    let error = parse_source(
        r#"
vector2 := struct:
    X : int = 0

vector2{X := 1,}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("trailing comma"));
}

#[test]
fn evaluates_archetype_let_clauses_for_subsequent_fields() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

PlayerPos := vector2:
    let:
        Base:int = 40
        Offset := 2
    X := Base
    Y := Base + Offset

PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_archetype_let_binding_used_before_declaration() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

vector2:
    X := Base
    let:
        Base:int = 40
    Y := 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Base`"));
}

#[test]
fn rejects_archetype_let_binding_outside_archetype() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int = 0

vector2:
    let:
        Base:int = 40
    X := Base

Base
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Base`"));
}

#[test]
fn rejects_non_binding_archetype_let_entries() {
    let error = parse_source(
        r#"
vector2 := struct:
    X : int = 0

vector2:
    let:
        1
    X := 1
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected let binding name"));
}

#[test]
fn evaluates_struct_values_with_type_annotations_and_equality() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

Origin:vector2 = vector2{}
AlsoOrigin := vector2{}
if (Origin = AlsoOrigin) {
    42
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_struct_equality_with_non_comparable_field() {
    let error = check_source(
        r#"
Read():int = 42

callback_holder := struct:
    Callback:type{_():int}

Left := callback_holder{Callback := Read}
Right := callback_holder{Callback := Read}
if (Left = Right). 1 else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "equality struct `callback_holder` field `Callback` type `function/0 -> int` is not comparable"
    ));
}

#[test]
fn rejects_nested_struct_equality_with_non_comparable_field() {
    let error = check_source(
        r#"
Read():int = 42

inner_holder := struct:
    Callback:type{_():int}

outer_holder := struct:
    Inner:inner_holder

Left := outer_holder{Inner := inner_holder{Callback := Read}}
Right := outer_holder{Inner := inner_holder{Callback := Read}}
if (Left = Right). 1 else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "equality struct `outer_holder` field `Inner` type `inner_holder` is not comparable"
    ));
}

#[test]
fn evaluates_struct_fields_with_enum_and_option_defaults() {
    let source = r#"
damage_type := enum{Physical, Fire}
character := struct{}
damage_info := struct:
    Amount : int = 0
    Type : damage_type = damage_type.Physical
    Source : ?character = false

Info := damage_info{Amount := 42}
Info.Amount
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn rejects_unknown_struct_field() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int = 0

vector2{Y := 1}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("has no field `Y`"));
}

#[test]
fn rejects_missing_required_struct_field() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int
    Y : int = 0

vector2{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("missing required field `X`"));
}

#[test]
fn rejects_struct_field_type_mismatch() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int = 0

vector2{X := "bad"}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("field `X` expected"));
}

#[test]
fn rejects_local_struct_definition() {
    let error = check_source(
        r#"
{
    local_struct := struct{}
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("module level"));
}

#[test]
fn rejects_struct_expression_not_direct_definition_rhs() {
    let error = check_source("struct{}").expect_err("source should fail");

    assert!(error.to_string().contains("direct right-hand side"));
}

#[test]
fn evaluates_class_construction_and_field_access() {
    let source = r#"
player := class:
    Name : string
    Score : int = 0

Hero:player = player{Name := "Ava"}
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:0".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_private_class_members_inside_defining_class() {
    let source = r#"
counter := class:
    Value<private>:int = 40
    AddSecret<private>():int = Self.Value + 2
    Reveal():int = Self.AddSecret()

Counter := counter{}
Counter.Reveal()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_protected_class_field_inside_subclass() {
    let source = r#"
base_counter := class:
    Value<protected>:int = 40

child_counter := class(base_counter):
    Reveal():int = Self.Value + 2

Counter := child_counter{}
Counter.Reveal()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_private_class_field_access_outside_defining_class() {
    let error = check_source(
        r#"
counter := class:
    Value<private>:int = 42

Counter := counter{}
Counter.Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("private"));
}

#[test]
fn rejects_private_class_field_assignment_outside_defining_class() {
    let error = check_source(
        r#"
counter := class:
    var Value<private>:int = 0

Counter := counter{}
set Counter.Value = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("private"));
}

#[test]
fn rejects_private_class_method_call_outside_defining_class() {
    let error = check_source(
        r#"
counter := class:
    Hidden<private>():int = 42

Counter := counter{}
Counter.Hidden()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("private"));
}

#[test]
fn rejects_protected_class_field_access_outside_class_hierarchy() {
    let error = check_source(
        r#"
counter := class:
    Value<protected>:int = 42

Counter := counter{}
Counter.Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("protected"));
}

#[test]
fn rejects_private_base_class_field_access_from_subclass() {
    let error = check_source(
        r#"
base_counter := class:
    Value<private>:int = 40

child_counter := class(base_counter):
    Reveal():int = Self.Value + 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("private"));
}

#[test]
fn evaluates_class_colon_archetype_with_let_clause() {
    let source = r#"
player := class:
    Name : string
    Score : int = 0

Hero:player = player:
    let:
        Base:int = 40
    Name := "Ava"
    Score := Base + 2

Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_class_dot_archetype_single_field_construction() {
    let source = r#"
player := class:
    Name : string = "Ava"
    Score : int = 0

Hero := player . Score:=42
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_simple_constructor_function_returning_class_archetype() {
    let source = r#"
player := class:
    Name : string
    Score : int = 0

MakePlayer<constructor>(InName:string, InLevel:int)<transacts>:player =
    player:
        Name := InName
        Score := InLevel * 2

Hero := MakePlayer("Ava", 21)
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_public_constructor_initializing_internal_class_field() {
    let source = r#"
DataTypes<public> := module:
    countdown_timer<public> := class<concrete>:
        RemainingTime<internal>:float = 0.0
        GetRemainingTime<public>():float = RemainingTime

    MakeCountdownTimer<constructor><public>(MaxTime:float) := countdown_timer:
        RemainingTime := MaxTime

Timer := DataTypes.MakeCountdownTimer(42.0)
Timer.GetRemainingTime()
"#;

    assert_eq!(eval(source), Value::Float(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn rejects_constructor_initializing_private_class_field() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    countdown_timer<public> := class<concrete>:
        RemainingTime<private>:float = 0.0

    MakeCountdownTimer<constructor><public>(MaxTime:float) := countdown_timer:
        RemainingTime := MaxTime
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `RemainingTime` is private to class `DataTypes.countdown_timer`")
    );
}

#[test]
fn evaluates_protected_base_field_archetype_inside_subclass() {
    let source = r#"
base_counter := class:
    Value<protected>:int = 0
    Reveal():int = Value

child_counter := class(base_counter):
    RevealBase(NewValue:int):int =
        super{Value := NewValue}.Reveal()

child_counter{}.RevealBase(42)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn evaluates_constructor_delegation_to_same_class_constructor() {
    let source = r#"
player := class:
    Name:string
    Score:int

MakePlayer<constructor>(Name:string, Score:int) := player:
    Name := Name
    Score := Score

MakeNewPlayer<constructor>(Name:string) := player:
    Name := "ignored"
    Score := 999
    MakePlayer<constructor>(Name, 42)

Hero := MakeNewPlayer("Ava")
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
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
fn rejects_field_initializer_after_same_class_constructor_delegation() {
    let error = check_source(
        r#"
player := class:
    Name:string
    Score:int

MakePlayer<constructor>(Name:string) := player:
    Name := Name
    Score := 0

player:
    MakePlayer<constructor>("Ava")
    Score := 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field initializer `Score` cannot appear after constructor delegation")
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

    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail at runtime");

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
fn rejects_constructor_delegation_to_unrelated_class() {
    let error = check_source(
        r#"
badge := class:
    Label:string

MakeBadge<constructor>():badge =
    badge:
        Label := "VIP"

player := class:
    Name:string

player:
    Name := "Ava"
    MakeBadge<constructor>()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("not `player` or a superclass"));
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

    assert_eq!(eval(source), Value::Number(149.0));
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
fn evaluates_archetype_block_clause_before_subsequent_fields() {
    let source = r#"
var Seed:int = 0

player := class:
    Name : string
    Score : int = 0

Hero:player = player:
    block:
        set Seed += 40
    Name := "Ava"
    Score := Seed + 2

Hero.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_archetype_block_assignment_to_immutable_binding() {
    let error = check_source(
        r#"
Seed:int = 0

player := class:
    Name : string

player:
    block:
        set Seed += 1
    Name := "Ava"
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("immutable binding `Seed`"));
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
fn evaluates_class_mutable_fields_and_reference_semantics() {
    let source = r#"
player := class:
    Name : string
    var Score : int = 0

Hero := player{Name := "Ava"}
Alias := Hero
set Alias.Score = 10
set Hero.Score += 32
Alias.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_unique_class_identity_comparison() {
    let source = r#"
entity := class<unique>:
    Name : string

First := entity{Name := "same"}
Alias := First
Second := entity{Name := "same"}

if (First = Alias and First <> Second). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_non_unique_class_field_comparison() {
    let source = r#"
player := class:
    Name : string

First := player{Name := "same"}
Second := player{Name := "same"}

if (First = Second). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_unique_class_equality_with_non_comparable_field() {
    let error = check_source(
        r#"
Read():int = 42

handler := class:
    Callback:type{_():int}

Left := handler{Callback := Read}
Right := handler{Callback := Read}
if (Left = Right). 1 else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "equality class `handler` field `Callback` type `function/0 -> int` is not comparable"
    ));
}

#[test]
fn evaluates_unique_class_identity_comparison_with_non_comparable_field() {
    let source = r#"
Read():int = 42

handler := class<unique>:
    Callback:type{_():int}

Left := handler{Callback := Read}
Alias := Left
Right := handler{Callback := Read}

if (Left = Alias and Left <> Right). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_unique_class_values_as_map_keys() {
    let source = r#"
entity := class<unique>:
    Name : string

First := entity{Name := "same"}
Second := entity{Name := "same"}
Scores:[entity]int = map{First => 20, Second => 22}
if:
    FirstScore := Scores[First]
    SecondScore := Scores[Second]
then:
    FirstScore + SecondScore
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
fn rejects_non_unique_class_values_as_map_keys() {
    let error = check_source(
        r#"
player := class:
    Name : string

Hero := player{Name := "Ava"}
Scores:[player]int = map{Hero => 42}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("not comparable"));
}

#[test]
fn rejects_duplicate_class_specifier() {
    let error = parse_source(
        r#"
entity := class<unique><unique>:
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate class specifier"));
}

#[test]
fn rejects_duplicate_class_access_specifier() {
    let error = parse_source(
        r#"
entity := class<public><public>:
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate class specifier"));
}

#[test]
fn rejects_unsupported_class_specifier() {
    let error = parse_source(
        r#"
entity := class<native>:
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unsupported class specifier"));
}

#[test]
fn evaluates_persistable_final_class_in_player_weak_map() {
    let source = r#"
player := class<unique>:
    ID:int = 0

player_profile_data := class<final><persistable>:
    Version:int = 0
    XP:int = 0
    QuestHistory:[]string = array{}

Alice := player{ID := 1}
var Profiles:weak_map(player, player_profile_data) = map{}
if:
    set Profiles[Alice] = player_profile_data{XP := 42}
    Profile := Profiles[Alice]
then:
    Profile.XP
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
fn rejects_persistable_class_without_final() {
    let error = check_source(
        r#"
player_profile_data := class<persistable>:
    XP:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("persistable class `player_profile_data` must also be `final`")
    );
}

#[test]
fn rejects_persistable_unique_class() {
    let error = check_source(
        r#"
player_profile_data := class<final><unique><persistable>:
    XP:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("persistable class `player_profile_data` cannot be `unique`")
    );
}

#[test]
fn rejects_persistable_class_with_superclass() {
    let error = check_source(
        r#"
base_profile := class:
    Version:int = 0

player_profile_data := class<final><persistable>(base_profile):
    XP:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("persistable class `player_profile_data` cannot have a superclass")
    );
}

#[test]
fn rejects_persistable_class_var_field() {
    let error = check_source(
        r#"
player_profile_data := class<final><persistable>:
    var XP:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("field `XP` cannot be variable"));
}

#[test]
fn rejects_persistable_class_non_persistable_field() {
    let error = check_source(
        r#"
token := class<unique>:
    ID:int = 0

player_profile_data := class<final><persistable>:
    Token:token
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Token` has non-persistable type `token`")
    );
}

#[test]
fn evaluates_persistable_enum_in_player_weak_map() {
    let source = r#"
player := class<unique>:
    ID:int = 0

rank := enum<persistable>{Bronze, Silver, Gold}
Alice := player{ID := 1}
var SavedRank:weak_map(player, rank) = map{}
if:
    set SavedRank[Alice] = rank.Gold
then:
    {}
else:
    {}
if (SavedRank[Alice] = rank.Gold):
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
fn evaluates_persistable_struct_in_player_weak_map() {
    let source = r#"
player := class<unique>:
    ID:int = 0

rank := enum<persistable>{Bronze, Silver, Gold}
profile_snapshot := struct<persistable>:
    Level:int
    Rank:rank

Alice := player{ID := 1}
var Snapshots:weak_map(player, profile_snapshot) = map{}
if:
    set Snapshots[Alice] = profile_snapshot{Level := 42, Rank := rank.Gold}
    Snapshot := Snapshots[Alice]
then:
    Snapshot.Level
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
fn rejects_non_persistable_struct_in_player_weak_map() {
    let error = check_source(
        r#"
profile_snapshot := struct:
    Level:int

var Snapshots:weak_map(player, profile_snapshot) = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("weak_map(player, ...) value type `profile_snapshot` must be persistable")
    );
}

#[test]
fn rejects_persistable_struct_non_persistable_field() {
    let error = check_source(
        r#"
token := class<unique>:
    ID:int = 0

profile_snapshot := struct<persistable>:
    Token:token
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "persistable struct `profile_snapshot` field `Token` has non-persistable type `token`"
    ));
}

#[test]
fn rejects_duplicate_struct_persistable_specifier() {
    let error = parse_source(
        r#"
profile_snapshot := struct<persistable><persistable>:
    Level:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate struct specifier `persistable`")
    );
}

#[test]
fn rejects_duplicate_enum_persistable_specifier() {
    let error = parse_source(
        r#"
rank := enum<persistable><persistable>{Bronze, Silver}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate enum specifier `persistable`")
    );
}

#[test]
fn evaluates_abstract_class_as_base() {
    let source = r#"
entity := class<abstract>:
    Name:string

player := class(entity):
    Score:int = 42

Hero := player{Name := "Ava"}
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_epic_internal_class_type_without_instantiation() {
    let source = r#"
internal_device := class<epic_internal>:
    ID:int = 0

str(internal_device)
"#;

    assert_eq!(
        eval(source),
        Value::String("<class internal_device>".into())
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_epic_internal_class_archetype() {
    let error = check_source(
        r#"
internal_device := class<epic_internal>:
    ID:int = 0

internal_device{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("epic_internal class `internal_device` cannot be instantiated")
    );
}

#[test]
fn runtime_errors_on_epic_internal_class_archetype() {
    let source = r#"
internal_device := class<epic_internal>:
    ID:int = 0

internal_device{}
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("epic_internal class `internal_device` cannot be instantiated")
    );
}

#[test]
fn rejects_abstract_class_archetype() {
    let error = check_source(
        r#"
entity := class<abstract>:
    Name:string = "Ava"

entity{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("abstract class `entity` cannot be instantiated")
    );
}

#[test]
fn rejects_abstract_concrete_class() {
    let error = check_source(
        r#"
settings := class<abstract><concrete>:
    Name:string = "Ava"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot be both `abstract` and `concrete`")
    );
}

#[test]
fn accepts_official_abstract_method_declaration() {
    let source = r#"
command := class<computes><unique><abstract>:
    DebugString():string
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::ClassType("command".into())
    );
}

#[test]
fn evaluates_abstract_method_implementation() {
    let source = r#"
command := class<abstract>:
    DebugString():string

move_command := class(command):
    DebugString<override>():string = "move"

Item := move_command{}
Item.DebugString()
"#;

    assert_eq!(eval(source), Value::String("move".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_abstract_method_in_concrete_class() {
    let error = check_source(
        r#"
command := class:
    DebugString():string
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `command` must be `abstract` or implement method `DebugString`")
    );
}

#[test]
fn rejects_concrete_subclass_missing_abstract_method() {
    let error = check_source(
        r#"
command := class<abstract>:
    DebugString():string

move_command := class(command):
    Steps:int = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `move_command` must be `abstract` or implement method `DebugString`")
    );
}

#[test]
fn evaluates_abstract_subclass_inherits_abstract_method() {
    let source = r#"
command := class<abstract>:
    DebugString():string

movement_command := class<abstract>(command):
    Steps:int = 1

move_command := class(movement_command):
    DebugString<override>():string = "move"

Item := move_command{}
Item.DebugString()
"#;

    assert_eq!(eval(source), Value::String("move".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_abstract_method_without_return_type() {
    let error = check_source(
        r#"
command := class<abstract>:
    DebugString()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("abstract class method `DebugString` requires an explicit return type")
    );
}

#[test]
fn rejects_final_abstract_class_method() {
    let error = check_source(
        r#"
command := class<abstract>:
    DebugString<final>():string
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("abstract class method `DebugString` cannot be `final`")
    );
}

#[test]
fn evaluates_final_class_archetype() {
    let source = r#"
entity := class<final>:
    Name:string = "Ava"

entity{}.Name
"#;

    assert_eq!(eval(source), Value::String("Ava".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_final_class_inheritance() {
    let error = check_source(
        r#"
entity := class<final>:
    Name:string

player := class(entity):
    Score:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `entity` is `final` and cannot be inherited")
    );
}

#[test]
fn evaluates_final_class_field_inheritance() {
    let source = r#"
entity := class:
    ID<final>:int = 40

player := class(entity):
    Score:int = 2

Hero := player{}
Hero.ID + Hero.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_final_class_field_without_default() {
    let error = check_source(
        r#"
entity := class:
    ID<final>:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("final field `ID` must have a default value")
    );
}

#[test]
fn rejects_final_class_field_override() {
    let error = check_source(
        r#"
entity := class:
    ID<final>:int = 1

player := class(entity):
    ID<override>:int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `ID` overrides final inherited field `ID`")
    );
}

#[test]
fn rejects_final_class_field_archetype_override() {
    let error = check_source(
        r#"
entity := class:
    ID<final>:int = 1

entity{ID := 2}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("final field `ID` cannot be overridden by an archetype")
    );
}

#[test]
fn runtime_errors_on_final_class_field_archetype_override() {
    let source = r#"
entity := class:
    ID<final>:int = 1

entity{ID := 2}
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("final field `ID` cannot be overridden by an archetype")
    );
}

#[test]
fn evaluates_final_class_method_inheritance() {
    let source = r#"
entity := class:
    Score<final>():int = 40

player := class(entity):
    Bonus():int = 2

Hero := player{}
Hero.Score() + Hero.Bonus()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_final_class_method_override() {
    let error = check_source(
        r#"
entity := class:
    Score<final>():int = 1

player := class(entity):
    Score<override>():int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("method `Score` overrides final inherited method `Score`")
    );
}

#[test]
fn rejects_local_final_function_definition() {
    let error = check_source(
        r#"
Make():int =
    Helper<final>():int = 42
    Helper()

Make()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`final` specifier is not allowed on local definitions")
    );
}

#[test]
fn evaluates_concrete_class_empty_archetype() {
    let source = r#"
settings := class<concrete>:
    Name:string = "Ava"
    Score:int = 42

Item := settings{}
Item.Name + ":" + str(Item.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_concrete_class_with_empty_base_parens() {
    let source = r#"
settings := class<concrete>():
    Score:int = 42

settings{}.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_concrete_subclass_overriding_required_field_with_default() {
    let source = r#"
entity := class:
    Name:string

player := class<concrete>(entity):
    Name<override>:string = "Ava"
    Score:int = 42

Hero := player{}
Hero.Name + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_concrete_class_field_without_default() {
    let error = check_source(
        r#"
settings := class<concrete>:
    Name:string
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("must have a default value"));
}

#[test]
fn rejects_concrete_subclass_inherited_field_without_default() {
    let error = check_source(
        r#"
entity := class:
    Name:string

player := class<concrete>(entity):
    Score:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("field `Name`"));
    assert!(error.to_string().contains("must have a default value"));
}

#[test]
fn evaluates_class_inheritance_fields() {
    let source = r#"
entity := class:
    Name : string
    var Score : int = 0

player := class(entity):
    Team : string = "red"

Hero:player = player{Name := "Ava", Score := 10}
set Hero.Score += 32
Hero.Name + ":" + Hero.Team + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:red:42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_class_field_overrides() {
    let source = r#"
node := class:
    Data : any
    var Next : ?node = false

int_node := class(node):
    Data<override> : int
    var Next<override> : ?int_node = false

Head := int_node{Data := 40}
Tail := int_node{Data := 2}
set Head.Next = option{Tail}
Head.Data + if (Next := Head.Next?). Next.Data else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_editable_class_field_attributes() {
    let source = r#"
settings := class:
    @editable
    BasicInt:int = 40

    @editable
    Bonus<public>:int = 2

Item := settings{}
Item.BasicInt + Item.Bonus
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_inline_editable_class_field_attribute() {
    let source = r#"
settings := class:
    @editable BasicInt:int = 42

settings{}.BasicInt
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_editable_class_field_attribute_with_braced_arguments() {
    let source = r#"
Tip<localizes>:message = "Displayed in editor."

settings := class:
    @editable {ToolTip := Tip} BasicInt:int = 42

settings{}.BasicInt
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_editable_class_field_attribute_with_colon_arguments() {
    let source = r#"
Tip<localizes>:message = "Displayed in editor."
Category<localizes>:message = "General"

settings := class:
    @editable:
        ToolTip := Tip
        Categories := array{Category}
    BasicInt:int = 42

settings{}.BasicInt
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_duplicate_editable_class_field_attribute_argument() {
    let error = parse_source(
        r#"
Tip<localizes>:message = "Displayed in editor."

settings := class:
    @editable {ToolTip := Tip, ToolTip := Tip}
    BasicInt:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate field attribute argument")
    );
}

#[test]
fn rejects_empty_editable_class_field_attribute_braces() {
    let error = parse_source(
        r#"
settings := class:
    @editable {}
    BasicInt:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("require at least one argument"));
}

#[test]
fn rejects_unknown_editable_class_field_attribute_argument_reference() {
    let error = check_source(
        r#"
settings := class:
    @editable {ToolTip := MissingTip}
    BasicInt:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `MissingTip`"));
}

#[test]
fn rejects_unknown_class_field_attribute() {
    let error = parse_source(
        r#"
settings := class:
    @visible
    BasicInt:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown field attribute"));
}

#[test]
fn rejects_duplicate_class_field_attribute() {
    let error = parse_source(
        r#"
settings := class:
    @editable
    @editable
    BasicInt:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("duplicate field attribute"));
}

#[test]
fn rejects_class_field_attribute_on_method() {
    let error = parse_source(
        r#"
settings := class:
    @editable
    BasicInt():int =
        42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field attributes cannot apply to methods")
    );
}

#[test]
fn evaluates_subclass_assignment_to_base_class() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

ReadID(Item:entity):int = Item.ID

Base:entity = boss{ID := 40, Threat := 2}
ReadID(Base)
"#;

    assert_eq!(eval(source), Value::Number(40.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Number(40.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_implementer_argument_runtime_type_check() {
    let source = r#"
moveable := interface:
    MoveForward():int

rideable := interface(moveable):
    Mount():int

horse := class(rideable):
    MoveForward<override>():int = 42
    Mount<override>():int = 0

Use(Thing:moveable):int = Thing.MoveForward()

Use(horse{})
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_type_cast_to_actual_subclass() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

AsEntity()<computes>:entity = boss{ID := 1, Threat := 42}

if (Boss := boss[AsEntity()]):
    Boss.Threat
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
fn rejects_no_rollback_call_in_class_cast_failure_context() {
    let error = check_source(
        r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

AsEntity():entity = boss{ID := 1, Threat := 42}

if (Boss := boss[AsEntity()]):
    Boss.Threat
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
fn evaluates_failed_class_type_cast_in_failure_context() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

if (Boss := boss[entity{ID := 1}]):
    Boss.Threat
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_failed_class_type_cast_in_option_literal() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

Maybe := option{boss[entity{ID := 1}]}
if (Maybe?):
    0
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failed_class_type_cast_outside_failure_context() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

boss[entity{ID := 1}]
"#;
    assert_failable_context_error(source);
}

#[test]
fn rejects_unrelated_class_type_cast() {
    let error = check_source(
        r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

vehicle := class:
    Wheels : int

boss[vehicle{Wheels := 4}]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unrelated class"));
}

#[test]
fn rejects_class_field_override_without_specifier() {
    let error = check_source(
        r#"
base := class:
    Data : any

child := class(base):
    Data : int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate inherited class field")
    );
}

#[test]
fn rejects_class_field_override_without_inherited_field() {
    let error = check_source(
        r#"
base := class:
    Data : any

child := class(base):
    Other<override> : int
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("does not override"));
}

#[test]
fn rejects_duplicate_class_field_specifier() {
    let error = parse_source(
        r#"
base := class:
    Data : any

child := class(base):
    Data<override><override> : int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate class field specifier")
    );
}

#[test]
fn rejects_localizes_class_field_specifier_without_message_annotation() {
    let error = check_source(
        r#"
text_base := class:
    DefaultText<localizes>:string = "Hello"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`localizes` field specifier requires a `message` annotation")
    );
}

#[test]
fn rejects_unsupported_class_field_specifier() {
    let error = parse_source(
        r#"
player := class:
    Data<unique> : int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported class field specifier")
    );
}

#[test]
fn evaluates_class_methods_with_field_access_and_mutation() {
    let source = r#"
player := class:
    Name : string
    var Score : int = 0

    AddScore(Points:int)<transacts>:void =
        set Score += Points

    ScorePlus(Bonus:int):int =
        Score + Bonus

Hero := player{Name := "Ava"}
Hero.AddScore(40)
Hero.ScorePlus(2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_inherited_class_methods() {
    let source = r#"
entity := class:
    var Active : logic = false

    Activate()<transacts>:void =
        set Active = true

player := class(entity):
    Name : string

Hero := player{Name := "Ava"}
Hero.Activate()
if (Hero.Active?) { 42 } else { 0 }
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_override_class_methods() {
    let source = r#"
entity := class:
    Label():string =
        "entity"

player := class(entity):
    Label<override>():string =
        "player"

Hero := player{}
Hero.Label()
"#;

    assert_eq!(eval(source), Value::String("player".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_base_typed_receiver_dispatches_override_method() {
    let source = r#"
entity := class:
    Label():string =
        "entity"

player := class(entity):
    Label<override>():string =
        "player"

Hero:entity = player{}
Hero.Label()
"#;

    assert_eq!(eval(source), Value::String("player".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_base_typed_receiver_inherited_method_uses_virtual_self_dispatch() {
    let source = r#"
base := class:
    GetValue():int =
        10

    ComputeDouble():int =
        2 * Self.GetValue()

derived := class(base):
    GetValue<override>():int =
        21

Item:base = derived{}
Item.ComputeDouble()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_super_archetype_in_class_methods() {
    let source = r#"
entity := class:
    ID : int
    Name : string

    Label():string =
        str(ID) + ":" + Name

character := class(entity):
    Health : int

    Label<override>():string =
        super{ID := ID, Name := Name}.Label() + ":" + str(Health)

Hero := character{ID := 7, Name := "Ava", Health := 35}
Hero.Label()
"#;

    assert_eq!(eval(source), Value::String("7:Ava:35".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_super_archetype_in_class_blocks() {
    let source = r#"
entity := class:
    Base : int

character := class(entity):
    var Derived : int = 0

    block:
        set Derived = super{Base := Base}.Base + 40

Hero := character{Base := 2}
Hero.Derived
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_super_without_class_base() {
    let error = check_source(
        r#"
player := class:
    Score():int =
        super{}.Score
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `super`"));
}

#[test]
fn evaluates_super_qualified_parent_method_calls() {
    let source = r#"
counter := class:
    var Value : int = 0

    Add(Amount:int)<transacts>:void =
        set Value += Amount

tracked_counter := class(counter):
    var Calls : int = 0

    Add<override>(Amount:int)<transacts>:void =
        (super:)Add(Amount)
        set Calls += 1

Item := tracked_counter{Value := 40}
Item.Add(2)
Item.Value + Item.Calls
"#;

    assert_eq!(eval(source), Value::Number(43.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_super_parent_method_with_virtual_self_dispatch() {
    let source = r#"
base := class:
    GetValue():int =
        10

    ComputeDouble():int =
        2 * Self.GetValue()

derived := class(base):
    GetValue<override>():int =
        20

    ComputeDouble<override>():int =
        (super:)ComputeDouble()

derived{}.ComputeDouble()
"#;

    assert_eq!(eval(source), Value::Number(40.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unknown_super_qualified_parent_method() {
    let error = check_source(
        r#"
base := class:
    Value:int = 0

derived := class(base):
    Missing():int =
        (super:)Missing()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `base` has no method `Missing`")
    );
}

#[test]
fn evaluates_self_field_access_and_method_calls() {
    let source = r#"
player := class:
    var Score : int = 0

    AddScore(Points:int)<transacts>:void =
        set Self.Score += Points

    ApplyBonus(Bonus:int)<transacts>:void =
        Self.AddScore(Bonus)

    ScorePlus(Bonus:int):int =
        Self.Score + Bonus

Hero := player{}
Hero.AddScore(20)
Hero.ApplyBonus(20)
Hero.ScorePlus(2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_self_return_for_method_chaining() {
    let source = r#"
player := class:
    var Score : int = 0

    AddScore(Points:int)<transacts>:player =
        set Score += Points
        Self

Hero := player{}
Hero.AddScore(20).AddScore(22).Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_self_passed_to_top_level_function() {
    let source = r#"
ReadScore(Player:player):int = Player.Score

player := class:
    Score : int = 42

    Read():int =
        ReadScore(Self)

Hero := player{}
Hero.Read()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_self_outside_class_methods() {
    let error = check_source("Self").expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Self`"));
}

#[test]
fn evaluates_class_block_initialization() {
    let source = r#"
player := class:
    Name : string
    var Score : int = 0
    var Label : string = ""

    block:
        set Score += 40
        set Label = Self.Name + ":" + str(Self.Score)

Hero := player{Name := "Ava"}
Hero.Label + ":" + str(Hero.Score)
"#;

    assert_eq!(eval(source), Value::String("Ava:40:40".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_multiple_class_blocks_in_order() {
    let source = r#"
steps := class:
    var Step1 : int = 0
    var Step2 : int = 0
    var Step3 : int = 0

    block:
        set Step1 = 10

    block:
        set Step2 = Step1 + 5
        set Step3 = Step2 * 2

Value := steps{}
Value.Step1 + Value.Step2 + Value.Step3 - 13
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_block_method_calls() {
    let source = r#"
player := class:
    var Score : int = 0

    AddScore(Points:int)<transacts>:void =
        set Self.Score += Points

    block:
        Self.AddScore(42)

Hero := player{}
Hero.Score
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failable_index_inside_class_block() {
    let error = check_source(
        r#"
player := class:
    var Score:int = 0
    Items:[]int = array{1}

    block:
        set Score = Items[0]
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class block cannot contain failable expressions")
    );
}

#[test]
fn rejects_direct_suspends_call_inside_class_block() {
    let error = check_source(
        r#"
Wait()<suspends>:void = {}

player := class:
    var Score:int = 0

    block:
        Wait()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("can only be called in an async context")
    );
}

#[test]
fn evaluates_class_method_overloads_by_parameter_type() {
    let source = r#"
formatter := class:
    Score(Value:int):int =
        Value + 1

    Score(Value:string):int =
        2

    Total():int =
        Score(39) + Score("bonus")

Formatter := formatter{}
Formatter.Total()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subclass_method_overload_added_to_parent_group() {
    let source = r#"
c0 := class:
    F(X:int):int =
        X + 39

c1 := class(c0):
    F(X:float):int =
        2

Value := c1{}
Value.F(1) + Value.F(1.0)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subclass_method_override_of_one_overload() {
    let source = r#"
c0 := class:
    F(X:int):int =
        1

    F(X:string):int =
        2

c1 := class(c0):
    F<override>(X:int):int =
        40

Value := c1{}
Value.F(0) + Value.F("bonus")
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_super_call_to_overloaded_parent_method() {
    let source = r#"
c0 := class:
    F(X:int):int =
        X + 39

    F(X:string):int =
        2

c1 := class(c0):
    F<override>(X:int):int =
        (super:)F(X)

    Total():int =
        F(1) + (super:)F("bonus")

Value := c1{}
Value.Total()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_overloaded_methods() {
    let source = r#"
formatter := interface:
    Format(X:int):string
    Format(X:string):string

entity := class(formatter):
    Format<override>(X:int):string =
        "I" + str(X)

    Format<override>(X:string):string =
        "S" + X

Entity:formatter = entity{}
Entity.Format(4) + Entity.Format("2")
"#;

    assert_eq!(eval(source), Value::String("I4S2".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_class_qualified_method_call() {
    let source = r#"
c := class:
    (c:)F(X:int):int =
        X + 2

Value := c{}
Value.(c:)F(40)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_qualified_method_call_from_unqualified_override() {
    let source = r#"
rideable := interface:
    Mount():int

bicycle := class(rideable):
    Mount<override>():int =
        42

Ride := bicycle{}
Ride.(rideable:)Mount()
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_collision_qualified_methods() {
    let source = r#"
i := interface:
    B(X:int):int

j := interface:
    B(X:int):int

collision := class(i, j):
    (i:)B<override>(X:int):int =
        20 + X

    (j:)B<override>(X:int):int =
        30 + X

Obj := collision{}
Obj.(i:)B(1) + Obj.(j:)B(1)
"#;

    assert_eq!(eval(source), Value::Number(52.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_capture_of_overloaded_method_name() {
    let error = check_source(
        r#"
formatter := class:
    Format(X:int):int =
        X

    Format(X:string):int =
        1

Formatter := formatter{}
Captured := Formatter.Format
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("overloaded method `Format` must be called")
    );
}

#[test]
fn rejects_unknown_qualified_method_call() {
    let error = check_source(
        r#"
c := class:
    (c:)F():int =
        1

Value := c{}
Value.(missing:)F()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `c` has no method `(missing:)F`")
    );
}

#[test]
fn rejects_ambiguous_unqualified_override_for_interface_collision() {
    let error = check_source(
        r#"
i := interface:
    B(X:int):int

j := interface:
    B(X:int):int

collision := class(i, j):
    B<override>(X:int):int =
        X
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("override is ambiguous; use a qualified method name")
    );
}

#[test]
fn rejects_duplicate_class_method_overload_signature() {
    let error = check_source(
        r#"
formatter := class:
    Score(Value:int):int =
        Value

    Score(Other:int):int =
        Other + 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate class method overload `Score`")
    );
}

#[test]
fn rejects_duplicate_interface_method_overload_signature() {
    let error = check_source(
        r#"
formatter := interface:
    Format(Value:int):string
    Format(Other:int):string
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate interface method overload `Format`")
    );
}

#[test]
fn rejects_class_method_overload_option_logic_distinctness() {
    let error = check_source(
        r#"
formatter := class:
    Format(Value:?int):int =
        1

    Format(Value:logic):int =
        2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate class method overload `Format`")
    );
}

#[test]
fn rejects_interface_method_overload_function_parameter_distinctness() {
    let error = check_source(
        r#"
formatter := interface:
    Format(Value:type{_(:int):int}):int
    Format(Value:type{_(:string):int}):int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate interface method overload `Format`")
    );
}

#[test]
fn rejects_override_for_distinct_subclass_method_overload() {
    let error = check_source(
        r#"
c0 := class:
    F(X:int):int =
        X

c1 := class(c0):
    F<override>(X:string):int =
        1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("does not override an inherited method")
    );
}

#[test]
fn rejects_class_missing_one_interface_method_overload() {
    let error = check_source(
        r#"
formatter := interface:
    Format(X:int):string
    Format(X:string):string

entity := class(formatter):
    Format<override>(X:int):string =
        str(X)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("must be `abstract` or implement method `Format`")
    );
}

#[test]
fn rejects_class_block_assignment_to_immutable_field() {
    let error = check_source(
        r#"
player := class:
    Name : string

    block:
        set Name = "Mira"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable binding `Name`")
    );
}

#[test]
fn evaluates_decides_class_method_bracket_calls() {
    let source = r#"
player := class:
    Score : int = 42

    Pick()<decides><transacts>:int =
        Score

Hero := player{}
if (Value := Hero.Pick[]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_decides_class_method_failure_in_failure_context() {
    let source = r#"
player := class:
    Values : []int = array{42}

    Pick(Index:int)<decides><transacts>:int =
        Values[Index]

Hero := player{}
Found := if (Value := Hero.Pick[0]). Value else. 0
Missing := if (Value := Hero.Pick[1]). Value else. 0
Captured:?int = option{Hero.Pick[1]}
Found + Missing + if (Value := Captured?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_decides_class_method_parenthesis_calls() {
    let error = check_source(
        r#"
player := class:
    Score : int = 42

    Pick()<decides><transacts>:int =
        Score

Hero := player{}
Hero.Pick()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("functions with `<decides>` must be called with `[]`")
    );
}

#[test]
fn rejects_duplicate_inherited_class_method_without_override() {
    let error = check_source(
        r#"
entity := class:
    Label():string =
        "entity"

player := class(entity):
    Label():string =
        "player"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate inherited class method `Label`")
    );
}

#[test]
fn rejects_class_method_override_without_inherited_method() {
    let error = check_source(
        r#"
player := class:
    Label<override>():string =
        "player"
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("does not override an inherited method")
    );
}

#[test]
fn rejects_unknown_class_method_call() {
    let error = check_source(
        r#"
player := class:
    Name : string

Hero := player{Name := "Ava"}
Hero.Missing()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `player` has no member `Missing`")
    );
}

#[test]
fn rejects_class_method_assignment_to_immutable_field() {
    let error = check_source(
        r#"
player := class:
    Name : string

    Rename(NewName:string):void =
        set Name = NewName
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable binding `Name`")
    );
}

#[test]
fn rejects_class_parent_that_is_not_class_or_interface() {
    let error = check_source(
        r#"
vector2 := struct:
    X : int = 0

bad := class(vector2):
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class parent must be a class or interface")
    );
}

#[test]
fn rejects_unknown_class_base() {
    let error = check_source(
        r#"
player := class(missing_base):
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("unknown type `missing_base`"));
}

#[test]
fn rejects_duplicate_inherited_class_field() {
    let error = check_source(
        r#"
entity := class:
    Name : string

player := class(entity):
    Name : string
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate inherited class field `Name`")
    );
}

#[test]
fn rejects_unknown_class_field() {
    let error = check_source(
        r#"
player := class:
    Name : string

player{Score := 1}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class `player` has no field `Score`")
    );
}

#[test]
fn rejects_missing_required_class_field() {
    let error = check_source(
        r#"
player := class:
    Name : string

player{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("missing required field `Name`"));
}

#[test]
fn rejects_class_field_type_mismatch() {
    let error = check_source(
        r#"
player := class:
    Name : string

player{Name := 1}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("field `Name` expected"));
}

#[test]
fn rejects_assignment_to_immutable_class_field() {
    let error = check_source(
        r#"
player := class:
    Name : string

Hero := player{Name := "Ava"}
set Hero.Name = "Mira"
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("immutable field `Name`"));
}

#[test]
fn rejects_local_class_definition() {
    let error = check_source(
        r#"
{
    local_class := class:
        Value : int = 0
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("module level"));
}

#[test]
fn rejects_class_expression_not_direct_definition_rhs() {
    let error = check_source(
        r#"
class:
    Value : int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("direct right-hand side"));
}

#[test]
fn check_errors_include_source_locations() {
    let source = r#"x: number := "not a number""#;
    let pretty = check_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("line 1, column 14"));
    assert!(pretty.contains(r#""not a number""#));
    assert!(pretty.contains("^"));
}

#[test]
fn runtime_errors_include_source_locations() {
    let source = "1 / 0";
    let mut interpreter = Interpreter::new();
    let pretty = interpreter
        .eval_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("division by zero at line 1, column 1"));
    assert!(pretty.contains("1 / 0"));
    assert!(pretty.contains("^^^^^"));
}

#[test]
fn evaluates_arrays_indexing_and_length_member() {
    let source = r#"
xs := array{10, 20, 30}
xs.Length + xs[1]
"#;

    assert_eq!(eval(source), Value::Number(23.0));
}

#[test]
fn rejects_array_index_with_non_int() {
    let error = check_source(
        r#"
Values := array{10, 20}
Values[1.0]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("array index expected `int`"));
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
fn runtime_errors_on_float_array_index() {
    let error = Interpreter::new()
        .eval_source("array{10, 20}[1.0]")
        .expect_err("source should fail");

    assert!(error.to_string().contains("array index expected int"));
}

#[test]
fn rejects_bracket_array_literals() {
    let error = parse_source("[1, 2, 3]").expect_err("source should fail");
    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn evaluates_mutable_variables_and_array_slots() {
    let source = r#"
var xs: []int = array{1, 2, 3}
set xs[1] = 40
var total: int = 0
set total += xs[0] + xs[1] + xs[2]
total
"#;

    assert_eq!(eval(source), Value::Number(44.0));
}

#[test]
fn evaluates_var_declaration_expression() {
    let source = r#"
Initial := (var Total:int = 40)
set Total += 2
Initial + Total
"#;

    assert_eq!(eval(source), Value::Number(82.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_var_expression_without_type_annotation() {
    let error = parse_source("Value := (var Temp = 1)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected explicit type annotation after `var` name")
    );
}

#[test]
fn rejects_var_expression_colon_equal_initializer() {
    let error = parse_source("Value := (var Temp:int := 1)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `=` after variable declaration")
    );
}

#[test]
fn rejects_var_expression_type_mismatch() {
    let error = check_source(r#"Value := (var Temp:int = "bad")"#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Temp` is annotated as `int` but expression has type `string`")
    );
}

#[test]
fn rejects_var_expression_as_set_target() {
    let error = parse_source("set (var Temp:int = 0) = 1").expect_err("source should fail");

    assert!(error.to_string().contains("expected assignment target"));
}

#[test]
fn evaluates_computes_struct_field_mutation() {
    let source = r#"
point := struct<computes>:
    X:int = 0
    Y:int = 0

var P:point = point{}
Old := P
set P.X = 40
set P.Y += 2
P.X + P.Y + Old.X
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_computes_struct_field_mutation_in_failure_context() {
    let source = r#"
point := struct<computes>:
    X:int = 0

var P:point = point{}
Values := array{42}
Result := if (set P.X = Values[0]):
    P.X
else:
    0
Result
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_computes_struct_field_mutation() {
    let source = r#"
point := struct<computes>:
    X:int = 0

wrapper := struct<computes>:
    Inner:point

var W:wrapper = wrapper{Inner := point{}}
set W.Inner.X = 42
W.Inner.X
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_computes_struct_field_mutation_through_var_class_field() {
    let source = r#"
point := struct<computes>:
    X:int = 0

box := class:
    var Inner:point

B := box{Inner := point{}}
set B.Inner.X = 42
B.Inner.X
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_field_mutation_on_non_computes_struct() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

var P:point = point{}
set P.X = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("struct `point` must be `<computes>` to mutate fields")
    );
}

#[test]
fn rejects_computes_struct_field_mutation_through_immutable_binding() {
    let error = check_source(
        r#"
point := struct<computes>:
    X:int = 0

P:point = point{}
set P.X = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable binding `P`")
    );
}

#[test]
fn rejects_duplicate_struct_computes_specifier() {
    let error = parse_source(
        r#"
point := struct<computes><computes>:
    X:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate struct specifier `computes`")
    );
}

#[test]
fn rejects_computes_struct_field_mutation_through_immutable_class_field() {
    let error = check_source(
        r#"
point := struct<computes>:
    X:int = 0

box := class:
    Inner:point

B := box{Inner := point{}}
set B.Inner.X = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot assign to immutable field `Inner`")
    );
}

#[test]
fn evaluates_array_value_copy_semantics() {
    let source = r#"
row := []int
grid := []row

var Values:[]int = array{1, 2}
Snapshot := Values
if:
    set Values[0] = 99
then:
    {}
else:
    {}

var Matrix:grid = array{array{1, 2}, array{3, 4}}
MatrixSnapshot := Matrix
if:
    set Matrix[0][1] = 9
then:
    {}
else:
    {}

if:
    SnapshotValue := Snapshot[0]
    ValuesValue := Values[0]
    MatrixSnapshotValue := MatrixSnapshot[0][1]
    MatrixValue := Matrix[0][1]
then:
    SnapshotValue * 1000 + ValuesValue * 10 + MatrixSnapshotValue * 100 + MatrixValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(2199.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Number(50.0));
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
fn evaluates_compound_assignments_to_slots_and_fields() {
    let source = r#"
score := class:
    var Value:int = 10

var Values:[]int = array{10, 20}
if:
    set Values[0] *= 3
then:
    {}
else:
    {}
Item := score{}
set Item.Value -= 8
if (Value := Values[0]). Value + Item.Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(32.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn runtime_errors_on_divide_assignment_by_zero() {
    let source = r#"
var Value:int = 10
set Value /= 0
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(error.to_string().contains("division by zero"));
}

#[test]
fn evaluates_array_concatenation_and_tuple_append() {
    let source = r#"
var Values:[]int = array{1, 2}
set Values = Values + array{3}
set Values += (4, 5)
Values.Length + Values[4]
"#;

    assert_eq!(eval(source), Value::Number(10.0));
}

#[test]
fn evaluates_array_concatenation_value_copy_semantics() {
    let source = r#"
row := []int
grid := []row

var Left:grid = array{array{1, 2}}
PlusResult := Left + array{array{3, 4}}
TupleResult := Left + (array{5, 6}, array{7, 8})
if:
    set Left[0][1] = 9
then:
    {}
else:
    {}

if:
    PlusValue := PlusResult[0][1]
    TupleValue := TupleResult[0][1]
    LeftValue := Left[0][1]
then:
    PlusValue * 100 + TupleValue * 10 + LeftValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(229.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_tuple_to_array_assignment() {
    let source = r#"
Numbers:tuple(int, int, int) = (1, 2, 3)
Values:[]int = Numbers
var Other:[]int = array{}
set Other = (4, 5)
if:
    Value := Values[2]
    OtherValue := Other[1]
then:
    Values.Length + Value + Other.Length + OtherValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(13.0));
}

#[test]
fn evaluates_array_methods() {
    let source = r#"
Values:[]int = array{10, 20, 30, 20}
Slice := Values.Slice[1, 3]
Removed := Values.RemoveFirstElement[20]
AllRemoved := Values.RemoveAllElements[20]
RangeRemoved := Values.Remove[1, 3]
ElementRemoved := Values.RemoveElement[2]
FirstReplaced := Values.ReplaceFirstElement[20, 99]
AllReplaced := Values.ReplaceAllElements[20, 7]
IndexReplaced := Values.ReplaceElement[2, 8]
Inserted := Values.Insert[2, (25, 26)]
PatternReplaced := Values.ReplaceAll[(20, 30), array{7}]

var Total:int = 0
set Total += Slice.Length
set Total += Slice[0]
set Total += Values.Find[20]
set Total += Removed[1]
set Total += AllRemoved.Length
set Total += RangeRemoved[1]
set Total += ElementRemoved[2]
set Total += FirstReplaced[1]
set Total += AllReplaced[3]
set Total += IndexReplaced[2]
set Total += Inserted[3]
set Total += PatternReplaced[1]
set Total += PatternReplaced.Length
Total
"#;

    assert_eq!(eval(source), Value::Number(245.0));
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
fn rejects_array_replace_all_transacts_effect_in_computes_function() {
    let error = check_source(
        r#"
Build()<computes>:[]int =
    Values:[]int = array{1, 2, 1}
    Values.ReplaceAll[array{1}, array{3}]

Build()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn evaluates_array_replace_all_in_transacts_function() {
    let source = r#"
Build()<transacts>:[]int =
    Values:[]int = array{1, 2, 1}
    Values.ReplaceAll[array{1}, array{3}]

Result := Build()
if:
    First := Result[0]
    Second := Result[1]
    Third := Result[2]
then:
    First + Second + Third
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(8.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn allows_array_replace_all_elements_in_computes_function() {
    let source = r#"
Build()<computes>:[]int =
    Values:[]int = array{1, 2, 1}
    Values.ReplaceAllElements[1, 3]

Result := Build()
if:
    First := Result[0]
    Second := Result[1]
    Third := Result[2]
then:
    First + Second + Third
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(8.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_array_methods_value_copy_semantics() {
    let source = r#"
row := []int
grid := []row

var Rows:grid = array{array{1, 2}, array{3, 4}}
Slice := if (Result := Rows.Slice[0, 1]). Result else. array{}
Inserted := if (Result := Rows.Insert[1, array{array{5, 6}}]). Result else. array{}
Replaced := if (Result := Rows.ReplaceElement[0, array{7, 8}]). Result else. array{}
PatternReplaced := Rows.ReplaceAll[array{array{1, 2}}, array{array{9, 10}}]
if:
    set Rows[0][1] = 99
then:
    {}
else:
    {}

if:
    SliceValue := Slice[0][1]
    InsertedValue := Inserted[0][1]
    ReplacedValue := Replaced[0][1]
    PatternValue := PatternReplaced[0][1]
then:
    SliceValue * 1000 + InsertedValue * 100 + ReplacedValue * 10 + PatternValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(2290.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_slice_start_only_array_method() {
    let error = check_source(
        r#"
Values:[]int = array{10, 20, 30, 40}
Values.Slice[2]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`Slice` expected 2 arguments"));
}

#[test]
fn evaluates_string_array_methods() {
    let source = r#"
Text:string = "balloon"
Slice := if (Value := Text.Slice[1, 5]). Value else. ""
Inserted := if (Value := Text.Insert[1, "!!"]). Value else. ""
Replaced := Text.ReplaceAll["lo", "p"]

if:
    Slice = "allo"
    Inserted = "b!!alloon"
    Replaced = "balpon"
    Index := Text.Find['l']
then:
    Index + Slice.Length + Inserted.Length + Replaced.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(21.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_slice_start_only_string_array_method() {
    let error = check_source(
        r#"
Text:string = "balloon"
Text.Slice[2]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`Slice` expected 2 arguments"));
}

#[test]
fn evaluates_failable_string_array_methods_in_failure_context() {
    let source = r#"
Text:string = "abc"
FindHit := if (Index := Text.Find['a']). Index else. -1
FindMiss := if (Index := Text.Find['z']). Index else. 10
SliceMiss := if (Part := Text.Slice[2, 1]). Part.Length else. 20
RemoveMiss := if (Part := Text.RemoveElement[9]). Part.Length else. 30
FindHit + FindMiss + SliceMiss + RemoveMiss
"#;

    assert_eq!(eval(source), Value::Number(60.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_failable_array_methods_in_failure_context() {
    let source = r#"
Values:[]int = array{10, 20, 30}

FindHit := if (Index := Values.Find[20]). Index else. -1
FindMiss := if (Index := Values.Find[99]). Index else. 40
SliceHit := if (Slice := Values.Slice[1, 3]). Slice.Length else. 0
SliceMiss := if (Slice := Values.Slice[2, 1]). Slice.Length else. 0
RangeRemoved := if:
    Result := Values.Remove[1, 3]
    Value := Result[0]
then:
    Value
else:
    0
RangeRemoveMiss := if (Result := Values.Remove[3, 1]). Result.Length else. 5
ElementRemoved := if:
    Result := Values.RemoveElement[1]
    Value := Result[1]
then:
    Value
else:
    0
ElementRemoveMiss := if (Result := Values.RemoveElement[9]). Result.Length else. 6
FirstRemoved := if (Result := Values.RemoveFirstElement[20]). Result.Length else. 0
FirstRemoveMiss := if (Result := Values.RemoveFirstElement[99]). Result.Length else. 7
Replaced := if:
    Result := Values.ReplaceElement[1, 42]
    Value := Result[1]
then:
    Value
else:
    0
ReplaceMiss := if (Result := Values.ReplaceElement[9, 42]). Result.Length else. 8
FirstReplaced := if:
    Result := Values.ReplaceFirstElement[20, 7]
    Value := Result[1]
then:
    Value
else:
    0
FirstReplaceMiss := if (Result := Values.ReplaceFirstElement[99, 7]). Result.Length else. 9
Inserted := if:
    Result := Values.Insert[1, array{5}]
    Value := Result[1]
then:
    Value
else:
    0
InsertMiss := if (Result := Values.Insert[9, array{5}]). Result.Length else. 10

var Total:int = 0
set Total += FindHit
set Total += FindMiss
set Total += SliceHit
set Total += SliceMiss
set Total += RangeRemoved
set Total += RangeRemoveMiss
set Total += ElementRemoved
set Total += ElementRemoveMiss
set Total += FirstRemoved
set Total += FirstRemoveMiss
set Total += Replaced
set Total += ReplaceMiss
set Total += FirstReplaced
set Total += FirstReplaceMiss
set Total += Inserted
set Total += InsertMiss
Total
"#;

    assert_eq!(eval(source), Value::Number(184.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failable_slice_start_only_array_method() {
    let error = check_source(
        r#"
Values:[]int = array{10, 20, 30}
if (Slice := Values.Slice[1]). Slice.Length else. 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`Slice` expected 2 arguments"));
}

#[test]
fn captures_failable_array_method_failure_in_option_literal() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Found:?int = option{Values.Find[20]}
Missing:?int = option{Values.Find[99]}
Removed:?[]int = option{Values.RemoveElement[1]}
RemoveMissing:?[]int = option{Values.RemoveElement[9]}

First := if (Index := Found?). Index else. 0
Second := if (Index := Missing?). Index else. 40
Third := if:
    Result := Removed?
    Value := Result[1]
then:
    Value
else:
    0
Fourth := if (Result := RemoveMissing?). Result.Length else. 0
First + Second + Third + Fourth
"#;

    assert_eq!(eval(source), Value::Number(71.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_array_method_type_mismatch() {
    let error = check_source(
        r#"
Values:[]int = array{1}
Values.ReplaceElement[0, "bad"]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("new value expected"));
}

#[test]
fn rejects_array_method_float_index() {
    let error = check_source(
        r#"
Values:[]int = array{1, 2}
Values.RemoveElement[1.0]
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`RemoveElement` index expected `int`")
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
fn rejects_slice_float_index() {
    let error = check_source(
        r#"
Values:[]int = array{1, 2}
Values.Slice[0.0, 1]
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Slice` argument expected `int`")
    );
}

#[test]
fn rejects_bracket_method_on_non_array() {
    let error = check_source(r#"false.Find[1]"#).expect_err("source should fail");

    assert!(error.to_string().contains("no bracket method"));
}

#[test]
fn rejects_string_array_method_type_mismatch() {
    let error = check_source(r#""x".Find[1]"#).expect_err("source should fail");

    assert!(error.to_string().contains("`Find` expected `char`"));
}

#[test]
fn rejects_slice_with_too_many_arguments() {
    let error = check_source("Values:[]int = array{1, 2}\nValues.Slice[0, 1, 2]")
        .expect_err("source should fail");

    assert!(error.to_string().contains("`Slice` expected 2 arguments"));
}

#[test]
fn rejects_failed_array_find_outside_failure_context() {
    let source = r#"
Values:[]int = array{1, 2}
Values.Find[3]
"#;
    assert_failable_context_error(source);
}

#[test]
fn runtime_errors_on_invalid_array_slice() {
    let source = r#"
Values:[]int = array{1, 2}
Values.Slice[0, 3]
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(error.to_string().contains("out of bounds"));
}

#[test]
fn evaluates_concatenate_builtin() {
    let source = r#"
Values:[]int = Concatenate(array{1, 2}, array{3}, (4, 5))
Nested:[]int = Concatenate(array{array{6, 7}, array{8}})
Values.Length + Values[4] + Nested.Length + Nested[2]
"#;

    assert_eq!(eval(source), Value::Number(21.0));
}

#[test]
fn evaluates_concatenate_and_shuffle_value_copy_semantics() {
    let source = r#"
row := []int
grid := []row

var Rows:grid = array{array{1, 2}}
Combined:grid = Concatenate(Rows, array{array{3, 4}})
Flattened:[]int = Concatenate(array{array{5, 6}})
Shuffled:grid = Shuffle(Rows)
if:
    set Rows[0][1] = 9
then:
    {}
else:
    {}

if:
    CombinedValue := Combined[0][1]
    FlattenedValue := Flattened[1]
    ShuffledValue := Shuffled[0][1]
    RowsValue := Rows[0][1]
then:
    CombinedValue * 1000 + FlattenedValue * 100 + ShuffledValue * 10 + RowsValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(2629.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_concatenate_non_array_arguments() {
    let error = check_source("Concatenate(1)").expect_err("source should fail");

    assert!(error.to_string().contains("array argument item 1"));
}

#[test]
fn rejects_tuple_to_array_type_mismatch() {
    let error = check_source(r#"Values:[]int = (1, "bad")"#).expect_err("source should fail");

    assert!(error.to_string().contains("array<int>"));
}

#[test]
fn rejects_array_concatenation_type_mismatch() {
    let error = check_source(
        r#"
Values:[]int = array{1}
Values + array{"bad"}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("incompatible types"));
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
fn rejects_failable_option_query_inside_defer() {
    let error = check_source(
        r#"
Maybe:?int = false
defer:
    Maybe?
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
fn checks_spawn_inside_defer() {
    let source = r#"
Wait()<suspends>:void = {}
Run()<suspends>:void =
    defer:
        spawn{Wait()}
"#;

    assert!(check_source(source).is_ok());
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
fn rejects_range_expression_as_array_item() {
    let error = check_source("Values := array{1..3}").expect_err("source should fail");

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
fn evaluates_computes_call_in_for_filter_failure_context() {
    let source = r#"
Keep()<computes>:logic = true
Values:[]int = for (X := 1..3, Keep()?):
    X
if (Value := Values[2]). Values.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(6.0));
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

    assert_eq!(eval(source), Value::Number(6.0));
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
fn evaluates_for_failable_map_lookup_binding_clauses() {
    let source = r#"
Names:[]string = array{"ada", "missing", "grace"}
Scores:[string]int = map{"ada" => 20, "grace" => 22}

Values:[]int = for (Name : Names, Score := Scores[Name]):
    Score

if:
    First := Values[0]
    Second := Values[1]
then:
    Values.Length + First + Second
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(44.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_failable_option_query_binding_clauses() {
    let source = r#"
Maybe:?int = option{10}
Empty:?int = false

Kept:[]int = for (I := 1..2, Value := Maybe?):
    I + Value

Dropped:[]int = for (I := 1..2, Value := Empty?):
    I + Value

if:
    First := Kept[0]
    Second := Kept[1]
then:
    Kept.Length + First + Second + Dropped.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(25.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Number(5.0));
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

    assert_eq!(eval(source), Value::Number(3.0));
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

    assert_eq!(eval(source), Value::Number(0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_multiple_generator_clauses() {
    let source = r#"
Pairs:[]int = for (X := 1..2, Y := 1..3):
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
fn evaluates_for_unicode_strings_as_utf8_units() {
    let source = r#"
Units := for (Unit : "José") {
    Unit
}
if (Units = array{'J', 'o', 's', 0oC3, 0oA9}):
    Units.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(5.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_for_array_index_value_pairs() {
    let source = r#"
Values:[]int = array{10, 20, 30}
var Total:int = 0
for (Index -> Value : Values) {
    set Total += Index + Value
}
Total
"#;

    assert_eq!(eval(source), Value::Number(63.0));
}

#[test]
fn checks_loop_and_array_programs() {
    let source = r#"
var xs: []int = array{1, 2, 3}
if:
    set xs[0] = 10
then:
    {}
else:
    {}
var total: int = 0
for (item : xs) {
    set total = total + item
}
total
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_assignment_to_immutable_bindings() {
    let error = check_source(
        r#"
x := 1
set x = 2
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("immutable binding `x`"));
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
fn evaluates_verse_style_constant_definitions() {
    let source = r#"
Answer:int = 40
Truth:logic = true
if (Truth) {
    Answer + 2
} else {
    0
}
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_verse_array_literal_and_array_type() {
    let source = r#"
Values:[]int = array{3, 4, 5}
Values[0] + Values[1] + Values[2]
"#;

    assert_eq!(eval(source), Value::Number(12.0));
}

#[test]
fn evaluates_verse_map_literals_and_lookup() {
    let source = r#"
Scores:[string]int = map{
    "alice" => 10,
    "bob" => 20,
    "alice" => 15,
}
Scores["alice"] + Scores["bob"] + Scores.Length
"#;

    assert_eq!(eval(source), Value::Number(37.0));
}

#[test]
fn evaluates_array_and_map_length_members() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Scores:[string]int = map{"alice" => 10, "bob" => 20}
Values.Length + Scores.Length
"#;

    assert_eq!(eval(source), Value::Number(5.0));
}

#[test]
fn evaluates_official_length_extension_calls() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Scores:[string]int = map{"alice" => 10, "bob" => 20}
Text := "abc"
Values.Length() * 100 + Scores.Length() * 10 + Text.Length()
"#;

    assert_eq!(eval(source), Value::Number(323.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_length_extension_arguments() {
    let error = check_source("array{1}.Length(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn rejects_weak_map_length_extension_call() {
    let source = r#"
player := class<unique>:
    ID:int
Saved:weak_map(player, int) = map{}
Saved.Length()
"#;
    let error = check_source(source).expect_err("source should fail");

    assert!(error.to_string().contains("no member `Length`"));
}

#[test]
fn rejects_length_member_on_non_container() {
    let error = check_source(
        r#"
Pair:tuple(int, int) = (1, 2)
Pair.Length
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("no member `Length`"));
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
fn evaluates_map_mutation_and_insert() {
    let source = r#"
var Scores:[string]int = map{"alice" => 10}
set Scores["alice"] += 10
set Scores["bob"] = 5
Scores["alice"] + Scores["bob"] + Scores.Length
"#;

    assert_eq!(eval(source), Value::Number(27.0));
}

#[test]
fn evaluates_map_key_removal_by_reconstruction_pattern() {
    let source = r#"
RemoveKey(Scores:[string]int, Removed:string)<transacts>:[string]int =
    var NewScores:[string]int = map{}
    for (Name -> Score : Scores, Name <> Removed):
        set NewScores = ConcatenateMaps(NewScores, map{Name => Score})
    NewScores

Scores:[string]int = map{"Alice" => 100, "Bob" => 85, "Charlie" => 92}
Filtered := RemoveKey(Scores, "Bob")
Missing := if (Score := Filtered["Bob"]). Score else. 40
if:
    Alice := Filtered["Alice"]
    Charlie := Filtered["Charlie"]
then:
    Alice + Charlie - 190 + Missing
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
fn evaluates_map_insertion_order_after_mutation() {
    let source = r#"
var Scores:[string]int = map{"a" => 3, "b" => 1, "c" => 2}
if:
    set Scores["a"] = 0
    set Scores["d"] = 4
then:
    {}
else:
    {}
Order := for (Key -> Value : Scores):
    Key
SameOrder := if (Scores = map{"a" => 0, "b" => 1, "c" => 2, "d" => 4}). 1 else. 0
DifferentOrder := if (Scores <> map{"b" => 1, "c" => 2, "a" => 0, "d" => 4}). 1 else. 0
if:
    Order[0] = "a"
    Order[1] = "b"
    Order[2] = "c"
    Order[3] = "d"
then:
    40 + SameOrder + DifferentOrder
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
fn rejects_non_official_map_remove_member() {
    let error = check_source(
        r#"
Scores:[string]int = map{}
Scores.RemoveKey["Bob"]
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("no bracket method `RemoveKey`"));
}

#[test]
fn evaluates_map_value_copy_semantics() {
    let source = r#"
score_map := [string]int
team_map := [string]score_map

var Scores:[string]int = map{"ada" => 1}
Snapshot := Scores
if:
    set Scores["ada"] = 99
then:
    {}
else:
    {}

var Teams:team_map = map{"red" => map{"ada" => 2}}
TeamSnapshot := Teams
if:
    set Teams["red"]["ada"] = 8
then:
    {}
else:
    {}

if:
    SnapshotValue := Snapshot["ada"]
    ScoresValue := Scores["ada"]
    TeamSnapshotValue := TeamSnapshot["red"]["ada"]
    TeamsValue := Teams["red"]["ada"]
then:
    SnapshotValue * 1000 + ScoresValue * 10 + TeamSnapshotValue * 100 + TeamsValue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(2198.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_weak_map_type_annotations() {
    let source = r#"
player := class<unique>:
    ID:int = 0

var Saved:weak_map(player, int) = map{}
Saved
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::WeakMap(Box::new(Type::Class("player".into())), Box::new(Type::Int))
    );
}

#[test]
fn accepts_four_player_weak_maps_when_one_value_is_persistable_class() {
    let source = r#"
player_profile_data := class<final><persistable>:
    XP:int = 0

var SavedA:weak_map(player, int) = map{}
var SavedB:weak_map(player, float) = map{}
var SavedC:weak_map(player, []int) = map{}
var SavedD:weak_map(player, player_profile_data) = map{}
0
"#;

    assert_eq!(eval(source), Value::Number(0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_four_player_weak_maps_without_class_value() {
    let error = check_source(
        r#"
var SavedA:weak_map(player, int) = map{}
var SavedB:weak_map(player, float) = map{}
var SavedC:weak_map(player, []int) = map{}
var SavedD:weak_map(player, string) = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("at least one value type must be a persistable class")
    );
}

#[test]
fn rejects_more_than_four_player_weak_maps() {
    let error = check_source(
        r#"
player_profile_data := class<final><persistable>:
    XP:int = 0

var SavedA:weak_map(player, player_profile_data) = map{}
var SavedB:weak_map(player, int) = map{}
var SavedC:weak_map(player, float) = map{}
var SavedD:weak_map(player, []int) = map{}
var SavedE:weak_map(player, string) = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("limited to four per island"));
}

#[test]
fn evaluates_weak_map_lookup_and_update() {
    let source = r#"
player := class<unique>:
    ID:int = 0

Alice := player{ID := 1}
var Saved:weak_map(player, int) = map{}
if:
    set Saved[Alice] = 41
    set Saved[Alice] += 1
    Value := Saved[Alice]
then:
    Value
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
fn captures_weak_map_lookup_failure_in_option_literal() {
    let source = r#"
player := class<unique>:
    ID:int = 0

Alice := player{ID := 1}
Bob := player{ID := 2}
var Saved:weak_map(player, int) = map{}
if:
    set Saved[Alice] = 42
then:
    {}
else:
    {}
Found:?int = option{Saved[Alice]}
Missing:?int = option{Saved[Bob]}
if (Value := Found?):
    Value
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
fn rejects_weak_map_key_type_mismatch() {
    let error = check_source(
        r#"
player := class<unique>:
    ID:int = 0

var Saved:weak_map(player, int) = map{}
if:
    set Saved["alice"] = 42
then:
    {}
else:
    {}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("map key expects `player`"));
}

#[test]
fn rejects_weak_map_value_assignment_type_mismatch() {
    let error = check_source(
        r#"
player := class<unique>:
    ID:int = 0

Alice := player{ID := 1}
var Saved:weak_map(player, int) = map{}
if:
    set Saved[Alice] = "bad"
then:
    {}
else:
    {}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot assign `string`"));
}

#[test]
fn rejects_weak_map_length_member() {
    let error = check_source(
        r#"
player := class<unique>:
    ID:int = 0

Saved:weak_map(player, int) = map{}
Saved.Length
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("no member `Length`"));
}

#[test]
fn rejects_weak_map_iteration() {
    let error = check_source(
        r#"
player := class<unique>:
    ID:int = 0

Saved:weak_map(player, int) = map{}
for (Score : Saved):
    Score
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot iterate"));
}

#[test]
fn rejects_player_weak_map_non_persistable_value() {
    let error = check_source(
        r#"
player_profile_data := class<final>:
    XP:int = 0

var Profiles:weak_map(player, player_profile_data) = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("weak_map(player, ...) value type `player_profile_data` must be persistable")
    );
}

#[test]
fn evaluates_fits_in_player_map_success() {
    let source = r#"
Values:[]int = array{1, 2, 3}
Checked:[]int = if (Result := FitsInPlayerMap[Values]). Result else. array{}
if (Value := Checked[2]). Checked.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(6.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(
        check_source("if (Value := FitsInPlayerMap[array{1, 2}]). Value else. array{}")
            .expect("source should check"),
        Type::Array(Box::new(Type::Int))
    );
}

#[test]
fn captures_fits_in_player_map_size_failure_in_failure_context() {
    let source = r#"
Large := for (I := 1..33000). I
if (Checked := FitsInPlayerMap[Large]). Checked.Length else. 42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_failed_fits_in_player_map_outside_failure_context() {
    let source = r#"
Large := for (I := 1..33000). I
FitsInPlayerMap[Large]
"#;
    assert_failable_context_error(source);
}

#[test]
fn captures_fits_in_player_map_non_persistable_failure() {
    let source = r#"
Make():int = 42
if (Checked := FitsInPlayerMap[Make]). Checked() else. 7
"#;

    assert_eq!(eval(source), Value::Number(7.0));
}

#[test]
fn rejects_fits_in_player_map_parenthesis_call() {
    let error = check_source("FitsInPlayerMap(42)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("functions with `<decides>` must be called with `[]`")
    );
}

#[test]
fn rejects_weak_map_non_session_or_player_key() {
    let error = check_source(
        r#"
var Scores:weak_map(string, int) = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("weak_map key type must be `session` or `player`")
    );
}

#[test]
fn evaluates_get_session_value() {
    let source = r#"
Current:session = GetSession()
Current
"#;

    assert_eq!(eval(source), Value::Session);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Class("session".into())
    );
}

#[test]
fn evaluates_get_simulation_elapsed_time_function() {
    let source = "GetSimulationElapsedTime()";
    let value = eval(source);

    assert!(matches!(value, Value::Number(seconds) if seconds >= 0.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn get_simulation_elapsed_time_is_monotonic_within_eval() {
    let source = r#"
First := GetSimulationElapsedTime()
Second := GetSimulationElapsedTime()
if (Second >= First). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_get_simulation_elapsed_time_arguments() {
    let error = check_source("GetSimulationElapsedTime(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn checks_official_sleep_in_suspends_function() {
    let source = r#"
Wait()<suspends>:void =
    Sleep(-1.0)
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
fn rejects_official_sleep_outside_async_context() {
    let error = check_source(
        r#"
Wait():void =
    Sleep(-1.0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`<suspends>` effect"));
    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_sleep_arguments() {
    let error = check_source(
        r#"
Wait()<suspends>:void =
    Sleep()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected 1 arguments"));
}

#[test]
fn rejects_official_sleep_non_float_argument() {
    let error = check_source(
        r#"
Wait()<suspends>:void =
    Sleep("now")
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `float`"));
}

#[test]
fn rejects_official_sleep_in_failure_context() {
    let error = check_source(
        r#"
Check()<suspends>:int =
    if (Sleep(-1.0), false?):
        1
    else:
        2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`<no_rollback>` effect cannot be called in a failure context")
    );
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
fn evaluates_session_weak_map_lookup_and_update() {
    let source = r#"
var GlobalInt:weak_map(session, int) = map{}
if:
    set GlobalInt[GetSession()] = 41
    set GlobalInt[GetSession()] += 1
    Value := GlobalInt[GetSession()]
then:
    Value
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
fn evaluates_missing_session_weak_map_lookup_in_failure_context() {
    let source = r#"
var GlobalInt:weak_map(session, int) = map{}
if (Value := GlobalInt[GetSession()]):
    Value
else:
    42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_session_weak_map_key_type_mismatch() {
    let error = check_source(
        r#"
var GlobalInt:weak_map(session, int) = map{}
if:
    set GlobalInt[0] = 42
then:
    {}
else:
    {}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("map key expects `session`"));
}

#[test]
fn rejects_get_session_arguments() {
    let error = check_source("GetSession(1)").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 arguments"));
}

#[test]
fn evaluates_concatenate_maps_builtin() {
    let source = r#"
Base:[int]string = map{1 => "one", 2 => "old"}
Override:[int]string = map{2 => "two", 3 => "three"}
Combined:[int]string = ConcatenateMaps(Base, Override)
Combined[1] + Combined[2] + Combined[3] + str(Combined.Length)
"#;

    assert_eq!(eval(source), Value::String("onetwothree3".into()));
}

#[test]
fn evaluates_concatenate_maps_value_copy_semantics() {
    let source = r#"
score_map := [string]int
team_map := [string]score_map

var Base:team_map = map{"red" => map{"ada" => 1}}
Override:team_map = map{"blue" => map{"grace" => 2}}
Combined:team_map = ConcatenateMaps(Base, Override)
if:
    set Base["red"]["ada"] = 9
then:
    {}
else:
    {}

if:
    CombinedRed := Combined["red"]["ada"]
    BaseRed := Base["red"]["ada"]
    CombinedBlue := Combined["blue"]["grace"]
then:
    CombinedRed * 100 + BaseRed * 10 + CombinedBlue
else:
    0
"#;

    assert_eq!(eval(source), Value::Number(192.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_concatenate_maps_non_map_arguments() {
    let error =
        check_source("ConcatenateMaps(map{1 => 1}, array{2})").expect_err("source should fail");

    assert!(error.to_string().contains("argument 2 expected `map"));
}

#[test]
fn runtime_errors_on_missing_map_plus_equal_key() {
    let source = r#"
var Scores:[string]int = map{"alice" => 10}
set Scores["bob"] += 5
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(error.to_string().contains("not found"));
}

#[test]
fn evaluates_map_value_iteration() {
    let source = r#"
Scores:[string]int = map{"alice" => 2, "bob" => 3}
var Total:int = 0
for (Score : Scores) {
    set Total = Total + Score
}
Total
"#;

    assert_eq!(eval(source), Value::Number(5.0));
}

#[test]
fn evaluates_for_map_key_value_pairs() {
    let source = r#"
Scores:[int]int = map{1 => 2, 2 => 3}
var Total:int = 0
for (Rank -> Score : Scores) {
    set Total += Rank + Score
}
Total
"#;

    assert_eq!(eval(source), Value::Number(8.0));
}

#[test]
fn rejects_for_pair_iteration_over_range() {
    let error = check_source(
        r#"
for (Index -> Value : 1..3) {
    Value
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot use `->`"));
}

#[test]
fn rejects_for_pair_iteration_over_string() {
    let error = check_source(
        r#"
for (Index -> Letter : "abc") {
    Letter
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot use `->`"));
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

#[test]
fn checks_map_type_annotations() {
    let source = r#"
Scores:[string]int = map{}
Scores
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Map(Box::new(Type::String), Box::new(Type::Int))
    );
}

#[test]
fn evaluates_option_map_key_annotation() {
    let source = r#"
Scores:[?int]int = map{option{7} => 42}
if (Value := Scores[option{7}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_comparable_map_key_annotation() {
    let source = r#"
Scores:[comparable]int = map{option{7} => 42}
if (Value := Scores[option{7}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_rational_map_key_annotation() {
    let source = r#"
Half:rational = if (Value := 1 / 2). Value else. 0
Equivalent:rational = if (Value := 2 / 4). Value else. 0
Scores:[rational]int = map{Half => 42}
if (Value := Scores[Equivalent]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_array_map_keys_with_comparable_elements() {
    let source = r#"
Scores:[[]int]int = map{array{1, 2} => 40}
if (Value := Scores[array{1, 2}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(40.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_map_map_keys_with_comparable_keys_and_values() {
    let source = r#"
Nested:[[string]int]int = map{map{"ada" => 1} => 42}
if (Value := Nested[map{"ada" => 1}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_array_map_key_with_non_comparable_element() {
    let error =
        check_source("Scores:[[]type{_():int}]int = map{}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("map key type `function/0 -> int` is not comparable")
    );
}

#[test]
fn rejects_map_map_key_with_non_comparable_value() {
    let error =
        check_source("Scores:[[string]type{_():int}]int = map{}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("map key type `function/0 -> int` is not comparable")
    );
}

#[test]
fn evaluates_struct_map_key_with_comparable_fields() {
    let source = r#"
record_key := struct:
    ID:int
    Label:?string = false

Scores:[record_key]int = map{record_key{ID := 7, Label := option{"ready"}} => 42}
if (Value := Scores[record_key{ID := 7, Label := option{"ready"}}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_unique_class_map_key_annotation() {
    let error = check_source(
        r#"
thing := class:
    ID:int = 0

Scores:[thing]int = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("map key type `thing` is not comparable")
    );
}

#[test]
fn rejects_struct_map_key_with_non_comparable_field() {
    let error = check_source(
        r#"
bad_key := struct:
    Callback:type{_():int}

Scores:[bad_key]int = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "map key struct `bad_key` field `Callback` type `function/0 -> int` is not comparable"
    ));
}

#[test]
fn rejects_nested_struct_map_key_with_non_comparable_field() {
    let error = check_source(
        r#"
inner_key := struct:
    Callback:type{_():int}

outer_key := struct:
    Inner:inner_key

Scores:[outer_key]int = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "map key struct `outer_key` field `Inner` type `inner_key` is not comparable"
        )
    );
}

#[test]
fn rejects_function_map_key_annotation() {
    let error = check_source("Scores:[type{_():int}]int = map{}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("map key type `function/0 -> int` is not comparable")
    );
}

#[test]
fn rejects_non_comparable_type_alias_map_key_annotation() {
    let error = check_source(
        r#"
bad_key := ?type{_():int}
Scores:[bad_key]int = map{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("map key type `function/0 -> int` is not comparable")
    );
}

#[test]
fn evaluates_map_type_alias_annotations() {
    let source = r#"
player_map := [string]int
Scores:player_map = map{"ada" => 40, "grace" => 2}
if:
    Ada := Scores["ada"]
    Grace := Scores["grace"]
then:
    Ada + Grace
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
fn evaluates_nested_map_type_alias_annotations() {
    let source = r#"
player_map := [string]int
team_map := [string]player_map
Scores:team_map = map{"red" => map{"ada" => 42}}
if (Value := Scores["red"]["ada"]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_array_type_alias_runtime_coercion() {
    let source = r#"
number_list := []int
Values:number_list = (40, 2)
if:
    First := Values[0]
    Second := Values[1]
then:
    First + Second
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
fn evaluates_function_signature_type_alias_annotations() {
    let source = r#"
player_map := [string]int
ScoreFor(Scores:player_map, Name:string)<decides><transacts>:int = Scores[Name]
if (Score := ScoreFor[map{"ada" => 42}, "ada"]). Score else. 0
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

#[test]
fn rejects_map_key_type_mismatch() {
    let error =
        check_source(r#"Scores:[string]int = map{1 => 20}"#).expect_err("source should fail");

    assert!(error.to_string().contains("map<string, int>"));
}

#[test]
fn rejects_map_value_assignment_type_mismatch() {
    let error = check_source(
        r#"
var Scores:[string]int = map{"alice" => 10}
if:
    set Scores["bob"] = "bad"
then:
    {}
else:
    {}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot assign `string`"));
}

#[test]
fn evaluates_tuple_literals_and_indexing() {
    let source = r#"
Pair := (40, "ignored", 2)
Pair(0) + Pair(2)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_tuple_index_with_dynamic_int() {
    let source = r#"
Pair := (40, 2)
Index:int = 1
Pair(0) + Pair(Index)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_tuple_index_from_failable_int_result() {
    let source = r#"
Values:[]int = array{10, 20}
Pair := (40, 2)
Index := if (Value := Values.Find[20]). Value else. 0
Pair(0) + Pair(Index)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_tuple_type_annotations() {
    let source = r#"
Pair:tuple(int, string) = (1, "one")
Pair(1)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn checks_parenthesized_tuple_type_annotations() {
    let source = r#"
Pair:(int, string) = (1, "one")
Pair(1)
"#;

    assert_eq!(eval(source), Value::String("one".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_parenthesized_tuple_return_type_annotations() {
    let source = r#"
MakePair():(int, int) = (40, 2)
Pair := MakePair()
Pair(0) + Pair(1)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parenthesized_tuple_type_alias_annotations() {
    let source = r#"
pair_type := (int, int)
Pair:pair_type = (40, 2)
Pair(0) + Pair(1)
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_parenthesized_tuple_type_nested_in_array_annotation() {
    let source = r#"
Pairs:[](int, int) = array{(40, 1), (1, 2)}
if:
    First := Pairs[0]
    Second := Pairs[1]
then:
    First(0) + First(1) + Second(0)
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
fn rejects_single_element_parenthesized_tuple_type_annotation() {
    let error = parse_source("Value:(int) = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple type expects at least two element types")
    );
}

#[test]
fn rejects_empty_parenthesized_tuple_type_annotation() {
    let error = parse_source("Value:() = ()").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple type expects at least two element types")
    );
}

#[test]
fn evaluates_tuple_keys_in_maps() {
    let source = r#"
Grid:[tuple(int, int)]string = map{
    (0, 0) => "origin",
    (1, 0) => "east",
}
Grid[(1, 0)]
"#;

    assert_eq!(eval(source), Value::String("east".into()));
}

#[test]
fn rejects_tuple_index_out_of_bounds() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(2)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("out of bounds"));
}

#[test]
fn rejects_float_tuple_index() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(1.0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("tuple index expected `int`"));
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
fn rejects_negative_tuple_index_literal() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(-1)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple index must be a non-negative integer")
    );
}

#[test]
fn rejects_tuple_type_mismatch() {
    let error =
        check_source(r#"Pair:tuple(int, string) = (1, 2)"#).expect_err("source should fail");

    assert!(error.to_string().contains("tuple(int, string)"));
}

#[test]
fn evaluates_option_literals_and_unwrap() {
    let source = r#"
Maybe:?int = option{40}
Maybe? + 2
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn evaluates_option_braced_block_sequence() {
    let source = r#"
Maybe:?int = option{
    Value := 40
    Value + 2
}
if (Value := Maybe?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_single_line_braced_block_sequence() {
    let source = r#"
Maybe:?int = option{Value := 40; Value + 2}
if (Value := Maybe?). Value else. 0
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_tuple_value_in_option_braced_block() {
    let source = r#"
Maybe:?tuple(int, string) = option{
    Value := (40, "two")
    Value
}
if (Pair := Maybe?). Pair(0) + ToString(Pair(1)).Length else. 0
"#;

    assert_eq!(eval(source), Value::Number(43.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_array_items() {
    let source = r#"
Values:[]?int = array{false, option{40}, false}
First := if (Value := Values[0]?). Value else. 1
Second := if (Value := Values[1]?). Value else. 0
Third := if (Value := Values[2]?). Value else. 1
First + Second + Third
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_map_values() {
    let source = r#"
Scores:[string]?int = map{"empty" => false, "full" => option{40}}
Empty := if (Value := Scores["empty"]?). Value else. 1
Full := if (Value := Scores["full"]?). Value else. 0
Empty + Full + Scores.Length
"#;

    assert_eq!(eval(source), Value::Number(43.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_tuple_items() {
    let source = r#"
Pair:tuple(?int, ?int) = (false, option{41})
First := if (Value := Pair(0)?). Value else. 1
Second := if (Value := Pair(1)?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_function_arguments_and_returns() {
    let source = r#"
Default(Value:?int)<computes>:int = if (Actual := Value?). Actual else. 7
Pack(Values:[]?int)<computes>:int =
    First := if (Value := Values[0]?). Value else. 10
    Second := if (Value := Values[1]?). Value else. 0
    First + Second
Empty()<computes>:?int = false

FromReturn := if (Value := Empty()?). Value else. 5
Default(false) + Default(option{30}) + Pack(array{false, option{40}}) + FromReturn
"#;

    assert_eq!(eval(source), Value::Number(92.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_option_false_contextual_array_methods() {
    let source = r#"
Values:[]?int = array{option{40}}
Inserted := if (Result := Values.Insert[0, array{false}]). Result else. array{}
Replaced := if (Result := Values.ReplaceElement[0, false]). Result else. array{}
InsertEmpty := if (Value := Inserted[0]?). Value else. 1
InsertFull := if (Value := Inserted[1]?). Value else. 0
ReplaceEmpty := if (Value := Replaced[0]?). Value else. 1
InsertEmpty + InsertFull + ReplaceEmpty
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_logic_variable_as_contextual_option() {
    let error = check_source(
        r#"
Use(Value:?int):int = if (Actual := Value?). Actual else. 0
Flag:logic = false
Use(Flag)
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(
        message.contains("argument 1 expected `?int`, got `bool`"),
        "{message}"
    );
}

#[test]
fn rejects_true_as_contextual_option_array_item() {
    let error = check_source("Values:[]?int = array{true}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Values` is annotated as `array<?int>`")
    );
}

#[test]
fn evaluates_logic_query_in_failure_context() {
    let source = r#"
First := if (true?). 40 else. 0
Second := if (false?). 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_array_lookup_failure_in_option_literal() {
    let source = r#"
Values:[]int = array{42}
Found:?int = option{Values[0]}
Missing:?int = option{Values[5]}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_map_lookup_failure_in_option_literal() {
    let source = r#"
Scores:[string]int = map{"ada" => 42}
Found:?int = option{Scores["ada"]}
Missing:?int = option{Scores["grace"]}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_query_failure_in_option_literal() {
    let source = r#"
Ready:?logic = option{true?}
Blocked:?logic = option{false?}
First := if (Value := Ready?). if (Value?). 40 else. 0 else. 0
Second := if (Value := Blocked?). if (Value?). 0 else. 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_failure_in_option_braced_block_sequence() {
    let source = r#"
Values:[]int = array{40, 2}
Found:?int = option{
    First := Values[0]
    Second := Values[1]
    First + Second
}
Missing:?int = option{
    Value := Values[9]
    Value
}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_option_braced_block_sequence_when_it_fails() {
    let source = r#"
var Total:int = 0
Maybe:?int = option{
    set Total = 99
    false?
    1
}
if (Maybe?):
    0
else:
    Total + 42
"#;

    assert_eq!(eval(source), Value::Number(42.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn option_braced_block_bindings_do_not_escape() {
    let error = check_source(
        r#"
Maybe:?int = option{
    Value := 42
    Value
}
Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
}

#[test]
fn evaluates_empty_option_false_assignment() {
    let source = r#"
var Maybe:?int = false
set Maybe = option{7}
set Maybe = false
set Maybe = option{42}
Maybe?
"#;

    assert_eq!(eval(source), Value::Number(42.0));
}

#[test]
fn checks_empty_option_literal() {
    let source = r#"
Maybe:?int = option{}
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Option(Box::new(Type::Int))
    );
}

#[test]
fn rejects_option_type_mismatch() {
    let error = check_source(r#"Maybe:?int = option{"bad"}"#).expect_err("source should fail");

    assert!(error.to_string().contains("?int"));
}

#[test]
fn rejects_unwrap_on_non_option() {
    let error = check_source("if (1?). 1 else. 0").expect_err("source should fail");
    assert!(error.to_string().contains("query operator expected"));
}

#[test]
fn runtime_errors_on_empty_option_unwrap() {
    let source = r#"
Maybe:?int = false
Maybe?
"#;
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .eval_source(source)
        .expect_err("source should fail");

    assert!(error.to_string().contains("empty option"));
}

#[test]
fn rejects_var_without_explicit_type() {
    let error = parse_source("var Score = 0").expect_err("source should fail");
    assert!(error.to_string().contains("explicit type"));
}
