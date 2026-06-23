//! Executable inventory for finishing the remaining Structs/enums FT.
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
#[ignore = "planned column: struct and enum value-copy runtime isolation"]
fn evaluates_struct_enum_value_copy_semantics() {
    assert_runtime_cases(&[
        (
            "struct binding snapshots stay independent",
            r#"
point := struct<computes>:
    X:int = 0
    Y:int = 0

var Current:point = point{X := 1, Y := 1}
Snapshot := Current
set Current.X = 40
Current.X + Current.Y + Snapshot.X
"#,
            Value::Int(42),
        ),
        (
            "nested struct snapshots stay independent",
            r#"
point := struct<computes>:
    X:int = 0

wrapper := struct<computes>:
    Inner:point

var Current:wrapper = wrapper{Inner := point{X := 1}}
Snapshot := Current
set Current.Inner.X = 40
Current.Inner.X + Snapshot.Inner.X + 1
"#,
            Value::Int(42),
        ),
        (
            "array lookup returns a struct copy",
            r#"
point := struct<computes>:
    X:int = 0

Values:[]point = array{point{X := 1}}
var Local:point = if (Value := Values[0]). Value else. point{}
set Local.X = 40
Original := if (Value := Values[0]). Value.X else. 0
Local.X + Original + 1
"#,
            Value::Int(42),
        ),
        (
            "map lookup returns a struct copy",
            r#"
point := struct<computes>:
    X:int = 0

Values:[int]point = map{0 => point{X := 1}}
var Local:point = if (Value := Values[0]). Value else. point{}
set Local.X = 40
Original := if (Value := Values[0]). Value.X else. 0
Local.X + Original + 1
"#,
            Value::Int(42),
        ),
        (
            "option query returns a struct copy",
            r#"
point := struct<computes>:
    X:int = 0

Maybe:?point = option{point{X := 1}}
var Local:point = if (Value := Maybe?). Value else. point{}
set Local.X = 40
Original := if (Value := Maybe?). Value.X else. 0
Local.X + Original + 1
"#,
            Value::Int(42),
        ),
        (
            "class field read returns a struct copy",
            r#"
point := struct<computes>:
    X:int = 0

box := class:
    Inner:point

Holder := box{Inner := point{X := 1}}
var Local:point = Holder.Inner
set Local.X = 40
Local.X + Holder.Inner.X + 1
"#,
            Value::Int(42),
        ),
        (
            "enum fields copy with enclosing structs",
            r#"
state := enum{Ready, Done}

record := struct<computes>:
    State:state = state.Ready

var Current:record = record{}
Snapshot := Current
set Current.State = state.Done
if:
    Current.State = state.Done
    Snapshot.State = state.Ready
then:
    42
else:
    0
"#,
            Value::Int(42),
        ),
        (
            "struct-contained arrays are copied with struct snapshots",
            r#"
bag := struct<computes>:
    Items:[]int = array{}

var Current:bag = bag{Items := array{1}}
Snapshot := Current
if:
    set Current.Items[0] = 40
    New := Current.Items[0]
    Old := Snapshot.Items[0]
then:
    New + Old + 1
else:
    0
"#,
            Value::Int(42),
        ),
    ]);
}
