//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_open_enum_values() {
    let source = r#"
weapon := enum<open>{Sword, Bow}
Current:weapon = weapon.Sword
if (Current = weapon.Sword). true else. false
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
fn warns_unreachable_statement_after_exhaustive_never_case_expression() {
    assert_check_warning(
        r#"
state := enum{On, Off}

Bad(State:state):int = {
    case (State):
        state.On => Err("on")
        state.Off => Err("off")
    42
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after never-returning expression",
    );
}

#[test]
fn checks_reachable_statement_after_partially_never_case_expression() {
    let source = r#"
state := enum{On, Off}

Good(State:state):int = {
    Value:int = case (State):
        state.On => Err("on")
        state.Off => 40
    Value + 2
}
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::new(),
            Some(vec![Type::Enum("state".into())]),
            Type::Int
        )
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
IsPlaying<computes>(Value:state):logic = if (Value = state.Playing). true else. false
if (PausedID := StateID[state.Paused]). PausedID + if (IsPlaying(state.Playing)?) { 40 } else { 0 } else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
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
fn warns_duplicate_enum_case_arm() {
    assert_check_warning(
        r#"
day := enum:
    Monday

case (day.Monday):
    day.Monday => 1
    day.Monday => 2
"#,
        DiagnosticCode::UnreachableCode,
        "duplicate case",
    );
}

#[test]
fn warns_case_arm_after_wildcard() {
    assert_check_warning(
        r#"
day := enum:
    Monday
    Tuesday

case (day.Monday):
    _ => 0
    day.Monday => 1
"#,
        DiagnosticCode::UnreachableCode,
        "after wildcard",
    );
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
fn warns_duplicate_scalar_case_arm() {
    assert_check_warning(
        r#"
case (1):
    1 => 42
    1 => 0
    _ => -1
"#,
        DiagnosticCode::UnreachableCode,
        "duplicate case",
    );
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

    assert_eq!(eval(source), Value::Int(42));
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
