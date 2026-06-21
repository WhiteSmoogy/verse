use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{
    ArchetypeConstructorCall, ArchetypeEntry, ArchetypeLet, AssignOp, BinaryOp, CallArg, CaseArm,
    CasePattern, ClassBlock, ClassMethod, ConcurrentOp, Expr, ExprKind, ExtensionMethod,
    ForBinding, ForClause, InterpolatedStringPart, Param, ParamPattern, Program, Stmt, StmtKind,
    StructField, TypeAnnotation, TypeName, TypeParam, TypeParamConstraint, UnaryOp,
};
use crate::colors::NAMED_COLORS;
use crate::error::VerseError;
use crate::parser::parse_source;
use crate::token::{CharacterKind, NumberKind, NumberLiteral, Span};

pub fn check_source(source: &str) -> Result<Type, VerseError> {
    let program = parse_source(source)?;
    Checker::new().check_program(&program)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
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
    name: String,
    value_type: Type,
    named: bool,
    has_default: bool,
    tuple_items: Option<Vec<ParamSpec>>,
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => write!(formatter, "int"),
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

#[derive(Clone)]
struct Symbol {
    value_type: Type,
    mutable: bool,
}

#[derive(Clone)]
struct EnumInfo {
    variants: Vec<String>,
    open: bool,
    persistable: bool,
}

#[derive(Clone)]
struct StructInfo {
    kind: AggregateKind,
    base: Option<String>,
    interfaces: Vec<String>,
    unique: bool,
    abstract_class: bool,
    epic_internal_class: bool,
    final_class: bool,
    concrete: bool,
    castable: bool,
    persistable: bool,
    computes: bool,
    fields: Vec<StructFieldInfo>,
    methods: Vec<ClassMethodInfo>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AggregateKind {
    Struct,
    Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParametricTypeKind {
    Struct,
    Class,
    Interface,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessLevel {
    Public,
    Internal,
    Protected,
    Private,
}

#[derive(Clone)]
struct StructFieldInfo {
    name: String,
    value_type: Type,
    has_default: bool,
    mutable: bool,
    final_member: bool,
    access: AccessLevel,
    mutation_access: AccessLevel,
    owner: Option<String>,
    span: Span,
}

#[derive(Clone)]
struct ClassMethodInfo {
    qualifier: Option<String>,
    name: String,
    value_type: Type,
    final_member: bool,
    abstract_member: bool,
    access: AccessLevel,
    owner: Option<String>,
    span: Span,
}

#[derive(Clone)]
struct InterfaceInfo {
    parents: Vec<String>,
    fields: Vec<StructFieldInfo>,
    methods: Vec<ClassMethodInfo>,
}

#[derive(Clone)]
struct ExtensionMethodInfo {
    receiver_type: Type,
    method_type: Type,
    module_name: Option<String>,
    access: AccessLevel,
    span: Span,
}

#[derive(Clone)]
struct TypeAliasInfo {
    target: TypeAnnotation,
    span: Span,
    module_path: Vec<String>,
}

#[derive(Clone)]
struct ParametricTypeInfo {
    params: Vec<TypeParam>,
    expr: Expr,
    kind: ParametricTypeKind,
    module_path: Vec<String>,
    span: Span,
}

#[derive(Clone)]
struct ModuleInfo {
    members: HashMap<String, Type>,
    member_access: HashMap<String, AccessLevel>,
    imports: Vec<String>,
}

#[derive(Clone)]
struct PlayerWeakMapInfo {
    value_type: Type,
}

#[derive(Clone)]
struct DataMemberDefaultContext {
    aggregate_name: String,
    field_name: String,
}

type ClassMemberInfosResult = (
    Vec<StructFieldInfo>,
    Vec<ClassMethodInfo>,
    bool,
    bool,
    Option<String>,
    Vec<String>,
);

struct ClassDefinitionParts<'a> {
    specifiers: &'a [String],
    base: Option<&'a TypeAnnotation>,
    interfaces: &'a [TypeAnnotation],
    fields: &'a [StructField],
    methods: &'a [ClassMethod],
    extension_methods: &'a [ExtensionMethod],
    blocks: &'a [ClassBlock],
}

struct AsyncExprMarker {
    function_depth: usize,
    seen: bool,
}

pub struct Checker {
    scopes: Vec<HashMap<String, Symbol>>,
    scope_imports: Vec<Vec<String>>,
    enum_types: HashMap<String, EnumInfo>,
    struct_types: HashMap<String, StructInfo>,
    interface_types: HashMap<String, InterfaceInfo>,
    module_types: HashMap<String, ModuleInfo>,
    extension_methods: HashMap<String, Vec<ExtensionMethodInfo>>,
    parametric_types: HashMap<String, ParametricTypeInfo>,
    predeclared_aggregate_values: HashSet<String>,
    type_alias_defs: HashMap<String, TypeAliasInfo>,
    type_aliases: HashMap<String, Type>,
    type_param_scopes: Vec<HashMap<String, Type>>,
    player_weak_maps: Vec<PlayerWeakMapInfo>,
    module_path: Vec<String>,
    module_scope_depths: Vec<usize>,
    break_depth: usize,
    function_returns: Vec<Type>,
    function_effects: Vec<Vec<String>>,
    failure_context_depth: usize,
    range_context_depth: usize,
    defer_depth: usize,
    data_member_default_depth: usize,
    data_member_default_stack: Vec<DataMemberDefaultContext>,
    async_expr_markers: Vec<AsyncExprMarker>,
    suppressed_async_expr_markers: usize,
    class_context: Vec<String>,
    class_member_shadow_names: Vec<HashSet<String>>,
}

impl Checker {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert(
            "print".to_string(),
            Symbol::immutable(Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::None),
            }),
        );
        globals.insert(
            "Print".to_string(),
            Symbol::immutable(Type::Overload(vec![
                print_function_type(char_array_type()),
                print_function_type(Type::Message),
                print_function_type(diagnostic_type()),
            ])),
        );
        globals.insert(
            "color".to_string(),
            Symbol::immutable(Type::StructType("color".to_string())),
        );
        globals.insert(
            "color_alpha".to_string(),
            Symbol::immutable(Type::StructType("color_alpha".to_string())),
        );
        globals.insert(
            "locale".to_string(),
            Symbol::immutable(Type::StructType("locale".to_string())),
        );
        globals.insert(
            "session_environment".to_string(),
            Symbol::immutable(Type::EnumType("session_environment".to_string())),
        );
        globals.insert(
            "NamedColors".to_string(),
            Symbol::immutable(Type::Module("NamedColors".to_string())),
        );
        for name in BUILTIN_INTERFACE_NAMES {
            globals.insert(
                (*name).to_string(),
                Symbol::immutable(Type::InterfaceType((*name).to_string())),
            );
        }
        globals.insert(
            "assert_eq".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("expected", Type::Any), ("actual", Type::Any)],
                Type::None,
            )),
        );
        globals.insert(
            "str".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("value", Type::Any)],
                Type::String,
            )),
        );
        globals.insert(
            "Err".to_string(),
            Symbol::immutable(native_function_type(
                &["computes"],
                vec![("Message", Type::String)],
                Type::Never,
            )),
        );
        globals.insert(
            "ToDiagnostic".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Value", Type::Any)],
                Type::Class("diagnostic".to_string()),
            )),
        );
        globals.insert(
            "GetSecondsSinceEpoch".to_string(),
            Symbol::immutable(native_function_type(&[], Vec::new(), Type::Float)),
        );
        globals.insert(
            "MakeColorFromSRGB".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Red", Type::Float),
                    ("Green", Type::Float),
                    ("Blue", Type::Float),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeColorFromSRGBValues".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Red", Type::Int),
                    ("Green", Type::Int),
                    ("Blue", Type::Int),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeSRGBFromColor".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Color", color_type())],
                Type::Tuple(vec![Type::Float, Type::Float, Type::Float]),
            )),
        );
        globals.insert(
            "MakeColorFromHex".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("hexString", char_array_type())],
                color_type(),
            )),
        );
        globals.insert(
            "MakeColorFromHSV".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Hue", Type::Float),
                    ("Saturation", Type::Float),
                    ("Value", Type::Float),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeHSVFromColor".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Color", color_type())],
                Type::Tuple(vec![Type::Float, Type::Float, Type::Float]),
            )),
        );
        globals.insert(
            "MakeColorAlpha".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("R", Type::Float),
                    ("G", Type::Float),
                    ("B", Type::Float),
                    ("A", Type::Float),
                ],
                color_alpha_type(),
            )),
        );
        globals.insert(
            "Over".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("CA1", color_alpha_type()), ("CA2", color_alpha_type())],
                color_alpha_type(),
            )),
        );
        globals.insert(
            "ToString".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&[], vec![("Val", Type::Int)], char_array_type()),
                native_function_type(&[], vec![("Val", Type::Float)], char_array_type()),
                native_function_type(
                    &["computes"],
                    vec![("String", char_array_type())],
                    char_array_type(),
                ),
                native_function_type(
                    &["computes"],
                    vec![("Character", Type::Char)],
                    char_array_type(),
                ),
                native_function_type(
                    &["computes"],
                    vec![("Character", Type::Char32)],
                    char_array_type(),
                ),
            ])),
        );
        globals.insert(
            "Localize".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Message", Type::Message)],
                Type::String,
            )),
        );
        globals.insert(
            "Join".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &[],
                    vec![
                        ("Strings", Type::Array(Box::new(Type::String))),
                        ("Separator", Type::String),
                    ],
                    Type::String,
                ),
                native_function_type(
                    &["transacts"],
                    vec![
                        ("Messages", Type::Array(Box::new(Type::Message))),
                        ("Separator", Type::Message),
                    ],
                    Type::Message,
                ),
            ])),
        );
        globals.insert(
            "GetRandomFloat".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Low", Type::Float), ("High", Type::Float)],
                Type::Float,
            )),
        );
        globals.insert(
            "GetRandomInt".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Low", Type::Int), ("High", Type::Int)],
                Type::Int,
            )),
        );
        globals.insert(
            "Shuffle".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Input", Type::Array(Box::new(Type::Unknown)))],
                Type::Array(Box::new(Type::Unknown)),
            )),
        );
        globals.insert("Inf".to_string(), Symbol::immutable(Type::Float));
        globals.insert("NaN".to_string(), Symbol::immutable(Type::Float));
        globals.insert("PiFloat".to_string(), Symbol::immutable(Type::Float));
        globals.insert(
            "Concatenate".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![(
                    "Arrays",
                    Type::Array(Box::new(Type::Array(Box::new(Type::Unknown)))),
                )],
                Type::Array(Box::new(Type::Unknown)),
            )),
        );
        globals.insert(
            "ConcatenateMaps".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                ]),
                param_specs: None,
                return_type: Box::new(Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown))),
            }),
        );
        globals.insert(
            "MakeClassifiableSubset".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: vec!["transacts".to_string()],
                param_types: Some(vec![Type::Array(Box::new(Type::Unknown))]),
                param_specs: None,
                return_type: Box::new(Type::ClassifiableSubset(Box::new(Type::Unknown))),
            }),
        );
        globals.insert(
            "GetSession".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(0),
                arity_range: None,
                effects: vec!["transacts".to_string()],
                param_types: Some(Vec::new()),
                param_specs: None,
                return_type: Box::new(Type::Class("session".to_string())),
            }),
        );
        globals.insert(
            "GetSimulationElapsedTime".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                Vec::new(),
                Type::Float,
            )),
        );
        globals.insert(
            "Sleep".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts", "suspends", "no_rollback"],
                vec![("Seconds", Type::Float)],
                Type::None,
            )),
        );
        globals.insert(
            "MakeSuccess".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Result(Box::new(Type::Any), Box::new(Type::Never))),
            }),
        );
        globals.insert(
            "MakeError".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Result(Box::new(Type::Never), Box::new(Type::Any))),
            }),
        );
        globals.insert(
            "FitsInPlayerMap".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: vec!["reads".to_string(), "decides".to_string()],
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Any),
            }),
        );
        globals.insert(
            "Mod".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: vec!["decides".to_string(), "computes".to_string()],
                param_types: Some(vec![Type::Int, Type::Int]),
                param_specs: None,
                return_type: Box::new(Type::Int),
            }),
        );
        globals.insert(
            "Quotient".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: vec!["decides".to_string(), "computes".to_string()],
                param_types: Some(vec![Type::Int, Type::Int]),
                param_specs: None,
                return_type: Box::new(Type::Int),
            }),
        );
        globals.insert(
            "Clamp".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &[],
                    vec![("Value", Type::Int), ("A", Type::Int), ("B", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &[],
                    vec![
                        ("Value", Type::Float),
                        ("A", Type::Float),
                        ("B", Type::Float),
                    ],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Lerp".to_string(),
            Symbol::immutable(native_function_type(
                &["computes"],
                vec![
                    ("From", Type::Float),
                    ("To", Type::Float),
                    ("Parameter", Type::Float),
                ],
                Type::Float,
            )),
        );
        globals.insert(
            "Abs".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&[], vec![("Value", Type::Int)], Type::Int),
                native_function_type(&[], vec![("Value", Type::Float)], Type::Float),
            ])),
        );
        globals.insert(
            "Min".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Int), ("Y", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Float), ("Y", Type::Float)],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Max".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Int), ("Y", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Float), ("Y", Type::Float)],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Ceil".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Value", Type::Rational)], Type::Int),
                native_function_type(
                    &["reads", "computes", "decides"],
                    vec![("Val", Type::Float)],
                    Type::Int,
                ),
            ])),
        );
        globals.insert(
            "Floor".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Value", Type::Rational)], Type::Int),
                native_function_type(
                    &["reads", "computes", "decides"],
                    vec![("Val", Type::Float)],
                    Type::Int,
                ),
            ])),
        );
        globals.insert(
            "Round".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![("Val", Type::Float)],
                Type::Int,
            )),
        );
        globals.insert(
            "Int".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![("Val", Type::Float)],
                Type::Int,
            )),
        );
        for name in [
            "Sqrt", "Sin", "Cos", "Tan", "ArcSin", "ArcCos", "Sinh", "Cosh", "Tanh", "ArSinh",
            "ArCosh", "ArTanh", "Exp", "Ln",
        ] {
            globals.insert(
                name.to_string(),
                Symbol::immutable(native_function_type(
                    &["computes"],
                    vec![("X", Type::Float)],
                    Type::Float,
                )),
            );
        }
        globals.insert(
            "Sgn".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Val", Type::Int)], Type::Int),
                native_function_type(&["computes"], vec![("Val", Type::Float)], Type::Float),
            ])),
        );
        for (name, params) in [
            ("Pow", vec![("A", Type::Float), ("B", Type::Float)]),
            ("Log", vec![("B", Type::Float), ("X", Type::Float)]),
        ] {
            globals.insert(
                name.to_string(),
                Symbol::immutable(native_function_type(&["computes"], params, Type::Float)),
            );
        }
        globals.insert(
            "ArcTan".to_string(),
            Symbol::immutable(Type::Function {
                arity: None,
                arity_range: Some((1, 2)),
                effects: vec!["computes".to_string()],
                param_types: Some(vec![Type::Float]),
                param_specs: None,
                return_type: Box::new(Type::Float),
            }),
        );
        globals.insert(
            "IsAlmostEqual".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![
                    ("Val1", Type::Number),
                    ("Val2", Type::Number),
                    ("AbsoluteTolerance", Type::Number),
                ],
                Type::None,
            )),
        );

        let mut struct_types = HashMap::new();
        struct_types.insert("color".to_string(), builtin_color_info());
        struct_types.insert("color_alpha".to_string(), builtin_color_alpha_info());
        struct_types.insert("locale".to_string(), builtin_locale_info());
        let interface_types = builtin_interface_infos();

        let mut module_types = HashMap::new();
        let mut named_color_members = HashMap::new();
        let mut named_color_access = HashMap::new();
        for color in NAMED_COLORS {
            named_color_members.insert(color.name.to_string(), color_type());
            named_color_access.insert(color.name.to_string(), AccessLevel::Public);
        }
        module_types.insert(
            "NamedColors".to_string(),
            ModuleInfo {
                members: named_color_members,
                member_access: named_color_access,
                imports: Vec::new(),
            },
        );

        let mut enum_types = HashMap::new();
        enum_types.insert(
            "session_environment".to_string(),
            builtin_session_environment_info(),
        );

        Self {
            scopes: vec![globals],
            scope_imports: vec![Vec::new()],
            enum_types,
            struct_types,
            interface_types,
            module_types,
            extension_methods: HashMap::new(),
            parametric_types: HashMap::new(),
            predeclared_aggregate_values: HashSet::new(),
            type_alias_defs: HashMap::new(),
            type_aliases: HashMap::new(),
            type_param_scopes: Vec::new(),
            player_weak_maps: Vec::new(),
            module_path: Vec::new(),
            module_scope_depths: Vec::new(),
            break_depth: 0,
            function_returns: Vec::new(),
            function_effects: Vec::new(),
            failure_context_depth: 0,
            range_context_depth: 0,
            defer_depth: 0,
            data_member_default_depth: 0,
            data_member_default_stack: Vec::new(),
            async_expr_markers: Vec::new(),
            suppressed_async_expr_markers: 0,
            class_context: Vec::new(),
            class_member_shadow_names: Vec::new(),
        }
    }

    pub fn check_program(mut self, program: &Program) -> Result<Type, VerseError> {
        self.predeclare_top_level_modules(program);
        self.predeclare_top_level_module_member_access(program)?;
        self.predeclare_top_level_enums(program);
        self.predeclare_top_level_aggregate_names(program);
        self.predeclare_top_level_aggregate_values(program)?;
        self.predeclare_top_level_parametric_types(program)?;
        self.predeclare_using_imports_recursive(&program.statements)?;
        self.predeclare_top_level_type_aliases(program)?;
        self.predeclare_extension_methods_in_current_scope(&program.statements)?;
        self.predeclare_top_level_functions(program)?;
        self.define_top_level_interface_members(program)?;
        self.define_top_level_aggregate_members(program)?;
        self.validate_function_overloads_in_current_scope()?;
        self.check_statements(&program.statements)
    }

    fn predeclare_top_level_module_member_access(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_module_member_access(&program.statements)
    }

    fn predeclare_module_member_access(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    specifiers,
                    expr,
                    ..
                } => match &expr.kind {
                    ExprKind::ModuleDefinition {
                        statements: module_statements,
                        ..
                    } => {
                        self.record_current_module_member_access(name, specifiers, statement.span)?;
                        self.module_path.push(name.clone());
                        self.predeclare_module_member_access(module_statements)?;
                        self.module_path.pop();
                    }
                    ExprKind::EnumDefinition { .. }
                    | ExprKind::StructDefinition { .. }
                    | ExprKind::ClassDefinition { .. }
                    | ExprKind::InterfaceDefinition { .. } => {
                        self.record_current_module_member_access(
                            name,
                            module_member_specifiers(specifiers, expr),
                            statement.span,
                        )?;
                    }
                    _ => {}
                },
                StmtKind::ParametricType {
                    name, specifiers, ..
                } => self.record_current_module_member_access(name, specifiers, statement.span)?,
                StmtKind::TypeAlias { name, .. } => {
                    self.record_current_module_member_access(name, &[], statement.span)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn predeclare_top_level_modules(&mut self, program: &Program) {
        self.predeclare_modules(&program.statements);
    }

    fn predeclare_modules(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::ModuleDefinition { statements, .. } = &expr.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            self.module_types.entry(qualified).or_insert(ModuleInfo {
                members: HashMap::new(),
                member_access: HashMap::new(),
                imports: Vec::new(),
            });

            self.module_path.push(name.clone());
            self.predeclare_modules(statements);
            self.module_path.pop();
        }
    }

    fn predeclare_top_level_enums(&mut self, program: &Program) {
        self.predeclare_enums(&program.statements);
    }

    fn predeclare_enums(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            match &expr.kind {
                ExprKind::EnumDefinition {
                    open,
                    persistable,
                    variants,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    self.enum_types.entry(qualified).or_insert(EnumInfo {
                        variants: enum_variant_names(variants),
                        open: *open,
                        persistable: *persistable,
                    });
                }
                ExprKind::ModuleDefinition { statements, .. } => {
                    self.module_path.push(name.clone());
                    self.predeclare_enums(statements);
                    self.module_path.pop();
                }
                _ => {}
            }
        }
    }

    fn predeclare_top_level_aggregate_names(&mut self, program: &Program) {
        self.predeclare_aggregate_names(&program.statements);
    }

    fn predeclare_aggregate_names(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            if matches!(expr.kind, ExprKind::InterfaceDefinition { .. }) {
                let qualified = self.current_qualified_name(name);
                self.interface_types
                    .entry(qualified)
                    .or_insert(InterfaceInfo {
                        parents: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    });
                continue;
            }

            let (kind, persistable, computes) = match &expr.kind {
                ExprKind::StructDefinition {
                    persistable,
                    computes,
                    ..
                } => (AggregateKind::Struct, *persistable, *computes),
                ExprKind::ClassDefinition { specifiers, .. } => (
                    AggregateKind::Class,
                    class_has_specifier(specifiers, "persistable"),
                    false,
                ),
                ExprKind::ModuleDefinition { statements, .. } => {
                    self.module_path.push(name.clone());
                    self.predeclare_aggregate_names(statements);
                    self.module_path.pop();
                    continue;
                }
                _ => continue,
            };
            let qualified = self.current_qualified_name(name);
            self.struct_types.entry(qualified).or_insert(StructInfo {
                kind,
                base: None,
                interfaces: Vec::new(),
                unique: false,
                abstract_class: false,
                epic_internal_class: false,
                final_class: false,
                concrete: false,
                castable: false,
                persistable,
                computes,
                fields: Vec::new(),
                methods: Vec::new(),
            });
        }
    }

    fn predeclare_top_level_aggregate_values(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_aggregate_values_in_current_scope(&program.statements)
    }

    fn predeclare_aggregate_values_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            let value_type = match &expr.kind {
                ExprKind::StructDefinition { .. } => Type::StructType(qualified.clone()),
                ExprKind::ClassDefinition { .. } => Type::ClassType(qualified.clone()),
                ExprKind::InterfaceDefinition { .. } => Type::InterfaceType(qualified.clone()),
                _ => continue,
            };

            self.define_predeclared_aggregate_value(name, &qualified, value_type, statement.span)?;
        }

        Ok(())
    }

    fn predeclare_top_level_parametric_types(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_parametric_types(&program.statements)
    }

    fn predeclare_parametric_types(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let { name, expr, .. } => {
                    if let ExprKind::ModuleDefinition {
                        statements: module_statements,
                        ..
                    } = &expr.kind
                    {
                        self.module_path.push(name.clone());
                        self.predeclare_parametric_types(module_statements)?;
                        self.module_path.pop();
                    }
                }
                StmtKind::ParametricType {
                    name,
                    specifiers: _,
                    params,
                    expr,
                } => {
                    let kind = parametric_type_kind(expr).ok_or_else(|| {
                        VerseError::check_at(
                            "parametric type definitions must define a class, struct, or interface",
                            statement.span,
                        )
                    })?;
                    let qualified = self.current_qualified_name(name);
                    if matches!(
                        &expr.kind,
                        ExprKind::ClassDefinition { specifiers, .. }
                            if class_has_specifier(specifiers, "persistable")
                    ) {
                        return Err(VerseError::check_at(
                            format!("persistable class `{qualified}` cannot be parametric"),
                            statement.span,
                        ));
                    }
                    if matches!(
                        &expr.kind,
                        ExprKind::StructDefinition {
                            persistable: true,
                            ..
                        }
                    ) {
                        return Err(VerseError::check_at(
                            format!("persistable struct `{qualified}` cannot be parametric"),
                            statement.span,
                        ));
                    }
                    self.validate_type_parameter_names(params, statement.span)?;
                    if self.parametric_types.contains_key(&qualified) {
                        return Err(VerseError::check_at(
                            format!("duplicate parametric type `{name}`"),
                            statement.span,
                        ));
                    }
                    if self.enum_types.contains_key(&qualified)
                        || self.struct_types.contains_key(&qualified)
                        || self.interface_types.contains_key(&qualified)
                        || self.type_alias_defs.contains_key(&qualified)
                    {
                        return Err(VerseError::check_at(
                            format!(
                                "parametric type `{name}` conflicts with existing type `{name}`"
                            ),
                            statement.span,
                        ));
                    }
                    self.parametric_types.insert(
                        qualified,
                        ParametricTypeInfo {
                            params: params.clone(),
                            expr: expr.clone(),
                            kind,
                            module_path: self.module_path.clone(),
                            span: statement.span,
                        },
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn predeclare_top_level_type_aliases(&mut self, program: &Program) -> Result<(), VerseError> {
        self.predeclare_type_aliases(&program.statements)?;

        let names = self.type_alias_defs.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.resolve_type_alias(&name, &mut Vec::new())?;
        }

        Ok(())
    }

    fn predeclare_type_aliases(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            if let StmtKind::Let { name, expr, .. } = &statement.kind
                && let ExprKind::ModuleDefinition {
                    statements: module_statements,
                    ..
                } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.predeclare_type_aliases(module_statements)?;
                self.module_path.pop();
                continue;
            }

            let StmtKind::TypeAlias { name, target } = &statement.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            self.validate_type_alias_name(name, &qualified, statement.span)?;
            if self.type_alias_defs.contains_key(&qualified) {
                return Err(VerseError::check_at(
                    format!("duplicate type alias `{name}`"),
                    statement.span,
                ));
            }

            self.type_alias_defs.insert(
                qualified,
                TypeAliasInfo {
                    target: target.clone(),
                    span: statement.span,
                    module_path: self.module_path.clone(),
                },
            );
        }

        Ok(())
    }

    fn validate_type_alias_name(
        &self,
        name: &str,
        qualified: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if is_reserved_type_alias_name(name) {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with builtin type name"),
                span,
            ));
        }

        if self.enum_types.contains_key(qualified) || self.struct_types.contains_key(qualified) {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with existing type `{name}`"),
                span,
            ));
        }

        if self.module_types.contains_key(qualified)
            || (self.module_path.is_empty() && self.scopes[0].contains_key(name))
        {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with existing value `{name}`"),
                span,
            ));
        }

        Ok(())
    }

    fn resolve_type_alias(
        &mut self,
        name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<Type, VerseError> {
        if let Some(value_type) = self.type_aliases.get(name) {
            return Ok(value_type.clone());
        }

        let Some(info) = self.type_alias_defs.get(name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown type `{name}`"),
                Span::new(0, 0, 1, 1),
            ));
        };

        if visiting.iter().any(|item| item == name) {
            let mut cycle = visiting.clone();
            cycle.push(name.to_string());
            return Err(VerseError::check_at(
                format!("cyclic type alias `{}`", cycle.join(" -> ")),
                info.span,
            ));
        }

        visiting.push(name.to_string());
        let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
        let value_type =
            self.resolve_type_alias_target(&info.target.name, info.target.span, visiting);
        self.module_path = previous_module_path;
        visiting.pop();
        let value_type = value_type?;

        self.type_aliases
            .insert(name.to_string(), value_type.clone());
        Ok(value_type)
    }

    fn resolve_type_alias_target(
        &mut self,
        name: &TypeName,
        span: Span,
        visiting: &mut Vec<String>,
    ) -> Result<Type, VerseError> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.resolve_type_alias_target(item, span, visiting)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => {
                let key_type = self.resolve_type_alias_target(key, span, visiting)?;
                let value_type = self.resolve_type_alias_target(value, span, visiting)?;
                ensure_comparable_key(&key_type, &self.struct_types, span)?;
                Type::Map(Box::new(key_type), Box::new(value_type))
            }
            TypeName::WeakMap(key, value) => {
                let key_type = self.resolve_type_alias_target(key, span, visiting)?;
                let value_type = self.resolve_type_alias_target(value, span, visiting)?;
                validate_weak_map_type(
                    &key_type,
                    &value_type,
                    span,
                    &self.enum_types,
                    &self.struct_types,
                )?;
                Type::WeakMap(Box::new(key_type), Box::new(value_type))
            }
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.resolve_type_alias_target(item, span, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeName::Option(item) => Type::Option(Box::new(
                self.resolve_type_alias_target(item, span, visiting)?,
            )),
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.resolve_type_alias_target(param, span, visiting))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.resolve_type_alias_target(
                    return_type,
                    span,
                    visiting,
                )?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.resolve_type_alias_target(arg, span, visiting))
                    .collect::<Result<Vec<_>, _>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(alias_name) = self.resolve_type_alias_reference(name, span)? {
                    self.resolve_type_alias(&alias_name, visiting)?
                } else {
                    self.named_type_to_type(name, span)?
                }
            }
        };
        Ok(value_type)
    }

    fn resolve_type_alias_reference(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Option<String>, VerseError> {
        if self.type_alias_defs.contains_key(name) {
            self.ensure_qualified_type_alias_accessible(name, span)?;
            return Ok(Some(name.to_string()));
        }

        if !name.contains('.') {
            if let Some(qualified) = self
                .resolve_contextual_type_name(name)
                .filter(|qualified| self.type_alias_defs.contains_key(qualified))
            {
                self.ensure_qualified_type_alias_accessible(&qualified, span)?;
                Ok(Some(qualified))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn named_type_to_type(&self, name: &str, span: Span) -> Result<Type, VerseError> {
        if self.enum_types.contains_key(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            return Ok(Type::Enum(name.to_string()));
        }
        if self.interface_types.contains_key(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            return Ok(Type::Interface(name.to_string()));
        }
        if let Some(info) = self.struct_types.get(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            match info.kind {
                AggregateKind::Struct => return Ok(Type::Struct(name.to_string())),
                AggregateKind::Class => return Ok(Type::Class(name.to_string())),
            }
        }

        if !name.contains('.')
            && let Some(qualified) = self.resolve_contextual_type_name(name)
        {
            if self.enum_types.contains_key(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::Enum(qualified));
            }
            if self.interface_types.contains_key(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::Interface(qualified));
            }
            if let Some(info) = self.struct_types.get(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(match info.kind {
                    AggregateKind::Struct => Type::Struct(qualified),
                    AggregateKind::Class => Type::Class(qualified),
                });
            }
        }

        if is_builtin_class_type_name(name) {
            return Ok(Type::Class(name.to_string()));
        }

        Err(VerseError::check_at(format!("unknown type `{name}`"), span))
    }

    fn ensure_qualified_type_name_accessible(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some((module_name, member_name)) = name.rsplit_once('.') else {
            return Ok(());
        };
        let Some(module_info) = self.module_types.get(module_name) else {
            return Ok(());
        };
        let access = module_info
            .member_access
            .get(member_name)
            .copied()
            .unwrap_or(AccessLevel::Internal);
        self.ensure_module_member_accessible(module_name, access, member_name, span)
    }

    fn ensure_qualified_type_alias_accessible(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some((module_name, member_name)) = name.rsplit_once('.') else {
            return Ok(());
        };
        let Some(module_info) = self.module_types.get(module_name) else {
            return Ok(());
        };
        let access = module_info
            .member_access
            .get(member_name)
            .copied()
            .unwrap_or(AccessLevel::Internal);
        self.ensure_module_member_accessible(module_name, access, member_name, span)
    }

    fn current_qualified_name(&self, name: &str) -> String {
        if self.module_path.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.module_path.join("."), name)
        }
    }

    fn current_definition_level(&self) -> bool {
        self.scopes.len() == 1
            || self
                .module_scope_depths
                .last()
                .is_some_and(|depth| self.scopes.len() == *depth)
    }

    fn current_module_name(&self) -> Option<String> {
        (!self.module_path.is_empty()).then(|| self.module_path.join("."))
    }

    fn resolve_contextual_type_name(&self, name: &str) -> Option<String> {
        if !self.module_path.is_empty() {
            let qualified = self.current_qualified_name(name);
            if self.enum_types.contains_key(&qualified)
                || self.interface_types.contains_key(&qualified)
                || self.struct_types.contains_key(&qualified)
                || self.type_aliases.contains_key(&qualified)
                || self.type_alias_defs.contains_key(&qualified)
                || self.parametric_types.contains_key(&qualified)
            {
                return Some(qualified);
            }
        }

        if let Some(module_name) = self.current_module_name()
            && let Some(module_info) = self.module_types.get(&module_name)
            && let Some(qualified) = module_info.imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                (self.enum_types.contains_key(&qualified)
                    || self.interface_types.contains_key(&qualified)
                    || self.struct_types.contains_key(&qualified)
                    || self.type_aliases.contains_key(&qualified)
                    || self.type_alias_defs.contains_key(&qualified)
                    || self.parametric_types.contains_key(&qualified))
                .then_some(qualified)
            })
        {
            return Some(qualified);
        }

        self.scope_imports.iter().rev().find_map(|imports| {
            imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                (self.enum_types.contains_key(&qualified)
                    || self.interface_types.contains_key(&qualified)
                    || self.struct_types.contains_key(&qualified)
                    || self.type_aliases.contains_key(&qualified)
                    || self.type_alias_defs.contains_key(&qualified)
                    || self.parametric_types.contains_key(&qualified))
                .then_some(qualified)
            })
        })
    }

    fn resolve_parametric_type_reference(&self, name: &str) -> Option<String> {
        if self.parametric_types.contains_key(name) {
            return Some(name.to_string());
        }
        if !name.contains('.') {
            self.resolve_contextual_type_name(name)
                .filter(|qualified| self.parametric_types.contains_key(qualified))
        } else {
            None
        }
    }

    fn resolve_type_param(&self, name: &str) -> Option<Type> {
        if name.contains('.') {
            return None;
        }
        self.type_param_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn push_type_param_scope(&mut self, params: impl IntoIterator<Item = (String, Type)>) {
        self.type_param_scopes.push(params.into_iter().collect());
    }

    fn pop_type_param_scope(&mut self) {
        self.type_param_scopes.pop();
    }

    fn resolve_module_path(&self, path: &str) -> Option<String> {
        if self.module_types.contains_key(path) {
            return Some(path.to_string());
        }

        if !path.contains('.') && !self.module_path.is_empty() {
            let qualified = self.current_qualified_name(path);
            if self.module_types.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        self.scope_imports.iter().rev().find_map(|imports| {
            imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{path}");
                self.module_types
                    .contains_key(&qualified)
                    .then_some(qualified)
            })
        })
    }

    fn current_scope_imports_mut(&mut self) -> &mut Vec<String> {
        self.scope_imports
            .last_mut()
            .expect("checker should always have import scope")
    }

    fn add_current_import(&mut self, module_name: String) {
        let imports = self.current_scope_imports_mut();
        if !imports.iter().any(|import| import == &module_name) {
            imports.push(module_name.clone());
        }
        if let Some(current_module) = self.current_module_name()
            && let Some(info) = self.module_types.get_mut(&current_module)
            && !info.imports.iter().any(|import| import == &module_name)
        {
            info.imports.push(module_name);
        }
    }

    fn predeclare_using_imports(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Using { path } = &statement.kind else {
                continue;
            };
            if is_absolute_module_path(path) {
                continue;
            }
            let Some(module_name) = self.resolve_module_path(path) else {
                return Err(VerseError::check_at(
                    format!("unsupported module path `{path}`"),
                    statement.span,
                ));
            };
            self.add_current_import(module_name);
        }
        Ok(())
    }

    fn predeclare_using_imports_recursive(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        self.predeclare_using_imports(statements)?;

        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            else {
                continue;
            };

            self.module_path.push(name.clone());
            self.push_scope();
            let result = self.predeclare_using_imports_recursive(module_statements);
            self.pop_scope();
            self.module_path.pop();
            result?;
        }

        Ok(())
    }

    fn annotation_to_type(
        &mut self,
        annotation: Option<&TypeAnnotation>,
    ) -> Result<Type, VerseError> {
        annotation
            .map(|annotation| self.type_name_to_type(annotation))
            .unwrap_or(Ok(Type::Unknown))
    }

    fn type_name_to_type(&mut self, annotation: &TypeAnnotation) -> Result<Type, VerseError> {
        self.type_name_to_type_name(&annotation.name, annotation.span)
    }

    fn type_name_to_type_name(&mut self, name: &TypeName, span: Span) -> Result<Type, VerseError> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.type_name_to_type_name(item, span)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => {
                let key_type = self.type_name_to_type_name(key, span)?;
                let value_type = self.type_name_to_type_name(value, span)?;
                ensure_comparable_key(&key_type, &self.struct_types, span)?;
                Type::Map(Box::new(key_type), Box::new(value_type))
            }
            TypeName::WeakMap(key, value) => {
                let key_type = self.type_name_to_type_name(key, span)?;
                let value_type = self.type_name_to_type_name(value, span)?;
                validate_weak_map_type(
                    &key_type,
                    &value_type,
                    span,
                    &self.enum_types,
                    &self.struct_types,
                )?;
                Type::WeakMap(Box::new(key_type), Box::new(value_type))
            }
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.type_name_to_type_name(item, span))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeName::Option(item) => {
                Type::Option(Box::new(self.type_name_to_type_name(item, span)?))
            }
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.type_name_to_type_name(param, span))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.type_name_to_type_name(return_type, span)?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.type_name_to_type_name(arg, span))
                    .collect::<Result<Vec<_>, _>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(value_type) = self.type_aliases.get(name).cloned() {
                    self.ensure_qualified_type_alias_accessible(name, span)?;
                    value_type
                } else if !name.contains('.')
                    && let Some(qualified) = self.resolve_contextual_type_name(name)
                    && let Some(value_type) = self.type_aliases.get(&qualified).cloned()
                {
                    self.ensure_qualified_type_alias_accessible(&qualified, span)?;
                    value_type
                } else {
                    self.named_type_to_type(name, span)?
                }
            }
        };
        Ok(value_type)
    }

    fn type_name_to_type_name_for_assignability(&self, name: &TypeName) -> Option<Type> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.type_name_to_type_name_for_assignability(item)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => Type::Map(
                Box::new(self.type_name_to_type_name_for_assignability(key)?),
                Box::new(self.type_name_to_type_name_for_assignability(value)?),
            ),
            TypeName::WeakMap(key, value) => Type::WeakMap(
                Box::new(self.type_name_to_type_name_for_assignability(key)?),
                Box::new(self.type_name_to_type_name_for_assignability(value)?),
            ),
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.type_name_to_type_name_for_assignability(item))
                    .collect::<Option<Vec<_>>>()?,
            ),
            TypeName::Option(item) => Type::Option(Box::new(
                self.type_name_to_type_name_for_assignability(item)?,
            )),
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.type_name_to_type_name_for_assignability(param))
                        .collect::<Option<Vec<_>>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.type_name_to_type_name_for_assignability(return_type)?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.type_name_to_type_name_for_assignability(arg))
                    .collect::<Option<Vec<_>>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, Span::new(0, 0, 0, 0)).ok()?
                } else {
                    let qualified = self.resolve_parametric_type_reference(name)?;
                    let info = self.parametric_types.get(&qualified)?;
                    let instance_name = render_parametric_instance_type_name(&qualified, &args);
                    match info.kind {
                        ParametricTypeKind::Struct
                            if self.struct_types.contains_key(&instance_name) =>
                        {
                            Type::Struct(instance_name)
                        }
                        ParametricTypeKind::Class
                            if self.struct_types.contains_key(&instance_name) =>
                        {
                            Type::Class(instance_name)
                        }
                        ParametricTypeKind::Interface
                            if self.interface_types.contains_key(&instance_name) =>
                        {
                            Type::Interface(instance_name)
                        }
                        _ => return None,
                    }
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(value_type) = self.type_aliases.get(name).cloned() {
                    value_type
                } else if !name.contains('.')
                    && let Some(qualified) = self.resolve_contextual_type_name(name)
                    && let Some(value_type) = self.type_aliases.get(&qualified).cloned()
                {
                    value_type
                } else {
                    self.named_type_to_type(name, Span::new(0, 0, 0, 0)).ok()?
                }
            }
        };
        Some(value_type)
    }

    fn constrained_type_param_supertype(
        &mut self,
        value_type: &Type,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        let Type::Param(_, TypeParamConstraint::Subtype(parent)) = value_type else {
            return Ok(None);
        };
        let supertype = self.type_name_to_type_name(parent, span)?;
        Ok((!matches!(supertype, Type::Param(_, _))).then_some(supertype))
    }

    fn constrained_type_param_supertype_for_assignability(
        &self,
        value_type: &Type,
    ) -> Option<Type> {
        let Type::Param(_, TypeParamConstraint::Subtype(parent)) = value_type else {
            return None;
        };
        let supertype = self.type_name_to_type_name_for_assignability(parent)?;
        (!matches!(supertype, Type::Param(_, _))).then_some(supertype)
    }

    fn param_types(&mut self, params: &[Param]) -> Result<Vec<Type>, VerseError> {
        params
            .iter()
            .map(|param| self.annotation_to_type(param.annotation.as_ref()))
            .collect()
    }

    fn param_specs(&mut self, params: &[Param]) -> Result<Vec<ParamSpec>, VerseError> {
        params.iter().map(|param| self.param_spec(param)).collect()
    }

    fn param_spec(&mut self, param: &Param) -> Result<ParamSpec, VerseError> {
        Ok(ParamSpec {
            name: param.name.clone(),
            value_type: self.annotation_to_type(param.annotation.as_ref())?,
            named: param.named,
            has_default: param.default.is_some(),
            tuple_items: match &param.pattern {
                ParamPattern::Tuple(params) => Some(self.param_specs(params)?),
                ParamPattern::Binding | ParamPattern::Anonymous => None,
            },
        })
    }

    fn instantiate_parametric_type(
        &mut self,
        name: &str,
        args: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let Some(qualified) = self.resolve_parametric_type_reference(name) else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        self.ensure_qualified_type_name_accessible(&qualified, span)?;
        let Some(info) = self.parametric_types.get(&qualified).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        if info.params.len() != args.len() {
            return Err(VerseError::check_at(
                format!(
                    "parametric type `{name}` expected {} type arguments, got {}",
                    info.params.len(),
                    args.len()
                ),
                span,
            ));
        }
        self.ensure_type_arguments_satisfy_constraints(&info.params, args, span)?;

        let instance_name = render_parametric_instance_type_name(&qualified, args);
        match info.kind {
            ParametricTypeKind::Struct if self.struct_types.contains_key(&instance_name) => {
                return Ok(Type::Struct(instance_name));
            }
            ParametricTypeKind::Class if self.struct_types.contains_key(&instance_name) => {
                return Ok(Type::Class(instance_name));
            }
            ParametricTypeKind::Interface if self.interface_types.contains_key(&instance_name) => {
                return Ok(Type::Interface(instance_name));
            }
            _ => {}
        }

        let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
        self.push_type_param_scope(
            info.params
                .iter()
                .zip(args)
                .map(|(param, arg)| (param.name.clone(), arg.clone())),
        );
        let result = (|| match &info.expr.kind {
            ExprKind::StructDefinition {
                fields,
                persistable,
                computes,
                ..
            } => {
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Struct,
                        base: None,
                        interfaces: Vec::new(),
                        unique: false,
                        abstract_class: false,
                        epic_internal_class: false,
                        final_class: false,
                        concrete: false,
                        castable: false,
                        persistable: *persistable,
                        computes: *computes,
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let fields = self.struct_field_infos_with_owner(fields, Some(&instance_name))?;
                if *persistable {
                    self.ensure_persistable_struct(&instance_name, &fields)?;
                }
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Struct,
                        base: None,
                        interfaces: Vec::new(),
                        unique: false,
                        abstract_class: false,
                        epic_internal_class: false,
                        final_class: false,
                        concrete: false,
                        castable: false,
                        persistable: *persistable,
                        computes: *computes,
                        fields,
                        methods: Vec::new(),
                    },
                );
                Ok(Type::Struct(instance_name.clone()))
            }
            ExprKind::ClassDefinition {
                base,
                interfaces,
                specifiers,
                fields,
                methods,
                extension_methods,
                blocks,
                ..
            } => {
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base: None,
                        interfaces: Vec::new(),
                        unique: class_has_specifier(specifiers, "unique"),
                        abstract_class: class_has_specifier(specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                        final_class: class_has_specifier(specifiers, "final"),
                        concrete: class_has_specifier(specifiers, "concrete"),
                        castable: class_has_specifier(specifiers, "castable"),
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let (fields, methods, unique, castable, base, implemented_interfaces) = self
                    .class_member_infos(
                        &instance_name,
                        ClassDefinitionParts {
                            specifiers,
                            base: base.as_ref(),
                            interfaces,
                            fields,
                            methods,
                            extension_methods,
                            blocks,
                        },
                    )?;
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base,
                        interfaces: implemented_interfaces,
                        unique,
                        abstract_class: class_has_specifier(specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                        final_class: class_has_specifier(specifiers, "final"),
                        concrete: class_has_specifier(specifiers, "concrete"),
                        castable,
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        fields,
                        methods,
                    },
                );
                Ok(Type::Class(instance_name.clone()))
            }
            ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } => {
                self.interface_types.insert(
                    instance_name.clone(),
                    InterfaceInfo {
                        parents: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let parent_names = self.interface_parent_names(parents)?;
                let inherited_fields = self.interface_field_requirements(&parent_names)?;
                let local_fields =
                    self.struct_field_infos_with_owner(fields, Some(&instance_name))?;
                let fields =
                    self.merge_interface_field_set(inherited_fields, local_fields, info.span)?;
                let inherited_methods = self.interface_method_requirements(&parent_names)?;
                let local_methods = self.interface_local_method_infos(&instance_name, methods)?;
                let method_infos =
                    self.merge_interface_method_set(inherited_methods, local_methods, info.span)?;
                self.interface_types.insert(
                    instance_name.clone(),
                    InterfaceInfo {
                        parents: parent_names,
                        fields: fields.clone(),
                        methods: method_infos,
                    },
                );
                self.check_interface_method_bodies(&instance_name, &fields, methods)?;
                Ok(Type::Interface(instance_name.clone()))
            }
            _ => Err(VerseError::check_at(
                "parametric type definitions must define a class, struct, or interface",
                info.span,
            )),
        })();
        self.pop_type_param_scope();
        self.module_path = previous_module_path;
        result
    }

    fn check_parametric_type_call_args(
        &mut self,
        name: &str,
        args: &[CallArg],
        span: Span,
    ) -> Result<Vec<Type>, VerseError> {
        let Some(qualified) = self.resolve_parametric_type_reference(name) else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        self.ensure_qualified_type_name_accessible(&qualified, span)?;
        let expected = self
            .parametric_types
            .get(&qualified)
            .map(|info| info.params.len())
            .unwrap_or(0);
        if args.len() != expected {
            return Err(VerseError::check_at(
                format!(
                    "parametric type `{name}` expected {expected} type arguments, got {}",
                    args.len()
                ),
                span,
            ));
        }
        args.iter()
            .map(|arg| {
                let CallArg::Positional(expr) = arg else {
                    return Err(VerseError::check_at(
                        "parametric type arguments do not accept named arguments",
                        call_arg_expr(arg).span,
                    ));
                };
                let type_name = self.expr_to_type_name(expr)?;
                self.type_name_to_type_name(&type_name, expr.span)
            })
            .collect()
    }

    fn ensure_type_arguments_satisfy_constraints(
        &mut self,
        params: &[TypeParam],
        args: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        for (param, arg) in params.iter().zip(args) {
            self.ensure_type_arg_satisfies_constraint(&param.name, &param.constraint, arg, span)?;
        }
        Ok(())
    }

    fn ensure_type_arg_satisfies_constraint(
        &mut self,
        param_name: &str,
        constraint: &TypeParamConstraint,
        actual: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        match constraint {
            TypeParamConstraint::Type => Ok(()),
            TypeParamConstraint::Subtype(expected_name) => {
                let expected = self.type_name_to_type_name(expected_name, span)?;
                if self.is_assignable(&expected, actual) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "type argument `{actual}` for `{param_name}` must be a subtype of `{expected}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    fn ensure_inferred_type_param_constraints(
        &mut self,
        param_types: Option<&[Type]>,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(param_types) = param_types else {
            return Ok(());
        };
        let mut checked = Vec::new();
        for param_type in param_types {
            self.ensure_inferred_type_param_constraints_inner(
                param_type,
                inferred,
                span,
                &mut checked,
            )?;
        }
        Ok(())
    }

    fn ensure_inferred_type_param_constraints_inner(
        &mut self,
        value_type: &Type,
        inferred: &HashMap<String, Type>,
        span: Span,
        checked: &mut Vec<String>,
    ) -> Result<(), VerseError> {
        match value_type {
            Type::Param(name, constraint) => {
                if !checked.iter().any(|checked_name| checked_name == name) {
                    if let Some(actual) = inferred.get(name) {
                        self.ensure_type_arg_satisfies_constraint(name, constraint, actual, span)?;
                    }
                    checked.push(name.clone());
                }
                Ok(())
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Task(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Signalable(item) => {
                self.ensure_inferred_type_param_constraints_inner(item, inferred, span, checked)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
                self.ensure_inferred_type_param_constraints_inner(key, inferred, span, checked)?;
                self.ensure_inferred_type_param_constraints_inner(value, inferred, span, checked)
            }
            Type::Tuple(items) => {
                for item in items {
                    self.ensure_inferred_type_param_constraints_inner(
                        item, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            Type::Event(payload)
            | Type::Generator(payload)
            | Type::Awaitable(payload)
            | Type::Subscribable(payload)
            | Type::Listenable(payload) => {
                if let Some(payload) = payload {
                    self.ensure_inferred_type_param_constraints_inner(
                        payload, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            Type::Function {
                param_types,
                param_specs,
                return_type,
                ..
            } => {
                if let Some(param_types) = param_types {
                    for param_type in param_types {
                        self.ensure_inferred_type_param_constraints_inner(
                            param_type, inferred, span, checked,
                        )?;
                    }
                }
                if let Some(param_specs) = param_specs {
                    for param in param_specs {
                        self.ensure_inferred_type_param_constraints_inner(
                            &param.value_type,
                            inferred,
                            span,
                            checked,
                        )?;
                    }
                }
                self.ensure_inferred_type_param_constraints_inner(
                    return_type,
                    inferred,
                    span,
                    checked,
                )
            }
            Type::Overload(overloads) => {
                for overload in overloads {
                    self.ensure_inferred_type_param_constraints_inner(
                        overload, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn substitute_type_params_runtime(
        &mut self,
        value_type: &Type,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<Type, VerseError> {
        let substituted = substitute_type_params(value_type, inferred);
        match substituted {
            Type::Struct(name) => Ok(Type::Struct(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::StructType(name) => Ok(Type::StructType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::Class(name) => Ok(Type::Class(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::ClassType(name) => Ok(Type::ClassType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::Interface(name) => Ok(Type::Interface(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::InterfaceType(name) => Ok(Type::InterfaceType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            other => Ok(other),
        }
    }

    fn substitute_parametric_instance_name(
        &mut self,
        name: &str,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<String, VerseError> {
        let candidates = self
            .parametric_types
            .iter()
            .map(|(qualified, info)| (qualified.clone(), info.clone()))
            .collect::<Vec<_>>();
        for (qualified, info) in candidates {
            let generic_args = info
                .params
                .iter()
                .map(|param| Type::Param(param.name.clone(), param.constraint.clone()))
                .collect::<Vec<_>>();
            let generic_name = render_parametric_instance_type_name(&qualified, &generic_args);
            if generic_name != name {
                continue;
            }
            let mut actual_args = Vec::with_capacity(info.params.len());
            for param in &info.params {
                let Some(actual) = inferred.get(&param.name).cloned() else {
                    return Ok(name.to_string());
                };
                actual_args.push(actual);
            }
            let actual_name = render_parametric_instance_type_name(&qualified, &actual_args);
            self.instantiate_parametric_type(&qualified, &actual_args, span)?;
            return Ok(actual_name);
        }
        Ok(replace_type_param_atoms(name, inferred))
    }

    fn define_top_level_interface_members(&mut self, program: &Program) -> Result<(), VerseError> {
        self.define_interface_members(&program.statements)
    }

    fn define_interface_members(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            if let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.define_interface_members(module_statements)?;
                self.module_path.pop();
                continue;
            }

            let ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } = &expr.kind
            else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            let parent_names = self.interface_parent_names(parents)?;
            let inherited_fields = self.interface_field_requirements(&parent_names)?;
            let local_fields = self.struct_field_infos_with_owner(fields, Some(&qualified))?;
            let fields =
                self.merge_interface_field_set(inherited_fields, local_fields, statement.span)?;
            let inherited_methods = self.interface_method_requirements(&parent_names)?;
            let local_methods = self.interface_local_method_infos(&qualified, methods)?;
            let method_infos =
                self.merge_interface_method_set(inherited_methods, local_methods, statement.span)?;
            self.interface_types.insert(
                qualified.clone(),
                InterfaceInfo {
                    parents: parent_names,
                    fields: fields.clone(),
                    methods: method_infos,
                },
            );
            self.check_interface_method_bodies(&qualified, &fields, methods)?;
        }

        Ok(())
    }

    fn interface_parent_names(
        &mut self,
        parents: &[TypeAnnotation],
    ) -> Result<Vec<String>, VerseError> {
        let mut names = Vec::with_capacity(parents.len());
        for parent in parents {
            let parent_type = self.type_name_to_type(parent)?;
            let Type::Interface(parent_name) = parent_type else {
                return Err(VerseError::check_at(
                    format!("interface parent must be an interface, got `{parent_type}`"),
                    parent.span,
                ));
            };
            names.push(parent_name);
        }
        Ok(dedupe_strings(names))
    }

    fn interface_field_requirements(
        &self,
        interfaces: &[String],
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        let mut fields = Vec::new();
        for interface in interfaces {
            let Some(info) = self.interface_types.get(interface) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface}`"),
                    Span::new(0, 0, 1, 1),
                ));
            };
            fields =
                self.merge_interface_field_set(fields, info.fields.clone(), Span::new(0, 0, 1, 1))?;
        }
        Ok(fields)
    }

    fn interface_method_requirements(
        &self,
        interfaces: &[String],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut methods = Vec::new();
        for interface in interfaces {
            let Some(info) = self.interface_types.get(interface) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface}`"),
                    Span::new(0, 0, 1, 1),
                ));
            };
            methods = self.merge_interface_method_set(
                methods,
                info.methods.clone(),
                Span::new(0, 0, 1, 1),
            )?;
        }
        Ok(methods)
    }

    fn interface_local_method_infos(
        &mut self,
        interface_name: &str,
        methods: &[ClassMethod],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        for method in methods {
            let info = ClassMethodInfo {
                qualifier: method
                    .qualifier
                    .clone()
                    .or_else(|| Some(interface_name.to_string())),
                name: method.name.clone(),
                value_type: self.class_method_declared_type(method)?,
                final_member: false,
                abstract_member: method.body.is_none(),
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                owner: Some(interface_name.to_string()),
                span: method.span,
            };
            push_distinct_local_method_info(&mut infos, info, "interface", &self.struct_types)?;
        }
        Ok(infos)
    }

    fn merge_interface_field_set(
        &self,
        mut base: Vec<StructFieldInfo>,
        fields: Vec<StructFieldInfo>,
        span: Span,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        for field in fields {
            if let Some(index) = base.iter().position(|existing| existing.name == field.name) {
                let existing = &base[index];
                if existing.final_member && existing.owner != field.owner {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, existing.name
                        ),
                        field.span,
                    ));
                }
                if existing.value_type != field.value_type || existing.mutable != field.mutable {
                    return Err(VerseError::check_at(
                        format!(
                            "interface field `{}` has incompatible inherited definitions",
                            field.name
                        ),
                        span,
                    ));
                }
                if !existing.has_default && field.has_default {
                    base[index] = field;
                }
                continue;
            }
            base.push(field);
        }
        Ok(base)
    }

    fn merge_interface_method_set(
        &self,
        mut base: Vec<ClassMethodInfo>,
        methods: Vec<ClassMethodInfo>,
        span: Span,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        for method in methods {
            if let Some(existing_index) = base.iter().position(|existing| {
                existing.name == method.name
                    && method_qualifiers_conflict(existing, &method)
                    && function_signatures_conflict(
                        &existing.value_type,
                        &method.value_type,
                        &self.struct_types,
                    )
            }) {
                let existing = &base[existing_index];
                if existing.value_type != method.value_type {
                    return Err(VerseError::check_at(
                        format!(
                            "interface method `{}` has incompatible inherited signatures",
                            method.name
                        ),
                        span,
                    ));
                }
                if existing.abstract_member && !method.abstract_member {
                    base[existing_index] = method;
                }
                continue;
            }
            base.push(method);
        }
        Ok(base)
    }

    fn check_interface_method_bodies(
        &mut self,
        interface_name: &str,
        fields: &[StructFieldInfo],
        methods: &[ClassMethod],
    ) -> Result<(), VerseError> {
        let method_bindings = self
            .interface_types
            .get(interface_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for method in methods {
            let Some(body) = method.body.as_ref() else {
                continue;
            };

            self.push_scope();
            let method_type = (|| {
                self.define(
                    "Self",
                    Type::Interface(interface_name.to_string()),
                    false,
                    method.span,
                )?;
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        method.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, method.span)?;
                }
                self.check_function(
                    &method.params,
                    &method.effects,
                    method.return_type.as_ref(),
                    body,
                )
            })();
            self.pop_scope();
            method_type?;
        }

        Ok(())
    }

    fn define_top_level_aggregate_members(&mut self, program: &Program) -> Result<(), VerseError> {
        self.define_aggregate_members(&program.statements)
    }

    fn define_aggregate_members(&mut self, statements: &[Stmt]) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            if let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.define_aggregate_members(module_statements)?;
                self.module_path.pop();
                continue;
            }
            let (
                fields,
                methods,
                kind,
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                persistable,
                computes,
            ) = match &expr.kind {
                ExprKind::StructDefinition {
                    fields,
                    persistable,
                    computes,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    (
                        self.struct_field_infos_with_owner(fields, Some(&qualified))?,
                        Vec::new(),
                        AggregateKind::Struct,
                        None,
                        Vec::new(),
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        *persistable,
                        *computes,
                    )
                }
                ExprKind::ClassDefinition {
                    base,
                    interfaces,
                    specifiers,
                    fields,
                    methods,
                    extension_methods,
                    blocks,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    let (fields, methods, unique, castable, base, implemented_interfaces) = self
                        .class_member_infos(
                            &qualified,
                            ClassDefinitionParts {
                                specifiers,
                                base: base.as_ref(),
                                interfaces,
                                fields,
                                methods,
                                extension_methods,
                                blocks,
                            },
                        )?;
                    (
                        fields,
                        methods,
                        AggregateKind::Class,
                        base,
                        implemented_interfaces,
                        unique,
                        class_has_specifier(specifiers, "abstract"),
                        class_has_specifier(specifiers, "epic_internal"),
                        class_has_specifier(specifiers, "final"),
                        class_has_specifier(specifiers, "concrete"),
                        castable,
                        class_has_specifier(specifiers, "persistable"),
                        false,
                    )
                }
                _ => continue,
            };
            let qualified = self.current_qualified_name(name);
            if kind == AggregateKind::Struct && persistable {
                self.ensure_persistable_struct(&qualified, &fields)?;
            }
            self.struct_types.insert(
                qualified,
                StructInfo {
                    kind,
                    base,
                    interfaces,
                    unique,
                    abstract_class,
                    epic_internal_class,
                    final_class,
                    concrete,
                    castable,
                    persistable,
                    computes,
                    fields,
                    methods,
                },
            );
        }

        Ok(())
    }

    fn struct_field_infos_with_owner(
        &mut self,
        fields: &[StructField],
        owner: Option<&str>,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        fields
            .iter()
            .map(|field| {
                let Some(annotation) = field.annotation.as_ref() else {
                    return Err(VerseError::check_at(
                        "expected explicit type annotation after struct field name",
                        field.span,
                    ));
                };
                let value_type = self.type_name_to_type(annotation)?;
                if field_has_specifier(&field.specifiers, "localizes")
                    && !matches!(annotation.name, TypeName::Message)
                {
                    return Err(VerseError::check_at(
                        "`localizes` field specifier requires a `message` annotation",
                        field.span,
                    ));
                }
                if let Some(default) = &field.default {
                    let default_type =
                        self.check_data_member_default(owner, &field.name, default)?;
                    self.ensure_expr_assignable(&value_type, &default_type, default, || {
                        format!(
                            "default value for field `{}` must be `{value_type}`, got `{default_type}`",
                            field.name
                        )
                    })?;
                }
                if field_has_specifier(&field.specifiers, "final") && field.default.is_none() {
                    return Err(VerseError::check_at(
                        format!("final field `{}` must have a default value", field.name),
                        field.span,
                    ));
                }
                let access = access_level_from_specifiers(&field.specifiers, "field", field.span)?;
                let mutation_access = if field.mutable && !field.var_specifiers.is_empty() {
                    access_level_from_specifiers(&field.var_specifiers, "var field", field.span)?
                } else {
                    access
                };
                Ok(StructFieldInfo {
                    name: field.name.clone(),
                    value_type,
                    has_default: field.default.is_some(),
                    mutable: field.mutable,
                    final_member: field_has_specifier(&field.specifiers, "final"),
                    access,
                    mutation_access,
                    owner: owner.map(str::to_string),
                    span: field.span,
                })
            })
            .collect()
    }

    fn check_data_member_default(
        &mut self,
        owner: Option<&str>,
        field_name: &str,
        default: &Expr,
    ) -> Result<Type, VerseError> {
        self.data_member_default_depth += 1;
        if let Some(owner) = owner {
            self.data_member_default_stack
                .push(DataMemberDefaultContext {
                    aggregate_name: owner.to_string(),
                    field_name: field_name.to_string(),
                });
        }
        self.function_effects.push(vec!["converges".to_string()]);
        let result = if let Some(owner) = owner {
            self.push_scope();
            let result = self
                .define_current_aggregate_type_if_unshadowed(owner, default.span)
                .and_then(|_| self.check_expr(default));
            self.pop_scope();
            result
        } else {
            self.check_expr(default)
        };
        self.function_effects.pop();
        if owner.is_some() {
            self.data_member_default_stack.pop();
        }
        self.data_member_default_depth -= 1;
        result
    }

    fn define_current_aggregate_type_if_unshadowed(
        &mut self,
        aggregate_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if aggregate_name.contains('(') {
            return Ok(());
        }
        let name = aggregate_unqualified_name(aggregate_name);
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            return Ok(());
        }
        if let Some(info) = self.struct_types.get(aggregate_name) {
            let value_type = match info.kind {
                AggregateKind::Struct => Type::StructType(aggregate_name.to_string()),
                AggregateKind::Class => Type::ClassType(aggregate_name.to_string()),
            };
            return self.define(name, value_type, false, span);
        }
        if self.interface_types.contains_key(aggregate_name) {
            return self.define(
                name,
                Type::InterfaceType(aggregate_name.to_string()),
                false,
                span,
            );
        }
        Ok(())
    }

    fn check_class_field_attributes(&mut self, fields: &[StructField]) -> Result<(), VerseError> {
        for field in fields {
            if field
                .attributes
                .iter()
                .any(|attribute| attribute.name == "editable")
                && let Some(annotation) = field.annotation.as_ref()
            {
                let value_type = self.type_name_to_type(annotation)?;
                if type_contains_type_param(&value_type) {
                    return Err(VerseError::check_at(
                        format!(
                            "`@editable` field `{}` cannot use a type parameter in its annotation",
                            field.name
                        ),
                        field.span,
                    ));
                }
            }
            for attribute in &field.attributes {
                for argument in &attribute.arguments {
                    self.check_expr(&argument.expr)?;
                }
            }
        }
        Ok(())
    }

    fn class_member_infos(
        &mut self,
        class_name: &str,
        parts: ClassDefinitionParts<'_>,
    ) -> Result<ClassMemberInfosResult, VerseError> {
        let ClassDefinitionParts {
            specifiers,
            base,
            interfaces,
            fields,
            methods,
            extension_methods,
            blocks,
        } = parts;

        if class_has_specifier(specifiers, "abstract")
            && class_has_specifier(specifiers, "concrete")
        {
            return Err(VerseError::check_at(
                format!("class `{class_name}` cannot be both `abstract` and `concrete`"),
                fields
                    .first()
                    .map_or_else(|| Span::new(0, 0, 1, 1), |field| field.span),
            ));
        }

        let mut implemented_interfaces = Vec::new();
        let (mut inherited_fields, mut inherited_methods, base_name, base_unique, base_castable) =
            if let Some(base) = base {
                match self.type_name_to_type(base)? {
                    Type::Class(base_name) => {
                        if is_builtin_class_base_name(&base_name) {
                            (Vec::new(), Vec::new(), Some(base_name), false, false)
                        } else {
                            let Some(info) = self.struct_types.get(&base_name).cloned() else {
                                return Err(VerseError::check_at(
                                    format!("unknown class `{base_name}`"),
                                    base.span,
                                ));
                            };
                            if info.kind != AggregateKind::Class {
                                return Err(VerseError::check_at(
                                    format!("class base must be a class, got `{base_name}`"),
                                    base.span,
                                ));
                            }
                            if info.final_class {
                                return Err(VerseError::check_at(
                                    format!(
                                        "class `{base_name}` is `final` and cannot be inherited"
                                    ),
                                    base.span,
                                ));
                            }
                            (
                                info.fields,
                                info.methods,
                                Some(base_name),
                                info.unique,
                                info.castable,
                            )
                        }
                    }
                    Type::Interface(interface_name) => {
                        implemented_interfaces.push(interface_name);
                        (Vec::new(), Vec::new(), None, false, false)
                    }
                    Type::Modifier(item_type) => (
                        Vec::new(),
                        vec![modifier_method_info(item_type.as_ref(), base.span)],
                        None,
                        false,
                        false,
                    ),
                    other => {
                        return Err(VerseError::check_at(
                            format!("class parent must be a class or interface, got `{other}`"),
                            base.span,
                        ));
                    }
                }
            } else {
                (Vec::new(), Vec::new(), None, false, false)
            };
        let class_span =
            class_definition_diagnostic_span(base, fields, methods, extension_methods, blocks);
        let has_final_super = class_has_specifier(specifiers, "final_super");
        if has_final_super && base_name.as_deref() != Some("component") {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` with `<final_super>` must directly inherit from `component`"
                ),
                class_span,
            ));
        }
        if base_name.as_deref() == Some("component") && !has_final_super {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` directly inheriting from `component` must specify `<final_super>`"
                ),
                class_span,
            ));
        }
        for interface in interfaces {
            let interface_type = self.type_name_to_type(interface)?;
            let Type::Interface(interface_name) = interface_type else {
                if let Type::Modifier(item_type) = interface_type {
                    inherited_methods
                        .push(modifier_method_info(item_type.as_ref(), interface.span));
                    continue;
                }
                return Err(VerseError::check_at(
                    format!("additional class parent must be an interface, got `{interface_type}`"),
                    interface.span,
                ));
            };
            implemented_interfaces.push(interface_name);
        }
        if let Some(base_name) = &base_name
            && let Some(base_info) = self.struct_types.get(base_name)
        {
            implemented_interfaces.extend(base_info.interfaces.clone());
        }
        implemented_interfaces = dedupe_strings(implemented_interfaces);
        let interface_fields = self.interface_field_requirements(&implemented_interfaces)?;
        inherited_fields =
            self.merge_interface_fields(inherited_fields, interface_fields, class_name, base)?;
        let interface_methods = self.interface_method_requirements(&implemented_interfaces)?;
        inherited_methods =
            self.merge_interface_methods(inherited_methods, interface_methods, class_name, base)?;
        let unique = class_has_specifier(specifiers, "unique") || base_unique;
        let castable = class_has_specifier(specifiers, "castable") || base_castable;

        let local = self.struct_field_infos_with_owner(fields, Some(class_name))?;
        for field in local {
            let source_field = fields
                .iter()
                .find(|candidate| candidate.name == field.name)
                .expect("field info should come from class field");
            let override_field = field_has_specifier(&source_field.specifiers, "override");
            if let Some(index) = inherited_fields
                .iter()
                .position(|candidate| candidate.name == field.name)
            {
                if !override_field {
                    return Err(VerseError::check_at(
                        format!("duplicate inherited class field `{}`", field.name),
                        source_field.span,
                    ));
                }
                let inherited = &inherited_fields[index];
                if inherited.final_member {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, inherited.name
                        ),
                        source_field.span,
                    ));
                }
                if !self.is_field_override_assignable(
                    &inherited.value_type,
                    &field.value_type,
                    class_name,
                    base_name.as_deref(),
                ) {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides `{}` but has incompatible type `{}`",
                            field.name, inherited.value_type, field.value_type
                        ),
                        source_field.span,
                    ));
                }
                inherited_fields[index] = field;
                continue;
            } else if override_field {
                return Err(VerseError::check_at(
                    format!(
                        "field `{}` does not override an inherited field",
                        field.name
                    ),
                    source_field.span,
                ));
            }
            if inherited_methods
                .iter()
                .any(|candidate| candidate.name == field.name)
            {
                return Err(VerseError::check_at(
                    format!("duplicate inherited class member `{}`", field.name),
                    fields
                        .iter()
                        .find(|candidate| candidate.name == field.name)
                        .map_or_else(|| Span::new(0, 0, 1, 1), |candidate| candidate.span),
                ));
            }
            inherited_fields.push(field);
        }

        if !class_has_specifier(specifiers, "abstract") {
            self.ensure_interface_required_fields_initializable(
                class_name,
                specifiers,
                &inherited_fields,
            )?;
        }

        if class_has_specifier(specifiers, "concrete") {
            self.ensure_concrete_class_fields(class_name, &inherited_fields)?;
        }

        if class_has_specifier(specifiers, "persistable") {
            self.ensure_persistable_class(class_name, specifiers, base, &inherited_fields)?;
        }

        let local_method_signatures = self.class_method_signature_infos(class_name, methods)?;
        let method_signatures = self.merge_class_methods(
            &inherited_fields,
            inherited_methods.clone(),
            methods,
            local_method_signatures,
        )?;
        let previous_info = self.struct_types.insert(
            class_name.to_string(),
            StructInfo {
                kind: AggregateKind::Class,
                base: base_name.clone(),
                interfaces: implemented_interfaces.clone(),
                unique,
                abstract_class: class_has_specifier(specifiers, "abstract"),
                epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                final_class: class_has_specifier(specifiers, "final"),
                concrete: class_has_specifier(specifiers, "concrete"),
                castable,
                persistable: class_has_specifier(specifiers, "persistable"),
                computes: false,
                fields: inherited_fields.clone(),
                methods: method_signatures,
            },
        );
        let local_checks = self.with_local_extension_methods(extension_methods, |checker| {
            checker.check_class_extension_methods(
                class_name,
                base_name.as_deref(),
                &inherited_fields,
                extension_methods,
            )?;
            let local_methods = checker.class_method_infos(
                class_name,
                base_name.as_deref(),
                &inherited_fields,
                methods,
            )?;
            let blocks_result = checker.check_class_blocks(
                class_name,
                base_name.as_deref(),
                &inherited_fields,
                blocks,
            );
            Ok((local_methods, blocks_result))
        });
        if let Some(previous_info) = previous_info {
            self.struct_types
                .insert(class_name.to_string(), previous_info);
        } else {
            self.struct_types.remove(class_name);
        }
        let (local_methods, blocks_result) = local_checks?;
        blocks_result?;
        let inherited_methods =
            self.merge_class_methods(&inherited_fields, inherited_methods, methods, local_methods)?;
        self.ensure_abstract_methods_implemented(
            class_name,
            class_has_specifier(specifiers, "abstract"),
            class_definition_diagnostic_span(base, fields, methods, extension_methods, blocks),
            &inherited_methods,
        )?;

        Ok((
            inherited_fields,
            inherited_methods,
            unique,
            castable,
            base_name,
            implemented_interfaces,
        ))
    }

    fn ensure_abstract_methods_implemented(
        &self,
        class_name: &str,
        is_abstract: bool,
        class_span: Span,
        methods: &[ClassMethodInfo],
    ) -> Result<(), VerseError> {
        if is_abstract {
            return Ok(());
        }

        if let Some(method) = methods.iter().find(|method| method.abstract_member) {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` must be `abstract` or implement method `{}`",
                    method.name
                ),
                class_span,
            ));
        }

        Ok(())
    }

    fn ensure_interface_required_fields_initializable(
        &self,
        class_name: &str,
        class_specifiers: &[String],
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        let class_is_public = class_has_specifier(class_specifiers, "public");
        for field in fields {
            let Some(owner) = field.owner.as_deref() else {
                continue;
            };
            if field.has_default || !self.interface_types.contains_key(owner) {
                continue;
            }
            let inaccessible_from_constructor =
                matches!(field.access, AccessLevel::Private | AccessLevel::Protected)
                    || (field.access == AccessLevel::Internal && class_is_public);
            if inaccessible_from_constructor {
                return Err(VerseError::check_at(
                    format!(
                        "class `{class_name}` must be `abstract` or provide a default value for interface field `{}`",
                        field.name
                    ),
                    field.span,
                ));
            }
        }
        Ok(())
    }

    fn ensure_concrete_class_fields(
        &self,
        class_name: &str,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        for field in fields {
            if !field.has_default {
                return Err(VerseError::check_at(
                    format!(
                        "concrete class `{class_name}` field `{}` must have a default value",
                        field.name
                    ),
                    field.span,
                ));
            }
        }
        Ok(())
    }

    fn ensure_persistable_class(
        &self,
        class_name: &str,
        specifiers: &[String],
        base: Option<&TypeAnnotation>,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        let span = fields
            .first()
            .map_or_else(|| Span::new(0, 0, 1, 1), |field| field.span);

        if !class_has_specifier(specifiers, "final") {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` must also be `final`"),
                span,
            ));
        }

        if class_has_specifier(specifiers, "unique") {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` cannot be `unique`"),
                span,
            ));
        }

        if let Some(base) = base {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` cannot have a superclass"),
                base.span,
            ));
        }

        for field in fields {
            if field.mutable {
                return Err(VerseError::check_at(
                    format!(
                        "persistable class `{class_name}` field `{}` cannot be variable",
                        field.name
                    ),
                    field.span,
                ));
            }

            if !self.is_persistable_type(&field.value_type) {
                return Err(VerseError::check_at(
                    format!(
                        "persistable class `{class_name}` field `{}` has non-persistable type `{}`",
                        field.name, field.value_type
                    ),
                    field.span,
                ));
            }
        }

        Ok(())
    }

    fn ensure_persistable_struct(
        &self,
        struct_name: &str,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        for field in fields {
            if !self.is_persistable_type(&field.value_type) {
                return Err(VerseError::check_at(
                    format!(
                        "persistable struct `{struct_name}` field `{}` has non-persistable type `{}`",
                        field.name, field.value_type
                    ),
                    field.span,
                ));
            }
        }

        Ok(())
    }

    fn check_class_blocks(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        fields: &[StructFieldInfo],
        blocks: &[ClassBlock],
    ) -> Result<(), VerseError> {
        for block in blocks {
            self.push_scope();
            self.class_context.push(class_name.to_string());
            let result = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    block.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        block.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        block.span,
                    )?;
                }
                self.define_current_class_type_if_unshadowed(class_name, block.span)?;
                if let Some(span) = defer_body_failable_expr(&block.body) {
                    return Err(VerseError::check_at(
                        "class block cannot contain failable expressions",
                        span,
                    ));
                }
                self.push_class_member_shadow_names(class_name, fields);
                let block_result = self.check_expr(&block.body);
                self.pop_class_member_shadow_names();
                block_result?;
                Ok(())
            })();
            self.class_context.pop();
            self.pop_scope();
            result?;
        }
        Ok(())
    }

    fn is_field_override_assignable(
        &self,
        expected: &Type,
        actual: &Type,
        class_name: &str,
        base_name: Option<&str>,
    ) -> bool {
        self.is_assignable(expected, actual)
            || matches!(
                (expected, actual, base_name),
                (Type::Class(expected), Type::Class(actual), Some(base))
                    if expected == base && actual == class_name
            )
            || matches!(
                (expected, actual),
                (Type::Option(expected), Type::Option(actual))
                    if self.is_field_override_assignable(expected, actual, class_name, base_name)
            )
    }

    fn merge_class_methods(
        &self,
        fields: &[StructFieldInfo],
        mut inherited_methods: Vec<ClassMethodInfo>,
        source_methods: &[ClassMethod],
        local_methods: Vec<ClassMethodInfo>,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        for method in local_methods {
            let source_method = source_methods
                .iter()
                .find(|candidate| candidate.span == method.span)
                .expect("local method info should have a source method");
            if fields.iter().any(|candidate| candidate.name == method.name) {
                return Err(VerseError::check_at(
                    format!("duplicate class member `{}`", method.name),
                    source_method.span,
                ));
            }

            let override_method = has_effect(&source_method.effects, "override");
            let matching_index =
                inherited_method_override_index(&inherited_methods, &method, &self.struct_types)?;
            let duplicate_index =
                inherited_method_duplicate_index(&inherited_methods, &method, &self.struct_types);

            if override_method {
                let Some(index) = matching_index else {
                    return Err(VerseError::check_at(
                        format!(
                            "method `{}` does not override an inherited method",
                            method.name
                        ),
                        source_method.span,
                    ));
                };
                let inherited = &inherited_methods[index];
                if inherited.final_member {
                    return Err(VerseError::check_at(
                        format!(
                            "method `{}` overrides final inherited method `{}`",
                            method.name, inherited.name
                        ),
                        source_method.span,
                    ));
                }
                self.ensure_assignable(
                    &inherited.value_type,
                    &method.value_type,
                    source_method.span,
                    || {
                        format!(
                            "override method `{}` must be assignable to inherited method type `{}`",
                            method.name, inherited.value_type
                        )
                    },
                )?;
                let mut replacement = method;
                if replacement.qualifier.is_none() {
                    replacement.qualifier = inherited.qualifier.clone();
                }
                inherited_methods[index] = replacement;
            } else {
                if duplicate_index.is_some() {
                    return Err(VerseError::check_at(
                        format!("duplicate inherited class method `{}`", method.name),
                        source_method.span,
                    ));
                }
                inherited_methods.push(method);
            }
        }

        Ok(inherited_methods)
    }

    fn merge_interface_methods(
        &self,
        mut inherited_methods: Vec<ClassMethodInfo>,
        interface_methods: Vec<ClassMethodInfo>,
        class_name: &str,
        base: Option<&TypeAnnotation>,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let span = base.map_or_else(|| Span::new(0, 0, 1, 1), |base| base.span);
        for method in interface_methods {
            if let Some(existing_index) = inherited_methods
                .iter()
                .position(|candidate| {
                    method_signatures_conflict(candidate, &method, &self.struct_types)
                        && method_qualifiers_conflict(candidate, &method)
                })
                .or_else(|| {
                    inherited_methods.iter().position(|candidate| {
                        candidate.qualifier.is_none()
                            && method_signatures_conflict(candidate, &method, &self.struct_types)
                    })
                })
            {
                let existing = &inherited_methods[existing_index];
                self.ensure_assignable(&method.value_type, &existing.value_type, span, || {
                    format!(
                        "class `{class_name}` inherited method `{}` is not assignable to interface method type `{}`",
                        existing.name, method.value_type
                    )
                })?;
                if inherited_methods[existing_index].qualifier.is_none() {
                    inherited_methods[existing_index].qualifier = method.qualifier.clone();
                }
                continue;
            }
            inherited_methods.push(method);
        }
        Ok(inherited_methods)
    }

    fn merge_interface_fields(
        &self,
        mut inherited_fields: Vec<StructFieldInfo>,
        interface_fields: Vec<StructFieldInfo>,
        class_name: &str,
        base: Option<&TypeAnnotation>,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        let span = base.map_or_else(|| Span::new(0, 0, 1, 1), |base| base.span);
        for field in interface_fields {
            if let Some(existing) = inherited_fields
                .iter()
                .find(|candidate| candidate.name == field.name)
            {
                if existing.value_type != field.value_type || existing.mutable != field.mutable {
                    return Err(VerseError::check_at(
                        format!(
                            "class `{class_name}` inherited field `{}` is not assignable to interface field type `{}`",
                            existing.name, field.value_type
                        ),
                        span,
                    ));
                }
                continue;
            }
            inherited_fields.push(field);
        }
        Ok(inherited_fields)
    }

    fn class_method_signature_infos(
        &mut self,
        class_name: &str,
        methods: &[ClassMethod],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        for method in methods {
            self.validate_abstract_class_method_shape(method)?;
            let info = ClassMethodInfo {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                final_member: has_effect(&method.effects, "final"),
                abstract_member: self.class_method_is_abstract(method),
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                owner: Some(class_name.to_string()),
                value_type: self.class_method_declared_type(method)?,
                span: method.span,
            };
            push_distinct_local_method_info(&mut infos, info, "class", &self.struct_types)?;
        }
        Ok(infos)
    }

    fn class_method_declared_type(&mut self, method: &ClassMethod) -> Result<Type, VerseError> {
        validate_function_effect_combination(&method.effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects: method.effects.clone(),
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(self.annotation_to_type(method.return_type.as_ref())?),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    fn extension_receiver_type(&mut self, extension: &ExtensionMethod) -> Result<Type, VerseError> {
        let Some(annotation) = extension.receiver.annotation.as_ref() else {
            return Err(VerseError::check_at(
                "extension method receiver requires an explicit type annotation",
                extension.receiver.span,
            ));
        };
        self.type_name_to_type(annotation)
    }

    fn ensure_extension_method_not_conflicting_with_member(
        &self,
        receiver_type: &Type,
        method: &ClassMethod,
        span: Span,
    ) -> Result<(), VerseError> {
        match receiver_type {
            Type::Class(class_name) => {
                let Some(info) = self.struct_types.get(class_name) else {
                    return Ok(());
                };
                if info.methods.iter().any(|member| member.name == method.name) {
                    return Err(VerseError::check_at(
                        format!(
                            "extension method `{}` conflicts with class `{class_name}` method `{}`",
                            method.name, method.name
                        ),
                        span,
                    ));
                }
            }
            Type::Interface(interface_name) => {
                let Some(info) = self.interface_types.get(interface_name) else {
                    return Ok(());
                };
                if info.methods.iter().any(|member| member.name == method.name) {
                    return Err(VerseError::check_at(
                        format!(
                            "extension method `{}` conflicts with interface `{interface_name}` method `{}`",
                            method.name, method.name
                        ),
                        span,
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn extension_method_declared_type(&mut self, method: &ClassMethod) -> Result<Type, VerseError> {
        validate_function_effect_combination(&method.effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects: method.effects.clone(),
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(self.annotation_to_type(method.return_type.as_ref())?),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    fn extension_method_type_with_return(
        &mut self,
        method: &ClassMethod,
        return_type: Type,
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(&method.effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects: method.effects.clone(),
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(return_type.clone()),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    fn class_method_is_abstract(&self, method: &ClassMethod) -> bool {
        method.body.is_none() || has_effect(&method.effects, "abstract")
    }

    fn validate_abstract_class_method_shape(&self, method: &ClassMethod) -> Result<(), VerseError> {
        if !self.class_method_is_abstract(method) {
            return Ok(());
        }

        if method.return_type.is_none() {
            return Err(VerseError::check_at(
                format!(
                    "abstract class method `{}` requires an explicit return type",
                    method.name
                ),
                method.span,
            ));
        }

        if has_effect(&method.effects, "final") {
            return Err(VerseError::check_at(
                format!("abstract class method `{}` cannot be `final`", method.name),
                method.span,
            ));
        }

        if method.body.is_some() && has_effect(&method.effects, "abstract") {
            return Err(VerseError::check_at(
                format!("abstract class method `{}` cannot have a body", method.name),
                method.span,
            ));
        }

        Ok(())
    }

    fn class_method_infos(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        fields: &[StructFieldInfo],
        methods: &[ClassMethod],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        let method_bindings = self
            .struct_types
            .get(class_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for method in methods {
            self.validate_abstract_class_method_shape(method)?;
            if method.body.is_none() {
                infos.push(ClassMethodInfo {
                    qualifier: method.qualifier.clone(),
                    name: method.name.clone(),
                    value_type: self.class_method_declared_type(method)?,
                    final_member: has_effect(&method.effects, "final"),
                    abstract_member: true,
                    access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                    owner: Some(class_name.to_string()),
                    span: method.span,
                });
                continue;
            }

            self.push_scope();
            self.class_context.push(class_name.to_string());
            let method_type = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    method.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        method.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        method.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, method.span)?;
                }
                self.define_current_class_type_if_unshadowed(class_name, method.span)?;

                self.push_class_member_shadow_names(class_name, fields);
                let function_result = self.check_function(
                    &method.params,
                    &method.effects,
                    method.return_type.as_ref(),
                    method
                        .body
                        .as_ref()
                        .expect("concrete class method should have a body"),
                );
                self.pop_class_member_shadow_names();
                function_result
            })();
            self.class_context.pop();
            self.pop_scope();

            infos.push(ClassMethodInfo {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                value_type: method_type?,
                final_member: has_effect(&method.effects, "final"),
                abstract_member: false,
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                owner: Some(class_name.to_string()),
                span: method.span,
            });
        }

        Ok(infos)
    }

    fn check_class_extension_methods(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        fields: &[StructFieldInfo],
        extensions: &[ExtensionMethod],
    ) -> Result<(), VerseError> {
        let method_bindings = self
            .struct_types
            .get(class_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for extension in extensions {
            let Some(body) = extension.method.body.as_ref() else {
                return Err(VerseError::check_at(
                    "extension method requires a body",
                    extension.span,
                ));
            };
            let receiver_type = self.extension_receiver_type(extension)?;
            self.ensure_extension_method_not_conflicting_with_member(
                &receiver_type,
                &extension.method,
                extension.span,
            )?;
            let mut params = Vec::with_capacity(extension.method.params.len() + 1);
            params.push(extension.receiver.clone());
            params.extend(extension.method.params.clone());

            self.push_scope();
            self.class_context.push(class_name.to_string());
            let checked_type = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    extension.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        extension.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        extension.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, extension.span)?;
                }
                self.define_current_class_type_if_unshadowed(class_name, extension.span)?;

                self.push_class_member_shadow_names(class_name, fields);
                let function_result = self.check_function(
                    &params,
                    &extension.method.effects,
                    extension.method.return_type.as_ref(),
                    body,
                );
                self.pop_class_member_shadow_names();
                function_result
            })();
            self.class_context.pop();
            self.pop_scope();

            let Type::Function { return_type, .. } = checked_type? else {
                unreachable!("check_function should always return a function type");
            };
            let visible_type =
                self.extension_method_type_with_return(&extension.method, *return_type)?;
            self.update_local_extension_method_type(
                &extension.method.name,
                &receiver_type,
                visible_type,
                extension.span,
            )?;
        }

        Ok(())
    }

    fn define_current_class_type_if_unshadowed(
        &mut self,
        class_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if class_name.contains('(') {
            return Ok(());
        }
        let name = aggregate_unqualified_name(class_name);
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            return Ok(());
        }
        self.define(name, Type::ClassType(class_name.to_string()), false, span)
    }

    fn predeclare_top_level_functions(&mut self, program: &Program) -> Result<(), VerseError> {
        self.predeclare_functions_in_current_scope(&program.statements)
    }

    fn predeclare_functions_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::Function {
                params,
                effects,
                return_type,
                ..
            } = &expr.kind
            else {
                continue;
            };

            if self.type_aliases.contains_key(name) {
                return Err(VerseError::check_at(
                    format!("function `{name}` conflicts with type alias `{name}`"),
                    statement.span,
                ));
            }
            let type_params = collect_function_type_params(params)?;
            self.validate_type_parameter_constraints(&type_params, statement.span)?;
            self.push_type_param_scope(type_params.iter().map(|param| {
                (
                    param.name.clone(),
                    Type::Param(param.name.clone(), param.constraint.clone()),
                )
            }));
            let function_type = (|| {
                Ok(Type::Function {
                    arity: Some(params.len()),
                    arity_range: None,
                    effects: effects.clone(),
                    param_types: Some(self.param_types(params)?),
                    param_specs: Some(self.param_specs(params)?),
                    return_type: Box::new(self.annotation_to_type(return_type.as_ref())?),
                })
            })();
            self.pop_type_param_scope();
            let function_type = function_type?;
            self.define_predeclared_function(name, function_type, statement.span)?;
        }

        Ok(())
    }

    fn define_predeclared_function(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type));
            return Ok(());
        };

        if existing.mutable {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }

        match &mut existing.value_type {
            Type::Function { .. } => {
                if function_signatures_conflict(
                    &existing.value_type,
                    &function_type,
                    &self.struct_types,
                ) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                let previous = existing.value_type.clone();
                existing.value_type = Type::Overload(vec![previous, function_type]);
                Ok(())
            }
            Type::Overload(overloads) => {
                if overloads.iter().any(|overload| {
                    function_signatures_conflict(overload, &function_type, &self.struct_types)
                }) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                overloads.push(function_type);
                Ok(())
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    fn predeclare_extension_methods_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::ExtensionMethod(method) = &statement.kind else {
                continue;
            };
            self.register_extension_method_signature(method)?;
        }

        Ok(())
    }

    fn register_extension_method_signature(
        &mut self,
        extension: &ExtensionMethod,
    ) -> Result<(), VerseError> {
        let receiver_type = self.extension_receiver_type(extension)?;
        let method_type = self.extension_method_declared_type(&extension.method)?;
        let access = access_level_from_specifiers(
            &extension.method.effects,
            "extension method",
            extension.span,
        )?;
        let module_name = self.current_module_name();
        let methods = self
            .extension_methods
            .entry(extension.method.name.clone())
            .or_default();

        if let Some(existing) = methods.iter().find(|method| {
            method.module_name == module_name && method.receiver_type == receiver_type
        }) {
            return Err(VerseError::check_at(
                format!(
                    "duplicate extension method `{}` for receiver type `{receiver_type}`",
                    extension.method.name
                ),
                existing.span.through(extension.span),
            ));
        }

        methods.push(ExtensionMethodInfo {
            receiver_type,
            method_type,
            module_name,
            access,
            span: extension.span,
        });

        Ok(())
    }

    fn with_local_extension_methods<T>(
        &mut self,
        extensions: &[ExtensionMethod],
        f: impl FnOnce(&mut Self) -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        let previous = self.extension_methods.clone();
        let result = self
            .register_local_extension_method_signatures(extensions)
            .and_then(|_| f(self));
        self.extension_methods = previous;
        result
    }

    fn register_local_extension_method_signatures(
        &mut self,
        extensions: &[ExtensionMethod],
    ) -> Result<(), VerseError> {
        let mut local = Vec::with_capacity(extensions.len());
        for extension in extensions {
            let receiver_type = self.extension_receiver_type(extension)?;
            if local.iter().any(
                |(name, existing_receiver, _, _, _): &(String, Type, Type, AccessLevel, Span)| {
                    name == &extension.method.name && existing_receiver == &receiver_type
                },
            ) {
                return Err(VerseError::check_at(
                    format!(
                        "duplicate extension method `{}` for receiver type `{receiver_type}`",
                        extension.method.name
                    ),
                    extension.span,
                ));
            }
            let method_type = self.extension_method_declared_type(&extension.method)?;
            let access = access_level_from_specifiers(
                &extension.method.effects,
                "extension method",
                extension.span,
            )?;
            local.push((
                extension.method.name.clone(),
                receiver_type,
                method_type,
                access,
                extension.span,
            ));
        }

        for (name, receiver_type, method_type, access, span) in local {
            let methods = self.extension_methods.entry(name).or_default();
            methods.retain(|method| method.receiver_type != receiver_type);
            methods.push(ExtensionMethodInfo {
                receiver_type,
                method_type,
                module_name: None,
                access,
                span,
            });
        }

        Ok(())
    }

    fn check_statements(&mut self, statements: &[Stmt]) -> Result<Type, VerseError> {
        let mut last = Type::None;
        let mut unreachable_after: Option<(&'static str, Span)> = None;

        for statement in statements {
            if let Some((message, span)) = unreachable_after {
                return Err(VerseError::check_at(message, span.through(statement.span)));
            }
            last = self.check_stmt(statement)?;
            if last == Type::Never || self.statement_never_completes(statement) {
                unreachable_after =
                    Some((unreachable_statement_message(statement), statement.span));
            }
        }

        Ok(last)
    }

    fn check_stmt(&mut self, statement: &Stmt) -> Result<Type, VerseError> {
        match &statement.kind {
            StmtKind::Using { path } => {
                if !self.current_definition_level() {
                    return Err(VerseError::check_at(
                        "`using` statements are only supported at module level",
                        statement.span,
                    ));
                }
                if is_absolute_module_path(path) {
                    if !is_supported_using_path(path) {
                        return Err(VerseError::check_at(
                            format!("unsupported module path `{path}`"),
                            statement.span,
                        ));
                    }
                    return Ok(Type::None);
                }

                let Some(module_name) = self.resolve_module_path(path) else {
                    return Err(VerseError::check_at(
                        format!("unsupported module path `{path}`"),
                        statement.span,
                    ));
                };
                self.add_current_import(module_name);
                Ok(Type::None)
            }
            StmtKind::Let {
                name,
                specifiers,
                annotation,
                expr,
            } => self.check_binding(
                name,
                specifiers,
                annotation.as_ref(),
                expr,
                false,
                statement.span,
            ),
            StmtKind::ParametricType {
                name,
                specifiers,
                params,
                expr,
            } => self.check_parametric_type_definition(
                name,
                specifiers,
                params,
                expr,
                statement.span,
            ),
            StmtKind::TypeAlias { .. } => {
                if !self.current_definition_level() {
                    return Err(VerseError::check_at(
                        "type aliases are only supported at module level",
                        statement.span,
                    ));
                }
                Ok(Type::None)
            }
            StmtKind::ExtensionMethod(method) => self.check_extension_method(method),
            StmtKind::Var {
                name,
                annotation,
                expr,
            } => self.check_binding(name, &[], annotation.as_ref(), expr, true, statement.span),
            StmtKind::Set { target, op, expr } => {
                self.check_set_expression(target, *op, expr, false, statement.span)
            }
            StmtKind::Return(expr) => {
                let Some(expected) = self.function_returns.last().cloned() else {
                    return Err(VerseError::check_at(
                        "`return` used outside a function",
                        statement.span,
                    ));
                };

                let actual = self.check_expr(expr)?;
                self.ensure_expr_assignable(&expected, &actual, expr, || {
                    format!("cannot return `{actual}` from function returning `{expected}`")
                })?;
                Ok(Type::Never)
            }
            StmtKind::Break => {
                if self.break_depth == 0 {
                    Err(VerseError::check_at(
                        "`break` used outside a loop",
                        statement.span,
                    ))
                } else {
                    Ok(Type::Never)
                }
            }
            StmtKind::Defer(body) => {
                if defer_body_is_empty(body) {
                    return Err(VerseError::check_at(
                        "`defer` block cannot be empty",
                        statement.span,
                    ));
                }
                if let Some((message, span)) = defer_body_escape(body, 0) {
                    return Err(VerseError::check_at(message, span));
                }
                if let Some(span) = defer_body_failable_expr(body) {
                    return Err(VerseError::check_at(
                        "`defer` block cannot contain failable expressions",
                        span,
                    ));
                }
                self.defer_depth += 1;
                let result = self.check_expr(body);
                self.defer_depth -= 1;
                result?;
                Ok(Type::None)
            }
            StmtKind::Expr(expr) => self.check_expr(expr),
        }
    }

    fn check_parametric_type_definition(
        &mut self,
        name: &str,
        specifiers: &[String],
        params: &[TypeParam],
        expr: &Expr,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !self.current_definition_level() {
            return Err(VerseError::check_at(
                "parametric type definitions are only supported at module level",
                span,
            ));
        }
        self.validate_data_specifiers(name, specifiers, None, false, span)?;
        let Some(kind) = parametric_type_kind(expr) else {
            return Err(VerseError::check_at(
                "parametric type definitions must define a class, struct, or interface",
                span,
            ));
        };
        self.validate_type_parameter_names(params, span)?;
        self.validate_type_parameter_constraints(params, span)?;
        self.check_parametric_type_field_attributes(params, expr)?;
        let qualified = self.current_qualified_name(name);
        let value_type = Type::ParametricType {
            name: qualified,
            params: params.iter().map(|param| param.name.clone()).collect(),
            kind,
        };
        self.define(name, value_type.clone(), false, span)?;
        self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
        Ok(value_type)
    }

    fn check_parametric_type_field_attributes(
        &mut self,
        params: &[TypeParam],
        expr: &Expr,
    ) -> Result<(), VerseError> {
        self.push_type_param_scope(params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = match &expr.kind {
            ExprKind::ClassDefinition { fields, .. }
            | ExprKind::InterfaceDefinition { fields, .. } => {
                self.check_class_field_attributes(fields)
            }
            _ => Ok(()),
        };
        self.pop_type_param_scope();
        result
    }

    fn validate_type_parameter_names(
        &self,
        params: &[TypeParam],
        span: Span,
    ) -> Result<(), VerseError> {
        if params.is_empty() {
            return Err(VerseError::check_at(
                "parametric type definitions expect at least one type parameter",
                span,
            ));
        }
        let mut seen = Vec::new();
        for param in params {
            if seen.iter().any(|name| name == &param.name) {
                return Err(VerseError::check_at(
                    format!("duplicate type parameter `{}`", param.name),
                    param.span,
                ));
            }
            seen.push(param.name.clone());
        }
        Ok(())
    }

    fn validate_type_parameter_constraints(
        &mut self,
        params: &[TypeParam],
        span: Span,
    ) -> Result<(), VerseError> {
        for param in params {
            if let TypeParamConstraint::Subtype(parent) = &param.constraint {
                self.type_name_to_type_name(parent, span)?;
            }
        }
        Ok(())
    }

    fn check_extension_method(&mut self, extension: &ExtensionMethod) -> Result<Type, VerseError> {
        if !self.current_definition_level() {
            return Err(VerseError::check_at(
                "extension methods are only supported at module level",
                extension.span,
            ));
        }

        let Some(body) = extension.method.body.as_ref() else {
            return Err(VerseError::check_at(
                "extension method requires a body",
                extension.span,
            ));
        };

        let receiver_type = self.extension_receiver_type(extension)?;
        self.ensure_extension_method_not_conflicting_with_member(
            &receiver_type,
            &extension.method,
            extension.span,
        )?;
        let mut params = Vec::with_capacity(extension.method.params.len() + 1);
        params.push(extension.receiver.clone());
        params.extend(extension.method.params.clone());
        let checked_type = self.check_function(
            &params,
            &extension.method.effects,
            extension.method.return_type.as_ref(),
            body,
        )?;

        let Type::Function { return_type, .. } = checked_type else {
            unreachable!("check_function should always return a function type");
        };
        let visible_type =
            self.extension_method_type_with_return(&extension.method, *return_type)?;
        self.update_extension_method_type(
            &extension.method.name,
            &receiver_type,
            visible_type,
            extension.span,
        )?;

        Ok(Type::None)
    }

    fn update_extension_method_type(
        &mut self,
        name: &str,
        receiver_type: &Type,
        method_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let module_name = self.current_module_name();
        let methods = self.extension_methods.entry(name.to_string()).or_default();
        if let Some(method) = methods.iter_mut().find(|method| {
            method.module_name == module_name && &method.receiver_type == receiver_type
        }) {
            method.method_type = method_type;
            return Ok(());
        }

        methods.push(ExtensionMethodInfo {
            receiver_type: receiver_type.clone(),
            method_type,
            module_name,
            access: AccessLevel::Internal,
            span,
        });
        Ok(())
    }

    fn update_local_extension_method_type(
        &mut self,
        name: &str,
        receiver_type: &Type,
        method_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(methods) = self.extension_methods.get_mut(name) else {
            return Err(VerseError::check_at(
                format!("unknown local extension method `{name}`"),
                span,
            ));
        };
        if let Some(method) = methods
            .iter_mut()
            .rev()
            .find(|method| method.module_name.is_none() && &method.receiver_type == receiver_type)
        {
            method.method_type = method_type;
            return Ok(());
        }

        Err(VerseError::check_at(
            format!("unknown local extension method `{name}` for receiver type `{receiver_type}`"),
            span,
        ))
    }

    fn check_binding(
        &mut self,
        name: &str,
        specifiers: &[String],
        annotation: Option<&TypeAnnotation>,
        expr: &Expr,
        mutable: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        self.validate_data_specifiers(name, specifiers, annotation, mutable, span)?;

        if let ExprKind::EnumDefinition {
            open,
            persistable,
            variants,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "enum definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "enum definitions are only supported at module level",
                    span,
                ));
            }
            validate_enum_variant_qualifiers(name, variants)?;
            let qualified = self.current_qualified_name(name);
            self.enum_types
                .entry(qualified.clone())
                .or_insert(EnumInfo {
                    variants: enum_variant_names(variants),
                    open: *open,
                    persistable: *persistable,
                });
            let value_type = Type::EnumType(qualified);
            self.define(name, value_type.clone(), false, span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::StructDefinition {
            fields,
            persistable,
            computes,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "struct definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "struct definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            if !self.struct_types.contains_key(&qualified) {
                let fields = self.struct_field_infos_with_owner(fields, Some(&qualified))?;
                if *persistable {
                    self.ensure_persistable_struct(&qualified, &fields)?;
                }
                self.struct_types.insert(
                    qualified.clone(),
                    StructInfo {
                        kind: AggregateKind::Struct,
                        base: None,
                        interfaces: Vec::new(),
                        unique: false,
                        abstract_class: false,
                        epic_internal_class: false,
                        final_class: false,
                        concrete: false,
                        castable: false,
                        persistable: *persistable,
                        computes: *computes,
                        fields,
                        methods: Vec::new(),
                    },
                );
            }
            let value_type = Type::StructType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::ClassDefinition {
            base,
            interfaces,
            specifiers: class_specifiers,
            fields,
            methods,
            extension_methods,
            blocks,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "class definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "class definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            if !self.struct_types.contains_key(&qualified) {
                let (fields, methods, unique, castable, base, implemented_interfaces) = self
                    .class_member_infos(
                        &qualified,
                        ClassDefinitionParts {
                            specifiers: class_specifiers,
                            base: base.as_ref(),
                            interfaces,
                            fields,
                            methods,
                            extension_methods,
                            blocks,
                        },
                    )?;
                self.struct_types.insert(
                    qualified.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base,
                        interfaces: implemented_interfaces,
                        unique,
                        abstract_class: class_has_specifier(class_specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(class_specifiers, "epic_internal"),
                        final_class: class_has_specifier(class_specifiers, "final"),
                        concrete: class_has_specifier(class_specifiers, "concrete"),
                        castable,
                        persistable: class_has_specifier(class_specifiers, "persistable"),
                        computes: false,
                        fields,
                        methods,
                    },
                );
            }
            self.check_class_field_attributes(fields)?;
            let value_type = Type::ClassType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(
                name,
                value_type.clone(),
                module_member_specifiers(specifiers, expr),
                span,
            )?;
            return Ok(value_type);
        }

        if let ExprKind::InterfaceDefinition { fields, .. } = &expr.kind {
            if mutable {
                return Err(VerseError::check_at(
                    "interface definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "interface definitions are only supported at module level",
                    span,
                ));
            }
            self.check_class_field_attributes(fields)?;
            let qualified = self.current_qualified_name(name);
            let value_type = Type::InterfaceType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
            if mutable {
                return Err(VerseError::check_at(
                    "module definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "module definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            self.module_types
                .entry(qualified.clone())
                .or_insert(ModuleInfo {
                    members: HashMap::new(),
                    member_access: HashMap::new(),
                    imports: Vec::new(),
                });
            let value_type = Type::Module(qualified.clone());
            self.define(name, value_type.clone(), false, span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            self.check_module_body(&qualified, statements)?;
            return Ok(value_type);
        }

        if let ExprKind::Function { effects, .. } = &expr.kind
            && has_effect(effects, "final")
            && !self.current_definition_level()
        {
            return Err(VerseError::check_at(
                "`final` specifier is not allowed on local definitions",
                span,
            ));
        }

        if annotation.is_none() && matches!(&expr.kind, ExprKind::External) {
            return Err(VerseError::check_at(
                "`external {}` requires an explicit type annotation",
                expr.span,
            ));
        }

        let inferred = self.check_expr(expr)?;
        let checked_type = if let Some(annotation) = annotation {
            let expected = self.type_name_to_type(annotation)?;
            self.ensure_expr_assignable(&expected, &inferred, expr, || {
                format!(
                    "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
                )
            })?;
            expected
        } else {
            inferred
        };

        if self.scopes.len() == 1 && self.type_aliases.contains_key(name) {
            return Err(VerseError::check_at(
                format!("binding `{name}` conflicts with type alias `{name}`"),
                span,
            ));
        }

        let binding_type = if !mutable && matches!(&expr.kind, ExprKind::Function { .. }) {
            if self.current_definition_level() && self.is_current_predeclared_function(name) {
                self.update_current_function_binding(name, checked_type.clone(), span)?
            } else {
                self.define_or_overload_function(name, checked_type.clone(), span)?
            }
        } else {
            self.define(name, checked_type.clone(), mutable, span)?;
            checked_type.clone()
        };
        self.record_current_module_member(
            name,
            binding_type.clone(),
            module_member_specifiers(specifiers, expr),
            span,
        )?;
        self.record_player_weak_map_binding(&binding_type, span)?;
        Ok(binding_type)
    }

    fn record_player_weak_map_binding(
        &mut self,
        value_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        if !self.current_definition_level() {
            return Ok(());
        }

        let Type::WeakMap(key_type, weak_value_type) = value_type else {
            return Ok(());
        };
        if !matches!(key_type.as_ref(), Type::Class(name) if name == "player") {
            return Ok(());
        }

        self.player_weak_maps.push(PlayerWeakMapInfo {
            value_type: weak_value_type.as_ref().clone(),
        });

        if self.player_weak_maps.len() > 4 {
            return Err(VerseError::check_at(
                "module-scoped `weak_map(player, ...)` variables are limited to four per island",
                span,
            ));
        }

        if self.player_weak_maps.len() == 4
            && !self
                .player_weak_maps
                .iter()
                .any(|weak_map| self.player_weak_map_value_is_class(&weak_map.value_type))
        {
            return Err(VerseError::check_at(
                "when four module-scoped `weak_map(player, ...)` variables are defined, at least one value type must be a persistable class",
                span,
            ));
        }

        Ok(())
    }

    fn player_weak_map_value_is_class(&self, value_type: &Type) -> bool {
        matches!(value_type, Type::Class(name)
            if self
                .struct_types
                .get(name)
                .is_some_and(|info| info.kind == AggregateKind::Class && info.persistable))
    }

    fn validate_data_specifiers(
        &self,
        name: &str,
        specifiers: &[String],
        annotation: Option<&TypeAnnotation>,
        mutable: bool,
        span: Span,
    ) -> Result<(), VerseError> {
        if specifiers.is_empty() {
            return Ok(());
        }

        if mutable {
            return Err(VerseError::check_at(
                format!("data specifiers are not supported on mutable binding `{name}`"),
                span,
            ));
        }

        if specifiers.iter().any(|specifier| specifier == "localizes")
            && !matches!(
                annotation.map(|annotation| &annotation.name),
                Some(TypeName::Message)
            )
        {
            return Err(VerseError::check_at(
                "`localizes` data specifier requires a `message` annotation",
                span,
            ));
        }

        Ok(())
    }

    fn record_current_module_member(
        &mut self,
        name: &str,
        value_type: Type,
        specifiers: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(module_name) = self.current_module_name() else {
            return Ok(());
        };
        if !self.current_definition_level() {
            return Ok(());
        }
        let access = access_level_from_specifiers(specifiers, "module member", span)?;
        if let Some(info) = self.module_types.get_mut(&module_name) {
            info.members.insert(name.to_string(), value_type);
            info.member_access.insert(name.to_string(), access);
        }
        Ok(())
    }

    fn record_current_module_member_access(
        &mut self,
        name: &str,
        specifiers: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(module_name) = self.current_module_name() else {
            return Ok(());
        };
        let access = access_level_from_specifiers(specifiers, "module member", span)?;
        if let Some(info) = self.module_types.get_mut(&module_name) {
            info.member_access.insert(name.to_string(), access);
        }
        Ok(())
    }

    fn check_module_body(
        &mut self,
        module_name: &str,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        let previous_module_path = std::mem::replace(
            &mut self.module_path,
            module_name.split('.').map(str::to_string).collect(),
        );
        self.push_scope();
        self.module_scope_depths.push(self.scopes.len());
        let result = (|| {
            self.predeclare_using_imports(statements)?;
            self.predeclare_aggregate_values_in_current_scope(statements)?;
            self.predeclare_extension_methods_in_current_scope(statements)?;
            self.predeclare_functions_in_current_scope(statements)?;
            self.check_statements(statements)?;
            Ok(())
        })();
        self.module_scope_depths.pop();
        self.pop_scope();
        self.module_path = previous_module_path;
        result
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        match &expr.kind {
            ExprKind::Number { kind, .. } => match kind {
                NumberKind::Int => Ok(Type::Int),
                NumberKind::Float => Ok(Type::Float),
            },
            ExprKind::Char { kind, .. } => match kind {
                CharacterKind::Char => Ok(Type::Char),
                CharacterKind::Char32 => Ok(Type::Char32),
            },
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::String(_) => Ok(Type::String),
            ExprKind::InterpolatedString(parts) => {
                for part in parts {
                    if let InterpolatedStringPart::Expr(expr) = part {
                        self.check_expr(expr)?;
                    }
                }
                Ok(Type::String)
            }
            ExprKind::None => Ok(Type::None),
            ExprKind::Ident(name) => self.check_ident(name, expr.span, false),
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::Unary { op, expr } => self.check_unary(*op, expr),
            ExprKind::Binary { op, .. } if is_failure_binary_op(*op) => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::Binary { left, op, right } => self.check_binary(left, *op, right),
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.push_scope();
                let then_result = (|| {
                    self.check_if_condition(condition)?;
                    self.check_expr(then_branch)
                })();
                self.pop_scope();
                let then_type = then_result?;

                let else_type = if let Some(else_branch) = else_branch {
                    self.check_expr(else_branch)?
                } else {
                    Type::None
                };

                unify_types(&then_type, &else_type, expr.span)
            }
            ExprKind::FailureBind { .. } | ExprKind::FailureSequence(_) => {
                Err(VerseError::check_at(
                    "failure binding is only valid in an `if` condition",
                    expr.span,
                ))
            }
            ExprKind::Set { target, op, expr } => {
                self.check_set_expression(target, *op, expr, false, expr.span)
            }
            ExprKind::Var {
                name,
                annotation,
                expr,
            } => self.check_var_expression(name, annotation, expr, false, expr.span),
            ExprKind::External => Ok(Type::Unknown),
            ExprKind::Case { subject, arms } => self.check_case(subject, arms, expr.span),
            ExprKind::Loop { body } => {
                if !loop_body_has_non_break_statement(body) {
                    return Err(VerseError::check_at(
                        "`loop` body must contain at least one non-break statement",
                        body.span,
                    ));
                }
                self.break_depth += 1;
                self.check_expr(body)?;
                self.break_depth -= 1;
                Ok(Type::None)
            }
            ExprKind::For { clauses, body } => self.check_for(clauses, body),
            ExprKind::Profile { description, body } => {
                let description_type = self.check_expr(description)?;
                self.ensure_expr_assignable(&Type::String, &description_type, description, || {
                    format!("profile description expected `string`, got `{description_type}`")
                })?;
                self.check_expr(body)
            }
            ExprKind::Spawn { body } => self.check_spawn(body),
            ExprKind::Concurrent { op, body } => {
                let value_type = self.check_concurrent(*op, body, expr.span)?;
                self.mark_async_expression();
                Ok(value_type)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.push_scope();
                let result = self.check_statements(statements);
                self.pop_scope();
                result
            }
            ExprKind::Function {
                params,
                effects,
                return_type,
                body,
            } => self.without_enclosing_failure_context(|checker| {
                checker.check_function(params, effects, return_type.as_ref(), body)
            }),
            ExprKind::Call { callee, args } => {
                let callee_type = self.check_callee_expr(callee)?;
                if let Type::ParametricType { name, kind, .. } = &callee_type {
                    let type_args = self.check_parametric_type_call_args(name, args, expr.span)?;
                    let instance = self.instantiate_parametric_type(name, &type_args, expr.span)?;
                    return match (kind, instance) {
                        (ParametricTypeKind::Struct, Type::Struct(name)) => {
                            Ok(Type::StructType(name))
                        }
                        (ParametricTypeKind::Class, Type::Class(name)) => Ok(Type::ClassType(name)),
                        (ParametricTypeKind::Interface, Type::Interface(name)) => {
                            Ok(Type::InterfaceType(name))
                        }
                        (_, other) => Err(VerseError::check_at(
                            format!("parametric type `{name}` produced `{other}`"),
                            expr.span,
                        )),
                    };
                }

                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.check_expr(call_arg_expr(arg))?);
                }

                if is_length_member_callee(callee)
                    && matches!(callee_type, Type::Int | Type::Unknown | Type::Any)
                {
                    self.check_call_arguments(
                        (Some(0), None),
                        Some(&[]),
                        None,
                        args,
                        &arg_types,
                        expr.span,
                    )?;
                    return Ok(Type::Int);
                }

                if is_shuffle_callee(callee) && is_shuffle_function_type(&callee_type) {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_shuffle_call(args, &arg_types, expr.span);
                }

                if is_concatenate_callee(callee) && is_concatenate_function_type(&callee_type) {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_concatenate_call(args, &arg_types, expr.span);
                }

                if is_make_classifiable_subset_callee(callee)
                    && is_make_classifiable_subset_function_type(&callee_type)
                {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_make_classifiable_subset_call(args, &arg_types, expr.span);
                }

                if is_make_result_callee(callee) {
                    return self.check_make_result_call(callee, args, &arg_types, expr.span);
                }

                match callee_type {
                    Type::Function {
                        arity,
                        arity_range,
                        effects,
                        mut param_types,
                        mut param_specs,
                        return_type,
                    } => {
                        if has_effect(&effects, "decides") {
                            return Err(VerseError::check_at(
                                "functions with `<decides>` must be called with `[]`",
                                expr.span,
                            ));
                        }
                        if self.in_failure_context() {
                            ensure_callable_in_failure_context(&effects, expr.span)?;
                        }
                        self.ensure_callable_in_async_context(&effects, expr.span)?;
                        self.ensure_current_function_allows_call_effects(&effects, expr.span)?;
                        let mut return_type = *return_type;
                        if let Some(inferred) =
                            infer_function_type_params(param_types.as_deref(), &arg_types)
                                .filter(|inferred| !inferred.is_empty())
                        {
                            self.ensure_inferred_type_param_constraints(
                                param_types.as_deref(),
                                &inferred,
                                expr.span,
                            )?;
                            if let Some(types) = param_types.as_mut() {
                                for value_type in types {
                                    *value_type = substitute_type_params(value_type, &inferred);
                                }
                            }
                            if let Some(specs) = param_specs.as_mut() {
                                for spec in specs {
                                    spec.value_type =
                                        substitute_type_params(&spec.value_type, &inferred);
                                }
                            }
                            return_type = self.substitute_type_params_runtime(
                                &return_type,
                                &inferred,
                                expr.span,
                            )?;
                        }
                        self.check_call_arguments(
                            (arity, arity_range),
                            param_types.as_deref(),
                            param_specs.as_deref(),
                            args,
                            &arg_types,
                            expr.span,
                        )?;
                        Ok(return_type)
                    }
                    Type::Overload(overloads) => self.check_overloaded_call(
                        &overloads,
                        false,
                        self.in_failure_context(),
                        args,
                        &arg_types,
                        expr.span,
                    ),
                    Type::Tuple(items) => {
                        self.check_tuple_access(&items, args, &arg_types, expr.span)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot call value of type `{other}`"),
                        callee.span,
                    )),
                }
            }
            ExprKind::BracketCall { callee, args } => self.check_bracket_call(expr, callee, args),
            ExprKind::Array(items) => {
                let mut item_type = Type::Unknown;
                let mut pending_empty_options = Vec::new();
                for item in items {
                    self.merge_collection_item_type(
                        &mut item_type,
                        &mut pending_empty_options,
                        item,
                    )?;
                }
                finalize_collection_item_type(&mut item_type, &mut pending_empty_options)?;
                Ok(Type::Array(Box::new(item_type)))
            }
            ExprKind::Map(entries) => {
                let mut key_type = Type::Unknown;
                let mut value_type = Type::Unknown;
                let mut pending_empty_keys = Vec::new();
                let mut pending_empty_values = Vec::new();
                for (key, value) in entries {
                    self.merge_collection_item_type(&mut key_type, &mut pending_empty_keys, key)?;
                    self.merge_collection_item_type(
                        &mut value_type,
                        &mut pending_empty_values,
                        value,
                    )?;
                }
                finalize_collection_item_type(&mut key_type, &mut pending_empty_keys)?;
                finalize_collection_item_type(&mut value_type, &mut pending_empty_values)?;
                if !entries.is_empty() {
                    ensure_comparable_key(&key_type, &self.struct_types, expr.span)?;
                }
                Ok(Type::Map(Box::new(key_type), Box::new(value_type)))
            }
            ExprKind::EnumDefinition { .. } => Err(VerseError::check_at(
                "enum definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::StructDefinition { .. } => Err(VerseError::check_at(
                "struct definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::ClassDefinition { .. } => Err(VerseError::check_at(
                "class definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::InterfaceDefinition { .. } => Err(VerseError::check_at(
                "interface definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::ModuleDefinition { .. } => Err(VerseError::check_at(
                "module definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::Archetype {
                callee, entries, ..
            } => self.check_archetype(callee, entries),
            ExprKind::Tuple(items) => {
                let mut item_types = Vec::with_capacity(items.len());
                for item in items {
                    item_types.push(self.check_expr(item)?);
                }
                Ok(Type::Tuple(item_types))
            }
            ExprKind::Option(value) => {
                let item_type = if let Some(value) = value {
                    self.check_failure_expr(value)?
                } else {
                    Type::Unknown
                };
                Ok(Type::Option(Box::new(item_type)))
            }
            ExprKind::UnwrapOption(_) => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, expr.span, false)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => self.check_qualified_member_expr(object, qualifier, name, expr.span, false),
            ExprKind::Member { object, name } => {
                self.check_member_expr(object, name, expr.span, false)
            }
            ExprKind::Index { collection, index } => {
                self.ensure_failable_expression_allowed(expr.span)?;
                let collection_type = self.check_expr(collection)?;
                let index_type = self.check_expr(index)?;
                match collection_type {
                    Type::Array(item) => {
                        ensure_int_index_type(&index_type, "array index", index.span)?;
                        Ok(*item)
                    }
                    Type::String => {
                        ensure_int_index_type(&index_type, "string index", index.span)?;
                        Ok(Type::Char)
                    }
                    Type::Map(key, value) | Type::WeakMap(key, value) => {
                        self.check_map_like_lookup(&key, &value, &index_type, index)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot index value of type `{other}`"),
                        collection.span,
                    )),
                }
            }
        }
    }

    fn check_callee_expr(&mut self, callee: &Expr) -> Result<Type, VerseError> {
        match &callee.kind {
            ExprKind::Ident(name) => self.check_ident(name, callee.span, true),
            ExprKind::Member { object, name } => {
                self.check_member_expr(object, name, callee.span, true)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => self.check_qualified_member_expr(object, qualifier, name, callee.span, true),
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, callee.span, true)
            }
            _ => self.check_expr(callee),
        }
    }

    fn check_failure_callee_expr(&mut self, callee: &Expr) -> Result<Type, VerseError> {
        match &callee.kind {
            ExprKind::Ident(name) => self.check_ident(name, callee.span, true),
            ExprKind::Member { object, name } => {
                self.check_failure_member_expr(object, name, callee.span, true)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => {
                self.check_failure_qualified_member_expr(object, qualifier, name, callee.span, true)
            }
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, callee.span, true)
            }
            _ if is_failable_condition_expr(callee) => self.check_failure_expr(callee),
            _ => self.check_expr(callee),
        }
    }

    fn check_ident(
        &self,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        let Some(symbol) = self.lookup_accessible(name, span)? else {
            if let Some(qualified) = self.resolve_parametric_type_reference(name)
                && let Some(info) = self.parametric_types.get(&qualified)
            {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::ParametricType {
                    name: qualified,
                    params: info.params.iter().map(|param| param.name.clone()).collect(),
                    kind: info.kind,
                });
            }
            return Err(VerseError::check_at(
                format!("undefined name `{name}`"),
                span,
            ));
        };
        if matches!(symbol.value_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded function `{name}` must be called"),
                span,
            ));
        }
        Ok(symbol.value_type)
    }

    fn check_qualified_name(
        &self,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        if qualifier != "super" {
            let Some(module_info) = self.module_types.get(qualifier) else {
                return Err(VerseError::check_at(
                    format!("unknown qualifier `{qualifier}`"),
                    span,
                ));
            };
            let Some(value_type) = module_info.members.get(name) else {
                return Err(VerseError::check_at(
                    format!("module `{qualifier}` has no member `{name}`"),
                    span,
                ));
            };
            let access = module_info
                .member_access
                .get(name)
                .copied()
                .unwrap_or(AccessLevel::Internal);
            self.ensure_module_member_accessible(qualifier, access, name, span)?;
            if matches!(value_type, Type::Overload(_)) && !allow_overload {
                return Err(VerseError::check_at(
                    format!("overloaded function `{name}` must be called"),
                    span,
                ));
            }
            return Ok(value_type.clone());
        }
        let Some(symbol) = self.lookup("super") else {
            return Err(VerseError::check_at("undefined name `super`", span));
        };
        let Type::ClassType(base_name) = symbol.value_type else {
            return Err(VerseError::check_at(
                "qualifier `super` is not a class type",
                span,
            ));
        };
        let Some(info) = self.struct_types.get(&base_name) else {
            return Err(VerseError::check_at(
                format!("unknown class `{base_name}`"),
                span,
            ));
        };
        let methods = info
            .methods
            .iter()
            .filter(|method| method.name == name)
            .collect::<Vec<_>>();
        if methods.is_empty() {
            return Err(VerseError::check_at(
                format!("class `{base_name}` has no method `{name}`"),
                span,
            ));
        }
        for method in &methods {
            let owner = method.owner.as_deref().unwrap_or(&base_name);
            self.ensure_aggregate_member_accessible(owner, method.access, name, "method", span)?;
        }
        let method_type = method_group_type(methods).expect("non-empty method group should type");
        if matches!(method_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded method `{name}` must be called"),
                span,
            ));
        }
        Ok(method_type)
    }

    fn check_member_expr(
        &mut self,
        object: &Expr,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let ExprKind::Ident(enum_name) = &object.kind
            && let Some(info) = self.enum_types.get(enum_name)
        {
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        let object_type = if is_failable_condition_expr(object) {
            self.ensure_failable_expression_allowed(span)?;
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_member(&object_type, name, span, allow_extension_method)
    }

    fn check_failure_member_expr(
        &mut self,
        object: &Expr,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let ExprKind::Ident(enum_name) = &object.kind
            && let Some(info) = self.enum_types.get(enum_name)
        {
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        let object_type = if is_failable_condition_expr(object) {
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_member(&object_type, name, span, allow_extension_method)
    }

    fn check_qualified_member_expr(
        &mut self,
        object: &Expr,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        let object_type = if is_failable_condition_expr(object) {
            self.ensure_failable_expression_allowed(span)?;
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_qualified_member(&object_type, qualifier, name, span, allow_overload)
    }

    fn check_failure_qualified_member_expr(
        &mut self,
        object: &Expr,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        let object_type = if is_failable_condition_expr(object) {
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_qualified_member(&object_type, qualifier, name, span, allow_overload)
    }

    fn check_qualified_member(
        &mut self,
        object_type: &Type,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.check_qualified_member(&supertype, qualifier, name, span, allow_overload);
        }

        let (label, aggregate_name, methods) = match object_type {
            Type::Class(class_name) => {
                let Some(info) = self.struct_types.get(class_name) else {
                    return Err(VerseError::check_at(
                        format!("unknown class `{class_name}`"),
                        span,
                    ));
                };
                ("class", class_name.as_str(), info.methods.as_slice())
            }
            Type::Interface(interface_name) => {
                let Some(info) = self.interface_types.get(interface_name) else {
                    return Err(VerseError::check_at(
                        format!("unknown interface `{interface_name}`"),
                        span,
                    ));
                };
                (
                    "interface",
                    interface_name.as_str(),
                    info.methods.as_slice(),
                )
            }
            other => {
                if let Some(method_type) =
                    self.qualified_extension_member_type(object_type, qualifier, name, span)?
                {
                    if !allow_overload {
                        return Err(VerseError::check_at(
                            format!("extension method `({qualifier}:){name}` must be called"),
                            span,
                        ));
                    }
                    return Ok(method_type);
                }
                return Err(VerseError::check_at(
                    format!("cannot use qualified member on type `{other}`"),
                    span,
                ));
            }
        };

        let methods = methods
            .iter()
            .filter(|method| method.name == name && method_has_qualifier(method, qualifier))
            .collect::<Vec<_>>();
        if methods.is_empty() {
            if let Some(method_type) =
                self.qualified_extension_member_type(object_type, qualifier, name, span)?
            {
                if !allow_overload {
                    return Err(VerseError::check_at(
                        format!("extension method `({qualifier}:){name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            return Err(VerseError::check_at(
                format!("{label} `{aggregate_name}` has no method `({qualifier}:){name}`"),
                span,
            ));
        }
        for method in &methods {
            let owner = method.owner.as_deref().unwrap_or(aggregate_name);
            self.ensure_aggregate_member_accessible(owner, method.access, name, "method", span)?;
        }
        let method_type = method_group_type(methods).expect("non-empty method group should type");
        if matches!(method_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded method `({qualifier}:){name}` must be called"),
                span,
            ));
        }
        Ok(method_type)
    }

    fn check_assignment_target(&mut self, target: &Expr) -> Result<Type, VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_accessible(name, target.span)? else {
                    return Err(VerseError::check_at(
                        format!("undefined name `{name}`"),
                        target.span,
                    ));
                };
                if !symbol.mutable {
                    return Err(VerseError::check_at(
                        format!("cannot assign to immutable binding `{name}`"),
                        target.span,
                    ));
                }
                Ok(symbol.value_type)
            }
            ExprKind::Index { collection, index } => {
                let collection_type = self.check_assignment_target(collection)?;
                let index_type = self.check_expr(index)?;
                match collection_type {
                    Type::Array(item) => {
                        ensure_int_index_type(&index_type, "array index", index.span)?;
                        Ok(*item)
                    }
                    Type::String => {
                        ensure_int_index_type(&index_type, "string index", index.span)?;
                        Ok(Type::Char)
                    }
                    Type::Map(key, value) | Type::WeakMap(key, value) => {
                        self.check_map_like_lookup(&key, &value, &index_type, index)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot index value of type `{other}`"),
                        collection.span,
                    )),
                }
            }
            ExprKind::Member { object, name } => {
                let object_type = self.check_expr(object)?;
                let field = match &object_type {
                    Type::Struct(struct_name) => {
                        self.check_assignment_target(object)?;
                        let Some(info) = self.struct_types.get(struct_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown struct `{struct_name}`"),
                                target.span,
                            ));
                        };
                        if !info.computes {
                            return Err(VerseError::check_at(
                                format!(
                                    "struct `{struct_name}` must be `<computes>` to mutate fields"
                                ),
                                target.span,
                            ));
                        }
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("struct `{struct_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })?
                    }
                    Type::Class(class_name) => {
                        let Some(info) = self.struct_types.get(class_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown class `{class_name}`"),
                                target.span,
                            ));
                        };
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("class `{class_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })
                            .and_then(|field| {
                                let owner = field.owner.as_deref().unwrap_or(class_name);
                                self.ensure_aggregate_member_accessible(
                                    owner,
                                    field.mutation_access,
                                    name,
                                    "field",
                                    target.span,
                                )?;
                                Ok(field)
                            })?
                    }
                    Type::Interface(interface_name) => {
                        let Some(info) = self.interface_types.get(interface_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown interface `{interface_name}`"),
                                target.span,
                            ));
                        };
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("interface `{interface_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })
                            .and_then(|field| {
                                let owner = field.owner.as_deref().unwrap_or(interface_name);
                                self.ensure_aggregate_member_accessible(
                                    owner,
                                    field.mutation_access,
                                    name,
                                    "field",
                                    target.span,
                                )?;
                                Ok(field)
                            })?
                    }
                    _ => {
                        return Err(VerseError::check_at(
                            format!("cannot assign to member `{name}` on type `{object_type}`"),
                            target.span,
                        ));
                    }
                };
                if !matches!(object_type, Type::Struct(_)) && !field.mutable {
                    return Err(VerseError::check_at(
                        format!("cannot assign to immutable field `{name}`"),
                        target.span,
                    ));
                }

                Ok(field.value_type.clone())
            }
            _ => Err(VerseError::check_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn check_set_expression(
        &mut self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        failure_context: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !failure_context && assignment_target_has_failable_expr(target) {
            self.ensure_failable_expression_allowed(target.span)?;
        }
        let target_type = self.check_assignment_target(target)?;
        self.ensure_current_function_allows_mutation(span)?;
        let expr_type = if failure_context {
            self.check_failure_expr(expr)?
        } else {
            self.check_expr(expr)?
        };
        match op {
            AssignOp::Assign => {
                self.ensure_expr_assignable(&target_type, &expr_type, expr, || {
                    format!("cannot assign `{expr_type}` to target of type `{target_type}`")
                })?;
            }
            AssignOp::AddAssign => {
                let result_type = check_add(&target_type, target.span, &expr_type, expr.span)?;
                self.ensure_assignable(&target_type, &result_type, expr.span, || {
                    format!(
                        "cannot assign `+=` result `{result_type}` to target of type `{target_type}`"
                    )
                })?;
            }
            AssignOp::SubAssign | AssignOp::MulAssign | AssignOp::DivAssign => {
                ensure_number_like(&target_type, "assignment target", target.span)?;
                ensure_number_like(&expr_type, "assignment value", expr.span)?;
                let result_type = if op == AssignOp::DivAssign {
                    divide_numeric_type(&target_type, &expr_type)
                } else {
                    unify_numeric_types(&target_type, &expr_type)
                };
                self.ensure_assignable(&target_type, &result_type, expr.span, || {
                    format!(
                        "cannot assign compound result `{result_type}` to target of type `{target_type}`"
                    )
                })?;
            }
        }
        Ok(Type::None)
    }

    fn check_map_like_lookup(
        &mut self,
        key: &Type,
        value: &Type,
        index_type: &Type,
        index: &Expr,
    ) -> Result<Type, VerseError> {
        self.ensure_expr_assignable(key, index_type, index, || {
            format!("map key expects `{key}`, got `{index_type}`")
        })?;
        Ok(value.clone())
    }

    fn iter_item_type(&mut self, iterable: &Expr) -> Result<Type, VerseError> {
        let iterable_type = self.with_range_context(|checker| checker.check_expr(iterable))?;
        match iterable_type {
            Type::Range => Ok(Type::Int),
            Type::Array(item) => Ok(*item),
            Type::Map(_, value) => Ok(*value),
            Type::Generator(Some(item)) => Ok(*item),
            Type::Generator(None) => Ok(Type::Unknown),
            Type::String => Ok(Type::Char),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot iterate over value of type `{other}`"),
                iterable.span,
            )),
        }
    }

    fn iter_binding_types(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
    ) -> Result<Vec<(String, Type)>, VerseError> {
        match binding {
            ForBinding::Value(name) => Ok(vec![(name.clone(), self.iter_item_type(iterable)?)]),
            ForBinding::Pair { key, value } => {
                match self.with_range_context(|checker| checker.check_expr(iterable))? {
                    Type::Array(item) => Ok(vec![(key.clone(), Type::Int), (value.clone(), *item)]),
                    Type::Map(key_type, value_type) => {
                        Ok(vec![(key.clone(), *key_type), (value.clone(), *value_type)])
                    }
                    Type::Generator(_) => Err(VerseError::check_at(
                        "`->` pair iteration is not supported for `generator`",
                        iterable.span,
                    )),
                    Type::Unknown | Type::Any => Ok(vec![
                        (key.clone(), Type::Unknown),
                        (value.clone(), Type::Unknown),
                    ]),
                    other => Err(VerseError::check_at(
                        format!("cannot use `->` for iteration over type `{other}`"),
                        iterable.span,
                    )),
                }
            }
        }
    }

    fn check_for(&mut self, clauses: &[ForClause], body: &Expr) -> Result<Type, VerseError> {
        self.push_scope();
        let result = (|| {
            for clause in clauses {
                match clause {
                    ForClause::Generator {
                        binding, iterable, ..
                    } => {
                        for (name, item_type) in self.iter_binding_types(binding, iterable)? {
                            self.define(&name, item_type, false, iterable.span)?;
                        }
                    }
                    ForClause::RangeOrLet { name, expr, span } => {
                        let expr_type =
                            self.with_range_context(|checker| checker.check_failure_expr(expr))?;
                        if expr_type == Type::Range {
                            self.define(name, Type::Int, false, *span)?;
                        } else {
                            self.define(name, expr_type, false, *span)?;
                        }
                    }
                    ForClause::Let { name, expr, span } => {
                        let expr_type = self.check_failure_expr(expr)?;
                        self.define(name, expr_type, false, *span)?;
                    }
                    ForClause::Filter(expr) => {
                        self.check_failure_expr(expr)?;
                        if !failure_condition_has_failable_expr(expr) {
                            return Err(VerseError::check_at(
                                "for filter must contain at least one failable expression",
                                expr.span,
                            ));
                        }
                    }
                }
            }

            let item_type = self.check_expr(body)?;
            Ok(Type::Array(Box::new(item_type)))
        })();
        self.pop_scope();
        result
    }

    fn check_spawn(&mut self, body: &Expr) -> Result<Type, VerseError> {
        if self.data_member_default_depth > 0 {
            return Err(VerseError::check_at(
                "data-member default value cannot contain `spawn` expressions",
                body.span,
            ));
        }

        self.push_scope();
        let result = self.with_suppressed_async_expr_marker(|checker| {
            let spawned_expr = spawn_body_expr(body)?;
            let ExprKind::Call { callee, args } = &spawned_expr.kind else {
                return Err(VerseError::check_at(
                    "`spawn` body must be a single async function call",
                    spawned_expr.span,
                ));
            };

            let return_type = checker.check_spawn_call(callee, args, spawned_expr.span)?;
            Ok(Type::Task(Box::new(return_type)))
        });
        self.pop_scope();
        result
    }

    fn check_spawn_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Type, VerseError> {
        let callee_type = self.check_callee_expr(callee)?;
        let arg_types = args
            .iter()
            .map(|arg| self.check_expr(call_arg_expr(arg)))
            .collect::<Result<Vec<_>, _>>()?;

        match callee_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        span,
                    ));
                }
                if !has_effect(&effects, "suspends") {
                    return Err(VerseError::check_at(
                        "`spawn` body must call a function with `<suspends>` effect",
                        span,
                    ));
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    &arg_types,
                    span,
                )?;
                Ok(*return_type)
            }
            Type::Overload(overloads) => {
                self.check_spawn_overloaded_call(&overloads, args, &arg_types, span)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot spawn value of type `{other}`"),
                callee.span,
            )),
        }
    }

    fn check_spawn_overloaded_call(
        &mut self,
        overloads: &[Type],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut saw_matching_immediate = false;
        for overload in overloads {
            let Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } = overload
            else {
                continue;
            };

            if has_effect(effects, "decides") {
                continue;
            }

            if self
                .check_call_arguments(
                    (*arity, *arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    arg_types,
                    span,
                )
                .is_err()
            {
                continue;
            }

            if !has_effect(effects, "suspends") {
                saw_matching_immediate = true;
                continue;
            }

            return Ok(return_type.as_ref().clone());
        }

        if saw_matching_immediate {
            return Err(VerseError::check_at(
                "`spawn` body must call a function with `<suspends>` effect",
                span,
            ));
        }

        Err(VerseError::check_at(
            format!(
                "no overload matches spawn call with argument types ({})",
                render_type_list(arg_types)
            ),
            span,
        ))
    }

    fn check_concurrent(
        &mut self,
        op: ConcurrentOp,
        body: &Expr,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !self.current_function_has_effect("suspends") {
            return Err(VerseError::check_at(
                format!(
                    "`{}` expression requires an async context",
                    concurrent_op_name(op)
                ),
                span,
            ));
        }

        let statements = concurrent_body_statements(body)?;
        let minimum = if matches!(op, ConcurrentOp::Branch) {
            1
        } else {
            2
        };
        if statements.len() < minimum {
            return Err(VerseError::check_at(
                format!(
                    "`{}` expression expects at least {minimum} subexpressions",
                    concurrent_op_name(op)
                ),
                body.span,
            ));
        }

        self.push_scope();
        let result = (|| {
            let mut item_types = Vec::with_capacity(statements.len());
            for statement in statements {
                self.push_scope();
                self.push_async_expr_marker();
                let item_type = self.check_stmt(statement);
                let has_async_expr = self.pop_async_expr_marker();
                self.pop_scope();
                let item_type = item_type?;
                if !has_async_expr {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}` branch must contain an async expression",
                            concurrent_op_name(op)
                        ),
                        statement.span,
                    ));
                }
                item_types.push(item_type);
            }

            match op {
                ConcurrentOp::Sync => Ok(Type::Tuple(item_types)),
                ConcurrentOp::Race | ConcurrentOp::Rush => {
                    let mut result_type = Type::Unknown;
                    for item_type in item_types {
                        result_type = unify_types(&result_type, &item_type, span)?;
                    }
                    Ok(result_type)
                }
                ConcurrentOp::Branch => Ok(Type::None),
            }
        })();
        self.pop_scope();
        result
    }

    fn check_function(
        &mut self,
        params: &[Param],
        effects: &[String],
        return_type: Option<&TypeAnnotation>,
        body: &Expr,
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(effects, body.span)?;
        let type_params = collect_function_type_params(params)?;
        self.validate_type_parameter_constraints(&type_params, body.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            let checked_return = self.annotation_to_type(return_type)?;
            self.push_scope();
            self.function_returns.push(checked_return.clone());
            self.function_effects.push(effects.to_vec());
            let body_type = (|| {
                for param in params {
                    let param_type = self.annotation_to_type(param.annotation.as_ref())?;
                    self.define_param_pattern(param, &param_type)?;
                }

                if has_effect(effects, "decides") {
                    self.check_failure_expr(body)
                } else {
                    self.check_expr(body)
                }
            })();
            self.function_returns.pop();
            self.function_effects.pop();
            self.pop_scope();
            let body_type = body_type?;

            let checked_return = if let Some(return_type) = return_type {
                let expected = self.type_name_to_type(return_type)?;
                if expected == Type::None && has_effect(effects, "decides") {
                    expected
                } else if body_type != Type::Never {
                    self.ensure_expr_assignable(&expected, &body_type, body, || {
                        format!(
                            "function is annotated to return `{expected}` but body has type `{body_type}`"
                        )
                    })?;
                    expected
                } else {
                    expected
                }
            } else {
                body_type
            };

            if has_effect(effects, "localizes") && checked_return != Type::Message {
                return Err(VerseError::check_at(
                    "`localizes` function specifier requires a `message` return type",
                    body.span,
                ));
            }

            let param_types = self.param_types(params)?;
            let param_specs = self.param_specs(params)?;
            Ok(Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.to_vec(),
                param_types: Some(param_types),
                param_specs: Some(param_specs),
                return_type: Box::new(checked_return),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    fn define_param_pattern(&mut self, param: &Param, value_type: &Type) -> Result<(), VerseError> {
        match &param.pattern {
            ParamPattern::Binding => {
                if let Some(default) = &param.default {
                    let default_type = self.check_expr(default)?;
                    self.ensure_expr_assignable(value_type, &default_type, default, || {
                        format!(
                            "default value for parameter `{}` must be `{value_type}`, got `{default_type}`",
                            param.name
                        )
                    })?;
                }
                self.define(&param.name, value_type.clone(), false, param.span)
            }
            ParamPattern::Anonymous => Ok(()),
            ParamPattern::Tuple(params) => match value_type {
                Type::Tuple(items) if items.len() == params.len() => {
                    for (param, item_type) in params.iter().zip(items) {
                        self.define_param_pattern(param, item_type)?;
                    }
                    Ok(())
                }
                Type::Unknown | Type::Any => {
                    for param in params {
                        let item_type = self.annotation_to_type(param.annotation.as_ref())?;
                        self.define_param_pattern(param, &item_type)?;
                    }
                    Ok(())
                }
                Type::Tuple(items) => Err(VerseError::check_at(
                    format!(
                        "destructured tuple parameter expected {} elements, got {}",
                        params.len(),
                        items.len()
                    ),
                    param.span,
                )),
                other => Err(VerseError::check_at(
                    format!("destructured tuple parameter requires a tuple type, got `{other}`"),
                    param.span,
                )),
            },
        }
    }

    fn check_call_arguments(
        &mut self,
        arity_spec: (Option<usize>, Option<(usize, usize)>),
        param_types: Option<&[Type]>,
        param_specs: Option<&[ParamSpec]>,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        let (arity, arity_range) = arity_spec;
        if let Some(param_specs) = param_specs {
            return self.check_spec_call_arguments(param_specs, args, arg_types, span);
        }

        if args.iter().any(CallArg::is_named) {
            return Err(VerseError::check_at(
                "named arguments are not supported for this callable",
                span,
            ));
        }

        let positional_args = args.iter().map(call_arg_expr).collect::<Vec<_>>();

        let Some(param_types) = param_types else {
            if let Some(expected) = arity
                && expected != args.len()
            {
                return Err(VerseError::check_at(
                    format!("expected {expected} arguments, got {}", args.len()),
                    span,
                ));
            }
            if let Some((min, max)) = arity_range
                && !(min..=max).contains(&args.len())
            {
                return Err(VerseError::check_at(
                    format!("expected {min}..={max} arguments, got {}", args.len()),
                    span,
                ));
            }
            return Ok(());
        };

        if let Some((min, max)) = arity_range
            && let [param_type] = param_types
        {
            if !(min..=max).contains(&args.len()) {
                return Err(VerseError::check_at(
                    format!("expected {min}..={max} arguments, got {}", args.len()),
                    span,
                ));
            }
            for (index, (arg, arg_type)) in positional_args.iter().zip(arg_types).enumerate() {
                self.ensure_expr_assignable(param_type, arg_type, arg, || {
                    format!(
                        "argument {} expected `{param_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if let [Type::Array(item_type)] = param_types {
            if args.len() == 1
                && self.expr_is_assignable_to_expected(
                    &param_types[0],
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }

            for (index, (arg, arg_type)) in positional_args.iter().zip(arg_types).enumerate() {
                self.ensure_expr_assignable(item_type, arg_type, arg, || {
                    format!(
                        "array argument item {} expected `{item_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if let [Type::Tuple(items)] = param_types {
            if args.len() == 1
                && self.expr_is_assignable_to_expected(
                    &param_types[0],
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }

            if args.len() != items.len() {
                return Err(VerseError::check_at(
                    format!(
                        "expected {} arguments for tuple parameter, got {}",
                        items.len(),
                        args.len()
                    ),
                    span,
                ));
            }

            for (index, ((arg, arg_type), item_type)) in
                positional_args.iter().zip(arg_types).zip(items).enumerate()
            {
                self.ensure_expr_assignable(item_type, arg_type, arg, || {
                    format!(
                        "tuple argument item {} expected `{item_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        let expanded_tuple = match arg_types {
            [Type::Tuple(items)] if items.len() == param_types.len() => Some(items.as_slice()),
            _ => None,
        };

        if let Some(items) = expanded_tuple {
            for (index, (expected, actual)) in param_types.iter().zip(items).enumerate() {
                self.ensure_expr_assignable(expected, actual, positional_args[0], || {
                    format!(
                        "argument {} expected `{expected}`, got `{actual}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if args.len() != param_types.len() {
            return Err(VerseError::check_at(
                format!(
                    "expected {} arguments, got {}",
                    param_types.len(),
                    args.len()
                ),
                span,
            ));
        }

        for (index, ((arg, arg_type), param_type)) in positional_args
            .iter()
            .zip(arg_types)
            .zip(param_types)
            .enumerate()
        {
            self.ensure_expr_assignable(param_type, arg_type, arg, || {
                format!(
                    "argument {} expected `{param_type}`, got `{arg_type}`",
                    index + 1
                )
            })?;
        }

        Ok(())
    }

    fn check_overloaded_call(
        &mut self,
        overloads: &[Type],
        require_decides: bool,
        require_rollback: bool,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut best_match = None;
        let mut saw_defer_suspends_match = false;
        let mut saw_suspends_match = false;
        let mut saw_no_rollback_match = false;
        let mut saw_effect_mismatch = None;

        for overload in overloads {
            let Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } = overload
            else {
                continue;
            };

            if has_effect(effects, "decides") != require_decides {
                continue;
            }

            if self
                .check_call_arguments(
                    (*arity, *arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    arg_types,
                    span,
                )
                .is_err()
            {
                continue;
            }

            let has_suspends_effect = has_effect(effects, "suspends");
            if has_suspends_effect {
                if self.defer_depth > 0 {
                    saw_defer_suspends_match = true;
                    continue;
                }
                if !self.current_function_has_effect("suspends") {
                    saw_suspends_match = true;
                    continue;
                }
            }

            if require_rollback && has_no_rollback_effect(effects) {
                saw_no_rollback_match = true;
                continue;
            }

            if let Err(error) = self.ensure_current_function_allows_call_effects(effects, span) {
                saw_effect_mismatch = Some(error);
                continue;
            }

            let score = self.overload_match_score(
                param_types.as_deref(),
                param_specs.as_deref(),
                args,
                arg_types,
            );
            if best_match
                .as_ref()
                .is_none_or(|(best_score, _, _)| score < *best_score)
            {
                best_match = Some((score, return_type.as_ref().clone(), has_suspends_effect));
            }
        }

        if let Some((_, return_type, has_suspends_effect)) = best_match {
            if has_suspends_effect {
                self.mark_async_expression();
            }
            return Ok(return_type);
        }

        if saw_defer_suspends_match {
            return Err(VerseError::check_at(
                "`defer` block cannot contain suspend expressions",
                span,
            ));
        }

        if saw_suspends_match {
            return Err(VerseError::check_at(
                "function with `<suspends>` effect can only be called in an async context",
                span,
            ));
        }

        if saw_no_rollback_match {
            return Err(VerseError::check_at(
                "function with `<no_rollback>` effect cannot be called in a failure context",
                span,
            ));
        }

        if let Some(error) = saw_effect_mismatch {
            return Err(error);
        }

        let call_style = if require_decides { "[]" } else { "()" };
        Err(VerseError::check_at(
            format!(
                "no overload matches {call_style} call with argument types ({})",
                render_type_list(arg_types)
            ),
            span,
        ))
    }

    fn overload_match_score(
        &self,
        param_types: Option<&[Type]>,
        param_specs: Option<&[ParamSpec]>,
        args: &[CallArg],
        arg_types: &[Type],
    ) -> usize {
        if let Some(param_specs) = param_specs
            && args.iter().any(CallArg::is_named)
        {
            return self
                .spec_overload_match_score(param_specs, args, arg_types)
                .unwrap_or(usize::MAX / 2);
        }

        if args.iter().any(CallArg::is_named) {
            return usize::MAX;
        }

        let Some(param_types) = param_types else {
            return usize::MAX / 2;
        };

        if param_types.len() != arg_types.len() {
            return usize::MAX / 2;
        }

        param_types
            .iter()
            .zip(arg_types)
            .map(|(expected, actual)| {
                self.type_match_score(expected, actual)
                    .unwrap_or(usize::MAX / 4)
            })
            .sum()
    }

    fn spec_overload_match_score(
        &self,
        param_specs: &[ParamSpec],
        args: &[CallArg],
        arg_types: &[Type],
    ) -> Option<usize> {
        let mut assigned = vec![false; param_specs.len()];
        let mut positional_index = 0usize;
        let mut score = 0usize;

        for (arg, arg_type) in args.iter().zip(arg_types) {
            match arg {
                CallArg::Positional(_) => {
                    let (param_index, param) = param_specs
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)?;
                    positional_index = param_index + 1;
                    if assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    score += self.type_match_score(&param.value_type, arg_type)?;
                }
                CallArg::Named { name, optional, .. } => {
                    let (param_index, param) = param_specs
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == *name)?;
                    if (*optional && !param.named) || assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    score += self.type_match_score(&param.value_type, arg_type)?;
                }
            }
        }

        for (index, param) in param_specs.iter().enumerate() {
            if !assigned[index] && !param.has_default {
                return None;
            }
        }

        Some(score)
    }

    fn type_match_score(&self, expected: &Type, actual: &Type) -> Option<usize> {
        if expected == actual {
            return Some(0);
        }

        match (expected, actual) {
            (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown | Type::Never) => {
                Some(50)
            }
            (Type::Message, Type::String) => Some(1),
            (Type::Array(expected), Type::String) if is_string_char_type(expected) => Some(1),
            (Type::String, Type::Array(actual)) if is_string_char_type(actual) => Some(1),
            (Type::Class(expected), Type::Class(actual))
                if self.is_class_subtype(actual, expected) =>
            {
                Some(1)
            }
            (Type::Array(expected), Type::Array(actual)) => self
                .type_match_score(expected, actual)
                .map(|score| score + 1),
            (
                Type::Map(expected_key, expected_value)
                | Type::WeakMap(expected_key, expected_value),
                Type::Map(actual_key, actual_value) | Type::WeakMap(actual_key, actual_value),
            ) => Some(
                self.type_match_score(expected_key, actual_key)?
                    + self.type_match_score(expected_value, actual_value)?
                    + 1,
            ),
            (
                Type::Result(expected_success, expected_error),
                Type::Result(actual_success, actual_error),
            ) => Some(
                self.type_match_score(expected_success, actual_success)?
                    + self.type_match_score(expected_error, actual_error)?
                    + 1,
            ),
            (Type::Tuple(expected_items), Type::Tuple(actual_items))
                if expected_items.len() == actual_items.len() =>
            {
                expected_items
                    .iter()
                    .zip(actual_items)
                    .try_fold(1, |score, (expected, actual)| {
                        Some(score + self.type_match_score(expected, actual)?)
                    })
            }
            (Type::Option(expected), Type::Option(actual)) => self
                .type_match_score(expected, actual)
                .map(|score| score + 1),
            _ if self.is_assignable(expected, actual) => Some(10),
            _ => None,
        }
    }

    fn check_spec_call_arguments(
        &mut self,
        param_specs: &[ParamSpec],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        if let [param] = param_specs
            && let Some(tuple_items) = param.tuple_items.as_deref()
            && tuple_param_specs_have_named_or_default(tuple_items)
        {
            let positional_args = args.iter().map(call_arg_expr).collect::<Vec<_>>();
            if args.len() == 1
                && !args[0].is_named()
                && self.expr_is_assignable_to_expected(
                    &param.value_type,
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }
            return self.check_spec_call_arguments(tuple_items, args, arg_types, span);
        }

        if args.iter().all(|arg| !arg.is_named()) && param_specs.iter().all(|param| !param.named) {
            let param_types = param_specs
                .iter()
                .map(|param| param.value_type.clone())
                .collect::<Vec<_>>();
            return self.check_call_arguments(
                (Some(param_specs.len()), None),
                Some(&param_types),
                None,
                args,
                arg_types,
                span,
            );
        }

        let mut assigned = vec![false; param_specs.len()];
        let mut positional_index = 0usize;

        for (arg, arg_type) in args.iter().zip(arg_types) {
            match arg {
                CallArg::Positional(expr) => {
                    let Some((param_index, param)) = param_specs
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)
                    else {
                        return Err(VerseError::check_at(
                            "positional argument does not match any positional parameter",
                            expr.span,
                        ));
                    };
                    positional_index = param_index + 1;
                    assigned[param_index] = true;
                    self.ensure_expr_assignable(&param.value_type, arg_type, expr, || {
                        format!(
                            "argument `{}` expected `{}`, got `{arg_type}`",
                            param.name, param.value_type
                        )
                    })?;
                }
                CallArg::Named {
                    name,
                    expr,
                    optional,
                    span,
                } => {
                    let Some((param_index, param)) = param_specs
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == *name)
                    else {
                        let rendered = rendered_argument_name(name, *optional);
                        return Err(VerseError::check_at(
                            format!("unknown named argument `{rendered}`"),
                            *span,
                        ));
                    };
                    if *optional && !param.named {
                        return Err(VerseError::check_at(
                            format!("parameter `{name}` is not a named parameter"),
                            *span,
                        ));
                    }
                    if assigned[param_index] {
                        let rendered = rendered_argument_name(name, *optional);
                        return Err(VerseError::check_at(
                            format!("duplicate argument for parameter `{rendered}`"),
                            *span,
                        ));
                    }
                    assigned[param_index] = true;
                    let rendered = rendered_argument_name(&param.name, *optional);
                    self.ensure_expr_assignable(&param.value_type, arg_type, expr, || {
                        format!(
                            "argument `{rendered}` expected `{}`, got `{arg_type}`",
                            param.value_type
                        )
                    })?;
                }
            }
        }

        for (param, assigned) in param_specs.iter().zip(assigned) {
            if !assigned && !param.has_default {
                return Err(VerseError::check_at(
                    format!("missing required argument `{}`", rendered_param_name(param)),
                    span,
                ));
            }
        }

        Ok(())
    }

    fn check_tuple_access(
        &self,
        items: &[Type],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        if args.iter().any(CallArg::is_named) {
            return Err(VerseError::check_at(
                "tuple index does not accept named arguments",
                span,
            ));
        }

        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("tuple index expects 1 argument, got {}", args.len()),
                span,
            ));
        }

        let arg = call_arg_expr(&args[0]);
        ensure_int_index_type(&arg_types[0], "tuple index", arg.span)?;

        if let Some(index) = tuple_index_literal(arg) {
            if index < 0 {
                return Err(VerseError::check_at(
                    format!("tuple index must be a non-negative integer, got {index}"),
                    arg.span,
                ));
            }
            let Ok(index) = usize::try_from(index) else {
                return Err(VerseError::check_at(
                    "tuple index is outside the supported index range",
                    arg.span,
                ));
            };
            return items.get(index).cloned().ok_or_else(|| {
                VerseError::check_at(
                    format!(
                        "tuple index {index} out of bounds for length {}",
                        items.len()
                    ),
                    arg.span,
                )
            });
        }

        items.iter().try_fold(Type::Unknown, |current, item| {
            unify_types(&current, item, arg.span)
        })
    }

    fn check_archetype(
        &mut self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
    ) -> Result<Type, VerseError> {
        if is_official_event_archetype_callee(callee) {
            return self.check_event_archetype(callee, entries);
        }

        let callee_type = self.check_callee_expr(callee)?;
        let (aggregate_name, result_type, expected_kind, label) = match callee_type {
            Type::StructType(name) => (
                name.clone(),
                Type::Struct(name),
                AggregateKind::Struct,
                "struct",
            ),
            Type::ClassType(name) => (
                name.clone(),
                Type::Class(name),
                AggregateKind::Class,
                "class",
            ),
            other => {
                return Err(VerseError::check_at(
                    format!("cannot construct value from type `{other}`"),
                    callee.span,
                ));
            }
        };
        self.ensure_data_member_default_archetype_not_recursive(&aggregate_name, callee.span)?;

        let Some(info) = self.struct_types.get(&aggregate_name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown {label} `{aggregate_name}`"),
                callee.span,
            ));
        };
        if info.kind != expected_kind {
            return Err(VerseError::check_at(
                format!("unknown {label} `{aggregate_name}`"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class && info.abstract_class {
            return Err(VerseError::check_at(
                format!("abstract class `{aggregate_name}` cannot be instantiated"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class && info.epic_internal_class {
            return Err(VerseError::check_at(
                format!("epic_internal class `{aggregate_name}` cannot be instantiated"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class && info.unique {
            self.ensure_current_function_allows_allocation(callee.span)?;
        }

        self.push_scope();
        let result = (|| {
            let mut provided = vec![false; info.fields.len()];
            let mut direct_fields = vec![false; info.fields.len()];
            let mut constructor_delegation_seen = false;
            for entry in entries {
                match entry {
                    ArchetypeEntry::Let(binding) => {
                        self.check_archetype_let(binding)?;
                    }
                    ArchetypeEntry::Block(body) => {
                        self.check_expr(body)?;
                    }
                    ArchetypeEntry::Field(field) => {
                        if info.kind == AggregateKind::Class && constructor_delegation_seen {
                            return Err(VerseError::check_at(
                                format!(
                                    "field initializer `{}` cannot appear after constructor delegation",
                                    field.name
                                ),
                                field.span,
                            ));
                        }

                        let Some((index, expected)) = info
                            .fields
                            .iter()
                            .enumerate()
                            .find(|(_, candidate)| candidate.name == field.name)
                        else {
                            return Err(VerseError::check_at(
                                format!("{label} `{aggregate_name}` has no field `{}`", field.name),
                                field.span,
                            ));
                        };

                        if info.kind == AggregateKind::Class {
                            let owner = expected.owner.as_deref().unwrap_or(&aggregate_name);
                            self.ensure_aggregate_member_accessible(
                                owner,
                                expected.access,
                                &field.name,
                                "field",
                                field.span,
                            )?;
                        }

                        if info.kind == AggregateKind::Class && expected.final_member {
                            return Err(VerseError::check_at(
                                format!(
                                    "final field `{}` cannot be overridden by an archetype",
                                    field.name
                                ),
                                field.span,
                            ));
                        }

                        if direct_fields[index] {
                            return Err(VerseError::check_at(
                                format!("duplicate value for field `{}`", field.name),
                                field.span,
                            ));
                        }
                        direct_fields[index] = true;
                        provided[index] = true;

                        let actual = self.check_expr(&field.expr)?;
                        self.ensure_expr_assignable(
                            &expected.value_type,
                            &actual,
                            &field.expr,
                            || {
                                format!(
                                    "field `{}` expected `{}`, got `{actual}`",
                                    field.name, expected.value_type
                                )
                            },
                        )?;
                    }
                    ArchetypeEntry::ConstructorCall(call) => {
                        if info.kind != AggregateKind::Class {
                            return Err(VerseError::check_at(
                                "constructor delegation is only valid in class archetypes",
                                call.span,
                            ));
                        }
                        constructor_delegation_seen = true;

                        let delegated_class = self.check_archetype_constructor_call(call)?;
                        let same_class = delegated_class == aggregate_name;
                        if !same_class
                            && !class_is_subtype_of(
                                &aggregate_name,
                                &delegated_class,
                                &self.struct_types,
                            )
                        {
                            return Err(VerseError::check_at(
                                format!(
                                    "constructor `{}` returns `{delegated_class}`, which is not `{aggregate_name}` or a superclass",
                                    call.name
                                ),
                                call.span,
                            ));
                        }

                        if same_class {
                            provided.fill(false);
                            direct_fields.fill(false);
                        }

                        if let Some(delegated_info) = self.struct_types.get(&delegated_class) {
                            for delegated_field in &delegated_info.fields {
                                if let Some(index) = info
                                    .fields
                                    .iter()
                                    .position(|field| field.name == delegated_field.name)
                                {
                                    provided[index] = true;
                                }
                            }
                        }
                    }
                }
            }

            for (field, provided) in info.fields.iter().zip(provided) {
                if !provided && !field.has_default {
                    return Err(VerseError::check_at(
                        format!(
                            "missing required field `{}` for `{aggregate_name}`",
                            field.name
                        ),
                        callee.span,
                    ));
                }
            }

            Ok(result_type)
        })();
        self.pop_scope();
        result
    }

    fn check_archetype_constructor_call(
        &mut self,
        call: &ArchetypeConstructorCall,
    ) -> Result<String, VerseError> {
        let symbol = self
            .lookup_accessible(&call.name, call.span)?
            .ok_or_else(|| {
                VerseError::check_at(format!("undefined constructor `{}`", call.name), call.span)
            })?;

        let mut arg_types = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            arg_types.push(self.check_expr(call_arg_expr(arg))?);
        }

        let return_type = match symbol.value_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "constructor") {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}<constructor>` expects a constructor function",
                            call.name
                        ),
                        call.span,
                    ));
                }
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        call.span,
                    ));
                }
                if self.in_failure_context() {
                    ensure_callable_in_failure_context(&effects, call.span)?;
                }
                self.ensure_callable_in_async_context(&effects, call.span)?;
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call.args,
                    &arg_types,
                    call.span,
                )?;
                *return_type
            }
            Type::Overload(overloads) => {
                let constructors = overloads
                    .into_iter()
                    .filter(|overload| {
                        matches!(
                            overload,
                            Type::Function { effects, .. } if has_effect(effects, "constructor")
                        )
                    })
                    .collect::<Vec<_>>();
                if constructors.is_empty() {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}<constructor>` expects a constructor function",
                            call.name
                        ),
                        call.span,
                    ));
                }
                self.check_overloaded_call(
                    &constructors,
                    false,
                    self.in_failure_context(),
                    &call.args,
                    &arg_types,
                    call.span,
                )?
            }
            other => {
                return Err(VerseError::check_at(
                    format!(
                        "`{}<constructor>` cannot call value of type `{other}`",
                        call.name
                    ),
                    call.span,
                ));
            }
        };

        match return_type {
            Type::Class(name) => Ok(name),
            other => Err(VerseError::check_at(
                format!("constructor delegation must return a class, got `{other}`"),
                call.span,
            )),
        }
    }

    fn ensure_data_member_default_archetype_not_recursive(
        &self,
        aggregate_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(context) = self
            .data_member_default_stack
            .iter()
            .rev()
            .find(|context| context.aggregate_name == aggregate_name)
        {
            return Err(VerseError::check_at(
                format!(
                    "field default `{}.{}` recursively constructs `{aggregate_name}`",
                    context.aggregate_name, context.field_name
                ),
                span,
            ));
        }

        Ok(())
    }

    fn check_event_archetype(
        &mut self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
    ) -> Result<Type, VerseError> {
        if !entries.is_empty() {
            return Err(VerseError::check_at(
                "`event` archetype construction expects an empty body",
                callee.span,
            ));
        }

        let args = official_event_archetype_args(callee)
            .expect("event archetype callee should have been recognized");
        let mut type_args = Vec::with_capacity(args.len());
        for arg in args {
            let CallArg::Positional(expr) = arg else {
                return Err(VerseError::check_at(
                    "`event` type arguments do not accept named arguments",
                    call_arg_expr(arg).span,
                ));
            };
            let type_name = self.expr_to_type_name(expr)?;
            type_args.push(self.type_name_to_type_name(&type_name, expr.span)?);
        }

        official_parametric_type("event", &type_args, callee.span)
    }

    fn expr_to_type_name(&self, expr: &Expr) -> Result<TypeName, VerseError> {
        match &expr.kind {
            ExprKind::Ident(name) => Ok(TypeName::parse(name.clone())),
            ExprKind::Member { .. } => expr_to_type_path(expr)
                .map(TypeName::parse)
                .ok_or_else(|| VerseError::check_at("expected type argument", expr.span)),
            ExprKind::QualifiedName { qualifier, name } => {
                Ok(TypeName::Named(format!("{qualifier}.{name}")))
            }
            ExprKind::Call { callee, args } => {
                let Some(name) = expr_to_type_path(callee) else {
                    return Err(VerseError::check_at(
                        "expected parametric type name",
                        callee.span,
                    ));
                };

                let mut type_args = Vec::with_capacity(args.len());
                for arg in args {
                    let CallArg::Positional(expr) = arg else {
                        return Err(VerseError::check_at(
                            "parametric type arguments do not accept named arguments",
                            call_arg_expr(arg).span,
                        ));
                    };
                    type_args.push(self.expr_to_type_name(expr)?);
                }

                match name.as_str() {
                    "tuple" => {
                        if type_args.len() < 2 {
                            return Err(VerseError::check_at(
                                "tuple type expects at least two element types",
                                expr.span,
                            ));
                        }
                        Ok(TypeName::Tuple(type_args))
                    }
                    "weak_map" => {
                        if type_args.len() != 2 {
                            return Err(VerseError::check_at(
                                format!(
                                    "parametric type `weak_map` expected 2 type arguments, got {}",
                                    type_args.len()
                                ),
                                expr.span,
                            ));
                        }
                        Ok(TypeName::WeakMap(
                            Box::new(type_args[0].clone()),
                            Box::new(type_args[1].clone()),
                        ))
                    }
                    "event"
                    | "task"
                    | "generator"
                    | "castable_subtype"
                    | "concrete_subtype"
                    | "classifiable_subset"
                    | "modifier"
                    | "modifier_stack"
                    | "result"
                    | "awaitable"
                    | "signalable"
                    | "listenable"
                    | "subscribable" => Ok(TypeName::Applied {
                        name,
                        args: type_args,
                    }),
                    _ if self.resolve_parametric_type_reference(&name).is_some() => {
                        Ok(TypeName::Applied {
                            name,
                            args: type_args,
                        })
                    }
                    _ => Err(VerseError::check_at(
                        format!("unknown parametric type `{name}`"),
                        callee.span,
                    )),
                }
            }
            _ => Err(VerseError::check_at("expected type argument", expr.span)),
        }
    }

    fn check_archetype_let(&mut self, binding: &ArchetypeLet) -> Result<(), VerseError> {
        self.check_binding(
            &binding.name,
            &[],
            binding.annotation.as_ref(),
            &binding.expr,
            false,
            binding.span,
        )?;
        Ok(())
    }

    fn check_if_condition(&mut self, condition: &Expr) -> Result<(), VerseError> {
        self.check_if_condition_inner(condition)?;

        if !failure_condition_has_failable_expr(condition) {
            return Err(VerseError::check_at(
                "`if` condition must contain at least one failable expression",
                condition.span,
            ));
        }

        Ok(())
    }

    fn check_if_condition_inner(&mut self, condition: &Expr) -> Result<(), VerseError> {
        match &condition.kind {
            ExprKind::FailureSequence(clauses) => {
                for clause in clauses {
                    self.check_if_condition_inner(clause)?;
                }
                Ok(())
            }
            ExprKind::FailureBind { name, expr } => {
                let value_type = self.check_failure_expr(expr)?;
                self.define(name, value_type, false, condition.span)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.check_failure_statements(statements)?;
                Ok(())
            }
            ExprKind::Set { .. } => {
                self.check_failure_expr(condition)?;
                Ok(())
            }
            _ => {
                let is_failable = is_failable_condition_expr(condition);
                let condition_type = self.check_failure_expr(condition)?;
                if is_failable {
                    Ok(())
                } else {
                    ensure_bool_like(&condition_type, "if condition", condition.span)
                }
            }
        }
    }

    fn check_failure_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        self.failure_context_depth += 1;
        let result = self.check_failure_expr_inner(expr);
        self.failure_context_depth -= 1;
        result
    }

    fn check_failure_expr_inner(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        match &expr.kind {
            ExprKind::UnwrapOption(value) => match self.check_failure_expr(value)? {
                Type::Option(item) => Ok(*item),
                Type::Bool => Ok(Type::Bool),
                Type::Unknown | Type::Any => Ok(Type::Unknown),
                other => Err(VerseError::check_at(
                    format!("query operator expected `bool` or option, got `{other}`"),
                    value.span,
                )),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => {
                self.check_failure_expr(expr)?;
                Ok(Type::Bool)
            }
            ExprKind::Binary { left, op, right } if is_failure_binary_op(*op) => {
                self.check_failure_binary(left, *op, right)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.push_scope();
                let result = self.check_failure_statements(statements);
                self.pop_scope();
                result
            }
            ExprKind::Profile { description, body } => {
                let description_type = self.check_expr(description)?;
                self.ensure_expr_assignable(&Type::String, &description_type, description, || {
                    format!("profile description expected `string`, got `{description_type}`")
                })?;
                self.check_failure_expr(body)
            }
            ExprKind::Call { callee, args } => self.check_failure_call(callee, args, expr.span),
            ExprKind::BracketCall { callee, args } => self.check_failure_bracket_call(callee, args),
            ExprKind::Set {
                target,
                op,
                expr: value,
            } => self.check_set_expression(target, *op, value, true, expr.span),
            ExprKind::Var {
                name,
                annotation,
                expr: value,
            } => self.check_var_expression(name, annotation, value, true, expr.span),
            ExprKind::Member { object, name } => {
                self.check_failure_member_expr(object, name, expr.span, false)
            }
            _ => self.check_expr(expr),
        }
    }

    fn check_failure_statements(&mut self, statements: &[Stmt]) -> Result<Type, VerseError> {
        let mut last = Type::None;
        let mut unreachable_after: Option<(&'static str, Span)> = None;

        for statement in statements {
            if let Some((message, span)) = unreachable_after {
                return Err(VerseError::check_at(message, span.through(statement.span)));
            }
            last = match &statement.kind {
                StmtKind::Let {
                    name,
                    annotation,
                    expr,
                    ..
                } => self.check_failure_binding(
                    name,
                    annotation.as_ref(),
                    expr,
                    false,
                    statement.span,
                )?,
                StmtKind::Var {
                    name,
                    annotation,
                    expr,
                } => self.check_failure_binding(
                    name,
                    annotation.as_ref(),
                    expr,
                    true,
                    statement.span,
                )?,
                StmtKind::Return(expr) => {
                    let Some(expected) = self.function_returns.last().cloned() else {
                        return Err(VerseError::check_at(
                            "`return` used outside a function",
                            statement.span,
                        ));
                    };

                    let actual = self.check_failure_expr(expr)?;
                    self.ensure_expr_assignable(&expected, &actual, expr, || {
                        format!("cannot return `{actual}` from function returning `{expected}`")
                    })?;
                    Type::Never
                }
                StmtKind::Set { target, op, expr } => {
                    self.check_set_expression(target, *op, expr, true, statement.span)?
                }
                StmtKind::Expr(expr) => self.check_failure_expr(expr)?,
                _ => self.check_stmt(statement)?,
            };
            if last == Type::Never || self.statement_never_completes(statement) {
                unreachable_after =
                    Some((unreachable_statement_message(statement), statement.span));
            }
        }

        Ok(last)
    }

    fn check_failure_binding(
        &mut self,
        name: &str,
        annotation: Option<&TypeAnnotation>,
        expr: &Expr,
        mutable: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        if matches!(
            &expr.kind,
            ExprKind::EnumDefinition { .. }
                | ExprKind::StructDefinition { .. }
                | ExprKind::ClassDefinition { .. }
                | ExprKind::InterfaceDefinition { .. }
                | ExprKind::ModuleDefinition { .. }
        ) {
            return self.check_binding(name, &[], annotation, expr, mutable, span);
        }

        let inferred = self.check_failure_expr(expr)?;
        let checked_type = if let Some(annotation) = annotation {
            let expected = self.type_name_to_type(annotation)?;
            self.ensure_expr_assignable(&expected, &inferred, expr, || {
                format!(
                    "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
                )
            })?;
            expected
        } else {
            inferred
        };

        self.define(name, checked_type.clone(), mutable, span)?;
        Ok(checked_type)
    }

    fn statement_never_completes(&self, statement: &Stmt) -> bool {
        match &statement.kind {
            StmtKind::Return(_) | StmtKind::Break => true,
            StmtKind::Let { expr, .. } | StmtKind::Var { expr, .. } | StmtKind::Expr(expr) => {
                self.expr_never_completes(expr)
            }
            StmtKind::Set { target, expr, .. } => {
                self.expr_never_completes(target) || self.expr_never_completes(expr)
            }
            StmtKind::Using { .. }
            | StmtKind::ParametricType { .. }
            | StmtKind::TypeAlias { .. }
            | StmtKind::ExtensionMethod(_)
            | StmtKind::Defer(_) => false,
        }
    }

    fn expr_never_completes(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Unary { expr, .. } => self.expr_never_completes(expr),
            ExprKind::Binary { left, op, right } => {
                self.expr_never_completes(left)
                    || (!matches!(op, BinaryOp::And | BinaryOp::Or)
                        && self.expr_never_completes(right))
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr_never_completes(condition)
                    || else_branch.as_deref().is_some_and(|else_branch| {
                        self.expr_never_completes(then_branch)
                            && self.expr_never_completes(else_branch)
                    })
            }
            ExprKind::FailureBind { expr, .. } => self.expr_never_completes(expr),
            ExprKind::FailureSequence(items) => items
                .first()
                .is_some_and(|item| self.expr_never_completes(item)),
            ExprKind::Set { target, expr, .. } => {
                self.expr_never_completes(target) || self.expr_never_completes(expr)
            }
            ExprKind::Var { expr, .. } => self.expr_never_completes(expr),
            ExprKind::For { clauses, .. } => clauses
                .iter()
                .any(|clause| self.for_clause_expr_never_completes(clause)),
            ExprKind::Profile { description, body } => {
                self.expr_never_completes(description) || self.expr_never_completes(body)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
                .last()
                .is_some_and(|statement| self.statement_never_completes(statement)),
            ExprKind::Call { callee, args } => {
                self.expr_never_completes(callee)
                    || args
                        .iter()
                        .any(|arg| self.expr_never_completes(call_arg_expr(arg)))
                    || self.callee_returns_never(callee)
            }
            ExprKind::BracketCall { callee, args } => {
                self.expr_never_completes(callee)
                    || args.iter().any(|arg| self.expr_never_completes(arg))
                    || self.callee_returns_never(callee)
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => {
                items.iter().any(|item| self.expr_never_completes(item))
            }
            ExprKind::Map(entries) => entries.iter().any(|(key, value)| {
                self.expr_never_completes(key) || self.expr_never_completes(value)
            }),
            ExprKind::Archetype {
                callee, entries, ..
            } => {
                self.expr_never_completes(callee)
                    || entries
                        .iter()
                        .any(|entry| self.archetype_entry_never_completes(entry))
            }
            ExprKind::Case { subject, arms } => {
                self.expr_never_completes(subject)
                    || (case_arms_have_wildcard(arms)
                        && arms.iter().all(|arm| self.expr_never_completes(&arm.expr)))
            }
            ExprKind::Option(Some(value)) | ExprKind::UnwrapOption(value) => {
                self.expr_never_completes(value)
            }
            ExprKind::InterpolatedString(parts) => parts.iter().any(|part| match part {
                InterpolatedStringPart::Text(_) => false,
                InterpolatedStringPart::Expr(expr) => self.expr_never_completes(expr),
            }),
            ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
                self.expr_never_completes(object)
            }
            ExprKind::Index { collection, index } => {
                self.expr_never_completes(collection) || self.expr_never_completes(index)
            }
            ExprKind::Number { .. }
            | ExprKind::Char { .. }
            | ExprKind::Bool(_)
            | ExprKind::String(_)
            | ExprKind::None
            | ExprKind::Ident(_)
            | ExprKind::External
            | ExprKind::Loop { .. }
            | ExprKind::Spawn { .. }
            | ExprKind::Concurrent { .. }
            | ExprKind::Function { .. }
            | ExprKind::EnumDefinition { .. }
            | ExprKind::StructDefinition { .. }
            | ExprKind::ClassDefinition { .. }
            | ExprKind::InterfaceDefinition { .. }
            | ExprKind::ModuleDefinition { .. }
            | ExprKind::Option(None)
            | ExprKind::QualifiedName { .. } => false,
        }
    }

    fn for_clause_expr_never_completes(&self, clause: &ForClause) -> bool {
        match clause {
            ForClause::Generator { iterable, .. }
            | ForClause::Let { expr: iterable, .. }
            | ForClause::RangeOrLet { expr: iterable, .. }
            | ForClause::Filter(iterable) => self.expr_never_completes(iterable),
        }
    }

    fn archetype_entry_never_completes(&self, entry: &ArchetypeEntry) -> bool {
        match entry {
            ArchetypeEntry::Field(field) => self.expr_never_completes(&field.expr),
            ArchetypeEntry::Let(binding) => self.expr_never_completes(&binding.expr),
            ArchetypeEntry::Block(block) => self.expr_never_completes(block),
            ArchetypeEntry::ConstructorCall(call) => call
                .args
                .iter()
                .any(|arg| self.expr_never_completes(call_arg_expr(arg))),
        }
    }

    fn callee_returns_never(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Ident(name) => self
                .lookup(name)
                .is_some_and(|symbol| type_returns_never(&symbol.value_type)),
            _ => false,
        }
    }

    fn check_var_expression(
        &mut self,
        name: &str,
        annotation: &TypeAnnotation,
        expr: &Expr,
        failure_context: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        let expected = self.type_name_to_type(annotation)?;
        let inferred = if failure_context {
            self.check_failure_expr(expr)?
        } else {
            self.check_expr(expr)?
        };
        self.ensure_expr_assignable(&expected, &inferred, expr, || {
            format!(
                "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
            )
        })?;
        self.define(name, expected.clone(), true, span)?;
        Ok(expected)
    }

    fn check_failure_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        match op {
            BinaryOp::And => {
                self.check_failure_expr(left)?;
                self.check_failure_expr(right)
            }
            BinaryOp::Or => {
                let left_type = self.check_failure_expr(left)?;
                let right_type = self.check_failure_expr(right)?;
                unify_types(&left_type, &right_type, right.span)
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => self.check_failure_comparison_binary(left, op, right),
            BinaryOp::Divide | BinaryOp::Remainder => self.check_binary(left, op, right),
            _ => self.check_expr(&Expr::new(
                ExprKind::Binary {
                    left: Box::new(left.clone()),
                    op,
                    right: Box::new(right.clone()),
                },
                left.span.through(right.span),
            )),
        }
    }

    fn check_failure_comparison_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match op {
            BinaryOp::Equal | BinaryOp::NotEqual => {
                ensure_equality_comparable(&left_type, &self.struct_types, left.span)?;
                ensure_equality_comparable(&right_type, &self.struct_types, right.span)?;
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
            }
            _ => unreachable!("only comparison operators are handled here"),
        }

        Ok(left_type)
    }

    fn check_failure_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Type, VerseError> {
        let callee_type = self.check_failure_callee_expr(callee)?;
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expr(call_arg_expr(arg))?);
        }

        if is_shuffle_callee(callee) && is_shuffle_function_type(&callee_type) {
            return self.check_shuffle_call(args, &arg_types, span);
        }

        if is_concatenate_callee(callee) && is_concatenate_function_type(&callee_type) {
            return self.check_concatenate_call(args, &arg_types, span);
        }

        if is_make_classifiable_subset_callee(callee)
            && is_make_classifiable_subset_function_type(&callee_type)
        {
            return self.check_make_classifiable_subset_call(args, &arg_types, span);
        }

        if is_make_result_callee(callee) {
            return self.check_make_result_call(callee, args, &arg_types, span);
        }

        if is_length_member_callee(callee)
            && matches!(callee_type, Type::Int | Type::Unknown | Type::Any)
        {
            self.check_call_arguments((Some(0), None), Some(&[]), None, args, &arg_types, span)?;
            return Ok(Type::Int);
        }

        match callee_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        span,
                    ));
                }
                ensure_callable_in_failure_context(&effects, span)?;
                self.ensure_callable_in_async_context(&effects, span)?;
                self.ensure_current_function_allows_call_effects(&effects, span)?;
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    &arg_types,
                    span,
                )?;
                Ok(*return_type)
            }
            Type::Overload(overloads) => {
                self.check_overloaded_call(&overloads, false, true, args, &arg_types, span)
            }
            Type::Tuple(items) => self.check_tuple_access(&items, args, &arg_types, span),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot call value of type `{other}`"),
                callee.span,
            )),
        }
    }

    fn check_failure_bracket_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Type, VerseError> {
        let mut checked_arg_types = None;
        if let ExprKind::Member { object, name } = &callee.kind {
            let object_type = if is_failable_condition_expr(object) {
                self.check_failure_expr(object)?
            } else {
                self.check_expr(object)?
            };
            let arg_types = args
                .iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?;

            match object_type {
                Type::Array(item) => {
                    return self.check_array_method(
                        name,
                        item.as_ref(),
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::String => {
                    return self.check_array_method(
                        name,
                        &Type::Char,
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::Int | Type::Float | Type::Rational | Type::Number => {
                    return self.check_number_method(
                        name,
                        &object_type,
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::Unknown | Type::Any => return Ok(Type::Unknown),
                Type::Struct(_)
                | Type::Class(_)
                | Type::Module(_)
                | Type::Result(_, _)
                | Type::Event(_)
                | Type::Task(_)
                | Type::Generator(_)
                | Type::CastableSubtype(_)
                | Type::ConcreteSubtype(_)
                | Type::ClassifiableSubset(_)
                | Type::Awaitable(_)
                | Type::Signalable(_)
                | Type::Subscribable(_)
                | Type::Listenable(_)
                | Type::Param(_, TypeParamConstraint::Subtype(_)) => {
                    checked_arg_types = Some(arg_types)
                }
                other => {
                    return Err(VerseError::check_at(
                        format!("type `{other}` has no bracket method `{name}`"),
                        callee.span,
                    ));
                }
            }
        }

        let callee_type = self.check_failure_callee_expr(callee)?;
        let arg_types = if let Some(arg_types) = checked_arg_types {
            arg_types
        } else {
            args.iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?
        };

        if is_fits_in_player_map_callee(callee) {
            ensure_exact_arg_count("FitsInPlayerMap", args, 1, callee.span)?;
            return Ok(arg_types[0].clone());
        }

        match callee_type {
            Type::Array(item) => {
                ensure_exact_arg_count("array index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "array index", args[0].span)?;
                Ok(*item)
            }
            Type::String => {
                ensure_exact_arg_count("string index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "string index", args[0].span)?;
                Ok(Type::Char)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) => {
                ensure_exact_arg_count("map lookup", args, 1, callee.span)?;
                self.check_map_like_lookup(&key, &value, &arg_types[0], &args[0])
            }
            Type::Function {
                arity,
                arity_range,
                effects,
                mut param_types,
                mut param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions without `<decides>` must be called with `()`",
                        callee.span,
                    ));
                }
                ensure_callable_in_failure_context(&effects, callee.span)?;
                self.ensure_callable_in_async_context(&effects, callee.span)?;
                self.ensure_current_function_allows_call_effects(&effects, callee.span)?;
                let call_args = positional_call_args(args);
                let mut return_type = *return_type;
                if let Some(inferred) =
                    infer_function_type_params(param_types.as_deref(), &arg_types)
                        .filter(|inferred| !inferred.is_empty())
                {
                    self.ensure_inferred_type_param_constraints(
                        param_types.as_deref(),
                        &inferred,
                        callee.span,
                    )?;
                    if let Some(types) = param_types.as_mut() {
                        for value_type in types {
                            *value_type = substitute_type_params(value_type, &inferred);
                        }
                    }
                    if let Some(specs) = param_specs.as_mut() {
                        for spec in specs {
                            spec.value_type = substitute_type_params(&spec.value_type, &inferred);
                        }
                    }
                    return_type =
                        self.substitute_type_params_runtime(&return_type, &inferred, callee.span)?;
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                Ok(return_type)
            }
            Type::Overload(overloads) => {
                let call_args = positional_call_args(args);
                self.check_overloaded_call(
                    &overloads,
                    true,
                    true,
                    &call_args,
                    &arg_types,
                    callee.span,
                )
            }
            Type::ClassType(target) => {
                self.check_class_cast(&target, args, &arg_types, callee.span)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot use `[]` with value of type `{other}`"),
                callee.span,
            )),
        }
    }

    fn check_case(
        &mut self,
        subject: &Expr,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let subject_type = self.check_expr(subject)?;
        match subject_type {
            Type::Enum(enum_name) => self.check_enum_case(&enum_name, arms, span, subject.span),
            Type::Int | Type::Bool | Type::String | Type::Char | Type::Char8 | Type::Char32 => {
                self.check_scalar_case(&subject_type, arms, span)
            }
            Type::Unknown | Type::Any => self.check_unknown_case_arms(arms, span),
            other => Err(VerseError::check_at(
                format!(
                    "case subject must be `int`, `logic`, `string`, `char`, or enum, got `{other}`"
                ),
                subject.span,
            )),
        }
    }

    fn check_enum_case(
        &mut self,
        enum_name: &str,
        arms: &[CaseArm],
        span: Span,
        subject_span: Span,
    ) -> Result<Type, VerseError> {
        let Some(info) = self.enum_types.get(enum_name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown enum `{enum_name}`"),
                subject_span,
            ));
        };

        let mut result_type = None;
        let mut covered = Vec::<String>::new();
        let mut saw_wildcard = false;

        for arm in arms {
            let unreachable_after_wildcard = saw_wildcard;
            if unreachable_after_wildcard && !arm.ignore_unreachable {
                return Err(VerseError::check_at(
                    "case after wildcard is unreachable",
                    arm.span,
                ));
            }

            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    saw_wildcard = true;
                }
                CasePattern::Expr(pattern) => {
                    let pattern_type = self.check_expr(pattern)?;
                    self.ensure_assignable(
                        &Type::Enum(enum_name.to_string()),
                        &pattern_type,
                        pattern.span,
                        || {
                            format!(
                                "case pattern for `{enum_name}` must be `{enum_name}`, got `{pattern_type}`"
                            )
                        },
                    )?;

                    let variant = enum_case_variant(pattern, enum_name).ok_or_else(|| {
                        VerseError::check_at(
                            format!(
                                "case pattern for `{enum_name}` must be an explicit enum value"
                            ),
                            pattern.span,
                        )
                    })?;

                    let duplicate = covered.iter().any(|existing| existing == variant);
                    if duplicate && !arm.ignore_unreachable {
                        return Err(VerseError::check_at(
                            format!("duplicate case `{enum_name}.{variant}` is unreachable"),
                            pattern.span,
                        ));
                    }
                    if !duplicate && !unreachable_after_wildcard {
                        covered.push(variant.to_string());
                    }
                }
            }

            let next = self.check_case_arm_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, arm.expr.span)?,
                None => next,
            });
        }

        if !saw_wildcard && !self.case_failure_allowed() {
            if info.open {
                return Err(VerseError::check_at(
                    format!("case over open enum `{enum_name}` requires wildcard or `<decides>`"),
                    span,
                ));
            }

            let missing = info
                .variants
                .iter()
                .filter(|variant| !covered.iter().any(|covered| covered == *variant))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(VerseError::check_at(
                    format!(
                        "case expression for enum `{enum_name}` is missing cases: {}",
                        missing.join(", ")
                    ),
                    span,
                ));
            }
        }

        Ok(result_type.unwrap_or(Type::Unknown))
    }

    fn check_scalar_case(
        &mut self,
        subject_type: &Type,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut result_type = None;
        let mut covered = Vec::<CaseConstant>::new();
        let mut saw_wildcard = false;

        for arm in arms {
            let unreachable_after_wildcard = saw_wildcard;
            if unreachable_after_wildcard && !arm.ignore_unreachable {
                return Err(VerseError::check_at(
                    "case after wildcard is unreachable",
                    arm.span,
                ));
            }

            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    saw_wildcard = true;
                }
                CasePattern::Expr(pattern) => {
                    let pattern_type = self.check_expr(pattern)?;
                    self.ensure_assignable(subject_type, &pattern_type, pattern.span, || {
                        format!(
                            "case pattern for `{subject_type}` must be `{subject_type}`, got `{pattern_type}`"
                        )
                    })?;
                    let constant =
                        scalar_case_constant(pattern, subject_type).ok_or_else(|| {
                            VerseError::check_at(
                                format!(
                                    "case pattern for `{subject_type}` must be an `int`, `logic`, `string`, or `char` constant"
                                ),
                                pattern.span,
                            )
                        })?;
                    let duplicate = covered.iter().any(|existing| existing == &constant);
                    if duplicate && !arm.ignore_unreachable {
                        return Err(VerseError::check_at(
                            "duplicate case is unreachable",
                            pattern.span,
                        ));
                    }
                    if !duplicate && !unreachable_after_wildcard {
                        covered.push(constant);
                    }
                }
            }

            let next = self.check_case_arm_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, arm.expr.span)?,
                None => next,
            });
        }

        if !saw_wildcard
            && !self.case_failure_allowed()
            && !scalar_case_is_exhaustive(subject_type, &covered)
        {
            return Err(VerseError::check_at(
                format!("case over `{subject_type}` requires wildcard or failure context"),
                span,
            ));
        }

        Ok(result_type.unwrap_or(Type::Unknown))
    }

    fn check_case_arm_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        if self.failure_context_depth > 0 {
            self.check_failure_expr(expr)
        } else {
            self.check_expr(expr)
        }
    }

    fn case_failure_allowed(&self) -> bool {
        self.failure_context_depth > 0 || self.current_function_has_effect("decides")
    }

    fn check_unknown_case_arms(
        &mut self,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut result_type = None;
        let mut saw_wildcard = false;
        for arm in arms {
            if saw_wildcard && !arm.ignore_unreachable {
                return Err(VerseError::check_at(
                    "case after wildcard is unreachable",
                    arm.span,
                ));
            }
            match &arm.pattern {
                CasePattern::Wildcard { .. } => saw_wildcard = true,
                CasePattern::Expr(pattern) => {
                    self.check_expr(pattern)?;
                }
            }
            let next = self.check_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, span)?,
                None => next,
            });
        }
        Ok(result_type.unwrap_or(Type::Unknown))
    }

    fn current_function_has_effect(&self, effect: &str) -> bool {
        self.function_effects
            .last()
            .is_some_and(|effects| has_effect(effects, effect))
    }

    fn push_async_expr_marker(&mut self) {
        self.async_expr_markers.push(AsyncExprMarker {
            function_depth: self.function_effects.len(),
            seen: false,
        });
    }

    fn pop_async_expr_marker(&mut self) -> bool {
        self.async_expr_markers
            .pop()
            .expect("checker async expression marker stack should not underflow")
            .seen
    }

    fn mark_async_expression(&mut self) {
        if self.suppressed_async_expr_markers > 0 {
            return;
        }

        let function_depth = self.function_effects.len();
        if let Some(marker) = self.async_expr_markers.last_mut()
            && marker.function_depth == function_depth
        {
            marker.seen = true;
        }
    }

    fn with_suppressed_async_expr_marker<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.suppressed_async_expr_markers += 1;
        let result = f(self);
        self.suppressed_async_expr_markers -= 1;
        result
    }

    fn ensure_callable_in_async_context(
        &mut self,
        effects: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        if !has_effect(effects, "suspends") {
            return Ok(());
        }

        if self.defer_depth > 0 {
            return Err(VerseError::check_at(
                "`defer` block cannot contain suspend expressions",
                span,
            ));
        }

        if !self.current_function_has_effect("suspends") {
            return Err(VerseError::check_at(
                "function with `<suspends>` effect can only be called in an async context",
                span,
            ));
        }

        self.mark_async_expression();
        Ok(())
    }

    fn ensure_callee_type_effects_allowed(
        &self,
        callee_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        match callee_type {
            Type::Function { effects, .. } => {
                self.ensure_current_function_allows_call_effects(effects, span)
            }
            _ => Ok(()),
        }
    }

    fn ensure_current_function_allows_call_effects(
        &self,
        callee_effects: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(caller_effects) = self.function_effects.last() else {
            return Ok(());
        };
        if !has_explicit_call_effect_specifier(caller_effects) {
            return Ok(());
        }

        if has_no_rollback_effect(callee_effects) {
            if has_effect(caller_effects, "transacts") {
                return Ok(());
            }
            return Err(effect_call_error(caller_effects, "no_rollback", span));
        }

        let allowed = call_allowed_capabilities(caller_effects);
        for required in call_required_capabilities(callee_effects) {
            if !allowed.iter().any(|capability| capability == &required) {
                return Err(effect_call_error(caller_effects, required, span));
            }
        }

        Ok(())
    }

    fn ensure_current_function_allows_allocation(&self, span: Span) -> Result<(), VerseError> {
        let Some(caller_effects) = self.function_effects.last() else {
            return Ok(());
        };

        let allowed = call_allowed_capabilities(caller_effects);
        if allowed.iter().any(|capability| capability == &"allocates") {
            Ok(())
        } else {
            Err(effect_call_error(caller_effects, "allocates", span))
        }
    }

    fn in_failure_context(&self) -> bool {
        self.failure_context_depth > 0
    }

    fn failable_expression_allowed(&self) -> bool {
        self.in_failure_context() || self.current_function_has_effect("decides")
    }

    fn ensure_failable_expression_allowed(&self, span: Span) -> Result<(), VerseError> {
        if self.failable_expression_allowed() {
            Ok(())
        } else {
            Err(VerseError::check_at(
                "failable expression must be used in a failure context",
                span,
            ))
        }
    }

    fn with_range_context<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        self.range_context_depth += 1;
        let result = f(self);
        self.range_context_depth -= 1;
        result
    }

    fn without_enclosing_failure_context<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        let previous = self.failure_context_depth;
        self.failure_context_depth = 0;
        let result = f(self);
        self.failure_context_depth = previous;
        result
    }

    fn merge_collection_item_type<'a>(
        &mut self,
        current: &mut Type,
        pending_empty_options: &mut Vec<&'a Expr>,
        expr: &'a Expr,
    ) -> Result<(), VerseError> {
        if is_empty_option_candidate(expr) && matches!(current, Type::Unknown) {
            pending_empty_options.push(expr);
            return Ok(());
        }

        let next = self.check_expr(expr)?;
        if matches!(current, Type::Unknown) {
            *current = next;
            finalize_collection_item_type(current, pending_empty_options)?;
        } else if !is_empty_option_literal(current, expr) {
            *current = unify_types(current, &next, expr.span)?;
        }
        Ok(())
    }

    fn ensure_current_function_allows_mutation(&self, span: Span) -> Result<(), VerseError> {
        let Some(caller_effects) = self.function_effects.last() else {
            return Ok(());
        };

        let allowed = call_allowed_capabilities(caller_effects);
        if allowed.iter().any(|capability| capability == &"writes") {
            Ok(())
        } else {
            Err(VerseError::check_at(
                "mutable assignment in function requires `<writes>` or `<transacts>` effect",
                span,
            ))
        }
    }

    fn ensure_assignable(
        &self,
        expected: &Type,
        actual: &Type,
        span: Span,
        message: impl FnOnce() -> String,
    ) -> Result<(), VerseError> {
        if self.is_assignable(expected, actual) {
            Ok(())
        } else {
            Err(VerseError::check_at(message(), span))
        }
    }

    fn ensure_expr_assignable(
        &mut self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
        message: impl FnOnce() -> String,
    ) -> Result<(), VerseError> {
        if self.expr_is_assignable_to_expected(expected, actual, expr)? {
            Ok(())
        } else {
            Err(VerseError::check_at(message(), expr.span))
        }
    }

    fn expr_is_assignable_to_expected(
        &mut self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
    ) -> Result<bool, VerseError> {
        if is_empty_option_literal(expected, expr) {
            return Ok(true);
        }

        match (expected, &expr.kind) {
            (Type::Array(expected_item), ExprKind::Array(items)) => {
                for item in items {
                    let actual_item = self.check_expr(item)?;
                    if !self.expr_is_assignable_to_expected(expected_item, &actual_item, item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (
                Type::Map(expected_key, expected_value)
                | Type::WeakMap(expected_key, expected_value),
                ExprKind::Map(entries),
            ) => {
                for (key, value) in entries {
                    let actual_key = self.check_expr(key)?;
                    if !self.expr_is_assignable_to_expected(expected_key, &actual_key, key)? {
                        return Ok(false);
                    }

                    let actual_value = self.check_expr(value)?;
                    if !self.expr_is_assignable_to_expected(expected_value, &actual_value, value)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Type::Tuple(expected_items), ExprKind::Tuple(items))
                if expected_items.len() == items.len() =>
            {
                for (expected_item, item) in expected_items.iter().zip(items) {
                    let actual_item = self.check_expr(item)?;
                    if !self.expr_is_assignable_to_expected(expected_item, &actual_item, item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Type::Option(expected_item), ExprKind::Option(Some(value))) => {
                let actual_value = self.check_failure_expr(value)?;
                self.expr_is_assignable_to_expected(expected_item, &actual_value, value)
            }
            (Type::Option(_), ExprKind::Option(None)) => Ok(true),
            _ => Ok(self.is_assignable(expected, actual)),
        }
    }

    fn is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        matches!(expected, Type::Any | Type::Unknown)
            || matches!(actual, Type::Any | Type::Unknown | Type::Never)
            || expected == actual
            || self
                .constrained_type_param_supertype_for_assignability(actual)
                .is_some_and(|supertype| self.is_assignable(expected, &supertype))
            || matches!(expected, Type::Comparable)
                && ensure_comparable_key(actual, &self.struct_types, Span::new(0, 0, 0, 0)).is_ok()
            || matches!(expected, Type::Number) && is_numeric_type(actual)
            || matches!((expected, actual), (Type::Rational, Type::Int))
            || matches!((expected, actual), (Type::Float, Type::Int))
            || matches!((expected, actual), (Type::Message, Type::String))
            || matches!((expected, actual), (Type::Array(item), Type::String) if is_string_char_type(item))
            || matches!((expected, actual), (Type::String, Type::Array(item)) if is_string_char_type(item))
            || matches!(
                (expected, actual),
                (Type::Class(expected), Type::Class(actual))
                    if self.is_class_subtype(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Interface(expected), Type::Class(actual))
                    if self.class_implements_interface(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Modifier(expected), Type::Class(actual))
                    if self.class_implements_modifier(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Interface(expected), Type::Interface(actual))
                    if self.is_interface_subtype(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Array(expected), Type::Array(actual))
                    if self.is_assignable(expected, actual)
            )
            || matches!(
                (expected, actual),
                (Type::Array(expected), Type::Tuple(actual_items))
                    if actual_items
                        .iter()
                        .all(|actual| self.is_assignable(expected, actual))
            )
            || matches!(
                (expected, actual),
                (
                    Type::Map(expected_key, expected_value)
                    | Type::WeakMap(expected_key, expected_value),
                    Type::Map(actual_key, actual_value) | Type::WeakMap(actual_key, actual_value),
                )
                    if self.is_assignable(expected_key, actual_key)
                        && self.is_assignable(expected_value, actual_value)
            )
            || matches!(
                (expected, actual),
                (
                    Type::Result(expected_success, expected_error),
                    Type::Result(actual_success, actual_error),
                )
                    if self.is_assignable(expected_success, actual_success)
                        && self.is_assignable(expected_error, actual_error)
            )
            || self.parametric_builtin_is_assignable(expected, actual)
            || matches!(
                (expected, actual),
                (Type::Tuple(expected_items), Type::Tuple(actual_items))
                    if expected_items.len() == actual_items.len()
                        && expected_items
                            .iter()
                            .zip(actual_items)
                            .all(|(expected, actual)| self.is_assignable(expected, actual))
            )
            || matches!(
                (expected, actual),
                (Type::Option(expected), Type::Option(actual))
                    if self.is_assignable(expected, actual)
            )
            || self.function_is_assignable(expected, actual)
    }

    fn parametric_builtin_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Event(expected), Type::Event(actual))
            | (Type::Awaitable(expected), Type::Awaitable(actual))
            | (Type::Subscribable(expected), Type::Subscribable(actual))
            | (Type::Listenable(expected), Type::Listenable(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::Task(expected), Type::Task(actual)) => self.is_assignable(expected, actual),
            (Type::Generator(expected), Type::Generator(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::CastableSubtype(expected), Type::ClassType(actual)) => {
                self.class_type_value_is_castable_subtype(actual, expected)
            }
            (Type::ConcreteSubtype(expected), Type::ClassType(actual)) => {
                self.class_type_value_is_concrete_subtype(actual, expected)
            }
            (Type::CastableSubtype(expected), Type::CastableSubtype(actual))
            | (Type::ConcreteSubtype(expected), Type::ConcreteSubtype(actual))
            | (Type::ClassifiableSubset(expected), Type::ClassifiableSubset(actual))
            | (Type::Modifier(expected), Type::Modifier(actual))
            | (Type::ModifierStack(expected), Type::ModifierStack(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Modifier(expected), Type::ModifierStack(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Signalable(expected), Type::Signalable(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Awaitable(expected), Type::Task(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), Some(actual.as_ref()))
            }
            (Type::Awaitable(expected), Type::Event(actual))
            | (Type::Awaitable(expected), Type::Listenable(actual))
            | (Type::Subscribable(expected), Type::Listenable(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::Signalable(expected), Type::Event(actual)) => actual
                .as_deref()
                .is_some_and(|actual| self.is_assignable(expected, actual)),
            _ => false,
        }
    }

    fn class_type_value_is_castable_subtype(&self, actual: &str, expected: &Type) -> bool {
        self.struct_types.get(actual).is_some_and(|info| {
            info.kind == AggregateKind::Class
                && info.castable
                && self.class_type_value_satisfies_subtype(actual, expected)
        })
    }

    fn class_type_value_is_concrete_subtype(&self, actual: &str, expected: &Type) -> bool {
        self.struct_types.get(actual).is_some_and(|info| {
            info.kind == AggregateKind::Class
                && info.concrete
                && self.class_type_value_satisfies_subtype(actual, expected)
        })
    }

    fn class_type_value_satisfies_subtype(&self, actual: &str, expected: &Type) -> bool {
        match expected {
            Type::CastableSubtype(expected) => {
                self.class_type_value_is_castable_subtype(actual, expected)
            }
            _ => self.is_assignable(expected, &Type::Class(actual.to_string())),
        }
    }

    fn optional_payload_is_assignable(
        &self,
        expected: Option<&Type>,
        actual: Option<&Type>,
    ) -> bool {
        match (expected, actual) {
            (None, None) => true,
            (Some(expected), Some(actual)) => self.is_assignable(expected, actual),
            _ => false,
        }
    }

    fn is_persistable_type(&self, value_type: &Type) -> bool {
        match value_type {
            Type::Int
            | Type::Float
            | Type::Rational
            | Type::Number
            | Type::Bool
            | Type::String
            | Type::Message
            | Type::Char
            | Type::Char8
            | Type::Char32
            | Type::None => true,
            Type::Array(item) | Type::Option(item) => self.is_persistable_type(item),
            Type::Map(key, value) => {
                self.is_persistable_type(key) && self.is_persistable_type(value)
            }
            Type::Tuple(items) => items.iter().all(|item| self.is_persistable_type(item)),
            Type::Enum(name) => self
                .enum_types
                .get(name)
                .is_some_and(|info| info.persistable),
            Type::Struct(name) | Type::Class(name) => self
                .struct_types
                .get(name)
                .is_some_and(|info| info.persistable),
            Type::Any
            | Type::Comparable
            | Type::Unknown
            | Type::Never
            | Type::Range
            | Type::EnumType(_)
            | Type::StructType(_)
            | Type::ClassType(_)
            | Type::Interface(_)
            | Type::InterfaceType(_)
            | Type::Module(_)
            | Type::Param(_, _)
            | Type::ParametricType { .. }
            | Type::WeakMap(_, _)
            | Type::Result(_, _)
            | Type::Event(_)
            | Type::Task(_)
            | Type::Generator(_)
            | Type::CastableSubtype(_)
            | Type::ConcreteSubtype(_)
            | Type::ClassifiableSubset(_)
            | Type::Modifier(_)
            | Type::ModifierStack(_)
            | Type::Awaitable(_)
            | Type::Signalable(_)
            | Type::Subscribable(_)
            | Type::Listenable(_)
            | Type::Function { .. }
            | Type::Overload(_) => false,
        }
    }

    fn function_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        if let Type::Overload(overloads) = actual {
            return overloads
                .iter()
                .any(|actual| self.function_is_assignable(expected, actual));
        }

        let (
            Type::Function {
                arity: expected_arity,
                arity_range: None,
                effects: expected_effects,
                param_types: expected_params,
                return_type: expected_return,
                ..
            },
            Type::Function {
                arity: actual_arity,
                arity_range: None,
                effects: actual_effects,
                param_types: actual_params,
                return_type: actual_return,
                ..
            },
        ) = (expected, actual)
        else {
            return false;
        };

        if let (Some(expected_arity), Some(actual_arity)) = (expected_arity, actual_arity)
            && expected_arity != actual_arity
        {
            return false;
        }

        if !function_effects_are_assignable(expected_effects, actual_effects) {
            return false;
        }

        let params_match = match (expected_params, actual_params) {
            (Some(expected_params), Some(actual_params)) => {
                expected_params.len() == actual_params.len()
                    && expected_params
                        .iter()
                        .zip(actual_params)
                        .all(|(expected, actual)| self.is_assignable(actual, expected))
            }
            _ => true,
        };

        params_match && self.is_assignable(expected_return, actual_return)
    }

    fn is_class_subtype(&self, actual: &str, expected: &str) -> bool {
        if self.is_builtin_class_subtype(actual, expected) {
            return true;
        }

        let mut current = Some(actual);
        while let Some(name) = current {
            if name == expected {
                return true;
            }
            current = self
                .struct_types
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        false
    }

    fn is_builtin_class_subtype(&self, actual: &str, expected: &str) -> bool {
        if self.struct_types.contains_key(actual) || self.struct_types.contains_key(expected) {
            return false;
        }
        matches!(
            (actual, expected),
            ("player", "agent") | ("agent", "entity") | ("player", "entity")
        )
    }

    fn is_interface_subtype(&self, actual: &str, expected: &str) -> bool {
        if actual == expected {
            return true;
        }
        let Some(info) = self.interface_types.get(actual) else {
            return false;
        };
        info.parents
            .iter()
            .any(|parent| self.is_interface_subtype(parent, expected))
    }

    fn class_implements_interface(&self, actual: &str, expected: &str) -> bool {
        let mut current = Some(actual);
        while let Some(name) = current {
            let Some(info) = self.struct_types.get(name) else {
                return false;
            };
            if info
                .interfaces
                .iter()
                .any(|interface| self.is_interface_subtype(interface, expected))
            {
                return true;
            }
            current = info.base.as_deref();
        }
        false
    }

    fn class_implements_modifier(&self, actual: &str, expected: &Type) -> bool {
        let required = modifier_evaluate_type(expected);
        let mut current = Some(actual);
        while let Some(name) = current {
            let Some(info) = self.struct_types.get(name) else {
                return false;
            };
            if info.methods.iter().any(|method| {
                method.name == "Evaluate" && self.is_assignable(&required, &method.value_type)
            }) {
                return true;
            }
            current = info.base.as_deref();
        }
        false
    }

    fn classes_are_cast_related(&self, target: &str, source: &str) -> bool {
        self.is_class_subtype(source, target) || self.is_class_subtype(target, source)
    }

    fn check_class_cast(
        &self,
        target: &str,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        ensure_exact_arg_count("class cast", args, 1, span)?;
        match &arg_types[0] {
            Type::Class(source) if self.classes_are_cast_related(target, source) => {
                Ok(Type::Class(target.to_string()))
            }
            Type::Class(source) => Err(VerseError::check_at(
                format!("cannot cast class `{source}` to unrelated class `{target}`"),
                args[0].span,
            )),
            Type::Unknown | Type::Any => Ok(Type::Class(target.to_string())),
            other => Err(VerseError::check_at(
                format!("class cast expected class instance, got `{other}`"),
                args[0].span,
            )),
        }
    }

    fn check_shuffle_call(
        &self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("`Shuffle` expected 1 arguments, got {}", args.len()),
                span,
            ));
        }
        if let CallArg::Named {
            name,
            optional,
            span,
            ..
        } = &args[0]
        {
            if name != "Input" {
                let rendered = rendered_argument_name(name, *optional);
                return Err(VerseError::check_at(
                    format!("unknown named argument `{rendered}`"),
                    *span,
                ));
            }
            if *optional {
                return Err(VerseError::check_at(
                    "parameter `Input` is not a named parameter",
                    *span,
                ));
            }
        }

        match &arg_types[0] {
            Type::Array(item_type) => Ok(Type::Array(item_type.clone())),
            Type::Unknown | Type::Any => Ok(Type::Array(Box::new(Type::Unknown))),
            other => Err(VerseError::check_at(
                format!("argument 1 expected `array`, got `{other}`"),
                call_arg_expr(&args[0]).span,
            )),
        }
    }

    fn check_concatenate_call(
        &mut self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let arrays_type = Type::Array(Box::new(Type::Array(Box::new(Type::Unknown))));
        let param_types = vec![arrays_type.clone()];
        let param_specs = vec![ParamSpec {
            name: "Arrays".to_string(),
            value_type: arrays_type,
            named: false,
            has_default: false,
            tuple_items: None,
        }];
        self.check_call_arguments(
            (Some(1), None),
            Some(&param_types),
            Some(&param_specs),
            args,
            arg_types,
            span,
        )?;

        let item_type = infer_concatenate_item_type(args, arg_types, span)?;
        Ok(Type::Array(Box::new(item_type)))
    }

    fn check_make_classifiable_subset_call(
        &self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!(
                    "`MakeClassifiableSubset` expected 1 arguments, got {}",
                    args.len()
                ),
                span,
            ));
        }
        if args[0].is_named() {
            return Err(VerseError::check_at(
                "`MakeClassifiableSubset` does not accept named arguments",
                span,
            ));
        }

        match &arg_types[0] {
            Type::Array(item_type) => Ok(Type::ClassifiableSubset(Box::new(
                classifiable_subset_element_type(item_type.as_ref()),
            ))),
            Type::Unknown | Type::Any => Ok(Type::ClassifiableSubset(Box::new(Type::Unknown))),
            other => Err(VerseError::check_at(
                format!("argument 1 expected `array`, got `{other}`"),
                call_arg_expr(&args[0]).span,
            )),
        }
    }

    fn check_make_result_call(
        &self,
        callee: &Expr,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let name = make_result_callee_name(callee).expect("caller should check result callee");
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("`{name}` expected 1 arguments, got {}", args.len()),
                span,
            ));
        }
        if args[0].is_named() {
            return Err(VerseError::check_at(
                format!("`{name}` does not accept named arguments"),
                span,
            ));
        }

        let payload_type = arg_types
            .first()
            .cloned()
            .expect("arity check should leave one argument type");
        match name {
            "MakeSuccess" => Ok(Type::Result(Box::new(payload_type), Box::new(Type::Never))),
            "MakeError" => Ok(Type::Result(Box::new(Type::Never), Box::new(payload_type))),
            _ => unreachable!("only MakeSuccess and MakeError are result constructors"),
        }
    }

    fn check_bracket_call(
        &mut self,
        call: &Expr,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Type, VerseError> {
        let mut checked_arg_types = None;
        if let ExprKind::Member { object, name } = &callee.kind {
            let object_type = self.check_expr(object)?;
            let extension_type = self.extension_member_type(&object_type, name, callee.span)?;
            let arg_types = args
                .iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?;

            match &object_type {
                Type::Array(item) => {
                    match self.check_array_method(name, item, args, &arg_types, callee.span) {
                        Ok(value_type) => {
                            if Self::array_method_is_failable(name) {
                                self.ensure_failable_expression_allowed(call.span)?;
                            }
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::String => {
                    match self.check_array_method(name, &Type::Char, args, &arg_types, callee.span)
                    {
                        Ok(value_type) => {
                            if Self::array_method_is_failable(name) {
                                self.ensure_failable_expression_allowed(call.span)?;
                            }
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::Int | Type::Float | Type::Rational | Type::Number => {
                    match self.check_number_method(
                        name,
                        &object_type,
                        args,
                        &arg_types,
                        callee.span,
                    ) {
                        Ok(value_type) => {
                            self.ensure_failable_expression_allowed(call.span)?;
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::Unknown | Type::Any => return Ok(Type::Unknown),
                Type::Struct(_)
                | Type::Class(_)
                | Type::Module(_)
                | Type::Result(_, _)
                | Type::Event(_)
                | Type::Task(_)
                | Type::Generator(_)
                | Type::CastableSubtype(_)
                | Type::ConcreteSubtype(_)
                | Type::ClassifiableSubset(_)
                | Type::Awaitable(_)
                | Type::Signalable(_)
                | Type::Subscribable(_)
                | Type::Listenable(_)
                | Type::Param(_, TypeParamConstraint::Subtype(_)) => {
                    checked_arg_types = Some(arg_types)
                }
                other => {
                    if extension_type.is_some() {
                        checked_arg_types = Some(arg_types);
                    } else {
                        return Err(VerseError::check_at(
                            format!("type `{other}` has no bracket method `{name}`"),
                            callee.span,
                        ));
                    }
                }
            }
        }

        let callee_type = self.check_callee_expr(callee)?;
        let arg_types = if let Some(arg_types) = checked_arg_types {
            arg_types
        } else {
            args.iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?
        };

        if is_fits_in_player_map_callee(callee) {
            ensure_exact_arg_count("FitsInPlayerMap", args, 1, callee.span)?;
            self.ensure_failable_expression_allowed(call.span)?;
            return Ok(arg_types[0].clone());
        }

        match callee_type {
            Type::Array(item) => {
                ensure_exact_arg_count("array index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "array index", args[0].span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(*item)
            }
            Type::String => {
                ensure_exact_arg_count("string index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "string index", args[0].span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(Type::Char)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) => {
                ensure_exact_arg_count("map lookup", args, 1, callee.span)?;
                let value_type =
                    self.check_map_like_lookup(&key, &value, &arg_types[0], &args[0])?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(value_type)
            }
            Type::Function {
                arity,
                arity_range,
                effects,
                mut param_types,
                mut param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions without `<decides>` must be called with `()`",
                        callee.span,
                    ));
                }
                if self.in_failure_context() {
                    ensure_callable_in_failure_context(&effects, callee.span)?;
                }
                self.ensure_callable_in_async_context(&effects, callee.span)?;
                self.ensure_current_function_allows_call_effects(&effects, callee.span)?;
                let call_args = positional_call_args(args);
                let mut return_type = *return_type;
                if let Some(inferred) =
                    infer_function_type_params(param_types.as_deref(), &arg_types)
                        .filter(|inferred| !inferred.is_empty())
                {
                    self.ensure_inferred_type_param_constraints(
                        param_types.as_deref(),
                        &inferred,
                        callee.span,
                    )?;
                    if let Some(types) = param_types.as_mut() {
                        for value_type in types {
                            *value_type = substitute_type_params(value_type, &inferred);
                        }
                    }
                    if let Some(specs) = param_specs.as_mut() {
                        for spec in specs {
                            spec.value_type = substitute_type_params(&spec.value_type, &inferred);
                        }
                    }
                    return_type =
                        self.substitute_type_params_runtime(&return_type, &inferred, callee.span)?;
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(return_type)
            }
            Type::Overload(overloads) => {
                let call_args = positional_call_args(args);
                let return_type = self.check_overloaded_call(
                    &overloads,
                    true,
                    self.in_failure_context(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(return_type)
            }
            Type::ClassType(target) => {
                let value_type = self.check_class_cast(&target, args, &arg_types, callee.span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(value_type)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot use `[]` with value of type `{other}`"),
                callee.span,
            )),
        }
    }

    fn check_array_method(
        &mut self,
        name: &str,
        item_type: &Type,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        match name {
            "Slice" => {
                ensure_arg_count_range(name, args, 1, 2, span)?;
                for (arg, arg_type) in args.iter().zip(arg_types) {
                    ensure_int_index_type(arg_type, "`Slice` argument", arg.span)?;
                }
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Find" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!("`Find` expected `{item_type}`, got `{}`", arg_types[0])
                })?;
                Ok(Type::Int)
            }
            "RemoveFirstElement" | "RemoveAllElements" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!("`{name}` expected `{item_type}`, got `{}`", arg_types[0])
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "RemoveElement" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_int_index_type(&arg_types[0], "`RemoveElement` index", args[0].span)?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Remove" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`Remove` start", args[0].span)?;
                ensure_int_index_type(&arg_types[1], "`Remove` end", args[1].span)?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceFirstElement" | "ReplaceAllElements" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!(
                        "`{name}` old value expected `{item_type}`, got `{}`",
                        arg_types[0]
                    )
                })?;
                self.ensure_expr_assignable(item_type, &arg_types[1], &args[1], || {
                    format!(
                        "`{name}` new value expected `{item_type}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceElement" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`ReplaceElement` index", args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[1], &args[1], || {
                    format!(
                        "`ReplaceElement` new value expected `{item_type}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Insert" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`Insert` index", args[0].span)?;
                let expected = Type::Array(Box::new(item_type.clone()));
                self.ensure_expr_assignable(&expected, &arg_types[1], &args[1], || {
                    format!(
                        "`Insert` values expected `{expected}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceAll" => {
                let effects = vec!["transacts".to_string()];
                self.ensure_current_function_allows_call_effects(&effects, span)?;
                ensure_exact_arg_count(name, args, 2, span)?;
                let expected = Type::Array(Box::new(item_type.clone()));
                self.ensure_expr_assignable(&expected, &arg_types[0], &args[0], || {
                    format!(
                        "`ReplaceAll` pattern expected `{expected}`, got `{}`",
                        arg_types[0]
                    )
                })?;
                self.ensure_expr_assignable(&expected, &arg_types[1], &args[1], || {
                    format!(
                        "`ReplaceAll` replacement expected `{expected}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            _ => Err(VerseError::check_at(
                format!("unknown array method `{name}`"),
                span,
            )),
        }
    }

    fn check_number_method(
        &self,
        name: &str,
        receiver_type: &Type,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        match name {
            "IsFinite" => {
                ensure_exact_arg_count(name, args, 0, span)?;
                Ok(receiver_type.clone())
            }
            "IsAlmostZero" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_number_like(
                    &arg_types[0],
                    "`IsAlmostZero` AbsoluteTolerance",
                    args[0].span,
                )?;
                Ok(Type::None)
            }
            _ => Err(VerseError::check_at(
                format!("unknown number method `{name}`"),
                span,
            )),
        }
    }

    fn array_method_is_failable(name: &str) -> bool {
        matches!(
            name,
            "Slice"
                | "Find"
                | "RemoveFirstElement"
                | "RemoveElement"
                | "Remove"
                | "ReplaceFirstElement"
                | "ReplaceElement"
                | "Insert"
        )
    }

    fn ensure_aggregate_member_accessible(
        &self,
        owner: &str,
        access: AccessLevel,
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if self.interface_types.contains_key(owner) {
            self.ensure_interface_member_accessible(owner, access, member_name, member_kind, span)
        } else {
            self.ensure_class_member_accessible(owner, access, member_name, member_kind, span)
        }
    }

    fn ensure_class_member_accessible(
        &self,
        owner_class: &str,
        access: AccessLevel,
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal => {
                let current_module = self.current_module_name();
                if current_module.as_deref() == aggregate_module_name(owner_class) {
                    Ok(())
                } else {
                    let module_name = aggregate_module_name(owner_class).unwrap_or("<root module>");
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is internal to module `{module_name}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Private => {
                if self
                    .class_context
                    .last()
                    .is_some_and(|current| current == owner_class)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is private to class `{owner_class}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Protected => {
                if self.class_context.last().is_some_and(|current| {
                    current == owner_class || self.is_class_subtype(current, owner_class)
                }) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is protected in class `{owner_class}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    fn ensure_interface_member_accessible(
        &self,
        owner_interface: &str,
        access: AccessLevel,
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal => {
                let current_module = self.current_module_name();
                if current_module.as_deref() == aggregate_module_name(owner_interface) {
                    Ok(())
                } else {
                    let module_name =
                        aggregate_module_name(owner_interface).unwrap_or("<root module>");
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is internal to module `{module_name}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Private => Err(VerseError::check_at(
                format!(
                    "{member_kind} `{member_name}` is private to interface `{owner_interface}`"
                ),
                span,
            )),
            AccessLevel::Protected => {
                if self.class_context.last().is_some_and(|current| {
                    self.class_implements_interface(current, owner_interface)
                }) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is protected in interface `{owner_interface}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    fn ensure_module_member_accessible(
        &self,
        module_name: &str,
        access: AccessLevel,
        member_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal | AccessLevel::Private | AccessLevel::Protected => {
                if self.current_module_name().as_deref() == Some(module_name) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("member `{member_name}` is internal to module `{module_name}`"),
                        span,
                    ))
                }
            }
        }
    }

    fn check_member(
        &mut self,
        object_type: &Type,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.check_member(&supertype, name, span, allow_extension_method);
        }

        if let Type::Module(module_name) = object_type {
            let Some(info) = self.module_types.get(module_name) else {
                return Err(VerseError::check_at(
                    format!("unknown module `{module_name}`"),
                    span,
                ));
            };
            let Some(value_type) = info.members.get(name) else {
                return Err(VerseError::check_at(
                    format!("module `{module_name}` has no member `{name}`"),
                    span,
                ));
            };
            let access = info
                .member_access
                .get(name)
                .copied()
                .unwrap_or(AccessLevel::Internal);
            self.ensure_module_member_accessible(module_name, access, name, span)?;
            return Ok(value_type.clone());
        }

        if let Type::EnumType(enum_name) = object_type {
            let Some(info) = self.enum_types.get(enum_name) else {
                return Err(VerseError::check_at(
                    format!("unknown enum `{enum_name}`"),
                    span,
                ));
            };
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        if let Type::Result(success_type, error_type) = object_type {
            return match name {
                "GetSuccess" => Ok(result_accessor_type(success_type.as_ref())),
                "GetError" => Ok(result_accessor_type(error_type.as_ref())),
                _ => Err(VerseError::check_at(
                    format!("interface `{object_type}` has no member `{name}`"),
                    span,
                )),
            };
        }

        match object_type {
            Type::Event(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Signal" => Ok(signal_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Task(payload) => {
                return match name {
                    "Await" => Ok(await_type(Some(payload.as_ref()))),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::ClassifiableSubset(item) => {
                return match name {
                    "Contains" => Ok(classifiable_subset_contains_type(item.as_ref())),
                    "ContainsAny" | "ContainsAll" => {
                        Ok(classifiable_subset_contains_many_type(item.as_ref()))
                    }
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Modifier(item) => {
                return match name {
                    "Evaluate" => Ok(modifier_evaluate_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::ModifierStack(item) => {
                return match name {
                    "FirstPosition" | "LastPosition" => Ok(Type::Option(Box::new(Type::Rational))),
                    "Evaluate" => Ok(modifier_evaluate_type(item.as_ref())),
                    "AddModifier" => Ok(modifier_stack_add_modifier_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Awaitable(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Signalable(payload) => {
                return match name {
                    "Signal" => Ok(signal_type(Some(payload.as_ref()))),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Subscribable(payload) => {
                return match name {
                    "Subscribe" => Ok(subscribe_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Listenable(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Subscribe" => Ok(subscribe_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            _ => {}
        }

        if let Type::Interface(interface_name) = object_type {
            let Some(info) = self.interface_types.get(interface_name) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface_name}`"),
                    span,
                ));
            };
            if let Some(field) = info.fields.iter().find(|field| field.name == name) {
                let owner = field.owner.as_deref().unwrap_or(interface_name);
                self.ensure_aggregate_member_accessible(owner, field.access, name, "field", span)?;
                return Ok(field.value_type.clone());
            }
            if let Some(method_type) =
                method_group_type(info.methods.iter().filter(|method| method.name == name))
            {
                for method in info.methods.iter().filter(|method| method.name == name) {
                    let owner = method.owner.as_deref().unwrap_or(interface_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        method.access,
                        name,
                        "method",
                        span,
                    )?;
                }
                if matches!(method_type, Type::Overload(_)) && !allow_extension_method {
                    return Err(VerseError::check_at(
                        format!("overloaded method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            return Err(VerseError::check_at(
                format!("interface `{interface_name}` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "session" && !self.struct_types.contains_key(class_name))
        {
            if name == "Environment" {
                return Ok(Type::Function {
                    arity: Some(0),
                    arity_range: None,
                    effects: vec!["transacts".to_string()],
                    param_types: Some(Vec::new()),
                    param_specs: None,
                    return_type: Box::new(Type::Enum("session_environment".to_string())),
                });
            }
            return Err(VerseError::check_at(
                format!("class `session` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "player" && !self.struct_types.contains_key(class_name))
        {
            if name == "IsActive" {
                return Ok(Type::Function {
                    arity: Some(0),
                    arity_range: None,
                    effects: vec![
                        "reads".to_string(),
                        "computes".to_string(),
                        "decides".to_string(),
                    ],
                    param_types: Some(Vec::new()),
                    param_specs: None,
                    return_type: Box::new(Type::None),
                });
            }
            return Err(VerseError::check_at(
                format!("class `player` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "team" && !self.struct_types.contains_key(class_name))
        {
            return Err(VerseError::check_at(
                format!("class `team` has no member `{name}`"),
                span,
            ));
        }

        if let Type::Struct(aggregate_name) | Type::Class(aggregate_name) = object_type {
            let Some(info) = self.struct_types.get(aggregate_name) else {
                let label = if matches!(object_type, Type::Class(_)) {
                    "class"
                } else {
                    "struct"
                };
                return Err(VerseError::check_at(
                    format!("unknown {label} `{aggregate_name}`"),
                    span,
                ));
            };
            let label = match info.kind {
                AggregateKind::Struct => "struct",
                AggregateKind::Class => "class",
            };
            let member_label = if info.kind == AggregateKind::Class {
                "member"
            } else {
                "field"
            };
            if let Some(field) = info.fields.iter().find(|field| field.name == name) {
                if info.kind == AggregateKind::Class {
                    let owner = field.owner.as_deref().unwrap_or(aggregate_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        field.access,
                        name,
                        "field",
                        span,
                    )?;
                }
                return Ok(field.value_type.clone());
            }
            if info.kind == AggregateKind::Class
                && let Some(method_type) =
                    method_group_type(info.methods.iter().filter(|method| method.name == name))
            {
                for method in info.methods.iter().filter(|method| method.name == name) {
                    let owner = method.owner.as_deref().unwrap_or(aggregate_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        method.access,
                        name,
                        "method",
                        span,
                    )?;
                }
                if matches!(method_type, Type::Overload(_)) && !allow_extension_method {
                    return Err(VerseError::check_at(
                        format!("overloaded method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            if let Some(method_type) = self.extension_member_type(object_type, name, span)? {
                if !allow_extension_method {
                    return Err(VerseError::check_at(
                        format!("extension method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }

            return Err(VerseError::check_at(
                format!("{label} `{aggregate_name}` has no {member_label} `{name}`"),
                span,
            ));
        }

        if let Some(method_type) = self.extension_member_type(object_type, name, span)? {
            if !allow_extension_method {
                return Err(VerseError::check_at(
                    format!("extension method `{name}` must be called"),
                    span,
                ));
            }
            return Ok(method_type);
        }

        if name != "Length" {
            return Err(VerseError::check_at(
                format!("unknown member `{name}` on type `{object_type}`"),
                span,
            ));
        }

        match object_type {
            Type::Array(_) | Type::Map(_, _) | Type::String => Ok(Type::Int),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("type `{other}` has no member `Length`"),
                span,
            )),
        }
    }

    fn extension_member_type(
        &mut self,
        object_type: &Type,
        name: &str,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.extension_member_type(&supertype, name, span);
        }

        let Some(methods) = self.extension_methods.get(name) else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        for method in methods
            .iter()
            .filter(|method| self.is_assignable(&method.receiver_type, object_type))
        {
            if self.extension_method_is_visible(method, name, span)? {
                candidates.push(method.method_type.clone());
            }
        }

        Ok(match candidates.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Type::Overload(candidates)),
        })
    }

    fn qualified_extension_member_type(
        &mut self,
        object_type: &Type,
        qualifier: &str,
        name: &str,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.qualified_extension_member_type(&supertype, qualifier, name, span);
        }

        let Some(methods) = self.extension_methods.get(name) else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        for method in methods.iter().filter(|method| {
            self.is_assignable(&method.receiver_type, object_type)
                && extension_method_has_qualifier(method, qualifier)
        }) {
            if self.extension_method_is_visible(method, name, span)? {
                candidates.push(method.method_type.clone());
            }
        }

        Ok(match candidates.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Type::Overload(candidates)),
        })
    }

    fn extension_method_is_visible(
        &self,
        method: &ExtensionMethodInfo,
        name: &str,
        span: Span,
    ) -> Result<bool, VerseError> {
        match method.module_name.as_deref() {
            None => Ok(true),
            Some(module_name) if self.current_module_name().as_deref() == Some(module_name) => {
                Ok(true)
            }
            Some(module_name) => {
                let imported = self
                    .scope_imports
                    .iter()
                    .rev()
                    .any(|imports| imports.iter().any(|import| import == module_name));
                if !imported {
                    return Ok(false);
                }
                self.ensure_module_member_accessible(module_name, method.access, name, span)?;
                Ok(true)
            }
        }
    }

    fn check_unary(&mut self, op: UnaryOp, expr: &Expr) -> Result<Type, VerseError> {
        let value = self.check_expr(expr)?;
        match op {
            UnaryOp::Positive => {
                ensure_number_like(&value, "unary `+`", expr.span)?;
                Ok(value)
            }
            UnaryOp::Negate => {
                ensure_number_like(&value, "unary `-`", expr.span)?;
                Ok(value)
            }
            UnaryOp::Not => {
                ensure_bool_like(&value, "`not`", expr.span)?;
                Ok(Type::Bool)
            }
        }
    }

    fn check_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match op {
            BinaryOp::Add => {
                let result = check_add(&left_type, left.span, &right_type, right.span)?;
                if matches!(
                    (&left_type, &right_type),
                    (Type::ClassifiableSubset(_), Type::ClassifiableSubset(_))
                ) {
                    self.ensure_current_function_allows_call_effects(
                        &["transacts".to_string()],
                        left.span.through(right.span),
                    )?;
                }
                Ok(result)
            }
            BinaryOp::Subtract => check_subtract(&left_type, left.span, &right_type, right.span),
            BinaryOp::Multiply => check_multiply(&left_type, left.span, &right_type, right.span),
            BinaryOp::Remainder => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
                Ok(unify_numeric_types(&left_type, &right_type))
            }
            BinaryOp::Divide => check_divide(&left_type, left.span, &right_type, right.span),
            BinaryOp::Range => {
                if self.range_context_depth == 0 {
                    return Err(VerseError::check_at(
                        "range expressions are only valid in `for` expressions",
                        left.span.through(right.span),
                    ));
                }
                ensure_number_like(&left_type, "range start", left.span)?;
                ensure_number_like(&right_type, "range end", right.span)?;
                Ok(Type::Range)
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                ensure_equality_comparable(&left_type, &self.struct_types, left.span)?;
                ensure_equality_comparable(&right_type, &self.struct_types, right.span)?;
                Ok(Type::Bool)
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
                Ok(Type::Bool)
            }
            BinaryOp::And | BinaryOp::Or => {
                ensure_bool_like(&left_type, "left operand", left.span)?;
                ensure_bool_like(&right_type, "right operand", right.span)?;
                Ok(Type::Bool)
            }
        }
    }

    fn define(
        &mut self,
        name: &str,
        value_type: Type,
        mutable: bool,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        if current.contains_key(name) {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }
        current.insert(
            name.to_string(),
            Symbol {
                value_type,
                mutable,
            },
        );
        Ok(())
    }

    fn define_predeclared_aggregate_value(
        &mut self,
        name: &str,
        qualified: &str,
        value_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        if current.contains_key(name) {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }
        current.insert(name.to_string(), Symbol::immutable(value_type));
        self.predeclared_aggregate_values
            .insert(qualified.to_string());
        Ok(())
    }

    fn define_aggregate_value(
        &mut self,
        name: &str,
        qualified: &str,
        value_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        if self.predeclared_aggregate_values.remove(qualified) {
            let current = self
                .scopes
                .last_mut()
                .expect("checker should always have a scope");
            match current.get_mut(name) {
                Some(symbol) if !symbol.mutable && symbol.value_type == value_type => {
                    symbol.value_type = value_type;
                    return Ok(());
                }
                _ => {
                    return Err(VerseError::check_at(
                        format!("duplicate definition `{name}`"),
                        span,
                    ));
                }
            }
        }

        self.define(name, value_type, false, span)
    }

    fn push_class_member_shadow_names(&mut self, class_name: &str, fields: &[StructFieldInfo]) {
        let mut names = fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<HashSet<_>>();
        if let Some(info) = self.struct_types.get(class_name) {
            names.extend(info.methods.iter().map(|method| method.name.clone()));
        }
        self.class_member_shadow_names.push(names);
    }

    fn pop_class_member_shadow_names(&mut self) {
        self.class_member_shadow_names
            .pop()
            .expect("class member shadow stack should not underflow");
    }

    fn ensure_not_shadowing_class_member(&self, name: &str, span: Span) -> Result<(), VerseError> {
        if self
            .class_member_shadow_names
            .last()
            .is_some_and(|members| members.contains(name))
        {
            Err(VerseError::check_at(
                format!("definition `{name}` cannot shadow class member `{name}`"),
                span,
            ))
        } else {
            Ok(())
        }
    }

    fn lookup(&self, name: &str) -> Option<Symbol> {
        for (scope, imports) in self.scopes.iter().zip(&self.scope_imports).rev() {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol.clone());
            }
            for module_name in imports.iter().rev() {
                if let Some(value_type) = self
                    .module_types
                    .get(module_name)
                    .and_then(|module| module.members.get(name))
                {
                    return Some(Symbol {
                        value_type: value_type.clone(),
                        mutable: false,
                    });
                }
            }
        }
        None
    }

    fn lookup_accessible(&self, name: &str, span: Span) -> Result<Option<Symbol>, VerseError> {
        for (scope, imports) in self.scopes.iter().zip(&self.scope_imports).rev() {
            if let Some(symbol) = scope.get(name) {
                return Ok(Some(symbol.clone()));
            }
            for module_name in imports.iter().rev() {
                let Some(module) = self.module_types.get(module_name) else {
                    continue;
                };
                let Some(value_type) = module.members.get(name) else {
                    continue;
                };
                let access = module
                    .member_access
                    .get(name)
                    .copied()
                    .unwrap_or(AccessLevel::Internal);
                self.ensure_module_member_accessible(module_name, access, name, span)?;
                return Ok(Some(Symbol {
                    value_type: value_type.clone(),
                    mutable: false,
                }));
            }
        }
        Ok(None)
    }

    fn is_current_predeclared_function(&self, name: &str) -> bool {
        matches!(
            self.scopes
                .last()
                .and_then(|scope| scope.get(name))
                .map(|symbol| &symbol.value_type),
            Some(Type::Function { .. } | Type::Overload(_))
        )
    }

    fn define_or_overload_function(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<Type, VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type.clone()));
            return Ok(function_type);
        };

        if existing.mutable {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }

        match &mut existing.value_type {
            Type::Function { .. } => {
                if function_signatures_conflict(
                    &existing.value_type,
                    &function_type,
                    &self.struct_types,
                ) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                let previous = existing.value_type.clone();
                existing.value_type = Type::Overload(vec![previous, function_type]);
                Ok(existing.value_type.clone())
            }
            Type::Overload(overloads) => {
                if overloads.iter().any(|overload| {
                    function_signatures_conflict(overload, &function_type, &self.struct_types)
                }) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                overloads.push(function_type);
                Ok(existing.value_type.clone())
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    fn update_current_function_binding(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<Type, VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type.clone()));
            return Ok(function_type);
        };

        match &mut existing.value_type {
            Type::Function { .. } => {
                existing.value_type = function_type.clone();
                Ok(function_type)
            }
            Type::Overload(overloads) => {
                if let Some(index) = overloads.iter().position(|overload| {
                    function_signatures_match_exactly(overload, &function_type)
                }) {
                    overloads[index] = function_type;
                    return Ok(existing.value_type.clone());
                }
                Err(VerseError::check_at(
                    format!("unknown predeclared overload `{name}`"),
                    span,
                ))
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    fn validate_function_overloads_in_current_scope(&self) -> Result<(), VerseError> {
        let current = self
            .scopes
            .last()
            .expect("checker should always have a scope");
        for (name, symbol) in current {
            let Type::Overload(overloads) = &symbol.value_type else {
                continue;
            };
            for (index, left) in overloads.iter().enumerate() {
                if overloads
                    .iter()
                    .skip(index + 1)
                    .any(|right| function_signatures_conflict(left, right, &self.struct_types))
                {
                    return Err(VerseError::check(format!("duplicate overload `{name}`")));
                }
            }
        }
        Ok(())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_imports.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.scope_imports.pop();
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Symbol {
    fn immutable(value_type: Type) -> Self {
        Self {
            value_type,
            mutable: false,
        }
    }
}

fn call_arg_expr(arg: &CallArg) -> &Expr {
    match arg {
        CallArg::Positional(expr) => expr,
        CallArg::Named { expr, .. } => expr,
    }
}

fn type_returns_never(value_type: &Type) -> bool {
    match value_type {
        Type::Function { return_type, .. } => return_type.as_ref() == &Type::Never,
        Type::Overload(overloads) => {
            !overloads.is_empty() && overloads.iter().all(type_returns_never)
        }
        _ => false,
    }
}

fn spawn_body_expr(body: &Expr) -> Result<&Expr, VerseError> {
    let ExprKind::Block(statements) = &body.kind else {
        return Err(VerseError::check_at(
            "`spawn` expects a braced expression body",
            body.span,
        ));
    };
    let [statement] = statements.as_slice() else {
        return Err(VerseError::check_at(
            "`spawn` body must contain exactly one expression",
            body.span,
        ));
    };
    let StmtKind::Expr(expr) = &statement.kind else {
        return Err(VerseError::check_at(
            "`spawn` body must contain exactly one expression",
            statement.span,
        ));
    };
    Ok(expr)
}

fn concurrent_body_statements(body: &Expr) -> Result<&[Stmt], VerseError> {
    let ExprKind::ColonBlock(statements) = &body.kind else {
        return Err(VerseError::check_at(
            "concurrency expression expects an indented block body",
            body.span,
        ));
    };
    Ok(statements)
}

fn concurrent_op_name(op: ConcurrentOp) -> &'static str {
    match op {
        ConcurrentOp::Sync => "sync",
        ConcurrentOp::Race => "race",
        ConcurrentOp::Rush => "rush",
        ConcurrentOp::Branch => "branch",
    }
}

fn official_event_archetype_args(callee: &Expr) -> Option<&[CallArg]> {
    let ExprKind::Call { callee, args } = &callee.kind else {
        return None;
    };
    matches!(&callee.kind, ExprKind::Ident(name) if name == "event").then_some(args.as_slice())
}

fn is_official_event_archetype_callee(callee: &Expr) -> bool {
    official_event_archetype_args(callee).is_some()
}

fn expr_to_type_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Member { object, name } => {
            let mut path = expr_to_type_path(object)?;
            path.push('.');
            path.push_str(name);
            Some(path)
        }
        ExprKind::QualifiedName { qualifier, name } => Some(format!("{qualifier}.{name}")),
        _ => None,
    }
}

fn class_definition_diagnostic_span(
    base: Option<&TypeAnnotation>,
    fields: &[StructField],
    methods: &[ClassMethod],
    extension_methods: &[ExtensionMethod],
    blocks: &[ClassBlock],
) -> Span {
    base.map(|base| base.span)
        .or_else(|| fields.first().map(|field| field.span))
        .or_else(|| methods.first().map(|method| method.span))
        .or_else(|| extension_methods.first().map(|method| method.span))
        .or_else(|| blocks.first().map(|block| block.span))
        .unwrap_or_else(|| Span::new(0, 0, 1, 1))
}

fn method_binding_types(methods: &[ClassMethodInfo]) -> Vec<(String, Type)> {
    let mut grouped: Vec<(String, Vec<Type>)> = Vec::new();
    for method in methods {
        if let Some((_, overloads)) = grouped.iter_mut().find(|(name, _)| name == &method.name) {
            overloads.push(method.value_type.clone());
        } else {
            grouped.push((method.name.clone(), vec![method.value_type.clone()]));
        }
    }

    grouped
        .into_iter()
        .map(|(name, overloads)| {
            let value_type = match overloads.as_slice() {
                [single] => single.clone(),
                _ => Type::Overload(overloads),
            };
            (name, value_type)
        })
        .collect()
}

fn method_group_type<'a>(methods: impl IntoIterator<Item = &'a ClassMethodInfo>) -> Option<Type> {
    let overloads = methods
        .into_iter()
        .map(|method| method.value_type.clone())
        .collect::<Vec<_>>();
    match overloads.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(Type::Overload(overloads)),
    }
}

fn qualifier_matches(stored: &str, requested: &str) -> bool {
    stored == requested
        || stored.rsplit('.').next() == Some(requested)
        || requested.rsplit('.').next() == Some(stored)
}

fn method_has_qualifier(method: &ClassMethodInfo, qualifier: &str) -> bool {
    method
        .qualifier
        .as_deref()
        .is_some_and(|stored| qualifier_matches(stored, qualifier))
}

fn extension_method_has_qualifier(method: &ExtensionMethodInfo, qualifier: &str) -> bool {
    method
        .module_name
        .as_deref()
        .is_some_and(|stored| qualifier_matches(stored, qualifier))
}

fn method_qualifiers_conflict(left: &ClassMethodInfo, right: &ClassMethodInfo) -> bool {
    match (left.qualifier.as_deref(), right.qualifier.as_deref()) {
        (Some(left), Some(right)) => qualifier_matches(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn method_signatures_conflict(
    left: &ClassMethodInfo,
    right: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    left.name == right.name
        && function_signatures_conflict(&left.value_type, &right.value_type, struct_types)
}

fn inherited_method_override_index(
    inherited_methods: &[ClassMethodInfo],
    method: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<Option<usize>, VerseError> {
    let candidates = inherited_methods
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            method_signatures_conflict(candidate, method, struct_types).then_some(index)
        })
        .collect::<Vec<_>>();

    if method.qualifier.is_some() {
        return Ok(candidates
            .into_iter()
            .find(|index| method_qualifiers_conflict(&inherited_methods[*index], method)));
    }

    if let Some(index) = candidates
        .iter()
        .copied()
        .find(|index| method_qualifiers_conflict(&inherited_methods[*index], method))
    {
        return Ok(Some(index));
    }

    match candidates.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(VerseError::check_at(
            format!(
                "method `{}` override is ambiguous; use a qualified method name",
                method.name
            ),
            method.span,
        )),
    }
}

fn inherited_method_duplicate_index(
    inherited_methods: &[ClassMethodInfo],
    method: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> Option<usize> {
    inherited_methods.iter().position(|candidate| {
        method_signatures_conflict(candidate, method, struct_types)
            && (method.qualifier.is_none() || method_qualifiers_conflict(candidate, method))
    })
}

fn push_distinct_local_method_info(
    infos: &mut Vec<ClassMethodInfo>,
    info: ClassMethodInfo,
    aggregate_kind: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<(), VerseError> {
    if infos.iter().any(|existing| {
        existing.name == info.name
            && method_qualifiers_conflict(existing, &info)
            && function_signatures_conflict(&existing.value_type, &info.value_type, struct_types)
    }) {
        return Err(VerseError::check_at(
            format!("duplicate {aggregate_kind} method overload `{}`", info.name),
            info.span,
        ));
    }
    infos.push(info);
    Ok(())
}

fn function_signatures_match_exactly(left: &Type, right: &Type) -> bool {
    let (
        Type::Function {
            arity: left_arity,
            arity_range: left_arity_range,
            param_types: left_param_types,
            param_specs: left_param_specs,
            ..
        },
        Type::Function {
            arity: right_arity,
            arity_range: right_arity_range,
            param_types: right_param_types,
            param_specs: right_param_specs,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };

    left_arity == right_arity
        && left_arity_range == right_arity_range
        && left_param_types == right_param_types
        && exact_param_specs_key(left_param_specs.as_deref())
            == exact_param_specs_key(right_param_specs.as_deref())
}

fn exact_param_specs_key(specs: Option<&[ParamSpec]>) -> Option<Vec<(bool, String, Type)>> {
    let specs = specs?;
    let mut key = specs
        .iter()
        .map(|spec| {
            (
                spec.named,
                if spec.named {
                    spec.name.clone()
                } else {
                    String::new()
                },
                spec.value_type.clone(),
            )
        })
        .collect::<Vec<_>>();
    if key.iter().all(|(named, _, _)| *named) {
        key.sort_by(|left, right| left.1.cmp(&right.1));
    }
    Some(key)
}

fn function_signatures_conflict(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let (
        Type::Function {
            arity: left_arity,
            arity_range: left_arity_range,
            param_types: left_param_types,
            param_specs: left_param_specs,
            ..
        },
        Type::Function {
            arity: right_arity,
            arity_range: right_arity_range,
            param_types: right_param_types,
            param_specs: right_param_specs,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };

    if left_arity_range != right_arity_range {
        return false;
    }

    if let (Some(left_specs), Some(right_specs)) =
        (left_param_specs.as_deref(), right_param_specs.as_deref())
    {
        return param_specs_overlap(left_specs, right_specs, struct_types);
    }

    left_arity == right_arity
        && param_type_lists_overlap(
            left_param_types.as_deref(),
            right_param_types.as_deref(),
            struct_types,
        )
}

fn param_specs_overlap(
    left: &[ParamSpec],
    right: &[ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if param_specs_overlap_direct(left, right, struct_types) {
        return true;
    }

    let left_variants = expanded_single_tuple_param_spec_variants(left);
    let right_variants = expanded_single_tuple_param_spec_variants(right);

    for left_variant in &left_variants {
        if param_specs_overlap_direct(left_variant, right, struct_types) {
            return true;
        }
        for right_variant in &right_variants {
            if param_specs_overlap_direct(left_variant, right_variant, struct_types) {
                return true;
            }
        }
    }

    right_variants
        .iter()
        .any(|right_variant| param_specs_overlap_direct(left, right_variant, struct_types))
}

fn expanded_single_tuple_param_spec_variants(specs: &[ParamSpec]) -> Vec<Vec<ParamSpec>> {
    let [single] = specs else {
        return Vec::new();
    };
    let Some(items) = &single.tuple_items else {
        return Vec::new();
    };

    let mut variants = vec![items.clone()];
    variants.extend(expanded_single_tuple_param_spec_variants(items));
    variants
}

fn param_specs_overlap_direct(
    left: &[ParamSpec],
    right: &[ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let left_positional = left
        .iter()
        .filter(|spec| !spec.named)
        .map(|spec| &spec.value_type)
        .collect::<Vec<_>>();
    let right_positional = right
        .iter()
        .filter(|spec| !spec.named)
        .map(|spec| &spec.value_type)
        .collect::<Vec<_>>();

    if !param_type_slices_overlap(&left_positional, &right_positional, struct_types) {
        return false;
    }

    let left_named = left.iter().filter(|spec| spec.named).collect::<Vec<_>>();
    let right_named = right.iter().filter(|spec| spec.named).collect::<Vec<_>>();

    required_named_params_are_accepted_by(&left_named, &right_named, struct_types)
        && required_named_params_are_accepted_by(&right_named, &left_named, struct_types)
}

fn required_named_params_are_accepted_by(
    required_source: &[&ParamSpec],
    target: &[&ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    required_source
        .iter()
        .filter(|spec| !spec.has_default)
        .all(|required| {
            target
                .iter()
                .find(|candidate| candidate.name == required.name)
                .is_some_and(|candidate| {
                    types_not_distinct(&required.value_type, &candidate.value_type, struct_types)
                })
        })
}

fn param_type_lists_overlap(
    left: Option<&[Type]>,
    right: Option<&[Type]>,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left_refs = left.iter().collect::<Vec<_>>();
            let right_refs = right.iter().collect::<Vec<_>>();
            param_type_slices_overlap(&left_refs, &right_refs, struct_types)
        }
        _ => true,
    }
}

fn param_type_slices_overlap(
    left: &[&Type],
    right: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| types_not_distinct(left, right, struct_types))
    {
        return true;
    }

    if let [single] = left
        && single_param_overlaps_sequence(single, right, struct_types)
    {
        return true;
    }

    if let [single] = right {
        return single_param_overlaps_sequence(single, left, struct_types);
    }

    false
}

fn single_param_overlaps_sequence(
    single: &Type,
    sequence: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match single {
        Type::Tuple(items) if items.len() == sequence.len() => items
            .iter()
            .zip(sequence)
            .all(|(item, sequence_type)| types_not_distinct(item, sequence_type, struct_types)),
        _ => false,
    }
}

fn types_not_distinct(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if left == right {
        return true;
    }

    match (left, right) {
        (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown) => true,
        (Type::None, _) | (_, Type::None) => true,
        (Type::Option(_), Type::Bool) | (Type::Bool, Type::Option(_)) => true,
        (Type::Array(_), Type::Map(_, _) | Type::WeakMap(_, _))
        | (Type::Map(_, _) | Type::WeakMap(_, _), Type::Array(_)) => true,
        (Type::Function { .. }, Type::Array(_) | Type::Map(_, _) | Type::WeakMap(_, _))
        | (Type::Array(_) | Type::Map(_, _) | Type::WeakMap(_, _), Type::Function { .. }) => true,
        (Type::Function { .. }, Type::Function { .. }) => true,
        (Type::Interface(_), Type::Class(_)) | (Type::Class(_), Type::Interface(_)) => true,
        (Type::Class(left), Type::Class(right)) => {
            class_types_not_distinct(left, right, struct_types)
        }
        (Type::Tuple(items), Type::Array(item)) | (Type::Array(item), Type::Tuple(items)) => items
            .iter()
            .any(|tuple_item| types_not_distinct(tuple_item, item, struct_types)),
        (Type::Tuple(items), Type::Map(key, value))
        | (Type::Map(key, value), Type::Tuple(items)) => {
            matches!(key.as_ref(), Type::Int)
                && items
                    .iter()
                    .any(|tuple_item| types_not_distinct(tuple_item, value, struct_types))
        }
        (Type::Tuple(items), Type::Option(item)) | (Type::Option(item), Type::Tuple(items)) => {
            matches!(items.as_slice(), [single] if types_not_distinct(single, item, struct_types))
        }
        _ => false,
    }
}

fn class_types_not_distinct(
    left: &str,
    right: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    left == right
        || class_is_subtype_of(left, right, struct_types)
        || class_is_subtype_of(right, left, struct_types)
}

fn class_is_subtype_of(
    child: &str,
    parent: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let mut current = Some(child);
    while let Some(name) = current {
        if name == parent {
            return true;
        }
        current = struct_types.get(name).and_then(|info| info.base.as_deref());
    }
    false
}

fn positional_call_args(args: &[Expr]) -> Vec<CallArg> {
    args.iter().cloned().map(CallArg::Positional).collect()
}

fn infer_function_type_params(
    param_types: Option<&[Type]>,
    arg_types: &[Type],
) -> Option<HashMap<String, Type>> {
    let param_types = param_types?;
    if param_types.len() != arg_types.len() {
        return Some(HashMap::new());
    }
    let mut inferred = HashMap::new();
    for (param_type, arg_type) in param_types.iter().zip(arg_types) {
        infer_type_params_from_type(param_type, arg_type, &mut inferred)?;
    }
    Some(inferred)
}

fn infer_type_params_from_type(
    pattern: &Type,
    actual: &Type,
    inferred: &mut HashMap<String, Type>,
) -> Option<()> {
    match (pattern, actual) {
        (Type::Param(name, _), actual) => {
            if inferred.contains_key(name) {
                Some(())
            } else {
                inferred.insert(name.clone(), actual.clone());
                Some(())
            }
        }
        (Type::Array(pattern), Type::Array(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (Type::Map(pattern_key, pattern_value), Type::Map(actual_key, actual_value))
        | (Type::WeakMap(pattern_key, pattern_value), Type::WeakMap(actual_key, actual_value)) => {
            infer_type_params_from_type(pattern_key, actual_key, inferred)?;
            infer_type_params_from_type(pattern_value, actual_value, inferred)
        }
        (Type::Tuple(pattern_items), Type::Tuple(actual_items))
            if pattern_items.len() == actual_items.len() =>
        {
            for (pattern, actual) in pattern_items.iter().zip(actual_items) {
                infer_type_params_from_type(pattern, actual, inferred)?;
            }
            Some(())
        }
        (Type::Option(pattern), Type::Option(actual))
        | (Type::Task(pattern), Type::Task(actual))
        | (Type::CastableSubtype(pattern), Type::CastableSubtype(actual))
        | (Type::ConcreteSubtype(pattern), Type::ConcreteSubtype(actual))
        | (Type::ClassifiableSubset(pattern), Type::ClassifiableSubset(actual))
        | (Type::Modifier(pattern), Type::Modifier(actual))
        | (Type::ModifierStack(pattern), Type::ModifierStack(actual))
        | (Type::Signalable(pattern), Type::Signalable(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (
            Type::Result(pattern_success, pattern_error),
            Type::Result(actual_success, actual_error),
        ) => {
            infer_type_params_from_type(pattern_success, actual_success, inferred)?;
            infer_type_params_from_type(pattern_error, actual_error, inferred)
        }
        (Type::Event(pattern), Type::Event(actual))
        | (Type::Generator(pattern), Type::Generator(actual))
        | (Type::Awaitable(pattern), Type::Awaitable(actual))
        | (Type::Subscribable(pattern), Type::Subscribable(actual))
        | (Type::Listenable(pattern), Type::Listenable(actual)) => match (pattern, actual) {
            (Some(pattern), Some(actual)) => infer_type_params_from_type(pattern, actual, inferred),
            _ => Some(()),
        },
        (
            Type::Function {
                param_types: pattern_params,
                return_type: pattern_return,
                ..
            },
            Type::Function {
                param_types: actual_params,
                return_type: actual_return,
                ..
            },
        ) => {
            if let (Some(pattern_params), Some(actual_params)) = (pattern_params, actual_params) {
                if pattern_params.len() != actual_params.len() {
                    return None;
                }
                for (pattern, actual) in pattern_params.iter().zip(actual_params) {
                    infer_type_params_from_type(pattern, actual, inferred)?;
                }
            }
            infer_type_params_from_type(pattern_return, actual_return, inferred)
        }
        _ => Some(()),
    }
}

fn substitute_type_params(value_type: &Type, inferred: &HashMap<String, Type>) -> Type {
    match value_type {
        Type::Param(name, _) => inferred
            .get(name)
            .cloned()
            .unwrap_or_else(|| value_type.clone()),
        Type::Array(item) => Type::Array(Box::new(substitute_type_params(item, inferred))),
        Type::Map(key, value) => Type::Map(
            Box::new(substitute_type_params(key, inferred)),
            Box::new(substitute_type_params(value, inferred)),
        ),
        Type::WeakMap(key, value) => Type::WeakMap(
            Box::new(substitute_type_params(key, inferred)),
            Box::new(substitute_type_params(value, inferred)),
        ),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| substitute_type_params(item, inferred))
                .collect(),
        ),
        Type::Option(item) => Type::Option(Box::new(substitute_type_params(item, inferred))),
        Type::Result(success, error) => Type::Result(
            Box::new(substitute_type_params(success, inferred)),
            Box::new(substitute_type_params(error, inferred)),
        ),
        Type::Event(payload) => Type::Event(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Task(payload) => Type::Task(Box::new(substitute_type_params(payload, inferred))),
        Type::Generator(payload) => Type::Generator(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::CastableSubtype(item) => {
            Type::CastableSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ConcreteSubtype(item) => {
            Type::ConcreteSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ClassifiableSubset(item) => {
            Type::ClassifiableSubset(Box::new(substitute_type_params(item, inferred)))
        }
        Type::Modifier(item) => Type::Modifier(Box::new(substitute_type_params(item, inferred))),
        Type::ModifierStack(item) => {
            Type::ModifierStack(Box::new(substitute_type_params(item, inferred)))
        }
        Type::Awaitable(payload) => Type::Awaitable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Signalable(payload) => {
            Type::Signalable(Box::new(substitute_type_params(payload, inferred)))
        }
        Type::Subscribable(payload) => Type::Subscribable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Listenable(payload) => Type::Listenable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Function {
            arity,
            arity_range,
            effects,
            param_types,
            param_specs,
            return_type,
        } => Type::Function {
            arity: *arity,
            arity_range: *arity_range,
            effects: effects.clone(),
            param_types: param_types.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| substitute_type_params(param, inferred))
                    .collect()
            }),
            param_specs: param_specs.as_ref().map(|specs| {
                specs
                    .iter()
                    .map(|spec| substitute_param_spec(spec, inferred))
                    .collect()
            }),
            return_type: Box::new(substitute_type_params(return_type, inferred)),
        },
        Type::Overload(overloads) => Type::Overload(
            overloads
                .iter()
                .map(|overload| substitute_type_params(overload, inferred))
                .collect(),
        ),
        _ => value_type.clone(),
    }
}

fn type_contains_type_param(value_type: &Type) -> bool {
    match value_type {
        Type::Param(_, _) => true,
        Type::Array(item)
        | Type::Option(item)
        | Type::Task(item)
        | Type::CastableSubtype(item)
        | Type::ConcreteSubtype(item)
        | Type::ClassifiableSubset(item)
        | Type::Modifier(item)
        | Type::ModifierStack(item)
        | Type::Signalable(item) => type_contains_type_param(item),
        Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
            type_contains_type_param(key) || type_contains_type_param(value)
        }
        Type::Tuple(items) | Type::Overload(items) => items.iter().any(type_contains_type_param),
        Type::Event(payload)
        | Type::Generator(payload)
        | Type::Awaitable(payload)
        | Type::Subscribable(payload)
        | Type::Listenable(payload) => payload.as_deref().is_some_and(type_contains_type_param),
        Type::Function {
            param_types,
            param_specs,
            return_type,
            ..
        } => {
            param_types
                .as_ref()
                .is_some_and(|params| params.iter().any(type_contains_type_param))
                || param_specs
                    .as_ref()
                    .is_some_and(|specs| specs.iter().any(param_spec_contains_type_param))
                || type_contains_type_param(return_type)
        }
        Type::Int
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
        | Type::Any
        | Type::Comparable
        | Type::Unknown
        | Type::Never
        | Type::Range
        | Type::Enum(_)
        | Type::EnumType(_)
        | Type::Struct(_)
        | Type::StructType(_)
        | Type::Class(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::ParametricType { .. } => false,
    }
}

fn param_spec_contains_type_param(spec: &ParamSpec) -> bool {
    type_contains_type_param(&spec.value_type)
        || spec
            .tuple_items
            .as_ref()
            .is_some_and(|items| items.iter().any(param_spec_contains_type_param))
}

fn substitute_param_spec(spec: &ParamSpec, inferred: &HashMap<String, Type>) -> ParamSpec {
    ParamSpec {
        name: spec.name.clone(),
        value_type: substitute_type_params(&spec.value_type, inferred),
        named: spec.named,
        has_default: spec.has_default,
        tuple_items: spec.tuple_items.as_ref().map(|items| {
            items
                .iter()
                .map(|item| substitute_param_spec(item, inferred))
                .collect()
        }),
    }
}

fn collect_function_type_params(params: &[Param]) -> Result<Vec<TypeParam>, VerseError> {
    let mut collected = Vec::new();
    collect_function_type_params_inner(params, &mut collected)?;
    Ok(collected)
}

fn collect_function_type_params_inner(
    params: &[Param],
    collected: &mut Vec<TypeParam>,
) -> Result<(), VerseError> {
    for param in params {
        for type_param in &param.type_params {
            if collected
                .iter()
                .any(|existing: &TypeParam| existing.name == type_param.name)
            {
                return Err(VerseError::check_at(
                    format!("duplicate type parameter `{}`", type_param.name),
                    type_param.span,
                ));
            }
            collected.push(type_param.clone());
        }
        if let ParamPattern::Tuple(items) = &param.pattern {
            collect_function_type_params_inner(items, collected)?;
        }
    }
    Ok(())
}

fn enum_case_variant<'a>(expr: &'a Expr, enum_name: &str) -> Option<&'a str> {
    let ExprKind::Member { object, name } = &expr.kind else {
        return None;
    };
    let ExprKind::Ident(object_name) = &object.kind else {
        return None;
    };
    (object_name == enum_name).then_some(name.as_str())
}

#[derive(Clone, PartialEq, Eq)]
enum CaseConstant {
    Int(i128),
    Bool(bool),
    String(String),
    Char(char),
}

fn scalar_case_constant(expr: &Expr, subject_type: &Type) -> Option<CaseConstant> {
    match subject_type {
        Type::Int => int_case_constant(expr).map(CaseConstant::Int),
        Type::Bool => match &expr.kind {
            ExprKind::Bool(value) => Some(CaseConstant::Bool(*value)),
            _ => None,
        },
        Type::String => match &expr.kind {
            ExprKind::String(value) => Some(CaseConstant::String(value.clone())),
            _ => None,
        },
        Type::Char | Type::Char8 | Type::Char32 => match &expr.kind {
            ExprKind::Char { value, .. } => Some(CaseConstant::Char(*value)),
            _ => None,
        },
        _ => None,
    }
}

fn int_case_constant(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Number {
            value: NumberLiteral::Int(value),
            kind: NumberKind::Int,
        } => Some(*value),
        ExprKind::Unary {
            op: UnaryOp::Positive,
            expr,
        } => int_case_constant(expr),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => int_case_constant(expr).map(|value| -value),
        _ => None,
    }
}

fn scalar_case_is_exhaustive(subject_type: &Type, covered: &[CaseConstant]) -> bool {
    matches!(subject_type, Type::Bool)
        && covered.contains(&CaseConstant::Bool(true))
        && covered.contains(&CaseConstant::Bool(false))
}

fn case_arms_have_wildcard(arms: &[CaseArm]) -> bool {
    arms.iter()
        .any(|arm| matches!(arm.pattern, CasePattern::Wildcard { .. }))
}

fn is_failable_condition_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } => true,
        ExprKind::Unary {
            op: UnaryOp::Not, ..
        } => true,
        ExprKind::Binary { left, op, right } => {
            is_failure_binary_op(*op)
                || is_failable_condition_expr(left)
                || is_failable_condition_expr(right)
        }
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            failure_statements_have_failable_expr(statements)
        }
        ExprKind::Profile { body, .. } => is_failable_condition_expr(body),
        ExprKind::Spawn { .. } => false,
        ExprKind::Concurrent { .. } => false,
        ExprKind::Case { subject, arms } => {
            is_failable_condition_expr(subject)
                || !case_arms_have_wildcard(arms)
                || arms.iter().any(|arm| {
                    (match &arm.pattern {
                        CasePattern::Wildcard { .. } => false,
                        CasePattern::Expr(pattern) => is_failable_condition_expr(pattern),
                    }) || is_failable_condition_expr(&arm.expr)
                })
        }
        ExprKind::For { body, .. } => is_failable_condition_expr(body),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            is_failable_condition_expr(object)
        }
        ExprKind::Call { callee, .. } => is_failable_condition_expr(callee),
        ExprKind::Var { expr, .. } => is_failable_condition_expr(expr),
        ExprKind::Set { target, expr, .. } => {
            assignment_target_has_failable_expr(target) || is_failable_condition_expr(expr)
        }
        _ => false,
    }
}

fn assignment_target_has_failable_expr(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Index { .. } => true,
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            assignment_target_has_failable_expr(object) || is_failable_condition_expr(object)
        }
        _ => false,
    }
}

fn failure_condition_has_failable_expr(condition: &Expr) -> bool {
    match &condition.kind {
        ExprKind::FailureSequence(clauses) => {
            clauses.iter().any(failure_condition_has_failable_expr)
        }
        ExprKind::FailureBind { expr, .. } => is_failable_condition_expr(expr),
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            failure_statements_have_failable_expr(statements)
        }
        _ => is_failable_condition_expr(condition),
    }
}

fn failure_statements_have_failable_expr(statements: &[Stmt]) -> bool {
    statements.iter().any(failure_statement_has_failable_expr)
}

fn failure_statement_has_failable_expr(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Var { expr, .. }
        | StmtKind::Return(expr)
        | StmtKind::Defer(expr)
        | StmtKind::Expr(expr) => is_failable_condition_expr(expr),
        StmtKind::Set { target, expr, .. } => {
            assignment_target_has_failable_expr(target) || is_failable_condition_expr(expr)
        }
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ParametricType { .. }
        | StmtKind::ExtensionMethod(_) => false,
        StmtKind::Break => false,
    }
}

fn unreachable_statement_message(statement: &Stmt) -> &'static str {
    match &statement.kind {
        StmtKind::Return(_) => "unreachable code after `return`",
        StmtKind::Break => "unreachable code after `break`",
        _ => "unreachable code after never-returning expression",
    }
}

fn is_failure_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Divide
            | BinaryOp::Remainder
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::And
            | BinaryOp::Or
    )
}

fn enum_variant_names(variants: &[crate::ast::EnumVariant]) -> Vec<String> {
    variants
        .iter()
        .map(|variant| variant.name.clone())
        .collect()
}

fn validate_enum_variant_qualifiers(
    enum_name: &str,
    variants: &[crate::ast::EnumVariant],
) -> Result<(), VerseError> {
    for variant in variants {
        if let Some(qualifier) = &variant.qualifier
            && qualifier != enum_name
        {
            return Err(VerseError::check_at(
                format!(
                    "qualified enum value `{}` must use enum name `{enum_name}`",
                    variant.name
                ),
                variant.span,
            ));
        }
    }
    Ok(())
}

fn rendered_param_name(param: &ParamSpec) -> String {
    if param.named {
        format!("?{}", param.name)
    } else {
        param.name.clone()
    }
}

fn tuple_param_specs_have_named_or_default(params: &[ParamSpec]) -> bool {
    params.iter().any(|param| {
        param.named
            || param.has_default
            || param
                .tuple_items
                .as_deref()
                .is_some_and(tuple_param_specs_have_named_or_default)
    })
}

fn rendered_argument_name(name: &str, optional: bool) -> String {
    if optional {
        format!("?{name}")
    } else {
        name.to_string()
    }
}

impl CallArg {
    fn is_named(&self) -> bool {
        matches!(self, Self::Named { .. })
    }
}

fn loop_body_has_non_break_statement(body: &Expr) -> bool {
    match &body.kind {
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
            .iter()
            .any(|statement| !matches!(statement.kind, StmtKind::Break)),
        _ => true,
    }
}

fn is_supported_using_path(path: &str) -> bool {
    path.starts_with("/Verse.org/")
        || path.starts_with("/Fortnite.com/")
        || path.starts_with("/UnrealEngine.com/")
}

fn is_absolute_module_path(path: &str) -> bool {
    path.starts_with('/')
}

fn is_reserved_type_alias_name(name: &str) -> bool {
    matches!(
        name,
        "number"
            | "int"
            | "float"
            | "rational"
            | "bool"
            | "logic"
            | "string"
            | "message"
            | "char"
            | "char8"
            | "char32"
            | "none"
            | "void"
            | "any"
            | "comparable"
            | "array"
            | "function"
            | "tuple"
            | "type"
            | "weak_map"
            | "diagnostic"
            | "entity"
            | "component"
            | "tag"
            | "agent"
            | "session"
            | "player"
            | "team"
            | "event"
            | "task"
            | "generator"
            | "castable_subtype"
            | "concrete_subtype"
            | "classifiable_subset"
            | "modifier"
            | "modifier_stack"
            | "result"
            | "awaitable"
            | "signalable"
            | "listenable"
            | "subscribable"
    )
}

fn defer_body_is_empty(body: &Expr) -> bool {
    matches!(
        &body.kind,
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) if statements.is_empty()
    )
}

fn defer_body_failable_expr(body: &Expr) -> Option<Span> {
    match &body.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } => Some(body.span),
        ExprKind::Unary {
            op: UnaryOp::Not, ..
        } => Some(body.span),
        ExprKind::Unary { expr, .. } => defer_body_failable_expr(expr),
        ExprKind::Binary { left, op, right } => {
            if is_failure_binary_op(*op) {
                Some(body.span)
            } else {
                defer_body_failable_expr(left).or_else(|| defer_body_failable_expr(right))
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            if failure_condition_has_failable_expr(condition) {
                Some(condition.span)
            } else {
                defer_body_failable_expr(then_branch)
                    .or_else(|| else_branch.as_deref().and_then(defer_body_failable_expr))
            }
        }
        ExprKind::FailureBind { expr, .. } => Some(expr.span),
        ExprKind::FailureSequence(items) => items.iter().find_map(|item| {
            if failure_condition_has_failable_expr(item) {
                Some(item.span)
            } else {
                defer_body_failable_expr(item)
            }
        }),
        ExprKind::Set { target, expr, .. } => {
            if assignment_target_has_failable_expr(target) {
                Some(target.span)
            } else {
                defer_body_failable_expr(expr)
            }
        }
        ExprKind::Var { expr, .. } => defer_body_failable_expr(expr),
        ExprKind::Loop { body } => defer_body_failable_expr(body),
        ExprKind::For { clauses, body } => clauses
            .iter()
            .find_map(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => {
                    if failure_condition_has_failable_expr(iterable) {
                        Some(iterable.span)
                    } else {
                        defer_body_failable_expr(iterable)
                    }
                }
            })
            .or_else(|| defer_body_failable_expr(body)),
        ExprKind::Profile { description, body } => {
            defer_body_failable_expr(description).or_else(|| defer_body_failable_expr(body))
        }
        ExprKind::Spawn { body } | ExprKind::Concurrent { body, .. } => {
            defer_body_failable_expr(body)
        }
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            statements.iter().find_map(defer_statement_failable_expr)
        }
        ExprKind::Function { .. } => None,
        ExprKind::Call { callee, args } => defer_body_failable_expr(callee).or_else(|| {
            args.iter().find_map(|arg| match arg {
                CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                    defer_body_failable_expr(expr)
                }
            })
        }),
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            items.iter().find_map(defer_body_failable_expr)
        }
        ExprKind::Map(entries) => entries.iter().find_map(|(key, value)| {
            defer_body_failable_expr(key).or_else(|| defer_body_failable_expr(value))
        }),
        ExprKind::StructDefinition { fields, .. } | ExprKind::ClassDefinition { fields, .. } => {
            fields
                .iter()
                .find_map(|field| field.default.as_ref().and_then(defer_body_failable_expr))
        }
        ExprKind::Archetype {
            callee, entries, ..
        } => defer_body_failable_expr(callee).or_else(|| {
            entries.iter().find_map(|entry| match entry {
                ArchetypeEntry::Field(field) => defer_body_failable_expr(&field.expr),
                ArchetypeEntry::Let(binding) => defer_body_failable_expr(&binding.expr),
                ArchetypeEntry::Block(block) => defer_body_failable_expr(block),
                ArchetypeEntry::ConstructorCall(call) => call
                    .args
                    .iter()
                    .find_map(|arg| defer_body_failable_expr(call_arg_expr(arg))),
            })
        }),
        ExprKind::Case { subject, arms } => defer_body_failable_expr(subject).or_else(|| {
            if !case_arms_have_wildcard(arms) {
                return Some(body.span);
            }
            arms.iter().find_map(|arm| match &arm.pattern {
                CasePattern::Wildcard { .. } => defer_body_failable_expr(&arm.expr),
                CasePattern::Expr(pattern) => defer_body_failable_expr(pattern)
                    .or_else(|| defer_body_failable_expr(&arm.expr)),
            })
        }),
        ExprKind::Option(Some(value)) => defer_body_failable_expr(value),
        ExprKind::InterpolatedString(parts) => parts.iter().find_map(|part| match part {
            InterpolatedStringPart::Text(_) => None,
            InterpolatedStringPart::Expr(expr) => defer_body_failable_expr(expr),
        }),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            defer_body_failable_expr(object)
        }
        ExprKind::Index { .. } => Some(body.span),
        ExprKind::QualifiedName { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Ident(_)
        | ExprKind::EnumDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. }
        | ExprKind::Option(None) => None,
    }
}

fn defer_statement_failable_expr(statement: &Stmt) -> Option<Span> {
    match &statement.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Var { expr, .. }
        | StmtKind::Return(expr)
        | StmtKind::Defer(expr)
        | StmtKind::Expr(expr) => defer_body_failable_expr(expr),
        StmtKind::Set { target, expr, .. } => {
            if assignment_target_has_failable_expr(target) {
                Some(target.span)
            } else {
                defer_body_failable_expr(expr)
            }
        }
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ParametricType { .. }
        | StmtKind::ExtensionMethod(_)
        | StmtKind::Break => None,
    }
}

fn defer_body_escape(body: &Expr, loop_depth: usize) -> Option<(&'static str, Span)> {
    match &body.kind {
        ExprKind::Unary { expr, .. } | ExprKind::UnwrapOption(expr) => {
            defer_body_escape(expr, loop_depth)
        }
        ExprKind::Binary { left, right, .. } => {
            defer_body_escape(left, loop_depth).or_else(|| defer_body_escape(right, loop_depth))
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => defer_body_escape(condition, loop_depth)
            .or_else(|| defer_body_escape(then_branch, loop_depth))
            .or_else(|| {
                else_branch
                    .as_deref()
                    .and_then(|branch| defer_body_escape(branch, loop_depth))
            }),
        ExprKind::FailureBind { expr, .. } => defer_body_escape(expr, loop_depth),
        ExprKind::Set { target, expr, .. } => {
            defer_body_escape(target, loop_depth).or_else(|| defer_body_escape(expr, loop_depth))
        }
        ExprKind::Var { expr, .. } => defer_body_escape(expr, loop_depth),
        ExprKind::FailureSequence(items) | ExprKind::Array(items) | ExprKind::Tuple(items) => items
            .iter()
            .find_map(|item| defer_body_escape(item, loop_depth)),
        ExprKind::Loop { body } => defer_body_escape(body, loop_depth + 1),
        ExprKind::For { clauses, body } => clauses
            .iter()
            .find_map(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => defer_body_escape(iterable, loop_depth),
            })
            .or_else(|| defer_body_escape(body, loop_depth)),
        ExprKind::Profile { description, body } => defer_body_escape(description, loop_depth)
            .or_else(|| defer_body_escape(body, loop_depth)),
        ExprKind::Spawn { body } => defer_body_escape(body, loop_depth),
        ExprKind::Concurrent { body, .. } => defer_body_escape(body, loop_depth),
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
            .iter()
            .find_map(|statement| defer_statement_escape(statement, loop_depth)),
        ExprKind::Function { .. } => None,
        ExprKind::Call { callee, args } => defer_body_escape(callee, loop_depth).or_else(|| {
            args.iter().find_map(|arg| match arg {
                CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                    defer_body_escape(expr, loop_depth)
                }
            })
        }),
        ExprKind::BracketCall { callee, args } => {
            defer_body_escape(callee, loop_depth).or_else(|| {
                args.iter()
                    .find_map(|arg| defer_body_escape(arg, loop_depth))
            })
        }
        ExprKind::Map(entries) => entries.iter().find_map(|(key, value)| {
            defer_body_escape(key, loop_depth).or_else(|| defer_body_escape(value, loop_depth))
        }),
        ExprKind::StructDefinition { fields, .. } | ExprKind::ClassDefinition { fields, .. } => {
            fields.iter().find_map(|field| {
                field
                    .default
                    .as_ref()
                    .and_then(|default| defer_body_escape(default, loop_depth))
            })
        }
        ExprKind::Archetype {
            callee, entries, ..
        } => defer_body_escape(callee, loop_depth).or_else(|| {
            entries.iter().find_map(|entry| match entry {
                ArchetypeEntry::Field(field) => defer_body_escape(&field.expr, loop_depth),
                ArchetypeEntry::Let(binding) => defer_body_escape(&binding.expr, loop_depth),
                ArchetypeEntry::Block(block) => defer_body_escape(block, loop_depth),
                ArchetypeEntry::ConstructorCall(call) => call
                    .args
                    .iter()
                    .find_map(|arg| defer_body_escape(call_arg_expr(arg), loop_depth)),
            })
        }),
        ExprKind::Case { subject, arms } => defer_body_escape(subject, loop_depth).or_else(|| {
            arms.iter().find_map(|arm| match &arm.pattern {
                CasePattern::Wildcard { .. } => defer_body_escape(&arm.expr, loop_depth),
                CasePattern::Expr(pattern) => defer_body_escape(pattern, loop_depth)
                    .or_else(|| defer_body_escape(&arm.expr, loop_depth)),
            })
        }),
        ExprKind::Option(Some(value)) => defer_body_escape(value, loop_depth),
        ExprKind::Option(None) => None,
        ExprKind::InterpolatedString(parts) => parts.iter().find_map(|part| match part {
            InterpolatedStringPart::Text(_) => None,
            InterpolatedStringPart::Expr(expr) => defer_body_escape(expr, loop_depth),
        }),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            defer_body_escape(object, loop_depth)
        }
        ExprKind::Index { collection, index } => defer_body_escape(collection, loop_depth)
            .or_else(|| defer_body_escape(index, loop_depth)),
        ExprKind::QualifiedName { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Ident(_)
        | ExprKind::EnumDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. } => None,
    }
}

fn defer_statement_escape(statement: &Stmt, loop_depth: usize) -> Option<(&'static str, Span)> {
    match &statement.kind {
        StmtKind::Return(_) => Some(("`return` cannot be used inside `defer`", statement.span)),
        StmtKind::Break if loop_depth == 0 => {
            Some(("`break` cannot exit a `defer` body", statement.span))
        }
        StmtKind::Break => None,
        StmtKind::Using { .. } => None,
        StmtKind::TypeAlias { .. } => None,
        StmtKind::ParametricType { .. } => None,
        StmtKind::ExtensionMethod(_) => None,
        StmtKind::Let { expr, .. } | StmtKind::Var { expr, .. } | StmtKind::Expr(expr) => {
            defer_body_escape(expr, loop_depth)
        }
        StmtKind::Set { target, expr, .. } => {
            defer_body_escape(target, loop_depth).or_else(|| defer_body_escape(expr, loop_depth))
        }
        StmtKind::Defer(body) => defer_body_escape(body, loop_depth),
    }
}

fn aggregate_module_name(aggregate_name: &str) -> Option<&str> {
    let uninstantiated = aggregate_name
        .split_once('(')
        .map_or(aggregate_name, |(name, _)| name);
    uninstantiated.rsplit_once('.').map(|(module, _)| module)
}

fn aggregate_unqualified_name(aggregate_name: &str) -> &str {
    let uninstantiated = aggregate_name
        .split_once('(')
        .map_or(aggregate_name, |(name, _)| name);
    uninstantiated
        .rsplit_once('.')
        .map_or(uninstantiated, |(_, name)| name)
}

fn has_effect(effects: &[String], name: &str) -> bool {
    effects.iter().any(|effect| effect == name)
}

fn is_access_specifier(specifier: &str) -> bool {
    matches!(specifier, "public" | "internal" | "protected" | "private")
}

fn module_member_specifiers<'a>(binding_specifiers: &'a [String], expr: &'a Expr) -> &'a [String] {
    if binding_specifiers
        .iter()
        .any(|specifier| is_access_specifier(specifier))
    {
        return binding_specifiers;
    }

    match &expr.kind {
        ExprKind::Function { effects, .. }
            if effects
                .iter()
                .any(|specifier| is_access_specifier(specifier)) =>
        {
            effects
        }
        ExprKind::ClassDefinition { specifiers, .. }
            if specifiers
                .iter()
                .any(|specifier| is_access_specifier(specifier)) =>
        {
            specifiers
        }
        _ => binding_specifiers,
    }
}

fn access_level_from_specifiers(
    specifiers: &[String],
    _context: &str,
    _span: Span,
) -> Result<AccessLevel, VerseError> {
    Ok(
        match specifiers
            .iter()
            .rev()
            .find(|specifier| is_access_specifier(specifier))
            .map(|specifier| specifier.as_str())
        {
            Some("public") => AccessLevel::Public,
            Some("protected") => AccessLevel::Protected,
            Some("private") => AccessLevel::Private,
            Some("internal") | None => AccessLevel::Internal,
            Some(_) => unreachable!("filtered access specifiers"),
        },
    )
}

fn ensure_callable_in_failure_context(effects: &[String], span: Span) -> Result<(), VerseError> {
    if has_no_rollback_effect(effects) {
        return Err(VerseError::check_at(
            "function with `<no_rollback>` effect cannot be called in a failure context",
            span,
        ));
    }

    Ok(())
}

fn has_no_rollback_effect(effects: &[String]) -> bool {
    if has_effect(effects, "no_rollback") {
        return true;
    }

    ![
        "transacts",
        "varies",
        "computes",
        "converges",
        "reads",
        "writes",
        "allocates",
    ]
    .into_iter()
    .any(|effect| has_effect(effects, effect))
}

fn has_explicit_call_effect_specifier(effects: &[String]) -> bool {
    [
        "transacts",
        "varies",
        "computes",
        "converges",
        "reads",
        "writes",
        "allocates",
    ]
    .into_iter()
    .any(|effect| has_effect(effects, effect))
}

fn call_allowed_capabilities(effects: &[String]) -> Vec<&'static str> {
    let mut capabilities = Vec::new();

    if has_effect(effects, "transacts") {
        push_capability(&mut capabilities, "transacts");
        push_capability(&mut capabilities, "varies");
        push_capability(&mut capabilities, "reads");
        push_capability(&mut capabilities, "writes");
        push_capability(&mut capabilities, "allocates");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "varies") {
        push_capability(&mut capabilities, "varies");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "computes") {
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "converges") {
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "reads") {
        push_capability(&mut capabilities, "reads");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "writes") {
        push_capability(&mut capabilities, "writes");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "allocates") {
        push_capability(&mut capabilities, "allocates");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }

    capabilities
}

fn call_required_capabilities(effects: &[String]) -> Vec<&'static str> {
    let mut capabilities = Vec::new();

    if has_effect(effects, "transacts") {
        push_capability(&mut capabilities, "transacts");
    } else if has_effect(effects, "varies") {
        push_capability(&mut capabilities, "varies");
    } else if has_effect(effects, "computes") {
        push_capability(&mut capabilities, "computes");
    } else if has_effect(effects, "converges") {
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "reads") {
        push_capability(&mut capabilities, "reads");
    }
    if has_effect(effects, "writes") {
        push_capability(&mut capabilities, "writes");
    }
    if has_effect(effects, "allocates") {
        push_capability(&mut capabilities, "allocates");
    }

    capabilities
}

fn effect_call_error(caller_effects: &[String], required: &str, span: Span) -> VerseError {
    VerseError::check_at(
        format!(
            "function with {} effect cannot call function requiring <{}> effect",
            render_effect_set(caller_effects),
            required
        ),
        span,
    )
}

fn render_effect_set(effects: &[String]) -> String {
    let rendered = effects
        .iter()
        .filter(|effect| is_function_effect_name(effect))
        .map(|effect| format!("<{effect}>"))
        .collect::<Vec<_>>();
    if rendered.is_empty() {
        "<no_rollback>".to_string()
    } else {
        rendered.join("")
    }
}

fn function_effects_are_assignable(expected: &[String], actual: &[String]) -> bool {
    if has_effect(expected, "decides") != has_effect(actual, "decides") {
        return false;
    }

    let expected = effect_capabilities(expected);
    let actual = effect_capabilities(actual);
    actual
        .iter()
        .all(|capability| expected.iter().any(|expected| expected == capability))
}

fn effect_capabilities(effects: &[String]) -> Vec<&'static str> {
    let mut capabilities = Vec::new();

    if has_no_rollback_effect(effects) {
        push_capability(&mut capabilities, "no_rollback");
    }
    if has_effect(effects, "transacts") {
        push_capability(&mut capabilities, "transacts");
        push_capability(&mut capabilities, "varies");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
        push_capability(&mut capabilities, "allocates");
        push_capability(&mut capabilities, "reads");
        push_capability(&mut capabilities, "writes");
    }
    if has_effect(effects, "varies") {
        push_capability(&mut capabilities, "varies");
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "computes") {
        push_capability(&mut capabilities, "computes");
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "converges") {
        push_capability(&mut capabilities, "converges");
    }
    if has_effect(effects, "reads") {
        push_capability(&mut capabilities, "reads");
    }
    if has_effect(effects, "writes") {
        push_capability(&mut capabilities, "writes");
    }
    if has_effect(effects, "allocates") {
        push_capability(&mut capabilities, "allocates");
    }
    if has_effect(effects, "suspends") {
        push_capability(&mut capabilities, "suspends");
    }

    capabilities
}

fn push_capability(capabilities: &mut Vec<&'static str>, capability: &'static str) {
    if !capabilities.iter().any(|existing| existing == &capability) {
        capabilities.push(capability);
    }
}

fn validate_function_effect_combination(effects: &[String], span: Span) -> Result<(), VerseError> {
    let mut seen = Vec::new();
    for effect in effects
        .iter()
        .filter(|effect| is_function_effect_name(effect))
    {
        if seen.iter().any(|seen_effect| seen_effect == effect) {
            return Err(VerseError::check_at(
                format!("duplicate function effect `<{effect}>`"),
                span,
            ));
        }
        seen.push(effect.as_str());
    }

    if has_effect(effects, "decides") && !has_effect(effects, "transacts") {
        return Err(VerseError::check_at(
            "function with `<decides>` must also have `<transacts>`",
            span,
        ));
    }

    if has_effect(effects, "constructor") && has_effect(effects, "suspends") {
        return Err(VerseError::check_at(
            "constructor functions cannot use `<suspends>`",
            span,
        ));
    }

    let exclusive = ["transacts", "varies", "computes", "converges"]
        .into_iter()
        .filter(|effect| has_effect(effects, effect))
        .collect::<Vec<_>>();
    if exclusive.len() > 1 {
        return Err(VerseError::check_at(
            format!(
                "function exclusive effects cannot be combined: {}",
                exclusive
                    .into_iter()
                    .map(|effect| format!("<{effect}>"))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            span,
        ));
    }

    Ok(())
}

fn is_function_effect_name(name: &str) -> bool {
    matches!(
        name,
        "converges"
            | "computes"
            | "varies"
            | "transacts"
            | "suspends"
            | "decides"
            | "reads"
            | "writes"
            | "allocates"
    )
}

fn is_fits_in_player_map_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "FitsInPlayerMap")
}

fn is_shuffle_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "Shuffle")
}

