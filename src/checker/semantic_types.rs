use std::collections::HashMap;
use std::fmt;

use crate::ast::{Expr, TypeName, TypeParamConstraint};
use crate::error::VerseError;
use crate::token::Span;

use super::{
    AggregateKind, ParametricTypeKind, StructFieldInfo, StructInfo, char_array_type, color_type,
    is_builtin_comparable_class_name, is_byte_char_type, is_char_type, is_string_char_type,
    render_effects,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntRange {
    pub min: i64,
    pub max: i64,
}

impl IntRange {
    pub fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }

    pub fn contains(self, value: i64) -> bool {
        self.min <= value && value <= self.max
    }

    pub fn contains_range(self, other: Self) -> bool {
        self.min <= other.min && other.max <= self.max
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    IntRange(IntRange),
    Float,
    Rational,
    Number,
    Bool,
    String,
    Message,
    Char,
    Char8,
    Char32,
    None,
    Any,
    Comparable,
    TypeValue,
    Unknown,
    Never,
    Range,
    Enum(String),
    EnumType(String),
    Struct(String),
    StructType(String),
    Class(String),
    ClassType(String),
    Interface(String),
    InterfaceType(String),
    Module(String),
    Param(String, TypeParamConstraint),
    ParametricType {
        name: String,
        params: Vec<String>,
        kind: ParametricTypeKind,
    },
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    WeakMap(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Event(Option<Box<Type>>),
    Task(Box<Type>),
    Generator(Option<Box<Type>>),
    Subtype(Box<Type>),
    CastableSubtype(Box<Type>),
    ConcreteSubtype(Box<Type>),
    ClassifiableSubset(Box<Type>),
    Modifier(Box<Type>),
    ModifierStack(Box<Type>),
    Awaitable(Option<Box<Type>>),
    Signalable(Box<Type>),
    Subscribable(Option<Box<Type>>),
    Listenable(Option<Box<Type>>),
    Function {
        arity: Option<usize>,
        arity_range: Option<(usize, usize)>,
        effects: Vec<String>,
        param_types: Option<Vec<Type>>,
        param_specs: Option<Vec<ParamSpec>>,
        return_type: Box<Type>,
    },
    Overload(Vec<Type>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamSpec {
    pub(super) name: String,
    pub(super) value_type: Type,
    pub(super) named: bool,
    pub(super) has_default: bool,
    pub(super) tuple_items: Option<Vec<ParamSpec>>,
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => write!(formatter, "int"),
            Self::IntRange(range) => write!(formatter, "int_range({}, {})", range.min, range.max),
            Self::Float => write!(formatter, "float"),
            Self::Rational => write!(formatter, "rational"),
            Self::Number => write!(formatter, "number"),
            Self::Bool => write!(formatter, "bool"),
            Self::String => write!(formatter, "string"),
            Self::Message => write!(formatter, "message"),
            Self::Char => write!(formatter, "char"),
            Self::Char8 => write!(formatter, "char8"),
            Self::Char32 => write!(formatter, "char32"),
            Self::None => write!(formatter, "none"),
            Self::Any => write!(formatter, "any"),
            Self::Comparable => write!(formatter, "comparable"),
            Self::TypeValue => write!(formatter, "type"),
            Self::Unknown => write!(formatter, "unknown"),
            Self::Never => write!(formatter, "never"),
            Self::Range => write!(formatter, "range"),
            Self::Enum(name) => write!(formatter, "{name}"),
            Self::EnumType(name) => write!(formatter, "enum<{name}>"),
            Self::Struct(name) => write!(formatter, "{name}"),
            Self::StructType(name) => write!(formatter, "struct<{name}>"),
            Self::Class(name) => write!(formatter, "{name}"),
            Self::ClassType(name) => write!(formatter, "class<{name}>"),
            Self::Interface(name) => write!(formatter, "{name}"),
            Self::InterfaceType(name) => write!(formatter, "interface<{name}>"),
            Self::Module(name) => write!(formatter, "module<{name}>"),
            Self::Param(name, _) => write!(formatter, "{name}"),
            Self::ParametricType { name, params, .. } => {
                write!(formatter, "parametric_type<{name}/{}>", params.len())
            }
            Self::Array(item) => write!(formatter, "array<{item}>"),
            Self::Map(key, value) => write!(formatter, "map<{key}, {value}>"),
            Self::WeakMap(key, value) => write!(formatter, "weak_map<{key}, {value}>"),
            Self::Tuple(items) => {
                let rendered = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "tuple({rendered})")
            }
            Self::Option(item) => write!(formatter, "?{item}"),
            Self::Result(success, error) => write!(formatter, "result({success},{error})"),
            Self::Event(Some(payload)) => write!(formatter, "event({payload})"),
            Self::Event(None) => write!(formatter, "event()"),
            Self::Task(payload) => write!(formatter, "task({payload})"),
            Self::Generator(Some(item)) => write!(formatter, "generator({item})"),
            Self::Generator(None) => write!(formatter, "generator()"),
            Self::Subtype(item) => write!(formatter, "subtype({item})"),
            Self::CastableSubtype(item) => write!(formatter, "castable_subtype({item})"),
            Self::ConcreteSubtype(item) => write!(formatter, "concrete_subtype({item})"),
            Self::ClassifiableSubset(item) => write!(formatter, "classifiable_subset({item})"),
            Self::Modifier(item) => write!(formatter, "modifier({item})"),
            Self::ModifierStack(item) => write!(formatter, "modifier_stack({item})"),
            Self::Awaitable(Some(payload)) => write!(formatter, "awaitable({payload})"),
            Self::Awaitable(None) => write!(formatter, "awaitable()"),
            Self::Signalable(payload) => write!(formatter, "signalable({payload})"),
            Self::Subscribable(Some(payload)) => write!(formatter, "subscribable({payload})"),
            Self::Subscribable(None) => write!(formatter, "subscribable()"),
            Self::Listenable(Some(payload)) => write!(formatter, "listenable({payload})"),
            Self::Listenable(None) => write!(formatter, "listenable()"),
            Self::Function {
                arity,
                arity_range,
                effects,
                return_type,
                ..
            } => {
                if let Some(arity) = arity {
                    write!(
                        formatter,
                        "function/{arity}{} -> {return_type}",
                        render_effects(effects)
                    )
                } else if let Some((min, max)) = arity_range {
                    write!(
                        formatter,
                        "function/{min}..={max}{} -> {return_type}",
                        render_effects(effects)
                    )
                } else {
                    write!(
                        formatter,
                        "function/*{} -> {return_type}",
                        render_effects(effects)
                    )
                }
            }
            Self::Overload(overloads) => {
                let rendered = overloads
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ");
                write!(formatter, "overload({rendered})")
            }
        }
    }
}

pub(super) fn ensure_number_like(
    value_type: &Type,
    context: &str,
    span: Span,
) -> Result<(), VerseError> {
    match value_type {
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::Rational
        | Type::Number
        | Type::Unknown
        | Type::Any => Ok(()),
        other => Err(VerseError::check_at(
            format!("{context} expected `number`, got `{other}`"),
            span,
        )),
    }
}

pub(super) fn ensure_bool_like(
    value_type: &Type,
    context: &str,
    span: Span,
) -> Result<(), VerseError> {
    match value_type {
        Type::Bool | Type::Unknown | Type::Any => Ok(()),
        other => Err(VerseError::check_at(
            format!("{context} expected `bool`, got `{other}`"),
            span,
        )),
    }
}

pub(super) fn type_param_constraint_declares_comparable(constraint: &TypeParamConstraint) -> bool {
    matches!(
        constraint,
        TypeParamConstraint::Subtype(TypeName::Comparable)
    )
}

pub(super) fn ensure_comparable_key(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
) -> Result<(), VerseError> {
    ensure_comparable_key_inner(value_type, struct_types, span, &mut Vec::new())
}

pub(super) fn ensure_equality_comparable(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
) -> Result<(), VerseError> {
    ensure_equality_comparable_inner(value_type, struct_types, span, &mut Vec::new())
}

pub(super) fn ensure_equality_comparable_inner(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
    visiting_aggregates: &mut Vec<String>,
) -> Result<(), VerseError> {
    match value_type {
        Type::Function { .. }
        | Type::Overload(_)
        | Type::Range
        | Type::Never
        | Type::TypeValue
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::ParametricType { .. }
        | Type::WeakMap(_, _)
        | Type::Result(_, _)
        | Type::Event(_)
        | Type::Task(_)
        | Type::Generator(_)
        | Type::Subtype(_)
        | Type::CastableSubtype(_)
        | Type::ConcreteSubtype(_)
        | Type::ClassifiableSubset(_)
        | Type::Modifier(_)
        | Type::ModifierStack(_)
        | Type::Awaitable(_)
        | Type::Signalable(_)
        | Type::Subscribable(_)
        | Type::Listenable(_) => Err(VerseError::check_at(
            format!("equality operand type `{value_type}` is not comparable"),
            span,
        )),
        Type::Param(_, constraint) if type_param_constraint_declares_comparable(constraint) => {
            Ok(())
        }
        Type::Param(_, _) => Err(VerseError::check_at(
            format!("equality operand type `{value_type}` is not comparable"),
            span,
        )),
        Type::Class(name) => {
            if !struct_types.contains_key(name) && is_builtin_comparable_class_name(name) {
                return Ok(());
            }
            let Some(info) = struct_types.get(name) else {
                return Err(VerseError::check_at(
                    format!("equality operand type `{value_type}` is not comparable"),
                    span,
                ));
            };
            if info.kind != AggregateKind::Class {
                return Err(VerseError::check_at(
                    format!("equality operand type `{value_type}` is not comparable"),
                    span,
                ));
            }
            if info.unique {
                return Ok(());
            }
            ensure_aggregate_fields_equality_comparable(
                "class",
                name,
                &info.fields,
                struct_types,
                span,
                visiting_aggregates,
            )
        }
        Type::Struct(name) => {
            let Some(info) = struct_types.get(name) else {
                return Err(VerseError::check_at(
                    format!("unknown struct `{name}`"),
                    span,
                ));
            };
            ensure_aggregate_fields_equality_comparable(
                "struct",
                name,
                &info.fields,
                struct_types,
                span,
                visiting_aggregates,
            )
        }
        Type::Array(item) | Type::Option(item) => {
            ensure_equality_comparable_inner(item, struct_types, span, visiting_aggregates)
        }
        Type::Map(key, value) => {
            ensure_equality_comparable_inner(key, struct_types, span, visiting_aggregates)?;
            ensure_equality_comparable_inner(value, struct_types, span, visiting_aggregates)
        }
        Type::Tuple(items) => {
            for item in items {
                ensure_equality_comparable_inner(item, struct_types, span, visiting_aggregates)?;
            }
            Ok(())
        }
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::Rational
        | Type::Number
        | Type::Bool
        | Type::String
        | Type::Message
        | Type::Char
        | Type::Char8
        | Type::Char32
        | Type::None
        | Type::Comparable
        | Type::Enum(_)
        | Type::Any
        | Type::Unknown => Ok(()),
    }
}

pub(super) fn ensure_aggregate_fields_equality_comparable(
    label: &str,
    name: &str,
    fields: &[StructFieldInfo],
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
    visiting_aggregates: &mut Vec<String>,
) -> Result<(), VerseError> {
    if visiting_aggregates.iter().any(|visiting| visiting == name) {
        return Ok(());
    }
    visiting_aggregates.push(name.to_string());
    for field in fields {
        if ensure_equality_comparable_inner(
            &field.value_type,
            struct_types,
            span,
            visiting_aggregates,
        )
        .is_err()
        {
            visiting_aggregates.pop();
            return Err(VerseError::check_at(
                format!(
                    "equality {label} `{name}` field `{}` type `{}` is not comparable",
                    field.name, field.value_type
                ),
                span,
            ));
        }
    }
    visiting_aggregates.pop();
    Ok(())
}

pub(super) fn ensure_comparable_key_inner(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
    visiting_structs: &mut Vec<String>,
) -> Result<(), VerseError> {
    match value_type {
        Type::Function { .. }
        | Type::Overload(_)
        | Type::Range
        | Type::Never
        | Type::TypeValue
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::ParametricType { .. }
        | Type::Result(_, _)
        | Type::Event(_)
        | Type::Task(_)
        | Type::Generator(_)
        | Type::Subtype(_)
        | Type::CastableSubtype(_)
        | Type::ConcreteSubtype(_)
        | Type::ClassifiableSubset(_)
        | Type::Modifier(_)
        | Type::ModifierStack(_)
        | Type::Awaitable(_)
        | Type::Signalable(_)
        | Type::Subscribable(_)
        | Type::Listenable(_) => Err(VerseError::check_at(
            format!("map key type `{value_type}` is not comparable"),
            span,
        )),
        Type::Param(_, constraint) if type_param_constraint_declares_comparable(constraint) => {
            Ok(())
        }
        Type::Param(_, _) => Err(VerseError::check_at(
            format!("map key type `{value_type}` is not comparable"),
            span,
        )),
        Type::Class(name) => {
            let comparable = (!struct_types.contains_key(name)
                && is_builtin_comparable_class_name(name))
                || struct_types
                    .get(name)
                    .is_some_and(|info| info.kind == AggregateKind::Class && info.unique);
            if comparable {
                Ok(())
            } else {
                Err(VerseError::check_at(
                    format!("map key type `{value_type}` is not comparable"),
                    span,
                ))
            }
        }
        Type::Array(item) => {
            ensure_comparable_key_inner(item, struct_types, span, visiting_structs)
        }
        Type::Map(key, value) => {
            ensure_comparable_key_inner(key, struct_types, span, visiting_structs)?;
            ensure_comparable_key_inner(value, struct_types, span, visiting_structs)
        }
        Type::WeakMap(_, _) => Err(VerseError::check_at(
            "weak_map values are not comparable map keys",
            span,
        )),
        Type::Tuple(items) => {
            for item in items {
                ensure_comparable_key_inner(item, struct_types, span, visiting_structs)?;
            }
            Ok(())
        }
        Type::Option(item) => {
            ensure_comparable_key_inner(item, struct_types, span, visiting_structs)
        }
        Type::Struct(name) => {
            let Some(info) = struct_types.get(name) else {
                return Err(VerseError::check_at(
                    format!("unknown struct `{name}`"),
                    span,
                ));
            };
            if visiting_structs.iter().any(|visiting| visiting == name) {
                return Ok(());
            }
            visiting_structs.push(name.clone());
            for field in &info.fields {
                if ensure_comparable_key_inner(
                    &field.value_type,
                    struct_types,
                    span,
                    visiting_structs,
                )
                .is_err()
                {
                    visiting_structs.pop();
                    return Err(VerseError::check_at(
                        format!(
                            "map key struct `{name}` field `{}` type `{}` is not comparable",
                            field.name, field.value_type
                        ),
                        span,
                    ));
                }
            }
            visiting_structs.pop();
            Ok(())
        }
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::Rational
        | Type::Number
        | Type::Bool
        | Type::String
        | Type::Message
        | Type::Char
        | Type::Char8
        | Type::Char32
        | Type::None
        | Type::Comparable
        | Type::Enum(_)
        | Type::Any
        | Type::Unknown => Ok(()),
    }
}

pub(super) fn ensure_exact_arg_count(
    name: &str,
    args: &[Expr],
    expected: usize,
    span: Span,
) -> Result<(), VerseError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(VerseError::check_at(
            format!("`{name}` expected {expected} arguments, got {}", args.len()),
            span,
        ))
    }
}

