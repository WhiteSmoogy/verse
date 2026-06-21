use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::TypeName;
use crate::error::VerseError;
use crate::token::Span;

use super::{
    RuntimeClassField, RuntimeClassInstanceField, RuntimeClassMethod, RuntimeExtensionMethod,
    RuntimeModifierEntry, RuntimeSubscriptionEntry, Value, qualify_runtime_named_value,
    tuple_value_to_array, value_copy,
};
#[derive(Clone)]
pub struct Env(Rc<RefCell<Scope>>);

struct Scope {
    values: HashMap<String, Binding>,
    type_aliases: HashMap<String, TypeName>,
    extension_methods: HashMap<String, Vec<RuntimeExtensionMethod>>,
    module_imports: Vec<Env>,
    module_name: Option<String>,
    parent: Option<Env>,
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
}

type ArraySnapshot = (Rc<RefCell<Vec<Value>>>, Vec<Value>);
type MapSnapshot = (Rc<RefCell<Vec<(Value, Value)>>>, Vec<(Value, Value)>);
type ClassFieldsSnapshot = (
    Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
    Vec<RuntimeClassInstanceField>,
);
type ModifierStackSnapshot = (
    Rc<RefCell<Vec<RuntimeModifierEntry>>>,
    Vec<RuntimeModifierEntry>,
    Rc<RefCell<u64>>,
    u64,
);
type SubscriptionSnapshot = (
    Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    Vec<RuntimeSubscriptionEntry>,
    Rc<RefCell<u64>>,
    u64,
);

struct ScopeSnapshot {
    env: Env,
    values: HashMap<String, Binding>,
    type_aliases: HashMap<String, TypeName>,
    extension_methods: HashMap<String, Vec<RuntimeExtensionMethod>>,
    module_imports: Vec<Env>,
    module_name: Option<String>,
}

pub(super) struct EnvTransaction {
    scopes: Vec<ScopeSnapshot>,
    arrays: Vec<ArraySnapshot>,
    maps: Vec<MapSnapshot>,
    class_fields: Vec<ClassFieldsSnapshot>,
    modifier_stacks: Vec<ModifierStackSnapshot>,
    subscriptions: Vec<SubscriptionSnapshot>,
}

struct TransactionCollector {
    seen_scopes: HashSet<usize>,
    seen_arrays: HashSet<usize>,
    seen_maps: HashSet<usize>,
    seen_class_fields: HashSet<usize>,
    seen_modifier_stacks: HashSet<usize>,
    seen_subscriptions: HashSet<usize>,
    scopes: Vec<ScopeSnapshot>,
    arrays: Vec<ArraySnapshot>,
    maps: Vec<MapSnapshot>,
    class_fields: Vec<ClassFieldsSnapshot>,
    modifier_stacks: Vec<ModifierStackSnapshot>,
    subscriptions: Vec<SubscriptionSnapshot>,
}

impl EnvTransaction {
    pub(super) fn capture(env: &Env) -> Self {
        let mut collector = TransactionCollector::new();
        collector.collect_env(env);
        Self {
            scopes: collector.scopes,
            arrays: collector.arrays,
            maps: collector.maps,
            class_fields: collector.class_fields,
            modifier_stacks: collector.modifier_stacks,
            subscriptions: collector.subscriptions,
        }
    }

    pub(super) fn restore(self) {
        for (items, snapshot) in self.arrays {
            *items.borrow_mut() = snapshot;
        }
        for (entries, snapshot) in self.maps {
            *entries.borrow_mut() = snapshot;
        }
        for (fields, snapshot) in self.class_fields {
            *fields.borrow_mut() = snapshot;
        }
        for (entries, entries_snapshot, next_order, next_order_snapshot) in self.modifier_stacks {
            *entries.borrow_mut() = entries_snapshot;
            *next_order.borrow_mut() = next_order_snapshot;
        }
        for (subscribers, subscribers_snapshot, next_id, next_id_snapshot) in self.subscriptions {
            *subscribers.borrow_mut() = subscribers_snapshot;
            *next_id.borrow_mut() = next_id_snapshot;
        }
        for snapshot in self.scopes {
            let mut scope = snapshot.env.0.borrow_mut();
            scope.values = snapshot.values;
            scope.type_aliases = snapshot.type_aliases;
            scope.extension_methods = snapshot.extension_methods;
            scope.module_imports = snapshot.module_imports;
            scope.module_name = snapshot.module_name;
        }
    }
}

