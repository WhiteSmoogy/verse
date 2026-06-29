use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use crate::eval::Value;
use crate::token::Span;

pub type NativeResult<T> = Result<T, NativeError>;
pub type NativeInt = i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeError {
    Failure(String),
    Runtime(String),
}

impl NativeError {
    pub fn failure(message: impl Into<String>) -> Self {
        Self::Failure(message.into())
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(message.into())
    }
}

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failure(message) | Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<std::io::Error> for NativeError {
    fn from(error: std::io::Error) -> Self {
        Self::runtime(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InjectedNativeFunction {
    pub runtime_name: String,
    pub arity: usize,
    pub effects: Vec<String>,
}

impl InjectedNativeFunction {
    pub fn decides(&self) -> bool {
        self.effects.iter().any(|effect| effect == "decides")
    }

    pub fn suspends(&self) -> bool {
        self.effects.iter().any(|effect| effect == "suspends")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFunctionSignature {
    pub runtime_name: &'static str,
    pub arity: usize,
    pub effects: &'static [&'static str],
}

impl NativeFunctionSignature {
    fn to_injected(&self) -> InjectedNativeFunction {
        InjectedNativeFunction {
            runtime_name: self.runtime_name.to_string(),
            arity: self.arity,
            effects: self
                .effects
                .iter()
                .map(|effect| (*effect).to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NativeCallContext {
    pub runtime_name: &'static str,
}

#[derive(Debug, Clone)]
pub enum NativeCallResult {
    Value(Value),
    Failure(String),
    RuntimeError(String),
}

impl NativeCallResult {
    pub fn from_result<T: IntoNativeValue>(result: NativeResult<T>) -> Self {
        match result {
            Ok(value) => Self::Value(value.into_native_value()),
            Err(NativeError::Failure(message)) => Self::Failure(message),
            Err(NativeError::Runtime(message)) => Self::RuntimeError(message),
        }
    }
}

type NativeHandler = dyn Fn(Vec<Value>, Span) -> NativeCallResult + Send + Sync + 'static;

#[derive(Clone, Default)]
pub struct NativeRegistry {
    handlers: Arc<HashMap<NativeFunctionKey, Arc<NativeHandler>>>,
}

impl NativeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> NativeRegistryBuilder {
        NativeRegistryBuilder::default()
    }

    pub fn call(
        &self,
        runtime_name: &str,
        arity: usize,
        args: Vec<Value>,
        span: Span,
    ) -> Option<NativeCallResult> {
        self.handlers
            .get(&NativeFunctionKey::new(runtime_name, arity))
            .map(|handler| handler(args, span))
    }

    pub fn merge(&self, other: &NativeRegistry) -> NativeRegistry {
        let mut handlers = (*self.handlers).clone();
        handlers.extend(
            other
                .handlers
                .iter()
                .map(|(key, handler)| (key.clone(), Arc::<NativeHandler>::clone(handler))),
        );
        NativeRegistry {
            handlers: Arc::new(handlers),
        }
    }
}

#[derive(Default)]
pub struct NativeRegistryBuilder {
    handlers: HashMap<NativeFunctionKey, Arc<NativeHandler>>,
}

impl NativeRegistryBuilder {
    pub fn register(
        &mut self,
        runtime_name: &'static str,
        arity: usize,
        handler: impl Fn(Vec<Value>, Span) -> NativeCallResult + Send + Sync + 'static,
    ) {
        self.handlers.insert(
            NativeFunctionKey::new(runtime_name, arity),
            Arc::new(handler),
        );
    }

    pub fn build(self) -> NativeRegistry {
        NativeRegistry {
            handlers: Arc::new(self.handlers),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NativeFunctionKey {
    runtime_name: String,
    arity: usize,
}

impl NativeFunctionKey {
    fn new(runtime_name: impl Into<String>, arity: usize) -> Self {
        Self {
            runtime_name: runtime_name.into(),
            arity,
        }
    }
}

#[derive(Clone)]
pub struct InjectedNativeApi {
    path: &'static str,
    digest: &'static str,
    functions: &'static [NativeFunctionSignature],
    registry: NativeRegistry,
}

impl InjectedNativeApi {
    pub fn new(
        path: &'static str,
        digest: &'static str,
        functions: &'static [NativeFunctionSignature],
        registry: NativeRegistry,
    ) -> Self {
        Self {
            path,
            digest,
            functions,
            registry,
        }
    }

    pub fn path(&self) -> &'static str {
        self.path
    }

    pub fn digest(&self) -> &'static str {
        self.digest
    }

    pub fn functions(&self) -> impl Iterator<Item = InjectedNativeFunction> + '_ {
        self.functions
            .iter()
            .map(NativeFunctionSignature::to_injected)
    }

    pub fn registry(&self) -> &NativeRegistry {
        &self.registry
    }
}

#[derive(Clone, Default)]
pub struct NativeApiBundle {
    digests: Vec<&'static str>,
    absolute_paths: Vec<&'static str>,
    functions: Vec<InjectedNativeFunction>,
    registry: NativeRegistry,
}

impl NativeApiBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_apis(apis: impl IntoIterator<Item = InjectedNativeApi>) -> Self {
        let mut bundle = Self::new();
        for api in apis {
            bundle.push(api);
        }
        bundle
    }

    pub fn push(&mut self, api: InjectedNativeApi) {
        self.digests.push(api.digest());
        self.absolute_paths.push(api.path());
        self.functions.extend(api.functions());
        self.registry = self.registry.merge(api.registry());
    }

    pub fn source_with_digests(&self, source: &str) -> String {
        if self.digests.is_empty() {
            return source.to_string();
        }
        let mut merged = self.digests.join("\n");
        if !merged.ends_with('\n') {
            merged.push('\n');
        }
        merged.push_str(source);
        merged
    }

    pub fn absolute_paths(&self) -> &[&'static str] {
        &self.absolute_paths
    }

    pub fn functions(&self) -> &[InjectedNativeFunction] {
        &self.functions
    }

    pub fn registry(&self) -> &NativeRegistry {
        &self.registry
    }
}

pub trait FromNativeValue: Sized {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self>;
}

pub trait IntoNativeValue {
    fn into_native_value(self) -> Value;
}

pub fn native_type_name_matches(left: &str, right: &str) -> bool {
    let left_erased = native_erased_type_name(left);
    let right_erased = native_erased_type_name(right);
    left == right
        || left_erased == right_erased
        || native_local_type_name(left) == right
        || native_local_type_name(right) == left
        || native_local_type_name(left_erased) == right_erased
        || native_local_type_name(right_erased) == left_erased
        || native_local_type_name(left_erased) == native_local_type_name(right_erased)
}

fn native_erased_type_name(name: &str) -> &str {
    name.split_once('(')
        .map(|(generic, _)| generic)
        .unwrap_or(name)
}

fn native_local_type_name(name: &str) -> &str {
    name.rsplit_once('.')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

pub fn native_struct_fields(
    value: Value,
    expected_type: &str,
    name: &str,
) -> NativeResult<Vec<(String, Value)>> {
    match value {
        Value::StructInstance {
            struct_name,
            fields,
            ..
        } if native_type_name_matches(&struct_name, expected_type) => Ok(fields),
        other => Err(NativeError::runtime(format!(
            "`{name}` expected struct `{expected_type}`, got {other}"
        ))),
    }
}

pub fn take_native_struct_field(
    fields: &mut Vec<(String, Value)>,
    field_name: &str,
    type_name: &str,
) -> NativeResult<Value> {
    let index = fields
        .iter()
        .position(|(name, _)| name == field_name)
        .ok_or_else(|| {
            NativeError::runtime(format!(
                "struct `{type_name}` is missing field `{field_name}`"
            ))
        })?;
    Ok(fields.remove(index).1)
}

pub fn native_enum_variant(value: Value, expected_type: &str, name: &str) -> NativeResult<String> {
    match value {
        Value::EnumValue { enum_name, variant }
            if native_type_name_matches(&enum_name, expected_type) =>
        {
            Ok(variant)
        }
        other => Err(NativeError::runtime(format!(
            "`{name}` expected enum `{expected_type}`, got {other}"
        ))),
    }
}

impl FromNativeValue for Value {
    fn from_native_value(value: Value, _name: &str) -> NativeResult<Self> {
        Ok(value)
    }
}

impl IntoNativeValue for Value {
    fn into_native_value(self) -> Value {
        self
    }
}

impl FromNativeValue for i128 {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::Int(value) => Ok(i128::from(value)),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected int, got {other}"
            ))),
        }
    }
}

impl IntoNativeValue for i128 {
    fn into_native_value(self) -> Value {
        Value::Int(i64::try_from(self).expect("native i128 int return is outside i64 range"))
    }
}

impl FromNativeValue for () {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::None => Ok(()),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected void, got {other}"
            ))),
        }
    }
}

