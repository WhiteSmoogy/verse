use std::cell::RefCell;
use std::rc::Rc;

use super::{RuntimeClassField, RuntimeStructField, Value};

pub(super) fn value_copy(value: &Value) -> Value {
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
        Value::Array(items) => Value::Array(Rc::new(RefCell::new(
            items.borrow().iter().map(value_copy).collect(),
        ))),
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
        | Value::NativeArrayMethod { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. } => value.clone(),
    }
}