fn is_concatenate_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "Concatenate")
}

fn is_make_classifiable_subset_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "MakeClassifiableSubset")
}

fn make_result_callee_name(callee: &Expr) -> Option<&str> {
    let ExprKind::Ident(name) = &callee.kind else {
        return None;
    };
    matches!(name.as_str(), "MakeSuccess" | "MakeError").then_some(name.as_str())
}

fn is_make_result_callee(callee: &Expr) -> bool {
    make_result_callee_name(callee).is_some()
}

fn is_shuffle_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            effects,
            param_types: Some(param_types),
            return_type,
            ..
        } if has_effect(effects, "transacts")
            && matches!(param_types.as_slice(), [Type::Array(_)])
            && matches!(return_type.as_ref(), Type::Array(_))
    )
}

fn is_concatenate_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            param_types: Some(param_types),
            return_type,
            ..
        } if matches!(param_types.as_slice(), [Type::Array(item)] if matches!(item.as_ref(), Type::Array(_)))
            && matches!(return_type.as_ref(), Type::Array(_))
    )
}

fn infer_concatenate_item_type(
    args: &[CallArg],
    arg_types: &[Type],
    span: Span,
) -> Result<Type, VerseError> {
    if args.len() == 1
        && let Some(item_type) = concatenate_arrays_argument_item_type(&arg_types[0], span)?
    {
        return Ok(item_type);
    }

    let mut item_type = Type::Unknown;
    for arg_type in arg_types {
        let next = concatenate_packed_argument_item_type(arg_type, span)?;
        item_type = unify_types(&item_type, &next, span)?;
    }
    Ok(item_type)
}