impl FromNativeValue for i64 {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::Int(value) => Ok(value),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected int, got {other}"
            ))),
        }
    }
}

impl IntoNativeValue for i64 {
    fn into_native_value(self) -> Value {
        Value::Int(self)
    }
}

impl FromNativeValue for f64 {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::Float(value) => Ok(value),
            Value::Int(value) => Ok(value as f64),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected float, got {other}"
            ))),
        }
    }
}

impl IntoNativeValue for f64 {
    fn into_native_value(self) -> Value {
        Value::Float(self)
    }
}

impl FromNativeValue for bool {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::Bool(value) => Ok(value),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected logic, got {other}"
            ))),
        }
    }
}

impl IntoNativeValue for bool {
    fn into_native_value(self) -> Value {
        Value::Bool(self)
    }
}

impl FromNativeValue for String {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::String(value) | Value::Diagnostic(value) => Ok(value),
            other => Err(NativeError::runtime(format!(
                "`{name}` expected string, got {other}"
            ))),
        }
    }
}

impl IntoNativeValue for String {
    fn into_native_value(self) -> Value {
        Value::String(self)
    }
}

impl IntoNativeValue for &str {
    fn into_native_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl IntoNativeValue for () {
    fn into_native_value(self) -> Value {
        Value::None
    }
}

impl<T: FromNativeValue> FromNativeValue for Vec<T> {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        let values = match value {
            Value::Array(items) => items.borrow().clone(),
            Value::Tuple(items) => items,
            other => {
                return Err(NativeError::runtime(format!(
                    "`{name}` expected array, got {other}"
                )));
            }
        };
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| T::from_native_value(value, &format!("{name}[{index}]")))
            .collect()
    }
}