impl TransactionCollector {
    pub(super) fn new() -> Self {
        Self {
            seen_scopes: HashSet::new(),
            seen_arrays: HashSet::new(),
            seen_maps: HashSet::new(),
            seen_class_fields: HashSet::new(),
            seen_modifier_stacks: HashSet::new(),
            seen_subscriptions: HashSet::new(),
            scopes: Vec::new(),
            arrays: Vec::new(),
            maps: Vec::new(),
            class_fields: Vec::new(),
            modifier_stacks: Vec::new(),
            subscriptions: Vec::new(),
        }
    }

    fn collect_env(&mut self, env: &Env) {
        let id = Rc::as_ptr(&env.0) as usize;
        if !self.seen_scopes.insert(id) {
            return;
        }

        let scope = env.0.borrow();
        let values = scope.values.clone();
        let type_aliases = scope.type_aliases.clone();
        let extension_methods = scope.extension_methods.clone();
        let module_imports = scope.module_imports.clone();
        let module_name = scope.module_name.clone();
        let parent = scope.parent.clone();
        drop(scope);

        self.scopes.push(ScopeSnapshot {
            env: env.clone(),
            values: values.clone(),
            type_aliases,
            extension_methods: extension_methods.clone(),
            module_imports: module_imports.clone(),
            module_name,
        });

        for binding in values.values() {
            self.collect_value(&binding.value);
        }
        for methods in extension_methods.values() {
            for method in methods {
                self.collect_env(&method.closure);
            }
        }
        for module in &module_imports {
            self.collect_env(module);
        }
        if let Some(parent) = parent {
            self.collect_env(&parent);
        }
    }