fn concatenate_arrays_argument_item_type(
    value_type: &Type,
    span: Span,
) -> Result<Option<Type>, VerseError> {
    match value_type {
        Type::Array(item) => match item.as_ref() {
            Type::Array(nested) => Ok(Some(nested.as_ref().clone())),
            Type::Unknown | Type::Any => Ok(Some(Type::Unknown)),
            _ => Ok(None),
        },
        Type::Tuple(items) => {
            let mut item_type = Type::Unknown;
            for item in items {
                let next = match item {
                    Type::Array(nested) => nested.as_ref().clone(),
                    Type::Unknown | Type::Any => Type::Unknown,
                    _ => return Ok(None),
                };
                item_type = unify_types(&item_type, &next, span)?;
            }
            Ok(Some(item_type))
        }
        Type::Unknown | Type::Any => Ok(Some(Type::Unknown)),
        _ => Ok(None),
    }
}

fn concatenate_packed_argument_item_type(
    value_type: &Type,
    span: Span,
) -> Result<Type, VerseError> {
    match value_type {
        Type::Array(item) => Ok(item.as_ref().clone()),
        Type::Tuple(items) => {
            let mut item_type = Type::Unknown;
            for item in items {
                item_type = unify_types(&item_type, item, span)?;
            }
            Ok(item_type)
        }
        Type::Unknown | Type::Any => Ok(Type::Unknown),
        other => Ok(other.clone()),
    }
}