pub(super) fn ensure_arg_count_range(
    name: &str,
    args: &[Expr],
    min: usize,
    max: usize,
    span: Span,
) -> Result<(), VerseError> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(VerseError::check_at(
            format!(
                "`{name}` expected {min}..={max} arguments, got {}",
                args.len()
            ),
            span,
        ))
    }
}

pub(super) fn check_add(
    left_type: &Type,
    left_span: Span,
    right_type: &Type,
    right_span: Span,
) -> Result<Type, VerseError> {
    match (left_type, right_type) {
        (left, right) if is_numeric_type(left) && is_numeric_type(right) => {
            Ok(unify_numeric_types(left, right))
        }
        (left, right) if is_color_type(left) && is_color_type(right) => Ok(color_type()),
        (Type::String, Type::String) => Ok(Type::String),
        (left, right) if is_diagnostic_type(left) && is_diagnostic_type(right) => {
            Ok(diagnostic_type())
        }
        (left, Type::String) if is_diagnostic_type(left) => Ok(diagnostic_type()),
        (Type::String, right) if is_diagnostic_type(right) => Ok(diagnostic_type()),
        (Type::ClassifiableSubset(left), Type::ClassifiableSubset(right)) => Ok(
            Type::ClassifiableSubset(Box::new(unify_types(left, right, left_span)?)),
        ),
        (Type::Subtype(left), Type::Subtype(right)) => Ok(Type::Subtype(Box::new(unify_types(
            left, right, left_span,
        )?))),
        (Type::String, Type::Array(item)) | (Type::Array(item), Type::String)
            if is_string_char_type(item) =>
        {
            Ok(Type::String)
        }
        (Type::Array(left), Type::Array(right)) if is_char_type(left) && is_char_type(right) => {
            Ok(char_array_type())
        }
        (Type::Array(left), Type::Array(right)) => {
            Ok(Type::Array(Box::new(unify_types(left, right, left_span)?)))
        }
        (Type::Array(left), Type::Tuple(items)) => {
            let mut item_type = left.as_ref().clone();
            for item in items {
                item_type = unify_types(&item_type, item, right_span)?;
            }
            Ok(Type::Array(Box::new(item_type)))
        }
        (Type::Unknown, _) | (_, Type::Unknown) | (Type::Any, _) | (_, Type::Any) => {
            Ok(Type::Unknown)
        }
        _ => Err(VerseError::check_at(
            format!(
                "`+` expects two numbers, colors, strings, diagnostics, classifiable subsets, arrays, or array plus tuple, got `{left_type}` and `{right_type}`"
            ),
            left_span.through(right_span),
        )),
    }
}

