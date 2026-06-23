//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_extension_method_on_class_instance() {
    let source = r#"
counter := class:
    Value:int = 0

(Counter:counter).Double<public>():int = Counter.Value * 2

counter{Value := 21}.Double()
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_type_value_extension_accessor() {
    let source = r#"
axis := struct:
    X:int = 0
    Y:int = 0

(Kind:type).XAxis<public>()<computes>:axis =
    axis{X := 40, Y := 2}

Value := axis.XAxis
Value.X + Value.Y
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_qualified_type_value_extension_accessor() {
    let source = r#"
axis := struct:
    X:int = 0
    Y:int = 0

Ops<public> := module:
    (Kind:type).YAxis<public>()<computes>:axis =
        axis{X := 20, Y := 22}

using { Ops }
Value := axis.(Ops:)YAxis
Value.X + Value.Y
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_value_extension_accessor_with_parameters() {
    let error = check_source(
        r#"
axis := struct:
    X:int = 0

(Kind:type).Shift<public>(Offset:int)<computes>:int =
    Offset

Value := axis.Shift
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("extension method `Shift` must be called")
    );
}

#[test]
fn evaluates_qualified_extension_method_disambiguates_imported_modules() {
    let source = r#"
First<public> := module:
    (Value:int).Bump<public>():int = Value + 1

Second<public> := module:
    (Value:int).Bump<public>():int = Value + 100

using { First }
using { Second }
40.(First:)Bump() + 1.(Second:)Bump()
"#;

    assert_eq!(eval(source), Value::Int(142));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_qualified_extension_method_reference_without_call() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    (Value:int).Bump<public>():int = Value + 1

using { DataTypes }
Ref := 41.(DataTypes:)Bump
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("extension method `(DataTypes:)Bump` must be called")
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

    assert_eq!(eval(source), Value::Int(50));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_scope_extension_method_overloads_by_receiver_runtime() {
    let source = r#"
manager := class:
    (Value:string).Score():int =
        42
    (Value:int).Score():int =
        0

    Use():int =
        "ready".Score()

manager{}.Use()
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
    Trigger():int =
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

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_protected_var_assignment_inside_implementer() {
    let source = r#"
triggerable := interface:
    var<protected> Triggered<public>:logic = false

button := class(triggerable):
    Activate()<transacts>:void =
        set Self.Triggered = true

Target := button{}
Target.Activate()
if (Target.Triggered?). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_interface_protected_var_assignment_outside_implementer() {
    let error = check_source(
        r#"
triggerable := interface:
    var<protected> Triggered<public>:logic = false

button := class(triggerable):
    ID:int = 0

Target:triggerable = button{}
set Target.Triggered = true
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Triggered` is protected in interface `triggerable`")
    );
}

#[test]
fn evaluates_default_public_interface_var_assignment_with_protected_read() {
    let source = r#"
counter := interface:
    var Value<protected>:int = 0

button := class(counter):
    ID:int = 0

Target:counter = button{}
set Target.Value = 42
42
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_protected_field_with_default() {
    let source = r#"
secret := interface:
    Hidden<protected>:int = 40

box := class(secret):
    Reveal():int = Hidden + 2

box{}.Reveal()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_required_protected_field_with_class_default_override() {
    let source = r#"
secret := interface:
    Hidden<protected>:int

box := class(secret):
    Hidden<override><protected>:int = 40
    Reveal():int = Hidden + 2

box{}.Reveal()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_class_missing_interface_protected_field_default_through_run_source() {
    let error = check_source(
        r#"
secret := interface:
    Hidden<protected>:int

box := class(secret):
    Score:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "class `box` must be `abstract` or provide a default value for interface field `Hidden`"
    ));
}

#[test]
fn rejects_interface_required_private_field_without_class_default() {
    let error = check_source(
        r#"
secret := interface:
    Hidden<private>:int

box := class(secret):
    Score:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "class `box` must be `abstract` or provide a default value for interface field `Hidden`"
    ));
}

#[test]
fn evaluates_abstract_class_inherits_required_protected_interface_field() {
    let source = r#"
secret := interface:
    Hidden<protected>:int

box := class<abstract>(secret):
    Score():int = 42

box
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::ClassType("box".into())
    );
}

#[test]
fn rejects_interface_required_protected_field_without_class_default() {
    let error = run_source(
        r#"
secret := interface:
    Hidden<protected>:int

box := class(secret):
    Score:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "class `box` must be `abstract` or provide a default value for interface field `Hidden`"
    ));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_builtin_showable_interface_field() {
    let source = r#"
panel := class(showable):
    var Show<override><public>:?logic = false

Panel := panel{}
Showable:showable = Panel
set Showable.Show = option{true}
if (Panel.Show?):
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
    var Show<override><public>:logic = true
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
fn evaluates_native_class_binding_specifier() {
    let source = r#"
widget<native><public> := class<concrete>:
    Value:int = 42

widget{}.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_native_class_inheriting_native_class() {
    let source = r#"
base<native> := class:
    Value:int = 40

child<native> := class(base):
    Bonus:int = 2

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_native_class_inheriting_non_native_class() {
    let error = check_source(
        r#"
base := class:
    Value:int = 40

child<native> := class(base):
    Bonus:int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("native class `child` cannot inherit from non-native class `base`"),
        "{error}"
    );
}

#[test]
fn evaluates_native_class_member_with_native_struct_type() {
    let source = r#"
point<native> := struct:
    X<native>:int = 0

holder<native> := class:
    P:point = point{}

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_native_class_member_with_non_native_struct_type() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

holder<native> := class:
    P:point = point{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`struct point` contained as a member in a native type must also be native"),
        "{error}"
    );
}

#[test]
fn rejects_native_field_in_non_native_class() {
    let error = check_source(
        r#"
holder := class:
    Value<native>:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`native` field specifier requires a native enclosing type"),
        "{error}"
    );
}

#[test]
fn rejects_native_method_in_non_native_class() {
    let error = check_source(
        r#"
holder := class:
    Read<native>():int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`native` method specifier requires a native enclosing class"),
        "{error}"
    );
}

#[test]
fn rejects_native_method_with_non_native_struct_parameter() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

holder<native> := class:
    Use<native>(P:point):void
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
fn checks_decides_abstract_class_method_without_call_effect() {
    let source = r#"
picker := class<abstract>:
    Pick()<decides>:int

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_decides_interface_method_without_call_effect() {
    let source = r#"
picker := interface:
    Pick()<decides>:int

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_writes_method_class_field_assignment() {
    let source = r#"
counter := class:
    var Value:int = 0

    Increment()<writes>:void =
        set Value += 42

Counter := counter{}
Counter.Increment()
Counter.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_default_class_method_field_assignment() {
    let source = r#"
counter := class:
    var Value:int = 0

    Increment():void =
        set Value += 42

Counter := counter{}
Counter.Increment()
Counter.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_default_class_method_call_in_failure_context() {
    let source = r#"
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
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn evaluates_official_ordinary_named_method_arguments() {
    let source = r#"
counter := class:
    Add(Left:int, Right:int):int = Left + Right

counter{}.Add(Right := 2, Left := 40)
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_external_parametric_class_method_return_runtime_surface() {
    let source = r#"
box(t:type) := class:
    Value:t
    Read():t = external {}

Item:box(int) = external {}
Item.Read() + 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_shared_constraint_parametric_class_instance_methods() {
    let source = r#"
pair_box(t&u:type) := class:
    Left:t
    Right:u
    ReadLeft():t = Left

Box := pair_box(int, string){Left := 42, Right := "ready"}
Box.ReadLeft() + Box.Right.Length
"#;

    assert_eq!(eval(source), Value::Int(47));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_bounds_parametric_class_instance_methods() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

box(t:type(child_item, base_item)) := class:
    Item:t
    Read():int = Item.Value

Box := box(child_item){Item := child_item{}}
Box.Read()
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_type_bounds_parametric_class_upper_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
other_item := class:
    Value:int = 1

box(t:type(child_item, base_item)) := class:
    Item:t

Bad:box(other_item) = external {}
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
fn rejects_type_bounds_parametric_class_lower_mismatch() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0
grandchild_item := class(child_item):
    GrandchildValue:int = 0

box(t:type(child_item, base_item)) := class:
    Item:t

Bad:box(grandchild_item) = external {}
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
fn evaluates_castable_concrete_parametric_class_constraints() {
    let source = r#"
base_tag := class(tag){}
child_tag := class<concrete><castable>(base_tag){}

castable_box(t:castable_subtype(base_tag)) := class:
    Item:t
    Count():int = 1

prefab_box(t:concrete_subtype(castable_subtype(base_tag))) := class:
    Item:t
    Count():int = 2

short_prefab_box(t:castable_concrete_subtype(base_tag)) := class:
    Item:t
    Count():int = 3

Castable := castable_box(child_tag){Item := child_tag{}}
Prefab := prefab_box(child_tag){Item := child_tag{}}
Short := short_prefab_box(child_tag){Item := child_tag{}}
Castable.Count() + Prefab.Count() + Short.Count()
"#;

    assert_eq!(eval(source), Value::Int(6));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_castable_parametric_class_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
plain_tag := class<concrete>(base_tag){}

castable_box(t:castable_subtype(base_tag)) := class:
    Item:t

Bad:castable_box(plain_tag) = external {}
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
fn rejects_concrete_parametric_class_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
castable_tag := class<castable>(base_tag){}

prefab_box(t:concrete_subtype(castable_subtype(base_tag))) := class:
    Item:t

Bad:prefab_box(castable_tag) = external {}
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
fn evaluates_user_parametric_class_parent_runtime_surface() {
    let source = r#"
base_box(t:type) := class:
    Elements<public>:[]t = array{}
    Count<public>()<transacts>:int =
        Elements.Length

child_box(t:type) := class(base_box(t)):
    Extra<public>:int = 2

Box := child_box(int){}
Box.Count() + Box.Extra
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_user_parametric_interface_parent_runtime_surface() {
    let source = r#"
base_view(t:type) := interface:
    Elements<public>:[]t = array{}

child_view(t:type) := interface(base_view(t)):
    Extra<public>:int = 2

box := class(child_view(int)):
    Label:string = "ok"

Value:child_view(int) = box{}
Value.Elements.Length + Value.Extra
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_bounds_parametric_interface_runtime_surface() {
    let source = r#"
base_item := class:
    Value:int = 1
child_item := class(base_item):
    ChildValue:int = 0

reader(t:type(child_item, base_item)) := interface:
    Read():t

box := class(reader(child_item)):
    Read<override>():child_item = child_item{}

Value:reader(child_item) = box{}
Value.Read().Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_castable_concrete_parametric_interface_constraints() {
    let source = r#"
base_tag := class(tag){}
child_tag := class<concrete><castable>(base_tag){}

castable_reader(t:castable_subtype(base_tag)) := interface:
    ReadCastable():t

prefab_reader(t:concrete_subtype(castable_subtype(base_tag))) := interface:
    ReadPrefab():t

short_prefab_reader(t:castable_concrete_subtype(base_tag)) := interface:
    ReadShort():t

box := class(castable_reader(child_tag), prefab_reader(child_tag), short_prefab_reader(child_tag)):
    ReadCastable<override>():child_tag = child_tag{}
    ReadPrefab<override>():child_tag = child_tag{}
    ReadShort<override>():child_tag = child_tag{}

Value:castable_reader(child_tag) = box{}
Other:prefab_reader(child_tag) = box{}
Short:short_prefab_reader(child_tag) = box{}
First := Value.ReadCastable()
Second := Other.ReadPrefab()
Third := Short.ReadShort()
if (First = Second, Third = child_tag{}). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_castable_parametric_interface_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
plain_tag := class<concrete>(base_tag){}

castable_reader(t:castable_subtype(base_tag)) := interface:
    Read():t

Bad:castable_reader(plain_tag) = external {}
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
fn rejects_concrete_parametric_interface_constraint_mismatch() {
    let source = r#"
base_tag := class(tag){}
castable_tag := class<castable>(base_tag){}

prefab_reader(t:concrete_subtype(castable_subtype(base_tag))) := interface:
    Read():t

Bad:prefab_reader(castable_tag) = external {}
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
fn evaluates_user_parametric_interface_parent_method_runtime_surface() {
    let source = r#"
base_reader(t:type) := interface:
    Read<public>():t

child_reader(t:type) := interface(base_reader(t)):
    Extra<public>:int = 2

box := class(child_reader(int)):
    Value:int
    Read<override><public>():int = Value

Value:child_reader(int) = box{Value := 40}
Value.Read() + Value.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_user_parametric_interface_parent_default_method_runtime_surface() {
    let source = r#"
base_reader(t:type) := interface:
    Value<public>:t
    Read<public>():t = Value

child_reader(t:type) := interface(base_reader(t)):
    Extra<public>:int = 2

box := class(child_reader(int)):
    Value<override><public>:int = 40

Value:child_reader(int) = box{}
Value.Read() + Value.Extra
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_user_parametric_interface_method_runtime_surface() {
    let source = r#"
reader(t:type) := interface:
    Read<public>():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override><public>():t = Value

Value:reader(int) = box(int){Value := 42}
Value.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_user_parametric_class_scoped_extension_runtime_surface() {
    let source = r#"
box(t:type) := class:
    Value:t
    (Item:box(t)).Read<public>():t = Item.Value
    Use<public>():t = Self.Read()

Value := box(int){Value := 42}
Value.Use()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_extension_receiver() {
    let source = r#"
box(t:type) := class:
    Value:t

(Item:box(t) where t:type).Read():t =
    Item.Value

box(int){Value := 42}.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_extension_base_receiver() {
    let source = r#"
base_box(t:type) := class:
    Value:t

child_box(t:type) := class(base_box(t)):
    Extra:int = 2

(Item:base_box(t) where t:type).Read():t =
    Item.Value

child_box(int){Value := 42}.Read()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_type_parameter_inference_from_parametric_extension_interface_receiver() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

(Item:reader(t) where t:type).ReadValue():t =
    Item.Read()

box(int){Value := 42}.ReadValue()
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_parametric_extension_receiver_inferred_return_mismatch() {
    let source = r#"
box(t:type) := class:
    Value:t

(Item:box(t) where t:type).Read():t =
    Item.Value

Value:int = box(string){Value := "bad"}.Read()
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `int`"));
}

#[test]
fn rejects_parametric_extension_interface_receiver_inferred_return_mismatch() {
    let source = r#"
reader(t:type) := interface:
    Read():t

box(t:type) := class(reader(t)):
    Value:t
    Read<override>():t = Value

(Item:reader(t) where t:type).ReadValue():t =
    Item.Read()

Value:int = box(string){Value := "bad"}.ReadValue()
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(error.to_string().contains("annotated as `int`"));
}

#[test]
fn rejects_official_parametric_class_final_field_override() {
    let source = r#"
base_box(t:type) := class:
    Elements<final>:[]t = array{}

child_box(t:type) := class(base_box(t)):
    Elements<override>:[]t = array{}

Value:child_box(int) = child_box(int){}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("field `Elements` overrides final inherited field `Elements`")
    );
}

#[test]
fn runtime_errors_on_parametric_class_final_field_override() {
    let source = r#"
base_box(t:type) := class:
    Elements<final>:[]t = array{}

child_box(t:type) := class(base_box(t)):
    Elements<override>:[]t = array{}

child_box(int)
"#;
    let error = run_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Elements` overrides final inherited field `Elements`")
    );
}

#[test]
fn rejects_official_parametric_interface_final_field_override() {
    let source = r#"
base_view(t:type) := interface:
    Elements<final>:[]t = array{}

child_view(t:type) := interface(base_view(t)):
    Elements<override>:[]t = array{}

Value:child_view(int) = external {}
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("field `Elements` overrides final inherited field `Elements`")
    );
}

#[test]
fn runtime_errors_on_parametric_interface_final_field_override() {
    let source = r#"
base_view(t:type) := interface:
    Elements<final>:[]t = array{}

child_view(t:type) := interface(base_view(t)):
    Elements<override>:[]t = array{}

child_view(int)
"#;
    let error = run_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Elements` overrides final inherited field `Elements`")
    );
}

#[test]
fn evaluates_subtype_class_constraint_member_access_from_official_style() {
    let source = r#"
base_class := class:
    Property:int = 40
    Read():int =
        Property + 1

child_class := class(base_class):
    Extra:int = 2

GetBaseValue(X:base_class):int =
    X.Property

UseChild(X:child_class):int =
    X.Extra

Preserve(X:t where t:subtype(base_class)):t =
    X

Foo(X:t where t:subtype(base_class)):tuple(t, int) =
    (X, X.Property + X.Read() + GetBaseValue(X))

Result := Foo(Preserve(child_class{}))
UseChild(Result(0)) + Result(1)
"#;

    assert_eq!(eval(source), Value::Int(123));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subtype_class_constraint_extension_method_access() {
    let source = r#"
DataTypes<public> := module:
    (Base:base_class).Bump<public>():int =
        Base.Value + 1

using { DataTypes }

base_class := class:
    Value<public>:int = 40

child_class := class(base_class):
    Extra:int = 2

Use(X:t where t:subtype(base_class)):int =
    X.Bump() + X.(DataTypes:)Bump()

Use(child_class{})
"#;

    assert_eq!(eval(source), Value::Int(82));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subtype_interface_constraint_member_access() {
    let source = r#"
named := interface:
    Name:string

named_box := class(named):
    Name<override>:string = "box"

ReadName(X:t where t:subtype(named)):string =
    X.Name

ReadName(named_box{})
"#;

    assert_eq!(eval(source), Value::String("box".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_subtype_class_constraint_argument_mismatch() {
    let source = r#"
base_class := class:
    Property:int = 40

other_class := class:
    Property:int = 40

Foo(X:t where t:subtype(base_class)):int =
    X.Property

Foo(other_class{})
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("must be a subtype of `base_class`")
    );
}

#[test]
fn runtime_errors_on_subtype_class_constraint_argument_mismatch() {
    let source = r#"
base_class := class:
    Property:int = 40

other_class := class:
    Property:int = 40

Foo(X:t where t:subtype(base_class)):int =
    42

Foo(other_class{})
"#;

    let error = run_source(source).expect_err("source should fail at runtime");

    assert!(
        error
            .to_string()
            .contains("must be a subtype of `base_class`")
    );
}

#[test]
fn rejects_subtype_class_constraint_missing_member_access() {
    let source = r#"
base_class := class:
    Property:int = 40

Foo(X:t where t:subtype(base_class)):int =
    X.Missing
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("class `base_class` has no member `Missing`")
    );
}

#[test]
fn evaluates_castable_class_type_as_castable_subtype() {
    let source = r#"
puzzle_light := class<castable>(tag){}
TagType:castable_subtype(tag) = puzzle_light
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
fn evaluates_inherited_castable_class_type_as_castable_subtype() {
    let source = r#"
root_tag := class<castable>(tag){}
leaf_tag := class(root_tag){}
RootType:castable_subtype(tag) = leaf_tag
LeafType:castable_subtype(root_tag) = leaf_tag
Use(Root:castable_subtype(tag), Leaf:castable_subtype(root_tag)):int = 42
Use(RootType, LeafType)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_type_as_subtype() {
    let source = r#"
base_item := class:
    Value:int = 40
child_item := class(base_item):
    Extra:int = 2
BaseType:subtype(base_item) = base_item
ChildType:subtype(base_item) = child_item
Use(Kind:subtype(base_item)):int = 21
Use(BaseType) + Use(ChildType)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_interface_implementer_class_type_as_subtype() {
    let source = r#"
named := interface:
    Name:string
widget := class(named):
    Name<override>:string = "ready"
WidgetType:subtype(named) = widget
Use(Kind:subtype(named)):int = 42
Use(WidgetType)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unrelated_class_type_as_subtype() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 0
other_item := class:
    Value:int = 1
ItemType:subtype(base_item) = other_item
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "annotated as `subtype(base_item)` but expression has type `class<other_item>`"
        )
    );
}

#[test]
fn rejects_non_class_type_value_as_subtype() {
    let error = check_source(
        r#"
base_item := class:
    Value:int = 0
Item:subtype(base_item) = base_item{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `subtype(base_item)` but expression has type `base_item`")
    );
}

#[test]
fn evaluates_castable_class_type_function_argument_runtime() {
    let source = r#"
puzzle_light := class<castable>(tag){}
Use(TagType:castable_subtype(tag)):int = 42
Use(puzzle_light)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_castable_class_type_as_castable_subtype() {
    let error = check_source(
        r#"
puzzle_light := class(tag){}
TagType:castable_subtype(tag) = puzzle_light
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "annotated as `castable_subtype(tag)` but expression has type `class<puzzle_light>`"
    ));
}

#[test]
fn rejects_unrelated_castable_class_type_as_castable_subtype() {
    let error = check_source(
        r#"
thing := class<castable>:
    Value:int = 0
TagType:castable_subtype(tag) = thing
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "annotated as `castable_subtype(tag)` but expression has type `class<thing>`"
        )
    );
}

#[test]
fn rejects_non_castable_class_type_argument() {
    let error = run_source(
        r#"
puzzle_light := class(tag){}
Use(TagType:castable_subtype(tag)):int = 42
Use(puzzle_light)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `castable_subtype(tag)`, got `class<puzzle_light>`")
    );
}

#[test]
fn evaluates_concrete_class_type_as_concrete_subtype() {
    let source = r#"
puzzle_light := class<concrete>(tag){}
TagType:concrete_subtype(tag) = puzzle_light
Use(TagType:concrete_subtype(tag)):int = 42
Use(puzzle_light)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_concrete_castable_class_type_as_nested_concrete_subtype() {
    let source = r#"
puzzle_light := class<concrete><castable>(tag){}
TagType:concrete_subtype(castable_subtype(tag)) = puzzle_light
Use(TagType:concrete_subtype(castable_subtype(tag))):int = 42
Use(puzzle_light)
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_concrete_class_type_as_concrete_subtype() {
    let error = check_source(
        r#"
puzzle_light := class<castable>(tag){}
TagType:concrete_subtype(castable_subtype(tag)) = puzzle_light
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "annotated as `concrete_subtype(castable_subtype(tag))` but expression has type `class<puzzle_light>`"
    ));
}

#[test]
fn rejects_non_castable_class_type_as_nested_concrete_subtype() {
    let error = check_source(
        r#"
puzzle_light := class<concrete>(tag){}
TagType:concrete_subtype(castable_subtype(tag)) = puzzle_light
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "annotated as `concrete_subtype(castable_subtype(tag))` but expression has type `class<puzzle_light>`"
    ));
}

#[test]
fn rejects_unrelated_concrete_class_type_as_concrete_subtype() {
    let error = check_source(
        r#"
thing := class<concrete>:
    Value:int = 0
TagType:concrete_subtype(tag) = thing
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "annotated as `concrete_subtype(tag)` but expression has type `class<thing>`"
        )
    );
}

#[test]
fn rejects_non_concrete_class_type_argument() {
    let error = run_source(
        r#"
puzzle_light := class(tag){}
Use(TagType:concrete_subtype(tag)):int = 42
Use(puzzle_light)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `concrete_subtype(tag)`, got `class<puzzle_light>`")
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
fn evaluates_self_typed_class_field_without_recursive_default() {
    let source = r#"
node := class:
    var Next : ?node = false

Root := node{}
if (Root.Next?). 0 else. 42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_class_field_default_recursively_constructing_same_class() {
    let error = check_source(
        r#"
node := class:
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
fn rejects_class_field_default_no_rollback_call() {
    let error = check_source(
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
fn rejects_interface_field_default_transacts_call() {
    let error = check_source(
        r#"
Make()<transacts>:int = 42

bad := interface:
    Value:int = Make()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <converges> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_class_field_default_constructing_class_with_block() {
    let error = check_source(
        r#"
worker := class:
    var Value:int = 0
    block:
        set Value += 1

bad := class:
    Worker:worker = worker{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <converges> effect cannot call function requiring <transacts> effect"
    ));
}

#[test]
fn rejects_class_field_default_suspends_call() {
    let error = check_source(
        r#"
Wait()<suspends>:int = 42

bad := class:
    Value:int = Wait()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<suspends>` effect can only be called in an async context")
    );
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
    var<private> Value<public>:int = 0

Counter := counter{}
set Counter.Value = 42
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("private"));
}

#[test]
fn evaluates_default_public_var_assignment_with_protected_read() {
    let source = r#"
counter := class:
    var Value<protected>:int = 0

Counter := counter{}
set Counter.Value = 42
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_default_public_var_read_with_protected_read() {
    let error = check_source(
        r#"
counter := class:
    var Value<protected>:int = 0

Counter := counter{}
Counter.Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("protected"));
}

#[test]
fn rejects_protected_var_assignment_outside_class_hierarchy() {
    let error = check_source(
        r#"
weapon := class:
    var<protected> Ammo<public>:int = 10

Gun := weapon{}
set Gun.Ammo = 15
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Ammo` is protected in class `weapon`")
    );
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
fn rejects_protected_class_constructor_archetype_outside_subclass() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    token<public> := class<protected><concrete>:
        Value<public>:int = 42

DataTypes.token{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class constructor `DataTypes.token` is protected")
    );
}

#[test]
fn evaluates_protected_class_constructor_for_subclass() {
    let source = r#"
DataTypes<public> := module:
    base<public> := class<protected><concrete>:
        Value<public>:int = 40

    child<public> := class<concrete>(base):
        Bonus<public>:int = 2

Item := DataTypes.child{}
Item.Value + Item.Bonus
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_private_base_class_constructor_from_subclass() {
    let error = check_source(
        r#"
DataTypes<public> := module:
    base<public> := class<private><concrete>:
        Value<public>:int = 40

    child<public> := class<concrete>(base):
        Bonus<public>:int = 2

0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("class constructor `DataTypes.base` is private")
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
fn runtime_errors_on_constructor_delegation_to_unrelated_class() {
    let source = r#"
badge := class:
    Label:string

MakeBadge<constructor>():badge =
    badge:
        Label := "VIP"

player := class:
    Label:string

player:
    MakeBadge<constructor>()
"#;

    let error = run_source(source).expect_err("source should fail at runtime");

    assert!(error.to_string().contains("not `player` or a superclass"));
}

#[test]
fn evaluates_overloaded_constructor_functions_before_class_definition() {
    let source = r#"
MakePlayer<constructor>(Name:string):player =
    player:
        Name := Name
        Score := 0

MakePlayer<constructor>(Name:string, Score:int):player =
    player:
        Name := Name
        Score := Score

player := class:
    Name:string
    Score:int

Hero := MakePlayer("Ava", 42)
Default := MakePlayer("Bea")
Hero.Score + Default.Score
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_allocates_function_constructing_unique_class() {
    let source = r#"
token := class<unique>:
    ID:int = 0

MakeToken()<allocates>:token = token{ID := 1}

First := MakeToken()
Second := MakeToken()
if (First <> Second). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_transacts_function_constructing_unique_class() {
    let source = r#"
token := class<unique>:
    ID:int = 0

MakeToken()<transacts>:token = token{ID := 42}

MakeToken().ID
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_varies_function_constructing_unique_class() {
    let source = r#"
token := class<unique>:
    ID:int = 0

MakeToken()<varies>:token = token{ID := 42}

MakeToken().ID
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_no_rollback_function_constructing_unique_class() {
    let error = check_source(
        r#"
token := class<unique>:
    ID:int = 0

MakeToken():token = token{ID := 1}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <no_rollback> effect cannot call function requiring <allocates> effect"
    ));
}

#[test]
fn rejects_computes_function_constructing_unique_class() {
    let error = check_source(
        r#"
token := class<unique>:
    ID:int = 0

MakeToken()<computes>:token = token{ID := 1}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <allocates> effect"
    ));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert!(error.to_string().contains("Duplicate access levels"));
}

#[test]
fn rejects_conflicting_class_access_specifiers() {
    let cases = [
        (
            "class",
            r#"
entity := class<public><internal>:
    Name:string = "entity"
"#,
        ),
        (
            "field",
            r#"
entity := class:
    Data<public><internal>:int = 1
"#,
        ),
        (
            "method",
            r#"
entity := class:
    Score<public><internal>():int = 1
"#,
        ),
        (
            "var field",
            r#"
entity := class:
    var<public><private> Data:int = 1
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
fn rejects_duplicate_class_member_access_specifiers() {
    let cases = [
        (
            "field",
            r#"
entity := class:
    Data<public><public>:int = 1
"#,
        ),
        (
            "method",
            r#"
entity := class:
    Score<public><public>():int = 1
"#,
        ),
        (
            "var field",
            r#"
entity := class:
    var<public><public> Data:int = 1
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
fn evaluates_final_super_component_class_construction() {
    let source = r#"
light_component := class<final_super>(component):
    Value:int = 42

light_component{}.Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subclass_of_final_super_component_without_final_super() {
    let source = r#"
base_component := class<final_super>(component):
    Value:int = 40

child_component := class(base_component):
    Extra:int = 2

Read(Base:base_component):int = Base.Value
Accept(Item:component):int = 2

Read(child_component{}) + Accept(child_component{})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_component_class_without_final_super() {
    let error = check_source(
        r#"
bad_component := class(component):
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("directly inheriting from `component` must specify `<final_super>`")
    );
}

#[test]
fn rejects_final_super_without_component_base() {
    let error = check_source(
        r#"
base := class:
    Value:int = 0

bad_component := class<final_super>(base):
    Extra:int = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("with `<final_super>` must directly inherit from `component`")
    );
}

#[test]
fn rejects_final_super_without_base() {
    let error = check_source(
        r#"
bad_component := class<final_super>:
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("with `<final_super>` must directly inherit from `component`")
    );
}

#[test]
fn rejects_duplicate_final_super_class_specifier() {
    let error = parse_source(
        r#"
bad_component := class<final_super><final_super>(component):
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("duplicate class specifier `final_super`")
    );
}

#[test]
fn runtime_errors_on_component_class_without_final_super() {
    let error = run_source(
        r#"
bad_component := class(component):
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("directly inheriting from `component` must specify `<final_super>`")
    );
}

#[test]
fn evaluates_custom_tag_class_construction() {
    let source = r#"
puzzle_light := class(tag){}

Use(Tag:tag):int = 42

Use(puzzle_light{})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_custom_tag_class_hierarchy() {
    let source = r#"
fruit_tag := class(tag){}
banana_tag := class(fruit_tag){}

UseTag(Tag:tag):int = 20
UseFruit(Tag:fruit_tag):int = 22

UseTag(banana_tag{}) + UseFruit(banana_tag{})
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_non_empty_braced_class_definition() {
    let error = parse_source(
        r#"
bad_tag := class(tag){ Value:int = 0 }
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected `}` after empty class definition")
    );
}

#[test]
fn rejects_builtin_tag_assigned_from_unrelated_class() {
    let error = check_source(
        r#"
not_a_tag := class:
    Value:int = 0

Use(Tag:tag):int = 42
Use(not_a_tag{})
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `tag`, got `not_a_tag`")
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

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_final_class_method_inheritance() {
    let source = r#"
entity := class:
    Score<final>():int = 40

player := class(entity):
    Bonus():int = 2

Hero := player{}
Hero.Score() + Hero.Bonus()
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_concrete_class_with_empty_base_parens() {
    let source = r#"
settings := class<concrete>():
    Score:int = 42

settings{}.Score
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_class_field_override_inheriting_access_level() {
    let source = r#"
base := class:
    Data<protected> : int = 1

child := class(base):
    Data<override> : int = 42
    Reveal<public>():int = Data

child{}.Reveal()
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_editable_parametric_class_field_type_variable() {
    let error = check_source(
        r#"
box(t:type) := class:
    @editable
    Value:t
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(message.contains("`@editable` field `Value`"));
    assert!(message.contains("type parameter"));
}

#[test]
fn rejects_editable_parametric_class_field_nested_type_variable() {
    let error = check_source(
        r#"
box(t:type) := class:
    @editable
    Values:[]t = array{}
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(message.contains("`@editable` field `Values`"));
    assert!(message.contains("type parameter"));
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

    assert_eq!(eval(source), Value::Int(40));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
fn rejects_class_field_override_changing_access_level() {
    let error = check_source(
        r#"
base := class:
    Data<protected> : int = 1

child := class(base):
    Data<override><public> : int = 2
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("cannot change the inherited access level")
    );
}

#[test]
fn rejects_class_var_field_override_widening_omitted_mutation_access() {
    let error = check_source(
        r#"
weapon := class:
    var<protected> Ammo<public>:int = 10

gun := class(weapon):
    var Ammo<override><public>:int = 20

Gun := gun{}
set Gun.Ammo = 30
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("field `Ammo` is protected in class `weapon`")
    );
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
fn evaluates_native_predicts_class_field_specifier() {
    let source = r#"
log_level<native><public> := enum:
    Debug
    Normal

log<native><public> := class:
    DefaultLevel<native><public><predicts>:log_level = log_level.Normal

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_predicts_class_field_non_predicts_type() {
    let error = check_source(
        r#"
point := struct:
    X:int = 0

box := class:
    Field<predicts>:point = point{}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error.to_string().contains(
            "`predicts` field specifier requires a prediction-compatible type, got `point`"
        ),
        "{error}"
    );
}

#[test]
fn evaluates_predicts_extern_class_field_attribute() {
    let source = r#"
sync_state := class:
    @predicts_extern
    State<predicts>:int = 0

42
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_predicts_extern_without_predicts_field_specifier() {
    let error = check_source(
        r#"
sync_state := class:
    @predicts_extern
    State:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("@predicts_extern requires <predicts> on the same data member"),
        "{error}"
    );
}

#[test]
fn rejects_predicts_override_class_field() {
    let error = check_source(
        r#"
base := class:
    Field:int = 0

child := class(base):
    Field<override><predicts>:int = 1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("<override> cannot be used with <predicts> yet"),
        "{error}"
    );
}

#[test]
fn rejects_predicts_interface_field() {
    let error = check_source(
        r#"
readable := interface:
    Field<predicts>:int
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`predicts` field specifier can only be used on class fields"),
        "{error}"
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
fn rejects_class_method_override_widening_omitted_access_level() {
    let error = check_source(
        r#"
base := class:
    Hidden<protected>():int = 40

child := class(base):
    Hidden<override>():int = 42

child{}.Hidden()
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("method `Hidden` is protected in class `base`")
    );
}

#[test]
fn rejects_class_method_override_changing_access_level() {
    let error = check_source(
        r#"
base := class:
    Hidden<protected>():int = 40

child := class(base):
    Hidden<override><public>():int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("overridden method cannot change the inherited access level")
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(43));
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

    assert_eq!(eval(source), Value::Int(40));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
fn rejects_class_method_parameter_shadowing_field() {
    let error = check_source(
        r#"
counter := class:
    Value:int = 0

    Read(Value:int):int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `Value` cannot shadow class member `Value`")
    );
}

#[test]
fn rejects_class_method_local_shadowing_field() {
    let error = check_source(
        r#"
counter := class:
    Value:int = 0

    Read():int =
        Value:int = 42
        Value
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `Value` cannot shadow class member `Value`")
    );
}

#[test]
fn rejects_class_block_local_shadowing_field() {
    let error = check_source(
        r#"
counter := class:
    Value:int = 0

    block:
        Value:int = 42
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `Value` cannot shadow class member `Value`")
    );
}

#[test]
fn rejects_class_scope_extension_receiver_shadowing_field() {
    let error = check_source(
        r#"
counter := class:
    Value:int = 0

    (Value:int).Read():int = Value
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("definition `Value` cannot shadow class member `Value`")
    );
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_transacts_function_constructing_class_with_block() {
    let source = r#"
counter := class:
    var Value:int = 0

    block:
        set Value = 42

Make()<transacts>:counter = counter{}

Make().Value
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_computes_function_constructing_class_with_block() {
    let error = check_source(
        r#"
counter := class:
    var Value:int = 0

    block:
        set Value = 42

Make()<computes>:counter = counter{}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains(
        "function with <computes> effect cannot call function requiring <transacts> effect"
    ));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(52));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
fn evaluates_slice_start_only_array_method() {
    let source = r#"
Values:[]int = array{10, 20, 30, 40}
Slice := if (Value := Values.Slice[2]). Value else. array{}
if:
    First := Slice[0]
    Second := Slice[1]
then:
    Slice.Length + First + Second
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(72));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_failable_slice_start_only_array_method() {
    let source = r#"
Values:[]int = array{10, 20, 30}
SliceHit := if (Slice := Values.Slice[1]). Slice.Length else. 0
SliceMiss := if (Slice := Values.Slice[4]). Slice.Length else. 10
SliceHit + SliceMiss
"#;

    assert_eq!(eval(source), Value::Int(12));
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
fn rejects_bracket_method_on_non_array() {
    let error = check_source(r#"false.Find[1]"#).expect_err("source should fail");

    assert!(error.to_string().contains("no bracket method"));
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
