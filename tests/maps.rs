//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

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
fn evaluates_map_slot_set_expression_before_failure_binding() {
    let source = r#"
var Scores:[string]int = map{}
Result := if (set Scores["ada"] = 42, Score := Scores["ada"]):
    Score
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_subtype_comparable_constraint_for_equality_and_map_key() {
    let source = r#"
Same(Left:t, Right:t where t:subtype(comparable))<decides><transacts>:t =
    Left = Right
    Left

Store(Value:int, Key:t where t:subtype(comparable)):int =
    Values:[t]int = map{Key => Value}
    if (Found := Values[Key]). Found else. 0

if (Matched := Same["score", "score"]):
    Store(42, Matched)
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
fn rejects_unconstrained_type_parameter_as_map_key() {
    let source = r#"
Store(Value:int, Key:t where t:type):int =
    Values:[t]int = map{Key => Value}
    Values.Length
"#;

    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("map key type `t` is not comparable")
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

    assert_eq!(eval(source), Value::Int(44));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_array_and_map_length_members() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Scores:[string]int = map{"alice" => 10, "bob" => 20}
Values.Length + Scores.Length
"#;

    assert_eq!(eval(source), Value::Int(5));
}

#[test]
fn evaluates_map_mutation_and_insert() {
    let source = r#"
var Scores:[string]int = map{"alice" => 10}
if:
    set Scores["alice"] += 10
    set Scores["bob"] = 5
then:
    {}
else:
    {}
if:
    Alice := Scores["alice"]
    Bob := Scores["bob"]
then:
    Alice + Bob + Scores.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(27));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(2198));
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
fn rejects_missing_map_plus_equal_key_outside_failure_context() {
    let source = r#"
var Scores:[string]int = map{"alice" => 10}
set Scores["bob"] += 5
"#;
    let error = run_source(source).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
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

    assert_eq!(eval(source), Value::Int(5));
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

    assert_eq!(eval(source), Value::Int(8));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_float_map_key_annotation() {
    let source = r#"
Key:float = 1.5
Scores:[float]int = map{Key => 42}
if (Value := Scores[1.5]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_float_map_key_insert_and_lookup() {
    let source = r#"
var Scores:[float]int = map{1.25 => 10}
if:
    set Scores[2.5] = 32
    First := Scores[1.25]
    Second := Scores[2.5]
then:
    First + Second
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
fn evaluates_comparable_map_key_annotation_with_float() {
    let source = r#"
Scores:[comparable]int = map{1.5 => 42}
if (Value := Scores[1.5]). Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(40));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn evaluates_option_false_contextual_map_values() {
    let source = r#"
Scores:[string]?int = map{"empty" => false, "full" => option{40}}
Empty := if (Value := Scores["empty"]?). Value else. 1
Full := if (Value := Scores["full"]?). Value else. 0
Empty + Full + Scores.Length
"#;

    assert_eq!(eval(source), Value::Int(43));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