pub(super) fn check_subtract(
    left_type: &Type,
    left_span: Span,
    right_type: &Type,
    right_span: Span,
) -> Result<Type, VerseError> {
    match (left_type, right_type) {
        (left, right) if is_color_type(left) && is_color_type(right) => Ok(color_type()),
        (Type::Unknown, _) | (_, Type::Unknown) | (Type::Any, _) | (_, Type::Any) => {
            Ok(Type::Unknown)
        }
        _ => {
            ensure_number_like(left_type, "left operand", left_span)?;
            ensure_number_like(right_type, "right operand", right_span)?;
            Ok(unify_numeric_types(left_type, right_type))
        }
    }
}

pub(super) fn check_multiply(
    left_type: &Type,
    left_span: Span,
    right_type: &Type,
    right_span: Span,
) -> Result<Type, VerseError> {
    match (left_type, right_type) {
        (left, right) if is_color_type(left) && is_color_type(right) => Ok(color_type()),
        (left, right) if is_color_type(left) && is_numeric_type(right) => Ok(color_type()),
        (left, right) if is_numeric_type(left) && is_color_type(right) => Ok(color_type()),
        (Type::Unknown, _) | (_, Type::Unknown) | (Type::Any, _) | (_, Type::Any) => {
            Ok(Type::Unknown)
        }
        _ => {
            ensure_number_like(left_type, "left operand", left_span)?;
            ensure_number_like(right_type, "right operand", right_span)?;
            Ok(unify_numeric_types(left_type, right_type))
        }
    }
}

