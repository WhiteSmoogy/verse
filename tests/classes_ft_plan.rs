//! Executable inventory for finishing the remaining Classes/interfaces FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn assert_runtime_cases(cases: &[(&str, &str, Value, Type)]) {
    for (name, source, expected, expected_type) in cases {
        assert_eq!(run_source(source).expect(name), expected.clone(), "{name}");
        assert_eq!(
            check_source(source).expect(name),
            expected_type.clone(),
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
fn evaluates_constructor_delegation_surfaces() {
    assert_runtime_cases(&[
        (
            "same-class constructor delegates to parent-delegating constructor",
            r#"
entity := class:
    Name:string
    Health:int

MakeEntity<constructor>(Name:string, Health:int) := entity:
    Name := Name
    Health := Health

character := class(entity):
    Class:string
    Level:int

MakeCharacterCore<constructor>(Name:string, Class:string, Level:int) := character:
    Class := Class
    Level := Level
    MakeEntity<constructor>(Name, Level * 9)

MakeCharacter<constructor>(Name:string) := character:
    MakeCharacterCore<constructor>(Name, "Go", 4)

Hero := MakeCharacter("Ava")
Hero.Health + Hero.Level + Hero.Class.Length
"#,
            Value::Int(42),
            Type::Int,
        ),
        (
            "delegation selects overloaded parent constructor",
            r#"
entity := class:
    Name:string
    Score:int

MakeEntity<constructor>(Name:string) := entity:
    Name := Name
    Score := 1

MakeEntity<constructor>(Score:int) := entity:
    Name := "score"
    Score := Score

player := class(entity):
    Bonus:int

MakePlayer<constructor>() := player:
    Bonus := 2
    MakeEntity<constructor>(40)

Hero := MakePlayer()
Hero.Score + Hero.Bonus
"#,
            Value::Int(42),
            Type::Int,
        ),
    ]);

    assert_check_rejects(&[(
        "constructor delegation cannot target subclass constructor from base archetype",
        r#"
base := class:
    Name:string

child := class(base):
    Score:int

MakeChild<constructor>() := child:
    Name := "Ava"
    Score := 42

base:
    MakeChild<constructor>()
"#,
        "not `base` or a superclass",
    )]);
}

#[test]
fn evaluates_class_block_failure_effect_rollback() {
    assert_runtime_cases(&[
        (
            "failed option construction rolls back escaped class block result",
            r#"
counter := class:
    var Value:int = 0

    block:
        set Value = 99

var Saved:?counter = false

Maybe:?counter = option{
    Item := counter{}
    set Saved = option{Item}
    false?
    Item
}

if (Maybe?):
    0
else:
    if (Saved?). 0 else. 42
"#,
            Value::Int(42),
            Type::Int,
        ),
        (
            "failed decides construction rolls back escaped class block result",
            r#"
counter := class:
    var Value:int = 0

    block:
        set Value = 99

var Saved:?counter = false

Make()<decides><transacts>:counter =
    Item := counter{}
    set Saved = option{Item}
    false?
    Item

if (Make[]):
    0
else:
    if (Saved?). 0 else. 42
"#,
            Value::Int(42),
            Type::Int,
        ),
        (
            "subclass construction includes inherited and local block effects",
            r#"
base := class:
    var Hits:int = 0

    block:
        set Hits += 10

child := class(base):
    block:
        set Hits += 30

Item := child{}
Item.Hits + 2
"#,
            Value::Int(42),
            Type::Int,
        ),
    ]);

    assert_check_rejects(&[
        (
            "computes construction rejects inherited block transaction effect",
            r#"
base := class:
    var Hits:int = 0

    block:
        set Hits += 1

child := class(base):
    Bonus:int = 2

Make()<computes>:child = child{}
"#,
            "function with <computes> effect cannot call function requiring <transacts> effect",
        ),
        (
            "class block rejects failable decides call",
            r#"
Pick()<decides><transacts>:int = 42

counter := class:
    block:
        Pick[]
"#,
            "class block cannot contain failable expressions",
        ),
    ]);
}

#[test]
#[ignore = "planned Classes/interfaces column: generic and qualified interface dispatch"]
fn planned_generic_qualified_interface_dispatch() {
    assert_runtime_cases(&[
        (
            "parametric interface default method dispatches through override",
            r#"
reader(t:type) := interface:
    Value:t
    Read():t = Value
    Pair():tuple(t, t) = (Read(), Read())

box(t:type) := class(reader(t)):
    Value<override>:t
    Read<override>():t = Value

Value:reader(int) = box(int){Value := 21}
Pair := Value.Pair()
Pair(0) + Pair(1)
"#,
            Value::Int(42),
            Type::Int,
        ),
        (
            "module-qualified colliding interface methods dispatch by qualifier",
            r#"
Contracts<public> := module:
    left<public> := interface:
        Score<public>():int

    right<public> := interface:
        Score<public>():int

combo := class(Contracts.left, Contracts.right):
    (Contracts.left:)Score<override><public>():int =
        40

    (Contracts.right:)Score<override><public>():int =
        2

Obj := combo{}
Left:Contracts.left = Obj
Right:Contracts.right = Obj
Left.(Contracts.left:)Score() + Right.(Contracts.right:)Score()
"#,
            Value::Int(42),
            Type::Int,
        ),
        (
            "module-qualified parametric interface default dispatches via implementing class",
            r#"
Contracts<public> := module:
    reader<public>(t:type) := interface:
        Value<public>:t
        Read<public>():t = Value
        Score<public>():int = Read()

box := class(Contracts.reader(int)):
    Value<override><public>:int = 42
    Read<override><public>():int = Value

Value:Contracts.reader(int) = box{}
Value.Score()
"#,
            Value::Int(42),
            Type::Int,
        ),
    ]);

    assert_check_rejects(&[(
        "module-qualified interface collision still requires qualified override",
        r#"
Contracts<public> := module:
    left<public> := interface:
        Score<public>():int

    right<public> := interface:
        Score<public>():int

combo := class(Contracts.left, Contracts.right):
    Score<override><public>():int =
        42
"#,
        "override is ambiguous; use a qualified method name",
    )]);
}
