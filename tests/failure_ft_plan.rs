//! Executable inventory for finishing the remaining Failure FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn assert_runtime_cases(cases: &[(&str, &str, Value)]) {
    for (name, source, expected) in cases {
        assert_eq!(run_source(source).expect(name), expected.clone(), "{name}");
        assert_eq!(check_source(source).expect(name), Type::Int, "{name}");
    }
}

#[test]
fn evaluates_failure_context_nested_expression_surfaces() {
    assert_runtime_cases(&[
        (
            "array and map literals contain failable subexpressions",
            r#"
Values:[]int = array{40, 2}
Result := if:
    Items := array{Values[0], Values[1]}
    Scores := map{Values[1] => Items[0]}
    Score := Scores[2]
then:
    Score + Items.Length
else:
    0
Result
"#,
            Value::Int(42),
        ),
        (
            "function arguments fail as part of the enclosing failure context",
            r#"
Pick(A:int, B:int)<computes>:int = A + B
Values:[]int = array{40}
First := if (Value := Pick(Values[0], 2), Value = 42). Value else. 0
Second := if (Value := Pick(Values[1], 1)). Value else. 0
First + Second
"#,
            Value::Int(42),
        ),
        (
            "case subjects and arms can participate in failure contexts",
            r#"
Values:[]int = array{2, 40}
Matched := if:
    Result := case (Values[0]):
        2 => Values[1] + 2
then:
    Result
else:
    0
Missing := if:
    Result := case (Values[2]):
        2 => 99
then:
    Result
else:
    0
Matched + Missing
"#,
            Value::Int(42),
        ),
        (
            "member receivers can be produced by failable expressions",
            r#"
box := class:
    Value:int
    Read():int = Value

Boxes:[]box = array{box{Value := 40}}
Result := if:
    Item := Boxes[0]
    Item.Read() = 40
then:
    Item.Read() + 2
else:
    0
Result
"#,
            Value::Int(42),
        ),
    ]);
}

#[test]
#[ignore = "planned column: nested mutable values roll back through structs"]
fn rolls_back_struct_contained_mutable_values_in_failure_contexts() {
    assert_runtime_cases(&[
        (
            "struct-held array and map slot mutations roll back",
            r#"
bag := struct<computes>:
    Items:[]int = array{}
    Scores:[int]int = map{}

var Bag:bag = bag{Items := array{1}, Scores := map{0 => 1}}
Result := if:
    set Bag.Items[0] = 40
    set Bag.Scores[0] = 40
    false?
then:
    0
else:
    if:
        Item := Bag.Items[0]
        Score := Bag.Scores[0]
    then:
        Item + Score + 40
    else:
        0
Result
"#,
            Value::Int(42),
        ),
        (
            "class fields containing structs restore nested mutable state",
            r#"
bag := struct<computes>:
    Items:[]int = array{}

box := class:
    var State:bag

Holder := box{State := bag{Items := array{1}}}
Result := if:
    set Holder.State.Items[0] = 40
    false?
then:
    0
else:
    if (Item := Holder.State.Items[0]). Item + 41 else. 0
Result
"#,
            Value::Int(42),
        ),
        (
            "option values containing structs restore nested mutable state",
            r#"
bag := struct<computes>:
    Items:[]int = array{}

var Maybe:?bag = option{bag{Items := array{1}}}
Result := if:
    Current := Maybe?
    set Current.Items[0] = 40
    set Maybe = option{Current}
    false?
then:
    0
else:
    if:
        Current := Maybe?
        Item := Current.Items[0]
    then:
        Item + 41
    else:
        0
Result
"#,
            Value::Int(42),
        ),
    ]);
}
