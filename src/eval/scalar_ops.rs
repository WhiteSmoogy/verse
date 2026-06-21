use super::color_ops::{color_pair_value, color_scale_value};
use super::numeric::{
    RationalValue, RuntimeNumber, rational_or_int, runtime_number, runtime_number_to_f64,
    runtime_number_to_rational,
};
use super::validation::char_array_to_string;
use super::value_ops::value_copy;
use super::{Value, array_value};
use crate::ast::BinaryOp;
use crate::error::VerseError;
use crate::token::Span;
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn positive_value(value: Value, span: Span) -> Result<Value, VerseError> {
    if runtime_number(&value).is_some() {
        Ok(value)
    } else {
        Err(VerseError::runtime_at(
            format!("unary `+` expected number, got {value}"),
            span,
        ))
    }
}

pub(super) fn negate_value(value: Value, span: Span) -> Result<Value, VerseError> {
    match runtime_number(&value) {
        Some(RuntimeNumber::Int(value)) => value
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| VerseError::runtime_at("integer negation overflow", span)),
        Some(RuntimeNumber::Float(value)) => Ok(Value::Float(-value)),
        Some(RuntimeNumber::Rational(value)) => Ok(Value::Rational(RationalValue::new(
            -value.numerator,
            value.denominator,
        ))),
        None => Err(VerseError::runtime_at(
            format!("unary `-` expected number, got {value}"),
            span,
        )),
    }
}

pub(super) fn add_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_pair_value(&left, &right, RuntimeNumberOp::Add) {
        return Ok(value);
    }

    match (left, right) {
        (left, right) if runtime_number(&left).is_some() && runtime_number(&right).is_some() => {
            numeric_binary_value(left, right, RuntimeNumberOp::Add, span)
        }
        (Value::String(left), Value::String(right)) => Ok(Value::String(format!("{left}{right}"))),
        (Value::Diagnostic(left), Value::Diagnostic(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::Diagnostic(left), Value::String(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::String(left), Value::Diagnostic(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::ClassifiableSubset(left), Value::ClassifiableSubset(right)) => {
            let left = left.borrow();
            let right = right.borrow();
            let mut values: Vec<Value> = Vec::new();
            for value in left.iter().chain(right.iter()) {
                if !values.iter().any(|existing| existing == value) {
                    values.push(value_copy(value));
                }
            }
            Ok(Value::ClassifiableSubset(Rc::new(RefCell::new(values))))
        }
        (Value::String(left), Value::Array(right)) => {
            let Some(right) = char_array_to_string(right.borrow().as_slice()) else {
                return Err(VerseError::runtime_at(
                    "`+` expected string-compatible `[]char`",
                    span,
                ));
            };
            Ok(Value::String(format!("{left}{right}")))
        }
        (Value::Array(left), Value::String(right)) => {
            let Some(left) = char_array_to_string(left.borrow().as_slice()) else {
                return Err(VerseError::runtime_at(
                    "`+` expected string-compatible `[]char`",
                    span,
                ));
            };
            Ok(Value::String(format!("{left}{right}")))
        }
        (Value::Array(left), Value::Array(right)) => {
            let mut values: Vec<Value> = left.borrow().iter().map(value_copy).collect();
            values.extend(right.borrow().iter().map(value_copy));
            Ok(array_value(values))
        }
        (Value::Array(left), Value::Tuple(right)) => {
            let mut values: Vec<Value> = left.borrow().iter().map(value_copy).collect();
            values.extend(right.iter().map(value_copy));
            Ok(array_value(values))
        }
        (left, right) => Err(VerseError::runtime_at(
            format!("`+` cannot combine `{left}` and `{right}`"),
            span,
        )),
    }
}

pub(super) fn eval_binary_values(
    left: Value,
    op: BinaryOp,
    right: Value,
    span: Span,
) -> Result<Value, VerseError> {
    match op {
        BinaryOp::Add => add_values(left, right, span),
        BinaryOp::Subtract => subtract_values(left, right, span),
        BinaryOp::Multiply => multiply_values(left, right, span),
        BinaryOp::Divide => divide_values(left, right, span),
        BinaryOp::Remainder => {
            if numeric_value_is_zero(&right, "`%` right operand", span)? {
                return Err(VerseError::runtime_at("remainder by zero", span));
            }
            remainder_values(left, right, span)
        }
        BinaryOp::Range => {
            let start = expect_integer(&left, "range start", span)?;
            let end = expect_integer(&right, "range end", span)?;
            Ok(Value::Range { start, end })
        }
        BinaryOp::Equal => Ok(Value::Bool(left == right)),
        BinaryOp::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOp::Less => Ok(Value::Bool(
            expect_number(&left, "`<` left operand", span)?
                < expect_number(&right, "`<` right operand", span)?,
        )),
        BinaryOp::LessEqual => Ok(Value::Bool(
            expect_number(&left, "`<=` left operand", span)?
                <= expect_number(&right, "`<=` right operand", span)?,
        )),
        BinaryOp::Greater => Ok(Value::Bool(
            expect_number(&left, "`>` left operand", span)?
                > expect_number(&right, "`>` right operand", span)?,
        )),
        BinaryOp::GreaterEqual => Ok(Value::Bool(
            expect_number(&left, "`>=` left operand", span)?
                >= expect_number(&right, "`>=` right operand", span)?,
        )),
        BinaryOp::And | BinaryOp::Or => unreachable!("short-circuited before value evaluation"),
    }
}

pub(super) fn subtract_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_pair_value(&left, &right, RuntimeNumberOp::Subtract) {
        return Ok(value);
    }

    numeric_binary_value(left, right, RuntimeNumberOp::Subtract, span)
}

pub(super) fn multiply_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_pair_value(&left, &right, RuntimeNumberOp::Multiply) {
        return Ok(value);
    }
    if let Some(value) = color_scale_value(&left, &right, RuntimeNumberOp::Multiply, span)? {
        return Ok(value);
    }
    if let Some(value) = color_scale_value(&right, &left, RuntimeNumberOp::Multiply, span)? {
        return Ok(value);
    }

    numeric_binary_value(left, right, RuntimeNumberOp::Multiply, span)
}

