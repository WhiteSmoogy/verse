//! Executable inventory for finishing the remaining Types FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn assert_runtime_cases(cases: &[(&str, &str, Value)]) {
    for (name, source, expected) in cases {
        assert_eq!(run_source(source).expect(name), *expected, "{name}");
        assert_eq!(check_source(source).expect(name), Type::Int, "{name}");
    }
}

fn assert_project_runtime_case(name: &str, files: &[(&str, &str)], entry: &str, expected: Value) {
    let root = temp_project_dir(name);
    for (relative, source) in files {
        write_project_file(&root, relative, source);
    }
    let entry = root.join(entry);
    assert_eq!(run_project_file(&entry).expect(name), expected, "{name}");
    assert_eq!(check_project_file(&entry).expect(name), Type::Int, "{name}");
}

fn assert_runtime_string_cases(cases: &[(&str, &str, &str)]) {
    for (name, source, expected) in cases {
        assert_eq!(
            run_source(source).expect(name),
            Value::String((*expected).into()),
            "{name}"
        );
    }
}

fn assert_check_rejects(cases: &[(&str, &str, &str)]) {
    for (name, source, expected_message) in cases {
        let error = check_source(source).expect_err(name);
        assert!(
            error.to_string().contains(expected_message),
            "{name}: expected error containing `{expected_message}`, got {error}"
        );
    }
}

#[test]
fn evaluates_types_column_arbitrary_precision_numeric_runtime() {
    assert_runtime_string_cases(&[
        (
            "big integer addition crosses i64 max",
            "ToString(9223372036854775807 + 1)",
            "9223372036854775808",
        ),
        (
            "big integer multiplication remains exact",
            "ToString(3037000500 * 3037000500)",
            "9223372037000250000",
        ),
        (
            "big rational numerator reduces exactly",
            r#"
if:
    Half := 9223372036854775808 / 2
    Half = 4611686018427387904
then:
    "ok"
else:
    "bad"
"#,
            "ok",
        ),
        (
            "minimum i128 rational numerator reduces exactly",
            r#"
if:
    Half := (-170141183460469231731687303715884105727 - 1) / 2
    Half = -85070591730234615865843651857942052864
then:
    "ok"
else:
    "bad"
"#,
            "ok",
        ),
    ]);
}

#[test]
fn evaluates_types_column_float_only_numeric_decides_surfaces() {
    assert_runtime_cases(&[(
        "float decides helpers still run",
        r#"
Close := if (IsAlmostEqual[1.0, 1.01, 0.02]). 20 else. 0
Zero := if ((0.01).IsAlmostZero[0.02]). 20 else. 0
Finite := if (Value := (2.0).IsFinite[]). if (Rounded := Round[Value]). Rounded else. 0 else. 0
Close + Zero + Finite
"#,
        Value::Int(42),
    )]);

    assert_check_rejects(&[
        (
            "rational IsAlmostEqual is not a float overload",
            r#"
if:
    Half := 1 / 2
    IsAlmostEqual[Half, Half, 0.1]
then:
    1
else:
    0
"#,
            "float",
        ),
        (
            "rational IsAlmostZero receiver is not a float receiver",
            r#"
if:
    Half := 1 / 2
    Half.IsAlmostZero[0.1]
then:
    1
else:
    0
"#,
            "IsAlmostZero",
        ),
        (
            "rational IsFinite receiver is not a float receiver",
            r#"
if:
    Half := 1 / 2
    Half.IsFinite[]
then:
    1
else:
    0
"#,
            "IsFinite",
        ),
    ]);
}

