use super::builtins::{color_alpha_value, color_value};
use super::numeric::{runtime_number, runtime_number_to_f64};
use super::scalar_ops::{RuntimeNumberOp, expect_integer, expect_number};
use super::validation::char_array_to_string;
use super::{NativeResult, Value};
use crate::error::VerseError;
use crate::token::Span;

fn color_components(value: &Value) -> Option<(f64, f64, f64)> {
    let Value::StructInstance {
        struct_name,
        fields,
        ..
    } = value
    else {
        return None;
    };
    if struct_name != "color" {
        return None;
    }

    let mut red = None;
    let mut green = None;
    let mut blue = None;
    for (name, value) in fields {
        match name.as_str() {
            "R" => red = runtime_number(value).map(runtime_number_to_f64),
            "G" => green = runtime_number(value).map(runtime_number_to_f64),
            "B" => blue = runtime_number(value).map(runtime_number_to_f64),
            _ => {}
        }
    }

    Some((red?, green?, blue?))
}

pub(super) fn color_pair_value(left: &Value, right: &Value, op: RuntimeNumberOp) -> Option<Value> {
    let (left_red, left_green, left_blue) = color_components(left)?;
    let (right_red, right_green, right_blue) = color_components(right)?;
    Some(color_value(
        apply_float_number_op(left_red, right_red, op),
        apply_float_number_op(left_green, right_green, op),
        apply_float_number_op(left_blue, right_blue, op),
    ))
}

pub(super) fn color_scale_value(
    color: &Value,
    factor: &Value,
    op: RuntimeNumberOp,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    let Some((red, green, blue)) = color_components(color) else {
        return Ok(None);
    };
    let factor = expect_number(factor, "color scale factor", span)?;
    if matches!(op, RuntimeNumberOp::Divide) && factor == 0.0 {
        return Err(VerseError::runtime_at("division by zero", span));
    }

    Ok(Some(color_value(
        apply_float_number_op(red, factor, op),
        apply_float_number_op(green, factor, op),
        apply_float_number_op(blue, factor, op),
    )))
}

pub(super) fn expect_color_components(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<(f64, f64, f64), VerseError> {
    color_components(value).ok_or_else(|| {
        VerseError::runtime_at(format!("{context} expected `color`, got {value}"), span)
    })
}

fn color_alpha_components(value: &Value) -> Option<(f64, f64, f64, f64)> {
    let Value::StructInstance {
        struct_name,
        fields,
        ..
    } = value
    else {
        return None;
    };
    if struct_name != "color_alpha" {
        return None;
    }

    let mut color = None;
    let mut alpha = None;
    for (name, value) in fields {
        match name.as_str() {
            "Color" => color = color_components(value),
            "A" => alpha = runtime_number(value).map(runtime_number_to_f64),
            _ => {}
        }
    }

    let (red, green, blue) = color?;
    Some((red, green, blue, alpha?))
}

pub(super) fn expect_color_alpha_components(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<(f64, f64, f64, f64), VerseError> {
    color_alpha_components(value).ok_or_else(|| {
        VerseError::runtime_at(
            format!("{context} expected `color_alpha`, got {value}"),
            span,
        )
    })
}

pub(super) fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
    let hue = hue.rem_euclid(360.0);
    if saturation == 0.0 {
        return (value, value, value);
    }

    let sector = hue / 60.0;
    let sector_index = sector.floor() as i32;
    let fraction = sector - f64::from(sector_index);
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));

    match sector_index {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    }
}

pub(super) fn rgb_to_hsv(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        (60.0 * ((green - blue) / delta)).rem_euclid(360.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };

    (hue, saturation, max)
}

pub(super) fn clamp_alpha(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

pub(super) fn native_make_color_from_srgb(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [red, green, blue]: [Value; 3] = args
        .try_into()
        .expect("native arity checked before MakeColorFromSRGB");
    Ok(NativeResult::Value(color_value(
        expect_number(&red, "`MakeColorFromSRGB` red", span)?,
        expect_number(&green, "`MakeColorFromSRGB` green", span)?,
        expect_number(&blue, "`MakeColorFromSRGB` blue", span)?,
    )))
}

pub(super) fn native_make_color_from_srgb_values(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [red, green, blue]: [Value; 3] = args
        .try_into()
        .expect("native arity checked before MakeColorFromSRGBValues");
    let red = expect_srgb_component_value(&red, "`MakeColorFromSRGBValues` red", span)?;
    let green = expect_srgb_component_value(&green, "`MakeColorFromSRGBValues` green", span)?;
    let blue = expect_srgb_component_value(&blue, "`MakeColorFromSRGBValues` blue", span)?;
    Ok(NativeResult::Value(color_value(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    )))
}

pub(super) fn native_make_srgb_from_color(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [color]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before MakeSRGBFromColor");
    let (red, green, blue) = expect_color_components(&color, "`MakeSRGBFromColor` InColor", span)?;
    Ok(NativeResult::Value(Value::Tuple(vec![
        Value::Float(red),
        Value::Float(green),
        Value::Float(blue),
    ])))
}

fn expect_srgb_component_value(value: &Value, context: &str, span: Span) -> Result<u8, VerseError> {
    let component = expect_integer(value, context, span)?;
    u8::try_from(component)
        .map_err(|_| VerseError::runtime_at(format!("{context} expected a value in 0..255"), span))
}

pub(super) fn native_make_color_from_hex(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [hex_string]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before MakeColorFromHex");
    let hex_string = expect_char_array_text(&hex_string, "`MakeColorFromHex` hexString", span)?;
    Ok(NativeResult::Value(color_from_hex_string(&hex_string)))
}

fn expect_char_array_text(value: &Value, context: &str, span: Span) -> Result<String, VerseError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Array(items) => char_array_to_string(items.borrow().as_slice()).ok_or_else(|| {
            VerseError::runtime_at(
                format!("{context} expected string-compatible `[]char`"),
                span,
            )
        }),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected `[]char`, got {value}"),
            span,
        )),
    }
}

