use std::fmt;

use crate::ast::TypeParamConstraint;

use super::{ParametricTypeKind, render_effects};

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