fn is_make_classifiable_subset_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            param_types: Some(param_types),
            return_type,
            ..
        } if matches!(param_types.as_slice(), [Type::Array(_)])
            && matches!(return_type.as_ref(), Type::ClassifiableSubset(_))
    )
}

fn is_length_member_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Member { name, .. } if name == "Length")
}

fn class_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn field_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn render_effects(effects: &[String]) -> String {
    effects
        .iter()
        .map(|effect| format!("<{effect}>"))
        .collect::<String>()
}

fn render_type_list(types: &[Type]) -> String {
    types
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_parametric_instance_type_name(name: &str, args: &[Type]) -> String {
    format!("{name}({})", render_type_list(args))
}

fn replace_type_param_atoms(name: &str, inferred: &HashMap<String, Type>) -> String {
    let mut result = String::new();
    let mut token = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            token.push(ch);
            continue;
        }
        if !token.is_empty() {
            if let Some(value_type) = inferred.get(&token) {
                result.push_str(&value_type.to_string());
            } else {
                result.push_str(&token);
            }
            token.clear();
        }
        result.push(ch);
    }
    if !token.is_empty() {
        if let Some(value_type) = inferred.get(&token) {
            result.push_str(&value_type.to_string());
        } else {
            result.push_str(&token);
        }
    }
    result
}

