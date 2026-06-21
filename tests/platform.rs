//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_official_session_environment_enum_values() {
    let source = r#"
Env:session_environment = session_environment.Edit
if (Env = session_environment.Edit and Env <> session_environment.Live). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(0));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(140));
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

    assert_eq!(eval(source), Value::Int(140));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_waits_for_sleep_zero_winner_and_continues_loser_while_scope_is_active()
 {
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
    Sleep(0.0)
    set Trace = Trace * 10 + 3
spawn{Run()}
BeforeParentResume:int = Trace
AfterParentResume:int = Trace
AfterParentResume
"#;

    assert_eq!(eval(source), Value::Int(5023));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_sleep_zero_body_cancels_when_scope_completes() {
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

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_sleep_zero_body_resumes_while_scope_is_active() {
    let source = r#"
var Result:int = 0
Work()<suspends><transacts>:void =
    Sleep(0.0)
    set Result = Result * 10 + 2
Run()<suspends><transacts>:void =
    branch:
        Work()
    Sleep(0.0)
    set Result = Result * 10 + 1
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(21));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_positive_sleep_body_cancels_when_scope_completes() {
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

    assert_eq!(eval(source), Value::Int(1));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(12));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_awaiting_sleep_inf_pending_task_outside_async_context() {
    let source = r#"
Never()<suspends>:void =
    Sleep(Inf)
Task:task(void) = spawn{Never()}
Task.Await()
"#;

    let error = run_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<suspends>` effect can only be called in an async context")
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

    assert_eq!(eval(source), Value::Int(42));
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
fn rejects_persistable_parametric_class() {
    let error = check_source(
        r#"
player_profile_data(t:type) := class<final><persistable>:
    Value:t
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("persistable class `player_profile_data` cannot be parametric")
    );
}

#[test]
fn runtime_errors_on_persistable_parametric_class() {
    let error = run_source(
        r#"
player_profile_data(t:type) := class<final><persistable>:
    Value:t
"#,
    )
    .expect_err("source should runtime error");

    assert!(
        error
            .to_string()
            .contains("persistable class `player_profile_data` cannot be parametric")
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_persistable_parametric_struct() {
    let error = check_source(
        r#"
profile_snapshot(t:type) := struct<persistable>:
    Value:t
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("persistable struct `profile_snapshot` cannot be parametric")
    );
}

#[test]
fn runtime_errors_on_persistable_parametric_struct() {
    let error = run_source(
        r#"
profile_snapshot(t:type) := struct<persistable>:
    Value:t
"#,
    )
    .expect_err("source should runtime error");

    assert!(
        error
            .to_string()
            .contains("persistable struct `profile_snapshot` cannot be parametric")
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

    assert_eq!(eval(source), Value::Int(0));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_persistable_color_in_player_weak_map() {
    let source = r#"
player := class<unique>:
    ID:int = 0

Alice := player{ID := 1}
Input:color = MakeColorFromSRGB(0.25, 0.5, 0.75)
var Saved:weak_map(player, color) = map{}
if:
    set Saved[Alice] = Input
    Color := Saved[Alice]
then:
    Color.R + Color.G + Color.B
else:
    0.0
"#;

    match eval(source) {
        Value::Float(actual) => assert_eq!(actual, 1.5),
        other => panic!("expected float 1.5, got {other:?}"),
    }
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(6));
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
fn evaluates_fits_in_player_map_color_success() {
    let source = r#"
Input:color = MakeColorFromSRGB(0.25, 0.5, 0.75)
Checked:color =
    if (Result := FitsInPlayerMap[Input]):
        Result
    else:
        color{R := 0.0, G := 0.0, B := 0.0}
Checked.R + Checked.G + Checked.B
"#;

    match eval(source) {
        Value::Float(actual) => assert_eq!(actual, 1.5),
        other => panic!("expected float 1.5, got {other:?}"),
    }
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn captures_fits_in_player_map_size_failure_in_failure_context() {
    let source = r#"
Large := for (I := 1..33000). I
if (Checked := FitsInPlayerMap[Large]). Checked.Length else. 42
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(7));
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

    assert!(matches!(value, Value::Float(seconds) if seconds >= 0.0));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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
