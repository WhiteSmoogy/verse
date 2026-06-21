//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn evaluates_color_struct_make_color_and_named_colors() {
    let source = r#"
using { /Verse.org/Colors }
Made:color = MakeColorFromSRGB(1.0, 2.0, 3.0)
Manual:color = color{R := NamedColors.Red.R, G := NamedColors.Green.G, B := NamedColors.Blue.B}
Made.R + Made.G + Made.B + Manual.R + Manual.G + Manual.B
"#;

    assert_eq!(eval(source), Value::Float(8.0 + 128.0 / 255.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_named_colors_extended_css_keywords() {
    let source = r#"
using { /Verse.org/Colors }
Alice:color = MakeColorFromSRGBValues(240, 248, 255)
Hot:color = MakeColorFromSRGBValues(255, 105, 180)
Pale:color = MakeColorFromSRGBValues(219, 112, 147)
if (NamedColors.AliceBlue = Alice and NamedColors.Hotpink = Hot and NamedColors.PaleVioletred = Pale and NamedColors.DarkSlateGrey = NamedColors.DarkSlateGray). 42 else. 0
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_color_arithmetic_operators() {
    let source = r#"
using { /Verse.org/Colors }
Base:color = MakeColorFromSRGB(0.25, 0.5, 0.75)
Other:color = MakeColorFromSRGB(0.5, 0.25, 0.25)
Sum:color = Base + Other
Diff:color = Sum - Base
Product:color = Base * Other
ScaledLeft:color = Base * 2
ScaledRight:color = 2.0 * Other
Divided:color = if (Value := ScaledLeft / 2). Value else. Base
Sum.R + Sum.G + Sum.B + Diff.R + Diff.G + Diff.B + Product.R + Product.G + Product.B + ScaledLeft.R + ScaledRight.B + Divided.G
"#;

    assert_eq!(eval(source), Value::Float(5.4375));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_color_from_srgb_values_and_hex() {
    let source = r##"
using { /Verse.org/Colors }
FromValues:color = MakeColorFromSRGBValues(255, 128, 0)
FromShortHex:color = MakeColorFromHex("#0f8")
FromLongHex:color = MakeColorFromHex("0000ffcc")
Invalid:color = MakeColorFromHex("bad value")
FromValues.R + FromValues.B + FromShortHex.G + FromLongHex.B + Invalid.R + Invalid.G + Invalid.B
"##;

    assert_eq!(eval(source), Value::Float(3.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_srgb_and_hsv_color_tuple_helpers() {
    let source = r#"
using { /Verse.org/Colors }
RGB:tuple(float, float, float) = MakeSRGBFromColor(MakeColorFromSRGB(1.0, 0.5, 0.0))
FromHSV:color = MakeColorFromHSV(480.0, 1.0, 1.0)
HSV:tuple(float, float, float) = MakeHSVFromColor(FromHSV)
RGB(0) + RGB(1) + RGB(2) + FromHSV.G + HSV(0) + HSV(1) + HSV(2)
"#;

    assert_eq!(eval(source), Value::Float(124.5));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn evaluates_official_color_helpers_with_ordinary_named_arguments() {
    let source = r#"
using { /Verse.org/Colors }
Color:color = MakeColorFromSRGB(Blue := 0.25, Red := 1.0, Green := 0.5)
RGB:tuple(float, float, float) = MakeSRGBFromColor(Color := Color)
if (RGB(0) = 1.0 and RGB(1) = 0.5 and RGB(2) = 0.25):
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
fn evaluates_official_color_alpha_struct_and_over() {
    let source = r#"
using { /Verse.org/Colors }
Front:color_alpha = MakeColorAlpha(1.0, 0.0, 0.0, 0.5)
Back:color_alpha = color_alpha{Color := MakeColorFromSRGB(0.0, 0.0, 1.0), A := 0.5}
Blended:color_alpha = Over(Front, Back)
Blended.Color.R * 3.0 + Blended.Color.B * 3.0 + Blended.A * 4.0
"#;

    assert_eq!(eval(source), Value::Float(6.0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Float
    );
}

#[test]
fn rejects_make_color_from_srgb_values_component_out_of_range_at_runtime() {
    let error = run_source("MakeColorFromSRGBValues(256, 0, 0)").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`MakeColorFromSRGBValues` red expected a value in 0..255")
    );
}

#[test]
fn rejects_over_with_zero_alpha_components_at_runtime() {
    let error = run_source(
        r#"
using { /Verse.org/Colors }
Over(MakeColorAlpha(1.0, 0.0, 0.0, 0.0), MakeColorAlpha(0.0, 0.0, 1.0, 0.0))
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("`Over` failed: both alpha components are zero")
    );
}

#[test]
fn rejects_color_operator_type_mismatch() {
    let error =
        check_source("MakeColorFromSRGB(1.0, 0.0, 0.0) + 1").expect_err("source should fail");

    assert!(error.to_string().contains("colors"));
}

#[test]
fn rejects_over_non_color_alpha_argument() {
    let error = check_source(
        r#"
using { /Verse.org/Colors }
Over(MakeColorAlpha(1.0, 0.0, 0.0, 0.5), MakeColorFromSRGB(0.0, 0.0, 1.0))
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("argument 2 expected `color_alpha`")
    );
}

#[test]
fn rejects_named_color_outside_official_css3_list() {
    let error = check_source(
        r#"
using { /Verse.org/Colors }
NamedColors.RebeccaPurple
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("module `NamedColors` has no member `RebeccaPurple`")
    );
}

#[test]
fn evaluates_official_print_named_duration_and_color_arguments() {
    let source = r#"
using { /Verse.org/Colors }
Print("Ready", ?Duration := 1.5, ?Color := MakeColorFromSRGB(1.0, 0.0, 0.0))
Print(ToDiagnostic("diag"), ?Color := NamedColors.Blue)
"#;

    assert_eq!(eval(source), Value::None);
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::None
    );
}

#[test]
fn rejects_print_bad_color_argument() {
    let error = check_source(r#"Print("Ready", ?Color := 1)"#).expect_err("source should fail");

    assert!(error.to_string().contains("no overload matches"));
}

#[test]
fn rejects_for_pair_iteration_over_range() {
    let error = check_source(
        r#"
for (Index -> Value : 1..3) {
    Value
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot use `->`"));
}

#[test]
fn rejects_for_pair_iteration_over_string() {
    let error = check_source(
        r#"
for (Index -> Letter : "abc") {
    Letter
}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("cannot use `->`"));
}
