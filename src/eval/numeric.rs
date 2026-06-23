use std::fmt;

use super::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RationalValue {
    pub(super) numerator: i128,
    pub(super) denominator: i128,
}

impl RationalValue {
    pub(crate) fn new(numerator: i128, denominator: i128) -> Self {
        debug_assert!(denominator != 0, "rational denominator cannot be zero");
        let mut numerator = numerator;
        let mut denominator = denominator;
        let divisor = gcd_i128(numerator, denominator);
        numerator = div_i128_by_u128(numerator, divisor);
        denominator = div_i128_by_u128(denominator, divisor);
        if denominator < 0
            && let (Some(flipped_numerator), Some(flipped_denominator)) =
                (numerator.checked_neg(), denominator.checked_neg())
        {
            numerator = flipped_numerator;
            denominator = flipped_denominator;
        }
        Self {
            numerator,
            denominator,
        }
    }

    pub(crate) fn from_int(value: i128) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    pub(crate) fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    pub(crate) fn is_integer(self) -> bool {
        self.denominator == 1
    }

    pub(crate) fn is_zero(self) -> bool {
        self.numerator == 0
    }

    pub(crate) fn checked_neg(self) -> Option<Self> {
        self.numerator
            .checked_neg()
            .map(|numerator| Self::new(numerator, self.denominator))
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator + other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }

    pub(crate) fn subtract(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator - other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }

    pub(crate) fn multiply(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.numerator,
            self.denominator * other.denominator,
        )
    }

    pub(crate) fn divide(self, other: Self) -> Option<Self> {
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
    Int(i128),
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

pub(crate) fn rational_or_int(value: RationalValue) -> Value {
    if value.is_integer() {
        Value::Int(value.numerator)
    } else {
        Value::Rational(value)
    }
}

fn gcd_i128(left: i128, right: i128) -> u128 {
    let mut left = left.unsigned_abs();
    let mut right = right.unsigned_abs();
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left.max(1)
}

fn div_i128_by_u128(value: i128, divisor: u128) -> i128 {
    if divisor == 1 {
        return value;
    }
    if let Ok(divisor) = i128::try_from(divisor) {
        return value / divisor;
    }
    debug_assert_eq!(value, i128::MIN);
    -1
}
