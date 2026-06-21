//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_hexadecimal_integer_literals() {
    let source = "0x7F + 0xFACE";

    assert_eq!(eval(source), Value::Int(64333));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_signed_64_bit_integer_literal_bounds() {
    assert_eq!(eval("9223372036854775807"), Value::Int(i64::MAX));
    assert_eq!(eval("-9223372036854775808"), Value::Int(i64::MIN));
    assert_eq!(eval("-0x8000000000000000"), Value::Int(i64::MIN));
}

#[test]
fn rejects_integer_literal_above_signed_64_bit_range() {
    let error = parse_source("9223372036854775808").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_integer_literal_below_signed_64_bit_range() {
    let error = parse_source("-9223372036854775809").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_hex_integer_literal_above_signed_64_bit_range() {
    let error = parse_source("0x8000000000000001").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("outside the 64-bit signed range")
    );
}

#[test]
fn rejects_empty_hexadecimal_integer_literal() {
    let error = parse_source("0x").expect_err("source should fail");

    assert!(error.to_string().contains("expected hexadecimal digits"));
}

#[test]
fn evaluates_inline_block_comments() {
    let source = "1<# inline comment #> + 2";

    assert_eq!(eval(source), Value::Int(3));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_nested_multiline_block_comments() {
    let source = r#"
Value := 40
<# outer
    <# nested #>
#>
Value + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_indented_comments() {
    let source = r#"
<#>
    This line is a Verse indented comment.
    This one is also ignored.
40 + 2
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_unterminated_block_comments() {
    let error = parse_source("<# missing").expect_err("source should fail");

    assert!(error.to_string().contains("unterminated block comment"));
}

#[test]
fn rejects_legacy_slash_line_comments() {
    let error = parse_source("1 // not Verse").expect_err("source should fail");

    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn evaluates_block_comments_inside_string_literals() {
    let source = r#""abc<#comment#>def""#;

    assert_eq!(eval(source), Value::String("abcdef".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_nested_block_comments_inside_string_literals() {
    let source = r#""a<# outer <# inner #> still outer #>b""#;

    assert_eq!(eval(source), Value::String("ab".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_inside_interpolated_string_text() {
    let source = r#"
Value:int = 42
"a<#left#>{Value}<#right#>b"
"#;

    assert_eq!(eval(source), Value::String("a42b".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_with_braces_inside_string_interpolation() {
    let source = r#""{40 <# } ignored by comment #> + 2}""#;

    assert_eq!(eval(source), Value::String("42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_block_comments_with_quotes_inside_nested_interpolation_strings() {
    let source = r#""{"a<# " ignored by comment #>b"}""#;

    assert_eq!(eval(source), Value::String("ab".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_unterminated_block_comments_inside_string_literals() {
    let error = parse_source(r#""abc<# missing""#).expect_err("source should fail");

    assert!(error.to_string().contains("unterminated block comment"));
}

#[test]
fn rejects_reserved_underscore_binding_name() {
    let error = parse_source("_ := 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn rejects_reserved_underscore_variable_name() {
    let error = parse_source("var _:int = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn rejects_reserved_underscore_parameter_name() {
    let error = parse_source("_Bad(_:int):int = 1").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("reserved identifier `_` cannot be used as a name")
    );
}

#[test]
fn rejects_official_profile_expression_non_string_description() {
    let error = check_source(
        r#"
profile(42):
    1
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("profile description expected `string`, got `int`")
    );
}

#[test]
fn evaluates_string_interpolation_official_example() {
    let source = r#""2 + 2 = {2 + 2}""#;

    assert_eq!(eval(source), Value::String("2 + 2 = 4".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_interpolation_with_bindings() {
    let source = r#"
Score:int = 40
"Score = {Score + 2}"
"#;

    assert_eq!(eval(source), Value::String("Score = 42".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_interpolation_with_nested_braced_expression() {
    let source = r#""Length = {array{1, 2, 3}.Length}""#;

    assert_eq!(eval(source), Value::String("Length = 3".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_escaped_string_interpolation_braces() {
    let source = r#""\{Value\}""#;

    assert_eq!(eval(source), Value::String("{Value}".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_empty_string_interpolants() {
    let source = r#""ab{}cd""#;

    assert_eq!(eval(source), Value::String("abcd".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_multiline_string_continuation_interpolants() {
    let source = r#""This is a multi-line {
}string that continues across {
}multiple lines.""#;

    assert_eq!(
        eval(source),
        Value::String("This is a multi-line string that continues across multiple lines.".into())
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_comment_only_string_interpolants() {
    let source = r#""This is another {
    # This comment is ignored
}message""#;

    assert_eq!(
        eval(source),
        Value::String("This is another message".into())
    );
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_line_comments_with_braces_inside_string_interpolation() {
    let source = r#""{40 # } ignored by comment
}""#;

    assert_eq!(eval(source), Value::String("40".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_unterminated_string_interpolation() {
    let error = parse_source(r#""{Value""#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("unterminated string interpolation")
    );
}

#[test]
fn rejects_unescaped_closing_brace_in_string() {
    let error = parse_source(r#""}""#).expect_err("source should fail");

    assert!(error.to_string().contains("unescaped `}`"));
}

#[test]
fn rejects_invalid_string_interpolation_expression() {
    let error = parse_source(r#""Value = {1 + }""#).expect_err("source should fail");

    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn rejects_unknown_name_in_string_interpolation() {
    let error = check_source(r#""Value = {Missing}""#).expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Missing`"));
}

#[test]
fn evaluates_string_value_assigned_to_message() {
    let source = r#"
Text:string = "Ready"
Label:message = Text
Label
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Message
    );
}

#[test]
fn evaluates_ascii_char_literals_and_annotations() {
    let source = r#"
Letter:char = 'a'
Letter
"#;

    assert_eq!(eval(source), Value::Char('a'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char
    );
}

#[test]
fn evaluates_non_ascii_char32_literals_and_annotations() {
    let source = r#"
Letter:char32 = '好'
Letter
"#;

    assert_eq!(eval(source), Value::Char32('好'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char32
    );
}

#[test]
fn evaluates_hex_character_literals() {
    let source = r#"
Byte:char = 0o65
Wide:char32 = 0u00E9
Emoji:char32 = 0u1f600
ToString(Byte) + ToString(Wide) + ToString(Emoji)
"#;

    assert_eq!(eval(source), Value::String("eé😀".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_char_literals_as_map_keys() {
    let source = r#"
Scores:[char]int = map{'a' => 41}
if (Score := Scores['a']). Score + 1 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_string_escape_table_entries() {
    let source = r#""\#\<\>\&\~\'""#;

    assert_eq!(eval(source), Value::String("#<>&~'".into()));
}

#[test]
fn evaluates_char_array_annotation_from_string() {
    let source = r#"
Text:[]char = "Ready"
Text
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Array(Box::new(Type::Char))
    );
}

#[test]
fn evaluates_string_annotation_from_char_array() {
    let source = r#"
Text:[]char = "Ready"
Label:string = Text
Label
"#;

    assert_eq!(eval(source), Value::String("Ready".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_string_literal_assigned_to_char32_array() {
    let error = check_source(r#"Utf32:[]char32 = "A""#).expect_err("source should fail");

    assert!(error.to_string().contains(
        "binding `Utf32` is annotated as `array<char32>` but expression has type `string`"
    ));
}

#[test]
fn evaluates_string_indexing_as_utf8_char_units() {
    let source = r#"
Text:string = "Verse"
if (Letter := Text[0]). Letter else. 'x'
"#;

    assert_eq!(eval(source), Value::Char('V'));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Char
    );
}

#[test]
fn evaluates_unicode_string_length_as_utf8_units() {
    let source = r#"
Text:string = "José"
Text.Length
"#;

    assert_eq!(eval(source), Value::Int(5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_string_index_failure_in_option_literal() {
    let source = r#"
Text:string = "ab"
option{Text[2]}
"#;

    assert_eq!(eval(source), Value::Option(None));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Option(Box::new(Type::Char))
    );
}

#[test]
fn rejects_string_index_with_non_int() {
    let error = check_source(r#"if (Letter := "abc"["0"]). Letter else. 'x'"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("string index expected `int`"));
}

#[test]
fn rejects_string_index_with_float() {
    let error = check_source(r#"if (Letter := "abc"[1.0]). Letter else. 'x'"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("string index expected `int`"));
}

#[test]
fn evaluates_string_slot_assignment() {
    let source = r#"
var Text:string = "Glorblex"
if:
    set Text[0] = 'F'
then:
    {}
else:
    {}
Text
"#;

    assert_eq!(eval(source), Value::String("Florblex".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_string_char_array_equality() {
    let source = r#"
if ("abc" = array{'a', 'b', 'c'}):
    1
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(1));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_string_annotation_from_char_array_literal() {
    let source = r#"
Text:string = array{'a', 'b', 'c'}
Text
"#;

    assert_eq!(eval(source), Value::String("abc".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn rejects_non_string_value_assigned_to_char_array() {
    let error = check_source("Text:[]char = 42").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Text` is annotated as `array<char>` but expression has type `int`")
    );
}

#[test]
fn rejects_string_literal_assigned_to_bare_char() {
    let error = check_source(r#"Letter:char = "A""#).expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char` but expression has type `string`")
    );
}

#[test]
fn rejects_char32_literal_assigned_to_char() {
    let error = check_source("Letter:char = '好'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char` but expression has type `char32`")
    );
}

#[test]
fn rejects_char_literal_assigned_to_char32() {
    let error = check_source("Letter:char32 = 'a'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Letter` is annotated as `char32` but expression has type `char`")
    );
}

#[test]
fn rejects_empty_character_literal() {
    let error = parse_source("Letter := ''").expect_err("source should fail");

    assert!(error.to_string().contains("empty character literal"));
}

#[test]
fn rejects_multi_character_literal() {
    let error = parse_source("Letter := 'ab'").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("character literal cannot contain multiple characters")
    );
}

#[test]
fn rejects_short_hex_char_literal() {
    let error = parse_source("Letter := 0o6").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected two hexadecimal digits after `0o`")
    );
}

#[test]
fn rejects_long_hex_char_literal() {
    let error = parse_source("Letter := 0o616").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`0o` character literal expects exactly two hexadecimal digits")
    );
}

#[test]
fn rejects_long_hex_char32_literal() {
    let error = parse_source("Letter := 0u1234567").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`0u` character literal expects at most six hexadecimal digits")
    );
}

#[test]
fn rejects_invalid_hex_char32_codepoint() {
    let error = parse_source("Letter := 0u110000").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("invalid Unicode code point `0u110000`")
    );
}

#[test]
fn evaluates_scientific_float_literals() {
    let source = "1.0e2 + 2.5e+1 + 5.0e-1";

    assert_eq!(eval(source), Value::Float(125.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_f64_suffixed_float_literals() {
    let source = "12.25f64 + 7.75f64";

    assert_eq!(eval(source), Value::Float(20.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn rejects_float_literal_exponent_without_digits() {
    let error = parse_source("1.0e+").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("expected exponent digits in float literal")
    );
}

#[test]
fn rejects_overflowing_float_literals() {
    let error = parse_source("1.7976931348623159e+308").expect_err("source should fail");

    assert!(error.to_string().contains("outside the finite f64 range"));
}

#[test]
fn rejects_float_literal_assigned_to_int() {
    let error = check_source("Whole:int = 1.5").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("binding `Whole` is annotated as `int` but expression has type `float`")
    );
}

#[test]
fn evaluates_int_literal_as_float_parameter() {
    let source = r#"
Scale(Value:float):float = Value + 0.5
Scale(41)
"#;

    assert_eq!(eval(source), Value::Float(41.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn if_then_condition_bindings_do_not_escape() {
    let error = check_source(
        r#"
if:
    Value := option{42}?
then:
    Value
else:
    0
Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
}

#[test]
fn rejects_non_verse_fun_function_literals() {
    let error = parse_source("Double := fun(X:int):int { X * 2 }").expect_err("source should fail");

    assert!(error.to_string().contains("expected `)`"));
}

#[test]
fn rejects_false_type_annotation_escape_in_concrete_class() {
    let error = check_source(
        r#"
c := class<concrete>:
    X:false = False:false
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected type"));
}

#[test]
fn captures_failed_class_type_cast_in_option_literal() {
    let source = r#"
entity := class:
    ID : int

boss := class(entity):
    Threat : int

Maybe := option{boss[entity{ID := 1}]}
if (Maybe?):
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
fn rejects_bracket_array_literals() {
    let error = parse_source("[1, 2, 3]").expect_err("source should fail");
    assert!(error.to_string().contains("expected expression"));
}

#[test]
fn evaluates_string_array_methods() {
    let source = r#"
Text:string = "balloon"
Slice := if (Value := Text.Slice[1, 5]). Value else. ""
Inserted := if (Value := Text.Insert[1, "!!"]). Value else. ""
Replaced := Text.ReplaceAll["lo", "p"]

if:
    Slice = "allo"
    Inserted = "b!!alloon"
    Replaced = "balpon"
    Index := Text.Find['l']
then:
    Index + Slice.Length + Inserted.Length + Replaced.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(21));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_string_slice_start_only_array_method() {
    let source = r#"
Text:string = "balloon"
Slice := if (Value := Text.Slice[2]). Value else. ""
Slice
"#;

    assert_eq!(eval(source), Value::String("lloon".into()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_failable_string_array_methods_in_failure_context() {
    let source = r#"
Text:string = "abc"
FindHit := if (Index := Text.Find['a']). Index else. -1
FindMiss := if (Index := Text.Find['z']). Index else. 10
SliceMiss := if (Part := Text.Slice[2, 1]). Part.Length else. 20
RemoveMiss := if (Part := Text.RemoveElement[9]). Part.Length else. 30
FindHit + FindMiss + SliceMiss + RemoveMiss
"#;

    assert_eq!(eval(source), Value::Int(60));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_failable_array_method_failure_in_option_literal() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Found:?int = option{Values.Find[20]}
Missing:?int = option{Values.Find[99]}
Removed:?[]int = option{Values.RemoveElement[1]}
RemoveMissing:?[]int = option{Values.RemoveElement[9]}

First := if (Index := Found?). Index else. 0
Second := if (Index := Missing?). Index else. 40
Third := if:
    Result := Removed?
    Value := Result[1]
then:
    Value
else:
    0
Fourth := if (Result := RemoveMissing?). Result.Length else. 0
First + Second + Third + Fourth
"#;

    assert_eq!(eval(source), Value::Int(71));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_string_array_method_type_mismatch() {
    let error = check_source(r#""x".Find[1]"#).expect_err("source should fail");

    assert!(error.to_string().contains("`Find` expected `char`"));
}

#[test]
fn evaluates_for_unicode_strings_as_utf8_units() {
    let source = r#"
Units := for (Unit : "José") {
    Unit
}
if (Units = array{'J', 'o', 's', 0oC3, 0oA9}):
    Units.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_verse_array_literal_and_array_type() {
    let source = r#"
Values:[]int = array{3, 4, 5}
if:
    First := Values[0]
    Second := Values[1]
    Third := Values[2]
then:
    First + Second + Third
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(12));
}

#[test]
fn evaluates_verse_map_literals_and_lookup() {
    let source = r#"
Scores:[string]int = map{
    "alice" => 10,
    "bob" => 20,
    "alice" => 15,
}
if:
    Alice := Scores["alice"]
    Bob := Scores["bob"]
then:
    Alice + Bob + Scores.Length
else:
    0
"#;

    assert_eq!(eval(source), Value::Int(37));
}

#[test]
fn evaluates_tuple_literals_and_indexing() {
    let source = r#"
Pair := (40, "ignored", 2)
Pair(0) + Pair(2)
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn rejects_negative_tuple_index_literal() {
    let error = check_source(
        r#"
Pair := (1, 2)
Pair(-1)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("tuple index must be a non-negative integer")
    );
}

#[test]
fn evaluates_option_literals_and_unwrap() {
    let source = r#"
Maybe:?int = option{40}
if (Value := Maybe?). Value + 2 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn captures_array_lookup_failure_in_option_literal() {
    let source = r#"
Values:[]int = array{42}
Found:?int = option{Values[0]}
Missing:?int = option{Values[5]}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_map_lookup_failure_in_option_literal() {
    let source = r#"
Scores:[string]int = map{"ada" => 42}
Found:?int = option{Scores["ada"]}
Missing:?int = option{Scores["grace"]}
First := if (Value := Found?). Value else. 0
Second := if (Value := Missing?). Value else. 0
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn captures_query_failure_in_option_literal() {
    let source = r#"
Ready:?logic = option{true?}
Blocked:?logic = option{false?}
First := if (Value := Ready?). if (Value?). 40 else. 0 else. 0
Second := if (Value := Blocked?). if (Value?). 0 else. 0 else. 2
First + Second
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn option_braced_block_bindings_do_not_escape() {
    let error = check_source(
        r#"
Maybe:?int = option{
    Value := 42
    Value
}
Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
}

#[test]
fn checks_empty_option_literal() {
    let source = r#"
Maybe:?int = option{}
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Option(Box::new(Type::Int))
    );
}
