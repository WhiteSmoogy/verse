//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_class_scope_extension_method_archetype_of_defining_class() {
    let source = r#"
counter := class:
    Value:int = 0

    (NewValue:int).AsCounter():counter =
        counter{Value := NewValue}

    Use():int =
        42.AsCounter().Value

counter{}.Use()
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_official_locale_empty_struct() {
    let source = r#"
Locale:locale = locale{}
if (Locale = locale{}):
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
fn evaluates_struct_defaults_construction_and_field_access() {
    let source = r#"
vector2 := struct:
    X : int = 0
    Y : int = 0

Origin := vector2{}
PlayerPos := vector2{X := 40, Y := 2}
Origin.X + PlayerPos.X + PlayerPos.Y
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn rejects_struct_field_default_recursively_constructing_same_struct() {
    let error = check_source(
        r#"
node := struct:
    Child : node = node{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field default `node.Child` recursively constructs `node`"),
        "{error}"
    );
}

#[test]
fn rejects_struct_field_default_computes_call() {
    let error = check_source(
        r#"
Make()<computes>:int = 42

bad := struct:
    Value:int = Make()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <converges> effect cannot call function requiring <computes> effect"
    ));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_method_archetype_of_defining_class() {
    let source = r#"
counter := class:
    Value:int = 0

    WithValue(NewValue:int):counter =
        counter{Value := NewValue}

Counter := counter{}
Counter.WithValue(42).Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_private_class_field_archetype_inside_defining_class() {
    let source = r#"
counter := class:
    Value<private>:int = 0

    WithValue(NewValue:int):counter =
        counter{Value := NewValue}

    Reveal():int = Value

counter{}.WithValue(42).Reveal()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_struct_map_key_with_comparable_fields() {
    let source = r#"
record_key := struct:
    ID:int
    Label:?string = false

Scores:[record_key]int = map{record_key{ID := 7, Label := option{"ready"}} => 42}
if (Value := Scores[record_key{ID := 7, Label := option{"ready"}}]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