pub(super) fn check_divide(
    left_type: &Type,
    left_span: Span,
    right_type: &Type,
    right_span: Span,
) -> Result<Type, VerseError> {
    match (left_type, right_type) {
        (left, right) if is_color_type(left) && is_numeric_type(right) => Ok(color_type()),
        (Type::Unknown, _) | (_, Type::Unknown) | (Type::Any, _) | (_, Type::Any) => {
            Ok(Type::Unknown)
        }
        _ => {
            ensure_number_like(left_type, "left operand", left_span)?;
            ensure_number_like(right_type, "right operand", right_span)?;
            Ok(divide_numeric_type(left_type, right_type))
        }
    }
}

pub(super) fn diagnostic_type() -> Type {
    Type::Class("diagnostic".to_string())
}

pub(super) fn is_diagnostic_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Class(name) if name == "diagnostic")
}

pub(super) fn is_color_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Struct(name) if name == "color")
}

pub(super) fn is_numeric_type(value_type: &Type) -> bool {
    matches!(
        value_type,
        Type::Int | Type::IntRange(_) | Type::Float | Type::Rational | Type::Number
    )
}

pub(super) fn unify_numeric_types(left: &Type, right: &Type) -> Type {
    match (left, right) {
        (Type::Float, _) | (_, Type::Float) => Type::Float,
        (Type::Rational, _) | (_, Type::Rational) => Type::Rational,
        (Type::Int | Type::IntRange(_), Type::Int | Type::IntRange(_)) => Type::Int,
        _ => Type::Number,
    }
}