fn color_from_hex_string(hex_string: &str) -> Value {
    let hex = hex_string.strip_prefix('#').unwrap_or(hex_string);
    let Some((red, green, blue)) = parse_hex_color_bytes(hex.as_bytes()) else {
        return color_value(0.0, 0.0, 0.0);
    };
    color_value(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    )
}

fn parse_hex_color_bytes(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    match bytes.len() {
        3 => Some((
            repeated_hex_byte(bytes[0])?,
            repeated_hex_byte(bytes[1])?,
            repeated_hex_byte(bytes[2])?,
        )),
        6 | 8 => Some((
            hex_byte(bytes[0], bytes[1])?,
            hex_byte(bytes[2], bytes[3])?,
            hex_byte(bytes[4], bytes[5])?,
        )),
        _ => None,
    }
}

fn repeated_hex_byte(value: u8) -> Option<u8> {
    let digit = hex_digit(value)?;
    Some((digit << 4) | digit)
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    Some((hex_digit(high)? << 4) | hex_digit(low)?)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(super) fn native_make_color_from_hsv(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [hue, saturation, value]: [Value; 3] = args
        .try_into()
        .expect("native arity checked before MakeColorFromHSV");
    let hue = expect_number(&hue, "`MakeColorFromHSV` Hue", span)?;
    let saturation = expect_number(&saturation, "`MakeColorFromHSV` Saturation", span)?;
    let value = expect_number(&value, "`MakeColorFromHSV` Value", span)?;
    let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
    Ok(NativeResult::Value(color_value(red, green, blue)))
}

pub(super) fn native_make_hsv_from_color(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [color]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before MakeHSVFromColor");
    let (red, green, blue) = expect_color_components(&color, "`MakeHSVFromColor` InColor", span)?;
    let (hue, saturation, value) = rgb_to_hsv(red, green, blue);
    Ok(NativeResult::Value(Value::Tuple(vec![
        Value::Float(hue),
        Value::Float(saturation),
        Value::Float(value),
    ])))
}

pub(super) fn native_make_color_alpha(
    args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let [red, green, blue, alpha]: [Value; 4] = args
        .try_into()
        .expect("native arity checked before MakeColorAlpha");
    Ok(NativeResult::Value(color_alpha_value(
        color_value(
            expect_number(&red, "`MakeColorAlpha` R", span)?,
            expect_number(&green, "`MakeColorAlpha` G", span)?,
            expect_number(&blue, "`MakeColorAlpha` B", span)?,
        ),
        expect_number(&alpha, "`MakeColorAlpha` A", span)?,
    )))
}

pub(super) fn native_over(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [front, back]: [Value; 2] = args.try_into().expect("native arity checked before Over");
    let (front_red, front_green, front_blue, front_alpha) =
        expect_color_alpha_components(&front, "`Over` CA1", span)?;
    let (back_red, back_green, back_blue, back_alpha) =
        expect_color_alpha_components(&back, "`Over` CA2", span)?;
    let front_alpha = clamp_alpha(front_alpha);
    let back_alpha = clamp_alpha(back_alpha);
    let out_alpha = front_alpha + back_alpha * (1.0 - front_alpha);

    if out_alpha == 0.0 {
        return Ok(NativeResult::Failure("both alpha components are zero"));
    }

    let back_weight = back_alpha * (1.0 - front_alpha);
    Ok(NativeResult::Value(color_alpha_value(
        color_value(
            (front_red * front_alpha + back_red * back_weight) / out_alpha,
            (front_green * front_alpha + back_green * back_weight) / out_alpha,
            (front_blue * front_alpha + back_blue * back_weight) / out_alpha,
        ),
        out_alpha,
    )))
}

fn apply_float_number_op(left: f64, right: f64, op: RuntimeNumberOp) -> f64 {
    match op {
        RuntimeNumberOp::Add => left + right,
        RuntimeNumberOp::Subtract => left - right,
        RuntimeNumberOp::Multiply => left * right,
        RuntimeNumberOp::Divide => left / right,
    }
}
