use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use rand::{Rng, rngs::OsRng};

use crate::ast::{Expr, Param, TypeName, TypeParam};
use crate::error::VerseError;
use crate::token::Span;

mod builtins;
mod bytecode;
pub(crate) use bytecode::{
    bytecode_call_native_cancel_method, bytecode_call_native_event_method,
    bytecode_call_native_subscribable_method, bytecode_call_native_subscription_cancel_method,
    bytecode_class_instance_value, bytecode_class_type_value, bytecode_event_signal_payload,
    bytecode_external_return_value, bytecode_external_value, bytecode_interface_type_value,
    bytecode_load_field_value, bytecode_modifier_stack_add,
    bytecode_modifier_stack_ordered_modifiers, bytecode_native_member_value,
    bytecode_new_running_task, bytecode_struct_type_value,
};
mod color_ops;
use color_ops::{
    color_pair_value, color_scale_value, native_make_color_alpha, native_make_color_from_hex,
    native_make_color_from_hsv, native_make_color_from_srgb, native_make_color_from_srgb_values,
    native_make_hsv_from_color, native_make_srgb_from_color, native_over,
};
mod numeric;
pub use numeric::RationalValue;
pub(crate) use numeric::rational_or_int;
use numeric::{RuntimeNumber, numeric_values_equal, runtime_number, runtime_number_to_rational};
mod scalar_ops;
use scalar_ops::{RuntimeNumberOp, expect_index_integer, expect_integer, expect_number};
mod player_map;
use player_map::{PLAYER_MAP_RECORD_LIMIT_BYTES, player_map_value_size};
mod string_ops;
pub(crate) use string_ops::replace_string_byte_failable;
use string_ops::{string_char_values, string_equals_char_array, string_value_to_char_array};
mod task;
pub use task::{RuntimeSuspension, RuntimeTask};
mod value_ops;
use value_ops::value_copy;
mod validation;
use validation::{char_array_to_string, expect_color_value};

type NativeFn = fn(Vec<Value>, Span) -> Result<NativeResult, VerseError>;

thread_local! {
    static CURRENT_EPOCH_SECONDS: RefCell<Option<f64>> = const { RefCell::new(None) };
    static SIMULATION_START_INSTANT: RefCell<Option<Instant>> = const { RefCell::new(None) };
    static RUNTIME_CLASS_TYPES: RefCell<HashMap<String, RuntimeClassTypeInfo>> = RefCell::new(HashMap::new());
}

