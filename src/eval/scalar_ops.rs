use super::Value;
use super::numeric::{RuntimeNumber, runtime_number, runtime_number_to_f64};
use crate::error::VerseError;
use crate::token::Span;

#[derive(Clone, Copy)]
pub(super) enum RuntimeNumberOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

pub(super) fn expect_number(value: &Value, context: &str, span: Span) -> Result<f64, VerseError> {
    match runtime_number(value) {
        Some(value) => Ok(runtime_number_to_f64(value)),
        None => Err(VerseError::runtime_at(
            format!("{context} expected number, got {value}"),
            span,
        )),
    }
}

pub(super) fn expect_integer(value: &Value, context: &str, span: Span) -> Result<i128, VerseError> {
    match runtime_number(value) {
        Some(RuntimeNumber::Int(value)) => Ok(value),
        Some(RuntimeNumber::Rational(value)) if value.is_integer() => Ok(value.numerator),
        Some(number) => {
            let number = runtime_number_to_f64(number);
            if number.fract() != 0.0 {
                return Err(VerseError::runtime_at(
                    format!("{context} expected integer, got {number}"),
                    span,
                ));
            }
            Ok(number as i128)
        }
        None => Err(VerseError::runtime_at(
            format!("{context} expected integer, got {value}"),
            span,
        )),
    }
}

pub(super) fn expect_index_integer(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<i64, VerseError> {
    match value {
        Value::Int(value) => i64::try_from(*value).map_err(|_| {
            VerseError::runtime_at(format!("{context} expected index in int64 range"), span)
        }),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected int, got {value}"),
            span,
        )),
    }
}