    fn collect_value(&mut self, value: &Value) {
        match value {
            Value::StructType { fields, .. } => {
                for field in fields {
                    if let Some(default) = &field.default {
                        self.collect_value(default);
                    }
                }
            }
            Value::StructInstance { fields, .. } => {
                for (_, value) in fields {
                    self.collect_value(value);
                }
            }
            Value::ClassType {
                fields,
                methods,
                blocks,
                ..
            } => {
                self.collect_runtime_class_fields(fields);
                self.collect_runtime_class_methods(methods);
                for block in blocks {
                    self.collect_env(&block.closure);
                    if let Some(super_type) = &block.super_type {
                        self.collect_value(super_type);
                    }
                    self.collect_runtime_extension_methods(&block.extension_methods);
                }
            }
            Value::InterfaceType {
                fields, methods, ..
            } => {
                self.collect_runtime_class_fields(fields);
                self.collect_runtime_class_methods(methods);
            }
            Value::ClassInstance {
                fields, methods, ..
            } => {
                self.collect_class_instance_fields(fields);
                self.collect_runtime_class_methods(methods);
            }
            Value::Array(items) => {
                let id = Rc::as_ptr(items) as usize;
                if self.seen_arrays.insert(id) {
                    let snapshot = items.borrow().clone();
                    for item in &snapshot {
                        self.collect_value(item);
                    }
                    self.arrays.push((items.clone(), snapshot));
                }
            }
            Value::Map(entries) => {
                let id = Rc::as_ptr(entries) as usize;
                if self.seen_maps.insert(id) {
                    let snapshot = entries.borrow().clone();
                    for (key, value) in &snapshot {
                        self.collect_value(key);
                        self.collect_value(value);
                    }
                    self.maps.push((entries.clone(), snapshot));
                }
            }
            Value::Tuple(items) => {
                for item in items {
                    self.collect_value(item);
                }
            }
            Value::Option(Some(value)) => self.collect_value(value),
            Value::Result { value, .. } => self.collect_value(value),
            Value::Subscribable {
                subscribers,
                next_subscriber_id,
                ..
            }
            | Value::Listenable {
                subscribers,
                next_subscriber_id,
                ..
            } => self.collect_subscriptions(subscribers, next_subscriber_id),
            Value::SubscriptionCancelHandle { subscribers, .. } => {
                let next_subscriber_id = Rc::new(RefCell::new(0));
                self.collect_subscriptions(subscribers, &next_subscriber_id);
            }
            Value::Generator { values, .. } => {
                let snapshot = values.borrow().clone();
                for item in &snapshot {
                    self.collect_value(item);
                }
            }
            Value::ClassifiableSubset(items) => {
                let snapshot = items.borrow().clone();
                for item in &snapshot {
                    self.collect_value(item);
                }
            }
            Value::Modifier { .. } => {}
            Value::ModifierStack {
                entries,
                next_order,
                ..
            } => {
                self.collect_modifier_stack(entries, next_order);
            }
            Value::ModifierCancelHandle { entries, .. } => {
                let next_order = Rc::new(RefCell::new(0));
                self.collect_modifier_stack(entries, &next_order);
            }
            Value::ParametricType { closure, .. } | Value::Function { closure, .. } => {
                self.collect_env(closure)
            }
            Value::Overload(overloads) => {
                for overload in overloads {
                    self.collect_value(overload);
                }
            }
            Value::BoundMethod {
                closure,
                super_type,
                extension_methods,
                fields,
                methods,
                ..
            } => {
                self.collect_env(closure);
                if let Some(super_type) = super_type {
                    self.collect_value(super_type);
                }
                self.collect_runtime_extension_methods(extension_methods);
                self.collect_class_instance_fields(fields);
                self.collect_runtime_class_methods(methods);
            }
            Value::NativeModifierMethod { receiver, .. } => self.collect_value(receiver),
            Value::NativeCancelMethod { entries, .. } => {
                let next_order = Rc::new(RefCell::new(0));
                self.collect_modifier_stack(entries, &next_order);
            }
            Value::NativeSubscribableMethod {
                subscribers,
                next_subscriber_id,
                ..
            } => self.collect_subscriptions(subscribers, next_subscriber_id),
            Value::NativeSubscriptionCancelMethod { subscribers, .. } => {
                let next_subscriber_id = Rc::new(RefCell::new(0));
                self.collect_subscriptions(subscribers, &next_subscriber_id);
            }
            Value::Module { env, .. } => self.collect_env(env),
            Value::Int(_)
            | Value::Float(_)
            | Value::Rational(_)
            | Value::Char(_)
            | Value::Char32(_)
            | Value::Bool(_)
            | Value::String(_)
            | Value::Diagnostic(_)
            | Value::External
            | Value::None
            | Value::Pending
            | Value::Suspended(_)
            | Value::Session
            | Value::Range { .. }
            | Value::EnumType { .. }
            | Value::EnumValue { .. }
            | Value::Event { .. }
            | Value::Awaitable { .. }
            | Value::Signalable { .. }
            | Value::Task(_)
            | Value::CastableSubtype(_)
            | Value::ConcreteSubtype(_)
            | Value::Option(None)
            | Value::NativeFunction { .. }
            | Value::NativeResultMethod { .. }
            | Value::NativeEventMethod { .. }
            | Value::NativeTaskMethod { .. } => {}
        }
    }

    fn collect_runtime_class_fields(&mut self, fields: &[RuntimeClassField]) {
        for field in fields {
            if let Some(default) = &field.default {
                self.collect_value(default);
            }
        }
    }

