//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

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
fn evaluates_if_failure_binding_array_lookup_success() {
    let source = r#"
Values:[]int = array{40, 2}
if (Value := Values[1]):
    Value + 40
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
fn evaluates_if_failure_binding_array_lookup_failure() {
    let source = r#"
Values:[]int = array{40, 2}
if (Value := Values[10]):
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

    assert_eq!(eval(source), Value::Int(81));
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
fn rejects_official_make_classifiable_subset_non_array_argument() {
    let error = check_source("MakeClassifiableSubset(42)").expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected `array`"));
}

#[test]
fn evaluates_arrays_indexing_and_length_member() {
    let source = r#"
xs := array{10, 20, 30}
xs.Length + xs[1]
"#;

    assert_eq!(eval(source), Value::Int(23));
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
fn runtime_errors_on_float_array_index() {
    let error = Interpreter::new()
        .eval_source("array{10, 20}[1.0]")
        .expect_err("source should fail");

    assert!(error.to_string().contains("array index expected int"));
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

    assert_eq!(eval(source), Value::Int(44));
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

    assert_eq!(eval(source), Value::Int(2199));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_array_concatenation_and_tuple_append() {
    let source = r#"
var Values:[]int = array{1, 2}
set Values = Values + array{3}
set Values += (4, 5)
Values.Length + Values[4]
"#;

    assert_eq!(eval(source), Value::Int(10));
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

    assert_eq!(eval(source), Value::Int(229));
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

    assert_eq!(eval(source), Value::Int(13));
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

    assert_eq!(eval(source), Value::Int(245));
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

    assert_eq!(eval(source), Value::Int(8));
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

    assert_eq!(eval(source), Value::Int(8));
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

    assert_eq!(eval(source), Value::Int(2290));
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

    assert_eq!(eval(source), Value::Int(184));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
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
fn rejects_slice_with_too_many_arguments() {
    let error = check_source("Values:[]int = array{1, 2}\nValues.Slice[0, 1, 2]")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Slice` expected 1..=2 arguments")
    );
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
Values:[]int = Concatenate(array{array{1, 2}, array{3}, array{4, 5}})
Nested:[]int = Concatenate(array{array{6, 7}, array{8}})
Named:[]int = Concatenate(Arrays := array{array{9}, array{10}})
Values.Length + Values[4] + Nested.Length + Nested[2] + Named[1]
"#;

    assert_eq!(eval(source), Value::Int(31));
}

#[test]
fn rejects_concatenate_non_array_arguments() {
    let error = check_source("Concatenate(1)").expect_err("source should fail");

    assert!(error.to_string().contains("array argument item 1"));
}

#[test]
fn evaluates_concatenate_single_array_parameter_packing() {
    let source = r#"
Values:[]int = Concatenate(array{1}, array{2}, (3, 4))
Single:[]int = Concatenate(array{5})
Tupled:[]int = Concatenate((array{6}, array{7}))
if:
    Last := Values[3]
    Only := Single[0]
    TupledLast := Tupled[1]
then:
    Values.Length + Last + Only + TupledLast
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(20));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_concatenate_infers_item_type() {
    let source = r#"
Values := Concatenate(array{array{1}, array{2}})
Value:int = if (Item := Values[0]). Item else. 0
Value
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );

    let error = check_source(
        r#"
Values := Concatenate(array{1}, array{2})
Text:string = if (Item := Values[0]). Item else. ""
"#,
    )
    .expect_err("source should fail");

    let message = error.to_string();
    assert!(
        message.contains("incompatible types `int` and `string`"),
        "{message}"
    );
}

#[test]
fn rejects_concatenate_inconsistent_item_types() {
    let error =
        check_source(r#"Concatenate(array{1}, array{"bad"})"#).expect_err("source should fail");

    assert!(error.to_string().contains("incompatible types"));
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
fn rejects_range_expression_as_array_item() {
    let error = check_source("Values := array{1..3}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("range expressions are only valid in `for` expressions")
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

    assert_eq!(eval(source), Value::Int(63));
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
fn evaluates_official_length_extension_calls() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Scores:[string]int = map{"alice" => 10, "bob" => 20}
Text := "abc"
Values.Length() * 100 + Scores.Length() * 10 + Text.Length()
"#;

    assert_eq!(eval(source), Value::Int(323));
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

    assert_eq!(eval(source), Value::Int(192));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
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

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
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
