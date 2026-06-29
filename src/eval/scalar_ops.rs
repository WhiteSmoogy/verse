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

pub(super) fn expect_integer(value: &Value, context: &str, span: Span) -> Result<i64, VerseError> {
    match runtime_number(value) {
        Some(RuntimeNumber::Int(value)) => Ok(value),
        Some(RuntimeNumber::Rational(value)) if value.is_integer() => {
            i64::try_from(value.numerator).map_err(|_| {
                VerseError::runtime_at(format!("{context} integer is outside int64 range"), span)
            })
        }
        Some(number) => {
            let number = runtime_number_to_f64(number);
            if number.fract() != 0.0 {
                return Err(VerseError::runtime_at(
                    format!("{context} expected integer, got {number}"),
                    span,
                ));
            }
            if number < i64::MIN as f64 || number >= I64_MAX_EXCLUSIVE_AS_F64 {
                return Err(VerseError::runtime_at(
                    format!("{context} integer is outside int64 range"),
                    span,
                ));
            }
            Ok(number as i64)
        }
        None => Err(VerseError::runtime_at(
            format!("{context} expected integer, got {value}"),
            span,
        )),
    }
}

const I64_MAX_EXCLUSIVE_AS_F64: f64 = 9_223_372_036_854_775_808.0;

pub(super) fn expect_index_integer(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<i64, VerseError> {
    match value {
        Value::Int(value) => Ok(*value),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected int, got {value}"),
            span,
        )),
    }
}