    fn collect_class_instance_fields(
        &mut self,
        fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
    ) {
        let id = Rc::as_ptr(fields) as usize;
        if self.seen_class_fields.insert(id) {
            let snapshot = fields.borrow().clone();
            for field in &snapshot {
                self.collect_value(&field.value);
            }
            self.class_fields.push((fields.clone(), snapshot));
        }
    }

    fn collect_modifier_stack(
        &mut self,
        entries: &Rc<RefCell<Vec<RuntimeModifierEntry>>>,
        next_order: &Rc<RefCell<u64>>,
    ) {
        let id = Rc::as_ptr(entries) as usize;
        if self.seen_modifier_stacks.insert(id) {
            let snapshot = entries.borrow().clone();
            for entry in &snapshot {
                self.collect_value(&entry.modifier);
            }
            self.modifier_stacks.push((
                entries.clone(),
                snapshot,
                next_order.clone(),
                *next_order.borrow(),
            ));
        }
    }

    fn collect_subscriptions(
        &mut self,
        subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        next_subscriber_id: &Rc<RefCell<u64>>,
    ) {
        let id = Rc::as_ptr(subscribers) as usize;
        if self.seen_subscriptions.insert(id) {
            let snapshot = subscribers.borrow().clone();
            for entry in &snapshot {
                self.collect_value(&entry.callback);
            }
            self.subscriptions.push((
                subscribers.clone(),
                snapshot,
                next_subscriber_id.clone(),
                *next_subscriber_id.borrow(),
            ));
        }
    }

    fn collect_runtime_class_methods(&mut self, methods: &[RuntimeClassMethod]) {
        for method in methods {
            self.collect_env(&method.closure);
            if let Some(super_type) = &method.super_type {
                self.collect_value(super_type);
            }
            self.collect_runtime_extension_methods(&method.extension_methods);
        }
    }

    fn collect_runtime_extension_methods(&mut self, methods: &[RuntimeExtensionMethod]) {
        for method in methods {
            self.collect_env(&method.closure);
        }
    }
}