pub(super) fn divide_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_scale_value(&left, &right, RuntimeNumberOp::Divide, span)? {
        return Ok(value);
    }

    numeric_binary_value(left, right, RuntimeNumberOp::Divide, span)
}

pub(super) fn remainder_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    let Some(left_number) = runtime_number(&left) else {
        return Err(VerseError::runtime_at(
            format!("left operand expected number, got {left}"),
            span,
        ));
    };
    let Some(right_number) = runtime_number(&right) else {
        return Err(VerseError::runtime_at(
            format!("right operand expected number, got {right}"),
            span,
        ));
    };

    match (left_number, right_number) {
        (RuntimeNumber::Int(left), RuntimeNumber::Int(right)) => Ok(Value::Int(left % right)),
        (left, right) => Ok(Value::Float(
            runtime_number_to_f64(left) % runtime_number_to_f64(right),
        )),
    }
}

#[derive(Clone, Copy)]
pub(super) enum RuntimeNumberOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

fn numeric_binary_value(
    left: Value,
    right: Value,
    op: RuntimeNumberOp,
    span: Span,
) -> Result<Value, VerseError> {
    let Some(left_number) = runtime_number(&left) else {
        return Err(VerseError::runtime_at(
            format!("left operand expected number, got {left}"),
            span,
        ));
    };
    let Some(right_number) = runtime_number(&right) else {
        return Err(VerseError::runtime_at(
            format!("right operand expected number, got {right}"),
            span,
        ));
    };

    if matches!(op, RuntimeNumberOp::Divide)
        && numeric_value_is_zero(&right, "`/` right operand", span)?
    {
        return Err(VerseError::runtime_at("division by zero", span));
    }

    if matches!(
        (left_number, right_number),
        (RuntimeNumber::Float(_), _) | (_, RuntimeNumber::Float(_))
    ) {
        return Ok(Value::Float(apply_float_number_op(
            runtime_number_to_f64(left_number),
            runtime_number_to_f64(right_number),
            op,
        )));
    }

    let left_rational =
        runtime_number_to_rational(left_number).expect("non-float number should be rational");
    let right_rational =
        runtime_number_to_rational(right_number).expect("non-float number should be rational");

    match op {
        RuntimeNumberOp::Add => Ok(rational_or_int(left_rational.add(right_rational))),
        RuntimeNumberOp::Subtract => Ok(rational_or_int(left_rational.subtract(right_rational))),
        RuntimeNumberOp::Multiply => Ok(rational_or_int(left_rational.multiply(right_rational))),
        RuntimeNumberOp::Divide => Ok(Value::Rational(
            left_rational
                .divide(right_rational)
                .expect("division by zero checked before rational division"),
        )),
    }
}

fn apply_float_number_op(left: f64, right: f64, op: RuntimeNumberOp) -> f64 {
    match op {
        RuntimeNumberOp::Add => left + right,
        RuntimeNumberOp::Subtract => left - right,
        RuntimeNumberOp::Multiply => left * right,
        RuntimeNumberOp::Divide => left / right,
    }
}

pub(super) fn numeric_value_is_zero(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<bool, VerseError> {
    match runtime_number(value) {
        Some(RuntimeNumber::Int(value)) => Ok(value == 0),
        Some(RuntimeNumber::Float(value)) => Ok(value == 0.0),
        Some(RuntimeNumber::Rational(value)) => Ok(value.numerator == 0),
        None => Err(VerseError::runtime_at(
            format!("{context} expected number, got {value}"),
            span,
        )),
    }
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

pub(super) fn expect_bool(value: &Value, context: &str, span: Span) -> Result<bool, VerseError> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected bool, got {value}"),
            span,
        )),
    }
}

pub(super) fn expect_integer(value: &Value, context: &str, span: Span) -> Result<i64, VerseError> {
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
            Ok(number as i64)
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
        Value::Int(value) => Ok(*value),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected int, got {value}"),
            span,
        )),
    }
}

pub(super) fn expect_index(value: &Value, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, "array index", span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("array index cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}

pub(super) fn expect_tuple_index(value: &Value, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, "tuple index", span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("tuple index cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}