pub enum NativeResult {
    Value(Value),
    Failure(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeClassTypeInfo {
    pub(crate) name: String,
    pub(crate) base: Option<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) unique: bool,
    pub(crate) abstract_class: bool,
    pub(crate) epic_internal_class: bool,
    pub(crate) final_class: bool,
    pub(crate) final_super: bool,
    pub(crate) concrete: bool,
    pub(crate) castable: bool,
}

pub(crate) fn register_runtime_class_type(info: RuntimeClassTypeInfo) {
    RUNTIME_CLASS_TYPES.with(|types| {
        types.borrow_mut().insert(info.name.clone(), info);
    });
}

pub(crate) fn register_runtime_class_types<I>(infos: I)
where
    I: IntoIterator<Item = RuntimeClassTypeInfo>,
{
    RUNTIME_CLASS_TYPES.with(|types| {
        let mut types = types.borrow_mut();
        for info in infos {
            types.insert(info.name.clone(), info);
        }
    });
}

fn runtime_class_type_info(name: &str) -> Option<RuntimeClassTypeInfo> {
    RUNTIME_CLASS_TYPES.with(|types| types.borrow().get(name).cloned())
}

#[derive(Clone, Default)]
pub struct Env;

pub(crate) fn with_stable_runtime_epoch<T>(
    body: impl FnOnce() -> Result<T, VerseError>,
) -> Result<T, VerseError> {
    let previous_epoch_seconds =
        CURRENT_EPOCH_SECONDS.with(|seconds| -> Result<Option<f64>, VerseError> {
            Ok(seconds.replace(Some(current_unix_epoch_seconds(Span::new(0, 0, 1, 1))?)))
        })?;
    let result = body();
    CURRENT_EPOCH_SECONDS.with(|seconds| {
        seconds.replace(previous_epoch_seconds);
    });
    result
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
    ExternalFunction {
        params: Vec<TypeName>,
        effects: Vec<String>,
        return_type: Box<TypeName>,
    },
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
        final_super: bool,
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
    SubscribableEventIntrnl {
        payload: Option<TypeName>,
        waiters: Rc<RefCell<Vec<Rc<RuntimeTask>>>>,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: Rc<RefCell<u64>>,
    },
    SubscribableEvent {
        payload: TypeName,
        waiters: Rc<RefCell<Vec<Rc<RuntimeTask>>>>,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: Rc<RefCell<u64>>,
    },
    StickyEvent {
        payload: Option<TypeName>,
        waiters: Rc<RefCell<Vec<Rc<RuntimeTask>>>>,
        signal: Rc<RefCell<Option<Value>>>,
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
    Subtype(TypeName),
    CastableSubtype(TypeName),
    ConcreteSubtype(TypeName),
    Type(TypeName),
    ClassifiableSubset(Rc<RefCell<Vec<Value>>>),
    ClassifiableSubsetKey {
        entries: Rc<RefCell<Vec<RuntimeClassifiableSubsetEntry>>>,
        entry_id: u64,
    },
    ClassifiableSubsetVar {
        entries: Rc<RefCell<Vec<RuntimeClassifiableSubsetEntry>>>,
        next_key: Rc<RefCell<u64>>,
    },
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
    NativeArrayMethod {
        name: String,
        receiver: Box<Value>,
    },
    NativeResultMethod {
        name: &'static str,
        result: Box<Value>,
    },
    NativeEventMethod {
        name: &'static str,
        payload: Option<TypeName>,
        waiters: Option<Rc<RefCell<Vec<Rc<RuntimeTask>>>>>,
        subscribers: Option<Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>>,
        sticky_signal: Option<Rc<RefCell<Option<Value>>>>,
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
    pub name: String,
    pub mutable: bool,
    pub final_member: bool,
    pub access: RuntimeAccessLevel,
    pub owner: Option<String>,
    pub default: Option<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAccessLevel {
    Public,
    Internal,
    Protected,
    Private,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeClassInstanceField {
    pub(crate) owner_class: String,
    pub(crate) name: String,
    pub(crate) mutable: bool,
    pub(crate) predicts: bool,
    pub(crate) predicts_extern: bool,
    pub(crate) value: Value,
}

#[derive(Clone)]
pub struct RuntimeClassMethod {
    pub qualifier: Option<String>,
    pub name: String,
    pub final_member: bool,
    pub params: Vec<Param>,
    pub effects: Vec<String>,
    pub body: Option<Box<Expr>>,
    pub closure: Env,
    pub super_type: Option<Box<Value>>,
    pub extension_methods: Rc<Vec<RuntimeExtensionMethod>>,
}

#[derive(Clone)]
pub struct RuntimeExtensionMethod {
    pub name: String,
    pub module_name: Option<String>,
    pub receiver: Param,
    pub params: Vec<Param>,
    pub effects: Vec<String>,
    pub body: Box<Expr>,
    pub closure: Env,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeModifierEntry {
    id: u64,
    position: RationalValue,
    order: u64,
    modifier: Value,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeClassifiableSubsetEntry {
    pub(crate) id: u64,
    pub(crate) value: Value,
}

#[derive(Clone)]
pub struct RuntimeSubscriptionEntry {
    pub id: u64,
    pub callback: Value,
}

#[derive(Clone)]
pub struct RuntimeClassBlock {
    pub body: Box<Expr>,
    pub closure: Env,
    pub super_type: Option<Box<Value>>,
    pub extension_methods: Rc<Vec<RuntimeExtensionMethod>>,
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
            (
                Self::ExternalFunction {
                    params: left_params,
                    effects: left_effects,
                    return_type: left_return_type,
                },
                Self::ExternalFunction {
                    params: right_params,
                    effects: right_effects,
                    return_type: right_return_type,
                },
            ) => {
                left_params == right_params
                    && left_effects == right_effects
                    && left_return_type == right_return_type
            }
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
            (Self::ClassType { name: left, .. }, Self::ClassType { name: right, .. }) => {
                left == right
            }
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
            (
                Self::SubscribableEventIntrnl {
                    payload: left_payload,
                    waiters: left_waiters,
                    subscribers: left_subscribers,
                    ..
                },
                Self::SubscribableEventIntrnl {
                    payload: right_payload,
                    waiters: right_waiters,
                    subscribers: right_subscribers,
                    ..
                },
            ) => {
                left_payload == right_payload
                    && Rc::ptr_eq(left_waiters, right_waiters)
                    && Rc::ptr_eq(left_subscribers, right_subscribers)
            }
            (
                Self::SubscribableEvent {
                    payload: left_payload,
                    waiters: left_waiters,
                    subscribers: left_subscribers,
                    ..
                },
                Self::SubscribableEvent {
                    payload: right_payload,
                    waiters: right_waiters,
                    subscribers: right_subscribers,
                    ..
                },
            ) => {
                left_payload == right_payload
                    && Rc::ptr_eq(left_waiters, right_waiters)
                    && Rc::ptr_eq(left_subscribers, right_subscribers)
            }
            (
                Self::StickyEvent {
                    payload: left_payload,
                    waiters: left_waiters,
                    signal: left_signal,
                },
                Self::StickyEvent {
                    payload: right_payload,
                    waiters: right_waiters,
                    signal: right_signal,
                },
            ) => {
                left_payload == right_payload
                    && Rc::ptr_eq(left_waiters, right_waiters)
                    && Rc::ptr_eq(left_signal, right_signal)
            }
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
            (Self::Subtype(left), Self::Subtype(right)) => left == right,
            (Self::CastableSubtype(left), Self::CastableSubtype(right)) => left == right,
            (Self::ConcreteSubtype(left), Self::ConcreteSubtype(right)) => left == right,
            (Self::Type(left), Self::Type(right)) => left == right,
            (Self::ClassifiableSubset(left), Self::ClassifiableSubset(right)) => {
                *left.borrow() == *right.borrow()
            }
            (
                Self::ClassifiableSubsetKey {
                    entries: left_entries,
                    entry_id: left_id,
                },
                Self::ClassifiableSubsetKey {
                    entries: right_entries,
                    entry_id: right_id,
                },
            ) => left_id == right_id && Rc::ptr_eq(left_entries, right_entries),
            (
                Self::ClassifiableSubsetVar { entries: left, .. },
                Self::ClassifiableSubsetVar { entries: right, .. },
            ) => Rc::ptr_eq(left, right),
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
            Self::ExternalFunction { params, .. } => {
                write!(formatter, "<external function/{}>", params.len())
            }
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
            Self::SubscribableEventIntrnl { payload, .. } => {
                if let Some(payload) = payload {
                    write!(
                        formatter,
                        "<subscribable_event_intrnl({})>",
                        render_runtime_type_name(payload)
                    )
                } else {
                    write!(formatter, "<subscribable_event_intrnl()>")
                }
            }
            Self::SubscribableEvent { payload, .. } => {
                write!(
                    formatter,
                    "<subscribable_event({})>",
                    render_runtime_type_name(payload)
                )
            }
            Self::StickyEvent { payload, .. } => {
                if let Some(payload) = payload {
                    write!(
                        formatter,
                        "<sticky_event({})>",
                        render_runtime_type_name(payload)
                    )
                } else {
                    write!(formatter, "<sticky_event()>")
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
            Self::Subtype(item) => {
                write!(formatter, "<subtype({})>", render_runtime_type_name(item))
            }
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
            Self::Type(item) => write!(formatter, "<type({})>", render_runtime_type_name(item)),
            Self::ClassifiableSubset(items) => {
                write!(formatter, "<classifiable_subset({})>", items.borrow().len())
            }
            Self::ClassifiableSubsetKey { .. } => write!(formatter, "<classifiable_subset_key>"),
            Self::ClassifiableSubsetVar { entries, .. } => {
                write!(
                    formatter,
                    "<classifiable_subset_var({})>",
                    entries.borrow().len()
                )
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
            Self::NativeArrayMethod { name, .. } => write!(formatter, "<native {name}>"),
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

#[derive(Clone)]
struct CallValue {
    name: Option<String>,
    optional: bool,
    value: Value,
    span: Span,
}

fn rendered_call_argument_name(name: &str, optional: bool) -> String {
    if optional {
        format!("?{name}")
    } else {
        name.to_string()
    }
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
        TypeName::Type => "type".to_string(),
        TypeName::TypeBounds { lower, upper } => format!(
            "type({},{})",
            render_runtime_type_name(lower),
            render_runtime_type_name(upper)
        ),
        TypeName::IntRange { min, max } => format!("int_range({min},{max})"),
        TypeName::FloatRange(range) => {
            format!("float_range({},{})", range.min.render(), range.max.render())
        }
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
        TypeName::Applied { name, args } if name == "option" && args.len() == 1 => {
            format!("?{}", render_runtime_type_name(&args[0]))
        }
        TypeName::Applied { name, args } => render_runtime_parametric_type_name(name, args),
        TypeName::Named(name) => name.clone(),
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

fn tuple_value_to_array(value: Value) -> Value {
    match value {
        Value::Tuple(items) => array_value(items.iter().map(value_copy).collect()),
        other => other,
    }
}

fn type_name_is_string_char(name: &TypeName) -> bool {
    matches!(name, TypeName::Char | TypeName::Char8)
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

pub(crate) fn bytecode_native_array_method_value(receiver: Value, name: &str) -> Option<Value> {
    match receiver {
        Value::Array(_) | Value::String(_) if array_method_name_is_supported(name) => {
            Some(Value::NativeArrayMethod {
                name: name.to_string(),
                receiver: Box::new(receiver),
            })
        }
        Value::ClassifiableSubset(_) if is_classifiable_subset_method_name(name) => {
            Some(Value::NativeArrayMethod {
                name: name.to_string(),
                receiver: Box::new(receiver),
            })
        }
        Value::ClassifiableSubsetVar { .. } if is_classifiable_subset_var_method_name(name) => {
            Some(Value::NativeArrayMethod {
                name: name.to_string(),
                receiver: Box::new(receiver),
            })
        }
        Value::ClassInstance { ref class_name, .. }
            if name == "IsOfType"
                && runtime_class_type_info(class_name).is_some_and(|info| info.castable) =>
        {
            Some(Value::NativeArrayMethod {
                name: name.to_string(),
                receiver: Box::new(receiver),
            })
        }
        _ => None,
    }
}

pub(crate) fn bytecode_call_native_array_method(
    receiver: &Value,
    name: &str,
    args: Vec<Value>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match receiver {
        Value::Array(items) => {
            eval_array_method_failable(name, items.borrow().as_slice(), args, span)
        }
        Value::String(text) => eval_string_array_method_failable(name, text, args, span),
        Value::ClassifiableSubset(items) => {
            match eval_classifiable_subset_method(
                name,
                items.borrow().as_slice(),
                args.as_slice(),
                span,
            )? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            }
        }
        Value::ClassifiableSubsetVar { entries, next_key } => {
            match eval_classifiable_subset_var_method(
                name,
                entries.clone(),
                next_key.clone(),
                args.as_slice(),
                span,
            )? {
                NativeResult::Value(value) => Ok(Some(value)),
                NativeResult::Failure(_) => Ok(None),
            }
        }
        Value::ClassInstance { .. } if name == "IsOfType" => {
            eval_class_instance_is_of_type_method(receiver, args.as_slice(), span)
        }
        other => Err(VerseError::runtime_at(
            format!("value `{other}` has no bracket method `{name}`"),
            span,
        )),
    }
}

pub(crate) fn bytecode_color_add_values(left: &Value, right: &Value) -> Option<Value> {
    color_pair_value(left, right, RuntimeNumberOp::Add)
}

pub(crate) fn bytecode_color_subtract_values(left: &Value, right: &Value) -> Option<Value> {
    color_pair_value(left, right, RuntimeNumberOp::Subtract)
}

pub(crate) fn bytecode_color_multiply_or_scale_values(
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    if let Some(value) = color_pair_value(left, right, RuntimeNumberOp::Multiply) {
        return Ok(Some(value));
    }
    if let Some(value) = color_scale_value(left, right, RuntimeNumberOp::Multiply, span)? {
        return Ok(Some(value));
    }
    color_scale_value(right, left, RuntimeNumberOp::Multiply, span)
}

pub(crate) fn bytecode_color_divide_values(
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    color_scale_value(left, right, RuntimeNumberOp::Divide, span)
}

pub(crate) fn bytecode_native_function_value(name: &str) -> Option<Value> {
    let (arity, decides, function): (Option<usize>, bool, NativeFn) = match name {
        "print" => (None, false, native_print),
        "Print" => (None, false, native_print),
        "assert_eq" => (Some(2), false, native_assert_eq),
        "str" => (Some(1), false, native_str),
        "Err" => (Some(1), false, native_err),
        "ToDiagnostic" => (Some(1), false, native_to_diagnostic),
        "GetSecondsSinceEpoch" => (Some(0), false, native_get_seconds_since_epoch),
        "GetSession" => (Some(0), false, native_get_session),
        "GetSimulationElapsedTime" => (Some(0), false, native_get_simulation_elapsed_time),
        "FitsInPlayerMap" => (Some(1), true, native_fits_in_player_map),
        "Mod" => (Some(2), true, native_mod),
        "Quotient" => (Some(2), true, native_quotient),
        "BitAnd" => (Some(2), false, native_bit_and),
        "BitOr" => (Some(2), false, native_bit_or),
        "BitXor" => (Some(2), false, native_bit_xor),
        "BitNot" => (Some(1), false, native_bit_not),
        "Clamp" => (Some(3), false, native_clamp),
        "Lerp" => (Some(3), false, native_lerp),
        "Abs" => (Some(1), false, native_abs),
        "Min" => (Some(2), false, native_min),
        "Max" => (Some(2), false, native_max),
        "Ceil" => (Some(1), true, native_ceil),
        "Floor" => (Some(1), true, native_floor),
        "Round" => (Some(1), true, native_round),
        "Int" => (Some(1), true, native_int),
        "Sqrt" => (Some(1), false, native_sqrt),
        "Sin" => (Some(1), false, native_sin),
        "Cos" => (Some(1), false, native_cos),
        "Tan" => (Some(1), false, native_tan),
        "ArcSin" => (Some(1), false, native_arcsin),
        "ArcCos" => (Some(1), false, native_arccos),
        "ArcTan" => (None, false, native_arctan),
        "Sinh" => (Some(1), false, native_sinh),
        "Cosh" => (Some(1), false, native_cosh),
        "Tanh" => (Some(1), false, native_tanh),
        "ArSinh" => (Some(1), false, native_arsinh),
        "ArCosh" => (Some(1), false, native_arcosh),
        "ArTanh" => (Some(1), false, native_artanh),
        "Pow" => (Some(2), false, native_pow),
        "Exp" => (Some(1), false, native_exp),
        "Ln" => (Some(1), false, native_ln),
        "Log" => (Some(2), false, native_log),
        "Sgn" => (Some(1), false, native_sgn),
        "IsAlmostEqual" => (Some(3), true, native_is_almost_equal),
        "MakeColorFromSRGB" => (Some(3), false, native_make_color_from_srgb),
        "MakeColorFromSRGBValues" => (Some(3), false, native_make_color_from_srgb_values),
        "MakeSRGBFromColor" => (Some(1), false, native_make_srgb_from_color),
        "MakeColorFromHex" => (Some(1), false, native_make_color_from_hex),
        "MakeColorFromHSV" => (Some(3), false, native_make_color_from_hsv),
        "MakeHSVFromColor" => (Some(1), false, native_make_hsv_from_color),
        "MakeColorAlpha" => (Some(4), false, native_make_color_alpha),
        "Over" => (Some(2), false, native_over),
        "ToString" => (Some(1), false, native_to_string),
        "Localize" => (Some(1), false, native_localize),
        "Join" => (Some(2), false, native_join),
        "GetRandomFloat" => (Some(2), false, native_get_random_float),
        "GetRandomInt" => (Some(2), false, native_get_random_int),
        "Shuffle" => (Some(1), false, native_shuffle),
        "Concatenate" => (None, false, native_concatenate),
        "Replace" => (Some(4), true, native_replace),
        "ConcatenateMaps" => (Some(2), false, native_concatenate_maps),
        "MakeClassifiableSubset" => (Some(1), false, native_make_classifiable_subset),
        "MakeClassifiableSubsetVar" => (Some(1), false, native_make_classifiable_subset_var),
        "GetCastableFinalSuperClass" => (Some(2), true, native_get_castable_final_super_class),
        "GetCastableFinalSuperClassFromType" => (
            Some(2),
            true,
            native_get_castable_final_super_class_from_type,
        ),
        "MakeSuccess" => (Some(1), false, native_make_success),
        "MakeError" => (Some(1), false, native_make_error),
        "Sleep" => (Some(1), false, native_sleep),
        "__verse_sync"
        | "__verse_race"
        | "__verse_rush"
        | "__verse_branch"
        | "__verse_begin_defer_scope"
        | "__verse_end_defer_scope"
        | "__verse_defer" => (None, false, native_vm_intrinsic_placeholder),
        _ => return None,
    };
    Some(Value::NativeFunction {
        name: match name {
            "print" => "print",
            "Print" => "Print",
            "assert_eq" => "assert_eq",
            "str" => "str",
            "Err" => "Err",
            "ToDiagnostic" => "ToDiagnostic",
            "GetSecondsSinceEpoch" => "GetSecondsSinceEpoch",
            "GetSession" => "GetSession",
            "GetSimulationElapsedTime" => "GetSimulationElapsedTime",
            "FitsInPlayerMap" => "FitsInPlayerMap",
            "Mod" => "Mod",
            "Quotient" => "Quotient",
            "BitAnd" => "BitAnd",
            "BitOr" => "BitOr",
            "BitXor" => "BitXor",
            "BitNot" => "BitNot",
            "Clamp" => "Clamp",
            "Lerp" => "Lerp",
            "Abs" => "Abs",
            "Min" => "Min",
            "Max" => "Max",
            "Ceil" => "Ceil",
            "Floor" => "Floor",
            "Round" => "Round",
            "Int" => "Int",
            "Sqrt" => "Sqrt",
            "Sin" => "Sin",
            "Cos" => "Cos",
            "Tan" => "Tan",
            "ArcSin" => "ArcSin",
            "ArcCos" => "ArcCos",
            "ArcTan" => "ArcTan",
            "Sinh" => "Sinh",
            "Cosh" => "Cosh",
            "Tanh" => "Tanh",
            "ArSinh" => "ArSinh",
            "ArCosh" => "ArCosh",
            "ArTanh" => "ArTanh",
            "Pow" => "Pow",
            "Exp" => "Exp",
            "Ln" => "Ln",
            "Log" => "Log",
            "Sgn" => "Sgn",
            "IsAlmostEqual" => "IsAlmostEqual",
            "MakeColorFromSRGB" => "MakeColorFromSRGB",
            "MakeColorFromSRGBValues" => "MakeColorFromSRGBValues",
            "MakeSRGBFromColor" => "MakeSRGBFromColor",
            "MakeColorFromHex" => "MakeColorFromHex",
            "MakeColorFromHSV" => "MakeColorFromHSV",
            "MakeHSVFromColor" => "MakeHSVFromColor",
            "MakeColorAlpha" => "MakeColorAlpha",
            "Over" => "Over",
            "ToString" => "ToString",
            "Localize" => "Localize",
            "Join" => "Join",
            "GetRandomFloat" => "GetRandomFloat",
            "GetRandomInt" => "GetRandomInt",
            "Shuffle" => "Shuffle",
            "Concatenate" => "Concatenate",
            "Replace" => "Replace",
            "ConcatenateMaps" => "ConcatenateMaps",
            "MakeClassifiableSubset" => "MakeClassifiableSubset",
            "MakeClassifiableSubsetVar" => "MakeClassifiableSubsetVar",
            "GetCastableFinalSuperClass" => "GetCastableFinalSuperClass",
            "GetCastableFinalSuperClassFromType" => "GetCastableFinalSuperClassFromType",
            "MakeSuccess" => "MakeSuccess",
            "MakeError" => "MakeError",
            "Sleep" => "Sleep",
            "__verse_sync" => "__verse_sync",
            "__verse_race" => "__verse_race",
            "__verse_rush" => "__verse_rush",
            "__verse_branch" => "__verse_branch",
            "__verse_begin_defer_scope" => "__verse_begin_defer_scope",
            "__verse_end_defer_scope" => "__verse_end_defer_scope",
            "__verse_defer" => "__verse_defer",
            _ => unreachable!("matched native function names above"),
        },
        arity,
        decides,
        function,
    })
}

fn bytecode_call_native_function_values(
    name: &'static str,
    arity: Option<usize>,
    decides: bool,
    function: NativeFn,
    args: Vec<CallValue>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match call_native_function(name, arity, function, args, span)? {
        NativeResult::Value(value) => Ok(Some(value)),
        NativeResult::Failure(reason) if decides => {
            let _ = reason;
            Ok(None)
        }
        NativeResult::Failure(reason) => Err(VerseError::runtime_at(
            format!("`{name}` failed: {reason}"),
            span,
        )),
    }
}

pub(crate) fn bytecode_call_native_function_named(
    name: &'static str,
    arity: Option<usize>,
    decides: bool,
    function: NativeFn,
    positional_args: Vec<Value>,
    named_args: Vec<(String, Value, Span)>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    let mut args = positional_args
        .into_iter()
        .map(|value| CallValue {
            name: None,
            optional: false,
            value,
            span,
        })
        .collect::<Vec<_>>();
    args.extend(named_args.into_iter().map(|(name, value, span)| CallValue {
        name: Some(name),
        optional: false,
        value,
        span,
    }));
    bytecode_call_native_function_values(name, arity, decides, function, args, span)
}

fn array_method_name_is_supported(name: &str) -> bool {
    matches!(
        name,
        "Slice"
            | "Find"
            | "RemoveFirstElement"
            | "RemoveAllElements"
            | "Remove"
            | "RemoveElement"
            | "ReplaceFirstElement"
            | "ReplaceAllElements"
            | "ReplaceElement"
            | "Insert"
            | "ReplaceAll"
    )
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

fn modifier_stack_position(stack: &Value, first: bool) -> Value {
    let Value::ModifierStack { entries, .. } = stack else {
        return Value::Rational(RationalValue::from_int(0));
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
    Value::Rational(position.unwrap_or_else(|| RationalValue::from_int(0)))
}

fn compare_rational(left: RationalValue, right: RationalValue) -> std::cmp::Ordering {
    let left_scaled = left.numerator * right.denominator;
    let right_scaled = right.numerator * left.denominator;
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
        "Replace" => vec![
            vec!["Input"],
            vec!["StartIndex"],
            vec!["StopIndex"],
            vec!["ElementsToReplaceWith"],
        ],
        "GetRandomFloat" | "GetRandomInt" => vec![vec!["Low"], vec!["High"]],
        "Shuffle" => vec![vec!["Input"]],
        "GetCastableFinalSuperClass" => vec![vec!["base_type"], vec!["Instance"]],
        "GetCastableFinalSuperClassFromType" => vec![vec!["base_type"], vec!["sub_type"]],
        "Sleep" => vec![vec!["Seconds"]],
        "Clamp" => vec![vec!["Value"], vec!["A"], vec!["B"]],
        "Lerp" => vec![vec!["From"], vec!["To"], vec!["Parameter"]],
        "Abs" | "Ceil" | "Floor" => vec![vec!["Value"]],
        "Min" | "Max" => vec![vec!["X"], vec!["Y"]],
        "BitAnd" | "BitOr" | "BitXor" => vec![vec!["X"], vec!["Y"]],
        "BitNot" => vec![vec!["X"]],
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
    match payload {
        TypeName::Any | TypeName::Comparable => true,
        TypeName::Type | TypeName::TypeBounds { .. } => runtime_value_is_type_value(value),
        TypeName::Int => matches!(value, Value::Int(_)),
        TypeName::IntRange { min, max } => {
            matches!(value, Value::Int(value) if *min <= *value && *value <= *max)
        }
        TypeName::Float => matches!(value, Value::Int(_) | Value::Float(_)),
        TypeName::FloatRange(range) => match value {
            Value::Int(value) => range.contains(*value as f64),
            Value::Float(value) => range.contains(*value),
            _ => false,
        },
        TypeName::Rational => matches!(value, Value::Int(_) | Value::Rational(_)),
        TypeName::Number => runtime_number(value).is_some(),
        TypeName::Bool => matches!(value, Value::Bool(_)),
        TypeName::String => match value {
            Value::String(_) => true,
            Value::Array(items) => char_array_to_string(items.borrow().as_slice()).is_some(),
            _ => false,
        },
        TypeName::Message => matches!(value, Value::String(_) | Value::Diagnostic(_)),
        TypeName::Char | TypeName::Char8 => matches!(value, Value::Char(_)),
        TypeName::Char32 => matches!(value, Value::Char32(_)),
        TypeName::None => matches!(value, Value::None),
        TypeName::Array(item_type) => match value {
            Value::String(_) if item_type.as_deref().is_some_and(type_name_is_string_char) => true,
            Value::Array(items) => item_type.as_deref().is_none_or(|item_type| {
                items
                    .borrow()
                    .iter()
                    .all(|item| runtime_event_payload_matches(item, item_type))
            }),
            _ => false,
        },
        TypeName::Map(key_type, value_type) | TypeName::WeakMap(key_type, value_type) => {
            match value {
                Value::Map(entries) => entries.borrow().iter().all(|(key, value)| {
                    runtime_event_payload_matches(key, key_type)
                        && runtime_event_payload_matches(value, value_type)
                }),
                _ => false,
            }
        }
        TypeName::Tuple(item_types) => match value {
            Value::Tuple(items) if items.len() == item_types.len() => items
                .iter()
                .zip(item_types)
                .all(|(item, item_type)| runtime_event_payload_matches(item, item_type)),
            _ => false,
        },
        TypeName::Option(item_type) => match value {
            Value::Option(Some(value)) => runtime_event_payload_matches(value, item_type),
            Value::Option(None) | Value::Bool(false) => true,
            _ => false,
        },
        TypeName::Applied { name, args } if name == "option" && args.len() == 1 => {
            runtime_event_payload_matches(value, &TypeName::Option(Box::new(args[0].clone())))
        }
        TypeName::Named(name) | TypeName::Applied { name, .. } => {
            runtime_named_value_matches(value, name)
        }
        TypeName::Function | TypeName::FunctionSignature { .. } => matches!(
            value,
            Value::NativeFunction { .. }
                | Value::NativeArrayMethod { .. }
                | Value::NativeResultMethod { .. }
                | Value::NativeEventMethod { .. }
                | Value::NativeSubscribableMethod { .. }
                | Value::NativeTaskMethod { .. }
                | Value::NativeModifierMethod { .. }
                | Value::NativeCancelMethod { .. }
                | Value::NativeSubscriptionCancelMethod { .. }
                | Value::ExternalFunction { .. }
                | Value::External
        ),
    }
}

fn runtime_value_is_type_value(value: &Value) -> bool {
    matches!(
        value,
        Value::StructType { .. }
            | Value::ClassType { .. }
            | Value::InterfaceType { .. }
            | Value::ParametricType { .. }
            | Value::Subtype(_)
            | Value::CastableSubtype(_)
            | Value::ConcreteSubtype(_)
            | Value::Type(_)
            | Value::External
    )
}

fn runtime_named_value_matches(value: &Value, expected: &str) -> bool {
    let expected = expected.rsplit('.').next().unwrap_or(expected);
    match value {
        Value::External => true,
        Value::Diagnostic(_) => expected == "diagnostic",
        Value::EnumValue { enum_name, .. } => runtime_names_match(enum_name, expected),
        Value::StructInstance { struct_name, .. } => runtime_names_match(struct_name, expected),
        Value::ClassInstance { class_name, .. } => runtime_names_match(class_name, expected),
        Value::EnumType { name, .. }
        | Value::StructType { name, .. }
        | Value::ClassType { name, .. }
        | Value::InterfaceType { name, .. }
        | Value::Module { name, .. } => runtime_names_match(name, expected),
        Value::Type(TypeName::Named(name) | TypeName::Applied { name, .. }) => {
            runtime_names_match(name, expected)
        }
        Value::Result {
            succeeded: true, ..
        } => expected == "result" || expected == "success_result",
        Value::Result {
            succeeded: false, ..
        } => expected == "result" || expected == "error_result",
        Value::SubscribableEventIntrnl { .. } => matches!(
            expected,
            "subscribable_event_intrnl"
                | "event"
                | "listenable"
                | "awaitable"
                | "signalable"
                | "subscribable"
        ),
        Value::SubscribableEvent { .. } => matches!(
            expected,
            "subscribable_event"
                | "subscribable_event_intrnl"
                | "event"
                | "listenable"
                | "awaitable"
                | "signalable"
                | "subscribable"
        ),
        Value::StickyEvent { .. } => {
            matches!(
                expected,
                "sticky_event" | "event" | "awaitable" | "signalable"
            )
        }
        Value::ClassifiableSubset(_) => expected == "classifiable_subset",
        Value::ClassifiableSubsetKey { .. } => expected == "classifiable_subset_key",
        Value::ClassifiableSubsetVar { .. } => expected == "classifiable_subset_var",
        Value::ModifierCancelHandle { .. } | Value::SubscriptionCancelHandle { .. } => {
            expected == "cancelable"
        }
        _ => false,
    }
}

fn runtime_names_match(actual: &str, expected: &str) -> bool {
    let actual_local = actual.rsplit('.').next().unwrap_or(actual);
    actual == expected || actual_local == expected
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

fn native_vm_intrinsic_placeholder(
    _args: Vec<Value>,
    span: Span,
) -> Result<NativeResult, VerseError> {
    Err(VerseError::runtime_at(
        "VM intrinsic must be handled by the bytecode call path",
        span,
    ))
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
    if seconds < 0.0 {
        return Ok(NativeResult::Value(Value::None));
    }
    Ok(NativeResult::Value(Value::Pending))
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

fn native_replace(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [input, start, stop, replacement]: [Value; 4] =
        args.try_into().expect("arity checked by caller");
    let Value::Array(input) = input else {
        return Err(VerseError::runtime_at(
            "`Replace` Input expected an array argument",
            span,
        ));
    };
    let Some(start) = array_position_value(&start, "`Replace` StartIndex", span)? else {
        return Ok(NativeResult::Failure("start index is negative"));
    };
    let Some(stop) = array_position_value(&stop, "`Replace` StopIndex", span)? else {
        return Ok(NativeResult::Failure("stop index is negative"));
    };
    let Value::Array(replacement) = tuple_value_to_array(replacement) else {
        return Err(VerseError::runtime_at(
            "`Replace` ElementsToReplaceWith expected an array argument",
            span,
        ));
    };

    let input = input.borrow();
    if !valid_slice_range(start, stop, input.len()) {
        return Ok(NativeResult::Failure("invalid replacement range"));
    }
    let mut result: Vec<Value> = input[..start].iter().map(value_copy).collect();
    result.extend(replacement.borrow().iter().map(value_copy));
    result.extend(input[stop..].iter().map(value_copy));
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

fn native_make_classifiable_subset_var(
    args: Vec<Value>,
    _span: Span,
) -> Result<NativeResult, VerseError> {
    let [elements]: [Value; 1] = args.try_into().expect("arity checked by caller");
    let Value::Array(elements) = elements else {
        return Err(VerseError::runtime(
            "`MakeClassifiableSubsetVar` expected an array argument",
        ));
    };

    let entries = elements
        .borrow()
        .iter()
        .enumerate()
        .map(|(id, value)| RuntimeClassifiableSubsetEntry {
            id: id as u64,
            value: value_copy(value),
        })
        .collect();
    Ok(NativeResult::Value(Value::ClassifiableSubsetVar {
        entries: Rc::new(RefCell::new(entries)),
        next_key: Rc::new(RefCell::new(elements.borrow().len() as u64)),
    }))
}

fn native_get_castable_final_super_class(
    args: Vec<Value>,
    _span: Span,
) -> Result<NativeResult, VerseError> {
    let [base_type, instance]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let Some(base_type) = runtime_query_type_ref(&base_type) else {
        return Ok(NativeResult::Failure(
            "base_type is not a class or interface type",
        ));
    };
    let Value::ClassInstance { class_name, .. } = instance else {
        return Ok(NativeResult::Failure("Instance is not a class instance"));
    };
    let Some(sub_type) = runtime_class_type_info(&class_name) else {
        return Ok(NativeResult::Failure("Instance class is unknown"));
    };
    get_castable_final_super_class_from_info(&base_type, sub_type)
}

fn native_get_castable_final_super_class_from_type(
    args: Vec<Value>,
    _span: Span,
) -> Result<NativeResult, VerseError> {
    let [base_type, sub_type]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let Some(base_type) = runtime_query_type_ref(&base_type) else {
        return Ok(NativeResult::Failure(
            "base_type is not a class or interface type",
        ));
    };
    let Some(sub_type) = runtime_class_info_from_value(&sub_type) else {
        return Ok(NativeResult::Failure("sub_type is not a class type"));
    };
    get_castable_final_super_class_from_info(&base_type, sub_type)
}

#[derive(Clone)]
enum RuntimeQueryTypeRef {
    Class(String),
    Interface(String),
}

impl RuntimeQueryTypeRef {
    fn name(&self) -> &str {
        match self {
            Self::Class(name) | Self::Interface(name) => name,
        }
    }

    fn is_interface(&self) -> bool {
        matches!(self, Self::Interface(_))
    }
}

fn runtime_query_type_ref(value: &Value) -> Option<RuntimeQueryTypeRef> {
    match value {
        Value::ClassType { name, .. } => Some(RuntimeQueryTypeRef::Class(name.clone())),
        Value::InterfaceType { name, .. } => Some(RuntimeQueryTypeRef::Interface(name.clone())),
        Value::Type(TypeName::Named(name)) => {
            runtime_class_type_info(name).map(|_| RuntimeQueryTypeRef::Class(name.clone()))
        }
        _ => None,
    }
}

fn runtime_class_info_from_value(value: &Value) -> Option<RuntimeClassTypeInfo> {
    match value {
        Value::ClassType {
            name,
            base,
            interfaces,
            unique,
            abstract_class,
            epic_internal_class,
            final_class,
            final_super,
            concrete,
            castable,
            ..
        } => Some(RuntimeClassTypeInfo {
            name: name.clone(),
            base: base.clone(),
            interfaces: interfaces.clone(),
            unique: *unique,
            abstract_class: *abstract_class,
            epic_internal_class: *epic_internal_class,
            final_class: *final_class,
            final_super: *final_super,
            concrete: *concrete,
            castable: *castable,
        }),
        Value::Type(TypeName::Named(name)) => runtime_class_type_info(name),
        _ => None,
    }
}

fn get_castable_final_super_class_from_info(
    base_type: &RuntimeQueryTypeRef,
    mut current: RuntimeClassTypeInfo,
) -> Result<NativeResult, VerseError> {
    let base_name = base_type.name();
    while current.name != base_name {
        let parent_matches = current.base.as_deref() == Some(base_name);
        let interface_matches =
            base_type.is_interface() && current.interfaces.iter().any(|name| name == base_name);
        if parent_matches || interface_matches {
            return if current.final_super && current.castable {
                Ok(NativeResult::Value(runtime_class_type_value(current)))
            } else {
                Ok(NativeResult::Failure(
                    "direct subclass is not final_super and castable",
                ))
            };
        }

        let Some(parent) = current.base.as_deref().and_then(runtime_class_type_info) else {
            break;
        };
        current = parent;
    }

    Ok(NativeResult::Failure("no castable final_super subclass"))
}

fn runtime_class_type_value(info: RuntimeClassTypeInfo) -> Value {
    register_runtime_class_type(info.clone());
    Value::ClassType {
        name: info.name,
        base: info.base,
        interfaces: info.interfaces,
        unique: info.unique,
        abstract_class: info.abstract_class,
        epic_internal_class: info.epic_internal_class,
        final_class: info.final_class,
        final_super: info.final_super,
        concrete: info.concrete,
        castable: info.castable,
        fields: Vec::new(),
        methods: Vec::new(),
        blocks: Vec::new(),
    }
}

fn is_classifiable_subset_method_name(name: &str) -> bool {
    matches!(
        name,
        "Contains"
            | "NotContains"
            | "ContainsAny"
            | "ContainsAll"
            | "ContainsNone"
            | "FilterByType"
    )
}

fn is_classifiable_subset_var_method_name(name: &str) -> bool {
    matches!(name, "Read" | "Write" | "Add" | "Remove") || is_classifiable_subset_method_name(name)
}

fn eval_class_instance_is_of_type_method(
    receiver: &Value,
    args: &[Value],
    span: Span,
) -> Result<Option<Value>, VerseError> {
    if args.len() != 1 {
        return Err(VerseError::runtime_at(
            format!("`IsOfType` expected 1 arguments, got {}", args.len()),
            span,
        ));
    }

    let Value::ClassInstance { class_name, .. } = receiver else {
        return Err(VerseError::runtime_at(
            "`IsOfType` expected a class instance receiver",
            span,
        ));
    };
    let Some(query_type) = runtime_query_type_ref(&args[0]) else {
        return Err(VerseError::runtime_at(
            format!("`IsOfType` expected a class type argument, got {}", args[0]),
            span,
        ));
    };

    if runtime_class_instance_matches_query_type(class_name, &query_type) {
        Ok(Some(Value::None))
    } else {
        Ok(None)
    }
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
            if items
                .iter()
                .any(|item| classifiable_subset_item_matches_type(item, &args[0]))
            {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("element is not present"))
            }
        }
        "NotContains" => {
            if items
                .iter()
                .any(|item| classifiable_subset_item_matches_type(item, &args[0]))
            {
                Ok(NativeResult::Failure("element is present"))
            } else {
                Ok(NativeResult::Value(Value::None))
            }
        }
        "ContainsAny" => {
            let values = expect_classifiable_subset_argument_array(name, &args[0], span)?;
            if values.borrow().iter().any(|candidate| {
                items
                    .iter()
                    .any(|item| classifiable_subset_item_matches_type(item, candidate))
            }) {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("no elements are present"))
            }
        }
        "ContainsAll" => {
            let values = expect_classifiable_subset_argument_array(name, &args[0], span)?;
            if values.borrow().iter().all(|candidate| {
                items
                    .iter()
                    .any(|item| classifiable_subset_item_matches_type(item, candidate))
            }) {
                Ok(NativeResult::Value(Value::None))
            } else {
                Ok(NativeResult::Failure("not all elements are present"))
            }
        }
        "ContainsNone" => {
            let values = expect_classifiable_subset_argument_array(name, &args[0], span)?;
            if values.borrow().iter().any(|candidate| {
                items
                    .iter()
                    .any(|item| classifiable_subset_item_matches_type(item, candidate))
            }) {
                Ok(NativeResult::Failure("an element is present"))
            } else {
                Ok(NativeResult::Value(Value::None))
            }
        }
        "FilterByType" => {
            let values = items
                .iter()
                .filter(|item| classifiable_subset_item_matches_type(item, &args[0]))
                .map(value_copy)
                .collect();
            Ok(NativeResult::Value(Value::ClassifiableSubset(Rc::new(
                RefCell::new(values),
            ))))
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown classifiable_subset method `{name}`"),
            span,
        )),
    }
}

fn eval_classifiable_subset_var_method(
    name: &str,
    entries: Rc<RefCell<Vec<RuntimeClassifiableSubsetEntry>>>,
    next_key: Rc<RefCell<u64>>,
    args: &[Value],
    span: Span,
) -> Result<NativeResult, VerseError> {
    match name {
        "Read" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`Read` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            Ok(NativeResult::Value(Value::ClassifiableSubset(Rc::new(
                RefCell::new(classifiable_subset_var_values(&entries)),
            ))))
        }
        "Write" => {
            let [set] = args else {
                return Err(VerseError::runtime_at(
                    format!("`Write` expected 1 arguments, got {}", args.len()),
                    span,
                ));
            };
            let Value::ClassifiableSubset(values) = set else {
                return Err(VerseError::runtime_at(
                    format!("`Write` expected classifiable_subset argument, got {set}"),
                    span,
                ));
            };
            let mut next = next_key.borrow_mut();
            let mut target = entries.borrow_mut();
            target.clear();
            for value in values.borrow().iter() {
                let id = *next;
                *next = next.saturating_add(1);
                target.push(RuntimeClassifiableSubsetEntry {
                    id,
                    value: value_copy(value),
                });
            }
            Ok(NativeResult::Value(Value::None))
        }
        "Add" => {
            let [value] = args else {
                return Err(VerseError::runtime_at(
                    format!("`Add` expected 1 arguments, got {}", args.len()),
                    span,
                ));
            };
            let id = {
                let mut next = next_key.borrow_mut();
                let id = *next;
                *next = next.saturating_add(1);
                id
            };
            entries.borrow_mut().push(RuntimeClassifiableSubsetEntry {
                id,
                value: value_copy(value),
            });
            Ok(NativeResult::Value(Value::ClassifiableSubsetKey {
                entries,
                entry_id: id,
            }))
        }
        "Remove" => {
            let [key] = args else {
                return Err(VerseError::runtime_at(
                    format!("`Remove` expected 1 arguments, got {}", args.len()),
                    span,
                ));
            };
            let Value::ClassifiableSubsetKey {
                entries: key_entries,
                entry_id,
            } = key
            else {
                return Err(VerseError::runtime_at(
                    format!("`Remove` expected classifiable_subset_key argument, got {key}"),
                    span,
                ));
            };
            if !Rc::ptr_eq(&entries, key_entries) {
                return Ok(NativeResult::Failure("key does not belong to this set"));
            }
            let mut values = entries.borrow_mut();
            let previous_len = values.len();
            values.retain(|entry| entry.id != *entry_id);
            if values.len() == previous_len {
                Ok(NativeResult::Failure("element was not present"))
            } else {
                Ok(NativeResult::Value(Value::None))
            }
        }
        _ if is_classifiable_subset_method_name(name) => {
            let values = classifiable_subset_var_values(&entries);
            eval_classifiable_subset_method(name, &values, args, span)
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown classifiable_subset_var method `{name}`"),
            span,
        )),
    }
}

fn classifiable_subset_var_values(
    entries: &Rc<RefCell<Vec<RuntimeClassifiableSubsetEntry>>>,
) -> Vec<Value> {
    entries
        .borrow()
        .iter()
        .map(|entry| value_copy(&entry.value))
        .collect()
}

fn classifiable_subset_item_matches_type(item: &Value, element_type: &Value) -> bool {
    match element_type {
        Value::ClassType { name, .. } => runtime_named_value_matches(item, name),
        Value::InterfaceType { name, .. } => runtime_named_value_matches(item, name),
        Value::Subtype(type_name)
        | Value::CastableSubtype(type_name)
        | Value::ConcreteSubtype(type_name)
        | Value::Type(type_name) => {
            item == element_type || runtime_event_payload_matches(item, type_name)
        }
        _ => item == element_type,
    }
}

fn runtime_class_instance_matches_query_type(
    class_name: &str,
    query_type: &RuntimeQueryTypeRef,
) -> bool {
    match query_type {
        RuntimeQueryTypeRef::Class(expected) => runtime_class_is_a(class_name, expected),
        RuntimeQueryTypeRef::Interface(expected) => {
            runtime_class_implements_interface(class_name, expected)
        }
    }
}

fn runtime_class_is_a(actual: &str, expected: &str) -> bool {
    let mut current = Some(actual.to_string());
    while let Some(class_name) = current {
        if runtime_names_match(&class_name, expected) {
            return true;
        }
        current = runtime_class_type_info(&class_name).and_then(|info| info.base);
    }
    false
}

fn runtime_class_implements_interface(actual: &str, expected: &str) -> bool {
    let mut current = Some(actual.to_string());
    while let Some(class_name) = current {
        let Some(info) = runtime_class_type_info(&class_name) else {
            return false;
        };
        if info
            .interfaces
            .iter()
            .any(|interface| runtime_names_match(interface, expected))
        {
            return true;
        }
        current = info.base;
    }
    false
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

    let modulus = divisor
        .checked_abs()
        .ok_or_else(|| VerseError::runtime_at("`Mod` divisor overflow", span))?;
    let remainder = ((dividend % modulus) + modulus) % modulus;
    Ok(NativeResult::Value(Value::Int(remainder)))
}

fn native_quotient(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    let dividend = expect_integer(&left, "`Quotient` X", span)?;
    let divisor = expect_integer(&right, "`Quotient` Y", span)?;
    if divisor == 0 {
        return Ok(NativeResult::Failure("division by zero"));
    }

    let modulus = divisor
        .checked_abs()
        .ok_or_else(|| VerseError::runtime_at("`Quotient` divisor overflow", span))?;
    let remainder = ((dividend % modulus) + modulus) % modulus;
    let quotient = dividend
        .checked_sub(remainder)
        .and_then(|value| value.checked_div(divisor))
        .ok_or_else(|| VerseError::runtime_at("`Quotient` integer overflow", span))?;
    Ok(NativeResult::Value(Value::Int(quotient)))
}

fn native_bit_and(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Int(
        expect_integer(&left, "`BitAnd` X", span)? & expect_integer(&right, "`BitAnd` Y", span)?,
    )))
}

fn native_bit_or(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Int(
        expect_integer(&left, "`BitOr` X", span)? | expect_integer(&right, "`BitOr` Y", span)?,
    )))
}

fn native_bit_xor(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [left, right]: [Value; 2] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Int(
        expect_integer(&left, "`BitXor` X", span)? ^ expect_integer(&right, "`BitXor` Y", span)?,
    )))
}

fn native_bit_not(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    Ok(NativeResult::Value(Value::Int(!expect_integer(
        &value,
        "`BitNot` X",
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

fn rational_floor_to_int(
    value: RationalValue,
    context: &str,
    span: Span,
) -> Result<i64, VerseError> {
    i64::try_from(value.numerator.div_euclid(value.denominator))
        .map_err(|_| VerseError::runtime_at(format!("{context} result is outside int range"), span))
}

fn rational_ceil_to_int(
    value: RationalValue,
    context: &str,
    span: Span,
) -> Result<i64, VerseError> {
    let floor = value.numerator.div_euclid(value.denominator);
    if value.numerator.rem_euclid(value.denominator) == 0 {
        i64::try_from(floor).map_err(|_| {
            VerseError::runtime_at(format!("{context} result is outside int range"), span)
        })
    } else {
        i64::try_from(floor + 1).map_err(|_| {
            VerseError::runtime_at(format!("{context} result is outside int range"), span)
        })
    }
}

fn native_ceil(args: Vec<Value>, span: Span) -> Result<NativeResult, VerseError> {
    let [value]: [Value; 1] = args.try_into().expect("arity checked by caller");
    if let Some(RuntimeNumber::Rational(value)) = runtime_number(&value) {
        return Ok(NativeResult::Value(Value::Int(rational_ceil_to_int(
            value, "`Ceil`", span,
        )?)));
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
        return Ok(NativeResult::Value(Value::Int(rational_floor_to_int(
            value, "`Floor`", span,
        )?)));
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
    let left = expect_float(&left, "`IsAlmostEqual` Val1", span)?;
    let right = expect_float(&right, "`IsAlmostEqual` Val2", span)?;
    let tolerance = expect_float(&tolerance, "`IsAlmostEqual` AbsoluteTolerance", span)?;
    if (left - right).abs() <= tolerance {
        Ok(NativeResult::Value(Value::None))
    } else {
        Ok(NativeResult::Failure("values are not within tolerance"))
    }
}

fn expect_float(value: &Value, context: &str, span: Span) -> Result<f64, VerseError> {
    match value {
        Value::Float(value) => Ok(*value),
        other => Err(VerseError::runtime_at(
            format!("{context} expected `float`, got {other}"),
            span,
        )),
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