impl<T: IntoNativeValue> IntoNativeValue for Vec<T> {
    fn into_native_value(self) -> Value {
        Value::Array(Rc::new(RefCell::new(
            self.into_iter()
                .map(IntoNativeValue::into_native_value)
                .collect(),
        )))
    }
}

impl<K, V> FromNativeValue for HashMap<K, V>
where
    K: FromNativeValue + Eq + Hash,
    V: FromNativeValue,
{
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        let entries = match value {
            Value::Map(entries) => entries.borrow().clone(),
            other => {
                return Err(NativeError::runtime(format!(
                    "`{name}` expected map, got {other}"
                )));
            }
        };
        entries
            .into_iter()
            .enumerate()
            .map(|(index, (key, value))| {
                let key = K::from_native_value(key, &format!("{name}.key[{index}]"))?;
                let value = V::from_native_value(value, &format!("{name}.value[{index}]"))?;
                Ok((key, value))
            })
            .collect()
    }
}

impl<K, V> IntoNativeValue for HashMap<K, V>
where
    K: IntoNativeValue,
    V: IntoNativeValue,
{
    fn into_native_value(self) -> Value {
        Value::Map(Rc::new(RefCell::new(
            self.into_iter()
                .map(|(key, value)| (key.into_native_value(), value.into_native_value()))
                .collect(),
        )))
    }
}

impl<T: FromNativeValue> FromNativeValue for Option<T> {
    fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
        match value {
            Value::Option(Some(value)) => T::from_native_value(*value, name).map(Some),
            Value::Option(None) | Value::Bool(false) => Ok(None),
            other => T::from_native_value(other, name).map(Some),
        }
    }
}

impl<T: IntoNativeValue> IntoNativeValue for Option<T> {
    fn into_native_value(self) -> Value {
        Value::Option(self.map(|value| Box::new(value.into_native_value())))
    }
}

macro_rules! impl_native_tuple {
    ($(($type_name:ident, $value_name:ident, $index:tt)),+ $(,)?) => {
        impl<$($type_name),+> FromNativeValue for ($($type_name,)+)
        where
            $($type_name: FromNativeValue),+
        {
            fn from_native_value(value: Value, name: &str) -> NativeResult<Self> {
                let items = match value {
                    Value::Tuple(items) => items,
                    other => {
                        return Err(NativeError::runtime(format!(
                            "`{name}` expected tuple, got {other}"
                        )));
                    }
                };
                let expected = 0usize $(+ { let _ = stringify!($type_name); 1usize })+;
                if items.len() != expected {
                    return Err(NativeError::runtime(format!(
                        "`{name}` expected tuple with {expected} elements, got {}",
                        items.len()
                    )));
                }
                let mut items = items.into_iter();
                $(
                    let $value_name = $type_name::from_native_value(
                        items.next().expect("tuple arity checked"),
                        &format!("{name}.{}", $index),
                    )?;
                )+
                Ok(($($value_name,)+))
            }
        }

        impl<$($type_name),+> IntoNativeValue for ($($type_name,)+)
        where
            $($type_name: IntoNativeValue),+
        {
            fn into_native_value(self) -> Value {
                let ($($value_name,)+) = self;
                Value::Tuple(vec![$($value_name.into_native_value()),+])
            }
        }
    };
}

impl_native_tuple!((A, a, 0));
impl_native_tuple!((A, a, 0), (B, b, 1));
impl_native_tuple!((A, a, 0), (B, b, 1), (C, c, 2));
impl_native_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3));
impl_native_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4));
impl_native_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5)
);
impl_native_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6)
);
impl_native_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7)
);