fn parametric_type_kind(expr: &Expr) -> Option<ParametricTypeKind> {
    match expr.kind {
        ExprKind::StructDefinition { .. } => Some(ParametricTypeKind::Struct),
        ExprKind::ClassDefinition { .. } => Some(ParametricTypeKind::Class),
        ExprKind::InterfaceDefinition { .. } => Some(ParametricTypeKind::Interface),
        _ => None,
    }
}

fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        if !deduped.iter().any(|existing| existing == &item) {
            deduped.push(item);
        }
    }
    deduped
}

fn char_array_type() -> Type {
    Type::Array(Box::new(Type::Char))
}

fn native_function_type(
    effects: &[&str],
    params: Vec<(&'static str, Type)>,
    return_type: Type,
) -> Type {
    Type::Function {
        arity: Some(params.len()),
        arity_range: None,
        effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
        param_types: Some(
            params
                .iter()
                .map(|(_, value_type)| value_type.clone())
                .collect(),
        ),
        param_specs: Some(
            params
                .into_iter()
                .map(|(name, value_type)| ParamSpec {
                    name: name.to_string(),
                    value_type,
                    named: false,
                    has_default: false,
                    tuple_items: None,
                })
                .collect(),
        ),
        return_type: Box::new(return_type),
    }
}

fn result_accessor_type(return_type: &Type) -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec!["computes".to_string(), "decides".to_string()],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(return_type.clone()),
    }
}