impl Env {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(Scope {
            values: HashMap::new(),
            type_aliases: HashMap::new(),
            extension_methods: HashMap::new(),
            module_imports: Vec::new(),
            module_name: None,
            parent: None,
        })))
    }

    pub(super) fn child(parent: &Env) -> Self {
        let module_name = parent.0.borrow().module_name.clone();
        Self(Rc::new(RefCell::new(Scope {
            values: HashMap::new(),
            type_aliases: HashMap::new(),
            extension_methods: HashMap::new(),
            module_imports: Vec::new(),
            module_name,
            parent: Some(parent.clone()),
        })))
    }

    pub(super) fn module_name(&self) -> Option<String> {
        self.0.borrow().module_name.clone()
    }

    pub(super) fn qualified_module_name(&self, name: &str) -> String {
        self.0
            .borrow()
            .parent
            .as_ref()
            .and_then(Env::module_name)
            .map_or_else(|| name.to_string(), |parent| format!("{parent}.{name}"))
    }

    pub(super) fn qualify_module_scope(&self, module_name: &str) {
        let nested_modules = {
            let mut scope = self.0.borrow_mut();
            scope.module_name = Some(module_name.to_string());
            for methods in scope.extension_methods.values_mut() {
                for method in methods {
                    method.module_name = Some(module_name.to_string());
                }
            }
            scope
                .values
                .iter()
                .filter_map(|(name, binding)| match &binding.value {
                    Value::Module { env, .. } => Some((name.clone(), env.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        for (name, env) in nested_modules {
            env.qualify_module_scope(&format!("{module_name}.{name}"));
        }
    }

    pub(super) fn define(&self, name: impl Into<String>, value: Value, mutable: bool) {
        self.0.borrow_mut().values.insert(
            name.into(),
            Binding {
                value: value_copy(&value),
                mutable,
            },
        );
    }

    pub(super) fn define_function(&self, name: impl Into<String>, value: Value) {
        let name = name.into();
        let mut scope = self.0.borrow_mut();
        if let Some(binding) = scope.values.get_mut(&name) {
            match &mut binding.value {
                Value::Function { .. } => {
                    let previous = value_copy(&binding.value);
                    binding.value = Value::Overload(vec![previous, value_copy(&value)]);
                    binding.mutable = false;
                    return;
                }
                Value::Overload(overloads) => {
                    overloads.push(value_copy(&value));
                    binding.mutable = false;
                    return;
                }
                _ => {}
            }
        }
        scope.values.insert(
            name,
            Binding {
                value: value_copy(&value),
                mutable: false,
            },
        );
    }

    pub(super) fn define_type_alias(&self, name: impl Into<String>, target: TypeName) {
        self.0.borrow_mut().type_aliases.insert(name.into(), target);
    }

    pub(super) fn define_extension_method(&self, name: String, method: RuntimeExtensionMethod) {
        self.0
            .borrow_mut()
            .extension_methods
            .entry(name)
            .or_default()
            .push(method);
    }

    pub(super) fn import_module(&self, module_env: Env) {
        self.0.borrow_mut().module_imports.push(module_env);
    }

    pub(super) fn get_type_alias(&self, name: &str) -> Option<TypeName> {
        if name.contains('.')
            && let Some(target) = self.get_qualified_type_alias(name)
        {
            return Some(target);
        }

        let scope = self.0.borrow();
        if let Some(target) = scope.type_aliases.get(name) {
            return Some(target.clone());
        }
        scope
            .parent
            .as_ref()
            .and_then(|parent| parent.get_type_alias(name))
    }

    fn get_qualified_type_alias(&self, name: &str) -> Option<TypeName> {
        let mut parts = name.split('.');
        let first = parts.next()?;
        let Value::Module {
            env: mut module_env,
            ..
        } = self.get(first)?
        else {
            return None;
        };
        let remaining = parts.collect::<Vec<_>>();
        let (&last, modules) = remaining.split_last()?;
        for part in modules {
            let Value::Module { env, .. } = module_env.get_local(part)? else {
                return None;
            };
            module_env = env;
        }
        module_env.get_local_type_alias(last)
    }

    pub(super) fn get_local_type_alias(&self, name: &str) -> Option<TypeName> {
        self.0.borrow().type_aliases.get(name).cloned()
    }

    pub(super) fn resolve_type_name(&self, type_name: &TypeName) -> TypeName {
        self.resolve_type_name_inner(type_name, &mut Vec::new())
    }

    fn resolve_type_name_inner(
        &self,
        type_name: &TypeName,
        visiting: &mut Vec<String>,
    ) -> TypeName {
        match type_name {
            TypeName::Array(item) => TypeName::Array(
                item.as_deref()
                    .map(|item| Box::new(self.resolve_type_name_inner(item, visiting))),
            ),
            TypeName::Map(key, value) => TypeName::Map(
                Box::new(self.resolve_type_name_inner(key, visiting)),
                Box::new(self.resolve_type_name_inner(value, visiting)),
            ),
            TypeName::WeakMap(key, value) => TypeName::WeakMap(
                Box::new(self.resolve_type_name_inner(key, visiting)),
                Box::new(self.resolve_type_name_inner(value, visiting)),
            ),
            TypeName::Tuple(items) => TypeName::Tuple(
                items
                    .iter()
                    .map(|item| self.resolve_type_name_inner(item, visiting))
                    .collect(),
            ),
            TypeName::Option(item) => {
                TypeName::Option(Box::new(self.resolve_type_name_inner(item, visiting)))
            }
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => TypeName::FunctionSignature {
                params: params
                    .iter()
                    .map(|param| self.resolve_type_name_inner(param, visiting))
                    .collect(),
                effects: effects.clone(),
                return_type: Box::new(self.resolve_type_name_inner(return_type, visiting)),
            },
            TypeName::Applied { name, args } => TypeName::Applied {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.resolve_type_name_inner(arg, visiting))
                    .collect(),
            },
            TypeName::Named(name) => {
                if visiting.iter().any(|item| item == name) {
                    return type_name.clone();
                }
                let Some(target) = self.get_type_alias(name) else {
                    return type_name.clone();
                };
                visiting.push(name.clone());
                let resolved = self.resolve_type_name_inner(&target, visiting);
                visiting.pop();
                resolved
            }
            TypeName::Int
            | TypeName::Float
            | TypeName::Rational
            | TypeName::Number
            | TypeName::Bool
            | TypeName::String
            | TypeName::Message
            | TypeName::Char
            | TypeName::Char8
            | TypeName::Char32
            | TypeName::None
            | TypeName::Any
            | TypeName::Comparable
            | TypeName::IntRange { .. }
            | TypeName::Function => type_name.clone(),
        }
    }

    pub(super) fn assign(
        &self,
        name: &str,
        mut value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        let mut scope = self.0.borrow_mut();
        if let Some(binding) = scope.values.get_mut(name) {
            if !binding.mutable {
                return Err(VerseError::runtime_at(
                    format!("cannot assign to immutable binding `{name}`"),
                    span,
                ));
            }
            if matches!(&binding.value, Value::Option(_)) && matches!(&value, Value::Bool(false)) {
                value = Value::Option(None);
            }
            if matches!(&binding.value, Value::Array(_)) && matches!(&value, Value::Tuple(_)) {
                value = tuple_value_to_array(value);
            }
            binding.value = value_copy(&value);
            return Ok(());
        }

        let parent = scope.parent.clone();
        drop(scope);

        if let Some(parent) = parent {
            parent.assign(name, value, span)
        } else {
            Err(VerseError::runtime_at(
                format!("undefined name `{name}`"),
                span,
            ))
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<Value> {
        let scope = self.0.borrow();
        if let Some(binding) = scope.values.get(name) {
            return Some(value_copy(&binding.value));
        }
        for module in scope.module_imports.iter().rev() {
            if let Some(value) = module.get_local(name) {
                return Some(value);
            }
        }
        scope.parent.as_ref().and_then(|parent| parent.get(name))
    }

    pub(super) fn get_qualified_path(&self, name: &str) -> Option<Value> {
        let mut parts = name.split('.');
        let first = parts.next()?;
        let Some(mut value) = self.get(first) else {
            return self.get(name);
        };
        let mut qualified = first.to_string();
        let mut has_qualifier = false;
        for part in parts {
            let Value::Module { env, .. } = value else {
                return None;
            };
            value = env.get_local(part)?;
            qualified.push('.');
            qualified.push_str(part);
            has_qualifier = true;
        }
        Some(if has_qualifier {
            qualify_runtime_named_value(value, &qualified)
        } else {
            value
        })
    }

    pub(super) fn get_local(&self, name: &str) -> Option<Value> {
        self.0
            .borrow()
            .values
            .get(name)
            .map(|binding| value_copy(&binding.value))
    }

    pub(super) fn get_extension_methods(&self, name: &str) -> Vec<RuntimeExtensionMethod> {
        let scope = self.0.borrow();
        let mut methods = scope
            .extension_methods
            .get(name)
            .cloned()
            .unwrap_or_default();
        for module in scope.module_imports.iter().rev() {
            methods.extend(module.get_local_extension_methods(name));
        }
        let parent = scope.parent.clone();
        drop(scope);

        if let Some(parent) = parent {
            methods.extend(parent.get_extension_methods(name));
        }

        methods
    }

    fn get_local_extension_methods(&self, name: &str) -> Vec<RuntimeExtensionMethod> {
        self.0
            .borrow()
            .extension_methods
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
}