pub(super) fn divide_numeric_type(left: &Type, right: &Type) -> Type {
    match (left, right) {
        (Type::Float, _) | (_, Type::Float) => Type::Float,
        (
            Type::Int | Type::IntRange(_) | Type::Rational,
            Type::Int | Type::IntRange(_) | Type::Rational,
        ) => Type::Rational,
        _ => unify_numeric_types(left, right),
    }
}

pub(super) fn unify_types(left: &Type, right: &Type, span: Span) -> Result<Type, VerseError> {
    if left == right {
        return Ok(left.clone());
    }

    match (left, right) {
        (Type::Never, other) | (other, Type::Never) => Ok(other.clone()),
        (Type::Unknown, other) | (other, Type::Unknown) => Ok(other.clone()),
        (Type::Any, other) | (other, Type::Any) => Ok(other.clone()),
        (left, right) if is_numeric_type(left) && is_numeric_type(right) => {
            Ok(unify_numeric_types(left, right))
        }
        (left, right) if is_byte_char_type(left) && is_byte_char_type(right) => Ok(Type::Char),
        (Type::String, Type::Array(item)) | (Type::Array(item), Type::String)
            if is_string_char_type(item) =>
        {
            Ok(Type::String)
        }
        (Type::Array(left), Type::Array(right)) => {
            Ok(Type::Array(Box::new(unify_types(left, right, span)?)))
        }
        (Type::Map(left_key, left_value), Type::Map(right_key, right_value)) => Ok(Type::Map(
            Box::new(unify_types(left_key, right_key, span)?),
            Box::new(unify_types(left_value, right_value, span)?),
        )),
        (
            Type::WeakMap(left_key, left_value),
            Type::WeakMap(right_key, right_value) | Type::Map(right_key, right_value),
        )
        | (Type::Map(left_key, left_value), Type::WeakMap(right_key, right_value)) => {
            Ok(Type::WeakMap(
                Box::new(unify_types(left_key, right_key, span)?),
                Box::new(unify_types(left_value, right_value, span)?),
            ))
        }
        (Type::Tuple(left_items), Type::Tuple(right_items))
            if left_items.len() == right_items.len() =>
        {
            left_items
                .iter()
                .zip(right_items)
                .map(|(left, right)| unify_types(left, right, span))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple)
        }
        (Type::Option(left), Type::Option(right)) => {
            Ok(Type::Option(Box::new(unify_types(left, right, span)?)))
        }
        (Type::Result(left_success, left_error), Type::Result(right_success, right_error)) => {
            Ok(Type::Result(
                Box::new(unify_types(left_success, right_success, span)?),
                Box::new(unify_types(left_error, right_error, span)?),
            ))
        }
        _ => Err(VerseError::check_at(
            format!("incompatible types `{left}` and `{right}`"),
            span,
        )),
    }
}