fn await_type(payload: Option<&Type>) -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec![
            "transacts".to_string(),
            "suspends".to_string(),
            "no_rollback".to_string(),
        ],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(payload.cloned().unwrap_or(Type::None)),
    }
}

fn signal_type(payload: Option<&Type>) -> Type {
    let param_types = payload.iter().copied().cloned().collect::<Vec<_>>();
    Type::Function {
        arity: Some(param_types.len()),
        arity_range: None,
        effects: vec!["transacts".to_string(), "no_rollback".to_string()],
        param_types: Some(param_types),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_contains_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::CastableSubtype(Box::new(item_type.clone()))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_contains_many_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::Array(Box::new(Type::CastableSubtype(
            Box::new(item_type.clone()),
        )))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_element_type(item_type: &Type) -> Type {
    match item_type {
        Type::CastableSubtype(item) => item.as_ref().clone(),
        other => other.clone(),
    }
}

fn modifier_evaluate_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: Vec::new(),
        param_types: Some(vec![item_type.clone()]),
        param_specs: None,
        return_type: Box::new(item_type.clone()),
    }
}

fn modifier_method_info(item_type: &Type, span: Span) -> ClassMethodInfo {
    ClassMethodInfo {
        qualifier: None,
        name: "Evaluate".to_string(),
        value_type: modifier_evaluate_type(item_type),
        final_member: false,
        abstract_member: true,
        access: AccessLevel::Public,
        owner: Some(format!("modifier({item_type})")),
        span,
    }
}