#[test]
fn evaluates_types_column_full_type_function_runtime_surfaces() {
    assert_runtime_cases(&[
        (
            "higher-order type former parameter",
            r#"
ListOf(Kind:type):type = []Kind
Use(Former:type{_(:type):type}, Kind:type, Item:Former(Kind)):int =
    42
Use(ListOf, int, array{42})
"#,
            Value::Int(42),
        ),
        (
            "static type function returning function signature",
            r#"
HandlerOf(Result:type):type = type{_(:int):Result}
Make(Kind:type):HandlerOf(Kind) = external {}
Make(int)(0) + 42
"#,
            Value::Int(42),
        ),
        (
            "static type function chained through option and array formers",
            r#"
ListOf(Kind:type):type = []Kind
MaybeList(Kind:type):type = ?ListOf(Kind)
Value:MaybeList(int) = option{array{42}}
if (Items := Value?):
    if (Item := Items[0]). Item else. 0
else. 0
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
fn evaluates_types_column_richer_type_parameter_constraints() {
    assert_runtime_cases(&[
        (
            "dependent subtype and comparable constraint chain",
            r#"
box(t:type) := class:
    Value:t
Read(Box:t where t:subtype(box(u)), u:subtype(comparable)):u = external {}
Read(box(int){Value := 0}) + 42
"#,
            Value::Int(42),
        ),
        (
            "dependent nested concrete/castable constraint",
            r#"
base_tag := class<abstract><unique>:
    Marker:int = 0
tagged := class<concrete><castable>(base_tag):
    Value:int = 0
Pick(Kind:t where t:concrete_subtype(castable_subtype(k)), k:type):k = external {}
Pick(tagged).Value + 42
"#,
            Value::Int(42),
        ),
        (
            "type-bounds supplier with dependent lower and upper",
            r#"
base_item := class:
    Value:int = 40
child_item := class(base_item):
    Marker:int = 0
Bounds(Lower:type, Upper:type):type = type(Lower, Upper)
Pick(Kind:Bounds(child_item, base_item)):Kind = external {}
Pick(child_item).Value + 42
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
fn evaluates_types_column_generated_parametric_member_surfaces() {
    assert_runtime_cases(&[
        (
            "constructed parametric class preserves external method return type",
            r#"
box(t:type) := class:
    Value:t
    Read():t = external {}
box(int){Value := 0}.Read() + 42
"#,
            Value::Int(42),
        ),
        (
            "parametric field default materializes through constructed archetype",
            r#"
box(t:type) := class:
    Value:t
holder(t:type) := class:
    Child:box(t) = external {}
holder(int){}.Child.Value + 42
"#,
            Value::Int(42),
        ),
        (
            "class-scoped extension over parametric interface receiver",
            r#"
reader(t:type) := interface:
    Read():t
box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = external {}
    (Item:reader(t)).ReadValue<public>():t = external {}
    Use<public>():t = Self.ReadValue()
box(int){Value := 0}.Use() + 42
"#,
            Value::Int(42),
        ),
        (
            "interface default method returns generated parametric aggregate",
            r#"
box(t:type) := class:
    Value:t
maker(t:type) := interface:
    Make():box(t) = external {}
item(t:type) := class(maker(t)):
    Marker:int = 0
Item:item(int) = item(int){}
Item.Make().Value + 42
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
fn evaluates_types_column_cross_module_parametric_surfaces() {
    assert_project_runtime_case(
        "public type function returns generated aggregate across using import",
        &[
            (
                "DataTypes.verse",
                r#"
DataTypes<public> := module:
    box<public>(t:type) := class:
        Value<public>:t
    BoxOf<public>(Kind:type):type = box(Kind)
    Make<public>(Kind:type):BoxOf(Kind) = external {}
"#,
            ),
            (
                "main.verse",
                r#"
using { DataTypes }
Make(int).Value + 42
"#,
            ),
        ],
        "main.verse",
        Value::Int(42),
    );

    assert_project_runtime_case(
        "qualified dependent type-value external return crosses module boundary",
        &[
            (
                "DataTypes.verse",
                r#"
DataTypes<public> := module:
    box<public>(t:type) := class:
        Value<public>:t
    Pick<public>(Kind:type):Kind = external {}
"#,
            ),
            (
                "main.verse",
                r#"
DataTypes.Pick(DataTypes.box(int)).Value + 42
"#,
            ),
        ],
        "main.verse",
        Value::Int(42),
    );
}
