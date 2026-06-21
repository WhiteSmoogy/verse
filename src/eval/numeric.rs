use std::fmt;

use super::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RationalValue {
    pub(super) numerator: i64,
    pub(super) denominator: i64,
}

impl RationalValue {
    pub(super) fn new(numerator: i64, denominator: i64) -> Self {
        debug_assert!(denominator != 0, "rational denominator cannot be zero");
        let mut numerator = numerator;
        let mut denominator = denominator;
        if denominator < 0 {
            numerator = -numerator;
            denominator = -denominator;
        }
        let divisor = gcd_i64(numerator, denominator);
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    pub(super) fn from_int(value: i64) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    pub(super) fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    pub(super) fn is_integer(self) -> bool {
        self.denominator == 1
    }

    pub(super) fn add(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator + other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }

    pub(super) fn subtract(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator - other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }

    pub(super) fn multiply(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.numerator,
            self.denominator * other.denominator,
        )
    }

    pub(super) fn divide(self, other: Self) -> Option<Self> {
        (other.numerator != 0).then(|| {
            Self::new(
                self.numerator * other.denominator,
                self.denominator * other.numerator,
            )
        })
    }
}

impl fmt::Display for RationalValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(formatter, "{}", self.numerator)
        } else {
            write!(formatter, "{}/{}", self.numerator, self.denominator)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum RuntimeNumber {
    Int(i64),
    Float(f64),
    Rational(RationalValue),
}

pub(super) fn runtime_number(value: &Value) -> Option<RuntimeNumber> {
    match value {
        Value::Int(value) => Some(RuntimeNumber::Int(*value)),
        Value::Float(value) => Some(RuntimeNumber::Float(*value)),
        Value::Rational(value) => Some(RuntimeNumber::Rational(*value)),
        _ => None,
    }
}

pub(super) fn runtime_number_to_f64(value: RuntimeNumber) -> f64 {
    match value {
        RuntimeNumber::Int(value) => value as f64,
        RuntimeNumber::Float(value) => value,
        RuntimeNumber::Rational(value) => value.to_f64(),
    }
}

pub(super) fn runtime_number_to_rational(value: RuntimeNumber) -> Option<RationalValue> {
    match value {
        RuntimeNumber::Int(value) => Some(RationalValue::from_int(value)),
        RuntimeNumber::Rational(value) => Some(value),
        RuntimeNumber::Float(_) => None,
    }
}

pub(super) fn numeric_values_equal(left: &Value, right: &Value) -> Option<bool> {
    let left = runtime_number(left)?;
    let right = runtime_number(right)?;
    if let (Some(left), Some(right)) = (
        runtime_number_to_rational(left),
        runtime_number_to_rational(right),
    ) {
        return Some(left == right);
    }
    Some(runtime_number_to_f64(left) == runtime_number_to_f64(right))
}

pub(super) fn rational_or_int(value: RationalValue) -> Value {
    if value.is_integer() {
        Value::Int(value.numerator)
    } else {
        Value::Rational(value)
    }
}

fn gcd_i64(left: i64, right: i64) -> i64 {
    let mut left = (left as i128).abs();
    let mut right = (right as i128).abs();
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left.max(1) as i64
}