fn modifier_stack_add_modifier_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(2),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![
            Type::Modifier(Box::new(item_type.clone())),
            Type::Rational,
        ]),
        param_specs: None,
        return_type: Box::new(Type::Interface("cancelable".to_string())),
    }
}

fn subscribe_type(payload: Option<&Type>) -> Type {
    let callback_param_types = payload.iter().copied().cloned().collect::<Vec<_>>();
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![Type::Function {
            arity: Some(callback_param_types.len()),
            arity_range: None,
            effects: Vec::new(),
            param_types: Some(callback_param_types),
            param_specs: None,
            return_type: Box::new(Type::None),
        }]),
        param_specs: None,
        return_type: Box::new(Type::Interface("cancelable".to_string())),
    }
}

fn is_official_parametric_type_name(name: &str) -> bool {
    matches!(
        name,
        "event"
            | "task"
            | "generator"
            | "castable_subtype"
            | "concrete_subtype"
            | "classifiable_subset"
            | "modifier"
            | "modifier_stack"
            | "result"
            | "awaitable"
            | "signalable"
            | "listenable"
            | "subscribable"
    )
}

fn official_parametric_type(name: &str, args: &[Type], span: Span) -> Result<Type, VerseError> {
    match name {
        "event" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Event(args.first().cloned().map(Box::new)))
        }
        "result" => {
            ensure_parametric_type_arity(name, args, &[2], span)?;
            Ok(Type::Result(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
            ))
        }
        "task" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Task(Box::new(args[0].clone())))
        }
        "generator" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Generator(args.first().cloned().map(Box::new)))
        }
        "castable_subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::CastableSubtype(Box::new(args[0].clone())))
        }
        "concrete_subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ConcreteSubtype(Box::new(args[0].clone())))
        }
        "classifiable_subset" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ClassifiableSubset(Box::new(args[0].clone())))
        }
        "modifier" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Modifier(Box::new(args[0].clone())))
        }
        "modifier_stack" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ModifierStack(Box::new(args[0].clone())))
        }
        "signalable" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Signalable(Box::new(args[0].clone())))
        }
        "awaitable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Awaitable(args.first().cloned().map(Box::new)))
        }
        "listenable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Listenable(args.first().cloned().map(Box::new)))
        }
        "subscribable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Subscribable(args.first().cloned().map(Box::new)))
        }
        _ => Err(VerseError::check_at(
            format!("unknown parametric type `{name}`"),
            span,
        )),
    }
}

