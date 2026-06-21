use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use rand::{Rng, rngs::OsRng};

use crate::ast::{
    ArchetypeConstructorCall, ArchetypeEntry, AssignOp, BinaryOp, CallArg, CaseArm, CasePattern,
    ClassBlock, ClassMethod, ConcurrentOp, Expr, ExprKind, ExtensionMethod, ForBinding, ForClause,
    InterpolatedStringPart, Param, ParamPattern, Program, Stmt, StmtKind, StructField,
    TypeAnnotation, TypeName, TypeParam, TypeParamConstraint, UnaryOp,
};
use crate::colors::NAMED_COLORS;
use crate::desugar::desugar_program;
use crate::error::VerseError;
use crate::ir::TypedProgram;
use crate::parser::parse_source;
use crate::token::{CharacterKind, NumberKind, NumberLiteral, Span};

mod numeric;
pub use numeric::RationalValue;
use numeric::{
    RuntimeNumber, numeric_values_equal, rational_or_int, runtime_number, runtime_number_to_f64,
    runtime_number_to_rational,
};
mod scope;
pub use scope::Env;
use scope::EnvTransaction;
mod task;
use task::{RuntimeScheduler, StructuredTaskWait};
pub use task::{RuntimeSuspension, RuntimeTask};

type NativeFn = fn(Vec<Value>, Span) -> Result<NativeResult, VerseError>;
type ValueContinuation = dyn Fn(&Interpreter, Value) -> Result<Flow, VerseError>;
type ValuesContinuation = dyn Fn(&Interpreter, Vec<Value>) -> Result<Flow, VerseError>;
type CallArgsContinuation = dyn Fn(&Interpreter, Vec<CallValue>) -> Result<Flow, VerseError>;
type FailureContinuation = dyn Fn(&Interpreter, Option<Value>) -> Result<Flow, VerseError>;

thread_local! {
    static CURRENT_EPOCH_SECONDS: RefCell<Option<f64>> = const { RefCell::new(None) };
    static SIMULATION_START_INSTANT: RefCell<Option<Instant>> = const { RefCell::new(None) };
    static CURRENT_RUNTIME_SCHEDULER: RefCell<Option<Rc<RuntimeScheduler>>> = const { RefCell::new(None) };
}

const PLAYER_MAP_RECORD_LIMIT_BYTES: usize = 256 * 1024;
const PLAYER_MAP_MAX_SIZE_DEPTH: usize = 128;

pub enum NativeResult {
    Value(Value),
    Failure(&'static str),
}

struct StructuredSyncState {
    values: Vec<Option<Value>>,
    remaining: usize,
}

struct StructuredFirstState {
    tasks: Vec<(usize, Rc<RuntimeTask>)>,
    cancel_losers: bool,
    completed: bool,
}

struct ForIterationState {
    clauses: Vec<ForClause>,
    index: usize,
    body: Expr,
    env: Env,
    results: Rc<RefCell<Vec<Value>>>,
    bindings: Vec<Vec<(String, Value)>>,
}

struct ForClauseState {
    clauses: Vec<ForClause>,
    index: usize,
    body: Expr,
    env: Env,
    results: Rc<RefCell<Vec<Value>>>,
}

enum FailureEval {
    Ready(Option<Value>),
    Pending(RuntimeSuspension),
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Rational(RationalValue),
    Char(char),
    Char32(char),
    Bool(bool),
    String(String),
    Diagnostic(String),
    External,
    None,
    Pending,
    Suspended(RuntimeSuspension),
    Session,
    Range {
        start: i64,
        end: i64,
    },
    EnumType {
        name: String,
        variants: Vec<String>,
        open: bool,
    },
    EnumValue {
        enum_name: String,
        variant: String,
    },
    StructType {
        name: String,
        computes: bool,
        fields: Vec<RuntimeStructField>,
    },
    StructInstance {
        struct_name: String,
        computes: bool,
        fields: Vec<(String, Value)>,
    },
    ClassType {
        name: String,
        base: Option<String>,
        interfaces: Vec<String>,
        unique: bool,
        abstract_class: bool,
        epic_internal_class: bool,
        final_class: bool,
        concrete: bool,
        castable: bool,
        fields: Vec<RuntimeClassField>,
        methods: Vec<RuntimeClassMethod>,
        blocks: Vec<RuntimeClassBlock>,
    },
    InterfaceType {
        name: String,
        parents: Vec<String>,
        fields: Vec<RuntimeClassField>,
        methods: Vec<RuntimeClassMethod>,
    },
    Module {
        name: String,
        env: Env,
    },
    ClassInstance {
        class_name: String,
        unique: bool,
        fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: Rc<Vec<RuntimeClassMethod>>,
    },
    Array(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<Vec<(Value, Value)>>>),
    Tuple(Vec<Value>),
    Option(Option<Box<Value>>),
    Result {
        succeeded: bool,
        value: Box<Value>,
    },
    Event {
        payload: Option<TypeName>,
        waiters: Rc<RefCell<Vec<Rc<RuntimeTask>>>>,
    },
    Awaitable {
        payload: Option<TypeName>,
    },
    Signalable {
        payload: TypeName,
    },
    Subscribable {
        payload: Option<TypeName>,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: Rc<RefCell<u64>>,
    },
    Listenable {
        payload: Option<TypeName>,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: Rc<RefCell<u64>>,
    },
    SubscriptionCancelHandle {
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        subscriber_id: u64,
    },
    Task(Rc<RuntimeTask>),
    Generator {
        item_type: Option<TypeName>,
        values: Rc<RefCell<Vec<Value>>>,
    },
    Modifier {
        item_type: TypeName,
    },
    ModifierStack {
        item_type: TypeName,
        entries: Rc<RefCell<Vec<RuntimeModifierEntry>>>,
        next_order: Rc<RefCell<u64>>,
    },
    ModifierCancelHandle {
        entries: Rc<RefCell<Vec<RuntimeModifierEntry>>>,
        entry_id: u64,
    },
    CastableSubtype(TypeName),
    ConcreteSubtype(TypeName),
    ClassifiableSubset(Rc<RefCell<Vec<Value>>>),
    ParametricType {
        name: String,
        params: Vec<TypeParam>,
        body: Box<Expr>,
        closure: Env,
    },
    Function {
        params: Vec<Param>,
        effects: Vec<String>,
        body: Box<Expr>,
        closure: Env,
    },
    Overload(Vec<Value>),
    BoundMethod {
        name: String,
        params: Vec<Param>,
        effects: Vec<String>,
        body: Box<Expr>,
        closure: Env,
        super_type: Option<Box<Value>>,
        extension_methods: Rc<Vec<RuntimeExtensionMethod>>,
        class_name: String,
        unique: bool,
        fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: Rc<Vec<RuntimeClassMethod>>,
    },
    NativeFunction {
        name: &'static str,
        arity: Option<usize>,
        decides: bool,
        function: NativeFn,
    },
    NativeResultMethod {
        name: &'static str,
        result: Box<Value>,
    },
    NativeEventMethod {
        name: &'static str,
        payload: Option<TypeName>,
        waiters: Option<Rc<RefCell<Vec<Rc<RuntimeTask>>>>>,
    },
    NativeSubscribableMethod {
        name: &'static str,
        payload: Option<TypeName>,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: Rc<RefCell<u64>>,
    },
    NativeTaskMethod {
        name: &'static str,
        task: Rc<RuntimeTask>,
    },
    NativeModifierMethod {
        name: &'static str,
        receiver: Box<Value>,
    },
    NativeCancelMethod {
        name: &'static str,
        entries: Rc<RefCell<Vec<RuntimeModifierEntry>>>,
        entry_id: u64,
    },
    NativeSubscriptionCancelMethod {
        name: &'static str,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        subscriber_id: u64,
    },
}

#[derive(Clone, PartialEq)]
pub struct RuntimeStructField {
    name: String,
    default: Option<Value>,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeClassField {
    name: String,
    mutable: bool,
    final_member: bool,
    access: RuntimeAccessLevel,
    owner: Option<String>,
    default: Option<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeAccessLevel {
    Public,
    Internal,
    Protected,
    Private,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeClassInstanceField {
    name: String,
    mutable: bool,
    value: Value,
}

#[derive(Clone)]
struct RuntimeDataMemberDefaultContext {
    aggregate_name: String,
    field_name: String,
}

#[derive(Clone)]
pub struct RuntimeClassMethod {
    qualifier: Option<String>,
    name: String,
    final_member: bool,
    params: Vec<Param>,
    effects: Vec<String>,
    body: Option<Box<Expr>>,
    closure: Env,
    super_type: Option<Box<Value>>,
    extension_methods: Rc<Vec<RuntimeExtensionMethod>>,
}

#[derive(Clone)]
pub struct RuntimeExtensionMethod {
    name: String,
    module_name: Option<String>,
    receiver: Param,
    params: Vec<Param>,
    effects: Vec<String>,
    body: Box<Expr>,
    closure: Env,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeModifierEntry {
    id: u64,
    position: RationalValue,
    order: u64,
    modifier: Value,
}

#[derive(Clone)]
pub struct RuntimeSubscriptionEntry {
    id: u64,
    callback: Value,
}

#[derive(Clone)]
pub struct RuntimeClassBlock {
    body: Box<Expr>,
    closure: Env,
    super_type: Option<Box<Value>>,
    extension_methods: Rc<Vec<RuntimeExtensionMethod>>,
}

struct RuntimeClassMembers {
    methods: Vec<RuntimeClassMethod>,
    blocks: Vec<RuntimeClassBlock>,
}

struct RuntimeClassDefinitionParts<'a> {
    specifiers: &'a [String],
    base: Option<&'a TypeAnnotation>,
    interfaces: &'a [TypeAnnotation],
    fields: &'a [StructField],
    methods: &'a [ClassMethod],
    extension_methods: &'a [ExtensionMethod],
    blocks: &'a [ClassBlock],
}

struct RuntimeParametricTypeTemplate<'a> {
    name: &'a str,
    params: &'a [TypeParam],
    body: &'a Expr,
    closure: &'a Env,
}

struct EvaluatedArchetypeField {
    name: String,
    value: Value,
    span: Span,
    explicit: bool,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        if let Some(equal) = numeric_values_equal(self, other) {
            return equal;
        }

        match (self, other) {
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Char(left), Self::Char(right)) => left == right,
            (Self::Char32(left), Self::Char32(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::String(left), Self::Array(right)) => {
                string_equals_char_array(left, right.borrow().as_slice())
            }
            (Self::Array(left), Self::String(right)) => {
                string_equals_char_array(right, left.borrow().as_slice())
            }
            (Self::Diagnostic(left), Self::Diagnostic(right)) => left == right,
            (Self::External, Self::External) => true,
            (Self::None, Self::None) => true,
            (Self::Pending, Self::Pending) | (Self::Suspended(_), Self::Suspended(_)) => true,
            (Self::Session, Self::Session) => true,
            (
                Self::Range {
                    start: left_start,
                    end: left_end,
                },
                Self::Range {
                    start: right_start,
                    end: right_end,
                },
            ) => left_start == right_start && left_end == right_end,
            (
                Self::EnumValue {
                    enum_name: left_enum,
                    variant: left_variant,
                },
                Self::EnumValue {
                    enum_name: right_enum,
                    variant: right_variant,
                },
            ) => left_enum == right_enum && left_variant == right_variant,
            (
                Self::StructInstance {
                    struct_name: left_name,
                    fields: left_fields,
                    ..
                },
                Self::StructInstance {
                    struct_name: right_name,
                    fields: right_fields,
                    ..
                },
            ) => left_name == right_name && left_fields == right_fields,
            (
                Self::ClassInstance {
                    class_name: left_name,
                    unique: left_unique,
                    fields: left_fields,
                    ..
                },
                Self::ClassInstance {
                    class_name: right_name,
                    unique: right_unique,
                    fields: right_fields,
                    ..
                },
            ) => {
                if *left_unique || *right_unique {
                    *left_unique == *right_unique && Rc::ptr_eq(left_fields, right_fields)
                } else {
                    left_name == right_name && *left_fields.borrow() == *right_fields.borrow()
                }
            }
            (Self::Array(left), Self::Array(right)) => *left.borrow() == *right.borrow(),
            (Self::Map(left), Self::Map(right)) => *left.borrow() == *right.borrow(),
            (Self::Tuple(left), Self::Tuple(right)) => left == right,
            (Self::Option(left), Self::Option(right)) => left == right,
            (
                Self::Result {
                    succeeded: left_succeeded,
                    value: left,
                },
                Self::Result {
                    succeeded: right_succeeded,
                    value: right,
                },
            ) => left_succeeded == right_succeeded && left == right,
            (
                Self::Event {
                    payload: left_payload,
                    waiters: left_waiters,
                },
                Self::Event {
                    payload: right_payload,
                    waiters: right_waiters,
                },
            ) => left_payload == right_payload && Rc::ptr_eq(left_waiters, right_waiters),
            (Self::Awaitable { payload: left }, Self::Awaitable { payload: right }) => {
                left == right
            }
            (Self::Signalable { payload: left }, Self::Signalable { payload: right }) => {
                left == right
            }
            (
                Self::Subscribable {
                    payload: left_payload,
                    subscribers: left_subscribers,
                    ..
                },
                Self::Subscribable {
                    payload: right_payload,
                    subscribers: right_subscribers,
                    ..
                },
            )
            | (
                Self::Listenable {
                    payload: left_payload,
                    subscribers: left_subscribers,
                    ..
                },
                Self::Listenable {
                    payload: right_payload,
                    subscribers: right_subscribers,
                    ..
                },
            ) => left_payload == right_payload && Rc::ptr_eq(left_subscribers, right_subscribers),
            (
                Self::SubscriptionCancelHandle {
                    subscribers: left_subscribers,
                    subscriber_id: left_id,
                },
                Self::SubscriptionCancelHandle {
                    subscribers: right_subscribers,
                    subscriber_id: right_id,
                },
            ) => left_id == right_id && Rc::ptr_eq(left_subscribers, right_subscribers),
            (Self::Task(left), Self::Task(right)) => Rc::ptr_eq(left, right),
            (
                Self::Generator {
                    item_type: left_type,
                    values: left_values,
                },
                Self::Generator {
                    item_type: right_type,
                    values: right_values,
                },
            ) => left_type == right_type && *left_values.borrow() == *right_values.borrow(),
            (
                Self::Modifier {
                    item_type: left_type,
                },
                Self::Modifier {
                    item_type: right_type,
                },
            ) => left_type == right_type,
            (
                Self::ModifierStack {
                    item_type: left_type,
                    entries: left_entries,
                    ..
                },
                Self::ModifierStack {
                    item_type: right_type,
                    entries: right_entries,
                    ..
                },
            ) => left_type == right_type && *left_entries.borrow() == *right_entries.borrow(),
            (
                Self::ModifierCancelHandle {
                    entries: left_entries,
                    entry_id: left_id,
                },
                Self::ModifierCancelHandle {
                    entries: right_entries,
                    entry_id: right_id,
                },
            ) => left_id == right_id && Rc::ptr_eq(left_entries, right_entries),
            (Self::CastableSubtype(left), Self::CastableSubtype(right)) => left == right,
            (Self::ConcreteSubtype(left), Self::ConcreteSubtype(right)) => left == right,
            (Self::ClassifiableSubset(left), Self::ClassifiableSubset(right)) => {
                *left.borrow() == *right.borrow()
            }
            (
                Self::ParametricType {
                    name: left,
                    params: left_params,
                    ..
                },
                Self::ParametricType {
                    name: right,
                    params: right_params,
                    ..
                },
            ) => left == right && left_params.len() == right_params.len(),
            (Self::Module { name: left, .. }, Self::Module { name: right, .. }) => left == right,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self}")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(value) => write!(formatter, "{value}"),
            Self::Float(value) if value.fract() == 0.0 => write!(formatter, "{value:.1}"),
            Self::Float(value) => write!(formatter, "{value}"),
            Self::Rational(value) => write!(formatter, "{value}"),
            Self::Char(value) | Self::Char32(value) => {
                write!(formatter, "'{}'", render_char_literal(*value))
            }
            Self::Bool(value) => write!(formatter, "{value}"),
            Self::String(value) => write!(formatter, "{value}"),
            Self::Diagnostic(_) => write!(formatter, "<diagnostic>"),
            Self::External => write!(formatter, "<external>"),
            Self::None => write!(formatter, "none"),
            Self::Pending => write!(formatter, "<pending>"),
            Self::Suspended(_) => write!(formatter, "<suspended>"),
            Self::Session => write!(formatter, "session"),
            Self::Range { start, end } => write!(formatter, "{start}..{end}"),
            Self::EnumType { name, .. } => write!(formatter, "<enum {name}>"),
            Self::EnumValue { enum_name, variant } => write!(formatter, "{enum_name}.{variant}"),
            Self::StructType { name, .. } => write!(formatter, "<struct {name}>"),
            Self::StructInstance {
                struct_name,
                fields,
                ..
            } => {
                let rendered = fields
                    .iter()
                    .map(|(name, value)| format!("{name} := {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "{struct_name}{{{rendered}}}")
            }
            Self::ClassType { name, .. } => write!(formatter, "<class {name}>"),
            Self::InterfaceType { name, .. } => write!(formatter, "<interface {name}>"),
            Self::Module { name, .. } => write!(formatter, "<module {name}>"),
            Self::ClassInstance {
                class_name, fields, ..
            } => {
                let rendered = fields
                    .borrow()
                    .iter()
                    .map(|field| format!("{} := {}", field.name, field.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "{class_name}{{{rendered}}}")
            }
            Self::Array(items) => {
                let rendered = items
                    .borrow()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "[{rendered}]")
            }
            Self::Map(entries) => {
                let rendered = entries
                    .borrow()
                    .iter()
                    .map(|(key, value)| format!("{key} => {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "map{{{rendered}}}")
            }
            Self::Tuple(items) => {
                let rendered = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "({rendered})")
            }
            Self::Option(Some(value)) => write!(formatter, "option{{{value}}}"),
            Self::Option(None) => write!(formatter, "false"),
            Self::Result { succeeded, value } => {
                let label = if *succeeded { "success" } else { "error" };
                write!(formatter, "<result {label}: {value}>")
            }
            Self::Event { payload, .. } => {
                if let Some(payload) = payload {
                    write!(formatter, "<event({})>", render_runtime_type_name(payload))
                } else {
                    write!(formatter, "<event()>")
                }
            }
            Self::Awaitable { payload } => {
                if let Some(payload) = payload {
                    write!(
                        formatter,
                        "<awaitable({})>",
                        render_runtime_type_name(payload)
                    )
                } else {
                    write!(formatter, "<awaitable()>")
                }
            }
            Self::Signalable { payload } => {
                write!(
                    formatter,
                    "<signalable({})>",
                    render_runtime_type_name(payload)
                )
            }
            Self::Subscribable { payload, .. } => {
                if let Some(payload) = payload {
                    write!(
                        formatter,
                        "<subscribable({})>",
                        render_runtime_type_name(payload)
                    )
                } else {
                    write!(formatter, "<subscribable()>")
                }
            }
            Self::Listenable { payload, .. } => {
                if let Some(payload) = payload {
                    write!(
                        formatter,
                        "<listenable({})>",
                        render_runtime_type_name(payload)
                    )
                } else {
                    write!(formatter, "<listenable()>")
                }
            }
            Self::SubscriptionCancelHandle { .. } => write!(formatter, "<cancelable>"),
            Self::Task(_) => write!(formatter, "<task>"),
            Self::Generator { values, .. } => {
                write!(formatter, "<generator({})>", values.borrow().len())
            }
            Self::Modifier { item_type } => {
                write!(
                    formatter,
                    "<modifier({})>",
                    render_runtime_type_name(item_type)
                )
            }
            Self::ModifierStack { entries, .. } => {
                write!(formatter, "<modifier_stack({})>", entries.borrow().len())
            }
            Self::ModifierCancelHandle { .. } => write!(formatter, "<cancelable>"),
            Self::CastableSubtype(item) => {
                write!(
                    formatter,
                    "<castable_subtype({})>",
                    render_runtime_type_name(item)
                )
            }
            Self::ConcreteSubtype(item) => {
                write!(
                    formatter,
                    "<concrete_subtype({})>",
                    render_runtime_type_name(item)
                )
            }
            Self::ClassifiableSubset(items) => {
                write!(formatter, "<classifiable_subset({})>", items.borrow().len())
            }
            Self::ParametricType { name, params, .. } => {
                write!(formatter, "<parametric_type {name}/{}>", params.len())
            }
            Self::Function { params, .. } => write!(
                formatter,
                "<function({})>",
                params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Overload(overloads) => write!(formatter, "<overload({})>", overloads.len()),
            Self::BoundMethod { name, params, .. } => write!(
                formatter,
                "<method {name}({})>",
                params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NativeFunction { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeResultMethod { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeEventMethod { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeSubscribableMethod { name, .. } => {
                write!(formatter, "<native {name}>")
            }
            Self::NativeTaskMethod { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeModifierMethod { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeCancelMethod { name, .. } => write!(formatter, "<native {name}>"),
            Self::NativeSubscriptionCancelMethod { name, .. } => {
                write!(formatter, "<native {name}>")
            }
        }
    }
}

fn render_char_literal(value: char) -> String {
    match value {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\'' => "\\'".to_string(),
        '\\' => "\\\\".to_string(),
        other => other.to_string(),
    }
}

fn color_struct_type() -> Value {
    Value::StructType {
        name: "color".to_string(),
        computes: false,
        fields: ["R", "G", "B"]
            .into_iter()
            .map(|name| RuntimeStructField {
                name: name.to_string(),
                default: None,
            })
            .collect(),
    }
}

fn color_alpha_struct_type() -> Value {
    Value::StructType {
        name: "color_alpha".to_string(),
        computes: false,
        fields: ["Color", "A"]
            .into_iter()
            .map(|name| RuntimeStructField {
                name: name.to_string(),
                default: None,
            })
            .collect(),
    }
}

fn locale_struct_type() -> Value {
    Value::StructType {
        name: "locale".to_string(),
        computes: false,
        fields: Vec::new(),
    }
}

fn session_environment_enum_type() -> Value {
    Value::EnumType {
        name: "session_environment".to_string(),
        variants: vec![
            "Edit".to_string(),
            "Private".to_string(),
            "Live".to_string(),
        ],
        open: false,
    }
}

fn color_value(red: f64, green: f64, blue: f64) -> Value {
    Value::StructInstance {
        struct_name: "color".to_string(),
        computes: false,
        fields: vec![
            ("R".to_string(), Value::Float(red)),
            ("G".to_string(), Value::Float(green)),
            ("B".to_string(), Value::Float(blue)),
        ],
    }
}

fn color_value_from_srgb_values(red: u8, green: u8, blue: u8) -> Value {
    color_value(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    )
}

fn color_alpha_value(color: Value, alpha: f64) -> Value {
    Value::StructInstance {
        struct_name: "color_alpha".to_string(),
        computes: false,
        fields: vec![
            ("Color".to_string(), color),
            ("A".to_string(), Value::Float(alpha)),
        ],
    }
}

fn named_colors_module() -> Value {
    let env = Env::new();
    for color in NAMED_COLORS {
        env.define(
            color.name,
            color_value_from_srgb_values(color.red, color.green, color.blue),
            false,
        );
    }
    Value::Module {
        name: "NamedColors".to_string(),
        env,
    }
}

fn builtin_interface_types() -> Vec<(&'static str, Value)> {
    vec![
        (
            "cancelable",
            builtin_interface_type(
                "cancelable",
                Vec::new(),
                Vec::new(),
                vec![builtin_interface_method("Cancel", &["transacts"])],
            ),
        ),
        (
            "disposable",
            builtin_interface_type(
                "disposable",
                Vec::new(),
                Vec::new(),
                vec![builtin_interface_method("Dispose", &["transacts"])],
            ),
        ),
        (
            "enableable",
            builtin_interface_type(
                "enableable",
                Vec::new(),
                Vec::new(),
                vec![
                    builtin_interface_method("Enable", &["transacts"]),
                    builtin_interface_method("Disable", &["transacts"]),
                    builtin_interface_method("IsEnabled", &["transacts", "decides"]),
                ],
            ),
        ),
        (
            "invalidatable",
            builtin_interface_type(
                "invalidatable",
                vec!["disposable".to_string()],
                Vec::new(),
                vec![
                    builtin_interface_method("Dispose", &["transacts"]),
                    builtin_interface_method("IsValid", &["transacts", "decides"]),
                ],
            ),
        ),
        (
            "showable",
            builtin_interface_type(
                "showable",
                Vec::new(),
                vec![RuntimeClassField {
                    name: "Show".to_string(),
                    mutable: true,
                    final_member: false,
                    access: RuntimeAccessLevel::Public,
                    owner: Some("showable".to_string()),
                    default: None,
                }],
                Vec::new(),
            ),
        ),
    ]
}

fn builtin_interface_type(
    name: &str,
    parents: Vec<String>,
    fields: Vec<RuntimeClassField>,
    methods: Vec<RuntimeClassMethod>,
) -> Value {
    Value::InterfaceType {
        name: name.to_string(),
        parents,
        fields,
        methods,
    }
}

fn builtin_interface_method(name: &str, effects: &[&str]) -> RuntimeClassMethod {
    RuntimeClassMethod {
        qualifier: None,
        name: name.to_string(),
        final_member: false,
        params: Vec::new(),
        effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
        body: None,
        closure: Env::new(),
        super_type: None,
        extension_methods: Rc::new(Vec::new()),
    }
}

fn runtime_modifier_method(item_type: TypeName) -> RuntimeClassMethod {
    RuntimeClassMethod {
        qualifier: None,
        name: "Evaluate".to_string(),
        final_member: false,
        params: vec![Param {
            name: "InValue".to_string(),
            annotation: Some(TypeAnnotation {
                name: item_type,
                span: Span::new(0, 0, 1, 1),
            }),
            type_params: Vec::new(),
            named: false,
            default: None,
            pattern: ParamPattern::Binding,
            span: Span::new(0, 0, 1, 1),
        }],
        effects: Vec::new(),
        body: None,
        closure: Env::new(),
        super_type: None,
        extension_methods: Rc::new(Vec::new()),
    }
}

pub struct Interpreter {
    globals: Env,
    scheduler: Rc<RuntimeScheduler>,
    active_tasks: RefCell<usize>,
    task_stack: RefCell<Vec<Rc<RuntimeTask>>>,
    data_member_default_depth: Cell<usize>,
    data_member_default_stack: RefCell<Vec<RuntimeDataMemberDefaultContext>>,
}

impl Interpreter {
    pub fn new() -> Self {
        SIMULATION_START_INSTANT.with(|start| {
            start.replace(Some(Instant::now()));
        });

        let globals = Env::new();
        globals.define(
            "print",
            Value::NativeFunction {
                name: "print",
                arity: None,
                decides: false,
                function: native_print,
            },
            false,
        );
        globals.define(
            "Print",
            Value::NativeFunction {
                name: "Print",
                arity: None,
                decides: false,
                function: native_print,
            },
            false,
        );
        globals.define("color", color_struct_type(), false);
        globals.define("color_alpha", color_alpha_struct_type(), false);
        globals.define("locale", locale_struct_type(), false);
        globals.define(
            "session_environment",
            session_environment_enum_type(),
            false,
        );
        globals.define("NamedColors", named_colors_module(), false);
        for (name, value) in builtin_interface_types() {
            globals.define(name, value, false);
        }
        globals.define(
            "assert_eq",
            Value::NativeFunction {
                name: "assert_eq",
                arity: Some(2),
                decides: false,
                function: native_assert_eq,
            },
            false,
        );
        globals.define(
            "str",
            Value::NativeFunction {
                name: "str",
                arity: Some(1),
                decides: false,
                function: native_str,
            },
            false,
        );
        globals.define(
            "Err",
            Value::NativeFunction {
                name: "Err",
                arity: Some(1),
                decides: false,
                function: native_err,
            },
            false,
        );
        globals.define(
            "ToDiagnostic",
            Value::NativeFunction {
                name: "ToDiagnostic",
                arity: Some(1),
                decides: false,
                function: native_to_diagnostic,
            },
            false,
        );
        globals.define(
            "GetSecondsSinceEpoch",
            Value::NativeFunction {
                name: "GetSecondsSinceEpoch",
                arity: Some(0),
                decides: false,
                function: native_get_seconds_since_epoch,
            },
            false,
        );
        globals.define(
            "MakeColorFromSRGB",
            Value::NativeFunction {
                name: "MakeColorFromSRGB",
                arity: Some(3),
                decides: false,
                function: native_make_color_from_srgb,
            },
            false,
        );
        globals.define(
            "MakeColorFromSRGBValues",
            Value::NativeFunction {
                name: "MakeColorFromSRGBValues",
                arity: Some(3),
                decides: false,
                function: native_make_color_from_srgb_values,
            },
            false,
        );
        globals.define(
            "MakeSRGBFromColor",
            Value::NativeFunction {
                name: "MakeSRGBFromColor",
                arity: Some(1),
                decides: false,
                function: native_make_srgb_from_color,
            },
            false,
        );
        globals.define(
            "MakeColorFromHex",
            Value::NativeFunction {
                name: "MakeColorFromHex",
                arity: Some(1),
                decides: false,
                function: native_make_color_from_hex,
            },
            false,
        );
        globals.define(
            "MakeColorFromHSV",
            Value::NativeFunction {
                name: "MakeColorFromHSV",
                arity: Some(3),
                decides: false,
                function: native_make_color_from_hsv,
            },
            false,
        );
        globals.define(
            "MakeHSVFromColor",
            Value::NativeFunction {
                name: "MakeHSVFromColor",
                arity: Some(1),
                decides: false,
                function: native_make_hsv_from_color,
            },
            false,
        );
        globals.define(
            "MakeColorAlpha",
            Value::NativeFunction {
                name: "MakeColorAlpha",
                arity: Some(4),
                decides: false,
                function: native_make_color_alpha,
            },
            false,
        );
        globals.define(
            "Over",
            Value::NativeFunction {
                name: "Over",
                arity: Some(2),
                decides: false,
                function: native_over,
            },
            false,
        );
        globals.define(
            "ToString",
            Value::NativeFunction {
                name: "ToString",
                arity: Some(1),
                decides: false,
                function: native_to_string,
            },
            false,
        );
        globals.define(
            "Localize",
            Value::NativeFunction {
                name: "Localize",
                arity: Some(1),
                decides: false,
                function: native_localize,
            },
            false,
        );
        globals.define(
            "Join",
            Value::NativeFunction {
                name: "Join",
                arity: Some(2),
                decides: false,
                function: native_join,
            },
            false,
        );
        globals.define(
            "GetRandomFloat",
            Value::NativeFunction {
                name: "GetRandomFloat",
                arity: Some(2),
                decides: false,
                function: native_get_random_float,
            },
            false,
        );
        globals.define(
            "GetRandomInt",
            Value::NativeFunction {
                name: "GetRandomInt",
                arity: Some(2),
                decides: false,
                function: native_get_random_int,
            },
            false,
        );
        globals.define(
            "Shuffle",
            Value::NativeFunction {
                name: "Shuffle",
                arity: Some(1),
                decides: false,
                function: native_shuffle,
            },
            false,
        );
        globals.define("Inf", Value::Float(f64::INFINITY), false);
        globals.define("NaN", Value::Float(f64::NAN), false);
        globals.define("PiFloat", Value::Float(std::f64::consts::PI), false);
        globals.define(
            "Concatenate",
            Value::NativeFunction {
                name: "Concatenate",
                arity: None,
                decides: false,
                function: native_concatenate,
            },
            false,
        );
        globals.define(
            "ConcatenateMaps",
            Value::NativeFunction {
                name: "ConcatenateMaps",
                arity: Some(2),
                decides: false,
                function: native_concatenate_maps,
            },
            false,
        );
        globals.define(
            "MakeClassifiableSubset",
            Value::NativeFunction {
                name: "MakeClassifiableSubset",
                arity: Some(1),
                decides: false,
                function: native_make_classifiable_subset,
            },
            false,
        );
        globals.define(
            "GetSession",
            Value::NativeFunction {
                name: "GetSession",
                arity: Some(0),
                decides: false,
                function: native_get_session,
            },
            false,
        );
        globals.define(
            "GetSimulationElapsedTime",
            Value::NativeFunction {
                name: "GetSimulationElapsedTime",
                arity: Some(0),
                decides: false,
                function: native_get_simulation_elapsed_time,
            },
            false,
        );
        globals.define(
            "Sleep",
            Value::NativeFunction {
                name: "Sleep",
                arity: Some(1),
                decides: false,
                function: native_sleep,
            },
            false,
        );
        globals.define(
            "MakeSuccess",
            Value::NativeFunction {
                name: "MakeSuccess",
                arity: Some(1),
                decides: false,
                function: native_make_success,
            },
            false,
        );
        globals.define(
            "MakeError",
            Value::NativeFunction {
                name: "MakeError",
                arity: Some(1),
                decides: false,
                function: native_make_error,
            },
            false,
        );
        globals.define(
            "FitsInPlayerMap",
            Value::NativeFunction {
                name: "FitsInPlayerMap",
                arity: Some(1),
                decides: true,
                function: native_fits_in_player_map,
            },
            false,
        );
        globals.define(
            "Mod",
            Value::NativeFunction {
                name: "Mod",
                arity: Some(2),
                decides: true,
                function: native_mod,
            },
            false,
        );
        globals.define(
            "Quotient",
            Value::NativeFunction {
                name: "Quotient",
                arity: Some(2),
                decides: true,
                function: native_quotient,
            },
            false,
        );
        globals.define(
            "Clamp",
            Value::NativeFunction {
                name: "Clamp",
                arity: Some(3),
                decides: false,
                function: native_clamp,
            },
            false,
        );
        globals.define(
            "Lerp",
            Value::NativeFunction {
                name: "Lerp",
                arity: Some(3),
                decides: false,
                function: native_lerp,
            },
            false,
        );
        globals.define(
            "Abs",
            Value::NativeFunction {
                name: "Abs",
                arity: Some(1),
                decides: false,
                function: native_abs,
            },
            false,
        );
        globals.define(
            "Min",
            Value::NativeFunction {
                name: "Min",
                arity: Some(2),
                decides: false,
                function: native_min,
            },
            false,
        );
        globals.define(
            "Max",
            Value::NativeFunction {
                name: "Max",
                arity: Some(2),
                decides: false,
                function: native_max,
            },
            false,
        );
        globals.define(
            "Ceil",
            Value::NativeFunction {
                name: "Ceil",
                arity: Some(1),
                decides: true,
                function: native_ceil,
            },
            false,
        );
        globals.define(
            "Floor",
            Value::NativeFunction {
                name: "Floor",
                arity: Some(1),
                decides: true,
                function: native_floor,
            },
            false,
        );
        globals.define(
            "Round",
            Value::NativeFunction {
                name: "Round",
                arity: Some(1),
                decides: true,
                function: native_round,
            },
            false,
        );
        globals.define(
            "Int",
            Value::NativeFunction {
                name: "Int",
                arity: Some(1),
                decides: true,
                function: native_int,
            },
            false,
        );
        for (name, function) in [
            ("Sqrt", native_sqrt as NativeFn),
            ("Sin", native_sin),
            ("Cos", native_cos),
            ("Tan", native_tan),
            ("ArcSin", native_arcsin),
            ("ArcCos", native_arccos),
            ("Sinh", native_sinh),
            ("Cosh", native_cosh),
            ("Tanh", native_tanh),
            ("ArSinh", native_arsinh),
            ("ArCosh", native_arcosh),
            ("ArTanh", native_artanh),
            ("Exp", native_exp),
            ("Ln", native_ln),
            ("Sgn", native_sgn),
        ] {
            globals.define(
                name,
                Value::NativeFunction {
                    name,
                    arity: Some(1),
                    decides: false,
                    function,
                },
                false,
            );
        }
        for (name, function) in [("Pow", native_pow as NativeFn), ("Log", native_log)] {
            globals.define(
                name,
                Value::NativeFunction {
                    name,
                    arity: Some(2),
                    decides: false,
                    function,
                },
                false,
            );
        }
        globals.define(
            "ArcTan",
            Value::NativeFunction {
                name: "ArcTan",
                arity: None,
                decides: false,
                function: native_arctan,
            },
            false,
        );
        globals.define(
            "IsAlmostEqual",
            Value::NativeFunction {
                name: "IsAlmostEqual",
                arity: Some(3),
                decides: true,
                function: native_is_almost_equal,
            },
            false,
        );

        Self {
            globals,
            scheduler: Rc::new(RuntimeScheduler::new()),
            active_tasks: RefCell::new(0),
            task_stack: RefCell::new(Vec::new()),
            data_member_default_depth: Cell::new(0),
            data_member_default_stack: RefCell::new(Vec::new()),
        }
    }

    pub fn eval_source(&mut self, source: &str) -> Result<Value, VerseError> {
        let program = parse_source(source)?;
        self.eval_program(&program)
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<Value, VerseError> {
        let program = desugar_program(program);
        self.eval_desugared_program(&program)
    }

    pub fn eval_typed_program(&mut self, program: &TypedProgram) -> Result<Value, VerseError> {
        self.eval_desugared_program(&program.program)
    }

    fn eval_desugared_program(&mut self, program: &Program) -> Result<Value, VerseError> {
        let previous_epoch_seconds =
            CURRENT_EPOCH_SECONDS.with(|seconds| -> Result<Option<f64>, VerseError> {
                Ok(seconds.replace(Some(current_unix_epoch_seconds(Span::new(0, 0, 1, 1))?)))
            })?;
        let previous_scheduler = CURRENT_RUNTIME_SCHEDULER
            .with(|scheduler| scheduler.replace(Some(self.scheduler.clone())));

        let result = match self.eval_top_level_statements(&program.statements, &self.globals) {
            Ok(Flow::Value(value)) => Ok(value),
            Ok(Flow::Return(_)) => Err(VerseError::runtime("`return` used outside a function")),
            Ok(Flow::Break) => Err(VerseError::runtime("`break` used outside a loop")),
            Ok(Flow::Pending(_)) => Err(VerseError::runtime(
                "expression is suspended and cannot complete without async scheduling support",
            )),
            Err(error) => Err(error),
        };

        CURRENT_EPOCH_SECONDS.with(|seconds| {
            seconds.replace(previous_epoch_seconds);
        });
        CURRENT_RUNTIME_SCHEDULER.with(|scheduler| {
            scheduler.replace(previous_scheduler);
        });

        result
    }

    fn eval_top_level_statements(
        &self,
        statements: &[Stmt],
        env: &Env,
    ) -> Result<Flow, VerseError> {
        self.eval_statements_from(
            statements.to_vec(),
            0,
            env.clone(),
            Vec::new(),
            Value::None,
            true,
        )
    }

    fn eval_statements(&self, statements: &[Stmt], env: &Env) -> Result<Flow, VerseError> {
        self.eval_statements_from(
            statements.to_vec(),
            0,
            env.clone(),
            Vec::new(),
            Value::None,
            false,
        )
    }

    fn eval_statements_from(
        &self,
        statements: Vec<Stmt>,
        start: usize,
        env: Env,
        mut defers: Vec<Deferred>,
        mut last: Value,
        drive_scheduler: bool,
    ) -> Result<Flow, VerseError> {
        for index in start..statements.len() {
            let statement = &statements[index];
            if let StmtKind::Defer(body) = &statement.kind {
                defers.push(Deferred {
                    body: body.clone(),
                    env: env.clone(),
                    span: statement.span,
                });
                last = Value::None;
                if drive_scheduler {
                    self.run_next_tick_sleepers();
                }
                continue;
            }

            match self.eval_stmt(statement, &env)? {
                Flow::Value(value) => {
                    last = value;
                    if drive_scheduler {
                        self.run_next_tick_sleepers();
                    }
                }
                Flow::Pending(suspension) => {
                    let remaining = statements.clone();
                    let continuation_env = env.clone();
                    let continuation_defers = defers.clone();
                    let cancel_defers = defers.clone();
                    return Ok(Flow::Pending(
                        suspension
                            .map(move |interpreter, flow| {
                                interpreter.continue_statements_after_pending(
                                    flow,
                                    remaining.clone(),
                                    index + 1,
                                    continuation_env.clone(),
                                    continuation_defers.clone(),
                                    drive_scheduler,
                                )
                            })
                            .on_cancel(move |interpreter| interpreter.run_defers(&cancel_defers)),
                    ));
                }
                signal => return self.finish_statements_flow(signal, &defers),
            }
        }

        self.finish_statements_flow(Flow::Value(last), &defers)
    }

    fn continue_statements_after_pending(
        &self,
        flow: Flow,
        statements: Vec<Stmt>,
        next_index: usize,
        env: Env,
        defers: Vec<Deferred>,
        drive_scheduler: bool,
    ) -> Result<Flow, VerseError> {
        match flow {
            Flow::Value(value) => self.eval_statements_from(
                statements,
                next_index,
                env,
                defers,
                value,
                drive_scheduler,
            ),
            Flow::Return(_) | Flow::Break => self.finish_statements_flow(flow, &defers),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    interpreter.continue_statements_after_pending(
                        flow,
                        statements.clone(),
                        next_index,
                        env.clone(),
                        defers.clone(),
                        drive_scheduler,
                    )
                })))
            }
        }
    }

    fn finish_statements_flow(&self, flow: Flow, defers: &[Deferred]) -> Result<Flow, VerseError> {
        if matches!(flow, Flow::Value(_) | Flow::Return(_) | Flow::Break) {
            self.run_defers(defers)?;
        }
        Ok(flow)
    }

    fn run_next_tick_sleepers(&self) {
        if *self.active_tasks.borrow() != 0 {
            return;
        }

        self.scheduler.cleanup_detached_tasks();
        let mut ready = self.scheduler.take_next_tick_sleepers();
        ready.extend(self.scheduler.take_ready_timed_sleepers(Instant::now()));

        if ready.is_empty()
            && let Some(deadline) = self.scheduler.next_timed_deadline()
        {
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline.duration_since(now));
            }
            ready.extend(self.scheduler.take_ready_timed_sleepers(Instant::now()));
        }

        for task in ready {
            task.resume(self, Value::None);
        }
        self.scheduler.cleanup_detached_tasks();
    }

    fn eval_in_task_context(
        &self,
        task: Rc<RuntimeTask>,
        expr: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        self.eval_with_task_context(task, || self.eval_expr(expr, env))
    }

    fn eval_with_task_context<T>(
        &self,
        task: Rc<RuntimeTask>,
        action: impl FnOnce() -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        *self.active_tasks.borrow_mut() += 1;
        self.task_stack.borrow_mut().push(task);
        let result = action();
        self.task_stack.borrow_mut().pop();
        *self.active_tasks.borrow_mut() -= 1;
        result
    }

    fn track_scoped_task(&self, task: Rc<RuntimeTask>) {
        if let Some(parent) = self.task_stack.borrow().last().cloned() {
            if !Rc::ptr_eq(&parent, &task) {
                parent.track_scoped_child(task);
            }
        } else {
            self.scheduler.track_detached_task(task);
        }
    }

    fn track_scoped_tasks_except(
        &self,
        tasks: &[(usize, Rc<RuntimeTask>)],
        completed_index: usize,
    ) {
        for (index, task) in tasks {
            if *index != completed_index && !task.is_complete() {
                self.track_scoped_task(task.clone());
            }
        }
    }

    fn run_defers(&self, defers: &[Deferred]) -> Result<(), VerseError> {
        for deferred in defers.iter().rev() {
            match self.eval_expr(&deferred.body, &deferred.env)? {
                Flow::Value(_) => {}
                Flow::Return(_) => {
                    return Err(VerseError::runtime_at(
                        "`return` cannot escape a `defer` body",
                        deferred.span,
                    ));
                }
                Flow::Break => {
                    return Err(VerseError::runtime_at(
                        "`break` cannot escape a `defer` body",
                        deferred.span,
                    ));
                }
                Flow::Pending(_) => {
                    return Err(VerseError::runtime_at(
                        "defer body cannot suspend",
                        deferred.span,
                    ));
                }
            }
        }

        Ok(())
    }

    fn flow_value_or_error(&self, flow: Flow, span: Span) -> Result<Value, VerseError> {
        match flow {
            Flow::Value(value) => Ok(value),
            Flow::Return(_) => Err(VerseError::runtime_at(
                "`return` used outside a function",
                span,
            )),
            Flow::Break => Err(VerseError::runtime_at("`break` used outside a loop", span)),
            Flow::Pending(_) => Err(VerseError::runtime_at(
                "expression is suspended and cannot complete without async scheduling support",
                span,
            )),
        }
    }

    fn bind_let_value(
        &self,
        name: &str,
        annotation: Option<&TypeAnnotation>,
        is_function_expr: bool,
        value: Value,
        env: &Env,
    ) -> Value {
        let value = match value {
            Value::EnumType { variants, open, .. } => Value::EnumType {
                name: name.to_string(),
                variants,
                open,
            },
            Value::StructType {
                computes, fields, ..
            } => Value::StructType {
                name: name.to_string(),
                computes,
                fields,
            },
            value @ Value::ClassType { .. }
                if should_coerce_class_type_for_annotation(env, annotation) =>
            {
                coerce_annotated_value(env, annotation, value)
            }
            Value::ClassType {
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                fields,
                methods,
                blocks,
                ..
            } => Value::ClassType {
                name: name.to_string(),
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                fields,
                methods,
                blocks,
            },
            Value::InterfaceType {
                parents,
                fields,
                methods,
                ..
            } => Value::InterfaceType {
                name: name.to_string(),
                parents,
                fields: qualify_runtime_interface_fields(name, fields),
                methods: qualify_runtime_interface_methods(name, methods),
            },
            Value::Module { env, .. } => Value::Module {
                name: {
                    let module_name = env.qualified_module_name(name);
                    env.qualify_module_scope(&module_name);
                    name.to_string()
                },
                env,
            },
            other => coerce_annotated_value(env, annotation, other),
        };
        if is_function_expr {
            env.define_function(name, value.clone());
        } else {
            env.define(name, value.clone(), false);
        }
        value
    }

    fn eval_named_type_definition(
        &self,
        name: &str,
        expr: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let runtime_name = env.qualified_module_name(name);
        match &expr.kind {
            ExprKind::StructDefinition {
                computes, fields, ..
            } => self
                .eval_struct_definition(Some(&runtime_name), *computes, fields, env)
                .map(Some),
            ExprKind::ClassDefinition {
                specifiers,
                base,
                interfaces,
                fields,
                methods,
                extension_methods,
                blocks,
                ..
            } => self
                .eval_class_definition(
                    Some(&runtime_name),
                    RuntimeClassDefinitionParts {
                        specifiers,
                        base: base.as_ref(),
                        interfaces,
                        fields,
                        methods,
                        extension_methods,
                        blocks,
                    },
                    env,
                )
                .map(Some),
            ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } => self
                .eval_interface_definition(Some(&runtime_name), parents, fields, methods, env)
                .map(Some),
            _ => Ok(None),
        }
    }

    fn eval_stmt(&self, statement: &Stmt, env: &Env) -> Result<Flow, VerseError> {
        match &statement.kind {
            StmtKind::Using { path } => {
                if !path.starts_with('/') {
                    let module = self.eval_module_path(path, env, statement.span)?;
                    env.import_module(module);
                }
                Ok(Flow::Value(Value::None))
            }
            StmtKind::Let {
                name,
                annotation,
                expr,
                ..
            } => {
                let is_function_expr = matches!(&expr.kind, ExprKind::Function { .. });
                let expr_span = expr.span;
                let value_flow =
                    if let Some(value) = self.eval_named_type_definition(name, expr, env)? {
                        Flow::Value(value)
                    } else {
                        self.eval_expr(expr, env)?
                    };
                let value = match value_flow {
                    Flow::Value(value) => value,
                    Flow::Pending(suspension) => {
                        let name = name.clone();
                        let annotation = annotation.clone();
                        let env = env.clone();
                        return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                            let name = name.clone();
                            let annotation = annotation.clone();
                            let env = env.clone();
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(move |interpreter, value| {
                                    let value = interpreter.bind_let_value(
                                        &name,
                                        annotation.as_ref(),
                                        is_function_expr,
                                        value,
                                        &env,
                                    );
                                    Ok(Flow::Value(value))
                                });
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                expr_span,
                                value_continuation,
                            )
                        })));
                    }
                    flow => return self.flow_value_or_error(flow, expr.span).map(Flow::Value),
                };
                let value =
                    self.bind_let_value(name, annotation.as_ref(), is_function_expr, value, env);
                Ok(Flow::Value(value))
            }
            StmtKind::TypeAlias { name, target } => {
                env.define_type_alias(name, target.name.clone());
                Ok(Flow::Value(Value::None))
            }
            StmtKind::ParametricType {
                name, params, expr, ..
            } => {
                if matches!(
                    &expr.kind,
                    ExprKind::ClassDefinition { specifiers, .. }
                        if class_has_specifier(specifiers, "persistable")
                ) {
                    return Err(VerseError::runtime_at(
                        format!("persistable class `{name}` cannot be parametric"),
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
                    return Err(VerseError::runtime_at(
                        format!("persistable struct `{name}` cannot be parametric"),
                        statement.span,
                    ));
                }
                let value = Value::ParametricType {
                    name: name.clone(),
                    params: params.clone(),
                    body: Box::new(expr.clone()),
                    closure: env.clone(),
                };
                env.define(name, value.clone(), false);
                Ok(Flow::Value(value))
            }
            StmtKind::ExtensionMethod(method) => {
                self.eval_extension_method_definition(method, env)?;
                Ok(Flow::Value(Value::None))
            }
            StmtKind::Var {
                name,
                annotation,
                expr,
            } => {
                let expr_span = expr.span;
                let value = match self.eval_expr(expr, env)? {
                    Flow::Value(value) => value,
                    Flow::Pending(suspension) => {
                        let name = name.clone();
                        let annotation = annotation.clone();
                        let env = env.clone();
                        return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                            let name = name.clone();
                            let annotation = annotation.clone();
                            let env = env.clone();
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(move |_, value| {
                                    let value =
                                        coerce_annotated_value(&env, annotation.as_ref(), value);
                                    env.define(&name, value.clone(), true);
                                    Ok(Flow::Value(value))
                                });
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                expr_span,
                                value_continuation,
                            )
                        })));
                    }
                    flow => return self.flow_value_or_error(flow, expr_span).map(Flow::Value),
                };
                let value = coerce_annotated_value(env, annotation.as_ref(), value);
                env.define(name, value.clone(), true);
                Ok(Flow::Value(value))
            }
            StmtKind::Set { target, op, expr } => self.eval_set_expression(target, *op, expr, env),
            StmtKind::Return(expr) => {
                let expr_span = expr.span;
                match self.eval_expr(expr, env)? {
                    Flow::Value(value) => Ok(Flow::Return(value)),
                    Flow::Pending(suspension) => {
                        Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(|_, value| Ok(Flow::Return(value)));
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                expr_span,
                                value_continuation,
                            )
                        })))
                    }
                    flow => self.flow_value_or_error(flow, expr_span).map(Flow::Return),
                }
            }
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Defer(_) => Ok(Flow::Value(Value::None)),
            StmtKind::Expr(expr) => self.eval_expr(expr, env),
        }
    }

    fn eval_extension_method_definition(
        &self,
        extension: &ExtensionMethod,
        env: &Env,
    ) -> Result<(), VerseError> {
        let Some(body) = extension.method.body.as_ref() else {
            return Err(VerseError::runtime_at(
                "extension method requires a body",
                extension.span,
            ));
        };

        env.define_extension_method(
            extension.method.name.clone(),
            RuntimeExtensionMethod {
                name: extension.method.name.clone(),
                module_name: env.module_name(),
                receiver: extension.receiver.clone(),
                params: extension.method.params.clone(),
                effects: extension.method.effects.clone(),
                body: Box::new(body.clone()),
                closure: env.clone(),
            },
        );
        Ok(())
    }

    fn eval_module_path(&self, path: &str, env: &Env, span: Span) -> Result<Env, VerseError> {
        let mut parts = path.split('.');
        let Some(first) = parts.next() else {
            return Err(VerseError::runtime_at("expected module path", span));
        };
        let mut value = env
            .get(first)
            .ok_or_else(|| VerseError::runtime_at(format!("undefined module `{first}`"), span))?;
        for part in parts {
            value = self.member_value(value, part, env, span)?;
        }
        match value {
            Value::Module { env, .. } => Ok(env),
            other => Err(VerseError::runtime_at(
                format!("`{path}` is not a module, got `{other}`"),
                span,
            )),
        }
    }

    fn eval_value(&self, expr: &Expr, env: &Env) -> Result<Value, VerseError> {
        match self.eval_expr(expr, env)? {
            Flow::Value(value) => Ok(value),
            Flow::Return(_) => Err(VerseError::runtime_at(
                "`return` used outside a function",
                expr.span,
            )),
            Flow::Break => Err(VerseError::runtime_at(
                "`break` used outside a loop",
                expr.span,
            )),
            Flow::Pending(_) => Err(VerseError::runtime_at(
                "expression is suspended and cannot complete without async scheduling support",
                expr.span,
            )),
        }
    }

    fn continue_when_value(
        interpreter: &Interpreter,
        flow: Flow,
        span: Span,
        continuation: Rc<ValueContinuation>,
    ) -> Result<Flow, VerseError> {
        match flow {
            Flow::Value(value) => continuation(interpreter, value),
            Flow::Return(_) => Err(VerseError::runtime_at(
                "`return` used outside a function",
                span,
            )),
            Flow::Break => Err(VerseError::runtime_at("`break` used outside a loop", span)),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    Self::continue_when_value(interpreter, flow, span, continuation.clone())
                })))
            }
        }
    }

    fn continue_when_failure_result(
        interpreter: &Interpreter,
        flow: Flow,
        span: Span,
        continuation: Rc<FailureContinuation>,
    ) -> Result<Flow, VerseError> {
        match flow {
            Flow::Value(Value::Result { succeeded, value }) => {
                if succeeded {
                    continuation(interpreter, Some(*value))
                } else {
                    continuation(interpreter, None)
                }
            }
            Flow::Value(other) => Err(VerseError::runtime_at(
                format!("internal failure continuation expected result, got `{other}`"),
                span,
            )),
            Flow::Return(_) => Err(VerseError::runtime_at(
                "`return` used outside a function",
                span,
            )),
            Flow::Break => Err(VerseError::runtime_at("`break` used outside a loop", span)),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    Self::continue_when_failure_result(
                        interpreter,
                        flow,
                        span,
                        continuation.clone(),
                    )
                })))
            }
        }
    }

    fn eval_values_then(
        &self,
        items: &[Expr],
        mut index: usize,
        env: &Env,
        mut values: Vec<Value>,
        continuation: Rc<ValuesContinuation>,
    ) -> Result<Flow, VerseError> {
        while let Some(item) = items.get(index) {
            let item_span = item.span;
            match self.eval_expr(item, env)? {
                Flow::Value(value) => {
                    values.push(value);
                    index += 1;
                }
                Flow::Pending(suspension) => {
                    let items = items.to_vec();
                    let env = env.clone();
                    let prefix = copy_values(&values);
                    let next_index = index + 1;
                    return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                        let items = items.clone();
                        let env = env.clone();
                        let prefix = copy_values(&prefix);
                        let continuation = continuation.clone();
                        let value_continuation: Rc<ValueContinuation> =
                            Rc::new(move |interpreter, value| {
                                let mut values = copy_values(&prefix);
                                values.push(value);
                                interpreter.eval_values_then(
                                    &items,
                                    next_index,
                                    &env,
                                    values,
                                    continuation.clone(),
                                )
                            });
                        Self::continue_when_value(interpreter, flow, item_span, value_continuation)
                    })));
                }
                flow => return self.flow_value_or_error(flow, item_span).map(Flow::Value),
            }
        }

        continuation(self, values)
    }

    fn eval_call_args_then(
        &self,
        args: &[CallArg],
        mut index: usize,
        env: &Env,
        mut values: Vec<CallValue>,
        continuation: Rc<CallArgsContinuation>,
    ) -> Result<Flow, VerseError> {
        while let Some(arg) = args.get(index) {
            let expr = call_arg_expr(arg);
            let expr_span = expr.span;
            match self.eval_expr(expr, env)? {
                Flow::Value(value) => {
                    values.push(call_value_from_arg(arg, value));
                    index += 1;
                }
                Flow::Pending(suspension) => {
                    let args = args.to_vec();
                    let arg = arg.clone();
                    let env = env.clone();
                    let prefix = copy_call_values(&values);
                    let next_index = index + 1;
                    return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                        let args = args.clone();
                        let arg = arg.clone();
                        let env = env.clone();
                        let prefix = copy_call_values(&prefix);
                        let continuation = continuation.clone();
                        let value_continuation: Rc<ValueContinuation> =
                            Rc::new(move |interpreter, value| {
                                let mut values = copy_call_values(&prefix);
                                values.push(call_value_from_arg(&arg, value));
                                interpreter.eval_call_args_then(
                                    &args,
                                    next_index,
                                    &env,
                                    values,
                                    continuation.clone(),
                                )
                            });
                        Self::continue_when_value(interpreter, flow, expr_span, value_continuation)
                    })));
                }
                flow => return self.flow_value_or_error(flow, expr_span).map(Flow::Value),
            }
        }

        continuation(self, values)
    }

    fn eval_call_args_ready(
        &self,
        args: &[CallArg],
        env: &Env,
    ) -> Result<Vec<CallValue>, VerseError> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(match arg {
                CallArg::Positional(expr) => CallValue {
                    name: None,
                    optional: false,
                    value: self.eval_value(expr, env)?,
                    span: expr.span,
                },
                CallArg::Named {
                    name,
                    optional,
                    expr,
                    span,
                } => CallValue {
                    name: Some(name.clone()),
                    optional: *optional,
                    value: self.eval_value(expr, env)?,
                    span: *span,
                },
            });
        }
        Ok(values)
    }

    fn eval_spawn(&self, body: &Expr, env: &Env) -> Result<Value, VerseError> {
        let spawned_expr = runtime_spawn_body_expr(body)?;
        let task_env = Env::child(env);
        let task = RuntimeTask::new_running();
        match self.eval_in_task_context(task.clone(), spawned_expr, &task_env) {
            Ok(flow) => task.set_from_flow(flow, Some(self)),
            Err(error) => task.complete_with_error(error, Some(self)),
        }
        self.scheduler.track_detached_task(task.clone());
        Ok(Value::Task(task))
    }

    fn eval_concurrent_task(
        &self,
        statement: &Stmt,
        env: &Env,
    ) -> Result<Rc<RuntimeTask>, VerseError> {
        let branch_env = Env::child(env);
        let task = RuntimeTask::new_running();
        match self.eval_with_task_context(task.clone(), || self.eval_stmt(statement, &branch_env)) {
            Ok(flow) => task.set_from_flow(flow, Some(self)),
            Err(error) => task.complete_with_error(error, Some(self)),
        }
        Ok(task)
    }

    fn eval_branch_task(&self, statement: &Stmt, env: &Env) -> Result<(), VerseError> {
        let task = self.eval_concurrent_task(statement, env)?;
        self.track_scoped_task(task);
        Ok(())
    }

    fn eval_sync_concurrent(
        &self,
        statements: &[Stmt],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let mut state = StructuredSyncState {
            values: vec![None; statements.len()],
            remaining: 0,
        };
        let mut waiting = Vec::new();
        let mut tasks = Vec::with_capacity(statements.len());

        for statement in statements {
            tasks.push(self.eval_concurrent_task(statement, env)?);
        }

        for (index, task) in tasks.into_iter().enumerate() {
            match task.await_result()? {
                Some(value) => state.values[index] = Some(value),
                None => {
                    state.remaining += 1;
                    waiting.push((index, task));
                }
            }
        }

        if state.remaining == 0 {
            return Ok(Flow::Value(Value::Tuple(structured_sync_values(&state)?)));
        }

        let state = Rc::new(RefCell::new(state));
        let wait = Rc::new(RefCell::new(StructuredTaskWait {
            tasks: waiting,
            registered: false,
        }));
        Ok(Self::sync_pending_flow(state, wait, span))
    }

    fn sync_pending_flow(
        state: Rc<RefCell<StructuredSyncState>>,
        wait: Rc<RefCell<StructuredTaskWait>>,
        span: Span,
    ) -> Flow {
        Flow::Pending(RuntimeSuspension::structured_tasks(wait.clone()).map(
            move |interpreter, flow| {
                interpreter.eval_sync_after_child(flow, state.clone(), wait.clone(), span)
            },
        ))
    }

    fn eval_sync_after_child(
        &self,
        flow: Flow,
        state: Rc<RefCell<StructuredSyncState>>,
        wait: Rc<RefCell<StructuredTaskWait>>,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let value = self.flow_value_or_error(flow, span)?;
        let (index, value) = structured_task_result_parts(value)?;

        {
            let mut state = state.borrow_mut();
            if index >= state.values.len() {
                return Err(VerseError::runtime(
                    "internal structured task branch index is out of bounds",
                ));
            }
            if state.values[index].is_none() {
                state.values[index] = Some(value);
                state.remaining = state.remaining.checked_sub(1).ok_or_else(|| {
                    VerseError::runtime("internal structured sync remaining count underflowed")
                })?;
            }
            if state.remaining == 0 {
                return Ok(Flow::Value(Value::Tuple(structured_sync_values(&state)?)));
            }
        }

        Ok(Self::sync_pending_flow(state, wait, span))
    }

    fn first_completed_task(
        tasks: &[(usize, Rc<RuntimeTask>)],
    ) -> Result<Option<(usize, Value)>, VerseError> {
        for (index, task) in tasks {
            if let Some(value) = task.await_result()? {
                return Ok(Some((*index, value)));
            }
        }
        Ok(None)
    }

    fn cancel_structured_losers(
        &self,
        tasks: &[(usize, Rc<RuntimeTask>)],
        winner_index: usize,
    ) -> Result<(), VerseError> {
        for (index, task) in tasks {
            if *index != winner_index {
                task.cancel_silently(self)?;
            }
        }
        Ok(())
    }

    fn eval_race_concurrent(
        &self,
        statements: &[Stmt],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let mut tasks = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let task = self.eval_concurrent_task(statement, env)?;
            tasks.push((index, task));
            if let Some((winner_index, value)) = Self::first_completed_task(&tasks)? {
                self.cancel_structured_losers(&tasks, winner_index)?;
                return Ok(Flow::Value(value));
            }
        }

        Ok(Self::first_pending_flow(tasks, true, span))
    }

    fn eval_rush_concurrent(
        &self,
        statements: &[Stmt],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let mut tasks = Vec::new();
        let mut winner = None;

        for (index, statement) in statements.iter().enumerate() {
            let task = self.eval_concurrent_task(statement, env)?;
            tasks.push((index, task));
            if winner.is_none() {
                winner = Self::first_completed_task(&tasks)?;
            }
        }

        if let Some((winner_index, value)) = winner {
            self.track_scoped_tasks_except(&tasks, winner_index);
            return Ok(Flow::Value(value));
        }

        Ok(Self::first_pending_flow(tasks, false, span))
    }

    fn first_pending_flow(
        tasks: Vec<(usize, Rc<RuntimeTask>)>,
        cancel_losers: bool,
        span: Span,
    ) -> Flow {
        let wait = Rc::new(RefCell::new(StructuredTaskWait {
            tasks: tasks.clone(),
            registered: false,
        }));
        let state = Rc::new(RefCell::new(StructuredFirstState {
            tasks,
            cancel_losers,
            completed: false,
        }));
        Flow::Pending(
            RuntimeSuspension::structured_tasks(wait).map(move |interpreter, flow| {
                interpreter.eval_first_after_child(flow, state.clone(), span)
            }),
        )
    }

    fn eval_first_after_child(
        &self,
        flow: Flow,
        state: Rc<RefCell<StructuredFirstState>>,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let value = self.flow_value_or_error(flow, span)?;
        let (winner_index, value) = structured_task_result_parts(value)?;
        let (cancel_losers, tasks) = {
            let mut state = state.borrow_mut();
            if state.completed {
                return Ok(Flow::Pending(RuntimeSuspension::unresumable()));
            }
            state.completed = true;
            (state.cancel_losers, state.tasks.clone())
        };
        if cancel_losers {
            self.cancel_structured_losers(&tasks, winner_index)?;
        } else {
            self.track_scoped_tasks_except(&tasks, winner_index);
        }
        Ok(Flow::Value(value))
    }

    fn eval_concurrent(
        &self,
        op: ConcurrentOp,
        body: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let statements = runtime_concurrent_body_statements(body)?;
        let block_env = Env::child(env);

        match op {
            ConcurrentOp::Sync => self.eval_sync_concurrent(statements, &block_env, body.span),
            ConcurrentOp::Race => self.eval_race_concurrent(statements, &block_env, body.span),
            ConcurrentOp::Rush => self.eval_rush_concurrent(statements, &block_env, body.span),
            ConcurrentOp::Branch => {
                for statement in statements {
                    self.eval_branch_task(statement, &block_env)?;
                }
                Ok(Flow::Value(Value::None))
            }
        }
    }

    fn eval_interpolated_string_parts(
        &self,
        parts: &[InterpolatedStringPart],
        mut index: usize,
        env: &Env,
        mut text: String,
    ) -> Result<Flow, VerseError> {
        while let Some(part) = parts.get(index) {
            match part {
                InterpolatedStringPart::Text(part_text) => {
                    text.push_str(part_text);
                    index += 1;
                }
                InterpolatedStringPart::Expr(expr) => {
                    let expr_span = expr.span;
                    match self.eval_expr(expr, env)? {
                        Flow::Value(value) => {
                            text.push_str(&value.to_string());
                            index += 1;
                        }
                        Flow::Pending(suspension) => {
                            let parts = parts.to_vec();
                            let env = env.clone();
                            let prefix = text.clone();
                            let next_index = index + 1;
                            return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                                let parts = parts.clone();
                                let env = env.clone();
                                let prefix = prefix.clone();
                                let value_continuation: Rc<ValueContinuation> =
                                    Rc::new(move |interpreter, value| {
                                        let mut text = prefix.clone();
                                        text.push_str(&value.to_string());
                                        interpreter.eval_interpolated_string_parts(
                                            &parts, next_index, &env, text,
                                        )
                                    });
                                Self::continue_when_value(
                                    interpreter,
                                    flow,
                                    expr_span,
                                    value_continuation,
                                )
                            })));
                        }
                        flow => {
                            return self.flow_value_or_error(flow, expr_span).map(Flow::Value);
                        }
                    }
                }
            }
        }

        Ok(Flow::Value(Value::String(text)))
    }

    fn eval_unary_expression(
        &self,
        op: UnaryOp,
        operand: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let operand_span = operand.span;
        match self.eval_expr(operand, env)? {
            Flow::Value(value) => self.eval_unary(op, value, operand_span).map(Flow::Value),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, value| {
                            interpreter
                                .eval_unary(op, value, operand_span)
                                .map(Flow::Value)
                        });
                    Self::continue_when_value(interpreter, flow, operand_span, value_continuation)
                })))
            }
            flow => self
                .flow_value_or_error(flow, operand_span)
                .map(Flow::Value),
        }
    }

    fn eval_var_expression(
        &self,
        name: &str,
        annotation: &TypeAnnotation,
        value_expr: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let value_span = value_expr.span;
        match self.eval_expr(value_expr, env)? {
            Flow::Value(value) => {
                let value = coerce_annotated_value(env, Some(annotation), value);
                env.define(name, value.clone(), true);
                Ok(Flow::Value(value))
            }
            Flow::Pending(suspension) => {
                let name = name.to_string();
                let annotation = annotation.clone();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let name = name.clone();
                    let annotation = annotation.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> = Rc::new(move |_, value| {
                        let value = coerce_annotated_value(&env, Some(&annotation), value);
                        env.define(&name, value.clone(), true);
                        Ok(Flow::Value(value))
                    });
                    Self::continue_when_value(interpreter, flow, value_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, value_span).map(Flow::Value),
        }
    }

    fn eval_profile_expression(
        &self,
        description: &Expr,
        body: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let description_span = description.span;
        match self.eval_expr(description, env)? {
            Flow::Value(value) => {
                expect_profile_description(&value, description_span)?;
                self.eval_expr(body, env)
            }
            Flow::Pending(suspension) => {
                let body = body.clone();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let body = body.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, value| {
                            expect_profile_description(&value, description_span)?;
                            interpreter.eval_expr(&body, &env)
                        });
                    Self::continue_when_value(
                        interpreter,
                        flow,
                        description_span,
                        value_continuation,
                    )
                })))
            }
            flow => self
                .flow_value_or_error(flow, description_span)
                .map(Flow::Value),
        }
    }

    fn eval_call_expression(
        &self,
        callee: &Expr,
        args: &[CallArg],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        if let ExprKind::Member { object, name } = &callee.kind
            && name == "Length"
        {
            return self.eval_member_call_expression(object, name, args, env, span, callee.span);
        }

        let callee_span = callee.span;
        match self.eval_expr(callee, env)? {
            Flow::Value(callee) => self.eval_call_after_callee(callee, args, env, span),
            Flow::Pending(suspension) => {
                let args = args.to_vec();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let args = args.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, callee| {
                            interpreter.eval_call_after_callee(callee, &args, &env, span)
                        });
                    Self::continue_when_value(interpreter, flow, callee_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, callee_span).map(Flow::Value),
        }
    }

    fn eval_member_call_expression(
        &self,
        object: &Expr,
        name: &str,
        args: &[CallArg],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Flow, VerseError> {
        let object_span = object.span;
        match self.eval_expr(object, env)? {
            Flow::Value(object) => {
                self.eval_member_call_after_object(object, name, args, env, span, callee_span)
            }
            Flow::Pending(suspension) => {
                let name = name.to_string();
                let args = args.to_vec();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let name = name.clone();
                    let args = args.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, object| {
                            interpreter.eval_member_call_after_object(
                                object,
                                &name,
                                &args,
                                &env,
                                span,
                                callee_span,
                            )
                        });
                    Self::continue_when_value(interpreter, flow, object_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, object_span).map(Flow::Value),
        }
    }

    fn eval_member_call_after_object(
        &self,
        object_value: Value,
        name: &str,
        args: &[CallArg],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Flow, VerseError> {
        let callee_value = self.member_value(object_value.clone(), name, env, callee_span)?;
        if is_builtin_length_receiver_value(&object_value) && matches!(callee_value, Value::Int(_))
        {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`Length` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            Ok(Flow::Value(callee_value))
        } else {
            self.eval_call_after_callee(callee_value, args, env, span)
        }
    }

    fn eval_call_after_callee(
        &self,
        callee: Value,
        args: &[CallArg],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        if matches!(callee, Value::ParametricType { .. }) {
            return self
                .eval_parametric_type_call(callee, args, env, span)
                .map(flow_from_value);
        }
        self.ensure_data_member_default_callee_allowed(&callee, span)?;

        let callee = value_copy(&callee);
        let continuation: Rc<CallArgsContinuation> = Rc::new(move |interpreter, values| {
            interpreter
                .call(value_copy(&callee), values, span)
                .map(flow_from_value)
        });
        self.eval_call_args_then(args, 0, env, Vec::new(), continuation)
    }

    fn eval_array_expression(&self, items: &[Expr], env: &Env) -> Result<Flow, VerseError> {
        let continuation: Rc<ValuesContinuation> =
            Rc::new(|_, values| Ok(Flow::Value(Value::Array(Rc::new(RefCell::new(values))))));
        self.eval_values_then(items, 0, env, Vec::new(), continuation)
    }

    fn eval_tuple_expression(&self, items: &[Expr], env: &Env) -> Result<Flow, VerseError> {
        let continuation: Rc<ValuesContinuation> =
            Rc::new(|_, values| Ok(Flow::Value(Value::Tuple(values))));
        self.eval_values_then(items, 0, env, Vec::new(), continuation)
    }

    fn eval_map_expression(&self, entries: &[(Expr, Expr)], env: &Env) -> Result<Flow, VerseError> {
        self.eval_map_entries_from(entries, 0, env, Vec::new())
    }

    fn eval_map_entries_from(
        &self,
        entries: &[(Expr, Expr)],
        index: usize,
        env: &Env,
        values: Vec<(Value, Value)>,
    ) -> Result<Flow, VerseError> {
        let Some((key_expr, value_expr)) = entries.get(index) else {
            return Ok(Flow::Value(Value::Map(Rc::new(RefCell::new(values)))));
        };

        let key_span = key_expr.span;
        match self.eval_expr(key_expr, env)? {
            Flow::Value(key) => {
                self.eval_map_entry_value(entries, index, key, value_expr, env, values)
            }
            Flow::Pending(suspension) => {
                let entries = entries.to_vec();
                let env = env.clone();
                let prefix = copy_map_entries(&values);
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let entries = entries.clone();
                    let env = env.clone();
                    let prefix = copy_map_entries(&prefix);
                    let value_expr = entries[index].1.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, key| {
                            interpreter.eval_map_entry_value(
                                &entries,
                                index,
                                key,
                                &value_expr,
                                &env,
                                copy_map_entries(&prefix),
                            )
                        });
                    Self::continue_when_value(interpreter, flow, key_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, key_span).map(Flow::Value),
        }
    }

    fn eval_map_entry_value(
        &self,
        entries: &[(Expr, Expr)],
        index: usize,
        key: Value,
        value_expr: &Expr,
        env: &Env,
        mut values: Vec<(Value, Value)>,
    ) -> Result<Flow, VerseError> {
        let value_span = value_expr.span;
        match self.eval_expr(value_expr, env)? {
            Flow::Value(value) => {
                upsert_map_entry(&mut values, key, value);
                self.eval_map_entries_from(entries, index + 1, env, values)
            }
            Flow::Pending(suspension) => {
                let entries = entries.to_vec();
                let env = env.clone();
                let prefix = copy_map_entries(&values);
                let key = value_copy(&key);
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let entries = entries.clone();
                    let env = env.clone();
                    let prefix = copy_map_entries(&prefix);
                    let key = value_copy(&key);
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, value| {
                            let mut values = copy_map_entries(&prefix);
                            upsert_map_entry(&mut values, value_copy(&key), value);
                            interpreter.eval_map_entries_from(&entries, index + 1, &env, values)
                        });
                    Self::continue_when_value(interpreter, flow, value_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, value_span).map(Flow::Value),
        }
    }

    fn eval_unwrap_option_expression(
        &self,
        value: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let value_span = value.span;
        match self.eval_expr(value, env)? {
            Flow::Value(value) => unwrap_option_value(value, span).map(Flow::Value),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |_, value| unwrap_option_value(value, span).map(Flow::Value));
                    Self::continue_when_value(interpreter, flow, value_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, value_span).map(Flow::Value),
        }
    }

    fn eval_qualified_member_expression(
        &self,
        object: &Expr,
        qualifier: &str,
        name: &str,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let object_span = object.span;
        match self.eval_expr(object, env)? {
            Flow::Value(object) => self
                .qualified_member_value(object, qualifier, name, span, env)
                .map(Flow::Value),
            Flow::Pending(suspension) => {
                let qualifier = qualifier.to_string();
                let name = name.to_string();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let qualifier = qualifier.clone();
                    let name = name.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, object| {
                            interpreter
                                .qualified_member_value(object, &qualifier, &name, span, &env)
                                .map(Flow::Value)
                        });
                    Self::continue_when_value(interpreter, flow, object_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, object_span).map(Flow::Value),
        }
    }

    fn eval_member_expression(
        &self,
        object: &Expr,
        name: &str,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let object_span = object.span;
        match self.eval_expr(object, env)? {
            Flow::Value(object) => self.member_value(object, name, env, span).map(Flow::Value),
            Flow::Pending(suspension) => {
                let name = name.to_string();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let name = name.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, object| {
                            interpreter
                                .member_value(object, &name, &env, span)
                                .map(Flow::Value)
                        });
                    Self::continue_when_value(interpreter, flow, object_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, object_span).map(Flow::Value),
        }
    }

    fn eval_index_expression(
        &self,
        collection: &Expr,
        index: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let collection_span = collection.span;
        match self.eval_expr(collection, env)? {
            Flow::Value(collection) => {
                self.eval_index_after_collection(collection, index, span, env)
            }
            Flow::Pending(suspension) => {
                let index = index.clone();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let index = index.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, collection| {
                            interpreter.eval_index_after_collection(collection, &index, span, &env)
                        });
                    Self::continue_when_value(
                        interpreter,
                        flow,
                        collection_span,
                        value_continuation,
                    )
                })))
            }
            flow => self
                .flow_value_or_error(flow, collection_span)
                .map(Flow::Value),
        }
    }

    fn eval_index_after_collection(
        &self,
        collection: Value,
        index: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let index_span = index.span;
        match self.eval_expr(index, env)? {
            Flow::Value(index) => self.index_value(collection, index, span).map(Flow::Value),
            Flow::Pending(suspension) => {
                let collection = value_copy(&collection);
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let collection = value_copy(&collection);
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, index| {
                            interpreter
                                .index_value(value_copy(&collection), index, span)
                                .map(Flow::Value)
                        });
                    Self::continue_when_value(interpreter, flow, index_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, index_span).map(Flow::Value),
        }
    }

    fn eval_expr(&self, expr: &Expr, env: &Env) -> Result<Flow, VerseError> {
        let value = match &expr.kind {
            ExprKind::Number { value, kind } => match kind {
                NumberKind::Int => {
                    let NumberLiteral::Int(value) = value else {
                        unreachable!("int number kind should carry an int literal");
                    };
                    let value = i64::try_from(*value).map_err(|_| {
                        VerseError::runtime_at(
                            "integer literal is outside the 64-bit signed range",
                            expr.span,
                        )
                    })?;
                    Value::Int(value)
                }
                NumberKind::Float => {
                    let NumberLiteral::Float(value) = value else {
                        unreachable!("float number kind should carry a float literal");
                    };
                    Value::Float(*value)
                }
            },
            ExprKind::Char { value, kind } => match kind {
                CharacterKind::Char => Value::Char(*value),
                CharacterKind::Char32 => Value::Char32(*value),
            },
            ExprKind::Bool(value) => Value::Bool(*value),
            ExprKind::String(value) => Value::String(value.clone()),
            ExprKind::InterpolatedString(parts) => {
                return self.eval_interpolated_string_parts(parts, 0, env, String::new());
            }
            ExprKind::None => Value::None,
            ExprKind::Ident(name) => {
                if name != "Self"
                    && let Some(value) = self.self_field_value(env, name)
                {
                    value
                } else {
                    env.get(name).ok_or_else(|| {
                        VerseError::runtime_at(format!("undefined name `{name}`"), expr.span)
                    })?
                }
            }
            ExprKind::Unary { op, expr } => {
                return self.eval_unary_expression(*op, expr, env);
            }
            ExprKind::Binary { left, op, right } => {
                self.eval_binary(left, *op, right, expr.span, env)?
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                return self.eval_if_expression(
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    env,
                );
            }
            ExprKind::FailureBind { .. } | ExprKind::FailureSequence(_) => {
                return Err(VerseError::runtime_at(
                    "failure binding is only valid in an `if` condition",
                    expr.span,
                ));
            }
            ExprKind::Set { target, op, expr } => {
                return self.eval_set_expression(target, *op, expr, env);
            }
            ExprKind::Var {
                name,
                annotation,
                expr,
            } => {
                return self.eval_var_expression(name, annotation, expr, env);
            }
            ExprKind::External => Value::External,
            ExprKind::Loop { body } => {
                loop {
                    match self.eval_expr(body, env)? {
                        Flow::Value(_) => {}
                        Flow::Break => break,
                        signal @ Flow::Return(_) => return Ok(signal),
                        Flow::Pending(suspension) => return Ok(Flow::Pending(suspension)),
                    }
                }
                Value::None
            }
            ExprKind::For { clauses, body } => return self.eval_for(clauses, body, env),
            ExprKind::Profile { description, body } => {
                return self.eval_profile_expression(description, body, env);
            }
            ExprKind::Spawn { body } => self.eval_spawn(body, env)?,
            ExprKind::Concurrent { op, body } => return self.eval_concurrent(*op, body, env),
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                let block_env = Env::child(env);
                return self.eval_statements(statements, &block_env);
            }
            ExprKind::Function {
                params,
                effects,
                body,
                ..
            } => Value::Function {
                params: params.clone(),
                effects: effects.clone(),
                body: body.clone(),
                closure: env.clone(),
            },
            ExprKind::Call { callee, args } => {
                return self.eval_call_expression(callee, args, env, expr.span);
            }
            ExprKind::BracketCall { callee, args } => {
                return self.eval_bracket_call(callee, args, env, expr.span);
            }
            ExprKind::Array(items) => {
                return self.eval_array_expression(items, env);
            }
            ExprKind::Map(entries) => {
                return self.eval_map_expression(entries, env);
            }
            ExprKind::EnumDefinition { open, variants, .. } => Value::EnumType {
                name: "<anonymous>".to_string(),
                variants: variants
                    .iter()
                    .map(|variant| variant.name.clone())
                    .collect(),
                open: *open,
            },
            ExprKind::StructDefinition {
                computes, fields, ..
            } => self.eval_struct_definition(None, *computes, fields, env)?,
            ExprKind::ClassDefinition {
                specifiers,
                base,
                interfaces,
                fields,
                methods,
                extension_methods,
                blocks,
                ..
            } => self.eval_class_definition(
                None,
                RuntimeClassDefinitionParts {
                    specifiers,
                    base: base.as_ref(),
                    interfaces,
                    fields,
                    methods,
                    extension_methods,
                    blocks,
                },
                env,
            )?,
            ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } => self.eval_interface_definition(None, parents, fields, methods, env)?,
            ExprKind::ModuleDefinition { statements, .. } => {
                self.eval_module_definition(statements, env)?
            }
            ExprKind::Archetype {
                callee, entries, ..
            } => self.eval_archetype(callee, entries, env)?,
            ExprKind::Case { subject, arms } => {
                return self.eval_case(subject, arms, env, expr.span);
            }
            ExprKind::Tuple(items) => {
                return self.eval_tuple_expression(items, env);
            }
            ExprKind::Option(value) => {
                return self.eval_option_expression(value.as_deref(), env);
            }
            ExprKind::UnwrapOption(value) => {
                return self.eval_unwrap_option_expression(value, expr.span, env);
            }
            ExprKind::QualifiedName { qualifier, name } => {
                self.qualified_name_value(qualifier, name, env, expr.span)?
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => {
                return self
                    .eval_qualified_member_expression(object, qualifier, name, expr.span, env);
            }
            ExprKind::Member { object, name } => {
                return self.eval_member_expression(object, name, expr.span, env);
            }
            ExprKind::Index {
                collection: collection_expr,
                index,
            } => {
                return self.eval_index_expression(collection_expr, index, expr.span, env);
            }
        };

        Ok(flow_from_value(value))
    }

    fn eval_failure_condition(
        &self,
        condition: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        match &condition.kind {
            ExprKind::FailureSequence(clauses) => {
                let mut last = Value::Bool(true);
                for clause in clauses {
                    let Some(value) = self.eval_failure_condition(clause, env)? else {
                        return Ok(None);
                    };
                    last = value;
                }
                Ok(Some(last))
            }
            ExprKind::FailureBind { name, expr } => {
                let Some(value) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                env.define(name, value.clone(), false);
                Ok(Some(value))
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.eval_failure_statements(statements, env)
            }
            _ => self.eval_failure_expr(condition, env),
        }
    }

    fn eval_failure_condition_transactional(
        &self,
        condition: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let transaction = EnvTransaction::capture(env);
        let result = self.eval_failure_condition(condition, env);
        if !matches!(&result, Ok(Some(_))) {
            transaction.restore();
        }
        result
    }

    fn eval_failure_expr_transactional(
        &self,
        expr: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let transaction = EnvTransaction::capture(env);
        let result = self.eval_failure_expr(expr, env);
        if !matches!(&result, Ok(Some(_))) {
            transaction.restore();
        }
        result
    }

    fn eval_failure_condition_transactional_maybe_pending(
        &self,
        condition: &Expr,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        let transaction = Rc::new(RefCell::new(Some(EnvTransaction::capture(env))));
        let result = self.eval_failure_condition_maybe_pending(condition, env)?;
        Ok(wrap_failure_transaction(
            result,
            transaction,
            condition.span,
        ))
    }

    fn eval_failure_expr_transactional_maybe_pending(
        &self,
        expr: &Expr,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        let transaction = Rc::new(RefCell::new(Some(EnvTransaction::capture(env))));
        let result = self.eval_failure_expr_maybe_pending(expr, env)?;
        Ok(wrap_failure_transaction(result, transaction, expr.span))
    }

    fn eval_failure_condition_maybe_pending(
        &self,
        condition: &Expr,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        match &condition.kind {
            ExprKind::FailureSequence(clauses) => self.eval_failure_sequence_maybe_pending(
                clauses.to_vec(),
                0,
                Value::Bool(true),
                env,
                condition.span,
            ),
            ExprKind::FailureBind { name, expr } => {
                let expr_span = expr.span;
                match self.eval_failure_expr_maybe_pending(expr, env)? {
                    FailureEval::Ready(Some(value)) => {
                        env.define(name, value.clone(), false);
                        Ok(FailureEval::Ready(Some(value)))
                    }
                    FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
                    FailureEval::Pending(suspension) => {
                        let name = name.clone();
                        let env = env.clone();
                        Ok(FailureEval::Pending(suspension.map(
                            move |interpreter, flow| {
                                let name = name.clone();
                                let env = env.clone();
                                let continuation: Rc<FailureContinuation> =
                                    Rc::new(move |_, result| {
                                        if let Some(value) = result {
                                            env.define(&name, value.clone(), false);
                                            Ok(failure_result_flow(Some(value)))
                                        } else {
                                            Ok(failure_result_flow(None))
                                        }
                                    });
                                Self::continue_when_failure_result(
                                    interpreter,
                                    flow,
                                    expr_span,
                                    continuation,
                                )
                            },
                        )))
                    }
                }
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => self
                .eval_failure_statements(statements, env)
                .map(FailureEval::Ready),
            _ => self.eval_failure_expr_maybe_pending(condition, env),
        }
    }

    fn eval_failure_sequence_maybe_pending(
        &self,
        clauses: Vec<Expr>,
        index: usize,
        last: Value,
        env: &Env,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        let Some(clause) = clauses.get(index) else {
            return Ok(FailureEval::Ready(Some(last)));
        };

        match self.eval_failure_condition_maybe_pending(clause, env)? {
            FailureEval::Ready(Some(value)) => {
                self.eval_failure_sequence_maybe_pending(clauses, index + 1, value, env, span)
            }
            FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
            FailureEval::Pending(suspension) => {
                let env = env.clone();
                let next_index = index + 1;
                Ok(FailureEval::Pending(suspension.map(
                    move |interpreter, flow| {
                        let clauses = clauses.clone();
                        let env = env.clone();
                        let continuation: Rc<FailureContinuation> =
                            Rc::new(move |interpreter, result| {
                                let Some(value) = result else {
                                    return Ok(failure_result_flow(None));
                                };
                                failure_eval_to_flow(
                                    interpreter.eval_failure_sequence_maybe_pending(
                                        clauses.clone(),
                                        next_index,
                                        value,
                                        &env,
                                        span,
                                    )?,
                                )
                            });
                        Self::continue_when_failure_result(interpreter, flow, span, continuation)
                    },
                )))
            }
        }
    }

    fn eval_failure_expr_maybe_pending(
        &self,
        expr: &Expr,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        match &expr.kind {
            ExprKind::UnwrapOption(value) => {
                self.eval_failure_unwrap_option_maybe_pending(value, expr.span, env)
            }
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr: operand,
            } => {
                let operand_span = operand.span;
                match self.eval_failure_expr_transactional_maybe_pending(operand, env)? {
                    FailureEval::Ready(result) => {
                        Ok(FailureEval::Ready(invert_failure_result(result)))
                    }
                    FailureEval::Pending(suspension) => Ok(FailureEval::Pending(suspension.map(
                        move |interpreter, flow| {
                            let continuation: Rc<FailureContinuation> =
                                Rc::new(move |_, result| {
                                    Ok(failure_result_flow(invert_failure_result(result)))
                                });
                            Self::continue_when_failure_result(
                                interpreter,
                                flow,
                                operand_span,
                                continuation,
                            )
                        },
                    ))),
                }
            }
            ExprKind::Binary { left, op, right } if *op == BinaryOp::And => {
                self.eval_failure_and_maybe_pending(left, right, expr.span, env)
            }
            ExprKind::Binary { left, op, right } if *op == BinaryOp::Or => {
                self.eval_failure_or_maybe_pending(left, right, expr.span, env)
            }
            ExprKind::Profile { description, body } => {
                let description = self.eval_value(description, env)?;
                expect_profile_description(&description, expr.span)?;
                self.eval_failure_expr_maybe_pending(body, env)
            }
            ExprKind::BracketCall { callee, args } => {
                self.eval_failure_bracket_call_maybe_pending(callee, args, env, expr.span)
            }
            ExprKind::Set {
                target,
                op,
                expr: value,
            } => self.eval_failure_set_expression_maybe_pending(target, *op, value, env),
            _ => self.eval_failure_expr(expr, env).map(FailureEval::Ready),
        }
    }

    fn eval_failure_and_maybe_pending(
        &self,
        left: &Expr,
        right: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        match self.eval_failure_expr_maybe_pending(left, env)? {
            FailureEval::Ready(Some(_)) => self.eval_failure_expr_maybe_pending(right, env),
            FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
            FailureEval::Pending(suspension) => {
                let right = right.clone();
                let env = env.clone();
                Ok(FailureEval::Pending(suspension.map(
                    move |interpreter, flow| {
                        let right = right.clone();
                        let env = env.clone();
                        let continuation: Rc<FailureContinuation> =
                            Rc::new(move |interpreter, result| {
                                if result.is_none() {
                                    return Ok(failure_result_flow(None));
                                }
                                failure_eval_to_flow(
                                    interpreter.eval_failure_expr_maybe_pending(&right, &env)?,
                                )
                            });
                        Self::continue_when_failure_result(interpreter, flow, span, continuation)
                    },
                )))
            }
        }
    }

    fn eval_failure_or_maybe_pending(
        &self,
        left: &Expr,
        right: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        match self.eval_failure_expr_transactional_maybe_pending(left, env)? {
            FailureEval::Ready(Some(value)) => Ok(FailureEval::Ready(Some(value))),
            FailureEval::Ready(None) => self.eval_failure_expr_maybe_pending(right, env),
            FailureEval::Pending(suspension) => {
                let right = right.clone();
                let env = env.clone();
                Ok(FailureEval::Pending(suspension.map(
                    move |interpreter, flow| {
                        let right = right.clone();
                        let env = env.clone();
                        let continuation: Rc<FailureContinuation> =
                            Rc::new(move |interpreter, result| {
                                if let Some(value) = result {
                                    return Ok(failure_result_flow(Some(value)));
                                }
                                failure_eval_to_flow(
                                    interpreter.eval_failure_expr_maybe_pending(&right, &env)?,
                                )
                            });
                        Self::continue_when_failure_result(interpreter, flow, span, continuation)
                    },
                )))
            }
        }
    }

    fn eval_failure_unwrap_option_maybe_pending(
        &self,
        value: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        let value_span = value.span;
        match self.eval_expr(value, env)? {
            Flow::Value(value) => {
                let result = unwrap_option_failure_value(value, span)?;
                Ok(FailureEval::Ready(result))
            }
            Flow::Pending(suspension) => Ok(FailureEval::Pending(suspension.map(
                move |interpreter, flow| {
                    let value_continuation: Rc<ValueContinuation> = Rc::new(move |_, value| {
                        unwrap_option_failure_value(value, span).map(failure_result_flow)
                    });
                    Self::continue_when_value(interpreter, flow, value_span, value_continuation)
                },
            ))),
            flow => self
                .flow_value_or_error(flow, value_span)
                .map(|value| FailureEval::Ready(Some(value))),
        }
    }

    fn eval_failure_statements(
        &self,
        statements: &[Stmt],
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let mut defers = Vec::new();
        let mut last = Value::None;

        let result = (|| {
            for statement in statements {
                match &statement.kind {
                    StmtKind::Let {
                        name,
                        annotation,
                        expr,
                        ..
                    } => {
                        let Some(value) = self.eval_failure_expr(expr, env)? else {
                            return Ok(None);
                        };
                        let value = match value {
                            Value::EnumType { variants, open, .. } => Value::EnumType {
                                name: name.clone(),
                                variants,
                                open,
                            },
                            Value::StructType {
                                computes, fields, ..
                            } => Value::StructType {
                                name: name.clone(),
                                computes,
                                fields,
                            },
                            value @ Value::ClassType { .. }
                                if should_coerce_class_type_for_annotation(
                                    env,
                                    annotation.as_ref(),
                                ) =>
                            {
                                coerce_annotated_value(env, annotation.as_ref(), value)
                            }
                            Value::ClassType {
                                base,
                                interfaces,
                                unique,
                                abstract_class,
                                epic_internal_class,
                                final_class,
                                concrete,
                                castable,
                                fields,
                                methods,
                                blocks,
                                ..
                            } => Value::ClassType {
                                name: name.clone(),
                                base,
                                interfaces,
                                unique,
                                abstract_class,
                                epic_internal_class,
                                final_class,
                                concrete,
                                castable,
                                fields,
                                methods,
                                blocks,
                            },
                            Value::InterfaceType {
                                parents,
                                fields,
                                methods,
                                ..
                            } => Value::InterfaceType {
                                name: name.clone(),
                                parents,
                                fields: qualify_runtime_interface_fields(name, fields),
                                methods: qualify_runtime_interface_methods(name, methods),
                            },
                            Value::Module { env, .. } => Value::Module {
                                name: name.clone(),
                                env,
                            },
                            other => coerce_annotated_value(env, annotation.as_ref(), other),
                        };
                        env.define(name, value.clone(), false);
                        last = value;
                    }
                    StmtKind::TypeAlias { name, target } => {
                        env.define_type_alias(name, target.name.clone());
                        last = Value::None;
                    }
                    StmtKind::ParametricType {
                        name, params, expr, ..
                    } => {
                        let value = Value::ParametricType {
                            name: name.clone(),
                            params: params.clone(),
                            body: Box::new(expr.clone()),
                            closure: env.clone(),
                        };
                        env.define(name, value, false);
                        last = Value::None;
                    }
                    StmtKind::ExtensionMethod(method) => {
                        self.eval_extension_method_definition(method, env)?;
                        last = Value::None;
                    }
                    StmtKind::Var {
                        name,
                        annotation,
                        expr,
                    } => {
                        let Some(value) = self.eval_failure_expr(expr, env)? else {
                            return Ok(None);
                        };
                        let value = coerce_annotated_value(env, annotation.as_ref(), value);
                        env.define(name, value.clone(), true);
                        last = value;
                    }
                    StmtKind::Set { target, op, expr } => {
                        let Some(value) =
                            self.eval_failure_set_expression(target, *op, expr, env)?
                        else {
                            return Ok(None);
                        };
                        last = value;
                    }
                    StmtKind::Return(expr) => {
                        let Some(value) = self.eval_failure_expr(expr, env)? else {
                            return Ok(None);
                        };
                        return Ok(Some(value));
                    }
                    StmtKind::Break => {
                        return Err(VerseError::runtime_at(
                            "`break` escaped failure context",
                            statement.span,
                        ));
                    }
                    StmtKind::Using { path } => {
                        if !path.starts_with('/') {
                            let module = self.eval_module_path(path, env, statement.span)?;
                            env.import_module(module);
                        }
                        last = Value::None;
                    }
                    StmtKind::Defer(body) => {
                        defers.push(Deferred {
                            body: body.clone(),
                            env: env.clone(),
                            span: statement.span,
                        });
                        last = Value::None;
                    }
                    StmtKind::Expr(expr) => {
                        let Some(value) = self.eval_failure_expr(expr, env)? else {
                            return Ok(None);
                        };
                        last = value;
                    }
                }
            }

            Ok(Some(last))
        })();

        if result.is_ok() {
            self.run_defers(&defers)?;
        }

        result
    }

    fn eval_failure_expr(&self, expr: &Expr, env: &Env) -> Result<Option<Value>, VerseError> {
        match &expr.kind {
            ExprKind::UnwrapOption(value) => match self.eval_value(value, env)? {
                Value::Option(Some(value)) => Ok(Some(*value)),
                Value::Option(None) | Value::Bool(false) => Ok(None),
                Value::Bool(true) => Ok(Some(Value::Bool(true))),
                other => Err(VerseError::runtime_at(
                    format!("query operator expected bool or option, got `{other}`"),
                    expr.span,
                )),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => {
                if self.eval_failure_expr_transactional(expr, env)?.is_some() {
                    Ok(None)
                } else {
                    Ok(Some(Value::Bool(true)))
                }
            }
            ExprKind::BracketCall { callee, args } => {
                self.eval_failure_bracket_call(callee, args, env, expr.span)
            }
            ExprKind::Case { subject, arms } => self.eval_failure_case(subject, arms, env),
            ExprKind::Set {
                target,
                op,
                expr: value,
            } => self.eval_failure_set_expression(target, *op, value, env),
            ExprKind::Var {
                name,
                annotation,
                expr: value,
            } => {
                let Some(value) = self.eval_failure_expr(value, env)? else {
                    return Ok(None);
                };
                let value = coerce_annotated_value(env, Some(annotation), value);
                env.define(name, value.clone(), true);
                Ok(Some(value))
            }
            ExprKind::Member { object, name } => {
                let object = if is_failable_condition_expr(object) {
                    let Some(value) = self.eval_failure_expr(object, env)? else {
                        return Ok(None);
                    };
                    value
                } else {
                    self.eval_value(object, env)?
                };
                self.member_value(object, name, env, expr.span).map(Some)
            }
            ExprKind::Index { collection, index } => {
                let collection = if is_failable_condition_expr(collection) {
                    let Some(value) = self.eval_failure_expr(collection, env)? else {
                        return Ok(None);
                    };
                    value
                } else {
                    self.eval_value(collection, env)?
                };
                let index = if is_failable_condition_expr(index) {
                    let Some(value) = self.eval_failure_expr(index, env)? else {
                        return Ok(None);
                    };
                    value
                } else {
                    self.eval_value(index, env)?
                };
                self.index_value_failable(collection, index, expr.span)
            }
            ExprKind::Call { callee, args } => {
                let callee = if is_failable_condition_expr(callee) {
                    let Some(value) = self.eval_failure_expr(callee, env)? else {
                        return Ok(None);
                    };
                    value
                } else {
                    self.eval_value(callee, env)?
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(match arg {
                        CallArg::Positional(expr) => CallValue {
                            name: None,
                            optional: false,
                            value: self.eval_value(expr, env)?,
                            span: expr.span,
                        },
                        CallArg::Named {
                            name,
                            optional,
                            expr,
                            span,
                        } => CallValue {
                            name: Some(name.clone()),
                            optional: *optional,
                            value: self.eval_value(expr, env)?,
                            span: *span,
                        },
                    });
                }
                self.call(callee, values, expr.span).map(Some)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                let block_env = Env::child(env);
                self.eval_failure_statements(statements, &block_env)
            }
            ExprKind::Profile { description, body } => {
                let description = self.eval_value(description, env)?;
                expect_profile_description(&description, expr.span)?;
                self.eval_failure_expr(body, env)
            }
            ExprKind::For { clauses, body } => self.eval_for_failure(clauses, body, env),
            ExprKind::Binary { left, op, right } if *op == BinaryOp::And => {
                let Some(_) = self.eval_failure_expr(left, env)? else {
                    return Ok(None);
                };
                self.eval_failure_expr(right, env)
            }
            ExprKind::Binary { left, op, right } if *op == BinaryOp::Or => {
                if let Some(value) = self.eval_failure_expr_transactional(left, env)? {
                    return Ok(Some(value));
                }
                self.eval_failure_expr(right, env)
            }
            ExprKind::Binary { left, op, right } if is_comparison_binary_op(*op) => {
                let left_value = self.eval_value(left, env)?;
                let right_value = self.eval_value(right, env)?;
                match eval_binary_values(value_copy(&left_value), *op, right_value, expr.span)? {
                    Value::Bool(true) => Ok(Some(left_value)),
                    Value::Bool(false) => Ok(None),
                    other => Err(VerseError::runtime_at(
                        format!("comparison operator produced `{other}`"),
                        expr.span,
                    )),
                }
            }
            ExprKind::Binary { left, op, right } if *op == BinaryOp::Divide => {
                let left = self.eval_value(left, env)?;
                let right = self.eval_value(right, env)?;
                if numeric_value_is_zero(&right, "`/` right operand", expr.span)? {
                    return Ok(None);
                }
                Ok(Some(divide_values(left, right, expr.span)?))
            }
            ExprKind::Binary { left, op, right } if *op == BinaryOp::Remainder => {
                let left = self.eval_value(left, env)?;
                let right = self.eval_value(right, env)?;
                if numeric_value_is_zero(&right, "`%` right operand", expr.span)? {
                    return Ok(None);
                }
                Ok(Some(remainder_values(left, right, expr.span)?))
            }
            ExprKind::Binary { .. } => {
                let value = self.eval_value(expr, env)?;
                match value {
                    Value::Bool(true) => Ok(Some(Value::Bool(true))),
                    Value::Bool(false) => Ok(None),
                    other => Ok(Some(other)),
                }
            }
            _ => {
                let value = self.eval_value(expr, env)?;
                match value {
                    Value::Bool(true) => Ok(Some(Value::Bool(true))),
                    Value::Bool(false) => Ok(None),
                    other => Ok(Some(other)),
                }
            }
        }
    }

    fn eval_unary(&self, op: UnaryOp, value: Value, span: Span) -> Result<Value, VerseError> {
        match op {
            UnaryOp::Positive => positive_value(value, span),
            UnaryOp::Negate => negate_value(value, span),
            UnaryOp::Not => Ok(Value::Bool(!expect_bool(&value, "`not`", span)?)),
        }
    }

    fn eval_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<Value, VerseError> {
        let left_span = left.span;
        let left_value = match self.eval_expr(left, env)? {
            Flow::Value(value) => value,
            Flow::Pending(suspension) => {
                let right = right.clone();
                let env = env.clone();
                return Ok(Value::Suspended(suspension.map(
                    move |interpreter, flow| {
                        let right = right.clone();
                        let env = env.clone();
                        let value_continuation: Rc<ValueContinuation> =
                            Rc::new(move |interpreter, left_value| {
                                interpreter
                                    .eval_binary_after_left(left_value, op, &right, span, &env)
                            });
                        Self::continue_when_value(interpreter, flow, left_span, value_continuation)
                    },
                )));
            }
            flow => return self.flow_value_or_error(flow, left_span),
        };

        match self.eval_binary_after_left(left_value, op, right, span, env)? {
            Flow::Value(value) => Ok(value),
            Flow::Pending(suspension) => Ok(Value::Suspended(suspension)),
            flow => self.flow_value_or_error(flow, span),
        }
    }

    fn eval_binary_after_left(
        &self,
        left: Value,
        op: BinaryOp,
        right: &Expr,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        if op == BinaryOp::And {
            let left = expect_bool(&left, "`and` left operand", span)?;
            if !left {
                return Ok(Flow::Value(Value::Bool(false)));
            }
            return self.eval_binary_bool_right(right, "`and` right operand", span, env);
        }

        if op == BinaryOp::Or {
            let left = expect_bool(&left, "`or` left operand", span)?;
            if left {
                return Ok(Flow::Value(Value::Bool(true)));
            }
            return self.eval_binary_bool_right(right, "`or` right operand", span, env);
        }

        let right_span = right.span;
        let right_value = match self.eval_expr(right, env)? {
            Flow::Value(value) => value,
            Flow::Pending(suspension) => {
                return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let left = value_copy(&left);
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |_, right_value| {
                            let value =
                                eval_binary_values(value_copy(&left), op, right_value, span)?;
                            Ok(Flow::Value(value))
                        });
                    Self::continue_when_value(interpreter, flow, right_span, value_continuation)
                })));
            }
            flow => return self.flow_value_or_error(flow, right_span).map(Flow::Value),
        };

        eval_binary_values(left, op, right_value, span).map(Flow::Value)
    }

    fn eval_binary_bool_right(
        &self,
        right: &Expr,
        context: &'static str,
        span: Span,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let right_span = right.span;
        match self.eval_expr(right, env)? {
            Flow::Value(value) => Ok(Flow::Value(Value::Bool(expect_bool(
                &value, context, span,
            )?))),
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let value_continuation: Rc<ValueContinuation> = Rc::new(move |_, value| {
                        Ok(Flow::Value(Value::Bool(expect_bool(
                            &value, context, span,
                        )?)))
                    });
                    Self::continue_when_value(interpreter, flow, right_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, right_span).map(Flow::Value),
        }
    }

    fn eval_struct_definition(
        &self,
        name: Option<&str>,
        computes: bool,
        fields: &[StructField],
        env: &Env,
    ) -> Result<Value, VerseError> {
        let mut runtime_fields = Vec::with_capacity(fields.len());
        for field in fields {
            let default = field
                .default
                .as_ref()
                .map(|default| self.eval_data_member_default(name, &field.name, default, env))
                .transpose()?;
            runtime_fields.push(RuntimeStructField {
                name: field.name.clone(),
                default,
            });
        }

        Ok(Value::StructType {
            name: name.unwrap_or("<anonymous>").to_string(),
            computes,
            fields: runtime_fields,
        })
    }

    fn eval_data_member_default(
        &self,
        owner: Option<&str>,
        field_name: &str,
        default: &Expr,
        env: &Env,
    ) -> Result<Value, VerseError> {
        let depth = self.data_member_default_depth.get();
        self.data_member_default_depth.set(depth + 1);
        if let Some(owner) = owner {
            self.data_member_default_stack
                .borrow_mut()
                .push(RuntimeDataMemberDefaultContext {
                    aggregate_name: owner.to_string(),
                    field_name: field_name.to_string(),
                });
        }
        let result = self.eval_value(default, env);
        if owner.is_some() {
            self.data_member_default_stack.borrow_mut().pop();
        }
        self.data_member_default_depth.set(depth);
        result
    }

    fn ensure_data_member_default_callee_allowed(
        &self,
        callee: &Value,
        span: Span,
    ) -> Result<(), VerseError> {
        if self.data_member_default_depth.get() == 0 {
            return Ok(());
        }

        match callee {
            Value::Function { effects, .. } | Value::BoundMethod { effects, .. } => {
                self.ensure_data_member_default_effects_allowed(effects, span)
            }
            Value::Overload(_) | Value::Tuple(_) => Ok(()),
            Value::NativeFunction { .. }
            | Value::NativeResultMethod { .. }
            | Value::NativeEventMethod { .. }
            | Value::NativeTaskMethod { .. }
            | Value::NativeModifierMethod { .. }
            | Value::NativeCancelMethod { .. }
            | Value::NativeSubscribableMethod { .. }
            | Value::NativeSubscriptionCancelMethod { .. } => {
                Err(data_member_default_call_error(span))
            }
            _ => Ok(()),
        }
    }

    fn ensure_data_member_default_effects_allowed(
        &self,
        effects: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        if self.data_member_default_depth.get() == 0 {
            return Ok(());
        }

        if has_runtime_effect(effects, "converges")
            && !has_runtime_effect(effects, "suspends")
            && !has_runtime_effect(effects, "decides")
        {
            Ok(())
        } else {
            Err(data_member_default_call_error(span))
        }
    }

    fn eval_class_definition(
        &self,
        name: Option<&str>,
        parts: RuntimeClassDefinitionParts<'_>,
        env: &Env,
    ) -> Result<Value, VerseError> {
        let RuntimeClassDefinitionParts {
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
            return Err(VerseError::runtime_at(
                "class cannot be both `abstract` and `concrete`",
                fields
                    .first()
                    .map_or_else(|| Span::new(0, 0, 1, 1), |field| field.span),
            ));
        }

        let (
            mut runtime_fields,
            mut runtime_methods,
            mut runtime_blocks,
            super_type,
            base_unique,
            base_castable,
            base_name,
            mut implemented_interfaces,
        ) = if let Some(base) = base {
            if let TypeName::Applied { name, args } = &base.name
                && name == "modifier"
                && args.len() == 1
            {
                (
                    Vec::new(),
                    vec![runtime_modifier_method(args[0].clone())],
                    Vec::new(),
                    None,
                    false,
                    false,
                    None,
                    vec![render_runtime_parametric_type_name(name, args)],
                )
            } else {
                let base_value = self.eval_type_annotation_value(base, env)?;
                match &base_value {
                    Value::ClassType {
                        name,
                        unique,
                        castable,
                        final_class,
                        fields,
                        methods,
                        blocks,
                        interfaces,
                        ..
                    } => {
                        if *final_class {
                            return Err(VerseError::runtime_at(
                                format!("class `{name}` is `final` and cannot be inherited"),
                                base.span,
                            ));
                        }
                        (
                            fields.clone(),
                            methods.clone(),
                            blocks.clone(),
                            Some(Box::new(base_value.clone())),
                            *unique,
                            *castable,
                            Some(name.clone()),
                            interfaces.clone(),
                        )
                    }
                    Value::InterfaceType {
                        name,
                        parents,
                        fields,
                        methods,
                    } => (
                        fields.clone(),
                        methods.clone(),
                        Vec::new(),
                        None,
                        false,
                        false,
                        None,
                        {
                            let mut interfaces = parents.clone();
                            interfaces.push(name.clone());
                            interfaces
                        },
                    ),
                    other => {
                        return Err(VerseError::runtime_at(
                            format!("class parent must be a class or interface, got `{other}`"),
                            base.span,
                        ));
                    }
                }
            }
        } else {
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                false,
                None,
                Vec::new(),
            )
        };
        let has_final_super = class_has_specifier(specifiers, "final_super");
        let class_span = runtime_class_definition_diagnostic_span(base, fields, methods, blocks);
        if has_final_super && base_name.as_deref() != Some("component") {
            return Err(VerseError::runtime_at(
                "class with `<final_super>` must directly inherit from `component`",
                class_span,
            ));
        }
        if base_name.as_deref() == Some("component") && !has_final_super {
            return Err(VerseError::runtime_at(
                "class directly inheriting from `component` must specify `<final_super>`",
                class_span,
            ));
        }
        for interface in interfaces {
            implemented_interfaces
                .extend(self.eval_interface_names_from_annotation(interface, env)?);
            for interface_field in self.eval_interface_fields_from_annotation(interface, env)? {
                if runtime_fields
                    .iter()
                    .any(|field| field.name == interface_field.name)
                {
                    continue;
                }
                runtime_fields.push(interface_field);
            }
            for interface_method in self.eval_interface_methods_from_annotation(interface, env)? {
                if runtime_methods
                    .iter()
                    .any(|method| runtime_class_methods_conflict(method, &interface_method))
                {
                    continue;
                }
                if let Some(index) = runtime_methods.iter().position(|method| {
                    method.qualifier.is_none()
                        && runtime_class_method_signatures_conflict(method, &interface_method)
                }) {
                    runtime_methods[index].qualifier = interface_method.qualifier.clone();
                    continue;
                }
                runtime_methods.push(interface_method);
            }
        }
        implemented_interfaces = dedupe_runtime_strings(implemented_interfaces);
        let unique = class_has_specifier(specifiers, "unique") || base_unique;
        let castable = class_has_specifier(specifiers, "castable") || base_castable;

        for field in fields {
            let override_field = field_has_specifier(&field.specifiers, "override");
            let inherited_field_index = runtime_fields
                .iter()
                .position(|candidate| candidate.name == field.name);
            if let Some(index) = inherited_field_index {
                if !override_field {
                    return Err(VerseError::runtime_at(
                        format!("duplicate inherited class field `{}`", field.name),
                        field.span,
                    ));
                }
                if runtime_fields[index].final_member {
                    return Err(VerseError::runtime_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, runtime_fields[index].name
                        ),
                        field.span,
                    ));
                }
                if field_has_specifier(&field.specifiers, "final") && field.default.is_none() {
                    return Err(VerseError::runtime_at(
                        format!("final field `{}` must have a default value", field.name),
                        field.span,
                    ));
                }
                let default = field
                    .default
                    .as_ref()
                    .map(|default| self.eval_data_member_default(name, &field.name, default, env))
                    .transpose()?;
                runtime_fields[index] = RuntimeClassField {
                    name: field.name.clone(),
                    mutable: field.mutable,
                    final_member: field_has_specifier(&field.specifiers, "final"),
                    access: runtime_access_level_from_specifiers(&field.specifiers),
                    owner: None,
                    default,
                };
                continue;
            } else if override_field {
                return Err(VerseError::runtime_at(
                    format!(
                        "field `{}` does not override an inherited field",
                        field.name
                    ),
                    field.span,
                ));
            }
            if runtime_methods
                .iter()
                .any(|candidate| candidate.name == field.name)
            {
                return Err(VerseError::runtime_at(
                    format!("duplicate inherited class member `{}`", field.name),
                    field.span,
                ));
            }
            if field_has_specifier(&field.specifiers, "final") && field.default.is_none() {
                return Err(VerseError::runtime_at(
                    format!("final field `{}` must have a default value", field.name),
                    field.span,
                ));
            }
            let default = field
                .default
                .as_ref()
                .map(|default| self.eval_data_member_default(name, &field.name, default, env))
                .transpose()?;
            runtime_fields.push(RuntimeClassField {
                name: field.name.clone(),
                mutable: field.mutable,
                final_member: field_has_specifier(&field.specifiers, "final"),
                access: runtime_access_level_from_specifiers(&field.specifiers),
                owner: None,
                default,
            });
        }

        if !class_has_specifier(specifiers, "abstract") {
            ensure_runtime_interface_required_fields_initializable(
                "<anonymous>",
                specifiers,
                &runtime_fields,
                fields.first().map_or_else(
                    || {
                        base.as_ref()
                            .map_or(Span::new(0, 0, 1, 1), |base| base.span)
                    },
                    |field| field.span,
                ),
            )?;
        }

        if class_has_specifier(specifiers, "concrete") {
            for field in &runtime_fields {
                if field.default.is_none() {
                    return Err(VerseError::runtime_at(
                        format!(
                            "concrete class field `{}` must have a default value",
                            field.name
                        ),
                        fields.first().map_or_else(
                            || {
                                base.as_ref()
                                    .map_or_else(|| Span::new(0, 0, 1, 1), |base| base.span)
                            },
                            |field| field.span,
                        ),
                    ));
                }
            }
        }

        let local_extension_methods = Rc::new(
            extension_methods
                .iter()
                .map(|extension| {
                    let Some(body) = extension.method.body.as_ref() else {
                        return Err(VerseError::runtime_at(
                            "extension method requires a body",
                            extension.span,
                        ));
                    };
                    Ok(RuntimeExtensionMethod {
                        name: extension.method.name.clone(),
                        module_name: None,
                        receiver: extension.receiver.clone(),
                        params: extension.method.params.clone(),
                        effects: extension.method.effects.clone(),
                        body: Box::new(body.clone()),
                        closure: env.clone(),
                    })
                })
                .collect::<Result<Vec<_>, VerseError>>()?,
        );

        for method in methods {
            if runtime_fields
                .iter()
                .any(|candidate| candidate.name == method.name)
            {
                return Err(VerseError::runtime_at(
                    format!("duplicate class member `{}`", method.name),
                    method.span,
                ));
            }

            if method.body.is_none() && method.return_type.is_none() {
                return Err(VerseError::runtime_at(
                    format!(
                        "abstract class method `{}` requires an explicit return type",
                        method.name
                    ),
                    method.span,
                ));
            }
            if method.body.is_none() && method_has_specifier(method, "final") {
                return Err(VerseError::runtime_at(
                    format!("abstract class method `{}` cannot be `final`", method.name),
                    method.span,
                ));
            }
            if method.body.is_some() && method_has_specifier(method, "abstract") {
                return Err(VerseError::runtime_at(
                    format!("abstract class method `{}` cannot have a body", method.name),
                    method.span,
                ));
            }

            let runtime_method = RuntimeClassMethod {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                final_member: method_has_specifier(method, "final"),
                params: method.params.clone(),
                effects: method.effects.clone(),
                body: method.body.clone().map(Box::new),
                closure: env.clone(),
                super_type: super_type.clone(),
                extension_methods: local_extension_methods.clone(),
            };

            let matching_index =
                runtime_inherited_method_override_index(&runtime_methods, &runtime_method);
            let duplicate_index =
                runtime_inherited_method_duplicate_index(&runtime_methods, &runtime_method);
            if method_has_specifier(method, "override") {
                let Some(index) = matching_index else {
                    return Err(VerseError::runtime_at(
                        format!(
                            "method `{}` does not override an inherited method",
                            method.name
                        ),
                        method.span,
                    ));
                };
                if runtime_methods[index].final_member {
                    return Err(VerseError::runtime_at(
                        format!(
                            "method `{}` overrides final inherited method `{}`",
                            method.name, runtime_methods[index].name
                        ),
                        method.span,
                    ));
                }
                let mut replacement = runtime_method;
                if replacement.qualifier.is_none() {
                    replacement.qualifier = runtime_methods[index].qualifier.clone();
                }
                runtime_methods[index] = replacement;
            } else {
                if duplicate_index.is_some() {
                    return Err(VerseError::runtime_at(
                        format!("duplicate inherited class method `{}`", method.name),
                        method.span,
                    ));
                }
                runtime_methods.push(runtime_method);
            }
        }

        if !class_has_specifier(specifiers, "abstract")
            && let Some(method) = runtime_methods.iter().find(|method| method.body.is_none())
        {
            let span = methods
                .iter()
                .find(|candidate| candidate.name == method.name)
                .map(|method| method.span)
                .or_else(|| fields.first().map(|field| field.span))
                .or_else(|| base.as_ref().map(|base| base.span))
                .unwrap_or_else(|| Span::new(0, 0, 1, 1));
            return Err(VerseError::runtime_at(
                format!(
                    "class must be `abstract` or implement method `{}`",
                    method.name
                ),
                span,
            ));
        }

        let mut local_blocks = blocks
            .iter()
            .map(|block| RuntimeClassBlock {
                body: Box::new(block.body.clone()),
                closure: env.clone(),
                super_type: super_type.clone(),
                extension_methods: local_extension_methods.clone(),
            })
            .collect::<Vec<_>>();
        local_blocks.extend(runtime_blocks);
        runtime_blocks = local_blocks;

        Ok(Value::ClassType {
            name: name.unwrap_or("<anonymous>").to_string(),
            base: base_name,
            interfaces: implemented_interfaces,
            unique,
            abstract_class: class_has_specifier(specifiers, "abstract"),
            epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
            final_class: class_has_specifier(specifiers, "final"),
            concrete: class_has_specifier(specifiers, "concrete"),
            castable,
            fields: runtime_fields,
            methods: runtime_methods,
            blocks: runtime_blocks,
        })
    }

    fn eval_interface_definition(
        &self,
        name: Option<&str>,
        parents: &[TypeAnnotation],
        fields: &[StructField],
        methods: &[ClassMethod],
        env: &Env,
    ) -> Result<Value, VerseError> {
        let mut parent_names = Vec::new();
        let mut runtime_fields = Vec::new();
        let mut runtime_methods = Vec::new();
        for parent in parents {
            match self.eval_type_annotation_value(parent, env)? {
                Value::InterfaceType {
                    name,
                    parents,
                    fields,
                    methods,
                } => {
                    parent_names.extend(parents);
                    parent_names.push(name);
                    runtime_fields.extend(fields);
                    runtime_methods.extend(methods);
                }
                other => {
                    return Err(VerseError::runtime_at(
                        format!("interface parent must be an interface, got `{other}`"),
                        parent.span,
                    ));
                }
            }
        }

        for field in fields {
            let default = field
                .default
                .as_ref()
                .map(|default| self.eval_data_member_default(name, &field.name, default, env))
                .transpose()?;
            if let Some(index) = runtime_fields
                .iter()
                .position(|existing: &RuntimeClassField| existing.name == field.name)
            {
                if runtime_fields[index].final_member {
                    return Err(VerseError::runtime_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, runtime_fields[index].name
                        ),
                        field.span,
                    ));
                }
                runtime_fields[index] = RuntimeClassField {
                    name: field.name.clone(),
                    mutable: field.mutable,
                    final_member: field_has_specifier(&field.specifiers, "final"),
                    access: runtime_access_level_from_specifiers(&field.specifiers),
                    owner: None,
                    default,
                };
                continue;
            }
            runtime_fields.push(RuntimeClassField {
                name: field.name.clone(),
                mutable: field.mutable,
                final_member: field_has_specifier(&field.specifiers, "final"),
                access: runtime_access_level_from_specifiers(&field.specifiers),
                owner: None,
                default,
            });
        }

        for method in methods {
            let runtime_method = RuntimeClassMethod {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                final_member: false,
                params: method.params.clone(),
                effects: method.effects.clone(),
                body: method.body.clone().map(Box::new),
                closure: env.clone(),
                super_type: None,
                extension_methods: Rc::new(Vec::new()),
            };
            if let Some(index) = runtime_methods
                .iter()
                .position(|existing| runtime_class_methods_conflict(existing, &runtime_method))
            {
                runtime_methods[index] = runtime_method;
                continue;
            }
            runtime_methods.push(runtime_method);
        }

        Ok(Value::InterfaceType {
            name: name.unwrap_or("<anonymous>").to_string(),
            parents: dedupe_runtime_strings(parent_names),
            fields: runtime_fields,
            methods: runtime_methods,
        })
    }

    fn eval_interface_methods_from_annotation(
        &self,
        interface: &TypeAnnotation,
        env: &Env,
    ) -> Result<Vec<RuntimeClassMethod>, VerseError> {
        if let TypeName::Applied { name, args } = &interface.name
            && name == "modifier"
            && args.len() == 1
        {
            return Ok(vec![runtime_modifier_method(args[0].clone())]);
        }
        match self.eval_type_annotation_value(interface, env)? {
            Value::InterfaceType { methods, .. } => Ok(methods),
            other => Err(VerseError::runtime_at(
                format!("additional class parent must be an interface, got `{other}`"),
                interface.span,
            )),
        }
    }

    fn eval_interface_fields_from_annotation(
        &self,
        interface: &TypeAnnotation,
        env: &Env,
    ) -> Result<Vec<RuntimeClassField>, VerseError> {
        if let TypeName::Applied { name, args } = &interface.name
            && name == "modifier"
            && args.len() == 1
        {
            return Ok(Vec::new());
        }
        match self.eval_type_annotation_value(interface, env)? {
            Value::InterfaceType { fields, .. } => Ok(fields),
            other => Err(VerseError::runtime_at(
                format!("additional class parent must be an interface, got `{other}`"),
                interface.span,
            )),
        }
    }

    fn eval_interface_names_from_annotation(
        &self,
        interface: &TypeAnnotation,
        env: &Env,
    ) -> Result<Vec<String>, VerseError> {
        if let TypeName::Applied { name, args } = &interface.name
            && name == "modifier"
            && args.len() == 1
        {
            return Ok(vec![render_runtime_parametric_type_name(name, args)]);
        }
        match self.eval_type_annotation_value(interface, env)? {
            Value::InterfaceType { name, parents, .. } => {
                let mut names = parents;
                names.push(name);
                Ok(names)
            }
            other => Err(VerseError::runtime_at(
                format!("additional class parent must be an interface, got `{other}`"),
                interface.span,
            )),
        }
    }

    fn eval_module_definition(&self, statements: &[Stmt], env: &Env) -> Result<Value, VerseError> {
        let module_env = Env::child(env);
        match self.eval_statements(statements, &module_env)? {
            Flow::Value(_) => Ok(Value::Module {
                name: "<anonymous>".to_string(),
                env: module_env,
            }),
            Flow::Return(_) => Err(VerseError::runtime("`return` used outside a function")),
            Flow::Break => Err(VerseError::runtime("`break` used outside a loop")),
            Flow::Pending(_) => Err(VerseError::runtime(
                "module definition is suspended and cannot complete without async scheduling support",
            )),
        }
    }

    fn eval_archetype(
        &self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
        env: &Env,
    ) -> Result<Value, VerseError> {
        if is_official_event_archetype_callee(callee) {
            return self.eval_event_archetype(callee, entries, env);
        }

        match self.eval_value(callee, env)? {
            Value::StructType {
                name,
                computes,
                fields: template_fields,
            } => {
                self.ensure_data_member_default_archetype_not_recursive(&name, callee.span)?;
                let fields = self.eval_archetype_entries(entries, env, None)?;
                self.eval_struct_archetype(name, computes, template_fields, &fields, callee.span)
            }
            Value::ClassType {
                name,
                unique,
                abstract_class,
                epic_internal_class,
                fields: template_fields,
                methods,
                blocks,
                ..
            } => {
                self.ensure_data_member_default_archetype_not_recursive(&name, callee.span)?;
                if abstract_class {
                    return Err(VerseError::runtime_at(
                        format!("abstract class `{name}` cannot be instantiated"),
                        callee.span,
                    ));
                }
                if epic_internal_class {
                    return Err(VerseError::runtime_at(
                        format!("epic_internal class `{name}` cannot be instantiated"),
                        callee.span,
                    ));
                }
                let fields = self.eval_archetype_entries(entries, env, Some(&name))?;
                self.eval_class_archetype(
                    name,
                    unique,
                    template_fields,
                    RuntimeClassMembers { methods, blocks },
                    &fields,
                    callee.span,
                )
            }
            other => Err(VerseError::runtime_at(
                format!("cannot construct value from `{other}`"),
                callee.span,
            )),
        }
    }

    fn ensure_data_member_default_archetype_not_recursive(
        &self,
        aggregate_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let context = self
            .data_member_default_stack
            .borrow()
            .iter()
            .rev()
            .find(|context| runtime_names_match(&context.aggregate_name, aggregate_name))
            .cloned();

        if let Some(context) = context {
            return Err(VerseError::runtime_at(
                format!(
                    "field default `{}.{}` recursively constructs `{aggregate_name}`",
                    context.aggregate_name, context.field_name
                ),
                span,
            ));
        }

        Ok(())
    }

    fn eval_event_archetype(
        &self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
        env: &Env,
    ) -> Result<Value, VerseError> {
        if !entries.is_empty() {
            return Err(VerseError::runtime_at(
                "`event` archetype construction expects an empty body",
                callee.span,
            ));
        }

        let args = official_event_archetype_args(callee)
            .expect("event archetype callee should have been recognized");
        if args.len() > 1 {
            return Err(VerseError::runtime_at(
                format!(
                    "parametric type `event` expected 0 or 1 type arguments, got {}",
                    args.len()
                ),
                callee.span,
            ));
        }

        let payload = match args {
            [] => None,
            [CallArg::Positional(expr)] => {
                let type_name = expr_to_type_name(expr)?;
                Some(env.resolve_type_name(&type_name))
            }
            [arg] => {
                return Err(VerseError::runtime_at(
                    "`event` type arguments do not accept named arguments",
                    call_arg_expr(arg).span,
                ));
            }
            _ => unreachable!("event arity was checked"),
        };

        Ok(event_value(payload))
    }

    fn eval_parametric_type_call(
        &self,
        callee: Value,
        args: &[CallArg],
        env: &Env,
        span: Span,
    ) -> Result<Value, VerseError> {
        let Value::ParametricType {
            name,
            params,
            body,
            closure,
        } = callee
        else {
            unreachable!("caller should pass a parametric type value");
        };
        if args.len() != params.len() {
            return Err(VerseError::runtime_at(
                format!(
                    "parametric type `{name}` expected {} type arguments, got {}",
                    params.len(),
                    args.len()
                ),
                span,
            ));
        }

        let mut type_args = Vec::with_capacity(args.len());
        for arg in args {
            let CallArg::Positional(expr) = arg else {
                return Err(VerseError::runtime_at(
                    "parametric type arguments do not accept named arguments",
                    call_arg_expr(arg).span,
                ));
            };
            type_args.push(expr_to_type_name(expr)?);
        }

        self.eval_parametric_type_instance(
            RuntimeParametricTypeTemplate {
                name: &name,
                params: &params,
                body: &body,
                closure: &closure,
            },
            &type_args,
            env,
            span,
        )
    }

    fn eval_parametric_type_instance(
        &self,
        template: RuntimeParametricTypeTemplate<'_>,
        args: &[TypeName],
        env: &Env,
        span: Span,
    ) -> Result<Value, VerseError> {
        if args.len() != template.params.len() {
            return Err(VerseError::runtime_at(
                format!(
                    "parametric type `{}` expected {} type arguments, got {}",
                    template.name,
                    template.params.len(),
                    args.len()
                ),
                span,
            ));
        }

        let instance_env = Env::child(template.closure);
        let mut resolved_args = Vec::with_capacity(args.len());
        for (param, arg) in template.params.iter().zip(args) {
            let resolved = env.resolve_type_name(arg);
            instance_env.define_type_alias(&param.name, resolved.clone());
            resolved_args.push(resolved);
        }

        let value = self.eval_value(template.body, &instance_env)?;
        let instance_name = render_runtime_parametric_type_name(template.name, &resolved_args);
        match value {
            Value::StructType {
                computes, fields, ..
            } => Ok(Value::StructType {
                name: instance_name,
                computes,
                fields,
            }),
            Value::ClassType {
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                fields,
                methods,
                blocks,
                ..
            } => Ok(Value::ClassType {
                name: instance_name,
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                fields,
                methods,
                blocks,
            }),
            Value::InterfaceType {
                parents,
                fields,
                methods,
                ..
            } => Ok(Value::InterfaceType {
                name: instance_name.clone(),
                parents,
                fields: qualify_runtime_interface_fields(&instance_name, fields),
                methods: qualify_runtime_interface_methods(&instance_name, methods),
            }),
            other => Err(VerseError::runtime_at(
                format!("parametric type `{}` produced `{other}`", template.name),
                span,
            )),
        }
    }

    fn eval_type_annotation_value(
        &self,
        annotation: &TypeAnnotation,
        env: &Env,
    ) -> Result<Value, VerseError> {
        match &annotation.name {
            TypeName::Named(name) => {
                if runtime_builtin_class_base_name(name) {
                    runtime_builtin_class_type(name).ok_or_else(|| {
                        VerseError::runtime_at(format!("unknown type `{name}`"), annotation.span)
                    })
                } else {
                    env.get_qualified_path(name).ok_or_else(|| {
                        VerseError::runtime_at(format!("unknown type `{name}`"), annotation.span)
                    })
                }
            }
            TypeName::Applied { name, args } => {
                let Some(value) = env.get_qualified_path(name) else {
                    return Err(VerseError::runtime_at(
                        format!("unknown parametric type `{name}`"),
                        annotation.span,
                    ));
                };
                let Value::ParametricType {
                    name,
                    params,
                    body,
                    closure,
                } = value
                else {
                    return Err(VerseError::runtime_at(
                        format!("`{name}` is not a parametric type"),
                        annotation.span,
                    ));
                };
                self.eval_parametric_type_instance(
                    RuntimeParametricTypeTemplate {
                        name: &name,
                        params: &params,
                        body: &body,
                        closure: &closure,
                    },
                    args,
                    env,
                    annotation.span,
                )
            }
            _ => Err(VerseError::runtime_at(
                "expected named or parametric type annotation",
                annotation.span,
            )),
        }
    }

    fn eval_archetype_entries(
        &self,
        entries: &[ArchetypeEntry],
        env: &Env,
        target_class: Option<&str>,
    ) -> Result<Vec<EvaluatedArchetypeField>, VerseError> {
        let entry_env = Env::child(env);
        let mut fields = Vec::new();
        let mut constructor_delegation_seen = false;
        for entry in entries {
            match entry {
                ArchetypeEntry::Let(binding) => {
                    let value = self.eval_value(&binding.expr, &entry_env)?;
                    entry_env.define(&binding.name, value, false);
                }
                ArchetypeEntry::Block(body) => {
                    self.eval_value(body, &entry_env)?;
                }
                ArchetypeEntry::Field(field) => {
                    if target_class.is_some() && constructor_delegation_seen {
                        return Err(VerseError::runtime_at(
                            format!(
                                "field initializer `{}` cannot appear after constructor delegation",
                                field.name
                            ),
                            field.span,
                        ));
                    }

                    let value = self.eval_value(&field.expr, &entry_env)?;
                    fields.push(EvaluatedArchetypeField {
                        name: field.name.clone(),
                        value,
                        span: field.span,
                        explicit: true,
                    });
                }
                ArchetypeEntry::ConstructorCall(call) => {
                    let Some(target_class) = target_class else {
                        return Err(VerseError::runtime_at(
                            "constructor delegation is only valid in class archetypes",
                            call.span,
                        ));
                    };
                    let value = self.eval_archetype_constructor_call(call, &entry_env)?;
                    let Value::ClassInstance {
                        class_name,
                        fields: delegated_fields,
                        ..
                    } = value
                    else {
                        return Err(VerseError::runtime_at(
                            "`<constructor>` delegation must return a class instance",
                            call.span,
                        ));
                    };

                    let same_class = runtime_names_match(&class_name, target_class);
                    if !same_class && !runtime_class_is_subtype(target_class, &class_name, env) {
                        return Err(VerseError::runtime_at(
                            format!(
                                "constructor `{}` returns `{class_name}`, which is not `{target_class}` or a superclass",
                                call.name
                            ),
                            call.span,
                        ));
                    }

                    if same_class {
                        fields.clear();
                    }
                    constructor_delegation_seen = true;

                    for field in delegated_fields.borrow().iter() {
                        fields.push(EvaluatedArchetypeField {
                            name: field.name.clone(),
                            value: value_copy(&field.value),
                            span: call.span,
                            explicit: false,
                        });
                    }
                }
            }
        }
        Ok(fields)
    }

    fn eval_archetype_constructor_call(
        &self,
        call: &ArchetypeConstructorCall,
        env: &Env,
    ) -> Result<Value, VerseError> {
        let callee = env.get(&call.name).ok_or_else(|| {
            VerseError::runtime_at(format!("undefined constructor `{}`", call.name), call.span)
        })?;
        let args = self.eval_call_args_ready(&call.args, env)?;

        if let Value::Function { effects, .. } = &callee {
            if !has_runtime_effect(effects, "constructor") {
                return Err(VerseError::runtime_at(
                    format!(
                        "`{}<constructor>` expects a constructor function",
                        call.name
                    ),
                    call.span,
                ));
            }
            if has_runtime_effect(effects, "decides") {
                return Err(VerseError::runtime_at(
                    "functions with `<decides>` must be called with `[]`",
                    call.span,
                ));
            }
            return self.call(callee, args, call.span);
        }

        match callee {
            Value::Overload(overloads) => {
                let constructors = overloads
                    .into_iter()
                    .filter(|overload| {
                        matches!(
                            overload,
                            Value::Function { effects, .. }
                                if has_runtime_effect(effects, "constructor")
                        )
                    })
                    .collect::<Vec<_>>();
                if constructors.is_empty() {
                    return Err(VerseError::runtime_at(
                        format!(
                            "`{}<constructor>` expects a constructor function",
                            call.name
                        ),
                        call.span,
                    ));
                }
                let Some(overload) = self.select_overload(&constructors, &args, false) else {
                    return Err(VerseError::runtime_at(
                        "no overload matches constructor delegation call",
                        call.span,
                    ));
                };
                self.call(overload, args, call.span)
            }
            other => Err(VerseError::runtime_at(
                format!("`{}<constructor>` cannot call `{other}`", call.name),
                call.span,
            )),
        }
    }

    fn eval_struct_archetype(
        &self,
        struct_name: String,
        computes: bool,
        template_fields: Vec<RuntimeStructField>,
        fields: &[EvaluatedArchetypeField],
        span: Span,
    ) -> Result<Value, VerseError> {
        let mut values = Vec::with_capacity(template_fields.len());
        for template in &template_fields {
            let value = fields
                .iter()
                .rev()
                .find(|field| field.name == template.name)
                .map(|field| value_copy(&field.value))
                .or_else(|| template.default.as_ref().map(value_copy))
                .ok_or_else(|| {
                    VerseError::runtime_at(
                        format!(
                            "missing required field `{}` for `{struct_name}`",
                            template.name
                        ),
                        span,
                    )
                })?;
            values.push((template.name.clone(), value));
        }

        for field in fields {
            if !template_fields
                .iter()
                .any(|template| template.name == field.name)
            {
                return Err(VerseError::runtime_at(
                    format!("struct `{struct_name}` has no field `{}`", field.name),
                    field.span,
                ));
            }
        }

        Ok(Value::StructInstance {
            struct_name,
            computes,
            fields: values,
        })
    }

    fn eval_class_archetype(
        &self,
        class_name: String,
        unique: bool,
        template_fields: Vec<RuntimeClassField>,
        members: RuntimeClassMembers,
        fields: &[EvaluatedArchetypeField],
        span: Span,
    ) -> Result<Value, VerseError> {
        let mut values = Vec::with_capacity(template_fields.len());
        for template in &template_fields {
            let value = if let Some(field) = fields
                .iter()
                .rev()
                .find(|field| field.name == template.name)
            {
                if template.final_member && field.explicit {
                    return Err(VerseError::runtime_at(
                        format!(
                            "final field `{}` cannot be overridden by an archetype",
                            field.name
                        ),
                        field.span,
                    ));
                }
                value_copy(&field.value)
            } else {
                template.default.as_ref().map(value_copy).ok_or_else(|| {
                    VerseError::runtime_at(
                        format!(
                            "missing required field `{}` for `{class_name}`",
                            template.name
                        ),
                        span,
                    )
                })?
            };
            values.push(RuntimeClassInstanceField {
                name: template.name.clone(),
                mutable: template.mutable,
                value,
            });
        }

        for field in fields {
            if !template_fields
                .iter()
                .any(|template| template.name == field.name)
            {
                return Err(VerseError::runtime_at(
                    format!("class `{class_name}` has no field `{}`", field.name),
                    field.span,
                ));
            }
        }

        let fields = Rc::new(RefCell::new(values));
        let methods = Rc::new(members.methods);
        for block in &members.blocks {
            self.eval_class_block(&class_name, unique, &fields, &methods, block)?;
        }

        Ok(Value::ClassInstance {
            class_name,
            unique,
            fields,
            methods,
        })
    }

    fn eval_class_block(
        &self,
        class_name: &str,
        unique: bool,
        fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: &Rc<Vec<RuntimeClassMethod>>,
        block: &RuntimeClassBlock,
    ) -> Result<(), VerseError> {
        let field_env = Env::child(&block.closure);
        let initial_fields = fields.borrow().clone();
        field_env.define(
            "Self",
            Value::ClassInstance {
                class_name: class_name.to_string(),
                unique,
                fields: fields.clone(),
                methods: methods.clone(),
            },
            false,
        );
        if let Some(super_type) = &block.super_type {
            field_env.define("super", super_type.as_ref().clone(), false);
        }
        for field in &initial_fields {
            field_env.define(&field.name, field.value.clone(), field.mutable);
        }
        self.bind_instance_methods(&field_env, class_name, unique, fields, methods);
        self.bind_instance_extension_methods(&field_env, &block.extension_methods);

        let flow = self.eval_expr(&block.body, &field_env);
        self.sync_instance_fields(fields, &field_env, &initial_fields);

        match flow? {
            Flow::Value(_) => Ok(()),
            Flow::Return(_) => Err(VerseError::runtime_at(
                "`return` escaped class block",
                block.body.span,
            )),
            Flow::Break => Err(VerseError::runtime_at(
                "`break` escaped class block",
                block.body.span,
            )),
            Flow::Pending(_) => Err(VerseError::runtime_at(
                "class block cannot suspend",
                block.body.span,
            )),
        }
    }

    fn bind_instance_methods(
        &self,
        env: &Env,
        class_name: &str,
        unique: bool,
        fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: &Rc<Vec<RuntimeClassMethod>>,
    ) {
        let mut grouped: Vec<(String, Vec<Value>)> = Vec::new();
        for method in methods.iter() {
            let Some(body) = method.body.clone() else {
                continue;
            };
            let value = Value::BoundMethod {
                name: method.name.clone(),
                params: method.params.clone(),
                effects: method.effects.clone(),
                body,
                closure: method.closure.clone(),
                super_type: method.super_type.clone(),
                extension_methods: method.extension_methods.clone(),
                class_name: class_name.to_string(),
                unique,
                fields: fields.clone(),
                methods: methods.clone(),
            };
            if let Some((_, overloads)) = grouped.iter_mut().find(|(name, _)| name == &method.name)
            {
                overloads.push(value);
            } else {
                grouped.push((method.name.clone(), vec![value]));
            }
        }

        for (name, overloads) in grouped {
            let value = match overloads.as_slice() {
                [single] => single.clone(),
                _ => Value::Overload(overloads),
            };
            env.define(&name, value, false);
        }
    }

    fn bound_method_value(
        &self,
        method: &RuntimeClassMethod,
        class_name: String,
        unique: bool,
        fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: Rc<Vec<RuntimeClassMethod>>,
        span: Span,
    ) -> Result<Value, VerseError> {
        let Some(body) = method.body.clone() else {
            return Err(VerseError::runtime_at(
                format!("abstract method `{}` cannot be called", method.name),
                span,
            ));
        };
        Ok(Value::BoundMethod {
            name: method.name.clone(),
            params: method.params.clone(),
            effects: method.effects.clone(),
            body,
            closure: method.closure.clone(),
            super_type: method.super_type.clone(),
            extension_methods: method.extension_methods.clone(),
            class_name,
            unique,
            fields,
            methods,
        })
    }

    fn bound_method_group_value<'a>(
        &self,
        candidate_methods: impl IntoIterator<Item = &'a RuntimeClassMethod>,
        class_name: String,
        unique: bool,
        fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        methods: Rc<Vec<RuntimeClassMethod>>,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        let mut overloads = Vec::new();
        for method in candidate_methods {
            overloads.push(self.bound_method_value(
                method,
                class_name.clone(),
                unique,
                fields.clone(),
                methods.clone(),
                span,
            )?);
        }
        Ok(match overloads.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Value::Overload(overloads)),
        })
    }

    fn bind_instance_extension_methods(
        &self,
        env: &Env,
        extension_methods: &[RuntimeExtensionMethod],
    ) {
        for method in extension_methods {
            let mut bound = method.clone();
            bound.closure = env.clone();
            env.define_extension_method(bound.name.clone(), bound);
        }
    }

    fn eval_if_expression(
        &self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let condition_env = Env::child(env);
        match self.eval_failure_condition_transactional_maybe_pending(condition, &condition_env)? {
            FailureEval::Ready(Some(_)) => self.eval_expr(then_branch, &condition_env),
            FailureEval::Ready(None) => {
                if let Some(else_branch) = else_branch {
                    self.eval_expr(else_branch, env)
                } else {
                    Ok(Flow::Value(Value::None))
                }
            }
            FailureEval::Pending(suspension) => {
                let condition_env = condition_env.clone();
                let outer_env = env.clone();
                let then_branch = then_branch.clone();
                let else_branch = else_branch.cloned();
                let condition_span = condition.span;
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let condition_env = condition_env.clone();
                    let outer_env = outer_env.clone();
                    let then_branch = then_branch.clone();
                    let else_branch = else_branch.clone();
                    let continuation: Rc<FailureContinuation> =
                        Rc::new(move |interpreter, result| {
                            if result.is_some() {
                                interpreter.eval_expr(&then_branch, &condition_env)
                            } else if let Some(else_branch) = &else_branch {
                                interpreter.eval_expr(else_branch, &outer_env)
                            } else {
                                Ok(Flow::Value(Value::None))
                            }
                        });
                    Self::continue_when_failure_result(
                        interpreter,
                        flow,
                        condition_span,
                        continuation,
                    )
                })))
            }
        }
    }

    fn eval_option_expression(&self, value: Option<&Expr>, env: &Env) -> Result<Flow, VerseError> {
        let Some(value) = value else {
            return Ok(Flow::Value(Value::Option(None)));
        };

        match self.eval_failure_expr_transactional_maybe_pending(value, env)? {
            FailureEval::Ready(Some(value)) => {
                Ok(Flow::Value(Value::Option(Some(Box::new(value)))))
            }
            FailureEval::Ready(None) => Ok(Flow::Value(Value::Option(None))),
            FailureEval::Pending(suspension) => {
                let value_span = value.span;
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let continuation: Rc<FailureContinuation> = Rc::new(move |_, result| {
                        Ok(Flow::Value(Value::Option(result.map(Box::new))))
                    });
                    Self::continue_when_failure_result(interpreter, flow, value_span, continuation)
                })))
            }
        }
    }

    fn eval_case(
        &self,
        subject: &Expr,
        arms: &[CaseArm],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        let subject_span = subject.span;
        let subject_value = match self.eval_expr(subject, env)? {
            Flow::Value(value) => value,
            Flow::Pending(suspension) => {
                let arms = arms.to_vec();
                let env = env.clone();
                return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let arms = arms.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, subject_value| {
                            interpreter.eval_case_after_subject(subject_value, &arms, &env, span)
                        });
                    Self::continue_when_value(interpreter, flow, subject_span, value_continuation)
                })));
            }
            flow => {
                return self
                    .flow_value_or_error(flow, subject_span)
                    .map(Flow::Value);
            }
        };
        self.eval_case_after_subject(subject_value, arms, env, span)
    }

    fn eval_case_after_subject(
        &self,
        subject_value: Value,
        arms: &[CaseArm],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        for arm in arms {
            let matched = match &arm.pattern {
                CasePattern::Wildcard { .. } => true,
                CasePattern::Expr(pattern) => self.eval_value(pattern, env)? == subject_value,
            };
            if matched {
                return self.eval_expr(&arm.expr, env);
            }
        }

        Err(VerseError::runtime_at(
            "case expression had no matching arm",
            span,
        ))
    }

    fn eval_failure_case(
        &self,
        subject: &Expr,
        arms: &[CaseArm],
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let subject_value = if is_failable_condition_expr(subject) {
            let Some(value) = self.eval_failure_expr(subject, env)? else {
                return Ok(None);
            };
            value
        } else {
            self.eval_value(subject, env)?
        };

        for arm in arms {
            let matched = match &arm.pattern {
                CasePattern::Wildcard { .. } => true,
                CasePattern::Expr(pattern) => self.eval_value(pattern, env)? == subject_value,
            };
            if matched {
                return self.eval_failure_expr(&arm.expr, env);
            }
        }

        Ok(None)
    }

    fn eval_bracket_call(
        &self,
        callee: &Expr,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        if let ExprKind::Member { object, name } = &callee.kind {
            return self.eval_bracket_member_call(object, name, args, env, span, callee.span);
        }

        let callee_span = callee.span;
        match self.eval_expr(callee, env)? {
            Flow::Value(callee_value) => {
                self.eval_bracket_after_callee(callee_value, args, env, span)
            }
            Flow::Pending(suspension) => {
                let args = args.to_vec();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let args = args.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, callee_value| {
                            interpreter.eval_bracket_after_callee(callee_value, &args, &env, span)
                        });
                    Self::continue_when_value(interpreter, flow, callee_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, callee_span).map(Flow::Value),
        }
    }

    fn eval_bracket_member_call(
        &self,
        object: &Expr,
        name: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Flow, VerseError> {
        let object_span = object.span;
        match self.eval_expr(object, env)? {
            Flow::Value(object) => {
                self.eval_bracket_member_after_object(object, name, args, env, span, callee_span)
            }
            Flow::Pending(suspension) => {
                let name = name.to_string();
                let args = args.to_vec();
                let env = env.clone();
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    let name = name.clone();
                    let args = args.clone();
                    let env = env.clone();
                    let value_continuation: Rc<ValueContinuation> =
                        Rc::new(move |interpreter, object| {
                            interpreter.eval_bracket_member_after_object(
                                object,
                                &name,
                                &args,
                                &env,
                                span,
                                callee_span,
                            )
                        });
                    Self::continue_when_value(interpreter, flow, object_span, value_continuation)
                })))
            }
            flow => self.flow_value_or_error(flow, object_span).map(Flow::Value),
        }
    }

    fn eval_bracket_member_after_object(
        &self,
        object: Value,
        name: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Flow, VerseError> {
        let object = value_copy(&object);
        let name = name.to_string();
        let eval_env = env.clone();
        let closure_env = env.clone();
        let continuation: Rc<ValuesContinuation> = Rc::new(move |interpreter, values| {
            interpreter
                .eval_bracket_member_values(
                    value_copy(&object),
                    &name,
                    values,
                    &closure_env,
                    span,
                    callee_span,
                )
                .map(flow_from_value)
        });
        self.eval_values_then(args, 0, &eval_env, Vec::new(), continuation)
    }

    fn eval_bracket_member_values(
        &self,
        object: Value,
        name: &str,
        values: Vec<Value>,
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Value, VerseError> {
        match object {
            Value::Array(items) => {
                match eval_array_method(name, items.borrow().as_slice(), values.clone(), span) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        let receiver = Value::Array(items.clone());
                        if let Some(callee_value) = self.extension_method_value(receiver, name, env)
                        {
                            self.call(callee_value, values_to_call_values(values, span), span)
                        } else {
                            Err(error)
                        }
                    }
                }
            }
            Value::String(text) => {
                match eval_string_array_method(name, &text, values.clone(), span) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        let receiver = Value::String(text);
                        if let Some(callee_value) = self.extension_method_value(receiver, name, env)
                        {
                            self.call(callee_value, values_to_call_values(values, span), span)
                        } else {
                            Err(error)
                        }
                    }
                }
            }
            receiver if runtime_number(&receiver).is_some() => {
                match eval_number_method(name, receiver.clone(), values.clone(), span) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        if let Some(callee_value) =
                            self.extension_method_value(receiver.clone(), name, env)
                        {
                            self.call(callee_value, values_to_call_values(values, span), span)
                        } else {
                            Err(error)
                        }
                    }
                }
            }
            other @ (Value::StructInstance { .. }
            | Value::ClassInstance { .. }
            | Value::Result { .. }) => {
                let callee_value = self.member_value(other, name, env, callee_span)?;
                self.call(callee_value, values_to_call_values(values, span), span)
            }
            Value::Module {
                name: module_name,
                env: module_env,
            } => {
                let Some(callee_value) = module_env.get_local(name) else {
                    return Err(VerseError::runtime_at(
                        format!("module `{module_name}` has no member `{name}`"),
                        callee_span,
                    ));
                };
                let qualified_name = format!("{module_name}.{name}");
                self.eval_bracket_call_values(
                    qualify_runtime_named_value(callee_value, &qualified_name),
                    values_to_call_values(values, span),
                    env,
                    span,
                )
            }
            Value::ClassifiableSubset(items) if is_classifiable_subset_method_name(name) => {
                eval_classifiable_subset_method(
                    name,
                    items.borrow().as_slice(),
                    values.as_slice(),
                    span,
                )?
                .into_value(name, span)
            }
            Value::External if is_classifiable_subset_method_name(name) => {
                eval_classifiable_subset_method(name, &[], values.as_slice(), span)?
                    .into_value(name, span)
            }
            other => {
                if let Some(callee_value) = self.extension_method_value(other.clone(), name, env) {
                    self.call(callee_value, values_to_call_values(values, span), span)
                } else {
                    Err(VerseError::runtime_at(
                        format!("value `{other}` has no bracket method `{name}`"),
                        callee_span,
                    ))
                }
            }
        }
    }

    fn eval_bracket_after_callee(
        &self,
        callee_value: Value,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Flow, VerseError> {
        self.ensure_data_member_default_callee_allowed(&callee_value, span)?;
        let callee_value = value_copy(&callee_value);
        let eval_env = env.clone();
        let closure_env = env.clone();
        let continuation: Rc<ValuesContinuation> = Rc::new(move |interpreter, values| {
            interpreter
                .eval_bracket_call_values(
                    value_copy(&callee_value),
                    values_to_call_values(values, span),
                    &closure_env,
                    span,
                )
                .map(flow_from_value)
        });
        self.eval_values_then(args, 0, &eval_env, Vec::new(), continuation)
    }

    fn eval_bracket_call_values(
        &self,
        callee_value: Value,
        values: Vec<CallValue>,
        env: &Env,
        span: Span,
    ) -> Result<Value, VerseError> {
        match callee_value {
            value @ (Value::Array(_) | Value::Map(_) | Value::String(_)) => {
                if values.len() != 1 {
                    return Err(VerseError::runtime_at(
                        format!("`[]` lookup expected 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let index = values.into_iter().next().unwrap().value;
                self.index_value(value, index, span)
            }
            value @ (Value::Function { .. }
            | Value::BoundMethod { .. }
            | Value::NativeFunction { .. }
            | Value::NativeResultMethod { .. }
            | Value::NativeEventMethod { .. }
            | Value::NativeSubscribableMethod { .. }
            | Value::NativeTaskMethod { .. }
            | Value::NativeModifierMethod { .. }
            | Value::NativeCancelMethod { .. }
            | Value::NativeSubscriptionCancelMethod { .. }) => self.call(value, values, span),
            Value::Overload(overloads) => self.call_overload(overloads, values, true, span),
            Value::ClassType { name, .. } => self
                .eval_class_cast(&name, values, env, span)?
                .ok_or_else(|| {
                    VerseError::runtime_at(format!("type cast to `{name}` failed"), span)
                }),
            other => Err(VerseError::runtime_at(
                format!("cannot use `[]` with value `{other}`"),
                span,
            )),
        }
    }

    fn eval_failure_bracket_call(
        &self,
        callee: &Expr,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        if let ExprKind::Member { object, name } = &callee.kind {
            let object = if is_failable_condition_expr(object) {
                let Some(value) = self.eval_failure_expr(object, env)? else {
                    return Ok(None);
                };
                value
            } else {
                self.eval_value(object, env)?
            };
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(self.eval_value(arg, env)?);
            }

            return match object {
                Value::Array(items) => {
                    match eval_array_method_failable(
                        name,
                        items.borrow().as_slice(),
                        values.clone(),
                        span,
                    ) {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            let receiver = Value::Array(items.clone());
                            if let Some(callee_value) =
                                self.extension_method_value(receiver, name, env)
                            {
                                let call_values = values
                                    .into_iter()
                                    .map(|value| CallValue {
                                        name: None,
                                        optional: false,
                                        value,
                                        span,
                                    })
                                    .collect();
                                self.call_failure(callee_value, call_values, span)
                            } else {
                                Err(error)
                            }
                        }
                    }
                }
                Value::String(text) => {
                    match eval_string_array_method_failable(name, &text, values.clone(), span) {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            let receiver = Value::String(text);
                            if let Some(callee_value) =
                                self.extension_method_value(receiver, name, env)
                            {
                                let call_values = values
                                    .into_iter()
                                    .map(|value| CallValue {
                                        name: None,
                                        optional: false,
                                        value,
                                        span,
                                    })
                                    .collect();
                                self.call_failure(callee_value, call_values, span)
                            } else {
                                Err(error)
                            }
                        }
                    }
                }
                receiver if runtime_number(&receiver).is_some() => {
                    match eval_number_method_failable(name, receiver.clone(), values.clone(), span)
                    {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            if let Some(callee_value) =
                                self.extension_method_value(receiver.clone(), name, env)
                            {
                                let call_values = values
                                    .into_iter()
                                    .map(|value| CallValue {
                                        name: None,
                                        optional: false,
                                        value,
                                        span,
                                    })
                                    .collect();
                                self.call_failure(callee_value, call_values, span)
                            } else {
                                Err(error)
                            }
                        }
                    }
                }
                other @ (Value::StructInstance { .. }
                | Value::ClassInstance { .. }
                | Value::Result { .. }) => {
                    let callee_value = self.member_value(other, name, env, callee.span)?;
                    let call_values = values
                        .into_iter()
                        .map(|value| CallValue {
                            name: None,
                            optional: false,
                            value,
                            span,
                        })
                        .collect();
                    self.call_failure(callee_value, call_values, span)
                }
                Value::Module {
                    name: module_name,
                    env: module_env,
                } => {
                    let Some(callee_value) = module_env.get_local(name) else {
                        return Err(VerseError::runtime_at(
                            format!("module `{module_name}` has no member `{name}`"),
                            callee.span,
                        ));
                    };
                    let qualified_name = format!("{module_name}.{name}");
                    let call_values = values
                        .into_iter()
                        .map(|value| CallValue {
                            name: None,
                            optional: false,
                            value,
                            span,
                        })
                        .collect();
                    match qualify_runtime_named_value(callee_value, &qualified_name) {
                        Value::ClassType { name, .. } => {
                            self.eval_class_cast(&name, call_values, env, span)
                        }
                        other => self
                            .eval_bracket_call_values(other, call_values, env, span)
                            .map(Some),
                    }
                }
                Value::ClassifiableSubset(items) if is_classifiable_subset_method_name(name) => {
                    match eval_classifiable_subset_method(
                        name,
                        items.borrow().as_slice(),
                        values.as_slice(),
                        span,
                    )? {
                        NativeResult::Value(value) => Ok(Some(value)),
                        NativeResult::Failure(_) => Ok(None),
                    }
                }
                Value::External if is_classifiable_subset_method_name(name) => {
                    match eval_classifiable_subset_method(name, &[], values.as_slice(), span)? {
                        NativeResult::Value(value) => Ok(Some(value)),
                        NativeResult::Failure(_) => Ok(None),
                    }
                }
                other => {
                    if let Some(callee_value) =
                        self.extension_method_value(other.clone(), name, env)
                    {
                        let call_values = values
                            .into_iter()
                            .map(|value| CallValue {
                                name: None,
                                optional: false,
                                value,
                                span,
                            })
                            .collect();
                        self.call_failure(callee_value, call_values, span)
                    } else {
                        Err(VerseError::runtime_at(
                            format!("value `{other}` has no bracket method `{name}`"),
                            callee.span,
                        ))
                    }
                }
            };
        }

        let callee_value = if is_failable_condition_expr(callee) {
            let Some(value) = self.eval_failure_expr(callee, env)? else {
                return Ok(None);
            };
            value
        } else {
            self.eval_value(callee, env)?
        };
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(CallValue {
                name: None,
                optional: false,
                value: self.eval_value(arg, env)?,
                span: arg.span,
            });
        }

        match callee_value {
            value @ (Value::Array(_) | Value::Map(_) | Value::String(_)) => {
                if values.len() != 1 {
                    return Err(VerseError::runtime_at(
                        format!("`[]` lookup expected 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let index = values.into_iter().next().unwrap().value;
                self.index_value_failable(value, index, span)
            }
            value @ (Value::Function { .. }
            | Value::Overload(_)
            | Value::BoundMethod { .. }
            | Value::NativeFunction { .. }
            | Value::NativeResultMethod { .. }
            | Value::NativeEventMethod { .. }
            | Value::NativeSubscribableMethod { .. }
            | Value::NativeTaskMethod { .. }
            | Value::NativeModifierMethod { .. }
            | Value::NativeCancelMethod { .. }
            | Value::NativeSubscriptionCancelMethod { .. }) => {
                self.call_failure(value, values, span)
            }
            Value::ClassType { name, .. } => self.eval_class_cast(&name, values, env, span),
            other => Err(VerseError::runtime_at(
                format!("cannot use `[]` with value `{other}`"),
                span,
            )),
        }
    }

    fn eval_failure_bracket_call_maybe_pending(
        &self,
        callee: &Expr,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        if let ExprKind::Member { object, name } = &callee.kind {
            if let Some(result) = self.eval_failure_member_bracket_call_maybe_pending(
                object,
                name,
                args,
                env,
                span,
                callee.span,
            )? {
                return Ok(result);
            }
            return self
                .eval_failure_bracket_call(callee, args, env, span)
                .map(FailureEval::Ready);
        }

        let callee_span = callee.span;
        if is_failable_condition_expr(callee) {
            return match self.eval_failure_expr_maybe_pending(callee, env)? {
                FailureEval::Ready(Some(callee_value)) => {
                    self.eval_failure_bracket_args_maybe_pending(callee_value, args, env, span)
                }
                FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
                FailureEval::Pending(suspension) => {
                    let args = args.to_vec();
                    let env = env.clone();
                    Ok(FailureEval::Pending(suspension.map(
                        move |interpreter, flow| {
                            let args = args.clone();
                            let env = env.clone();
                            let continuation: Rc<FailureContinuation> =
                                Rc::new(move |interpreter, result| {
                                    let Some(callee_value) = result else {
                                        return Ok(failure_result_flow(None));
                                    };
                                    failure_eval_to_flow(
                                        interpreter.eval_failure_bracket_args_maybe_pending(
                                            callee_value,
                                            &args,
                                            &env,
                                            span,
                                        )?,
                                    )
                                });
                            Self::continue_when_failure_result(
                                interpreter,
                                flow,
                                callee_span,
                                continuation,
                            )
                        },
                    )))
                }
            };
        }

        match self.eval_expr(callee, env)? {
            Flow::Value(callee_value) => {
                self.eval_failure_bracket_args_maybe_pending(callee_value, args, env, span)
            }
            Flow::Pending(suspension) => {
                let args = args.to_vec();
                let env = env.clone();
                Ok(FailureEval::Pending(suspension.map(
                    move |interpreter, flow| {
                        let args = args.clone();
                        let env = env.clone();
                        let value_continuation: Rc<ValueContinuation> =
                            Rc::new(move |interpreter, callee_value| {
                                failure_eval_to_flow(
                                    interpreter.eval_failure_bracket_args_maybe_pending(
                                        callee_value,
                                        &args,
                                        &env,
                                        span,
                                    )?,
                                )
                            });
                        Self::continue_when_value(
                            interpreter,
                            flow,
                            callee_span,
                            value_continuation,
                        )
                    },
                )))
            }
            flow => self
                .flow_value_or_error(flow, callee_span)
                .map(|value| FailureEval::Ready(Some(value))),
        }
    }

    fn eval_failure_member_bracket_call_maybe_pending(
        &self,
        object: &Expr,
        name: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<Option<FailureEval>, VerseError> {
        let object_span = object.span;
        if is_failable_condition_expr(object) {
            return match self.eval_failure_expr_maybe_pending(object, env)? {
                FailureEval::Ready(Some(object_value)) => self
                    .eval_failure_member_bracket_after_object_maybe_pending(
                        object_value,
                        name,
                        args,
                        env,
                        span,
                        callee_span,
                    )
                    .map(Some),
                FailureEval::Ready(None) => Ok(Some(FailureEval::Ready(None))),
                FailureEval::Pending(suspension) => {
                    let name = name.to_string();
                    let args = args.to_vec();
                    let env = env.clone();
                    Ok(Some(FailureEval::Pending(suspension.map(
                        move |interpreter, flow| {
                            let name = name.clone();
                            let args = args.clone();
                            let env = env.clone();
                            let continuation: Rc<FailureContinuation> =
                                Rc::new(move |interpreter, result| {
                                    let Some(object_value) = result else {
                                        return Ok(failure_result_flow(None));
                                    };
                                    failure_eval_to_flow(
                                        interpreter
                                            .eval_failure_member_bracket_after_object_maybe_pending(
                                                object_value,
                                                &name,
                                                &args,
                                                &env,
                                                span,
                                                callee_span,
                                            )?,
                                    )
                                });
                            Self::continue_when_failure_result(
                                interpreter,
                                flow,
                                object_span,
                                continuation,
                            )
                        },
                    ))))
                }
            };
        }

        match self.eval_expr(object, env)? {
            Flow::Value(object_value) => {
                if runtime_failure_member_bracket_supports_pending(&object_value, name) {
                    self.eval_failure_member_bracket_after_object_maybe_pending(
                        object_value,
                        name,
                        args,
                        env,
                        span,
                        callee_span,
                    )
                    .map(Some)
                } else {
                    Ok(None)
                }
            }
            Flow::Pending(suspension) => {
                let name = name.to_string();
                let args = args.to_vec();
                let env = env.clone();
                Ok(Some(FailureEval::Pending(suspension.map(
                    move |interpreter, flow| {
                        let name = name.clone();
                        let args = args.clone();
                        let env = env.clone();
                        let value_continuation: Rc<ValueContinuation> =
                            Rc::new(move |interpreter, object_value| {
                                failure_eval_to_flow(
                                    interpreter
                                        .eval_failure_member_bracket_after_object_maybe_pending(
                                            object_value,
                                            &name,
                                            &args,
                                            &env,
                                            span,
                                            callee_span,
                                        )?,
                                )
                            });
                        Self::continue_when_value(
                            interpreter,
                            flow,
                            object_span,
                            value_continuation,
                        )
                    },
                ))))
            }
            flow => self
                .flow_value_or_error(flow, object_span)
                .map(|value| Some(FailureEval::Ready(Some(value)))),
        }
    }

    fn eval_failure_member_bracket_after_object_maybe_pending(
        &self,
        object_value: Value,
        name: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
        callee_span: Span,
    ) -> Result<FailureEval, VerseError> {
        match object_value {
            other @ (Value::StructInstance { .. }
            | Value::ClassInstance { .. }
            | Value::Result { .. }) => {
                let callee_value = self.member_value(other, name, env, callee_span)?;
                self.eval_failure_bracket_args_maybe_pending(callee_value, args, env, span)
            }
            Value::Module {
                name: module_name,
                env: module_env,
            } => {
                let Some(callee_value) = module_env.get_local(name) else {
                    return Err(VerseError::runtime_at(
                        format!("module `{module_name}` has no member `{name}`"),
                        callee_span,
                    ));
                };
                let qualified_name = format!("{module_name}.{name}");
                self.eval_failure_bracket_args_maybe_pending(
                    qualify_runtime_named_value(callee_value, &qualified_name),
                    args,
                    env,
                    span,
                )
            }
            other => Err(VerseError::runtime_at(
                format!("value `{other}` has no bracket method `{name}`"),
                callee_span,
            )),
        }
    }

    fn eval_failure_bracket_args_maybe_pending(
        &self,
        callee_value: Value,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        let callee_value = value_copy(&callee_value);
        let eval_env = env.clone();
        let closure_env = env.clone();
        let continuation: Rc<ValuesContinuation> = Rc::new(move |interpreter, values| {
            failure_eval_to_flow(interpreter.eval_failure_bracket_values_maybe_pending(
                value_copy(&callee_value),
                values_to_call_values(values, span),
                &closure_env,
                span,
            )?)
        });
        match self.eval_values_then(args, 0, &eval_env, Vec::new(), continuation)? {
            Flow::Value(Value::Result { succeeded, value }) => {
                if succeeded {
                    Ok(FailureEval::Ready(Some(*value)))
                } else {
                    Ok(FailureEval::Ready(None))
                }
            }
            Flow::Pending(suspension) => Ok(FailureEval::Pending(suspension)),
            flow => self
                .flow_value_or_error(flow, span)
                .map(|value| FailureEval::Ready(Some(value))),
        }
    }

    fn eval_failure_bracket_values_maybe_pending(
        &self,
        callee_value: Value,
        values: Vec<CallValue>,
        env: &Env,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        match callee_value {
            value @ (Value::Array(_) | Value::Map(_) | Value::String(_)) => {
                if values.len() != 1 {
                    return Err(VerseError::runtime_at(
                        format!("`[]` lookup expected 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let index = values.into_iter().next().unwrap().value;
                self.index_value_failable(value, index, span)
                    .map(FailureEval::Ready)
            }
            value @ (Value::Function { .. }
            | Value::Overload(_)
            | Value::BoundMethod { .. }
            | Value::NativeFunction { .. }
            | Value::NativeResultMethod { .. }
            | Value::NativeEventMethod { .. }
            | Value::NativeSubscribableMethod { .. }
            | Value::NativeTaskMethod { .. }
            | Value::NativeModifierMethod { .. }
            | Value::NativeCancelMethod { .. }
            | Value::NativeSubscriptionCancelMethod { .. }) => {
                self.call_failure_maybe_pending(value, values, span)
            }
            Value::ClassType { name, .. } => self
                .eval_class_cast(&name, values, env, span)
                .map(FailureEval::Ready),
            other => Err(VerseError::runtime_at(
                format!("cannot use `[]` with value `{other}`"),
                span,
            )),
        }
    }

    fn eval_class_cast(
        &self,
        target: &str,
        args: Vec<CallValue>,
        env: &Env,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        if args.len() != 1 {
            return Err(VerseError::runtime_at(
                format!("class cast expected 1 argument, got {}", args.len()),
                span,
            ));
        }

        let value = args.into_iter().next().expect("arity checked").value;
        let Value::ClassInstance { class_name, .. } = &value else {
            return Err(VerseError::runtime_at(
                format!("class cast expected class instance, got `{value}`"),
                span,
            ));
        };

        if self.class_instance_conforms_to(class_name, target, env, span)? {
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn class_instance_conforms_to(
        &self,
        actual: &str,
        target: &str,
        env: &Env,
        span: Span,
    ) -> Result<bool, VerseError> {
        let mut current = Some(actual.to_string());
        while let Some(name) = current {
            if runtime_names_match(&name, target) {
                return Ok(true);
            }
            current = match runtime_named_type_value(&name, env) {
                Some(Value::ClassType { base, .. }) => base,
                Some(other) => {
                    return Err(VerseError::runtime_at(
                        format!("`{name}` is not a class type: `{other}`"),
                        span,
                    ));
                }
                None => return Ok(false),
            };
        }
        Ok(false)
    }

    fn eval_compound_assignment_value(
        op: AssignOp,
        left: Value,
        right: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        match op {
            AssignOp::Assign => Ok(right),
            AssignOp::AddAssign => add_values(left, right, span),
            AssignOp::SubAssign => subtract_values(left, right, span),
            AssignOp::MulAssign => multiply_values(left, right, span),
            AssignOp::DivAssign => divide_values(left, right, span),
        }
    }

    fn eval_set_expression(
        &self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        env: &Env,
    ) -> Result<Flow, VerseError> {
        let value = match op {
            AssignOp::Assign => {
                let expr_span = expr.span;
                match self.eval_expr(expr, env)? {
                    Flow::Value(value) => value,
                    Flow::Pending(suspension) => {
                        let target = target.clone();
                        let env = env.clone();
                        return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                            let target = target.clone();
                            let env = env.clone();
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(move |interpreter, value| {
                                    interpreter.assign_target(&target, value, &env)?;
                                    Ok(Flow::Value(Value::None))
                                });
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                expr_span,
                                value_continuation,
                            )
                        })));
                    }
                    flow => return self.flow_value_or_error(flow, expr_span).map(Flow::Value),
                }
            }
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                let span = target.span.through(expr.span);
                let left = self.read_assignment_target(target, env)?;
                match self.eval_expr(expr, env)? {
                    Flow::Value(right) => {
                        Self::eval_compound_assignment_value(op, left, right, span)?
                    }
                    Flow::Pending(suspension) => {
                        let target = target.clone();
                        let env = env.clone();
                        let left = value_copy(&left);
                        let expr_span = expr.span;
                        return Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                            let target = target.clone();
                            let env = env.clone();
                            let left = value_copy(&left);
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(move |interpreter, right| {
                                    let value = Self::eval_compound_assignment_value(
                                        op,
                                        value_copy(&left),
                                        right,
                                        span,
                                    )?;
                                    interpreter.assign_target(&target, value, &env)?;
                                    Ok(Flow::Value(Value::None))
                                });
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                expr_span,
                                value_continuation,
                            )
                        })));
                    }
                    flow => return self.flow_value_or_error(flow, expr.span).map(Flow::Value),
                }
            }
        };
        self.assign_target(target, value, env)?;
        Ok(Flow::Value(Value::None))
    }

    fn eval_failure_set_expression(
        &self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let value = match op {
            AssignOp::Assign => {
                let Some(value) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                value
            }
            AssignOp::AddAssign => {
                let Some(left) = self.read_assignment_target_failable(target, env)? else {
                    return Ok(None);
                };
                let Some(right) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                add_values(left, right, target.span.through(expr.span))?
            }
            AssignOp::SubAssign => {
                let span = target.span.through(expr.span);
                let Some(left) = self.read_assignment_target_failable(target, env)? else {
                    return Ok(None);
                };
                let Some(right) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                subtract_values(left, right, span)?
            }
            AssignOp::MulAssign => {
                let span = target.span.through(expr.span);
                let Some(left) = self.read_assignment_target_failable(target, env)? else {
                    return Ok(None);
                };
                let Some(right) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                multiply_values(left, right, span)?
            }
            AssignOp::DivAssign => {
                let span = target.span.through(expr.span);
                let Some(left) = self.read_assignment_target_failable(target, env)? else {
                    return Ok(None);
                };
                let Some(right) = self.eval_failure_expr(expr, env)? else {
                    return Ok(None);
                };
                divide_values(left, right, span)?
            }
        };

        let Some(()) = self.assign_target_failable(target, value, env)? else {
            return Ok(None);
        };
        Ok(Some(Value::None))
    }

    fn eval_failure_set_expression_maybe_pending(
        &self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        match op {
            AssignOp::Assign => match self.eval_failure_expr_maybe_pending(expr, env)? {
                FailureEval::Ready(Some(value)) => {
                    self.finish_failure_set_value(target, value, env)
                }
                FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
                FailureEval::Pending(suspension) => {
                    let target = target.clone();
                    let env = env.clone();
                    let expr_span = expr.span;
                    Ok(FailureEval::Pending(suspension.map(
                        move |interpreter, flow| {
                            let target = target.clone();
                            let env = env.clone();
                            let continuation: Rc<FailureContinuation> =
                                Rc::new(move |interpreter, result| {
                                    let Some(value) = result else {
                                        return Ok(failure_result_flow(None));
                                    };
                                    failure_eval_to_flow(
                                        interpreter
                                            .finish_failure_set_value(&target, value, &env)?,
                                    )
                                });
                            Self::continue_when_failure_result(
                                interpreter,
                                flow,
                                expr_span,
                                continuation,
                            )
                        },
                    )))
                }
            },
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                let span = target.span.through(expr.span);
                let Some(left) = self.read_assignment_target_failable(target, env)? else {
                    return Ok(FailureEval::Ready(None));
                };
                match self.eval_failure_expr_maybe_pending(expr, env)? {
                    FailureEval::Ready(Some(right)) => {
                        let value = Self::eval_compound_assignment_value(op, left, right, span)?;
                        self.finish_failure_set_value(target, value, env)
                    }
                    FailureEval::Ready(None) => Ok(FailureEval::Ready(None)),
                    FailureEval::Pending(suspension) => {
                        let target = target.clone();
                        let env = env.clone();
                        let left = value_copy(&left);
                        let expr_span = expr.span;
                        Ok(FailureEval::Pending(suspension.map(
                            move |interpreter, flow| {
                                let target = target.clone();
                                let env = env.clone();
                                let left = value_copy(&left);
                                let continuation: Rc<FailureContinuation> =
                                    Rc::new(move |interpreter, result| {
                                        let Some(right) = result else {
                                            return Ok(failure_result_flow(None));
                                        };
                                        let value = Self::eval_compound_assignment_value(
                                            op,
                                            value_copy(&left),
                                            right,
                                            span,
                                        )?;
                                        failure_eval_to_flow(
                                            interpreter
                                                .finish_failure_set_value(&target, value, &env)?,
                                        )
                                    });
                                Self::continue_when_failure_result(
                                    interpreter,
                                    flow,
                                    expr_span,
                                    continuation,
                                )
                            },
                        )))
                    }
                }
            }
        }
    }

    fn finish_failure_set_value(
        &self,
        target: &Expr,
        value: Value,
        env: &Env,
    ) -> Result<FailureEval, VerseError> {
        if self.assign_target_failable(target, value, env)?.is_some() {
            Ok(FailureEval::Ready(Some(Value::None)))
        } else {
            Ok(FailureEval::Ready(None))
        }
    }

    fn assign_target(&self, target: &Expr, value: Value, env: &Env) -> Result<(), VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                env.assign(name, value.clone(), target.span)?;
                self.assign_self_field_if_present(env, name, value);
                Ok(())
            }
            ExprKind::Index {
                collection: collection_expr,
                index,
            } => {
                let collection = self.eval_value(collection_expr, env)?;
                let index_value = self.eval_value(index, env)?;
                match collection {
                    Value::Array(items) => {
                        let index = expect_index(&index_value, index.span)?;
                        let mut updated = {
                            let items = items.borrow();
                            if index >= items.len() {
                                return Err(VerseError::runtime_at(
                                    format!(
                                        "array index {index} out of bounds for length {}",
                                        items.len()
                                    ),
                                    target.span,
                                ));
                            }
                            items.iter().map(value_copy).collect::<Vec<_>>()
                        };
                        updated[index] = if matches!(&updated[index], Value::Array(_))
                            && matches!(&value, Value::Tuple(_))
                        {
                            tuple_value_to_array(value)
                        } else {
                            value
                        };
                        self.assign_target(collection_expr, array_value(updated), env)
                    }
                    Value::String(text) => {
                        let updated = replace_string_byte(text, &index_value, value, target.span)?;
                        self.assign_target(collection_expr, Value::String(updated), env)
                    }
                    Value::Map(entries) => {
                        let mut updated = entries
                            .borrow()
                            .iter()
                            .map(|(key, value)| (value_copy(key), value_copy(value)))
                            .collect::<Vec<_>>();
                        upsert_map_entry(&mut updated, index_value, value);
                        self.assign_target(
                            collection_expr,
                            Value::Map(Rc::new(RefCell::new(updated))),
                            env,
                        )
                    }
                    other => Err(VerseError::runtime_at(
                        format!("cannot index value `{other}`"),
                        target.span,
                    )),
                }
            }
            ExprKind::Member {
                object: object_expr,
                name,
            } => {
                let sync_self_binding = matches!(&object_expr.kind, ExprKind::Ident(object_name) if object_name == "Self");
                let object_value = self.eval_value(object_expr, env)?;
                match object_value {
                    Value::StructInstance {
                        struct_name,
                        computes,
                        fields,
                    } => {
                        if !computes {
                            return Err(VerseError::runtime_at(
                                format!(
                                    "struct `{struct_name}` must be `<computes>` to mutate fields"
                                ),
                                target.span,
                            ));
                        }
                        let mut updated = fields
                            .iter()
                            .map(|(field_name, field_value)| {
                                (field_name.clone(), value_copy(field_value))
                            })
                            .collect::<Vec<_>>();
                        let Some((_, field_value)) = updated
                            .iter_mut()
                            .find(|(field_name, _)| field_name == name)
                        else {
                            return Err(VerseError::runtime_at(
                                format!("struct `{struct_name}` has no field `{name}`"),
                                target.span,
                            ));
                        };
                        *field_value = value_copy(&value);
                        self.assign_target(
                            object_expr,
                            Value::StructInstance {
                                struct_name,
                                computes,
                                fields: updated,
                            },
                            env,
                        )
                    }
                    Value::ClassInstance {
                        class_name, fields, ..
                    } => {
                        let mut fields = fields.borrow_mut();
                        let Some(field) = fields.iter_mut().find(|field| field.name == *name)
                        else {
                            return Err(VerseError::runtime_at(
                                format!("class `{class_name}` has no field `{name}`"),
                                target.span,
                            ));
                        };
                        if !field.mutable {
                            return Err(VerseError::runtime_at(
                                format!("cannot assign to immutable field `{name}`"),
                                target.span,
                            ));
                        }
                        field.value = value_copy(&value);
                        if sync_self_binding && env.get(name).is_some() {
                            env.assign(name, value, target.span)?;
                        }
                        Ok(())
                    }
                    other => Err(VerseError::runtime_at(
                        format!("cannot assign to member `{name}` on value `{other}`"),
                        target.span,
                    )),
                }
            }
            _ => Err(VerseError::runtime_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn assign_target_failable(
        &self,
        target: &Expr,
        value: Value,
        env: &Env,
    ) -> Result<Option<()>, VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                env.assign(name, value.clone(), target.span)?;
                self.assign_self_field_if_present(env, name, value);
                Ok(Some(()))
            }
            ExprKind::Index {
                collection: collection_expr,
                index,
            } => {
                let Some(collection) =
                    self.read_assignment_target_failable(collection_expr, env)?
                else {
                    return Ok(None);
                };
                let Some(index_value) = self.eval_assignment_index_failable(index, env)? else {
                    return Ok(None);
                };
                match collection {
                    Value::Array(items) => {
                        let index = expect_index(&index_value, index.span)?;
                        let mut updated = {
                            let items = items.borrow();
                            if index >= items.len() {
                                return Ok(None);
                            }
                            items.iter().map(value_copy).collect::<Vec<_>>()
                        };
                        updated[index] = if matches!(&updated[index], Value::Array(_))
                            && matches!(&value, Value::Tuple(_))
                        {
                            tuple_value_to_array(value)
                        } else {
                            value
                        };
                        self.assign_target_failable(collection_expr, array_value(updated), env)
                    }
                    Value::String(text) => {
                        let Some(updated) =
                            replace_string_byte_failable(text, &index_value, value, target.span)?
                        else {
                            return Ok(None);
                        };
                        self.assign_target_failable(collection_expr, Value::String(updated), env)
                    }
                    Value::Map(entries) => {
                        let mut updated = entries
                            .borrow()
                            .iter()
                            .map(|(key, value)| (value_copy(key), value_copy(value)))
                            .collect::<Vec<_>>();
                        upsert_map_entry(&mut updated, index_value, value);
                        self.assign_target_failable(
                            collection_expr,
                            Value::Map(Rc::new(RefCell::new(updated))),
                            env,
                        )
                    }
                    other => Err(VerseError::runtime_at(
                        format!("cannot index value `{other}`"),
                        target.span,
                    )),
                }
            }
            ExprKind::Member {
                object: object_expr,
                name,
            } => {
                let sync_self_binding = matches!(&object_expr.kind, ExprKind::Ident(object_name) if object_name == "Self");
                let Some(object_value) = self.read_assignment_target_failable(object_expr, env)?
                else {
                    return Ok(None);
                };
                match object_value {
                    Value::StructInstance {
                        struct_name,
                        computes,
                        fields,
                    } => {
                        if !computes {
                            return Err(VerseError::runtime_at(
                                format!(
                                    "struct `{struct_name}` must be `<computes>` to mutate fields"
                                ),
                                target.span,
                            ));
                        }
                        let mut updated = fields
                            .iter()
                            .map(|(field_name, field_value)| {
                                (field_name.clone(), value_copy(field_value))
                            })
                            .collect::<Vec<_>>();
                        let Some((_, field_value)) = updated
                            .iter_mut()
                            .find(|(field_name, _)| field_name == name)
                        else {
                            return Err(VerseError::runtime_at(
                                format!("struct `{struct_name}` has no field `{name}`"),
                                target.span,
                            ));
                        };
                        *field_value = value_copy(&value);
                        self.assign_target_failable(
                            object_expr,
                            Value::StructInstance {
                                struct_name,
                                computes,
                                fields: updated,
                            },
                            env,
                        )
                    }
                    Value::ClassInstance {
                        class_name, fields, ..
                    } => {
                        let mut fields = fields.borrow_mut();
                        let Some(field) = fields.iter_mut().find(|field| field.name == *name)
                        else {
                            return Err(VerseError::runtime_at(
                                format!("class `{class_name}` has no field `{name}`"),
                                target.span,
                            ));
                        };
                        if !field.mutable {
                            return Err(VerseError::runtime_at(
                                format!("cannot assign to immutable field `{name}`"),
                                target.span,
                            ));
                        }
                        field.value = value_copy(&value);
                        if sync_self_binding && env.get(name).is_some() {
                            env.assign(name, value, target.span)?;
                        }
                        Ok(Some(()))
                    }
                    other => Err(VerseError::runtime_at(
                        format!("cannot assign to member `{name}` on value `{other}`"),
                        target.span,
                    )),
                }
            }
            _ => Err(VerseError::runtime_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn read_assignment_target(&self, target: &Expr, env: &Env) -> Result<Value, VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                if name != "Self"
                    && let Some(value) = self.self_field_value(env, name)
                {
                    Ok(value)
                } else {
                    env.get(name).ok_or_else(|| {
                        VerseError::runtime_at(format!("undefined name `{name}`"), target.span)
                    })
                }
            }
            ExprKind::Index { collection, index } => {
                let collection = self.eval_value(collection, env)?;
                let index = self.eval_value(index, env)?;
                self.index_value(collection, index, target.span)
            }
            ExprKind::Member { object, name } => {
                let object = self.eval_value(object, env)?;
                self.member_value(object, name, env, target.span)
            }
            _ => Err(VerseError::runtime_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn read_assignment_target_failable(
        &self,
        target: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                if name != "Self"
                    && let Some(value) = self.self_field_value(env, name)
                {
                    Ok(Some(value))
                } else {
                    env.get(name).map(Some).ok_or_else(|| {
                        VerseError::runtime_at(format!("undefined name `{name}`"), target.span)
                    })
                }
            }
            ExprKind::Index { collection, index } => {
                let Some(collection) = self.read_assignment_target_failable(collection, env)?
                else {
                    return Ok(None);
                };
                let Some(index) = self.eval_assignment_index_failable(index, env)? else {
                    return Ok(None);
                };
                self.index_value_failable(collection, index, target.span)
            }
            ExprKind::Member { object, name } => {
                let Some(object) = self.read_assignment_target_failable(object, env)? else {
                    return Ok(None);
                };
                self.member_value(object, name, env, target.span).map(Some)
            }
            _ => Err(VerseError::runtime_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn eval_assignment_index_failable(
        &self,
        index: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        if is_failable_condition_expr(index) {
            self.eval_failure_expr(index, env)
        } else {
            self.eval_value(index, env).map(Some)
        }
    }

    fn index_value(
        &self,
        collection: Value,
        index: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        match collection {
            Value::Array(items) => {
                let index = expect_index(&index, span)?;
                let items = items.borrow();
                items.get(index).map(value_copy).ok_or_else(|| {
                    VerseError::runtime_at(
                        format!(
                            "array index {index} out of bounds for length {}",
                            items.len()
                        ),
                        span,
                    )
                })
            }
            Value::Map(entries) => {
                let entries = entries.borrow();
                entries
                    .iter()
                    .find_map(|(key, value)| (key == &index).then(|| value_copy(value)))
                    .ok_or_else(|| {
                        VerseError::runtime_at(format!("map key `{index}` not found"), span)
                    })
            }
            Value::String(text) => string_index_value(&text, &index, span),
            other => Err(VerseError::runtime_at(
                format!("cannot index value `{other}`"),
                span,
            )),
        }
    }

    fn index_value_failable(
        &self,
        collection: Value,
        index: Value,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        match collection {
            Value::Array(items) => {
                let index = expect_index(&index, span)?;
                Ok(items.borrow().get(index).map(value_copy))
            }
            Value::Map(entries) => {
                let entries = entries.borrow();
                Ok(entries
                    .iter()
                    .find_map(|(key, value)| (key == &index).then(|| value_copy(value))))
            }
            Value::String(text) => string_index_value_failable(&text, &index, span),
            other => Err(VerseError::runtime_at(
                format!("cannot index value `{other}`"),
                span,
            )),
        }
    }

    fn self_field_value(&self, env: &Env, name: &str) -> Option<Value> {
        let Value::ClassInstance { fields, .. } = env.get("Self")? else {
            return None;
        };
        fields
            .borrow()
            .iter()
            .find(|field| field.name == name)
            .map(|field| value_copy(&field.value))
    }

    fn assign_self_field_if_present(&self, env: &Env, name: &str, value: Value) {
        let Some(Value::ClassInstance { fields, .. }) = env.get("Self") else {
            return;
        };
        let mut fields = fields.borrow_mut();
        if let Some(field) = fields
            .iter_mut()
            .find(|field| field.name == name && field.mutable)
        {
            field.value = value;
        }
    }

    fn eval_for(&self, clauses: &[ForClause], body: &Expr, env: &Env) -> Result<Flow, VerseError> {
        let results = Rc::new(RefCell::new(Vec::new()));
        match self.eval_for_clauses(clauses, 0, body, env, results.clone())? {
            Some(Flow::Pending(suspension)) => {
                Ok(Flow::Pending(suspension.map(move |_, flow| {
                    finish_for_pending_flow(flow, results.clone())
                })))
            }
            Some(signal) => Ok(signal),
            None => Ok(Flow::Value(for_results_value(&results))),
        }
    }

    fn eval_for_clauses(
        &self,
        clauses: &[ForClause],
        index: usize,
        body: &Expr,
        env: &Env,
        results: Rc<RefCell<Vec<Value>>>,
    ) -> Result<Option<Flow>, VerseError> {
        let Some(clause) = clauses.get(index) else {
            return match self.eval_expr(body, env)? {
                Flow::Value(value) => {
                    results.borrow_mut().push(value);
                    Ok(None)
                }
                Flow::Break => Err(VerseError::runtime_at(
                    "`break` is not allowed inside `for`",
                    body.span,
                )),
                Flow::Return(value) => Ok(Some(Flow::Return(value))),
                Flow::Pending(suspension) => {
                    let body_span = body.span;
                    Ok(Some(Flow::Pending(suspension.map(
                        move |interpreter, flow| {
                            let results = results.clone();
                            let value_continuation: Rc<ValueContinuation> =
                                Rc::new(move |_, value| {
                                    results.borrow_mut().push(value);
                                    Ok(Flow::Value(Value::None))
                                });
                            Self::continue_when_value(
                                interpreter,
                                flow,
                                body_span,
                                value_continuation,
                            )
                        },
                    ))))
                }
            };
        };

        let clause_state = ForClauseState {
            clauses: clauses.to_vec(),
            index,
            body: body.clone(),
            env: env.clone(),
            results: results.clone(),
        };

        match clause {
            ForClause::Generator {
                binding, iterable, ..
            } => {
                let iterable_span = iterable.span;
                match self.eval_expr(iterable, env)? {
                    Flow::Value(iterable_value) => self.eval_for_generator_clause(
                        binding,
                        iterable_span,
                        iterable_value,
                        clause_state,
                    ),
                    Flow::Pending(suspension) => {
                        let binding = binding.clone();
                        Ok(Some(Flow::Pending(suspension.map(
                            move |interpreter, flow| {
                                let clause_state = clone_for_clause_state(&clause_state);
                                let binding = binding.clone();
                                let value_continuation: Rc<ValueContinuation> =
                                    Rc::new(move |interpreter, iterable_value| {
                                        for_clause_flow(interpreter.eval_for_generator_clause(
                                            &binding,
                                            iterable_span,
                                            iterable_value,
                                            clone_for_clause_state(&clause_state),
                                        )?)
                                    });
                                Self::continue_when_value(
                                    interpreter,
                                    flow,
                                    iterable_span,
                                    value_continuation,
                                )
                            },
                        ))))
                    }
                    flow => self
                        .flow_value_or_error(flow, iterable_span)
                        .map(|value| Some(Flow::Value(value))),
                }
            }
            ForClause::RangeOrLet { name, expr, .. } => {
                if is_failable_condition_expr(expr) {
                    let expr_span = expr.span;
                    match self.eval_failure_expr_transactional_maybe_pending(expr, env)? {
                        FailureEval::Ready(Some(value)) => {
                            return self.eval_for_range_or_let_clause(
                                name,
                                expr_span,
                                value,
                                clause_state,
                            );
                        }
                        FailureEval::Ready(None) => return Ok(None),
                        FailureEval::Pending(suspension) => {
                            let name = name.clone();
                            return Ok(Some(Flow::Pending(suspension.map(
                                move |interpreter, flow| {
                                    let clause_state = clone_for_clause_state(&clause_state);
                                    let name = name.clone();
                                    let continuation: Rc<FailureContinuation> =
                                        Rc::new(move |interpreter, result| {
                                            let Some(value) = result else {
                                                return Ok(Flow::Value(Value::None));
                                            };
                                            for_clause_flow(
                                                interpreter.eval_for_range_or_let_clause(
                                                    &name,
                                                    expr_span,
                                                    value,
                                                    clone_for_clause_state(&clause_state),
                                                )?,
                                            )
                                        });
                                    Self::continue_when_failure_result(
                                        interpreter,
                                        flow,
                                        expr_span,
                                        continuation,
                                    )
                                },
                            ))));
                        }
                    }
                }

                let expr_span = expr.span;
                let value = match self.eval_expr(expr, env)? {
                    Flow::Value(value) => value,
                    Flow::Pending(suspension) => {
                        let name = name.clone();
                        return Ok(Some(Flow::Pending(suspension.map(
                            move |interpreter, flow| {
                                let clause_state = clone_for_clause_state(&clause_state);
                                let name = name.clone();
                                let continuation_name = name.clone();
                                let value_continuation: Rc<ValueContinuation> =
                                    Rc::new(move |interpreter, value| {
                                        for_clause_flow(interpreter.eval_for_range_or_let_clause(
                                            &continuation_name,
                                            expr_span,
                                            value,
                                            clone_for_clause_state(&clause_state),
                                        )?)
                                    });
                                Self::continue_when_value(
                                    interpreter,
                                    flow,
                                    expr_span,
                                    value_continuation,
                                )
                            },
                        ))));
                    }
                    flow => {
                        return self
                            .flow_value_or_error(flow, expr_span)
                            .map(|value| Some(Flow::Value(value)));
                    }
                };
                self.eval_for_range_or_let_clause(name, expr_span, value, clause_state)
            }
            ForClause::Let { name, expr, .. } => {
                let expr_span = expr.span;
                match self.eval_expr(expr, env)? {
                    Flow::Value(value) => self.eval_for_let_clause(name, value, clause_state),
                    Flow::Pending(suspension) => {
                        let name = name.clone();
                        Ok(Some(Flow::Pending(suspension.map(
                            move |interpreter, flow| {
                                let clause_state = clone_for_clause_state(&clause_state);
                                let name = name.clone();
                                let continuation_name = name.clone();
                                let value_continuation: Rc<ValueContinuation> =
                                    Rc::new(move |interpreter, value| {
                                        for_clause_flow(interpreter.eval_for_let_clause(
                                            &continuation_name,
                                            value,
                                            clone_for_clause_state(&clause_state),
                                        )?)
                                    });
                                Self::continue_when_value(
                                    interpreter,
                                    flow,
                                    expr_span,
                                    value_continuation,
                                )
                            },
                        ))))
                    }
                    flow => self
                        .flow_value_or_error(flow, expr_span)
                        .map(|value| Some(Flow::Value(value))),
                }
            }
            ForClause::Filter(expr) => {
                let filter_span = expr.span;
                match self.eval_failure_expr_transactional_maybe_pending(expr, env)? {
                    FailureEval::Ready(Some(_)) => {
                        self.eval_for_clauses(clauses, index + 1, body, env, results)
                    }
                    FailureEval::Ready(None) => Ok(None),
                    FailureEval::Pending(suspension) => Ok(Some(Flow::Pending(suspension.map(
                        move |interpreter, flow| {
                            let clause_state = clone_for_clause_state(&clause_state);
                            let continuation: Rc<FailureContinuation> =
                                Rc::new(move |interpreter, result| {
                                    if result.is_none() {
                                        return Ok(Flow::Value(Value::None));
                                    }
                                    for_clause_flow(interpreter.eval_for_clauses(
                                        &clause_state.clauses,
                                        clause_state.index + 1,
                                        &clause_state.body,
                                        &clause_state.env,
                                        clause_state.results.clone(),
                                    )?)
                                });
                            Self::continue_when_failure_result(
                                interpreter,
                                flow,
                                filter_span,
                                continuation,
                            )
                        },
                    )))),
                }
            }
        }
    }

    fn eval_for_generator_clause(
        &self,
        binding: &ForBinding,
        iterable_span: Span,
        iterable_value: Value,
        state: ForClauseState,
    ) -> Result<Option<Flow>, VerseError> {
        let bindings = self.iter_bindings(binding, iterable_value, iterable_span)?;
        self.eval_for_iteration_bindings(
            ForIterationState {
                clauses: state.clauses,
                index: state.index,
                body: state.body,
                env: state.env,
                results: state.results,
                bindings,
            },
            0,
        )
    }

    fn eval_for_range_or_let_clause(
        &self,
        name: &str,
        expr_span: Span,
        value: Value,
        state: ForClauseState,
    ) -> Result<Option<Flow>, VerseError> {
        if matches!(value, Value::Range { .. }) {
            let bindings =
                self.iter_bindings(&ForBinding::Value(name.to_string()), value, expr_span)?;
            self.eval_for_iteration_bindings(
                ForIterationState {
                    clauses: state.clauses,
                    index: state.index,
                    body: state.body,
                    env: state.env,
                    results: state.results,
                    bindings,
                },
                0,
            )
        } else {
            self.eval_for_let_clause(name, value, state)
        }
    }

    fn eval_for_let_clause(
        &self,
        name: &str,
        value: Value,
        state: ForClauseState,
    ) -> Result<Option<Flow>, VerseError> {
        let let_env = Env::child(&state.env);
        let_env.define(name, value, false);
        self.eval_for_clauses(
            &state.clauses,
            state.index + 1,
            &state.body,
            &let_env,
            state.results,
        )
    }

    fn eval_for_iteration_bindings(
        &self,
        state: ForIterationState,
        start: usize,
    ) -> Result<Option<Flow>, VerseError> {
        for iteration_index in start..state.bindings.len() {
            let iteration_env = Env::child(&state.env);
            for (name, value) in &state.bindings[iteration_index] {
                iteration_env.define(name, value_copy(value), false);
            }
            if let Some(signal) = self.eval_for_clauses(
                &state.clauses,
                state.index + 1,
                &state.body,
                &iteration_env,
                state.results.clone(),
            )? {
                if let Flow::Pending(suspension) = signal {
                    let state = clone_for_iteration_state(&state);
                    let next_iteration = iteration_index + 1;
                    return Ok(Some(Flow::Pending(suspension.map(
                        move |interpreter, flow| {
                            interpreter.continue_for_after_iteration(
                                flow,
                                clone_for_iteration_state(&state),
                                next_iteration,
                            )
                        },
                    ))));
                }
                return Ok(Some(signal));
            }
        }
        Ok(None)
    }

    fn continue_for_after_iteration(
        &self,
        flow: Flow,
        state: ForIterationState,
        next_iteration: usize,
    ) -> Result<Flow, VerseError> {
        match flow {
            Flow::Value(_) => match self.eval_for_iteration_bindings(state, next_iteration)? {
                Some(flow) => Ok(flow),
                None => Ok(Flow::Value(Value::None)),
            },
            Flow::Pending(suspension) => {
                Ok(Flow::Pending(suspension.map(move |interpreter, flow| {
                    interpreter.continue_for_after_iteration(
                        flow,
                        clone_for_iteration_state(&state),
                        next_iteration,
                    )
                })))
            }
            signal => Ok(signal),
        }
    }

    fn eval_for_failure(
        &self,
        clauses: &[ForClause],
        body: &Expr,
        env: &Env,
    ) -> Result<Option<Value>, VerseError> {
        let mut results = Vec::new();
        if self.eval_for_failure_clauses(clauses, 0, body, env, &mut results)? {
            Ok(Some(Value::Array(Rc::new(RefCell::new(results)))))
        } else {
            Ok(None)
        }
    }

    fn eval_for_failure_clauses(
        &self,
        clauses: &[ForClause],
        index: usize,
        body: &Expr,
        env: &Env,
        results: &mut Vec<Value>,
    ) -> Result<bool, VerseError> {
        let Some(clause) = clauses.get(index) else {
            return {
                let transaction = EnvTransaction::capture(env);
                match self.eval_failure_expr(body, env)? {
                    Some(value) => {
                        results.push(value);
                        Ok(true)
                    }
                    None => {
                        transaction.restore();
                        Ok(false)
                    }
                }
            };
        };

        match clause {
            ForClause::Generator {
                binding, iterable, ..
            } => {
                let iterable_value = self.eval_value(iterable, env)?;
                let bindings = self.iter_bindings(binding, iterable_value, iterable.span)?;
                for iteration in bindings {
                    let iteration_env = Env::child(env);
                    for (name, value) in iteration {
                        iteration_env.define(name, value, false);
                    }
                    if !self.eval_for_failure_clauses(
                        clauses,
                        index + 1,
                        body,
                        &iteration_env,
                        results,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ForClause::RangeOrLet { name, expr, .. } => {
                let value = if is_failable_condition_expr(expr) {
                    let Some(value) = self.eval_failure_expr_transactional(expr, env)? else {
                        return Ok(true);
                    };
                    value
                } else {
                    self.eval_value(expr, env)?
                };
                if matches!(value, Value::Range { .. }) {
                    let bindings =
                        self.iter_bindings(&ForBinding::Value(name.clone()), value, expr.span)?;
                    for iteration in bindings {
                        let iteration_env = Env::child(env);
                        for (name, value) in iteration {
                            iteration_env.define(name, value, false);
                        }
                        if !self.eval_for_failure_clauses(
                            clauses,
                            index + 1,
                            body,
                            &iteration_env,
                            results,
                        )? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                } else {
                    let let_env = Env::child(env);
                    let_env.define(name, value, false);
                    self.eval_for_failure_clauses(clauses, index + 1, body, &let_env, results)
                }
            }
            ForClause::Let { name, expr, .. } => {
                let value = self.eval_value(expr, env)?;
                let let_env = Env::child(env);
                let_env.define(name, value, false);
                self.eval_for_failure_clauses(clauses, index + 1, body, &let_env, results)
            }
            ForClause::Filter(expr) => {
                if self
                    .eval_failure_condition_transactional(expr, env)?
                    .is_some()
                {
                    self.eval_for_failure_clauses(clauses, index + 1, body, env, results)
                } else {
                    Ok(true)
                }
            }
        }
    }

    fn iter_values(&self, iterable: Value, span: Span) -> Result<Vec<Value>, VerseError> {
        match iterable {
            Value::Range { start, end } => {
                let values = if start <= end {
                    (start..=end).map(Value::Int).collect()
                } else {
                    (end..=start).rev().map(Value::Int).collect()
                };
                Ok(values)
            }
            Value::Array(items) => Ok(items.borrow().iter().map(value_copy).collect()),
            Value::Map(entries) => Ok(entries
                .borrow()
                .iter()
                .map(|(_, value)| value_copy(value))
                .collect()),
            Value::String(value) => Ok(string_char_values(&value)),
            Value::Generator { values, .. } => Ok(values.borrow().iter().map(value_copy).collect()),
            other => Err(VerseError::runtime_at(
                format!("cannot iterate over value `{other}`"),
                span,
            )),
        }
    }

    fn iter_bindings(
        &self,
        binding: &ForBinding,
        iterable: Value,
        span: Span,
    ) -> Result<Vec<Vec<(String, Value)>>, VerseError> {
        match binding {
            ForBinding::Value(name) => Ok(self
                .iter_values(iterable, span)?
                .into_iter()
                .map(|value| vec![(name.clone(), value)])
                .collect()),
            ForBinding::Pair { key, value } => match iterable {
                Value::Array(items) => Ok(items
                    .borrow()
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        vec![
                            (key.clone(), Value::Int(index as i64)),
                            (value.clone(), value_copy(item)),
                        ]
                    })
                    .collect()),
                Value::Map(entries) => Ok(entries
                    .borrow()
                    .iter()
                    .map(|(entry_key, entry_value)| {
                        vec![
                            (key.clone(), value_copy(entry_key)),
                            (value.clone(), value_copy(entry_value)),
                        ]
                    })
                    .collect()),
                other => Err(VerseError::runtime_at(
                    format!("cannot use `->` for iteration over value `{other}`"),
                    span,
                )),
            },
        }
    }

    fn member_value(
        &self,
        object: Value,
        name: &str,
        env: &Env,
        span: Span,
    ) -> Result<Value, VerseError> {
        if let Value::EnumType {
            name: enum_name,
            variants,
            ..
        } = &object
        {
            if variants.iter().any(|variant| variant == name) {
                return Ok(Value::EnumValue {
                    enum_name: enum_name.clone(),
                    variant: name.to_string(),
                });
            }
            return Err(VerseError::runtime_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        if let Value::StructInstance {
            struct_name,
            fields,
            ..
        } = &object
        {
            if let Some(value) = fields
                .iter()
                .find(|(field_name, _)| field_name == name)
                .map(|(_, value)| value_copy(value))
            {
                return Ok(value);
            }
            if let Some(method) = self.extension_method_value(object.clone(), name, env) {
                return Ok(method);
            }
            return Err(VerseError::runtime_at(
                format!("struct `{struct_name}` has no field `{name}`"),
                span,
            ));
        }

        if let Value::Module {
            name: module_name,
            env,
        } = &object
        {
            return env.get(name).ok_or_else(|| {
                VerseError::runtime_at(
                    format!("module `{module_name}` has no member `{name}`"),
                    span,
                )
            });
        }

        if matches!(object, Value::Session) {
            if name == "Environment" {
                return Ok(Value::NativeFunction {
                    name: "Environment",
                    arity: Some(0),
                    decides: false,
                    function: native_session_environment,
                });
            }
            return Err(VerseError::runtime_at(
                format!("class `session` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object, Value::Result { .. }) {
            return match name {
                "GetSuccess" => Ok(Value::NativeResultMethod {
                    name: "GetSuccess",
                    result: Box::new(value_copy(&object)),
                }),
                "GetError" => Ok(Value::NativeResultMethod {
                    name: "GetError",
                    result: Box::new(value_copy(&object)),
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface result has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::Event { payload, waiters } = &object {
            return match name {
                "Await" => Ok(Value::NativeEventMethod {
                    name: "Await",
                    payload: payload.clone(),
                    waiters: Some(waiters.clone()),
                }),
                "Signal" => Ok(Value::NativeEventMethod {
                    name: "Signal",
                    payload: payload.clone(),
                    waiters: Some(waiters.clone()),
                }),
                _ => Err(VerseError::runtime_at(
                    format!("class `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::Awaitable { payload } = &object {
            return match name {
                "Await" => Ok(Value::NativeEventMethod {
                    name: "Await",
                    payload: payload.clone(),
                    waiters: None,
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::Signalable { payload } = &object {
            return match name {
                "Signal" => Ok(Value::NativeEventMethod {
                    name: "Signal",
                    payload: Some(payload.clone()),
                    waiters: None,
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        let is_listenable = matches!(&object, Value::Listenable { .. });
        if let Value::Subscribable {
            payload,
            subscribers,
            next_subscriber_id,
        }
        | Value::Listenable {
            payload,
            subscribers,
            next_subscriber_id,
        } = &object
        {
            return match name {
                "Await" if is_listenable => Ok(Value::NativeEventMethod {
                    name: "Await",
                    payload: payload.clone(),
                    waiters: None,
                }),
                "Subscribe" => Ok(Value::NativeSubscribableMethod {
                    name: "Subscribe",
                    payload: payload.clone(),
                    subscribers: subscribers.clone(),
                    next_subscriber_id: next_subscriber_id.clone(),
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::SubscriptionCancelHandle {
            subscribers,
            subscriber_id,
        } = &object
        {
            return match name {
                "Cancel" => Ok(Value::NativeSubscriptionCancelMethod {
                    name: "Cancel",
                    subscribers: subscribers.clone(),
                    subscriber_id: *subscriber_id,
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface `cancelable` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::Task(task) = &object {
            return match name {
                "Await" => Ok(Value::NativeTaskMethod {
                    name: "Await",
                    task: task.clone(),
                }),
                _ => Err(VerseError::runtime_at(
                    format!("class `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if matches!(object, Value::Modifier { .. } | Value::ModifierStack { .. }) {
            return match name {
                "Evaluate" => Ok(Value::NativeModifierMethod {
                    name: "Evaluate",
                    receiver: Box::new(value_copy(&object)),
                }),
                "AddModifier" if matches!(object, Value::ModifierStack { .. }) => {
                    Ok(Value::NativeModifierMethod {
                        name: "AddModifier",
                        receiver: Box::new(value_copy(&object)),
                    })
                }
                "FirstPosition" if matches!(object, Value::ModifierStack { .. }) => {
                    Ok(modifier_stack_position(&object, true))
                }
                "LastPosition" if matches!(object, Value::ModifierStack { .. }) => {
                    Ok(modifier_stack_position(&object, false))
                }
                _ => Err(VerseError::runtime_at(
                    format!("class `{object}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::ModifierCancelHandle { entries, entry_id } = &object {
            return match name {
                "Cancel" => Ok(Value::NativeCancelMethod {
                    name: "Cancel",
                    entries: entries.clone(),
                    entry_id: *entry_id,
                }),
                _ => Err(VerseError::runtime_at(
                    format!("interface `cancelable` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Value::ClassInstance {
            class_name,
            unique,
            fields,
            methods,
        } = &object
        {
            if let Some(value) = fields
                .borrow()
                .iter()
                .find(|field| field.name == name)
                .map(|field| value_copy(&field.value))
            {
                return Ok(value);
            }

            if let Some(method) = self.bound_method_group_value(
                methods.iter().filter(|method| method.name == name),
                class_name.clone(),
                *unique,
                fields.clone(),
                methods.clone(),
                span,
            )? {
                return Ok(method);
            }

            if let Some(method) = self.extension_method_value(object.clone(), name, env) {
                return Ok(method);
            }

            return Err(VerseError::runtime_at(
                format!("class `{class_name}` has no member `{name}`"),
                span,
            ));
        }

        if let Some(method) = self.extension_method_value(object.clone(), name, env) {
            return Ok(method);
        }

        if name != "Length" {
            return Err(VerseError::runtime_at(
                format!("unknown member `{name}` on value `{object}`"),
                span,
            ));
        }

        let length = match object {
            Value::Array(items) => items.borrow().len(),
            Value::Map(entries) => entries.borrow().len(),
            Value::String(text) => text.len(),
            other => {
                return Err(VerseError::runtime_at(
                    format!("value `{other}` has no member `Length`"),
                    span,
                ));
            }
        };

        Ok(Value::Int(length as i64))
    }

    fn extension_method_value(&self, object: Value, name: &str, env: &Env) -> Option<Value> {
        for method in env.get_extension_methods(name) {
            if !runtime_value_matches_annotation(
                &object,
                method.receiver.annotation.as_ref(),
                &method.closure,
            ) {
                continue;
            }

            let call_closure = Env::child(&method.closure);
            call_closure.define(&method.receiver.name, object, false);
            return Some(Value::Function {
                params: method.params,
                effects: method.effects,
                body: method.body,
                closure: call_closure,
            });
        }

        None
    }

    fn qualified_extension_method_value(
        &self,
        object: Value,
        qualifier: &str,
        name: &str,
        env: &Env,
    ) -> Option<Value> {
        for method in env.get_extension_methods(name) {
            if !runtime_extension_method_has_qualifier(&method, qualifier) {
                continue;
            }
            if !runtime_value_matches_annotation(
                &object,
                method.receiver.annotation.as_ref(),
                &method.closure,
            ) {
                continue;
            }

            let call_closure = Env::child(&method.closure);
            call_closure.define(&method.receiver.name, object, false);
            return Some(Value::Function {
                params: method.params,
                effects: method.effects,
                body: method.body,
                closure: call_closure,
            });
        }

        None
    }

    fn qualified_member_value(
        &self,
        object: Value,
        qualifier: &str,
        name: &str,
        span: Span,
        env: &Env,
    ) -> Result<Value, VerseError> {
        if let Value::ClassInstance {
            class_name,
            unique,
            fields,
            methods,
        } = &object
        {
            if let Some(method) = self.bound_method_group_value(
                methods.iter().filter(|method| {
                    method.name == name && runtime_method_has_qualifier(method, qualifier)
                }),
                class_name.clone(),
                *unique,
                fields.clone(),
                methods.clone(),
                span,
            )? {
                return Ok(method);
            }
            if let Some(method) =
                self.qualified_extension_method_value(object.clone(), qualifier, name, env)
            {
                return Ok(method);
            }
            return Err(VerseError::runtime_at(
                format!("class `{class_name}` has no method `({qualifier}:){name}`"),
                span,
            ));
        }

        if let Some(method) =
            self.qualified_extension_method_value(object.clone(), qualifier, name, env)
        {
            return Ok(method);
        }

        Err(VerseError::runtime_at(
            format!("cannot use qualified member on value `{object}`"),
            span,
        ))
    }

    fn qualified_name_value(
        &self,
        qualifier: &str,
        name: &str,
        env: &Env,
        span: Span,
    ) -> Result<Value, VerseError> {
        if qualifier != "super" {
            let qualified = format!("{qualifier}.{name}");
            return env.get_qualified_path(&qualified).ok_or_else(|| {
                VerseError::runtime_at(format!("unknown qualified name `{qualified}`"), span)
            });
        }

        let Value::ClassType {
            name: base_name,
            methods: super_methods,
            ..
        } = env
            .get("super")
            .ok_or_else(|| VerseError::runtime_at("undefined name `super`", span))?
        else {
            return Err(VerseError::runtime_at(
                "qualifier `super` is not a class type",
                span,
            ));
        };

        let Value::ClassInstance {
            class_name,
            unique,
            fields,
            methods,
        } = env
            .get("Self")
            .ok_or_else(|| VerseError::runtime_at("undefined name `Self`", span))?
        else {
            return Err(VerseError::runtime_at(
                "`super` method call requires `Self`",
                span,
            ));
        };

        self.bound_method_group_value(
            super_methods.iter().filter(|method| method.name == name),
            class_name,
            unique,
            fields,
            methods,
            span,
        )?
        .ok_or_else(|| {
            VerseError::runtime_at(format!("class `{base_name}` has no method `{name}`"), span)
        })
    }

    fn call(&self, callee: Value, args: Vec<CallValue>, span: Span) -> Result<Value, VerseError> {
        match callee {
            Value::Tuple(items) => {
                if args.iter().any(|arg| arg.name.is_some()) {
                    return Err(VerseError::runtime_at(
                        "tuple index does not accept named arguments",
                        span,
                    ));
                }
                if args.len() != 1 {
                    return Err(VerseError::runtime_at(
                        format!("tuple index expects 1 argument, got {}", args.len()),
                        span,
                    ));
                }

                let index = expect_tuple_index(&args[0].value, span)?;
                items.get(index).map(value_copy).ok_or_else(|| {
                    VerseError::runtime_at(
                        format!(
                            "tuple index {index} out of bounds for length {}",
                            items.len()
                        ),
                        span,
                    )
                })
            }
            Value::Function {
                params,
                effects,
                body,
                closure,
            } => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let call_env = Env::child(&closure);
                self.bind_function_args(&params, args, &call_env, span)?;

                match self.eval_expr(&body, &call_env)? {
                    Flow::Value(value) => Ok(value),
                    Flow::Return(value) => Ok(value),
                    Flow::Break => Err(VerseError::runtime_at("`break` escaped function", span)),
                    Flow::Pending(suspension) => Ok(Value::Suspended(suspension.map(
                        move |_, flow| match flow {
                            Flow::Value(value) | Flow::Return(value) => Ok(Flow::Value(value)),
                            Flow::Break => {
                                Err(VerseError::runtime_at("`break` escaped function", span))
                            }
                            Flow::Pending(suspension) => Ok(Flow::Pending(suspension)),
                        },
                    ))),
                }
            }
            Value::Overload(overloads) => self.call_overload(overloads, args, false, span),
            Value::BoundMethod {
                params,
                effects,
                body,
                closure,
                super_type,
                extension_methods,
                class_name,
                unique,
                fields,
                methods,
                ..
            } => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let field_env = Env::child(&closure);
                let initial_fields = fields.borrow().clone();
                let instance_class_name = class_name.clone();
                field_env.define(
                    "Self",
                    Value::ClassInstance {
                        class_name: instance_class_name.clone(),
                        unique,
                        fields: fields.clone(),
                        methods: methods.clone(),
                    },
                    false,
                );
                if let Some(super_type) = super_type {
                    field_env.define("super", *super_type, false);
                }
                for field in &initial_fields {
                    field_env.define(&field.name, field.value.clone(), field.mutable);
                }
                self.bind_instance_methods(
                    &field_env,
                    &instance_class_name,
                    unique,
                    &fields,
                    &methods,
                );
                self.bind_instance_extension_methods(&field_env, &extension_methods);

                let call_env = Env::child(&field_env);
                self.bind_function_args(&params, args, &call_env, span)?;

                let flow = self.eval_expr(&body, &call_env);
                self.sync_instance_fields(&fields, &field_env, &initial_fields);

                match flow? {
                    Flow::Value(value) => Ok(value),
                    Flow::Return(value) => Ok(value),
                    Flow::Break => Err(VerseError::runtime_at("`break` escaped method", span)),
                    Flow::Pending(suspension) => {
                        let fields = fields.clone();
                        let field_env = field_env.clone();
                        let initial_fields = initial_fields.clone();
                        Ok(Value::Suspended(suspension.map(
                            move |interpreter, flow| {
                                interpreter.sync_instance_fields(
                                    &fields,
                                    &field_env,
                                    &initial_fields,
                                );
                                match flow {
                                    Flow::Value(value) | Flow::Return(value) => {
                                        Ok(Flow::Value(value))
                                    }
                                    Flow::Break => {
                                        Err(VerseError::runtime_at("`break` escaped method", span))
                                    }
                                    Flow::Pending(suspension) => Ok(Flow::Pending(suspension)),
                                }
                            },
                        )))
                    }
                }
            }
            Value::NativeFunction {
                name,
                arity,
                decides: _,
                function,
            } => call_native_function(name, arity, function, args, span)?.into_value(name, span),
            Value::NativeResultMethod { name, result } => {
                call_native_result_method(name, &result, args, span)?.into_value(name, span)
            }
            Value::NativeEventMethod {
                name,
                payload,
                waiters,
            } => call_native_event_method(
                self,
                name,
                payload.as_ref(),
                waiters.as_ref(),
                args,
                span,
            )?
            .into_value(name, span),
            Value::NativeSubscribableMethod {
                name,
                payload,
                subscribers,
                next_subscriber_id,
            } => call_native_subscribable_method(
                name,
                payload.as_ref(),
                &subscribers,
                &next_subscriber_id,
                args,
                span,
            )?
            .into_value(name, span),
            Value::NativeTaskMethod { name, task } => {
                call_native_task_method(name, &task, args, span)?.into_value(name, span)
            }
            Value::NativeModifierMethod { name, receiver } => self
                .call_native_modifier_method(name, *receiver, args, span)?
                .into_value(name, span),
            Value::NativeCancelMethod {
                name,
                entries,
                entry_id,
            } => call_native_cancel_method(name, &entries, entry_id, args, span)?
                .into_value(name, span),
            Value::NativeSubscriptionCancelMethod {
                name,
                subscribers,
                subscriber_id,
            } => call_native_subscription_cancel_method(
                name,
                &subscribers,
                subscriber_id,
                args,
                span,
            )?
            .into_value(name, span),
            other => Err(VerseError::runtime_at(
                format!("`{other}` is not callable"),
                span,
            )),
        }
    }

    fn call_failure(
        &self,
        callee: Value,
        args: Vec<CallValue>,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        match callee {
            Value::Function {
                params,
                effects,
                body,
                closure,
            } if has_runtime_effect(&effects, "decides") => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let call_env = Env::child(&closure);
                self.bind_function_args(&params, args, &call_env, span)?;
                self.eval_failure_expr_transactional(&body, &call_env)
            }
            Value::Overload(overloads) => self.call_overload_failure(overloads, args, span),
            Value::BoundMethod {
                params,
                effects,
                body,
                closure,
                super_type,
                extension_methods,
                class_name,
                unique,
                fields,
                methods,
                ..
            } if has_runtime_effect(&effects, "decides") => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let field_env = Env::child(&closure);
                let initial_fields = fields.borrow().clone();
                let instance_class_name = class_name.clone();
                field_env.define(
                    "Self",
                    Value::ClassInstance {
                        class_name: instance_class_name.clone(),
                        unique,
                        fields: fields.clone(),
                        methods: methods.clone(),
                    },
                    false,
                );
                if let Some(super_type) = super_type {
                    field_env.define("super", *super_type, false);
                }
                for field in &initial_fields {
                    field_env.define(&field.name, field.value.clone(), field.mutable);
                }
                self.bind_instance_methods(
                    &field_env,
                    &instance_class_name,
                    unique,
                    &fields,
                    &methods,
                );
                self.bind_instance_extension_methods(&field_env, &extension_methods);

                let call_env = Env::child(&field_env);
                self.bind_function_args(&params, args, &call_env, span)?;
                let result = self.eval_failure_expr_transactional(&body, &call_env)?;
                if result.is_some() {
                    self.sync_instance_fields(&fields, &field_env, &initial_fields);
                }
                Ok(result)
            }
            Value::NativeFunction {
                name,
                arity,
                decides: true,
                function,
            } => match call_native_function(name, arity, function, args, span)? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            },
            Value::NativeResultMethod { name, result } => {
                match call_native_result_method(name, &result, args, span)? {
                    NativeResult::Value(value) => Ok(Some(value)),
                    NativeResult::Failure(_) => Ok(None),
                }
            }
            Value::NativeEventMethod {
                name,
                payload,
                waiters,
            } => {
                match call_native_event_method(
                    self,
                    name,
                    payload.as_ref(),
                    waiters.as_ref(),
                    args,
                    span,
                )? {
                    NativeResult::Value(value) => Ok(Some(value)),
                    NativeResult::Failure(_) => Ok(None),
                }
            }
            Value::NativeSubscribableMethod {
                name,
                payload,
                subscribers,
                next_subscriber_id,
            } => match call_native_subscribable_method(
                name,
                payload.as_ref(),
                &subscribers,
                &next_subscriber_id,
                args,
                span,
            )? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            },
            Value::NativeTaskMethod { name, task } => {
                match call_native_task_method(name, &task, args, span)? {
                    NativeResult::Value(value) => Ok(Some(value)),
                    NativeResult::Failure(_) => Ok(None),
                }
            }
            Value::NativeModifierMethod { name, receiver } => {
                match self.call_native_modifier_method(name, *receiver, args, span)? {
                    NativeResult::Value(value) => Ok(Some(value)),
                    NativeResult::Failure(_) => Ok(None),
                }
            }
            Value::NativeCancelMethod {
                name,
                entries,
                entry_id,
            } => match call_native_cancel_method(name, &entries, entry_id, args, span)? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            },
            Value::NativeSubscriptionCancelMethod {
                name,
                subscribers,
                subscriber_id,
            } => match call_native_subscription_cancel_method(
                name,
                &subscribers,
                subscriber_id,
                args,
                span,
            )? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            },
            other => self.call(other, args, span).map(Some),
        }
    }

    fn call_failure_maybe_pending(
        &self,
        callee: Value,
        args: Vec<CallValue>,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        match callee {
            Value::Function {
                params,
                effects,
                body,
                closure,
            } if has_runtime_effect(&effects, "decides") => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let call_env = Env::child(&closure);
                self.bind_function_args(&params, args, &call_env, span)?;
                self.eval_failure_expr_transactional_maybe_pending(&body, &call_env)
            }
            Value::Overload(overloads) => {
                self.call_overload_failure_maybe_pending(overloads, args, span)
            }
            Value::BoundMethod {
                params,
                effects,
                body,
                closure,
                super_type,
                extension_methods,
                class_name,
                unique,
                fields,
                methods,
                ..
            } if has_runtime_effect(&effects, "decides") => {
                self.ensure_data_member_default_effects_allowed(&effects, span)?;
                let field_env = Env::child(&closure);
                let initial_fields = fields.borrow().clone();
                let instance_class_name = class_name.clone();
                field_env.define(
                    "Self",
                    Value::ClassInstance {
                        class_name: instance_class_name.clone(),
                        unique,
                        fields: fields.clone(),
                        methods: methods.clone(),
                    },
                    false,
                );
                if let Some(super_type) = super_type {
                    field_env.define("super", *super_type, false);
                }
                for field in &initial_fields {
                    field_env.define(&field.name, field.value.clone(), field.mutable);
                }
                self.bind_instance_methods(
                    &field_env,
                    &instance_class_name,
                    unique,
                    &fields,
                    &methods,
                );
                self.bind_instance_extension_methods(&field_env, &extension_methods);

                let call_env = Env::child(&field_env);
                self.bind_function_args(&params, args, &call_env, span)?;
                match self.eval_failure_expr_transactional_maybe_pending(&body, &call_env)? {
                    FailureEval::Ready(result) => {
                        if result.is_some() {
                            self.sync_instance_fields(&fields, &field_env, &initial_fields);
                        }
                        Ok(FailureEval::Ready(result))
                    }
                    FailureEval::Pending(suspension) => {
                        let fields = fields.clone();
                        let field_env = field_env.clone();
                        let initial_fields = initial_fields.clone();
                        Ok(FailureEval::Pending(suspension.map(
                            move |interpreter, flow| {
                                let fields = fields.clone();
                                let field_env = field_env.clone();
                                let initial_fields = initial_fields.clone();
                                let continuation: Rc<FailureContinuation> =
                                    Rc::new(move |interpreter, result| {
                                        if result.is_some() {
                                            interpreter.sync_instance_fields(
                                                &fields,
                                                &field_env,
                                                &initial_fields,
                                            );
                                        }
                                        Ok(failure_result_flow(result))
                                    });
                                Self::continue_when_failure_result(
                                    interpreter,
                                    flow,
                                    span,
                                    continuation,
                                )
                            },
                        )))
                    }
                }
            }
            Value::NativeFunction {
                name,
                arity,
                decides: true,
                function,
            } => match call_native_function(name, arity, function, args, span)? {
                NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
            },
            Value::NativeResultMethod { name, result } => {
                match call_native_result_method(name, &result, args, span)? {
                    NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                    NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
                }
            }
            Value::NativeEventMethod {
                name,
                payload,
                waiters,
            } => match call_native_event_method(
                self,
                name,
                payload.as_ref(),
                waiters.as_ref(),
                args,
                span,
            )? {
                NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
            },
            Value::NativeSubscribableMethod {
                name,
                payload,
                subscribers,
                next_subscriber_id,
            } => match call_native_subscribable_method(
                name,
                payload.as_ref(),
                &subscribers,
                &next_subscriber_id,
                args,
                span,
            )? {
                NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
            },
            Value::NativeTaskMethod { name, task } => {
                match call_native_task_method(name, &task, args, span)? {
                    NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                    NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
                }
            }
            Value::NativeModifierMethod { name, receiver } => {
                match self.call_native_modifier_method(name, *receiver, args, span)? {
                    NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                    NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
                }
            }
            Value::NativeCancelMethod {
                name,
                entries,
                entry_id,
            } => match call_native_cancel_method(name, &entries, entry_id, args, span)? {
                NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
            },
            Value::NativeSubscriptionCancelMethod {
                name,
                subscribers,
                subscriber_id,
            } => match call_native_subscription_cancel_method(
                name,
                &subscribers,
                subscriber_id,
                args,
                span,
            )? {
                NativeResult::Value(value) => Ok(FailureEval::Ready(Some(value))),
                NativeResult::Failure(_) => Ok(FailureEval::Ready(None)),
            },
            other => match self.call(other, args, span)? {
                Value::Suspended(suspension) => Ok(FailureEval::Pending(suspension.map(
                    move |_, flow| match flow {
                        Flow::Value(value) => Ok(failure_result_flow(Some(value))),
                        Flow::Pending(suspension) => Ok(Flow::Pending(suspension)),
                        Flow::Return(value) => Ok(failure_result_flow(Some(value))),
                        Flow::Break => {
                            Err(VerseError::runtime_at("`break` escaped function", span))
                        }
                    },
                ))),
                value => Ok(FailureEval::Ready(Some(value))),
            },
        }
    }

    fn call_overload(
        &self,
        overloads: Vec<Value>,
        args: Vec<CallValue>,
        require_decides: bool,
        span: Span,
    ) -> Result<Value, VerseError> {
        let Some(overload) = self.select_overload(&overloads, &args, require_decides) else {
            let style = if require_decides { "[]" } else { "()" };
            return Err(VerseError::runtime_at(
                format!("no overload matches {style} call"),
                span,
            ));
        };
        self.call(overload, args, span)
    }

    fn call_overload_failure(
        &self,
        overloads: Vec<Value>,
        args: Vec<CallValue>,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        let Some(overload) = self.select_overload(&overloads, &args, true) else {
            return Err(VerseError::runtime_at("no overload matches [] call", span));
        };
        self.call_failure(overload, args, span)
    }

    fn call_overload_failure_maybe_pending(
        &self,
        overloads: Vec<Value>,
        args: Vec<CallValue>,
        span: Span,
    ) -> Result<FailureEval, VerseError> {
        let Some(overload) = self.select_overload(&overloads, &args, true) else {
            return Err(VerseError::runtime_at("no overload matches [] call", span));
        };
        self.call_failure_maybe_pending(overload, args, span)
    }

    fn select_overload(
        &self,
        overloads: &[Value],
        args: &[CallValue],
        require_decides: bool,
    ) -> Option<Value> {
        overloads
            .iter()
            .filter_map(|overload| {
                let score = match overload {
                    Value::Function {
                        params,
                        effects,
                        closure,
                        ..
                    } if has_runtime_effect(effects, "decides") == require_decides => {
                        self.function_call_match_score(params, args, closure)
                    }
                    Value::BoundMethod {
                        params,
                        effects,
                        closure,
                        ..
                    } if has_runtime_effect(effects, "decides") == require_decides => {
                        self.function_call_match_score(params, args, closure)
                    }
                    _ => None,
                }?;
                Some((score, overload.clone()))
            })
            .min_by_key(|(score, _)| *score)
            .map(|(_, overload)| overload)
    }

    fn function_call_match_score(
        &self,
        params: &[Param],
        args: &[CallValue],
        env: &Env,
    ) -> Option<usize> {
        let type_params = runtime_type_param_constraints(params);
        let match_env = Env::child(env);
        if let [param] = params
            && let ParamPattern::Tuple(items) = &param.pattern
            && tuple_params_have_named_or_default(items)
        {
            if args.len() == 1 && args[0].name.is_none() && matches!(args[0].value, Value::Tuple(_))
            {
                self.infer_runtime_type_params_for_param(
                    param,
                    &args[0].value,
                    &match_env,
                    &type_params,
                    args[0].span,
                )
                .ok()?;
                return runtime_type_match_score(
                    &args[0].value,
                    param.annotation.as_ref(),
                    &match_env,
                );
            }
            return self.function_call_match_score(items, args, &match_env);
        }

        if args.iter().all(|arg| arg.name.is_none()) && params.iter().all(|param| !param.named) {
            let values = function_call_values(
                params,
                args.iter().map(|arg| value_copy(&arg.value)).collect(),
                &match_env,
                args.first()
                    .map_or_else(|| Span::new(0, 0, 1, 1), |arg| arg.span),
            )
            .ok()?;
            for (param, value) in params.iter().zip(&values) {
                self.infer_runtime_type_params_for_param(
                    param,
                    value,
                    &match_env,
                    &type_params,
                    args.first()
                        .map_or_else(|| Span::new(0, 0, 1, 1), |arg| arg.span),
                )
                .ok()?;
            }
            return params
                .iter()
                .zip(values)
                .try_fold(0usize, |score, (param, value)| {
                    runtime_type_match_score(&value, param.annotation.as_ref(), &match_env)
                        .map(|next| score + next)
                });
        }

        let mut assigned = vec![false; params.len()];
        let mut positional_index = 0usize;
        let mut score = 0usize;

        for arg in args {
            match &arg.name {
                None => {
                    let (param_index, param) = params
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)?;
                    positional_index = param_index + 1;
                    if assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    self.infer_runtime_type_params_for_param(
                        param,
                        &arg.value,
                        &match_env,
                        &type_params,
                        arg.span,
                    )
                    .ok()?;
                    score += runtime_type_match_score(
                        &arg.value,
                        param.annotation.as_ref(),
                        &match_env,
                    )?;
                }
                Some(name) => {
                    let (param_index, param) = params
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == *name)?;
                    if (arg.optional && !param.named) || assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    self.infer_runtime_type_params_for_param(
                        param,
                        &arg.value,
                        &match_env,
                        &type_params,
                        arg.span,
                    )
                    .ok()?;
                    score += runtime_type_match_score(
                        &arg.value,
                        param.annotation.as_ref(),
                        &match_env,
                    )?;
                }
            }
        }

        for (index, param) in params.iter().enumerate() {
            if !assigned[index] && param.default.is_none() {
                return None;
            }
        }

        Some(score)
    }

    fn sync_instance_fields(
        &self,
        fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        field_env: &Env,
        initial_fields: &[RuntimeClassInstanceField],
    ) {
        let mut receiver_fields = fields.borrow_mut();
        for field in receiver_fields.iter_mut().filter(|field| field.mutable) {
            if let Some(value) = field_env.get(&field.name) {
                let unchanged = initial_fields
                    .iter()
                    .find(|initial| initial.name == field.name)
                    .is_some_and(|initial| initial.value == value);
                if !unchanged {
                    field.value = value;
                }
            }
        }
    }

    fn call_native_modifier_method(
        &self,
        name: &'static str,
        receiver: Value,
        args: Vec<CallValue>,
        span: Span,
    ) -> Result<NativeResult, VerseError> {
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(VerseError::runtime_at(
                format!("`{name}` does not accept named arguments"),
                span,
            ));
        }

        match (name, receiver) {
            ("Evaluate", Value::Modifier { .. }) => {
                let [value]: [CallValue; 1] = args.try_into().map_err(|args: Vec<CallValue>| {
                    VerseError::runtime_at(
                        format!("`Evaluate` expected 1 arguments, got {}", args.len()),
                        span,
                    )
                })?;
                Ok(NativeResult::Value(value.value))
            }
            (
                "Evaluate",
                Value::ModifierStack {
                    item_type, entries, ..
                },
            ) => {
                let [value]: [CallValue; 1] = args.try_into().map_err(|args: Vec<CallValue>| {
                    VerseError::runtime_at(
                        format!("`Evaluate` expected 1 arguments, got {}", args.len()),
                        span,
                    )
                })?;
                let value =
                    self.evaluate_modifier_stack(&item_type, &entries, value.value, span)?;
                Ok(NativeResult::Value(value))
            }
            (
                "AddModifier",
                Value::ModifierStack {
                    item_type,
                    entries,
                    next_order,
                },
            ) => {
                let [modifier, position]: [CallValue; 2] =
                    args.try_into().map_err(|args: Vec<CallValue>| {
                        VerseError::runtime_at(
                            format!("`AddModifier` expected 2 arguments, got {}", args.len()),
                            span,
                        )
                    })?;
                if !runtime_value_matches_modifier_type(&modifier.value, &item_type, &self.globals)
                {
                    return Err(VerseError::runtime_at(
                        format!(
                            "`AddModifier` expected modifier({}), got {}",
                            render_runtime_type_name(&item_type),
                            modifier.value
                        ),
                        modifier.span,
                    ));
                }
                let position = expect_runtime_rational(
                    &position.value,
                    "`AddModifier` Position",
                    position.span,
                )?;
                let id = {
                    let mut next = next_order.borrow_mut();
                    let id = *next;
                    *next += 1;
                    id
                };
                entries.borrow_mut().push(RuntimeModifierEntry {
                    id,
                    position,
                    order: id,
                    modifier: modifier.value,
                });
                Ok(NativeResult::Value(Value::ModifierCancelHandle {
                    entries,
                    entry_id: id,
                }))
            }
            (method, receiver) => Err(VerseError::runtime_at(
                format!("value `{receiver}` has no native modifier method `{method}`"),
                span,
            )),
        }
    }

    fn evaluate_modifier_stack(
        &self,
        item_type: &TypeName,
        entries: &Rc<RefCell<Vec<RuntimeModifierEntry>>>,
        input: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        let mut ordered = entries.borrow().clone();
        ordered.sort_by(|left, right| {
            compare_rational(left.position, right.position).then(left.order.cmp(&right.order))
        });

        let mut value = coerce_value_to_type_name(&self.globals, item_type, input);
        for entry in ordered {
            value = self.evaluate_modifier_value(entry.modifier, value, span)?;
            value = coerce_value_to_type_name(&self.globals, item_type, value);
        }
        Ok(value)
    }

    fn evaluate_modifier_value(
        &self,
        modifier: Value,
        input: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        match modifier {
            Value::Modifier { .. } => Ok(input),
            Value::ModifierStack {
                item_type, entries, ..
            } => self.evaluate_modifier_stack(&item_type, &entries, input, span),
            other => {
                let callee = self.member_value(other, "Evaluate", &self.globals, span)?;
                self.call(
                    callee,
                    vec![CallValue {
                        name: None,
                        optional: false,
                        value: input,
                        span,
                    }],
                    span,
                )
            }
        }
    }

    fn bind_function_args(
        &self,
        params: &[Param],
        args: Vec<CallValue>,
        call_env: &Env,
        span: Span,
    ) -> Result<(), VerseError> {
        let type_params = runtime_type_param_constraints(params);
        if let [param] = params
            && let ParamPattern::Tuple(items) = &param.pattern
            && tuple_params_have_named_or_default(items)
        {
            if args.len() == 1 && args[0].name.is_none() && matches!(args[0].value, Value::Tuple(_))
            {
                let arg = args.into_iter().next().expect("length checked");
                return self.bind_param_value(param, arg.value, call_env, arg.span, &type_params);
            }
            return self.bind_function_args(items, args, call_env, span);
        }

        if args.iter().all(|arg| arg.name.is_none()) && params.iter().all(|param| !param.named) {
            let values = function_call_values(
                params,
                args.into_iter().map(|arg| arg.value).collect(),
                call_env,
                span,
            )?;
            for (param, value) in params.iter().zip(values) {
                self.bind_param_value(param, value, call_env, span, &type_params)?;
            }
            return Ok(());
        }

        let mut assigned = vec![false; params.len()];
        let mut positional_index = 0usize;

        for arg in args {
            match arg.name {
                None => {
                    let Some((param_index, param)) = params
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)
                    else {
                        return Err(VerseError::runtime_at(
                            "positional argument does not match any positional parameter",
                            arg.span,
                        ));
                    };
                    positional_index = param_index + 1;
                    assigned[param_index] = true;
                    self.bind_param_value(param, arg.value, call_env, arg.span, &type_params)?;
                }
                Some(name) => {
                    let Some((param_index, param)) = params
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == name)
                    else {
                        let rendered = rendered_call_argument_name(&name, arg.optional);
                        return Err(VerseError::runtime_at(
                            format!("unknown named argument `{rendered}`"),
                            arg.span,
                        ));
                    };
                    if arg.optional && !param.named {
                        return Err(VerseError::runtime_at(
                            format!("parameter `{name}` is not a named parameter"),
                            arg.span,
                        ));
                    }
                    if assigned[param_index] {
                        let rendered = rendered_call_argument_name(&name, arg.optional);
                        return Err(VerseError::runtime_at(
                            format!("duplicate argument for parameter `{rendered}`"),
                            arg.span,
                        ));
                    }
                    assigned[param_index] = true;
                    self.bind_param_value(param, arg.value, call_env, arg.span, &type_params)?;
                }
            }
        }

        for (index, param) in params.iter().enumerate() {
            if assigned[index] {
                continue;
            }
            let Some(default) = &param.default else {
                let rendered = if param.named {
                    format!("?{}", param.name)
                } else {
                    param.name.clone()
                };
                return Err(VerseError::runtime_at(
                    format!("missing required argument `{rendered}`"),
                    span,
                ));
            };
            let value = self.eval_value(default, call_env)?;
            self.bind_param_value(param, value, call_env, span, &type_params)?;
        }

        Ok(())
    }

    fn bind_param_value(
        &self,
        param: &Param,
        value: Value,
        call_env: &Env,
        span: Span,
        type_params: &HashMap<String, TypeParamConstraint>,
    ) -> Result<(), VerseError> {
        self.infer_runtime_type_params_for_param(param, &value, call_env, type_params, span)?;
        let value = coerce_annotated_value(call_env, param.annotation.as_ref(), value);
        if let Some(annotation) = param.annotation.as_ref()
            && !runtime_annotation_has_unresolved_type_params(
                &annotation.name,
                type_params,
                call_env,
            )
            && !runtime_value_matches_annotation(&value, Some(annotation), call_env)
        {
            let resolved = call_env.resolve_type_name(&annotation.name);
            let rendered = if param.named {
                format!("?{}", param.name)
            } else if param.name.is_empty() {
                "argument".to_string()
            } else {
                param.name.clone()
            };
            return Err(VerseError::runtime_at(
                format!(
                    "argument `{rendered}` expected `{}`, got `{value}`",
                    render_runtime_type_name(&resolved)
                ),
                span,
            ));
        }
        match &param.pattern {
            ParamPattern::Binding => {
                call_env.define(&param.name, value, false);
                Ok(())
            }
            ParamPattern::Anonymous => Ok(()),
            ParamPattern::Tuple(params) => {
                let Value::Tuple(items) = value else {
                    return Err(VerseError::runtime_at(
                        format!(
                            "destructured tuple parameter expected tuple value for `{}`",
                            param.name
                        ),
                        span,
                    ));
                };
                if items.len() != params.len() {
                    return Err(VerseError::runtime_at(
                        format!(
                            "destructured tuple parameter expected {} elements, got {}",
                            params.len(),
                            items.len()
                        ),
                        span,
                    ));
                }
                for (param, value) in params.iter().zip(items) {
                    self.bind_param_value(param, value, call_env, span, type_params)?;
                }
                Ok(())
            }
        }
    }

    fn infer_runtime_type_params_for_param(
        &self,
        param: &Param,
        value: &Value,
        env: &Env,
        type_params: &HashMap<String, TypeParamConstraint>,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(annotation) = &param.annotation {
            self.infer_runtime_type_params_from_type_name(
                &annotation.name,
                value,
                env,
                type_params,
                span,
            )?;
        }

        Ok(())
    }

    fn infer_runtime_type_params_from_type_name(
        &self,
        type_name: &TypeName,
        value: &Value,
        env: &Env,
        type_params: &HashMap<String, TypeParamConstraint>,
        span: Span,
    ) -> Result<(), VerseError> {
        match type_name {
            TypeName::Named(name) if type_params.contains_key(name) => {
                if let Some(actual) = runtime_type_name_for_value(value, env) {
                    self.bind_runtime_type_param(name, &actual, value, env, type_params, span)?;
                }
                Ok(())
            }
            TypeName::Array(Some(item_type)) => match value {
                Value::String(_) if type_name_is_string_char(item_type) => Ok(()),
                Value::Array(items) => {
                    for item in items.borrow().iter() {
                        self.infer_runtime_type_params_from_type_name(
                            item_type,
                            item,
                            env,
                            type_params,
                            span,
                        )?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            TypeName::Map(key_type, value_type) | TypeName::WeakMap(key_type, value_type) => {
                if let Value::Map(entries) = value {
                    for (key, item) in entries.borrow().iter() {
                        self.infer_runtime_type_params_from_type_name(
                            key_type,
                            key,
                            env,
                            type_params,
                            span,
                        )?;
                        self.infer_runtime_type_params_from_type_name(
                            value_type,
                            item,
                            env,
                            type_params,
                            span,
                        )?;
                    }
                }
                Ok(())
            }
            TypeName::Tuple(item_types) => {
                if let Value::Tuple(items) = value {
                    for (item_type, item) in item_types.iter().zip(items) {
                        self.infer_runtime_type_params_from_type_name(
                            item_type,
                            item,
                            env,
                            type_params,
                            span,
                        )?;
                    }
                }
                Ok(())
            }
            TypeName::Option(item_type) => {
                if let Value::Option(Some(item)) = value {
                    self.infer_runtime_type_params_from_type_name(
                        item_type,
                        item,
                        env,
                        type_params,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeName::Applied { name, args } if name == "result" && args.len() == 2 => {
                if let Value::Result { succeeded, value } = value {
                    let item_type = if *succeeded { &args[0] } else { &args[1] };
                    self.infer_runtime_type_params_from_type_name(
                        item_type,
                        value,
                        env,
                        type_params,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeName::Applied { .. } | TypeName::FunctionSignature { .. } => Ok(()),
            _ => Ok(()),
        }
    }

    fn bind_runtime_type_param(
        &self,
        name: &str,
        actual: &TypeName,
        value: &Value,
        env: &Env,
        type_params: &HashMap<String, TypeParamConstraint>,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(constraint) = type_params.get(name) else {
            return Ok(());
        };
        self.ensure_runtime_type_param_constraint(name, actual, value, constraint, env, span)?;

        if let Some(existing) = env.get_local_type_alias(name) {
            if let Some(merged) = merge_runtime_type_names(&existing, actual, env) {
                env.define_type_alias(name, merged);
                return Ok(());
            }
            return Err(VerseError::runtime_at(
                format!(
                    "type argument `{}` for `{name}` conflicts with inferred `{}`",
                    render_runtime_type_name(actual),
                    render_runtime_type_name(&existing)
                ),
                span,
            ));
        }

        env.define_type_alias(name, actual.clone());
        Ok(())
    }

    fn ensure_runtime_type_param_constraint(
        &self,
        name: &str,
        actual: &TypeName,
        value: &Value,
        constraint: &TypeParamConstraint,
        env: &Env,
        span: Span,
    ) -> Result<(), VerseError> {
        let satisfies = match constraint {
            TypeParamConstraint::Type => true,
            TypeParamConstraint::Subtype(expected) => {
                let expected = env.resolve_type_name(expected);
                runtime_type_name_satisfies_constraint(actual, value, &expected, env)
            }
        };

        if satisfies {
            Ok(())
        } else {
            let expected = match constraint {
                TypeParamConstraint::Type => TypeName::Any,
                TypeParamConstraint::Subtype(expected) => env.resolve_type_name(expected),
            };
            Err(VerseError::runtime_at(
                format!(
                    "type argument `{}` for `{name}` must be a subtype of `{}`",
                    render_runtime_type_name(actual),
                    render_runtime_type_name(&expected)
                ),
                span,
            ))
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

enum Flow {
    Value(Value),
    Return(Value),
    Break,
    Pending(RuntimeSuspension),
}

fn flow_from_value(value: Value) -> Flow {
    match value {
        Value::Pending => Flow::Pending(RuntimeSuspension::unresumable()),
        Value::Suspended(suspension) => Flow::Pending(suspension),
        other => Flow::Value(other),
    }
}

fn event_value(payload: Option<TypeName>) -> Value {
    Value::Event {
        payload,
        waiters: Rc::new(RefCell::new(Vec::new())),
    }
}

#[derive(Clone)]
struct Deferred {
    body: Expr,
    env: Env,
    span: Span,
}

#[derive(Clone)]
struct CallValue {
    name: Option<String>,
    optional: bool,
    value: Value,
    span: Span,
}

fn call_value_from_arg(arg: &CallArg, value: Value) -> CallValue {
    match arg {
        CallArg::Positional(expr) => CallValue {
            name: None,
            optional: false,
            value,
            span: expr.span,
        },
        CallArg::Named {
            name,
            optional,
            span,
            ..
        } => CallValue {
            name: Some(name.clone()),
            optional: *optional,
            value,
            span: *span,
        },
    }
}

fn copy_values(values: &[Value]) -> Vec<Value> {
    values.iter().map(value_copy).collect()
}

fn copy_call_values(values: &[CallValue]) -> Vec<CallValue> {
    values
        .iter()
        .map(|value| CallValue {
            name: value.name.clone(),
            optional: value.optional,
            value: value_copy(&value.value),
            span: value.span,
        })
        .collect()
}

fn copy_map_entries(values: &[(Value, Value)]) -> Vec<(Value, Value)> {
    values
        .iter()
        .map(|(key, value)| (value_copy(key), value_copy(value)))
        .collect()
}

fn values_to_call_values(values: Vec<Value>, span: Span) -> Vec<CallValue> {
    values
        .into_iter()
        .map(|value| CallValue {
            name: None,
            optional: false,
            value,
            span,
        })
        .collect()
}

fn rendered_call_argument_name(name: &str, optional: bool) -> String {
    if optional {
        format!("?{name}")
    } else {
        name.to_string()
    }
}

fn structured_task_result_value(branch_index: usize, value: Value) -> Value {
    Value::Tuple(vec![Value::Int(branch_index as i64), value])
}

fn structured_task_result_parts(value: Value) -> Result<(usize, Value), VerseError> {
    let Value::Tuple(mut items) = value else {
        return Err(VerseError::runtime(
            "internal structured task result expected tuple",
        ));
    };
    if items.len() != 2 {
        return Err(VerseError::runtime(
            "internal structured task result expected two items",
        ));
    }
    let value = items.pop().expect("length checked");
    let index = items.pop().expect("length checked");
    let Value::Int(index) = index else {
        return Err(VerseError::runtime(
            "internal structured task result expected int branch index",
        ));
    };
    let index = usize::try_from(index)
        .map_err(|_| VerseError::runtime("internal structured task branch index is negative"))?;
    Ok((index, value))
}

fn structured_sync_values(state: &StructuredSyncState) -> Result<Vec<Value>, VerseError> {
    state
        .values
        .iter()
        .map(|value| {
            value.as_ref().map(value_copy).ok_or_else(|| {
                VerseError::runtime("internal structured sync result missing branch value")
            })
        })
        .collect()
}

fn for_results_value(results: &Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Array(Rc::new(RefCell::new(copy_values(&results.borrow()))))
}

fn finish_for_pending_flow(
    flow: Flow,
    results: Rc<RefCell<Vec<Value>>>,
) -> Result<Flow, VerseError> {
    match flow {
        Flow::Value(_) => Ok(Flow::Value(for_results_value(&results))),
        Flow::Pending(suspension) => {
            Ok(Flow::Pending(suspension.map(move |_, flow| {
                finish_for_pending_flow(flow, results.clone())
            })))
        }
        signal => Ok(signal),
    }
}

fn for_clause_flow(flow: Option<Flow>) -> Result<Flow, VerseError> {
    Ok(flow.unwrap_or(Flow::Value(Value::None)))
}

fn clone_for_clause_state(state: &ForClauseState) -> ForClauseState {
    ForClauseState {
        clauses: state.clauses.clone(),
        index: state.index,
        body: state.body.clone(),
        env: state.env.clone(),
        results: state.results.clone(),
    }
}

fn clone_for_bindings(bindings: &[Vec<(String, Value)>]) -> Vec<Vec<(String, Value)>> {
    bindings
        .iter()
        .map(|iteration| {
            iteration
                .iter()
                .map(|(name, value)| (name.clone(), value_copy(value)))
                .collect()
        })
        .collect()
}

fn clone_for_iteration_state(state: &ForIterationState) -> ForIterationState {
    ForIterationState {
        clauses: state.clauses.clone(),
        index: state.index,
        body: state.body.clone(),
        env: state.env.clone(),
        results: state.results.clone(),
        bindings: clone_for_bindings(&state.bindings),
    }
}

fn failure_result_flow(result: Option<Value>) -> Flow {
    Flow::Value(Value::Result {
        succeeded: result.is_some(),
        value: Box::new(result.unwrap_or(Value::None)),
    })
}

fn failure_eval_to_flow(result: FailureEval) -> Result<Flow, VerseError> {
    match result {
        FailureEval::Ready(result) => Ok(failure_result_flow(result)),
        FailureEval::Pending(suspension) => Ok(Flow::Pending(suspension)),
    }
}

fn invert_failure_result(result: Option<Value>) -> Option<Value> {
    if result.is_some() {
        None
    } else {
        Some(Value::Bool(true))
    }
}

fn wrap_failure_transaction(
    result: FailureEval,
    transaction: Rc<RefCell<Option<EnvTransaction>>>,
    span: Span,
) -> FailureEval {
    match result {
        FailureEval::Ready(result) => {
            finish_failure_transaction(&transaction, result.as_ref());
            FailureEval::Ready(result)
        }
        FailureEval::Pending(suspension) => {
            let transaction_for_resume = transaction.clone();
            let transaction_for_cancel = transaction.clone();
            FailureEval::Pending(
                suspension
                    .map(move |interpreter, flow| {
                        let transaction = transaction_for_resume.clone();
                        let continuation: Rc<FailureContinuation> = Rc::new(move |_, result| {
                            finish_failure_transaction(&transaction, result.as_ref());
                            Ok(failure_result_flow(result))
                        });
                        Interpreter::continue_when_failure_result(
                            interpreter,
                            flow,
                            span,
                            continuation,
                        )
                    })
                    .on_cancel(move |_| {
                        restore_failure_transaction(&transaction_for_cancel);
                        Ok(())
                    }),
            )
        }
    }
}

fn finish_failure_transaction(
    transaction: &Rc<RefCell<Option<EnvTransaction>>>,
    result: Option<&Value>,
) {
    if result.is_some() {
        transaction.borrow_mut().take();
    } else {
        restore_failure_transaction(transaction);
    }
}

fn restore_failure_transaction(transaction: &Rc<RefCell<Option<EnvTransaction>>>) {
    if let Some(transaction) = transaction.borrow_mut().take() {
        transaction.restore();
    }
}

fn unwrap_option_failure_value(value: Value, span: Span) -> Result<Option<Value>, VerseError> {
    match value {
        Value::Option(Some(value)) => Ok(Some(*value)),
        Value::Option(None) | Value::Bool(false) => Ok(None),
        Value::Bool(true) => Ok(Some(Value::Bool(true))),
        other => Err(VerseError::runtime_at(
            format!("query operator expected bool or option, got `{other}`"),
            span,
        )),
    }
}

fn runtime_failure_member_bracket_supports_pending(object: &Value, _name: &str) -> bool {
    matches!(
        object,
        Value::StructInstance { .. }
            | Value::ClassInstance { .. }
            | Value::Result { .. }
            | Value::Module { .. }
    )
}

fn unwrap_option_value(value: Value, span: Span) -> Result<Value, VerseError> {
    match value {
        Value::Option(Some(value)) => Ok(*value),
        Value::Option(None) | Value::Bool(false) => {
            Err(VerseError::runtime_at("cannot unwrap empty option", span))
        }
        other => Err(VerseError::runtime_at(
            format!("cannot unwrap value `{other}` as option"),
            span,
        )),
    }
}

fn call_arg_expr(arg: &CallArg) -> &Expr {
    match arg {
        CallArg::Positional(expr) => expr,
        CallArg::Named { expr, .. } => expr,
    }
}

fn runtime_spawn_body_expr(body: &Expr) -> Result<&Expr, VerseError> {
    let ExprKind::Block(statements) = &body.kind else {
        return Err(VerseError::runtime_at(
            "`spawn` expects a braced expression body",
            body.span,
        ));
    };
    let [statement] = statements.as_slice() else {
        return Err(VerseError::runtime_at(
            "`spawn` body must contain exactly one expression",
            body.span,
        ));
    };
    let StmtKind::Expr(expr) = &statement.kind else {
        return Err(VerseError::runtime_at(
            "`spawn` body must contain exactly one expression",
            statement.span,
        ));
    };
    Ok(expr)
}

fn runtime_concurrent_body_statements(body: &Expr) -> Result<&[Stmt], VerseError> {
    let ExprKind::ColonBlock(statements) = &body.kind else {
        return Err(VerseError::runtime_at(
            "concurrency expression expects an indented block body",
            body.span,
        ));
    };
    Ok(statements)
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

fn expr_to_type_name(expr: &Expr) -> Result<TypeName, VerseError> {
    match &expr.kind {
        ExprKind::Ident(name) => Ok(TypeName::parse(name.clone())),
        ExprKind::Member { .. } => expr_to_type_path(expr)
            .map(TypeName::parse)
            .ok_or_else(|| VerseError::runtime_at("expected type argument", expr.span)),
        ExprKind::QualifiedName { qualifier, name } => {
            Ok(TypeName::Named(format!("{qualifier}.{name}")))
        }
        ExprKind::Call { callee, args } => {
            let Some(name) = expr_to_type_path(callee) else {
                return Err(VerseError::runtime_at(
                    "expected parametric type name",
                    callee.span,
                ));
            };

            let mut type_args = Vec::with_capacity(args.len());
            for arg in args {
                let CallArg::Positional(expr) = arg else {
                    return Err(VerseError::runtime_at(
                        "parametric type arguments do not accept named arguments",
                        call_arg_expr(arg).span,
                    ));
                };
                type_args.push(expr_to_type_name(expr)?);
            }

            match name.as_str() {
                "tuple" => {
                    if type_args.len() < 2 {
                        return Err(VerseError::runtime_at(
                            "tuple type expects at least two element types",
                            expr.span,
                        ));
                    }
                    Ok(TypeName::Tuple(type_args))
                }
                "weak_map" => {
                    if type_args.len() != 2 {
                        return Err(VerseError::runtime_at(
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
                _ => Ok(TypeName::Applied {
                    name,
                    args: type_args,
                }),
            }
        }
        _ => Err(VerseError::runtime_at("expected type argument", expr.span)),
    }
}

fn is_failable_condition_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } => true,
        ExprKind::Unary {
            op: UnaryOp::Not,
            expr,
        } => is_failable_condition_expr(expr),
        ExprKind::Binary { left, op, right } => {
            is_failure_binary_op(*op)
                || is_failable_condition_expr(left)
                || is_failable_condition_expr(right)
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
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            is_failable_condition_expr(object)
        }
        ExprKind::Call { callee, .. } => is_failable_condition_expr(callee),
        ExprKind::Var { expr, .. } => is_failable_condition_expr(expr),
        _ => false,
    }
}

fn case_arms_have_wildcard(arms: &[CaseArm]) -> bool {
    arms.iter()
        .any(|arm| matches!(arm.pattern, CasePattern::Wildcard { .. }))
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

fn is_comparison_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

fn has_runtime_effect(effects: &[String], name: &str) -> bool {
    effects.iter().any(|effect| effect == name)
}

fn data_member_default_call_error(span: Span) -> VerseError {
    VerseError::runtime_at(
        "data-member default value can only call `<converges>` functions",
        span,
    )
}

fn class_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn field_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn runtime_access_level_from_specifiers(specifiers: &[String]) -> RuntimeAccessLevel {
    if field_has_specifier(specifiers, "public") {
        RuntimeAccessLevel::Public
    } else if field_has_specifier(specifiers, "protected") {
        RuntimeAccessLevel::Protected
    } else if field_has_specifier(specifiers, "private") {
        RuntimeAccessLevel::Private
    } else {
        RuntimeAccessLevel::Internal
    }
}

fn ensure_runtime_interface_required_fields_initializable(
    class_name: &str,
    class_specifiers: &[String],
    fields: &[RuntimeClassField],
    span: Span,
) -> Result<(), VerseError> {
    let class_is_public = class_has_specifier(class_specifiers, "public");
    for field in fields {
        if field.default.is_some() || field.owner.is_none() {
            continue;
        }
        let inaccessible_from_constructor = matches!(
            field.access,
            RuntimeAccessLevel::Private | RuntimeAccessLevel::Protected
        ) || (field.access == RuntimeAccessLevel::Internal
            && class_is_public);
        if inaccessible_from_constructor {
            return Err(VerseError::runtime_at(
                format!(
                    "class `{class_name}` must be `abstract` or provide a default value for interface field `{}`",
                    field.name
                ),
                span,
            ));
        }
    }
    Ok(())
}

fn tuple_params_have_named_or_default(params: &[Param]) -> bool {
    params.iter().any(|param| {
        param.named
            || param.default.is_some()
            || matches!(
                &param.pattern,
                ParamPattern::Tuple(items) if tuple_params_have_named_or_default(items)
            )
    })
}

fn function_call_values(
    params: &[Param],
    args: Vec<Value>,
    env: &Env,
    span: Span,
) -> Result<Vec<Value>, VerseError> {
    if let [param] = params
        && let Some(annotation) = param.annotation.as_ref()
    {
        let resolved = env.resolve_type_name(&annotation.name);
        if let Some(item_type) = array_type_name(&resolved) {
            if item_type.is_some_and(type_name_is_string_char)
                && args.len() == 1
                && matches!(&args[0], Value::String(_))
            {
                return Ok(args);
            }
            let value = if args.len() == 1 && matches!(&args[0], Value::Array(_) | Value::Tuple(_))
            {
                coerce_value_to_type_name(env, &resolved, args.into_iter().next().unwrap())
            } else {
                Value::Array(Rc::new(RefCell::new(
                    args.into_iter()
                        .map(|arg| match item_type {
                            Some(item_type) => coerce_value_to_type_name(env, item_type, arg),
                            None => arg,
                        })
                        .collect(),
                )))
            };
            return Ok(vec![value]);
        }
    }

    if let [param] = params
        && let Some(annotation) = param.annotation.as_ref()
    {
        let resolved = env.resolve_type_name(&annotation.name);
        if let Some(item_types) = tuple_type_name(&resolved)
            && !(args.len() == 1 && matches!(&args[0], Value::Tuple(_)))
        {
            if args.len() != item_types.len() {
                return Err(VerseError::runtime_at(
                    format!(
                        "expected {} arguments for tuple parameter, got {}",
                        item_types.len(),
                        args.len()
                    ),
                    span,
                ));
            }

            let values = args
                .into_iter()
                .zip(item_types)
                .map(|(arg, item_type)| coerce_value_to_type_name(env, item_type, arg))
                .collect();
            return Ok(vec![Value::Tuple(values)]);
        }
    }

    let args = match args.as_slice() {
        [Value::Tuple(items)] if items.len() == params.len() => {
            items.iter().map(value_copy).collect()
        }
        _ => args,
    };

    if args.len() != params.len() {
        return Err(VerseError::runtime_at(
            format!("expected {} arguments, got {}", params.len(), args.len()),
            span,
        ));
    }

    Ok(args)
}

fn coerce_annotated_value(env: &Env, annotation: Option<&TypeAnnotation>, value: Value) -> Value {
    coerce_value_to_type(env, annotation, value)
}

fn runtime_type_param_constraints(params: &[Param]) -> HashMap<String, TypeParamConstraint> {
    let mut constraints = HashMap::new();
    collect_runtime_type_param_constraints(params, &mut constraints);
    constraints
}

fn collect_runtime_type_param_constraints(
    params: &[Param],
    constraints: &mut HashMap<String, TypeParamConstraint>,
) {
    for param in params {
        for type_param in &param.type_params {
            constraints.insert(type_param.name.clone(), type_param.constraint.clone());
        }
        if let ParamPattern::Tuple(items) = &param.pattern {
            collect_runtime_type_param_constraints(items, constraints);
        }
    }
}

fn runtime_annotation_has_unresolved_type_params(
    type_name: &TypeName,
    type_params: &HashMap<String, TypeParamConstraint>,
    env: &Env,
) -> bool {
    match type_name {
        TypeName::Named(name) if type_params.contains_key(name) => {
            env.get_local_type_alias(name).is_none()
        }
        TypeName::Array(item) => item.as_deref().is_some_and(|item| {
            runtime_annotation_has_unresolved_type_params(item, type_params, env)
        }),
        TypeName::Map(key, value) | TypeName::WeakMap(key, value) => {
            runtime_annotation_has_unresolved_type_params(key, type_params, env)
                || runtime_annotation_has_unresolved_type_params(value, type_params, env)
        }
        TypeName::Tuple(items) => items
            .iter()
            .any(|item| runtime_annotation_has_unresolved_type_params(item, type_params, env)),
        TypeName::Option(item) => {
            runtime_annotation_has_unresolved_type_params(item, type_params, env)
        }
        TypeName::FunctionSignature {
            params,
            return_type,
            ..
        } => {
            params
                .iter()
                .any(|param| runtime_annotation_has_unresolved_type_params(param, type_params, env))
                || runtime_annotation_has_unresolved_type_params(return_type, type_params, env)
        }
        TypeName::Applied { args, .. } => args
            .iter()
            .any(|arg| runtime_annotation_has_unresolved_type_params(arg, type_params, env)),
        _ => false,
    }
}

fn runtime_type_name_for_value(value: &Value, env: &Env) -> Option<TypeName> {
    match value {
        Value::Int(_) => Some(TypeName::Int),
        Value::Float(_) => Some(TypeName::Float),
        Value::Rational(_) => Some(TypeName::Rational),
        Value::Bool(_) => Some(TypeName::Bool),
        Value::String(_) => Some(TypeName::String),
        Value::Diagnostic(_) => Some(TypeName::Named("diagnostic".to_string())),
        Value::None => Some(TypeName::None),
        Value::Char(_) => Some(TypeName::Char),
        Value::Char32(_) => Some(TypeName::Char32),
        Value::Range { .. } => Some(TypeName::Named("range".to_string())),
        Value::EnumValue { enum_name, .. } => Some(TypeName::Named(enum_name.clone())),
        Value::StructInstance { struct_name, .. } => Some(TypeName::Named(struct_name.clone())),
        Value::ClassInstance { class_name, .. } => Some(TypeName::Named(class_name.clone())),
        Value::EnumType { name, .. }
        | Value::StructType { name, .. }
        | Value::ClassType { name, .. }
        | Value::InterfaceType { name, .. } => Some(TypeName::Named(name.clone())),
        Value::Array(items) => runtime_array_item_type_name(items.borrow().as_slice(), env)
            .map(|item| TypeName::Array(Some(Box::new(item)))),
        Value::Map(entries) => runtime_map_type_name(entries.borrow().as_slice(), env),
        Value::Tuple(items) => items
            .iter()
            .map(|item| runtime_type_name_for_value(item, env))
            .collect::<Option<Vec<_>>>()
            .map(TypeName::Tuple),
        Value::Option(Some(value)) => {
            runtime_type_name_for_value(value, env).map(|item| TypeName::Option(Box::new(item)))
        }
        Value::Option(None) => None,
        Value::Function { .. } | Value::BoundMethod { .. } | Value::NativeFunction { .. } => {
            Some(TypeName::Function)
        }
        Value::Task(_) => None,
        Value::Event { payload, .. } => Some(TypeName::Applied {
            name: "event".to_string(),
            args: payload.iter().cloned().collect(),
        }),
        Value::Awaitable { payload } => Some(TypeName::Applied {
            name: "awaitable".to_string(),
            args: payload.iter().cloned().collect(),
        }),
        Value::Signalable { payload } => Some(TypeName::Applied {
            name: "signalable".to_string(),
            args: vec![payload.clone()],
        }),
        Value::Subscribable { payload, .. } => Some(TypeName::Applied {
            name: "subscribable".to_string(),
            args: payload.iter().cloned().collect(),
        }),
        Value::Listenable { payload, .. } => Some(TypeName::Applied {
            name: "listenable".to_string(),
            args: payload.iter().cloned().collect(),
        }),
        Value::Generator { item_type, .. } => Some(TypeName::Applied {
            name: "generator".to_string(),
            args: item_type.iter().cloned().collect(),
        }),
        Value::CastableSubtype(item) => Some(TypeName::Applied {
            name: "castable_subtype".to_string(),
            args: vec![item.clone()],
        }),
        Value::ConcreteSubtype(item) => Some(TypeName::Applied {
            name: "concrete_subtype".to_string(),
            args: vec![item.clone()],
        }),
        Value::ClassifiableSubset(_) => None,
        Value::Modifier { item_type } => Some(TypeName::Applied {
            name: "modifier".to_string(),
            args: vec![item_type.clone()],
        }),
        Value::ModifierStack { item_type, .. } => Some(TypeName::Applied {
            name: "modifier_stack".to_string(),
            args: vec![item_type.clone()],
        }),
        Value::Result { succeeded, value } => {
            let item = runtime_type_name_for_value(value, env)?;
            Some(TypeName::Applied {
                name: "result".to_string(),
                args: if *succeeded {
                    vec![item, TypeName::Any]
                } else {
                    vec![TypeName::Any, item]
                },
            })
        }
        Value::Module { name, .. } => Some(TypeName::Named(name.clone())),
        Value::Session => Some(TypeName::Named("session".to_string())),
        Value::External
        | Value::Overload(_)
        | Value::Pending
        | Value::Suspended(_)
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. }
        | Value::ParametricType { .. }
        | Value::ModifierCancelHandle { .. }
        | Value::SubscriptionCancelHandle { .. } => None,
    }
}

fn runtime_array_item_type_name(items: &[Value], env: &Env) -> Option<TypeName> {
    let mut inferred: Option<TypeName> = None;
    for item in items {
        let item_type = runtime_type_name_for_value(item, env)?;
        inferred = Some(match inferred {
            Some(current) => merge_runtime_type_names(&current, &item_type, env)?,
            None => item_type,
        });
    }
    inferred
}

fn runtime_map_type_name(entries: &[(Value, Value)], env: &Env) -> Option<TypeName> {
    let mut key_type: Option<TypeName> = None;
    let mut value_type: Option<TypeName> = None;
    for (key, value) in entries {
        let next_key = runtime_type_name_for_value(key, env)?;
        let next_value = runtime_type_name_for_value(value, env)?;
        key_type = Some(match key_type {
            Some(current) => merge_runtime_type_names(&current, &next_key, env)?,
            None => next_key,
        });
        value_type = Some(match value_type {
            Some(current) => merge_runtime_type_names(&current, &next_value, env)?,
            None => next_value,
        });
    }
    Some(TypeName::Map(Box::new(key_type?), Box::new(value_type?)))
}

fn merge_runtime_type_names(current: &TypeName, next: &TypeName, env: &Env) -> Option<TypeName> {
    if current == next {
        return Some(current.clone());
    }
    if runtime_type_names_assignable(next, current) {
        return Some(current.clone());
    }
    if runtime_type_names_assignable(current, next) {
        return Some(next.clone());
    }
    if let (TypeName::Named(left), TypeName::Named(right)) = (current, next) {
        if runtime_class_is_subtype(left, right, env)
            || runtime_interface_is_subtype(left, right, env)
        {
            return Some(next.clone());
        }
        if runtime_class_is_subtype(right, left, env)
            || runtime_interface_is_subtype(right, left, env)
        {
            return Some(current.clone());
        }
    }
    None
}

fn runtime_type_name_satisfies_constraint(
    actual: &TypeName,
    value: &Value,
    expected: &TypeName,
    env: &Env,
) -> bool {
    match expected {
        TypeName::Any => true,
        TypeName::Comparable => runtime_value_is_comparable(value),
        _ if runtime_type_names_assignable(actual, expected) => true,
        TypeName::Named(expected_name) => match actual {
            TypeName::Named(actual_name) => {
                runtime_class_is_subtype(actual_name, expected_name, env)
                    || runtime_class_implements_interface(actual_name, expected_name, env)
                    || runtime_interface_is_subtype(actual_name, expected_name, env)
            }
            _ => false,
        },
        _ => false,
    }
}

fn should_coerce_class_type_for_annotation(env: &Env, annotation: Option<&TypeAnnotation>) -> bool {
    annotation.is_some_and(|annotation| {
        matches!(
            env.resolve_type_name(&annotation.name),
            TypeName::Applied { name, args }
                if matches!(name.as_str(), "castable_subtype" | "concrete_subtype")
                    && args.len() == 1
        )
    })
}

fn runtime_value_matches_annotation(
    value: &Value,
    annotation: Option<&TypeAnnotation>,
    env: &Env,
) -> bool {
    let Some(annotation) = annotation else {
        return true;
    };
    let resolved = env.resolve_type_name(&annotation.name);
    runtime_value_matches_type_name(value, &resolved, env)
}

fn runtime_type_match_score(
    value: &Value,
    annotation: Option<&TypeAnnotation>,
    env: &Env,
) -> Option<usize> {
    let Some(annotation) = annotation else {
        return Some(50);
    };
    let resolved = env.resolve_type_name(&annotation.name);
    if !runtime_value_matches_type_name(value, &resolved, env) {
        return None;
    }

    match (&resolved, value) {
        (TypeName::Int, Value::Int(_))
        | (TypeName::IntRange { .. }, Value::Int(_))
        | (TypeName::Float, Value::Float(_))
        | (TypeName::Rational, Value::Rational(_))
        | (TypeName::Bool, Value::Bool(_))
        | (TypeName::String, Value::String(_))
        | (TypeName::Message, Value::String(_))
        | (TypeName::Char, Value::Char(_))
        | (TypeName::Char32, Value::Char32(_))
        | (TypeName::None, Value::None) => Some(0),
        (TypeName::Float | TypeName::Rational, Value::Int(_)) => Some(10),
        (TypeName::Number, _) if runtime_number(value).is_some() => Some(20),
        (TypeName::Any | TypeName::Comparable, _) => Some(50),
        _ => Some(25),
    }
}

fn runtime_value_matches_type_name(value: &Value, type_name: &TypeName, env: &Env) -> bool {
    let resolved = env.resolve_type_name(type_name);
    match &resolved {
        TypeName::Any => true,
        TypeName::Comparable => runtime_value_is_comparable(value),
        TypeName::Int => runtime_value_is_int(value),
        TypeName::IntRange { min, max } => match value {
            Value::Int(value) => min <= value && value <= max,
            Value::External => true,
            _ => false,
        },
        TypeName::Float => matches!(value, Value::Int(_) | Value::Float(_)),
        TypeName::Rational => matches!(value, Value::Int(_) | Value::Rational(_)),
        TypeName::Number => runtime_number(value).is_some(),
        TypeName::Bool => matches!(value, Value::Bool(_)),
        TypeName::String => match value {
            Value::String(_) => true,
            Value::Array(items) => char_array_to_string(items.borrow().as_slice()).is_some(),
            _ => false,
        },
        TypeName::Message => matches!(value, Value::String(_)),
        TypeName::Char => matches!(value, Value::Char(_)),
        TypeName::Char8 => false,
        TypeName::Char32 => matches!(value, Value::Char32(_)),
        TypeName::None => matches!(value, Value::None),
        TypeName::Array(item_type) => match value {
            Value::String(_) if item_type.as_deref().is_some_and(type_name_is_string_char) => true,
            Value::Array(items) => item_type.as_deref().is_none_or(|item_type| {
                items
                    .borrow()
                    .iter()
                    .all(|item| runtime_value_matches_type_name(item, item_type, env))
            }),
            _ => false,
        },
        TypeName::Map(key_type, value_type) | TypeName::WeakMap(key_type, value_type) => {
            match value {
                Value::Map(entries) => entries.borrow().iter().all(|(key, value)| {
                    runtime_value_matches_type_name(key, key_type, env)
                        && runtime_value_matches_type_name(value, value_type, env)
                }),
                _ => false,
            }
        }
        TypeName::Tuple(item_types) => match value {
            Value::Tuple(items) if items.len() == item_types.len() => items
                .iter()
                .zip(item_types)
                .all(|(item, item_type)| runtime_value_matches_type_name(item, item_type, env)),
            _ => false,
        },
        TypeName::Option(item_type) => match value {
            Value::Option(Some(value)) => runtime_value_matches_type_name(value, item_type, env),
            Value::Option(None) | Value::Bool(false) => true,
            _ => false,
        },
        TypeName::Function | TypeName::FunctionSignature { .. } => {
            matches!(
                value,
                Value::Function { .. }
                    | Value::BoundMethod { .. }
                    | Value::NativeFunction { .. }
                    | Value::NativeResultMethod { .. }
                    | Value::NativeEventMethod { .. }
                    | Value::NativeSubscribableMethod { .. }
                    | Value::NativeTaskMethod { .. }
                    | Value::NativeModifierMethod { .. }
                    | Value::NativeCancelMethod { .. }
                    | Value::NativeSubscriptionCancelMethod { .. }
            )
        }
        TypeName::Applied { name, args } if name == "event" => match value {
            Value::External => true,
            Value::Event { payload, .. } => event_payload_matches_type_args(payload.as_ref(), args),
            _ => false,
        },
        TypeName::Applied { name, args } if name == "task" => {
            args.len() == 1
                && match value {
                    Value::External => true,
                    Value::Task(task) => task.matches_payload_type(&args[0], env),
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "generator" => {
            matches!(args.len(), 0 | 1)
                && match value {
                    Value::External => true,
                    Value::Generator { item_type, values } => {
                        generator_type_matches_args(item_type.as_ref(), args)
                            && args.first().is_none_or(|item_type| {
                                values.borrow().iter().all(|value| {
                                    runtime_value_matches_type_name(value, item_type, env)
                                })
                            })
                    }
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "castable_subtype" => {
            args.len() == 1
                && match value {
                    Value::External => true,
                    Value::CastableSubtype(item) => item == &args[0],
                    Value::ClassType {
                        name,
                        castable: true,
                        ..
                    } => runtime_class_type_conforms_to_type_name(name, &args[0], env),
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "concrete_subtype" => {
            args.len() == 1
                && match value {
                    Value::External => true,
                    Value::ConcreteSubtype(item) => item == &args[0],
                    Value::ClassType {
                        name,
                        concrete: true,
                        castable,
                        ..
                    } => runtime_class_type_satisfies_subtype_type_name(
                        name, *castable, &args[0], env,
                    ),
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "classifiable_subset" => {
            args.len() == 1
                && match value {
                    Value::External => true,
                    Value::ClassifiableSubset(items) => items
                        .borrow()
                        .iter()
                        .all(|item| classifiable_subset_item_matches(item, &args[0], env)),
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "modifier" => {
            args.len() == 1 && runtime_value_matches_modifier_type(value, &args[0], env)
        }
        TypeName::Applied { name, args } if name == "modifier_stack" => {
            args.len() == 1
                && match value {
                    Value::External => true,
                    Value::ModifierStack {
                        item_type, entries, ..
                    } => {
                        item_type == &args[0]
                            && entries.borrow().iter().all(|entry| {
                                runtime_value_matches_modifier_type(&entry.modifier, item_type, env)
                            })
                    }
                    _ => false,
                }
        }
        TypeName::Applied { name, args } if name == "awaitable" => match value {
            Value::External => true,
            Value::Awaitable { payload } => event_payload_matches_type_args(payload.as_ref(), args),
            Value::Event { payload, .. } => event_payload_matches_type_args(payload.as_ref(), args),
            Value::Listenable { payload, .. } => {
                event_payload_matches_type_args(payload.as_ref(), args)
            }
            Value::Task(task) if args.len() == 1 => task.matches_payload_type(&args[0], env),
            _ => false,
        },
        TypeName::Applied { name, args } if name == "signalable" => match value {
            Value::External => true,
            Value::Signalable { payload } if args.len() == 1 => payload == &args[0],
            Value::Event {
                payload: Some(payload),
                ..
            } if args.len() == 1 => payload == &args[0],
            _ => false,
        },
        TypeName::Applied { name, args } if name == "listenable" => match value {
            Value::External => true,
            Value::Listenable { payload, .. } => {
                event_payload_matches_type_args(payload.as_ref(), args)
            }
            _ => false,
        },
        TypeName::Applied { name, args } if name == "subscribable" => match value {
            Value::External => true,
            Value::Subscribable { payload, .. } | Value::Listenable { payload, .. } => {
                event_payload_matches_type_args(payload.as_ref(), args)
            }
            _ => false,
        },
        TypeName::Applied { name, args } if name == "result" && args.len() == 2 => match value {
            Value::External => true,
            Value::Result { succeeded, value } => {
                let value_type = if *succeeded { &args[0] } else { &args[1] };
                runtime_value_matches_type_name(value, value_type, env)
            }
            _ => false,
        },
        TypeName::Applied { name, args } => match value {
            Value::External => true,
            _ => runtime_value_matches_named_type(
                value,
                &render_runtime_parametric_type_name(name, args),
                env,
            ),
        },
        TypeName::Named(name) => runtime_value_matches_named_type(value, name, env),
    }
}

fn event_payload_matches_type_args(payload: Option<&TypeName>, args: &[TypeName]) -> bool {
    match args {
        [] => payload.is_none(),
        [expected] => payload.is_some_and(|payload| payload == expected),
        _ => false,
    }
}

fn generator_type_matches_args(payload: Option<&TypeName>, args: &[TypeName]) -> bool {
    match args {
        [] => payload.is_none(),
        [expected] => payload.is_none_or(|payload| payload == expected),
        _ => false,
    }
}

fn classifiable_subset_item_matches(item: &Value, element_type: &TypeName, env: &Env) -> bool {
    match item {
        Value::CastableSubtype(item_type) => item_type == element_type,
        _ => runtime_value_matches_type_name(item, element_type, env),
    }
}

fn runtime_value_matches_modifier_type(value: &Value, item_type: &TypeName, env: &Env) -> bool {
    match value {
        Value::External => true,
        Value::Modifier {
            item_type: modifier_type,
        } => modifier_type == item_type,
        Value::ModifierStack {
            item_type: stack_type,
            ..
        } => stack_type == item_type,
        Value::ClassInstance { methods, .. } => methods.iter().any(|method| {
            method.name == "Evaluate"
                && method.params.len() == 1
                && method.params[0]
                    .annotation
                    .as_ref()
                    .is_none_or(|annotation| {
                        let param_type = env.resolve_type_name(&annotation.name);
                        runtime_type_names_assignable(&param_type, item_type)
                    })
        }),
        _ => false,
    }
}

fn runtime_type_names_assignable(actual: &TypeName, expected: &TypeName) -> bool {
    actual == expected
        || matches!(expected, TypeName::Any)
        || matches!(
            (expected, actual),
            (TypeName::Rational, TypeName::Int)
                | (TypeName::Float, TypeName::Int)
                | (
                    TypeName::Number,
                    TypeName::Int | TypeName::Float | TypeName::Rational
                )
                | (TypeName::Message, TypeName::String)
        )
}

fn runtime_value_is_int(value: &Value) -> bool {
    match value {
        Value::Int(_) => true,
        Value::Float(_) | Value::Rational(_) => false,
        _ => false,
    }
}

fn is_builtin_length_receiver_value(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Map(_) | Value::String(_))
}

fn runtime_value_is_comparable(value: &Value) -> bool {
    match value {
        Value::Int(_)
        | Value::Float(_)
        | Value::Rational(_)
        | Value::Char(_)
        | Value::Char32(_)
        | Value::Bool(_)
        | Value::String(_)
        | Value::None
        | Value::Session
        | Value::EnumValue { .. } => true,
        Value::StructInstance { .. } => true,
        Value::ClassInstance { unique, .. } => *unique,
        Value::Array(items) => items.borrow().iter().all(runtime_value_is_comparable),
        Value::Map(entries) => entries.borrow().iter().all(|(key, value)| {
            runtime_value_is_comparable(key) && runtime_value_is_comparable(value)
        }),
        Value::Tuple(items) => items.iter().all(runtime_value_is_comparable),
        Value::Option(Some(value)) => runtime_value_is_comparable(value),
        Value::Option(None) => true,
        Value::Result { .. } => false,
        Value::Pending | Value::Suspended(_) => false,
        Value::Event { .. } => false,
        Value::Awaitable { .. } => false,
        Value::Signalable { .. } => false,
        Value::Subscribable { .. } => false,
        Value::Listenable { .. } => false,
        Value::SubscriptionCancelHandle { .. } => false,
        Value::Task(_) => false,
        Value::Generator { .. } => false,
        Value::Modifier { .. }
        | Value::ModifierStack { .. }
        | Value::ModifierCancelHandle { .. }
        | Value::CastableSubtype(_)
        | Value::ConcreteSubtype(_)
        | Value::ClassifiableSubset(_) => false,
        Value::Function { .. }
        | Value::Overload(_)
        | Value::NativeFunction { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. }
        | Value::BoundMethod { .. }
        | Value::Range { .. }
        | Value::Diagnostic(_)
        | Value::External
        | Value::EnumType { .. }
        | Value::StructType { .. }
        | Value::ClassType { .. }
        | Value::InterfaceType { .. }
        | Value::ParametricType { .. }
        | Value::Module { .. } => false,
    }
}

fn runtime_value_matches_named_type(value: &Value, name: &str, env: &Env) -> bool {
    let local_name = name.rsplit('.').next().unwrap_or(name);
    match value {
        Value::External => true,
        Value::Diagnostic(_) => local_name == "diagnostic",
        Value::EnumValue { enum_name, .. } => enum_name == name || enum_name == local_name,
        Value::StructInstance { struct_name, .. } => {
            struct_name == name || struct_name == local_name
        }
        Value::ClassInstance { class_name, .. } => {
            runtime_class_instance_conforms_to(class_name, name, env)
        }
        Value::EnumType {
            name: type_name, ..
        }
        | Value::StructType {
            name: type_name, ..
        }
        | Value::ClassType {
            name: type_name, ..
        }
        | Value::Module {
            name: type_name, ..
        } => type_name == name || type_name == local_name,
        Value::ModifierCancelHandle { .. } | Value::SubscriptionCancelHandle { .. } => {
            local_name == "cancelable"
        }
        _ => false,
    }
}

fn runtime_class_instance_conforms_to(actual: &str, expected: &str, env: &Env) -> bool {
    if runtime_names_match(actual, expected) {
        return true;
    }
    if runtime_class_is_subtype(actual, expected, env)
        || runtime_class_implements_interface(actual, expected, env)
    {
        return true;
    }
    if runtime_builtin_class_base_name(expected) {
        return runtime_class_is_subtype(actual, expected, env);
    }

    match runtime_named_type_value(expected, env) {
        Some(Value::ClassType { name, .. }) => runtime_class_is_subtype(actual, &name, env),
        Some(Value::InterfaceType { name, .. }) => {
            runtime_class_implements_interface(actual, &name, env)
        }
        _ => false,
    }
}

fn runtime_class_type_conforms_to_type_name(actual: &str, expected: &TypeName, env: &Env) -> bool {
    match env.resolve_type_name(expected) {
        TypeName::Any => true,
        TypeName::Named(expected) => runtime_class_instance_conforms_to(actual, &expected, env),
        _ => false,
    }
}

fn runtime_class_type_satisfies_subtype_type_name(
    actual: &str,
    castable: bool,
    expected: &TypeName,
    env: &Env,
) -> bool {
    match env.resolve_type_name(expected) {
        TypeName::Applied { name, args } if name == "castable_subtype" && args.len() == 1 => {
            castable && runtime_class_type_conforms_to_type_name(actual, &args[0], env)
        }
        resolved => runtime_class_type_conforms_to_type_name(actual, &resolved, env),
    }
}

fn runtime_class_is_subtype(actual: &str, expected: &str, env: &Env) -> bool {
    if runtime_builtin_class_is_subtype(actual, expected) {
        return true;
    }

    let mut current = Some(actual.to_string());
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(name.clone()) {
            return false;
        }
        if runtime_names_match(&name, expected) {
            return true;
        }
        current = match runtime_named_type_value(&name, env) {
            Some(Value::ClassType { base, .. }) => base,
            _ => None,
        };
    }
    false
}

fn runtime_class_implements_interface(actual: &str, expected: &str, env: &Env) -> bool {
    let mut current = Some(actual.to_string());
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(name.clone()) {
            return false;
        }
        let Some(Value::ClassType {
            base, interfaces, ..
        }) = runtime_named_type_value(&name, env)
        else {
            return false;
        };
        if interfaces
            .iter()
            .any(|interface| runtime_interface_is_subtype(interface, expected, env))
        {
            return true;
        }
        current = base;
    }
    false
}

fn runtime_interface_is_subtype(actual: &str, expected: &str, env: &Env) -> bool {
    if runtime_names_match(actual, expected) {
        return true;
    }

    let mut seen = HashSet::new();
    runtime_interface_is_subtype_inner(actual, expected, env, &mut seen)
}

fn runtime_interface_is_subtype_inner(
    actual: &str,
    expected: &str,
    env: &Env,
    seen: &mut HashSet<String>,
) -> bool {
    if !seen.insert(actual.to_string()) {
        return false;
    }
    let Some(Value::InterfaceType { parents, .. }) = runtime_named_type_value(actual, env) else {
        return false;
    };
    parents.iter().any(|parent| {
        runtime_names_match(parent, expected)
            || runtime_interface_is_subtype_inner(parent, expected, env, seen)
    })
}

fn runtime_named_type_value(name: &str, env: &Env) -> Option<Value> {
    env.get_qualified_path(name).or_else(|| {
        let local_name = name.rsplit('.').next().unwrap_or(name);
        (local_name != name).then(|| env.get(local_name)).flatten()
    })
}

fn runtime_class_definition_diagnostic_span(
    base: Option<&TypeAnnotation>,
    fields: &[StructField],
    methods: &[ClassMethod],
    blocks: &[ClassBlock],
) -> Span {
    base.map_or_else(
        || {
            fields
                .first()
                .map(|field| field.span)
                .or_else(|| methods.first().map(|method| method.span))
                .or_else(|| blocks.first().map(|block| block.span))
                .unwrap_or_else(|| Span::new(0, 0, 1, 1))
        },
        |base| base.span,
    )
}

fn runtime_builtin_class_base_name(name: &str) -> bool {
    matches!(name.rsplit('.').next().unwrap_or(name), "component" | "tag")
}

fn runtime_builtin_class_type(name: &str) -> Option<Value> {
    let local_name = name.rsplit('.').next().unwrap_or(name);
    if !runtime_builtin_class_base_name(local_name) {
        return None;
    }
    Some(Value::ClassType {
        name: local_name.to_string(),
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        fields: Vec::new(),
        methods: Vec::new(),
        blocks: Vec::new(),
    })
}

fn runtime_names_match(actual: &str, expected: &str) -> bool {
    let actual_local = actual.rsplit('.').next().unwrap_or(actual);
    let expected_local = expected.rsplit('.').next().unwrap_or(expected);
    actual == expected
        || actual == expected_local
        || actual_local == expected
        || actual_local == expected_local
}

fn runtime_builtin_class_is_subtype(actual: &str, expected: &str) -> bool {
    let actual = actual.rsplit('.').next().unwrap_or(actual);
    let expected = expected.rsplit('.').next().unwrap_or(expected);
    matches!(
        (actual, expected),
        ("player", "agent") | ("agent", "entity") | ("player", "entity")
    )
}

fn render_runtime_parametric_type_name(name: &str, args: &[TypeName]) -> String {
    if args.is_empty() {
        return format!("{name}()");
    }

    let args = args
        .iter()
        .map(render_runtime_type_name)
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}({args})")
}

fn render_runtime_type_name(type_name: &TypeName) -> String {
    match type_name {
        TypeName::Int => "int".to_string(),
        TypeName::Float => "float".to_string(),
        TypeName::Rational => "rational".to_string(),
        TypeName::Number => "number".to_string(),
        TypeName::Bool => "logic".to_string(),
        TypeName::String => "string".to_string(),
        TypeName::Message => "message".to_string(),
        TypeName::Char => "char".to_string(),
        TypeName::Char8 => "char8".to_string(),
        TypeName::Char32 => "char32".to_string(),
        TypeName::None => "void".to_string(),
        TypeName::Any => "any".to_string(),
        TypeName::Comparable => "comparable".to_string(),
        TypeName::IntRange { min, max } => format!("int_range({min},{max})"),
        TypeName::Array(None) => "array".to_string(),
        TypeName::Array(Some(item)) => format!("[]{}", render_runtime_type_name(item)),
        TypeName::Map(key, value) => format!(
            "[{}]{}",
            render_runtime_type_name(key),
            render_runtime_type_name(value)
        ),
        TypeName::WeakMap(key, value) => format!(
            "weak_map({},{})",
            render_runtime_type_name(key),
            render_runtime_type_name(value)
        ),
        TypeName::Tuple(items) => {
            let items = items
                .iter()
                .map(render_runtime_type_name)
                .collect::<Vec<_>>()
                .join(",");
            format!("tuple({items})")
        }
        TypeName::Option(item) => format!("?{}", render_runtime_type_name(item)),
        TypeName::Function => "function".to_string(),
        TypeName::FunctionSignature { .. } => "type{...}".to_string(),
        TypeName::Applied { name, args } => render_runtime_parametric_type_name(name, args),
        TypeName::Named(name) => name.clone(),
    }
}

fn coerce_value_to_type(env: &Env, annotation: Option<&TypeAnnotation>, value: Value) -> Value {
    if let Some(annotation) = annotation {
        let resolved = env.resolve_type_name(&annotation.name);
        coerce_value_to_type_name(env, &resolved, value)
    } else {
        value
    }
}

fn coerce_value_to_type_name(env: &Env, type_name: &TypeName, value: Value) -> Value {
    let resolved = env.resolve_type_name(type_name);
    let type_name = &resolved;

    if let TypeName::Array(item_type) = type_name {
        if matches!(value, Value::String(_))
            && item_type.as_deref().is_some_and(type_name_is_string_char)
        {
            return value;
        }
        return coerce_array_value(env, item_type.as_deref(), value);
    }

    match type_name {
        TypeName::Option(item_type) => match value {
            Value::Bool(false) => Value::Option(None),
            Value::Option(Some(value)) => Value::Option(Some(Box::new(coerce_value_to_type_name(
                env, item_type, *value,
            )))),
            other => other,
        },
        TypeName::Map(key_type, value_type) | TypeName::WeakMap(key_type, value_type) => {
            coerce_map_value(env, key_type, value_type, value)
        }
        TypeName::Tuple(item_types) => coerce_tuple_value(env, item_types, value),
        TypeName::String => coerce_string_value(value),
        TypeName::Int => coerce_int_value(value),
        TypeName::Float => coerce_float_value(value),
        TypeName::Rational => coerce_rational_value(value),
        TypeName::Applied { name, args } if name == "castable_subtype" && args.len() == 1 => {
            match value {
                Value::External => Value::CastableSubtype(args[0].clone()),
                Value::ClassType {
                    name,
                    castable: true,
                    ..
                } if runtime_class_type_conforms_to_type_name(&name, &args[0], env) => {
                    Value::CastableSubtype(args[0].clone())
                }
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "concrete_subtype" && args.len() == 1 => {
            match value {
                Value::External => Value::ConcreteSubtype(args[0].clone()),
                Value::ClassType {
                    name,
                    concrete: true,
                    castable,
                    ..
                } if runtime_class_type_satisfies_subtype_type_name(
                    &name, castable, &args[0], env,
                ) =>
                {
                    Value::ConcreteSubtype(args[0].clone())
                }
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "generator" && matches!(args.len(), 0 | 1) => {
            match value {
                Value::External => Value::Generator {
                    item_type: args.first().cloned(),
                    values: Rc::new(RefCell::new(Vec::new())),
                },
                Value::Generator { item_type, values } => Value::Generator {
                    item_type: item_type.or_else(|| args.first().cloned()),
                    values,
                },
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "event" && matches!(args.len(), 0 | 1) => {
            match value {
                Value::External => event_value(args.first().cloned()),
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "awaitable" && matches!(args.len(), 0 | 1) => {
            match value {
                Value::External => Value::Awaitable {
                    payload: args.first().cloned(),
                },
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "signalable" && args.len() == 1 => {
            match value {
                Value::External => Value::Signalable {
                    payload: args[0].clone(),
                },
                other => other,
            }
        }
        TypeName::Applied { name, args }
            if name == "subscribable" && matches!(args.len(), 0 | 1) =>
        {
            match value {
                Value::External => Value::Subscribable {
                    payload: args.first().cloned(),
                    subscribers: Rc::new(RefCell::new(Vec::new())),
                    next_subscriber_id: Rc::new(RefCell::new(0)),
                },
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "listenable" && matches!(args.len(), 0 | 1) => {
            match value {
                Value::External => Value::Listenable {
                    payload: args.first().cloned(),
                    subscribers: Rc::new(RefCell::new(Vec::new())),
                    next_subscriber_id: Rc::new(RefCell::new(0)),
                },
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "classifiable_subset" && args.len() == 1 => {
            match value {
                Value::External => Value::ClassifiableSubset(Rc::new(RefCell::new(Vec::new()))),
                other => other,
            }
        }
        TypeName::Applied { name, args } if name == "modifier" && args.len() == 1 => match value {
            Value::External => Value::Modifier {
                item_type: args[0].clone(),
            },
            other => other,
        },
        TypeName::Applied { name, args } if name == "modifier_stack" && args.len() == 1 => {
            match value {
                Value::External => Value::ModifierStack {
                    item_type: args[0].clone(),
                    entries: Rc::new(RefCell::new(Vec::new())),
                    next_order: Rc::new(RefCell::new(0)),
                },
                other => other,
            }
        }
        _ => value,
    }
}

fn coerce_int_value(value: Value) -> Value {
    value
}

fn coerce_float_value(value: Value) -> Value {
    match value {
        Value::Int(value) => Value::Float(value as f64),
        other => other,
    }
}

fn coerce_rational_value(value: Value) -> Value {
    match value {
        Value::Int(value) => Value::Rational(RationalValue::from_int(value)),
        other => other,
    }
}

fn coerce_string_value(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let converted = {
                let items = items.borrow();
                char_array_to_string(items.as_slice())
            };
            converted.map(Value::String).unwrap_or(Value::Array(items))
        }
        other => other,
    }
}

fn coerce_array_value(env: &Env, item_type: Option<&TypeName>, value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let Some(item_type) = item_type else {
                return Value::Array(items);
            };
            array_value(
                items
                    .borrow()
                    .iter()
                    .map(|item| coerce_value_to_type_name(env, item_type, value_copy(item)))
                    .collect(),
            )
        }
        Value::Tuple(items) => Value::Array(Rc::new(RefCell::new(
            items
                .into_iter()
                .map(|item| match item_type {
                    Some(item_type) => coerce_value_to_type_name(env, item_type, item),
                    None => value_copy(&item),
                })
                .collect(),
        ))),
        other => other,
    }
}

fn coerce_map_value(env: &Env, key_type: &TypeName, value_type: &TypeName, value: Value) -> Value {
    match value {
        Value::Map(entries) => Value::Map(Rc::new(RefCell::new(
            entries
                .borrow()
                .iter()
                .map(|(key, value)| {
                    (
                        coerce_value_to_type_name(env, key_type, value_copy(key)),
                        coerce_value_to_type_name(env, value_type, value_copy(value)),
                    )
                })
                .collect(),
        ))),
        other => other,
    }
}

fn coerce_tuple_value(env: &Env, item_types: &[TypeName], value: Value) -> Value {
    match value {
        Value::Tuple(items) if items.len() == item_types.len() => Value::Tuple(
            items
                .into_iter()
                .zip(item_types)
                .map(|(item, item_type)| coerce_value_to_type_name(env, item_type, item))
                .collect(),
        ),
        other => other,
    }
}

fn tuple_value_to_array(value: Value) -> Value {
    match value {
        Value::Tuple(items) => array_value(items.iter().map(value_copy).collect()),
        other => other,
    }
}

fn value_copy(value: &Value) -> Value {
    match value {
        Value::Int(value) => Value::Int(*value),
        Value::Float(value) => Value::Float(*value),
        Value::Rational(value) => Value::Rational(*value),
        Value::Char(value) => Value::Char(*value),
        Value::Char32(value) => Value::Char32(*value),
        Value::Bool(value) => Value::Bool(*value),
        Value::String(value) => Value::String(value.clone()),
        Value::Diagnostic(value) => Value::Diagnostic(value.clone()),
        Value::External => Value::External,
        Value::None => Value::None,
        Value::Pending => Value::Pending,
        Value::Suspended(suspension) => Value::Suspended(suspension.clone()),
        Value::Session => Value::Session,
        Value::Range { start, end } => Value::Range {
            start: *start,
            end: *end,
        },
        Value::EnumType {
            name,
            variants,
            open,
        } => Value::EnumType {
            name: name.clone(),
            variants: variants.clone(),
            open: *open,
        },
        Value::EnumValue { enum_name, variant } => Value::EnumValue {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
        },
        Value::StructType {
            name,
            computes,
            fields,
        } => Value::StructType {
            name: name.clone(),
            computes: *computes,
            fields: fields
                .iter()
                .map(|field| RuntimeStructField {
                    name: field.name.clone(),
                    default: field.default.as_ref().map(value_copy),
                })
                .collect(),
        },
        Value::StructInstance {
            struct_name,
            computes,
            fields,
        } => Value::StructInstance {
            struct_name: struct_name.clone(),
            computes: *computes,
            fields: fields
                .iter()
                .map(|(name, value)| (name.clone(), value_copy(value)))
                .collect(),
        },
        Value::ClassType {
            name,
            base,
            interfaces,
            unique,
            abstract_class,
            epic_internal_class,
            final_class,
            concrete,
            castable,
            fields,
            methods,
            blocks,
        } => Value::ClassType {
            name: name.clone(),
            base: base.clone(),
            interfaces: interfaces.clone(),
            unique: *unique,
            abstract_class: *abstract_class,
            epic_internal_class: *epic_internal_class,
            final_class: *final_class,
            concrete: *concrete,
            castable: *castable,
            fields: fields
                .iter()
                .map(|field| RuntimeClassField {
                    name: field.name.clone(),
                    mutable: field.mutable,
                    final_member: field.final_member,
                    access: field.access,
                    owner: field.owner.clone(),
                    default: field.default.as_ref().map(value_copy),
                })
                .collect(),
            methods: methods.clone(),
            blocks: blocks.clone(),
        },
        Value::InterfaceType {
            name,
            parents,
            fields,
            methods,
        } => Value::InterfaceType {
            name: name.clone(),
            parents: parents.clone(),
            fields: fields
                .iter()
                .map(|field| RuntimeClassField {
                    name: field.name.clone(),
                    mutable: field.mutable,
                    final_member: field.final_member,
                    access: field.access,
                    owner: field.owner.clone(),
                    default: field.default.as_ref().map(value_copy),
                })
                .collect(),
            methods: methods.clone(),
        },
        Value::Module { name, env } => Value::Module {
            name: name.clone(),
            env: env.clone(),
        },
        Value::ClassInstance { .. } => value.clone(),
        Value::Array(items) => array_value(items.borrow().iter().map(value_copy).collect()),
        Value::Map(entries) => Value::Map(Rc::new(RefCell::new(
            entries
                .borrow()
                .iter()
                .map(|(key, value)| (value_copy(key), value_copy(value)))
                .collect(),
        ))),
        Value::Tuple(items) => Value::Tuple(items.iter().map(value_copy).collect()),
        Value::Option(Some(value)) => Value::Option(Some(Box::new(value_copy(value)))),
        Value::Option(None) => Value::Option(None),
        Value::Result { succeeded, value } => Value::Result {
            succeeded: *succeeded,
            value: Box::new(value_copy(value)),
        },
        Value::Event { payload, waiters } => Value::Event {
            payload: payload.clone(),
            waiters: waiters.clone(),
        },
        Value::Awaitable { payload } => Value::Awaitable {
            payload: payload.clone(),
        },
        Value::Signalable { payload } => Value::Signalable {
            payload: payload.clone(),
        },
        Value::Subscribable {
            payload,
            subscribers,
            next_subscriber_id,
        } => Value::Subscribable {
            payload: payload.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::Listenable {
            payload,
            subscribers,
            next_subscriber_id,
        } => Value::Listenable {
            payload: payload.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::SubscriptionCancelHandle {
            subscribers,
            subscriber_id,
        } => Value::SubscriptionCancelHandle {
            subscribers: subscribers.clone(),
            subscriber_id: *subscriber_id,
        },
        Value::Task(task) => Value::Task(task.clone()),
        Value::Generator { item_type, values } => Value::Generator {
            item_type: item_type.clone(),
            values: Rc::new(RefCell::new(
                values.borrow().iter().map(value_copy).collect(),
            )),
        },
        Value::Modifier { item_type } => Value::Modifier {
            item_type: item_type.clone(),
        },
        Value::ModifierStack {
            item_type,
            entries,
            next_order,
        } => Value::ModifierStack {
            item_type: item_type.clone(),
            entries: entries.clone(),
            next_order: next_order.clone(),
        },
        Value::ModifierCancelHandle { entries, entry_id } => Value::ModifierCancelHandle {
            entries: entries.clone(),
            entry_id: *entry_id,
        },
        Value::CastableSubtype(item) => Value::CastableSubtype(item.clone()),
        Value::ConcreteSubtype(item) => Value::ConcreteSubtype(item.clone()),
        Value::ClassifiableSubset(items) => Value::ClassifiableSubset(Rc::new(RefCell::new(
            items.borrow().iter().map(value_copy).collect(),
        ))),
        Value::ParametricType {
            name,
            params,
            body,
            closure,
        } => Value::ParametricType {
            name: name.clone(),
            params: params.clone(),
            body: body.clone(),
            closure: closure.clone(),
        },
        Value::Overload(overloads) => {
            Value::Overload(overloads.iter().map(value_copy).collect::<Vec<_>>())
        }
        Value::Function { .. }
        | Value::BoundMethod { .. }
        | Value::NativeFunction { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. } => value.clone(),
    }
}

fn qualify_runtime_named_value(value: Value, qualified_name: &str) -> Value {
    match value {
        Value::EnumType { variants, open, .. } => Value::EnumType {
            name: qualified_name.to_string(),
            variants,
            open,
        },
        Value::StructType {
            computes, fields, ..
        } => Value::StructType {
            name: qualified_name.to_string(),
            computes,
            fields,
        },
        Value::ClassType {
            base,
            interfaces,
            unique,
            abstract_class,
            epic_internal_class,
            final_class,
            concrete,
            castable,
            fields,
            methods,
            blocks,
            ..
        } => Value::ClassType {
            name: qualified_name.to_string(),
            base,
            interfaces,
            unique,
            abstract_class,
            epic_internal_class,
            final_class,
            concrete,
            castable,
            fields,
            methods,
            blocks,
        },
        Value::InterfaceType {
            parents,
            fields,
            methods,
            ..
        } => Value::InterfaceType {
            name: qualified_name.to_string(),
            parents,
            fields: qualify_runtime_interface_fields(qualified_name, fields),
            methods: qualify_runtime_interface_methods(qualified_name, methods),
        },
        Value::ParametricType {
            params,
            body,
            closure,
            ..
        } => Value::ParametricType {
            name: qualified_name.to_string(),
            params,
            body,
            closure,
        },
        Value::Module { env, .. } => Value::Module {
            name: qualified_name.to_string(),
            env,
        },
        other => other,
    }
}

fn string_char_values(value: &str) -> Vec<Value> {
    value
        .as_bytes()
        .iter()
        .map(|byte| Value::Char(char::from(*byte)))
        .collect()
}

fn char_value_to_byte(value: &Value) -> Option<u8> {
    match value {
        Value::Char(value) => u8::try_from(*value as u32).ok(),
        _ => None,
    }
}

fn char_array_to_string(items: &[Value]) -> Option<String> {
    let bytes = items
        .iter()
        .map(char_value_to_byte)
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn string_equals_char_array(text: &str, items: &[Value]) -> bool {
    text.len() == items.len()
        && items
            .iter()
            .zip(text.as_bytes())
            .all(|(item, byte)| char_value_to_byte(item).is_some_and(|item| item == *byte))
}

fn expect_string_index(value: &Value, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, "string index", span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("string index cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}

fn string_index_value(text: &str, index: &Value, span: Span) -> Result<Value, VerseError> {
    let index = expect_string_index(index, span)?;
    text.as_bytes()
        .get(index)
        .map(|byte| Value::Char(char::from(*byte)))
        .ok_or_else(|| {
            VerseError::runtime_at(
                format!(
                    "string index {index} out of bounds for length {}",
                    text.len()
                ),
                span,
            )
        })
}

fn string_index_value_failable(
    text: &str,
    index: &Value,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    let index = expect_string_index(index, span)?;
    Ok(text
        .as_bytes()
        .get(index)
        .map(|byte| Value::Char(char::from(*byte))))
}

fn replace_string_byte(
    text: String,
    index: &Value,
    value: Value,
    span: Span,
) -> Result<String, VerseError> {
    let length = text.len();
    let Some(updated) = replace_string_byte_at(text, index, value, span)? else {
        let index = expect_string_index(index, span)?;
        return Err(VerseError::runtime_at(
            format!("string index {index} out of bounds for length {length}"),
            span,
        ));
    };
    Ok(updated)
}

fn replace_string_byte_failable(
    text: String,
    index: &Value,
    value: Value,
    span: Span,
) -> Result<Option<String>, VerseError> {
    replace_string_byte_at(text, index, value, span)
}

fn replace_string_byte_at(
    text: String,
    index: &Value,
    value: Value,
    span: Span,
) -> Result<Option<String>, VerseError> {
    let index = expect_string_index(index, span)?;
    let Some(byte) = char_value_to_byte(&value) else {
        return Err(VerseError::runtime_at(
            format!("string slot expected `char`, got `{value}`"),
            span,
        ));
    };
    if index >= text.len() {
        return Ok(None);
    }
    let mut bytes = text.into_bytes();
    bytes[index] = byte;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| VerseError::runtime_at("string slot assignment produced invalid UTF-8", span))
}

fn dedupe_runtime_strings(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        if !deduped.contains(&item) {
            deduped.push(item);
        }
    }
    deduped
}

fn type_name_is_string_char(name: &TypeName) -> bool {
    matches!(name, TypeName::Char)
}

fn array_type_name(type_name: &TypeName) -> Option<Option<&TypeName>> {
    match type_name {
        TypeName::Array(item) => Some(item.as_deref()),
        _ => None,
    }
}

fn tuple_type_name(type_name: &TypeName) -> Option<&[TypeName]> {
    match type_name {
        TypeName::Tuple(items) => Some(items),
        _ => None,
    }
}

fn positive_value(value: Value, span: Span) -> Result<Value, VerseError> {
    if runtime_number(&value).is_some() {
        Ok(value)
    } else {
        Err(VerseError::runtime_at(
            format!("unary `+` expected number, got {value}"),
            span,
        ))
    }
}

fn negate_value(value: Value, span: Span) -> Result<Value, VerseError> {
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

fn add_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
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

fn eval_binary_values(
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

fn subtract_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_pair_value(&left, &right, RuntimeNumberOp::Subtract) {
        return Ok(value);
    }

    numeric_binary_value(left, right, RuntimeNumberOp::Subtract, span)
}

fn multiply_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
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

fn divide_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = color_scale_value(&left, &right, RuntimeNumberOp::Divide, span)? {
        return Ok(value);
    }

    numeric_binary_value(left, right, RuntimeNumberOp::Divide, span)
}

fn remainder_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
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
enum RuntimeNumberOp {
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

fn color_pair_value(left: &Value, right: &Value, op: RuntimeNumberOp) -> Option<Value> {
    let (left_red, left_green, left_blue) = color_components(left)?;
    let (right_red, right_green, right_blue) = color_components(right)?;
    Some(color_value(
        apply_float_number_op(left_red, right_red, op),
        apply_float_number_op(left_green, right_green, op),
        apply_float_number_op(left_blue, right_blue, op),
    ))
}

fn color_scale_value(
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

fn expect_color_components(
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

fn expect_color_alpha_components(
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

fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
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

fn rgb_to_hsv(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
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

fn clamp_alpha(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn numeric_value_is_zero(value: &Value, context: &str, span: Span) -> Result<bool, VerseError> {
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

fn expect_number(value: &Value, context: &str, span: Span) -> Result<f64, VerseError> {
    match runtime_number(value) {
        Some(value) => Ok(runtime_number_to_f64(value)),
        None => Err(VerseError::runtime_at(
            format!("{context} expected number, got {value}"),
            span,
        )),
    }
}

fn expect_bool(value: &Value, context: &str, span: Span) -> Result<bool, VerseError> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected bool, got {value}"),
            span,
        )),
    }
}

fn expect_integer(value: &Value, context: &str, span: Span) -> Result<i64, VerseError> {
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

fn expect_index_integer(value: &Value, context: &str, span: Span) -> Result<i64, VerseError> {
    match value {
        Value::Int(value) => Ok(*value),
        _ => Err(VerseError::runtime_at(
            format!("{context} expected int, got {value}"),
            span,
        )),
    }
}

fn expect_index(value: &Value, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, "array index", span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("array index cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}

fn expect_tuple_index(value: &Value, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, "tuple index", span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("tuple index cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}

fn upsert_map_entry(entries: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    if let Some((_, existing_value)) = entries
        .iter_mut()
        .find(|(existing_key, _)| existing_key == &key)
    {
        *existing_value = value;
    } else {
        entries.push((key, value));
    }
}

fn method_has_specifier(method: &ClassMethod, name: &str) -> bool {
    method.effects.iter().any(|effect| effect == name)
}

fn qualify_runtime_interface_fields(
    interface_name: &str,
    mut fields: Vec<RuntimeClassField>,
) -> Vec<RuntimeClassField> {
    for field in &mut fields {
        if field.owner.is_none() {
            field.owner = Some(interface_name.to_string());
        }
    }
    fields
}

fn qualify_runtime_interface_methods(
    interface_name: &str,
    mut methods: Vec<RuntimeClassMethod>,
) -> Vec<RuntimeClassMethod> {
    for method in &mut methods {
        if method.qualifier.is_none() {
            method.qualifier = Some(interface_name.to_string());
        }
    }
    methods
}

fn runtime_qualifier_matches(stored: &str, requested: &str) -> bool {
    stored == requested
        || stored.rsplit('.').next() == Some(requested)
        || requested.rsplit('.').next() == Some(stored)
}

fn runtime_method_has_qualifier(method: &RuntimeClassMethod, qualifier: &str) -> bool {
    method
        .qualifier
        .as_deref()
        .is_some_and(|stored| runtime_qualifier_matches(stored, qualifier))
}

fn runtime_extension_method_has_qualifier(
    method: &RuntimeExtensionMethod,
    qualifier: &str,
) -> bool {
    method
        .module_name
        .as_deref()
        .is_some_and(|stored| runtime_qualifier_matches(stored, qualifier))
}

fn runtime_class_method_qualifiers_conflict(
    left: &RuntimeClassMethod,
    right: &RuntimeClassMethod,
) -> bool {
    match (left.qualifier.as_deref(), right.qualifier.as_deref()) {
        (Some(left), Some(right)) => runtime_qualifier_matches(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn runtime_class_methods_conflict(left: &RuntimeClassMethod, right: &RuntimeClassMethod) -> bool {
    left.name == right.name
        && runtime_class_method_qualifiers_conflict(left, right)
        && runtime_param_specs_key(&left.params) == runtime_param_specs_key(&right.params)
}

fn runtime_class_method_signatures_conflict(
    left: &RuntimeClassMethod,
    right: &RuntimeClassMethod,
) -> bool {
    left.name == right.name
        && runtime_param_specs_key(&left.params) == runtime_param_specs_key(&right.params)
}

fn runtime_inherited_method_override_index(
    inherited_methods: &[RuntimeClassMethod],
    method: &RuntimeClassMethod,
) -> Option<usize> {
    let candidates = inherited_methods
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            runtime_class_method_signatures_conflict(candidate, method).then_some(index)
        })
        .collect::<Vec<_>>();

    if method.qualifier.is_some() {
        return candidates.into_iter().find(|index| {
            runtime_class_method_qualifiers_conflict(&inherited_methods[*index], method)
        });
    }

    if let Some(index) = candidates
        .iter()
        .copied()
        .find(|index| runtime_class_method_qualifiers_conflict(&inherited_methods[*index], method))
    {
        return Some(index);
    }

    match candidates.as_slice() {
        [index] => Some(*index),
        _ => None,
    }
}

fn runtime_inherited_method_duplicate_index(
    inherited_methods: &[RuntimeClassMethod],
    method: &RuntimeClassMethod,
) -> Option<usize> {
    inherited_methods.iter().position(|candidate| {
        runtime_class_method_signatures_conflict(candidate, method)
            && (method.qualifier.is_none()
                || runtime_class_method_qualifiers_conflict(candidate, method))
    })
}

fn runtime_param_specs_key(params: &[Param]) -> Vec<(bool, String, Option<TypeName>)> {
    let mut key = params
        .iter()
        .map(|param| {
            (
                param.named,
                if param.named {
                    param.name.clone()
                } else {
                    String::new()
                },
                param
                    .annotation
                    .as_ref()
                    .map(|annotation| annotation.name.clone()),
            )
        })
        .collect::<Vec<_>>();
    if key.iter().all(|(named, _, _)| *named) {
        key.sort_by(|left, right| left.1.cmp(&right.1));
    }
    key
}

fn eval_array_method(
    name: &str,
    items: &[Value],
    args: Vec<Value>,
    span: Span,
) -> Result<Value, VerseError> {
    match name {
        "Slice" => {
            let (start, end) = array_slice_args(args, items.len(), span)?;
            ensure_slice_range(start, end, items.len(), span)?;
            Ok(array_value(
                items[start..end].iter().map(value_copy).collect(),
            ))
        }
        "Find" => {
            let [needle]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            let index = items
                .iter()
                .position(|item| item == &needle)
                .ok_or_else(|| VerseError::runtime_at("`Find` failed: element not found", span))?;
            Ok(Value::Int(index as i64))
        }
        "RemoveFirstElement" => {
            let [needle]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            let index = items
                .iter()
                .position(|item| item == &needle)
                .ok_or_else(|| {
                    VerseError::runtime_at("`RemoveFirstElement` failed: element not found", span)
                })?;
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.remove(index);
            Ok(array_value(result))
        }
        "RemoveAllElements" => {
            let [needle]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            Ok(array_value(
                items
                    .iter()
                    .filter(|item| *item != &needle)
                    .map(value_copy)
                    .collect(),
            ))
        }
        "Remove" => {
            let [start, end]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let start = expect_array_position(&start, "Remove start", span)?;
            let end = expect_array_position(&end, "Remove end", span)?;
            ensure_slice_range(start, end, items.len(), span)?;
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.drain(start..end);
            Ok(array_value(result))
        }
        "RemoveElement" => {
            let [index]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            let index =
                expect_array_index_for_len(&index, items.len(), "RemoveElement index", span)?;
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.remove(index);
            Ok(array_value(result))
        }
        "ReplaceFirstElement" => {
            let [old, new]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let index = items.iter().position(|item| item == &old).ok_or_else(|| {
                VerseError::runtime_at("`ReplaceFirstElement` failed: element not found", span)
            })?;
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result[index] = value_copy(&new);
            Ok(array_value(result))
        }
        "ReplaceAllElements" => {
            let [old, new]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            Ok(array_value(
                items
                    .iter()
                    .map(|item| {
                        if item == &old {
                            value_copy(&new)
                        } else {
                            value_copy(item)
                        }
                    })
                    .collect(),
            ))
        }
        "ReplaceElement" => {
            let [index, new]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let index =
                expect_array_index_for_len(&index, items.len(), "ReplaceElement index", span)?;
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result[index] = value_copy(&new);
            Ok(array_value(result))
        }
        "Insert" => {
            let [index, values]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let index = expect_array_position(&index, "Insert index", span)?;
            if index > items.len() {
                return Err(VerseError::runtime_at(
                    format!(
                        "Insert index {index} out of bounds for length {}",
                        items.len()
                    ),
                    span,
                ));
            }
            let Value::Array(values) = tuple_value_to_array(values) else {
                return Err(VerseError::runtime_at(
                    "`Insert` expected an array or tuple of values",
                    span,
                ));
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            let inserted: Vec<Value> = values.borrow().iter().map(value_copy).collect();
            result.splice(index..index, inserted);
            Ok(array_value(result))
        }
        "ReplaceAll" => {
            let [pattern, replacement]: [Value; 2] =
                args.try_into().map_err(|args: Vec<Value>| {
                    array_method_arity_error(name, 2, 2, args.len(), span)
                })?;
            let Value::Array(pattern) = tuple_value_to_array(pattern) else {
                return Err(VerseError::runtime_at(
                    "`ReplaceAll` expected an array or tuple pattern",
                    span,
                ));
            };
            let Value::Array(replacement) = tuple_value_to_array(replacement) else {
                return Err(VerseError::runtime_at(
                    "`ReplaceAll` expected an array or tuple replacement",
                    span,
                ));
            };
            replace_all(
                items,
                pattern.borrow().as_slice(),
                replacement.borrow().as_slice(),
                span,
            )
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown array method `{name}`"),
            span,
        )),
    }
}

fn eval_array_method_failable(
    name: &str,
    items: &[Value],
    args: Vec<Value>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match name {
        "Slice" => {
            let Some((start, end)) = array_slice_args_failable(args, items.len(), span)? else {
                return Ok(None);
            };
            if !valid_slice_range(start, end, items.len()) {
                return Ok(None);
            }
            Ok(Some(array_value(
                items[start..end].iter().map(value_copy).collect(),
            )))
        }
        "Find" => {
            let [needle]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            Ok(items
                .iter()
                .position(|item| item == &needle)
                .map(|index| Value::Int(index as i64)))
        }
        "RemoveFirstElement" => {
            let [needle]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            let Some(index) = items.iter().position(|item| item == &needle) else {
                return Ok(None);
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.remove(index);
            Ok(Some(array_value(result)))
        }
        "RemoveAllElements" => eval_array_method(name, items, args, span).map(Some),
        "Remove" => {
            let [start, end]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let (Some(start), Some(end)) = (
                array_position_value(&start, "Remove start", span)?,
                array_position_value(&end, "Remove end", span)?,
            ) else {
                return Ok(None);
            };
            if !valid_slice_range(start, end, items.len()) {
                return Ok(None);
            }
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.drain(start..end);
            Ok(Some(array_value(result)))
        }
        "RemoveElement" => {
            let [index]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 1, 1, args.len(), span)
            })?;
            let Some(index) = array_index_value(&index, items.len(), "RemoveElement index", span)?
            else {
                return Ok(None);
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result.remove(index);
            Ok(Some(array_value(result)))
        }
        "ReplaceFirstElement" => {
            let [old, new]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let Some(index) = items.iter().position(|item| item == &old) else {
                return Ok(None);
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result[index] = value_copy(&new);
            Ok(Some(array_value(result)))
        }
        "ReplaceAllElements" => eval_array_method(name, items, args, span).map(Some),
        "ReplaceElement" => {
            let [index, new]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let Some(index) = array_index_value(&index, items.len(), "ReplaceElement index", span)?
            else {
                return Ok(None);
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            result[index] = value_copy(&new);
            Ok(Some(array_value(result)))
        }
        "Insert" => {
            let [index, values]: [Value; 2] = args.try_into().map_err(|args: Vec<Value>| {
                array_method_arity_error(name, 2, 2, args.len(), span)
            })?;
            let Some(index) = array_position_value(&index, "Insert index", span)? else {
                return Ok(None);
            };
            if index > items.len() {
                return Ok(None);
            }
            let Value::Array(values) = tuple_value_to_array(values) else {
                return Err(VerseError::runtime_at(
                    "`Insert` expected an array or tuple of values",
                    span,
                ));
            };
            let mut result: Vec<Value> = items.iter().map(value_copy).collect();
            let inserted: Vec<Value> = values.borrow().iter().map(value_copy).collect();
            result.splice(index..index, inserted);
            Ok(Some(array_value(result)))
        }
        "ReplaceAll" => eval_array_method(name, items, args, span).map(Some),
        _ => eval_array_method(name, items, args, span).map(Some),
    }
}

fn eval_string_array_method(
    name: &str,
    text: &str,
    args: Vec<Value>,
    span: Span,
) -> Result<Value, VerseError> {
    let items = string_char_values(text);
    let args = string_array_method_args(name, args);
    eval_array_method(name, &items, args, span).map(coerce_string_value)
}

fn eval_string_array_method_failable(
    name: &str,
    text: &str,
    args: Vec<Value>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    let items = string_char_values(text);
    let args = string_array_method_args(name, args);
    eval_array_method_failable(name, &items, args, span).map(|value| value.map(coerce_string_value))
}

fn string_array_method_args(name: &str, args: Vec<Value>) -> Vec<Value> {
    match name {
        "Insert" => args
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                if index == 1 {
                    string_value_to_char_array(value)
                } else {
                    value
                }
            })
            .collect(),
        "ReplaceAll" => args.into_iter().map(string_value_to_char_array).collect(),
        _ => args,
    }
}

fn string_value_to_char_array(value: Value) -> Value {
    match value {
        Value::String(text) => array_value(string_char_values(&text)),
        other => other,
    }
}

fn eval_number_method(
    name: &str,
    receiver: Value,
    args: Vec<Value>,
    span: Span,
) -> Result<Value, VerseError> {
    eval_number_method_failable(name, receiver, args, span)?
        .ok_or_else(|| VerseError::runtime_at(format!("`{name}` failed"), span))
}

fn eval_number_method_failable(
    name: &str,
    receiver: Value,
    args: Vec<Value>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match name {
        "IsFinite" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`IsFinite` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            let finite = match runtime_number(&receiver) {
                Some(RuntimeNumber::Float(value)) => value.is_finite(),
                Some(RuntimeNumber::Int(_) | RuntimeNumber::Rational(_)) => true,
                None => false,
            };
            Ok(finite.then_some(receiver))
        }
        "IsAlmostZero" => {
            if args.len() != 1 {
                return Err(VerseError::runtime_at(
                    format!("`IsAlmostZero` expected 1 argument, got {}", args.len()),
                    span,
                ));
            }
            let value = expect_number(&receiver, "`IsAlmostZero` Val", span)?;
            let tolerance = expect_number(&args[0], "`IsAlmostZero` AbsoluteTolerance", span)?;
            Ok((value.abs() <= tolerance).then_some(Value::None))
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown number method `{name}`"),
            span,
        )),
    }
}

fn array_position_value(
    value: &Value,
    context: &str,
    span: Span,
) -> Result<Option<usize>, VerseError> {
    let index = expect_index_integer(value, context, span)?;
    if index < 0 {
        return Ok(None);
    }
    Ok(Some(index as usize))
}

fn array_index_value(
    value: &Value,
    length: usize,
    context: &str,
    span: Span,
) -> Result<Option<usize>, VerseError> {
    let Some(index) = array_position_value(value, context, span)? else {
        return Ok(None);
    };
    if index >= length {
        return Ok(None);
    }
    Ok(Some(index))
}

fn valid_slice_range(start: usize, end: usize, length: usize) -> bool {
    start <= end && end <= length
}

fn replace_all(
    items: &[Value],
    pattern: &[Value],
    replacement: &[Value],
    span: Span,
) -> Result<Value, VerseError> {
    if pattern.is_empty() {
        return Err(VerseError::runtime_at(
            "`ReplaceAll` pattern cannot be empty",
            span,
        ));
    }

    let mut result = Vec::new();
    let mut index = 0;
    while index < items.len() {
        if index + pattern.len() <= items.len() && items[index..index + pattern.len()] == *pattern {
            result.extend(replacement.iter().map(value_copy));
            index += pattern.len();
        } else {
            result.push(value_copy(&items[index]));
            index += 1;
        }
    }
    Ok(array_value(result))
}

fn expect_array_position(value: &Value, context: &str, span: Span) -> Result<usize, VerseError> {
    let index = expect_index_integer(value, context, span)?;
    if index < 0 {
        return Err(VerseError::runtime_at(
            format!("{context} cannot be negative: {index}"),
            span,
        ));
    }
    Ok(index as usize)
}

fn expect_array_index_for_len(
    value: &Value,
    len: usize,
    context: &str,
    span: Span,
) -> Result<usize, VerseError> {
    let index = expect_array_position(value, context, span)?;
    if index >= len {
        return Err(VerseError::runtime_at(
            format!("{context} {index} out of bounds for length {len}"),
            span,
        ));
    }
    Ok(index)
}

fn ensure_slice_range(start: usize, end: usize, len: usize, span: Span) -> Result<(), VerseError> {
    if start > end {
        return Err(VerseError::runtime_at(
            format!("slice start {start} cannot be greater than end {end}"),
            span,
        ));
    }
    if end > len {
        return Err(VerseError::runtime_at(
            format!("slice end {end} out of bounds for length {len}"),
            span,
        ));
    }
    Ok(())
}

fn array_slice_args(
    args: Vec<Value>,
    len: usize,
    span: Span,
) -> Result<(usize, usize), VerseError> {
    match args.as_slice() {
        [start] => Ok((expect_array_position(start, "Slice start", span)?, len)),
        [start, end] => Ok((
            expect_array_position(start, "Slice start", span)?,
            expect_array_position(end, "Slice end", span)?,
        )),
        _ => Err(array_method_arity_error("Slice", 1, 2, args.len(), span)),
    }
}

fn array_slice_args_failable(
    args: Vec<Value>,
    len: usize,
    span: Span,
) -> Result<Option<(usize, usize)>, VerseError> {
    match args.as_slice() {
        [start] => {
            let Some(start) = array_position_value(start, "Slice start", span)? else {
                return Ok(None);
            };
            Ok(Some((start, len)))
        }
        [start, end] => {
            let (Some(start), Some(end)) = (
                array_position_value(start, "Slice start", span)?,
                array_position_value(end, "Slice end", span)?,
            ) else {
                return Ok(None);
            };
            Ok(Some((start, end)))
        }
        _ => Err(array_method_arity_error("Slice", 1, 2, args.len(), span)),
    }
}

fn array_method_arity_error(
    name: &str,
    min: usize,
    max: usize,
    actual: usize,
    span: Span,
) -> VerseError {
    let expected = if min == max {
        min.to_string()
    } else {
        format!("{min}..={max}")
    };
    VerseError::runtime_at(
        format!("`{name}` expected {expected} arguments, got {actual}"),
        span,
    )
}

fn array_value(items: Vec<Value>) -> Value {
    Value::Array(Rc::new(RefCell::new(items)))
}

fn player_map_value_size(value: &Value) -> Option<usize> {
    player_map_value_size_inner(value, 0)
}

fn player_map_value_size_inner(value: &Value, depth: usize) -> Option<usize> {
    if depth > PLAYER_MAP_MAX_SIZE_DEPTH {
        return None;
    }

    match value {
        Value::Int(_) | Value::Float(_) => Some(8),
        Value::Rational(_) => Some(16),
        Value::Char(_) => Some(1),
        Value::Char32(_) => Some(4),
        Value::Bool(_) => Some(1),
        Value::String(text) => Some(text.len()),
        Value::None => Some(0),
        Value::Pending | Value::Suspended(_) => None,
        Value::EnumValue { enum_name, variant } => enum_name.len().checked_add(variant.len()),
        Value::StructInstance { fields, .. } => {
            fields.iter().try_fold(0usize, |total, (name, value)| {
                total
                    .checked_add(name.len())?
                    .checked_add(player_map_value_size_inner(value, depth + 1)?)
            })
        }
        Value::ClassInstance { unique, fields, .. } => {
            if *unique {
                return None;
            }
            fields.borrow().iter().try_fold(0usize, |total, field| {
                total
                    .checked_add(field.name.len())?
                    .checked_add(player_map_value_size_inner(&field.value, depth + 1)?)
            })
        }
        Value::Array(items) => items.borrow().iter().try_fold(0usize, |total, item| {
            total.checked_add(player_map_value_size_inner(item, depth + 1)?)
        }),
        Value::Map(entries) => entries
            .borrow()
            .iter()
            .try_fold(0usize, |total, (key, value)| {
                total
                    .checked_add(player_map_value_size_inner(key, depth + 1)?)?
                    .checked_add(player_map_value_size_inner(value, depth + 1)?)
            }),
        Value::Tuple(items) => items.iter().try_fold(0usize, |total, item| {
            total.checked_add(player_map_value_size_inner(item, depth + 1)?)
        }),
        Value::Option(Some(value)) => {
            1usize.checked_add(player_map_value_size_inner(value, depth + 1)?)
        }
        Value::Option(None) => Some(1),
        Value::Result { .. }
        | Value::Event { .. }
        | Value::Awaitable { .. }
        | Value::Signalable { .. }
        | Value::Subscribable { .. }
        | Value::Listenable { .. }
        | Value::SubscriptionCancelHandle { .. }
        | Value::Task(_)
        | Value::Generator { .. }
        | Value::Modifier { .. }
        | Value::ModifierStack { .. }
        | Value::ModifierCancelHandle { .. }
        | Value::CastableSubtype(_)
        | Value::ConcreteSubtype(_)
        | Value::ClassifiableSubset(_)
        | Value::Diagnostic(_)
        | Value::External
        | Value::Session
        | Value::Range { .. }
        | Value::EnumType { .. }
        | Value::StructType { .. }
        | Value::ClassType { .. }
        | Value::InterfaceType { .. }
        | Value::ParametricType { .. }
        | Value::Module { .. }
        | Value::Function { .. }
        | Value::Overload(_)
        | Value::BoundMethod { .. }
        | Value::NativeFunction { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. } => None,
    }
}

impl NativeResult {
    fn into_value(self, name: &str, span: Span) -> Result<Value, VerseError> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Failure(reason) => Err(VerseError::runtime_at(
                format!("`{name}` failed: {reason}"),
                span,
            )),
        }
    }
}

fn modifier_stack_position(stack: &Value, first: bool) -> Value {
    let Value::ModifierStack { entries, .. } = stack else {
        return Value::Option(None);
    };
    let entries = entries.borrow();
    let position = if first {
        entries
            .iter()
            .map(|entry| entry.position)
            .min_by(|left, right| compare_rational(*left, *right))
    } else {
        entries
            .iter()
            .map(|entry| entry.position)
            .max_by(|left, right| compare_rational(*left, *right))
    };
    Value::Option(position.map(|position| Box::new(Value::Rational(position))))
}

fn compare_rational(left: RationalValue, right: RationalValue) -> std::cmp::Ordering {
    let left_scaled = left.numerator as i128 * right.denominator as i128;
    let right_scaled = right.numerator as i128 * left.denominator as i128;
    left_scaled.cmp(&right_scaled)
}

fn expect_runtime_rational(
    value: &Value,
    label: &str,
    span: Span,
) -> Result<RationalValue, VerseError> {
    let Some(number) = runtime_number(value) else {
        return Err(VerseError::runtime_at(
            format!("{label} expected rational, got {value}"),
            span,
        ));
    };
    runtime_number_to_rational(number).ok_or_else(|| {
        VerseError::runtime_at(format!("{label} expected rational, got {value}"), span)
    })
}

fn call_native_cancel_method(
    name: &'static str,
    entries: &Rc<RefCell<Vec<RuntimeModifierEntry>>>,
    entry_id: u64,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }
    if !args.is_empty() {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected 0 arguments, got {}", args.len()),
            span,
        ));
    }
    entries.borrow_mut().retain(|entry| entry.id != entry_id);
    Ok(NativeResult::Value(Value::None))
}

fn call_native_subscribable_method(
    name: &'static str,
    payload: Option<&TypeName>,
    subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    next_subscriber_id: &Rc<RefCell<u64>>,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }
    if args.len() != 1 {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected 1 argument, got {}", args.len()),
            span,
        ));
    }
    let callback = args.into_iter().next().expect("arity checked").value;
    let expected_arity = usize::from(payload.is_some());
    if !runtime_callable_accepts_arity(&callback, expected_arity) {
        return Err(VerseError::runtime_at(
            format!(
                "`Subscribe` Callback expected function/{expected_arity} -> void, got {callback}"
            ),
            span,
        ));
    }

    let id = {
        let mut next = next_subscriber_id.borrow_mut();
        let id = *next;
        *next = next.saturating_add(1);
        id
    };
    subscribers.borrow_mut().push(RuntimeSubscriptionEntry {
        id,
        callback: value_copy(&callback),
    });
    Ok(NativeResult::Value(Value::SubscriptionCancelHandle {
        subscribers: subscribers.clone(),
        subscriber_id: id,
    }))
}

fn call_native_subscription_cancel_method(
    name: &'static str,
    subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    subscriber_id: u64,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }
    if !args.is_empty() {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected 0 arguments, got {}", args.len()),
            span,
        ));
    }
    subscribers
        .borrow_mut()
        .retain(|entry| entry.id != subscriber_id);
    Ok(NativeResult::Value(Value::None))
}

fn runtime_callable_accepts_arity(value: &Value, expected_arity: usize) -> bool {
    match value {
        Value::Function { params, .. } | Value::BoundMethod { params, .. } => {
            params.len() == expected_arity
        }
        Value::Overload(overloads) => overloads
            .iter()
            .any(|overload| runtime_callable_accepts_arity(overload, expected_arity)),
        Value::NativeFunction { arity, .. } => arity.is_none_or(|arity| arity == expected_arity),
        Value::NativeResultMethod { name, .. }
        | Value::NativeSubscribableMethod { name, .. }
        | Value::NativeTaskMethod { name, .. }
        | Value::NativeModifierMethod { name, .. }
        | Value::NativeCancelMethod { name, .. }
        | Value::NativeSubscriptionCancelMethod { name, .. } => match *name {
            "Await" | "Cancel" | "GetSuccess" | "GetError" => expected_arity == 0,
            "Evaluate" | "Subscribe" => expected_arity == 1,
            "AddModifier" => expected_arity == 2,
            _ => false,
        },
        Value::NativeEventMethod { name: "Await", .. } => expected_arity == 0,
        Value::NativeEventMethod {
            name: "Signal",
            payload,
            ..
        } => expected_arity == usize::from(payload.is_some()),
        Value::NativeEventMethod { .. } => false,
        _ => false,
    }
}

fn call_native_function(
    name: &'static str,
    arity: Option<usize>,
    function: NativeFn,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if name == "Print" {
        return native_print_call(args, span);
    }
    let values = if args.iter().any(|arg| arg.name.is_some()) {
        let Some(param_aliases) = native_named_param_aliases(name) else {
            return Err(VerseError::runtime_at(
                format!("`{name}` does not accept named arguments"),
                span,
            ));
        };
        reorder_native_call_args(name, param_aliases, args, span)?
    } else {
        if let Some(expected) = arity
            && args.len() != expected
        {
            return Err(VerseError::runtime_at(
                format!("`{name}` expected {expected} arguments, got {}", args.len()),
                span,
            ));
        }
        args.into_iter().map(|arg| arg.value).collect()
    };
    function(values, span)
}

fn native_named_param_aliases(name: &str) -> Option<Vec<Vec<&'static str>>> {
    let aliases = match name {
        "assert_eq" => vec![vec!["expected"], vec!["actual"]],
        "str" => vec![vec!["value"]],
        "Err" => vec![vec!["Message"]],
        "ToDiagnostic" => vec![vec!["Value"]],
        "MakeColorFromSRGB" | "MakeColorFromSRGBValues" => {
            vec![vec!["Red"], vec!["Green"], vec!["Blue"]]
        }
        "MakeSRGBFromColor" | "MakeHSVFromColor" => vec![vec!["Color"]],
        "MakeColorFromHex" => vec![vec!["hexString"]],
        "MakeColorFromHSV" => vec![vec!["Hue"], vec!["Saturation"], vec!["Value"]],
        "MakeColorAlpha" => vec![vec!["R"], vec!["G"], vec!["B"], vec!["A"]],
        "Over" => vec![vec!["CA1"], vec!["CA2"]],
        "ToString" => vec![vec!["Val", "String", "Character"]],
        "Localize" => vec![vec!["Message"]],
        "Join" => vec![vec!["Strings", "Messages"], vec!["Separator"]],
        "Concatenate" => vec![vec!["Arrays"]],
        "GetRandomFloat" | "GetRandomInt" => vec![vec!["Low"], vec!["High"]],
        "Shuffle" => vec![vec!["Input"]],
        "Sleep" => vec![vec!["Seconds"]],
        "Clamp" => vec![vec!["Value"], vec!["A"], vec!["B"]],
        "Lerp" => vec![vec!["From"], vec!["To"], vec!["Parameter"]],
        "Abs" | "Ceil" | "Floor" => vec![vec!["Value"]],
        "Min" | "Max" => vec![vec!["X"], vec!["Y"]],
        "Round" | "Int" | "Sgn" => vec![vec!["Val"]],
        "Sqrt" | "Sin" | "Cos" | "Tan" | "ArcSin" | "ArcCos" | "Sinh" | "Cosh" | "Tanh"
        | "ArSinh" | "ArCosh" | "ArTanh" | "Exp" | "Ln" => vec![vec!["X"]],
        "Pow" => vec![vec!["A"], vec!["B"]],
        "Log" => vec![vec!["B"], vec!["X"]],
        "IsAlmostEqual" => vec![vec!["Val1"], vec!["Val2"], vec!["AbsoluteTolerance"]],
        _ => return None,
    };
    Some(aliases)
}

fn reorder_native_call_args(
    function_name: &str,
    param_aliases: Vec<Vec<&'static str>>,
    args: Vec<CallValue>,
    span: Span,
) -> Result<Vec<Value>, VerseError> {
    let expected = param_aliases.len();
    let got = args.len();
    let mut assigned = vec![false; expected];
    let mut values = vec![None; expected];
    let mut positional_index = 0usize;

    for arg in args {
        match arg.name {
            None => {
                while positional_index < expected && assigned[positional_index] {
                    positional_index += 1;
                }
                if positional_index >= expected {
                    return Err(VerseError::runtime_at(
                        format!("`{function_name}` expected {expected} arguments, got {got}"),
                        arg.span,
                    ));
                }
                assigned[positional_index] = true;
                values[positional_index] = Some(arg.value);
                positional_index += 1;
            }
            Some(name) => {
                let Some(param_index) = param_aliases
                    .iter()
                    .position(|aliases| aliases.iter().any(|alias| *alias == name))
                else {
                    let rendered = rendered_call_argument_name(&name, arg.optional);
                    return Err(VerseError::runtime_at(
                        format!("unknown named argument `{rendered}`"),
                        arg.span,
                    ));
                };
                if arg.optional {
                    return Err(VerseError::runtime_at(
                        format!("parameter `{name}` is not a named parameter"),
                        arg.span,
                    ));
                }
                if assigned[param_index] {
                    return Err(VerseError::runtime_at(
                        format!("duplicate argument for parameter `{name}`"),
                        arg.span,
                    ));
                }
                assigned[param_index] = true;
                values[param_index] = Some(arg.value);
            }
        }
    }

    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            value.ok_or_else(|| {
                VerseError::runtime_at(
                    format!("missing required argument `{}`", param_aliases[index][0]),
                    span,
                )
            })
        })
        .collect()
}

fn call_native_result_method(
    name: &'static str,
    result: &Value,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }
    if !args.is_empty() {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected 0 arguments, got {}", args.len()),
            span,
        ));
    }

    let Value::Result { succeeded, value } = result else {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected a result receiver"),
            span,
        ));
    };

    match (name, *succeeded) {
        ("GetSuccess", true) | ("GetError", false) => Ok(NativeResult::Value(value_copy(value))),
        ("GetSuccess", false) => Ok(NativeResult::Failure("result is an error")),
        ("GetError", true) => Ok(NativeResult::Failure("result is a success")),
        _ => Err(VerseError::runtime_at(
            format!("unknown result method `{name}`"),
            span,
        )),
    }
}

fn call_native_event_method(
    interpreter: &Interpreter,
    name: &'static str,
    payload: Option<&TypeName>,
    waiters: Option<&Rc<RefCell<Vec<Rc<RuntimeTask>>>>>,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }

    match name {
        "Signal" => {
            validate_event_signal_args(payload, &args, span)?;
            if let Some(waiters) = waiters {
                let value = event_signal_value(payload, &args);
                let ready = std::mem::take(&mut *waiters.borrow_mut());
                for task in ready {
                    task.resume(interpreter, value_copy(&value));
                }
            }
            Ok(NativeResult::Value(Value::None))
        }
        "Await" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`Await` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            Ok(NativeResult::Value(if let Some(waiters) = waiters {
                Value::Suspended(RuntimeSuspension::event(waiters.clone()))
            } else {
                Value::Pending
            }))
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown event method `{name}`"),
            span,
        )),
    }
}

fn call_native_task_method(
    name: &'static str,
    task: &Rc<RuntimeTask>,
    args: Vec<CallValue>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(VerseError::runtime_at(
            format!("`{name}` does not accept named arguments"),
            span,
        ));
    }

    match name {
        "Await" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`Await` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }

            match task.await_result()? {
                Some(value) => Ok(NativeResult::Value(value)),
                None => Ok(NativeResult::Value(Value::Suspended(
                    RuntimeSuspension::task(task.clone()),
                ))),
            }
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown task method `{name}`"),
            span,
        )),
    }
}

fn validate_event_signal_args(
    payload: Option<&TypeName>,
    args: &[CallValue],
    span: Span,
) -> Result<(), VerseError> {
    let Some(payload) = payload else {
        if args.is_empty() {
            return Ok(());
        }
        return Err(VerseError::runtime_at(
            format!("`Signal` expected 0 arguments, got {}", args.len()),
            span,
        ));
    };

    if let TypeName::Tuple(items) = payload {
        if args.len() == 1 && runtime_event_payload_matches(&args[0].value, payload) {
            return Ok(());
        }

        if args.len() != items.len() {
            return Err(VerseError::runtime_at(
                format!(
                    "`Signal` expected {} arguments for tuple payload, got {}",
                    items.len(),
                    args.len()
                ),
                span,
            ));
        }

        for (index, (arg, item_type)) in args.iter().zip(items).enumerate() {
            if !runtime_event_payload_matches(&arg.value, item_type) {
                return Err(VerseError::runtime_at(
                    format!(
                        "`Signal` tuple argument item {} expected `{}`, got {}",
                        index + 1,
                        render_runtime_type_name(item_type),
                        arg.value
                    ),
                    arg.span,
                ));
            }
        }
        return Ok(());
    }

    if args.len() != 1 {
        return Err(VerseError::runtime_at(
            format!("`Signal` expected 1 argument, got {}", args.len()),
            span,
        ));
    }
    if !runtime_event_payload_matches(&args[0].value, payload) {
        return Err(VerseError::runtime_at(
            format!(
                "`Signal` argument expected `{}`, got {}",
                render_runtime_type_name(payload),
                args[0].value
            ),
            args[0].span,
        ));
    }
    Ok(())
}

fn event_signal_value(payload: Option<&TypeName>, args: &[CallValue]) -> Value {
    let Some(payload) = payload else {
        return Value::None;
    };

    if let TypeName::Tuple(_) = payload {
        if args.len() == 1 && runtime_event_payload_matches(&args[0].value, payload) {
            return value_copy(&args[0].value);
        }
        return Value::Tuple(args.iter().map(|arg| value_copy(&arg.value)).collect());
    }

    value_copy(&args[0].value)
}

fn runtime_event_payload_matches(value: &Value, payload: &TypeName) -> bool {
    runtime_value_matches_type_name(value, payload, &Env::new())
}

fn native_print_call(args: Vec<CallValue>, span: Span) -> Result<NativeResult, VerseError> {
    let mut message = None;
    let mut duration = None;
    let mut color = None;

    for arg in args {
        match arg.name.as_deref() {
            None if message.is_none() => message = Some(arg.value),
            None => {
                return Err(VerseError::runtime_at(
                    "`Print` only accepts one positional message argument",
                    arg.span,
                ));
            }
            Some("Message") if arg.optional => {
                return Err(VerseError::runtime_at(
                    "parameter `Message` is not a named parameter",
                    arg.span,
                ));
            }
            Some("Message") if message.is_none() => message = Some(arg.value),
            Some("Message") => {
                return Err(VerseError::runtime_at(
                    "duplicate argument for parameter `Message`",
                    arg.span,
                ));
            }
            Some("Duration") if duration.is_none() => duration = Some(arg.value),
            Some("Duration") => {
                let rendered = rendered_call_argument_name("Duration", arg.optional);
                return Err(VerseError::runtime_at(
                    format!("duplicate argument for parameter `{rendered}`"),
                    arg.span,
                ));
            }
            Some("Color") if color.is_none() => color = Some(arg.value),
            Some("Color") => {
                let rendered = rendered_call_argument_name("Color", arg.optional);
                return Err(VerseError::runtime_at(
                    format!("duplicate argument for parameter `{rendered}`"),
                    arg.span,
                ));
            }
            Some(name) => {
                let rendered = rendered_call_argument_name(name, arg.optional);
                return Err(VerseError::runtime_at(
                    format!("unknown named argument `{rendered}`"),
                    arg.span,
                ));
            }
        }
    }

    let Some(message) = message else {
        return Err(VerseError::runtime_at(
            "missing required argument `Message`",
            span,
        ));
    };

    let message = printable_message_text(message, span)?;
    if let Some(duration) = duration {
        expect_number(&duration, "`Print` duration", span)?;
    }
    if let Some(color) = color {
        expect_color_value(&color, span)?;
    }

    println!("{message}");
    Ok(NativeResult::Value(Value::None))
}

fn native_print(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let line = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    println!("{line}");
    Ok(NativeResult::Value(Value::None))
}

fn printable_message_text(message: Value, span: Span) -> Result<String, VerseError> {
    match message {
        Value::String(message) => Ok(message),
        Value::Array(items) => char_array_to_string(items.borrow().as_slice()).ok_or_else(|| {
            VerseError::runtime_at("`Print` expected string-compatible `[]char`", span)
        }),
        Value::Diagnostic(message) => Ok(message),
        other => Err(VerseError::runtime_at(
            format!("`Print` expected a string, message, or diagnostic, got {other}"),
            span,
        )),
    }
}

fn expect_profile_description(value: &Value, span: Span) -> Result<(), VerseError> {
    match value {
        Value::String(_) => Ok(()),
        Value::Array(items) if char_array_to_string(items.borrow().as_slice()).is_some() => Ok(()),
        other => Err(VerseError::runtime_at(
            format!("profile description expected `string`, got {other}"),
            span,
        )),
    }
}

fn expect_color_value(value: &Value, span: Span) -> Result<(), VerseError> {
    if matches!(
        value,
        Value::StructInstance { struct_name, .. } if struct_name == "color"
    ) {
        Ok(())
    } else {
        Err(VerseError::runtime_at(
            format!("`Print` color expected `color`, got {value}"),
            span,
        ))
    }
}

fn native_assert_eq(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    if args[0] == args[1] {
        Ok(NativeResult::Value(Value::None))
    } else {
        Err(VerseError::runtime(format!(
            "assert_eq failed: expected {}, got {}",
            args[0], args[1]
        )))
    }
}

fn native_str(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    Ok(NativeResult::Value(Value::String(args[0].to_string())))
}

fn native_err(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [message]: [Value; 1] = args.try_into().expect("arity checked by caller");
    let Value::String(message) = message else {
        return Err(VerseError::runtime_at("`Err` expected a string", span));
    };
    Err(VerseError::runtime_at(message, span))
}

fn native_to_diagnostic(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Diagnostic(value.to_string())))
}

fn native_get_seconds_since_epoch(
    _args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    let seconds = CURRENT_EPOCH_SECONDS
        .with(|current| *current.borrow())
        .map_or_else(|| current_unix_epoch_seconds(span), Ok)?;
    Ok(NativeResult::Value(Value::Float(seconds)))
}

fn native_get_random_float(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [low, high]: [Value; 2] = args
        .try_into()
        .expect("native arity checked before GetRandomFloat");
    let low = expect_number(&low, "`GetRandomFloat` Low", span)?;
    let high = expect_number(&high, "`GetRandomFloat` High", span)?;
    if !low.is_finite() || !high.is_finite() {
        return Err(VerseError::runtime_at(
            "`GetRandomFloat` bounds must be finite",
            span,
        ));
    }
    if low > high {
        return Err(VerseError::runtime_at(
            "`GetRandomFloat` Low must be less than or equal to High",
            span,
        ));
    }
    if low == high {
        return Ok(NativeResult::Value(Value::Float(low)));
    }

    let mut rng = OsRng;
    Ok(NativeResult::Value(Value::Float(rng.gen_range(low..=high))))
}

fn native_get_random_int(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [low, high]: [Value; 2] = args
        .try_into()
        .expect("native arity checked before GetRandomInt");
    let low = expect_integer(&low, "`GetRandomInt` Low", span)?;
    let high = expect_integer(&high, "`GetRandomInt` High", span)?;
    let (low, high) = if low <= high {
        (low, high)
    } else {
        (high, low)
    };
    if low == high {
        return Ok(NativeResult::Value(Value::Int(low)));
    }

    let mut rng = OsRng;
    Ok(NativeResult::Value(Value::Int(rng.gen_range(low..=high))))
}

fn native_shuffle(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [input]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before Shuffle");
    let Value::Array(items) = input else {
        return Err(VerseError::runtime_at(
            format!("`Shuffle` expected `array`, got {input}"),
            span,
        ));
    };
    let mut result: Vec<Value> = items.borrow().iter().map(value_copy).collect();
    let mut rng = OsRng;
    result.shuffle(&mut rng);
    Ok(NativeResult::Value(array_value(result)))
}

fn native_make_color_from_srgb(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [red, green, blue]: [Value; 3] = args
        .try_into()
        .expect("native arity checked before MakeColorFromSRGB");
    Ok(NativeResult::Value(color_value(
        expect_number(&red, "`MakeColorFromSRGB` red", span)?,
        expect_number(&green, "`MakeColorFromSRGB` green", span)?,
        expect_number(&blue, "`MakeColorFromSRGB` blue", span)?,
    )))
}

fn native_make_color_from_srgb_values(
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

fn native_make_srgb_from_color(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
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

fn native_make_color_from_hex(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
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

fn native_make_color_from_hsv(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [hue, saturation, value]: [Value; 3] = args
        .try_into()
        .expect("native arity checked before MakeColorFromHSV");
    let hue = expect_number(&hue, "`MakeColorFromHSV` Hue", span)?;
    let saturation = expect_number(&saturation, "`MakeColorFromHSV` Saturation", span)?;
    let value = expect_number(&value, "`MakeColorFromHSV` Value", span)?;
    let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
    Ok(NativeResult::Value(color_value(red, green, blue)))
}

fn native_make_hsv_from_color(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
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

fn native_make_color_alpha(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
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

fn native_over(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
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

fn native_get_session(_args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    Ok(NativeResult::Value(Value::Session))
}

fn native_get_simulation_elapsed_time(
    _args: Vec<Value>,
    _span: Span,
) -> Result<NativeResult, VerseError> {
    let seconds = SIMULATION_START_INSTANT.with(|start| {
        let mut start = start.borrow_mut();
        start
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64()
    });
    Ok(NativeResult::Value(Value::Float(seconds)))
}

fn native_sleep(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [seconds]: [Value; 1] = args.try_into().expect("native arity checked before Sleep");
    let seconds = expect_number(&seconds, "`Sleep` Seconds", span)?;

    if seconds.is_nan() {
        return Err(VerseError::runtime_at(
            "`Sleep` Seconds cannot be NaN",
            span,
        ));
    }
    if seconds.is_infinite() {
        return Ok(NativeResult::Value(Value::Pending));
    }
    if seconds > 0.0 {
        let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
            VerseError::runtime_at("`Sleep` Seconds is outside supported duration range", span)
        })?;
        let deadline = Instant::now().checked_add(duration).ok_or_else(|| {
            VerseError::runtime_at("`Sleep` Seconds is outside supported duration range", span)
        })?;
        return Ok(NativeResult::Value(CURRENT_RUNTIME_SCHEDULER.with(
            |scheduler| {
                scheduler
                    .borrow()
                    .as_ref()
                    .map(|scheduler| {
                        Value::Suspended(RuntimeSuspension::sleep_until(
                            scheduler.clone(),
                            deadline,
                        ))
                    })
                    .unwrap_or(Value::Pending)
            },
        )));
    }
    if seconds == 0.0 {
        return Ok(NativeResult::Value(CURRENT_RUNTIME_SCHEDULER.with(
            |scheduler| {
                scheduler
                    .borrow()
                    .as_ref()
                    .map(|scheduler| {
                        Value::Suspended(RuntimeSuspension::sleep_next_tick(scheduler.clone()))
                    })
                    .unwrap_or(Value::Pending)
            },
        )));
    }

    Ok(NativeResult::Value(Value::None))
}

fn native_make_success(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before MakeSuccess");
    Ok(NativeResult::Value(Value::Result {
        succeeded: true,
        value: Box::new(value),
    }))
}

fn native_make_error(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before MakeError");
    Ok(NativeResult::Value(Value::Result {
        succeeded: false,
        value: Box::new(value),
    }))
}

fn native_session_environment(_args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    Ok(NativeResult::Value(Value::EnumValue {
        enum_name: "session_environment".to_string(),
        variant: "Edit".to_string(),
    }))
}

fn native_fits_in_player_map(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before FitsInPlayerMap");
    match player_map_value_size(&value) {
        Some(size) if size <= PLAYER_MAP_RECORD_LIMIT_BYTES => Ok(NativeResult::Value(value)),
        Some(_) => Ok(NativeResult::Failure(
            "value exceeds player weak_map record size limit",
        )),
        None => Ok(NativeResult::Failure(
            "value is not persistable in a player weak_map",
        )),
    }
}

fn current_unix_epoch_seconds(span: Span) -> Result<f64, VerseError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| VerseError::runtime_at("system clock is before January 1, 1970 UTC", span))?;
    Ok(duration.as_secs_f64())
}

fn native_to_string(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args
        .try_into()
        .expect("native arity checked before ToString");
    let text = match value {
        Value::Int(value) => value.to_string(),
        Value::Float(value) => Value::Float(value).to_string(),
        Value::String(value) => value,
        Value::Array(items) => {
            char_array_to_string(items.borrow().as_slice()).ok_or_else(|| {
                VerseError::runtime_at("`ToString` expected string-compatible `[]char`", span)
            })?
        }
        Value::Char(value) | Value::Char32(value) => value.to_string(),
        Value::Rational(_) => {
            return Err(VerseError::runtime_at(
                "`ToString` expected `float`, `int`, `[]char`, or `char`, got rational",
                span,
            ));
        }
        other => {
            return Err(VerseError::runtime_at(
                format!("`ToString` expected `float`, `int`, `[]char`, or `char`, got {other}"),
                span,
            ));
        }
    };
    Ok(NativeResult::Value(Value::String(text)))
}

fn native_localize(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [message]: [Value; 1] = args.try_into().expect("arity checked by caller");
    let Value::String(message) = message else {
        return Err(VerseError::runtime_at(
            "`Localize` expected a message",
            span,
        ));
    };
    Ok(NativeResult::Value(Value::String(message)))
}

fn native_join(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [messages, separator]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let Value::Array(messages) = messages else {
        return Err(VerseError::runtime_at(
            "`Join` expected an array of strings or messages",
            span,
        ));
    };
    let Value::String(separator) = separator else {
        return Err(VerseError::runtime_at(
            "`Join` expected a string or message separator",
            span,
        ));
    };

    let mut rendered = Vec::new();
    for message in messages.borrow().iter() {
        let Value::String(message) = message else {
            return Err(VerseError::runtime_at(
                "`Join` expected an array of strings or messages",
                span,
            ));
        };
        rendered.push(message.clone());
    }

    Ok(NativeResult::Value(Value::String(
        rendered.join(&separator),
    )))
}

fn native_concatenate(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let mut result = Vec::new();

    if args.len() == 1 {
        let arg = args.into_iter().next().expect("length checked above");
        let arrays_candidate = tuple_value_to_array(value_copy(&arg));
        if let Value::Array(arrays) = &arrays_candidate
            && arrays
                .borrow()
                .iter()
                .all(|item| matches!(item, Value::Array(_)))
        {
            for array in arrays.borrow().iter() {
                let Value::Array(items) = array else {
                    unreachable!("checked above");
                };
                result.extend(items.borrow().iter().map(value_copy));
            }
            return Ok(NativeResult::Value(array_value(result)));
        }
        return concatenate_packed_array_args(vec![arg], span);
    }

    concatenate_packed_array_args(args, span)
}

fn concatenate_packed_array_args(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let mut result = Vec::new();
    for arg in args {
        match tuple_value_to_array(arg) {
            Value::Array(items) => result.extend(items.borrow().iter().map(value_copy)),
            other => {
                return Err(VerseError::runtime_at(
                    format!("`Concatenate` expected array arguments, got {other}"),
                    span,
                ));
            }
        }
    }

    Ok(NativeResult::Value(array_value(result)))
}

fn native_concatenate_maps(args: Vec<Value>, _span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let (Value::Map(left), Value::Map(right)) = (left, right) else {
        return Err(VerseError::runtime(
            "`ConcatenateMaps` expected two map arguments",
        ));
    };

    let mut result: Vec<(Value, Value)> = left
        .borrow()
        .iter()
        .map(|(key, value)| (value_copy(key), value_copy(value)))
        .collect();
    for (key, value) in right.borrow().iter() {
        upsert_map_entry(&mut result, value_copy(key), value_copy(value));
    }

    Ok(NativeResult::Value(Value::Map(Rc::new(RefCell::new(
        result,
    )))))
}

fn native_make_classifiable_subset(
    args: Vec<Value>,
    _span: Span,
) -> Result<NativeResult, VerseError> {
    let [elements]: [Value; 1] = args.try_into().expect("arity checked by caller");
    let Value::Array(elements) = elements else {
        return Err(VerseError::runtime(
            "`MakeClassifiableSubset` expected an array argument",
        ));
    };

    Ok(NativeResult::Value(Value::ClassifiableSubset(Rc::new(
        RefCell::new(elements.borrow().iter().map(value_copy).collect()),
    ))))
}

fn is_classifiable_subset_method_name(name: &str) -> bool {
    matches!(name, "Contains" | "ContainsAny" | "ContainsAll")
}

fn eval_classifiable_subset_method(
    name: &str,
    items: &[Value],
    args: &[Value],
    span: Span,
) -> Result<NativeResult, VerseError> {
    if args.len() != 1 {
        return Err(VerseError::runtime_at(
            format!("`{name}` expected 1 arguments, got {}", args.len()),
            span,
        ));
    }

    match name {
        "Contains" => {
            if items.iter().any(|item| item == &args[0]) {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("element is not present"))
            }
        }
        "ContainsAny" => {
            let values = expect_classifiable_subset_argument_array(name, &args[0], span)?;
            if values
                .borrow()
                .iter()
                .any(|candidate| items.iter().any(|item| item == candidate))
            {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("no elements are present"))
            }
        }
        "ContainsAll" => {
            let values = expect_classifiable_subset_argument_array(name, &args[0], span)?;
            if values
                .borrow()
                .iter()
                .all(|candidate| items.iter().any(|item| item == candidate))
            {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("not all elements are present"))
            }
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown classifiable_subset method `{name}`"),
            span,
        )),
    }
}

fn expect_classifiable_subset_argument_array(
    name: &str,
    value: &Value,
    span: Span,
) -> Result<Rc<RefCell<Vec<Value>>>, VerseError> {
    match value {
        Value::Array(values) => Ok(values.clone()),
        other => Err(VerseError::runtime_at(
            format!("`{name}` expected an array argument, got {other}"),
            span,
        )),
    }
}

fn native_mod(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let dividend = expect_integer(&left, "`Mod` X", span)?;
    let divisor = expect_integer(&right, "`Mod` Y", span)?;
    if divisor == 0 {
        return Ok(NativeResult::Failure("division by zero"));
    }

    let modulus = (divisor as i128).abs();
    let dividend = dividend as i128;
    let remainder = ((dividend % modulus) + modulus) % modulus;
    Ok(NativeResult::Value(Value::Int(integer_from_i128(
        remainder,
        "`Mod` result",
        span,
    )?)))
}

fn native_quotient(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let dividend = expect_integer(&left, "`Quotient` X", span)?;
    let divisor = expect_integer(&right, "`Quotient` Y", span)?;
    if divisor == 0 {
        return Ok(NativeResult::Failure("division by zero"));
    }

    let dividend = dividend as i128;
    let divisor = divisor as i128;
    let modulus = divisor.abs();
    let remainder = ((dividend % modulus) + modulus) % modulus;
    let quotient = (dividend - remainder) / divisor;
    Ok(NativeResult::Value(Value::Int(integer_from_i128(
        quotient,
        "`Quotient` result",
        span,
    )?)))
}

fn native_clamp(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value, left, right]: [Value; 3] = args.try_into().expect("arity checked by caller");
    if matches!(
        (&value, &left, &right),
        (Value::Int(_), Value::Int(_), Value::Int(_))
    ) {
        let value = expect_integer(&value, "`Clamp` value", span)?;
        let left = expect_integer(&left, "`Clamp` lower bound", span)?;
        let right = expect_integer(&right, "`Clamp` upper bound", span)?;
        let mut values = [value, left, right];
        values.sort_unstable();
        return Ok(NativeResult::Value(Value::Int(values[1])));
    }

    let value = expect_number(&value, "`Clamp` value", span)?;
    let left = expect_number(&left, "`Clamp` lower bound", span)?;
    let right = expect_number(&right, "`Clamp` upper bound", span)?;
    let mut values = [value, left, right];
    values.sort_by(verse_float_order);
    Ok(NativeResult::Value(Value::Float(values[1])))
}

fn native_lerp(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [from, to, parameter]: [Value; 3] = args.try_into().expect("arity checked by caller");
    let from = expect_number(&from, "`Lerp` from", span)?;
    let to = expect_number(&to, "`Lerp` to", span)?;
    let parameter = expect_number(&parameter, "`Lerp` parameter", span)?;
    if !from.is_finite() || !to.is_finite() || !parameter.is_finite() {
        return Err(VerseError::runtime_at(
            "`Lerp` expected finite arguments",
            span,
        ));
    }
    Ok(NativeResult::Value(Value::Float(
        from * (1.0 - parameter) + to * parameter,
    )))
}

fn native_abs(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    match runtime_number(&value) {
        Some(RuntimeNumber::Int(value)) => Ok(NativeResult::Value(Value::Int(
            value
                .checked_abs()
                .ok_or_else(|| VerseError::runtime_at("`Abs` integer overflow", span))?,
        ))),
        Some(RuntimeNumber::Float(value)) => Ok(NativeResult::Value(Value::Float(value.abs()))),
        Some(RuntimeNumber::Rational(_)) => Err(VerseError::runtime_at(
            "`Abs` expected `int` or `float`, got rational",
            span,
        )),
        None => Err(VerseError::runtime_at(
            format!("`Abs` expected `int` or `float`, got {value}"),
            span,
        )),
    }
}

fn native_min(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    if matches!((&left, &right), (Value::Int(_), Value::Int(_))) {
        let left = expect_integer(&left, "`Min` X", span)?;
        let right = expect_integer(&right, "`Min` Y", span)?;
        return Ok(NativeResult::Value(Value::Int(left.min(right))));
    }

    let left = expect_number(&left, "`Min` X", span)?;
    let right = expect_number(&right, "`Min` Y", span)?;
    if left.is_nan() || right.is_nan() {
        return Ok(NativeResult::Value(Value::Float(f64::NAN)));
    }
    Ok(NativeResult::Value(Value::Float(left.min(right))))
}

fn native_max(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    if matches!((&left, &right), (Value::Int(_), Value::Int(_))) {
        let left = expect_integer(&left, "`Max` X", span)?;
        let right = expect_integer(&right, "`Max` Y", span)?;
        return Ok(NativeResult::Value(Value::Int(left.max(right))));
    }

    let left = expect_number(&left, "`Max` X", span)?;
    let right = expect_number(&right, "`Max` Y", span)?;
    if left.is_nan() || right.is_nan() {
        return Ok(NativeResult::Value(Value::Float(f64::NAN)));
    }
    Ok(NativeResult::Value(Value::Float(left.max(right))))
}

fn verse_float_order(left: &f64, right: &f64) -> std::cmp::Ordering {
    match (left.is_nan(), right.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => left
            .partial_cmp(right)
            .expect("finite or infinite floats should compare"),
    }
}

fn integer_from_i128(value: i128, context: &str, span: Span) -> Result<i64, VerseError> {
    i64::try_from(value)
        .map_err(|_| VerseError::runtime_at(format!("{context} is outside int range"), span))
}

fn float_integer_result(value: f64, context: &str, span: Span) -> Result<i64, VerseError> {
    if !value.is_finite() {
        return Err(VerseError::runtime_at(
            format!("{context} expected a finite value"),
            span,
        ));
    }
    const I64_MAX_EXCLUSIVE_AS_F64: f64 = 9_223_372_036_854_775_808.0;
    if value < i64::MIN as f64 || value >= I64_MAX_EXCLUSIVE_AS_F64 {
        return Err(VerseError::runtime_at(
            format!("{context} result is outside int range"),
            span,
        ));
    }
    Ok(value as i64)
}

fn rational_floor_to_i64(value: RationalValue) -> i64 {
    value.numerator.div_euclid(value.denominator)
}

fn rational_ceil_to_i64(value: RationalValue) -> i64 {
    let floor = rational_floor_to_i64(value);
    if value.numerator.rem_euclid(value.denominator) == 0 {
        floor
    } else {
        floor + 1
    }
}

fn native_ceil(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    if let Some(RuntimeNumber::Rational(value)) = runtime_number(&value) {
        return Ok(NativeResult::Value(Value::Int(rational_ceil_to_i64(value))));
    }
    let value = expect_number(&value, "`Ceil` value", span)?;
    if !value.is_finite() {
        return Ok(NativeResult::Failure("value is not finite"));
    }
    Ok(NativeResult::Value(Value::Int(float_integer_result(
        value.ceil(),
        "`Ceil`",
        span,
    )?)))
}

fn native_floor(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    if let Some(RuntimeNumber::Rational(value)) = runtime_number(&value) {
        return Ok(NativeResult::Value(Value::Int(rational_floor_to_i64(
            value,
        ))));
    }
    let value = expect_number(&value, "`Floor` value", span)?;
    if !value.is_finite() {
        return Ok(NativeResult::Failure("value is not finite"));
    }
    Ok(NativeResult::Value(Value::Int(float_integer_result(
        value.floor(),
        "`Floor`",
        span,
    )?)))
}

fn native_round(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    if matches!(runtime_number(&value), Some(RuntimeNumber::Rational(_))) {
        return Err(VerseError::runtime_at(
            "`Round` expected `float`, got rational",
            span,
        ));
    }
    let value = expect_number(&value, "`Round` value", span)?;
    if !value.is_finite() {
        return Ok(NativeResult::Failure("value is not finite"));
    }
    Ok(NativeResult::Value(Value::Int(float_integer_result(
        round_ties_even(value),
        "`Round`",
        span,
    )?)))
}

fn native_int(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    if matches!(runtime_number(&value), Some(RuntimeNumber::Rational(_))) {
        return Err(VerseError::runtime_at(
            "`Int` expected `float`, got rational",
            span,
        ));
    }
    let value = expect_number(&value, "`Int` value", span)?;
    if !value.is_finite() {
        return Ok(NativeResult::Failure("value is not finite"));
    }
    Ok(NativeResult::Value(Value::Int(float_integer_result(
        value.trunc(),
        "`Int`",
        span,
    )?)))
}

fn native_sqrt(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Sqrt` value", f64::sqrt)
}

fn native_sin(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Sin` value", f64::sin)
}

fn native_cos(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Cos` value", f64::cos)
}

fn native_tan(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Tan` value", f64::tan)
}

fn native_arcsin(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`ArcSin` value", f64::asin)
}

fn native_arccos(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`ArcCos` value", f64::acos)
}

fn native_arctan(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    match args.as_slice() {
        [value] => Ok(NativeResult::Value(Value::Float(
            expect_number(value, "`ArcTan` value", span)?.atan(),
        ))),
        [y, x] => Ok(NativeResult::Value(Value::Float(
            expect_number(y, "`ArcTan` Y", span)?.atan2(expect_number(x, "`ArcTan` X", span)?),
        ))),
        _ => Err(VerseError::runtime_at(
            format!("`ArcTan` expected 1..=2 arguments, got {}", args.len()),
            span,
        )),
    }
}

fn native_sinh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Sinh` value", f64::sinh)
}

fn native_cosh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Cosh` value", f64::cosh)
}

fn native_tanh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Tanh` value", f64::tanh)
}

fn native_arsinh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`ArSinh` value", f64::asinh)
}

fn native_arcosh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`ArCosh` value", f64::acosh)
}

fn native_artanh(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`ArTanh` value", f64::atanh)
}

fn native_pow(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [base, exponent]: [Value; 2] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Float(
        expect_number(&base, "`Pow` A", span)?.powf(expect_number(&exponent, "`Pow` B", span)?),
    )))
}

fn native_exp(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Exp` value", f64::exp)
}

fn native_ln(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    native_unary_number(args, span, "`Ln` value", f64::ln)
}

fn native_log(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [base, value]: [Value; 2] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Float(
        expect_number(&value, "`Log` X", span)?.log(expect_number(&base, "`Log` B", span)?),
    )))
}

fn native_sgn(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    match runtime_number(&value) {
        Some(RuntimeNumber::Int(value)) => Ok(NativeResult::Value(Value::Int(if value > 0 {
            1
        } else if value < 0 {
            -1
        } else {
            0
        }))),
        Some(RuntimeNumber::Float(value)) => {
            Ok(NativeResult::Value(Value::Float(if value.is_nan() {
                f64::NAN
            } else if value > 0.0 {
                1.0
            } else if value < 0.0 {
                -1.0
            } else {
                0.0
            })))
        }
        Some(RuntimeNumber::Rational(_)) => Err(VerseError::runtime_at(
            "`Sgn` expected `int` or `float`, got rational",
            span,
        )),
        None => Err(VerseError::runtime_at(
            format!("`Sgn` expected `int` or `float`, got {value}"),
            span,
        )),
    }
}

fn native_is_almost_equal(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right, tolerance]: [Value; 3] = args.try_into().expect("arity checked by caller");
    let left = expect_number(&left, "`IsAlmostEqual` Val1", span)?;
    let right = expect_number(&right, "`IsAlmostEqual` Val2", span)?;
    let tolerance = expect_number(&tolerance, "`IsAlmostEqual` AbsoluteTolerance", span)?;
    if (left - right).abs() <= tolerance {
        Ok(NativeResult::Value(Value::None))
    } else {
        Ok(NativeResult::Failure("values are not within tolerance"))
    }
}

fn native_unary_number(
    args: Vec<Value>,
    span: Span,
    context: &str,
    operation: fn(f64) -> f64,
) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Float(operation(expect_number(
        &value, context, span,
    )?))))
}

fn round_ties_even(value: f64) -> f64 {
    let floor = value.floor();
    let fraction = value - floor;
    if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if (floor / 2.0).fract() == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}