fn ensure_parametric_type_arity(
    name: &str,
    args: &[Type],
    expected: &[usize],
    span: Span,
) -> Result<(), VerseError> {
    if expected.contains(&args.len()) {
        return Ok(());
    }

    let expected = expected
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    Err(VerseError::check_at(
        format!(
            "parametric type `{name}` expected {expected} type arguments, got {}",
            args.len()
        ),
        span,
    ))
}

fn color_type() -> Type {
    Type::Struct("color".to_string())
}

fn color_alpha_type() -> Type {
    Type::Struct("color_alpha".to_string())
}

fn is_builtin_class_type_name(name: &str) -> bool {
    matches!(
        name,
        "diagnostic" | "entity" | "component" | "tag" | "session" | "player" | "agent" | "team"
    )
}

fn is_builtin_class_base_name(name: &str) -> bool {
    matches!(name, "component" | "tag")
}

fn is_builtin_comparable_class_name(name: &str) -> bool {
    matches!(name, "session" | "player" | "team")
}

fn ensure_int_index_type(value_type: &Type, context: &str, span: Span) -> Result<(), VerseError> {
    match value_type {
        Type::Int | Type::Unknown | Type::Any => Ok(()),
        other => Err(VerseError::check_at(
            format!("{context} expected `int`, got `{other}`"),
            span,
        )),
    }
}

fn tuple_index_literal(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Number {
            value: NumberLiteral::Int(index),
            ..
        } => Some(*index),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => match &expr.kind {
            ExprKind::Number {
                value: NumberLiteral::Int(index),
                ..
            } => index.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

const BUILTIN_INTERFACE_NAMES: &[&str] = &[
    "cancelable",
    "disposable",
    "enableable",
    "invalidatable",
    "showable",
];

fn builtin_session_environment_info() -> EnumInfo {
    EnumInfo {
        variants: vec![
            "Edit".to_string(),
            "Private".to_string(),
            "Live".to_string(),
        ],
        open: false,
        persistable: false,
    }
}

fn print_function_type(message_type: Type) -> Type {
    Type::Function {
        arity: None,
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: None,
        param_specs: Some(vec![
            ParamSpec {
                name: "Message".to_string(),
                value_type: message_type,
                named: false,
                has_default: false,
                tuple_items: None,
            },
            ParamSpec {
                name: "Duration".to_string(),
                value_type: Type::Float,
                named: true,
                has_default: true,
                tuple_items: None,
            },
            ParamSpec {
                name: "Color".to_string(),
                value_type: color_type(),
                named: true,
                has_default: true,
                tuple_items: None,
            },
        ]),
        return_type: Box::new(Type::None),
    }
}

fn builtin_interface_infos() -> HashMap<String, InterfaceInfo> {
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "cancelable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![builtin_interface_method(
                "cancelable",
                "Cancel",
                &["transacts"],
            )],
        },
    );
    interfaces.insert(
        "disposable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![builtin_interface_method(
                "disposable",
                "Dispose",
                &["transacts"],
            )],
        },
    );
    interfaces.insert(
        "enableable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![
                builtin_interface_method("enableable", "Enable", &["transacts"]),
                builtin_interface_method("enableable", "Disable", &["transacts"]),
                builtin_interface_method("enableable", "IsEnabled", &["transacts", "decides"]),
            ],
        },
    );
    interfaces.insert(
        "invalidatable".to_string(),
        InterfaceInfo {
            parents: vec!["disposable".to_string()],
            fields: Vec::new(),
            methods: vec![
                builtin_interface_method("disposable", "Dispose", &["transacts"]),
                builtin_interface_method("invalidatable", "IsValid", &["transacts", "decides"]),
            ],
        },
    );
    interfaces.insert(
        "showable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: vec![builtin_interface_field(
                "showable",
                "Show",
                Type::Option(Box::new(Type::Bool)),
                true,
            )],
            methods: Vec::new(),
        },
    );
    interfaces
}

fn builtin_interface_method(
    interface_name: &str,
    method_name: &str,
    effects: &[&str],
) -> ClassMethodInfo {
    ClassMethodInfo {
        qualifier: None,
        name: method_name.to_string(),
        value_type: Type::Function {
            arity: Some(0),
            arity_range: None,
            effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
            param_types: Some(Vec::new()),
            param_specs: Some(Vec::new()),
            return_type: Box::new(Type::None),
        },
        final_member: false,
        abstract_member: true,
        access: AccessLevel::Public,
        owner: Some(interface_name.to_string()),
        span: Span::new(0, 0, 1, 1),
    }
}

fn builtin_interface_field(
    interface_name: &str,
    field_name: &str,
    value_type: Type,
    mutable: bool,
) -> StructFieldInfo {
    StructFieldInfo {
        name: field_name.to_string(),
        value_type,
        has_default: false,
        mutable,
        final_member: false,
        access: AccessLevel::Public,
        mutation_access: AccessLevel::Public,
        owner: Some(interface_name.to_string()),
        span: Span::new(0, 0, 1, 1),
    }
}

fn builtin_color_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        persistable: true,
        computes: false,
        fields: ["R", "G", "B"]
            .into_iter()
            .map(|name| StructFieldInfo {
                name: name.to_string(),
                value_type: Type::Float,
                has_default: false,
                mutable: false,
                final_member: false,
                access: AccessLevel::Public,
                mutation_access: AccessLevel::Public,
                owner: Some("color".to_string()),
                span: Span::new(0, 0, 1, 1),
            })
            .collect(),
        methods: Vec::new(),
    }
}

fn builtin_color_alpha_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        persistable: false,
        computes: false,
        fields: [
            ("Color".to_string(), color_type()),
            ("A".to_string(), Type::Float),
        ]
        .into_iter()
        .map(|(name, value_type)| StructFieldInfo {
            name,
            value_type,
            has_default: false,
            mutable: false,
            final_member: false,
            access: AccessLevel::Public,
            mutation_access: AccessLevel::Public,
            owner: Some("color_alpha".to_string()),
            span: Span::new(0, 0, 1, 1),
        })
        .collect(),
        methods: Vec::new(),
    }
}

fn builtin_locale_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        persistable: false,
        computes: false,
        fields: Vec::new(),
        methods: Vec::new(),
    }
}

fn is_char_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Char | Type::Char8 | Type::Char32)
}

fn is_string_char_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Char)
}

fn is_empty_option_literal(expected: &Type, expr: &Expr) -> bool {
    matches!(expected, Type::Option(_)) && matches!(&expr.kind, ExprKind::Bool(false))
}

fn is_empty_option_candidate(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Bool(false))
}

fn finalize_collection_item_type(
    current: &mut Type,
    pending_empty_options: &mut Vec<&Expr>,
) -> Result<(), VerseError> {
    if matches!(current, Type::Unknown) {
        if !pending_empty_options.is_empty() {
            *current = Type::Bool;
        }
        pending_empty_options.clear();
        return Ok(());
    }

    for expr in pending_empty_options.drain(..) {
        if !is_empty_option_literal(current, expr) {
            *current = unify_types(current, &Type::Bool, expr.span)?;
        }
    }
    Ok(())
}

fn validate_weak_map_type(
    key_type: &Type,
    value_type: &Type,
    span: Span,
    enum_types: &HashMap<String, EnumInfo>,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<(), VerseError> {
    match key_type {
        Type::Class(name) if name == "session" => Ok(()),
        Type::Class(name) if name == "player" => {
            if is_persistable_type_name(value_type, enum_types, struct_types) {
                Ok(())
            } else {
                Err(VerseError::check_at(
                    format!("weak_map(player, ...) value type `{value_type}` must be persistable"),
                    span,
                ))
            }
        }
        other => Err(VerseError::check_at(
            format!("weak_map key type must be `session` or `player`, got `{other}`"),
            span,
        )),
    }
}

fn is_persistable_type_name(
    value_type: &Type,
    enum_types: &HashMap<String, EnumInfo>,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match value_type {
        Type::Int
        | Type::Float
        | Type::Rational
        | Type::Number
        | Type::Bool
        | Type::String
        | Type::Message
        | Type::Char
        | Type::Char8
        | Type::Char32
        | Type::None => true,
        Type::Array(item) | Type::Option(item) => {
            is_persistable_type_name(item, enum_types, struct_types)
        }
        Type::Map(key, value) => {
            is_persistable_type_name(key, enum_types, struct_types)
                && is_persistable_type_name(value, enum_types, struct_types)
        }
        Type::Tuple(items) => items
            .iter()
            .all(|item| is_persistable_type_name(item, enum_types, struct_types)),
        Type::Enum(name) => enum_types.get(name).is_some_and(|info| info.persistable),
        Type::Struct(name) | Type::Class(name) => {
            struct_types.get(name).is_some_and(|info| info.persistable)
        }
        Type::Any
        | Type::Comparable
        | Type::Unknown
        | Type::Never
        | Type::Range
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::Param(_, _)
        | Type::ParametricType { .. }
        | Type::WeakMap(_, _)
        | Type::Result(_, _)
        | Type::Event(_)
        | Type::Task(_)
        | Type::Generator(_)
        | Type::CastableSubtype(_)
        | Type::ConcreteSubtype(_)
        | Type::ClassifiableSubset(_)
        | Type::Modifier(_)
        | Type::ModifierStack(_)
        | Type::Awaitable(_)
        | Type::Signalable(_)
        | Type::Subscribable(_)
        | Type::Listenable(_)
        | Type::Function { .. }
        | Type::Overload(_) => false,
    }
}

fn ensure_number_like(value_type: &Type, context: &str, span: Span) -> Result<(), VerseError> {
    match value_type {
        Type::Int | Type::Float | Type::Rational | Type::Number | Type::Unknown | Type::Any => {
            Ok(())
        }
        other => Err(VerseError::check_at(
            format!("{context} expected `number`, got `{other}`"),
            span,
        )),
    }
}

fn ensure_bool_like(value_type: &Type, context: &str, span: Span) -> Result<(), VerseError> {
    match value_type {
        Type::Bool | Type::Unknown | Type::Any => Ok(()),
        other => Err(VerseError::check_at(
            format!("{context} expected `bool`, got `{other}`"),
            span,
        )),
    }
}

fn type_param_constraint_declares_comparable(constraint: &TypeParamConstraint) -> bool {
    matches!(
        constraint,
        TypeParamConstraint::Subtype(TypeName::Comparable)
    )
}

fn ensure_comparable_key(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
) -> Result<(), VerseError> {
    ensure_comparable_key_inner(value_type, struct_types, span, &mut Vec::new())
}

fn ensure_equality_comparable(
    value_type: &Type,
    struct_types: &HashMap<String, StructInfo>,
    span: Span,
) -> Result<(), VerseError> {
    ensure_equality_comparable_inner(value_type, struct_types, span, &mut Vec::new())
}

fn ensure_equality_comparable_inner(
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

fn ensure_aggregate_fields_equality_comparable(
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

fn ensure_comparable_key_inner(
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

fn ensure_exact_arg_count(
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

fn ensure_arg_count_range(
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

fn check_add(
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

fn check_subtract(
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

fn check_multiply(
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

fn check_divide(
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

fn diagnostic_type() -> Type {
    Type::Class("diagnostic".to_string())
}

fn is_diagnostic_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Class(name) if name == "diagnostic")
}

fn is_color_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Struct(name) if name == "color")
}

fn is_numeric_type(value_type: &Type) -> bool {
    matches!(
        value_type,
        Type::Int | Type::Float | Type::Rational | Type::Number
    )
}

fn unify_numeric_types(left: &Type, right: &Type) -> Type {
    match (left, right) {
        (Type::Float, _) | (_, Type::Float) => Type::Float,
        (Type::Rational, _) | (_, Type::Rational) => Type::Rational,
        (Type::Int, Type::Int) => Type::Int,
        _ => Type::Number,
    }
}

fn divide_numeric_type(left: &Type, right: &Type) -> Type {
    match (left, right) {
        (Type::Float, _) | (_, Type::Float) => Type::Float,
        (Type::Int | Type::Rational, Type::Int | Type::Rational) => Type::Rational,
        _ => unify_numeric_types(left, right),
    }
}

fn unify_types(left: &Type, right: &Type, span: Span) -> Result<Type, VerseError> {
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
