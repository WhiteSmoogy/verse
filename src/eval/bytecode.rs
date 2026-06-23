use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::TypeName;
use crate::error::VerseError;
use crate::token::Span;

use super::value_ops::value_copy;
use super::{
    CallValue, RationalValue, RuntimeClassInstanceField, RuntimeClassTypeInfo,
    RuntimeClassifiableSubsetEntry, RuntimeModifierEntry, RuntimeSubscriptionEntry, Value,
    compare_rational, event_signal_value, expect_runtime_rational, modifier_stack_position,
    register_runtime_class_type, validate_event_signal_args,
};

pub(crate) fn bytecode_struct_type_value(name: String, computes: bool) -> Value {
    Value::StructType {
        name,
        computes,
        fields: Vec::new(),
    }
}

pub(crate) fn bytecode_class_type_value(info: RuntimeClassTypeInfo) -> Value {
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

pub(crate) fn bytecode_interface_type_value(name: String) -> Value {
    Value::InterfaceType {
        name,
        parents: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
    }
}

pub(crate) fn bytecode_class_instance_value(
    class_name: String,
    unique: bool,
    fields: Vec<(String, bool, Value)>,
) -> Value {
    Value::ClassInstance {
        class_name,
        unique,
        fields: Rc::new(RefCell::new(
            fields
                .into_iter()
                .map(|(name, mutable, value)| RuntimeClassInstanceField {
                    name,
                    mutable,
                    value,
                })
                .collect(),
        )),
        methods: Rc::new(Vec::new()),
    }
}

pub(crate) fn bytecode_external_value(type_name: &TypeName) -> Value {
    match type_name {
        TypeName::Applied { name, args } if name == "event" && matches!(args.len(), 0 | 1) => {
            Value::Event {
                payload: args.first().cloned(),
                waiters: Rc::new(RefCell::new(Vec::new())),
            }
        }
        TypeName::Applied { name, args }
            if name == "subscribable_event_intrnl" && matches!(args.len(), 0 | 1) =>
        {
            Value::SubscribableEventIntrnl {
                payload: args.first().cloned(),
                waiters: Rc::new(RefCell::new(Vec::new())),
                subscribers: Rc::new(RefCell::new(Vec::new())),
                next_subscriber_id: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args } if name == "subscribable_event" && args.len() == 1 => {
            Value::SubscribableEvent {
                payload: args[0].clone(),
                waiters: Rc::new(RefCell::new(Vec::new())),
                subscribers: Rc::new(RefCell::new(Vec::new())),
                next_subscriber_id: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args }
            if name == "sticky_event" && matches!(args.len(), 0 | 1) =>
        {
            Value::StickyEvent {
                payload: args.first().cloned(),
                waiters: Rc::new(RefCell::new(Vec::new())),
                signal: Rc::new(RefCell::new(None)),
            }
        }
        TypeName::Applied { name, args } if name == "awaitable" && matches!(args.len(), 0 | 1) => {
            Value::Awaitable {
                payload: args.first().cloned(),
            }
        }
        TypeName::Applied { name, args } if name == "signalable" && args.len() == 1 => {
            Value::Signalable {
                payload: args[0].clone(),
            }
        }
        TypeName::Applied { name, args }
            if name == "subscribable" && matches!(args.len(), 0 | 1) =>
        {
            Value::Subscribable {
                payload: args.first().cloned(),
                subscribers: Rc::new(RefCell::new(Vec::new())),
                next_subscriber_id: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args } if name == "listenable" && matches!(args.len(), 0 | 1) => {
            Value::Listenable {
                payload: args.first().cloned(),
                subscribers: Rc::new(RefCell::new(Vec::new())),
                next_subscriber_id: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args } if name == "generator" && matches!(args.len(), 0 | 1) => {
            Value::Generator {
                item_type: args.first().cloned(),
                values: Rc::new(RefCell::new(Vec::new())),
            }
        }
        TypeName::Applied { name, args } if name == "modifier" && args.len() == 1 => {
            Value::Modifier {
                item_type: args[0].clone(),
            }
        }
        TypeName::Applied { name, args } if name == "modifier_stack" && args.len() == 1 => {
            Value::ModifierStack {
                item_type: args[0].clone(),
                entries: Rc::new(RefCell::new(Vec::new())),
                next_order: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args } if name == "classifiable_subset" && args.len() == 1 => {
            let _ = args;
            Value::ClassifiableSubset(Rc::new(RefCell::new(Vec::new())))
        }
        TypeName::Applied { name, args }
            if name == "classifiable_subset_key" && args.len() == 1 =>
        {
            let _ = args;
            Value::ClassifiableSubsetKey {
                entries: Rc::new(RefCell::new(Vec::new())),
                entry_id: 0,
            }
        }
        TypeName::Applied { name, args }
            if name == "classifiable_subset_var" && args.len() == 1 =>
        {
            let _ = args;
            Value::ClassifiableSubsetVar {
                entries: Rc::new(RefCell::new(Vec::<RuntimeClassifiableSubsetEntry>::new())),
                next_key: Rc::new(RefCell::new(0)),
            }
        }
        TypeName::Applied { name, args } if name == "subtype" && args.len() == 1 => {
            Value::Subtype(args[0].clone())
        }
        TypeName::Applied { name, args } if name == "castable_subtype" && args.len() == 1 => {
            Value::CastableSubtype(args[0].clone())
        }
        TypeName::Applied { name, args } if name == "concrete_subtype" && args.len() == 1 => {
            Value::ConcreteSubtype(args[0].clone())
        }
        TypeName::Applied { name, args }
            if name == "castable_concrete_subtype" && args.len() == 1 =>
        {
            Value::ConcreteSubtype(TypeName::Applied {
                name: "castable_subtype".to_string(),
                args: vec![args[0].clone()],
            })
        }
        TypeName::Applied { name, args } if name == "result" && args.len() == 2 => {
            let _ = args;
            Value::Result {
                succeeded: true,
                value: Box::new(Value::External),
            }
        }
        TypeName::Applied { name, args } if name == "success_result" && args.len() == 1 => {
            let _ = args;
            Value::Result {
                succeeded: true,
                value: Box::new(Value::External),
            }
        }
        TypeName::Applied { name, args } if name == "error_result" && args.len() == 1 => {
            let _ = args;
            Value::Result {
                succeeded: false,
                value: Box::new(Value::External),
            }
        }
        TypeName::Array(_) => Value::Array(Rc::new(RefCell::new(Vec::new()))),
        TypeName::Map(_, _) | TypeName::WeakMap(_, _) => {
            Value::Map(Rc::new(RefCell::new(Vec::new())))
        }
        TypeName::Tuple(items) => Value::Tuple(vec![Value::External; items.len()]),
        TypeName::Option(_) => Value::Option(None),
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => Value::ExternalFunction {
            params: params.clone(),
            effects: effects.clone(),
            return_type: return_type.clone(),
        },
        _ => Value::External,
    }
}

pub(crate) fn bytecode_external_return_value(type_name: &TypeName) -> Value {
    match type_name {
        TypeName::Int => Value::Int(0),
        TypeName::IntRange { min, .. } => Value::Int(*min),
        TypeName::Float => Value::Float(0.0),
        TypeName::FloatRange(range) => {
            let value = if range.contains(0.0) {
                0.0
            } else {
                range.min.get()
            };
            Value::Float(value)
        }
        TypeName::Rational => Value::Rational(RationalValue::from_int(0)),
        TypeName::Number => Value::Int(0),
        TypeName::Bool => Value::Bool(false),
        TypeName::String | TypeName::Message => Value::String(String::new()),
        TypeName::Char | TypeName::Char8 => Value::Char('\0'),
        TypeName::Char32 => Value::Char32('\0'),
        TypeName::None => Value::None,
        TypeName::Array(_) => Value::Array(Rc::new(RefCell::new(Vec::new()))),
        TypeName::Map(_, _) | TypeName::WeakMap(_, _) => {
            Value::Map(Rc::new(RefCell::new(Vec::new())))
        }
        TypeName::Tuple(items) => Value::Tuple(
            items
                .iter()
                .map(bytecode_external_return_value)
                .collect::<Vec<_>>(),
        ),
        TypeName::Option(_) => Value::Option(None),
        TypeName::Applied { name, args } if name == "result" && args.len() == 2 => Value::Result {
            succeeded: true,
            value: Box::new(bytecode_external_return_value(&args[0])),
        },
        TypeName::Applied { name, args } if name == "success_result" && args.len() == 1 => {
            Value::Result {
                succeeded: true,
                value: Box::new(bytecode_external_return_value(&args[0])),
            }
        }
        TypeName::Applied { name, args } if name == "error_result" && args.len() == 1 => {
            Value::Result {
                succeeded: false,
                value: Box::new(bytecode_external_return_value(&args[0])),
            }
        }
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => Value::ExternalFunction {
            params: params.clone(),
            effects: effects.clone(),
            return_type: return_type.clone(),
        },
        _ => bytecode_external_value(type_name),
    }
}

pub(crate) fn bytecode_load_field_value(object: &Value, name: &str) -> Option<Value> {
    match object {
        Value::StructInstance { fields, .. } => fields
            .iter()
            .find_map(|(field_name, value)| (field_name == name).then(|| value_copy(value))),
        Value::ClassInstance { fields, .. } => fields
            .borrow()
            .iter()
            .find_map(|field| (field.name == name).then(|| value_copy(&field.value))),
        _ => None,
    }
}

pub(crate) fn bytecode_native_member_value(object: &Value, name: &str) -> Option<Value> {
    match object {
        Value::Session => match name {
            "Environment" => Some(Value::NativeFunction {
                name: "session.Environment",
                arity: Some(0),
                decides: false,
                function: super::native_session_environment,
            }),
            _ => None,
        },
        Value::Event { payload, waiters } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: None,
            }),
            "Signal" => Some(Value::NativeEventMethod {
                name: "Signal",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: None,
            }),
            _ => None,
        },
        Value::SubscribableEventIntrnl {
            payload,
            waiters,
            subscribers,
            next_subscriber_id,
        } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: None,
            }),
            "Signal" => Some(Value::NativeEventMethod {
                name: "Signal",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: Some(subscribers.clone()),
                sticky_signal: None,
            }),
            "Subscribe" => Some(Value::NativeSubscribableMethod {
                name: "Subscribe",
                payload: payload.clone(),
                subscribers: subscribers.clone(),
                next_subscriber_id: next_subscriber_id.clone(),
            }),
            _ => None,
        },
        Value::SubscribableEvent {
            payload,
            waiters,
            subscribers,
            next_subscriber_id,
        } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: Some(payload.clone()),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: None,
            }),
            "Signal" => Some(Value::NativeEventMethod {
                name: "Signal",
                payload: Some(payload.clone()),
                waiters: Some(waiters.clone()),
                subscribers: Some(subscribers.clone()),
                sticky_signal: None,
            }),
            "Broadcast" => Some(Value::NativeEventMethod {
                name: "Broadcast",
                payload: Some(payload.clone()),
                waiters: Some(waiters.clone()),
                subscribers: Some(subscribers.clone()),
                sticky_signal: None,
            }),
            "Subscribe" => Some(Value::NativeSubscribableMethod {
                name: "Subscribe",
                payload: Some(payload.clone()),
                subscribers: subscribers.clone(),
                next_subscriber_id: next_subscriber_id.clone(),
            }),
            _ => None,
        },
        Value::StickyEvent {
            payload,
            waiters,
            signal,
        } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: Some(signal.clone()),
            }),
            "Signal" => Some(Value::NativeEventMethod {
                name: "Signal",
                payload: payload.clone(),
                waiters: Some(waiters.clone()),
                subscribers: None,
                sticky_signal: Some(signal.clone()),
            }),
            "IsSignaled" => Some(Value::NativeEventMethod {
                name: "IsSignaled",
                payload: None,
                waiters: None,
                subscribers: None,
                sticky_signal: Some(signal.clone()),
            }),
            "ClearSignal" => Some(Value::NativeEventMethod {
                name: "ClearSignal",
                payload: None,
                waiters: None,
                subscribers: None,
                sticky_signal: Some(signal.clone()),
            }),
            _ => None,
        },
        Value::Awaitable { payload } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: payload.clone(),
                waiters: None,
                subscribers: None,
                sticky_signal: None,
            }),
            _ => None,
        },
        Value::Signalable { payload } => match name {
            "Signal" => Some(Value::NativeEventMethod {
                name: "Signal",
                payload: Some(payload.clone()),
                waiters: None,
                subscribers: None,
                sticky_signal: None,
            }),
            _ => None,
        },
        Value::Subscribable {
            payload,
            subscribers,
            next_subscriber_id,
        } => match name {
            "Subscribe" => Some(Value::NativeSubscribableMethod {
                name: "Subscribe",
                payload: payload.clone(),
                subscribers: subscribers.clone(),
                next_subscriber_id: next_subscriber_id.clone(),
            }),
            _ => None,
        },
        Value::Listenable {
            payload,
            subscribers,
            next_subscriber_id,
        } => match name {
            "Await" => Some(Value::NativeEventMethod {
                name: "Await",
                payload: payload.clone(),
                waiters: None,
                subscribers: None,
                sticky_signal: None,
            }),
            "Subscribe" => Some(Value::NativeSubscribableMethod {
                name: "Subscribe",
                payload: payload.clone(),
                subscribers: subscribers.clone(),
                next_subscriber_id: next_subscriber_id.clone(),
            }),
            _ => None,
        },
        Value::SubscriptionCancelHandle {
            subscribers,
            subscriber_id,
        } => match name {
            "Cancel" => Some(Value::NativeSubscriptionCancelMethod {
                name: "Cancel",
                subscribers: subscribers.clone(),
                subscriber_id: *subscriber_id,
            }),
            _ => None,
        },
        Value::Task(task) => match name {
            "Await" => Some(Value::NativeTaskMethod {
                name: "Await",
                task: task.clone(),
            }),
            "Cancel" => Some(Value::NativeTaskMethod {
                name: "Cancel",
                task: task.clone(),
            }),
            _ => None,
        },
        Value::Result { .. } => match name {
            "GetSuccess" | "GetError" => Some(Value::NativeResultMethod {
                name: match name {
                    "GetSuccess" => "GetSuccess",
                    "GetError" => "GetError",
                    _ => unreachable!("matched result method names above"),
                },
                result: Box::new(value_copy(object)),
            }),
            "Success" => match object {
                Value::Result {
                    succeeded: true,
                    value,
                } => Some(value_copy(value)),
                _ => None,
            },
            "Error" => match object {
                Value::Result {
                    succeeded: false,
                    value,
                } => Some(value_copy(value)),
                _ => None,
            },
            _ => None,
        },
        Value::Modifier { .. } | Value::ModifierStack { .. } => match name {
            "Evaluate" => Some(Value::NativeModifierMethod {
                name: "Evaluate",
                receiver: Box::new(value_copy(object)),
            }),
            "AddModifier" if matches!(object, Value::ModifierStack { .. }) => {
                Some(Value::NativeModifierMethod {
                    name: "AddModifier",
                    receiver: Box::new(value_copy(object)),
                })
            }
            "FirstPosition" if matches!(object, Value::ModifierStack { .. }) => {
                Some(modifier_stack_position(object, true))
            }
            "LastPosition" if matches!(object, Value::ModifierStack { .. }) => {
                Some(modifier_stack_position(object, false))
            }
            _ => None,
        },
        Value::ModifierCancelHandle { entries, entry_id } => match name {
            "Cancel" => Some(Value::NativeCancelMethod {
                name: "Cancel",
                entries: entries.clone(),
                entry_id: *entry_id,
            }),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn bytecode_call_native_event_method(
    name: &'static str,
    payload: Option<TypeName>,
    _waiters: Option<Rc<RefCell<Vec<Rc<super::RuntimeTask>>>>>,
    sticky_signal: Option<Rc<RefCell<Option<Value>>>>,
    args: Vec<(Value, Span)>,
    span: Span,
) -> Result<Value, VerseError> {
    let args = args
        .into_iter()
        .map(|(value, span)| CallValue {
            name: None,
            optional: false,
            value,
            span,
        })
        .collect::<Vec<_>>();
    match name {
        "Signal" | "Broadcast" => {
            validate_event_signal_args(payload.as_ref(), &args, span)?;
            if let Some(sticky_signal) = sticky_signal {
                let mut signal = sticky_signal.borrow_mut();
                if signal.is_some() {
                    return Err(VerseError::runtime_at(
                        "`Signal` called on an already signaled sticky_event",
                        span,
                    ));
                }
                *signal = Some(event_signal_value(payload.as_ref(), &args));
            }
            Ok(Value::None)
        }
        "Await" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`Await` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            if let Some(sticky_signal) = sticky_signal
                && let Some(value) = sticky_signal.borrow().as_ref()
            {
                return Ok(value_copy(value));
            }
            Ok(Value::Pending)
        }
        "IsSignaled" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`IsSignaled` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            if sticky_signal
                .as_ref()
                .is_some_and(|signal| signal.borrow().is_some())
            {
                Ok(Value::None)
            } else {
                Ok(Value::Option(None))
            }
        }
        "ClearSignal" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`ClearSignal` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            if let Some(sticky_signal) = sticky_signal {
                *sticky_signal.borrow_mut() = None;
            }
            Ok(Value::None)
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown event method `{name}`"),
            span,
        )),
    }
}

pub(crate) fn bytecode_new_running_task() -> Rc<super::RuntimeTask> {
    super::RuntimeTask::new_running()
}

pub(crate) fn bytecode_event_signal_payload(
    payload: Option<&TypeName>,
    args: Vec<(Value, Span)>,
) -> Value {
    let args = args
        .into_iter()
        .map(|(value, span)| CallValue {
            name: None,
            optional: false,
            value,
            span,
        })
        .collect::<Vec<_>>();
    event_signal_value(payload, &args)
}

pub(crate) fn bytecode_call_native_subscribable_method(
    name: &'static str,
    payload: Option<TypeName>,
    subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    next_subscriber_id: Rc<RefCell<u64>>,
    callback_accepts_arity: bool,
    callback: Option<Value>,
    arg_count: usize,
    span: Span,
) -> Result<Value, VerseError> {
    if name != "Subscribe" {
        return Err(VerseError::runtime_at(
            format!("unknown subscribable method `{name}`"),
            span,
        ));
    }
    if arg_count != 1 {
        return Err(VerseError::runtime_at(
            format!("`Subscribe` expected 1 argument, got {arg_count}"),
            span,
        ));
    }
    let expected_arity = usize::from(payload.is_some());
    if !callback_accepts_arity {
        return Err(VerseError::runtime_at(
            format!("`Subscribe` Callback expected function/{expected_arity} -> void"),
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
        callback: callback.unwrap_or(Value::External),
    });
    Ok(Value::SubscriptionCancelHandle {
        subscribers,
        subscriber_id: id,
    })
}

pub(crate) fn bytecode_call_native_subscription_cancel_method(
    name: &'static str,
    subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    subscriber_id: u64,
    arg_count: usize,
    span: Span,
) -> Result<Value, VerseError> {
    if name != "Cancel" {
        return Err(VerseError::runtime_at(
            format!("unknown cancel method `{name}`"),
            span,
        ));
    }
    if arg_count != 0 {
        return Err(VerseError::runtime_at(
            format!("`Cancel` expected 0 arguments, got {arg_count}"),
            span,
        ));
    }
    subscribers
        .borrow_mut()
        .retain(|entry| entry.id != subscriber_id);
    Ok(Value::None)
}

pub(crate) fn bytecode_call_native_cancel_method(
    name: &'static str,
    entries: Rc<RefCell<Vec<RuntimeModifierEntry>>>,
    entry_id: u64,
    arg_count: usize,
    span: Span,
) -> Result<Value, VerseError> {
    if name != "Cancel" {
        return Err(VerseError::runtime_at(
            format!("unknown cancel method `{name}`"),
            span,
        ));
    }
    if arg_count != 0 {
        return Err(VerseError::runtime_at(
            format!("`Cancel` expected 0 arguments, got {arg_count}"),
            span,
        ));
    }
    entries.borrow_mut().retain(|entry| entry.id != entry_id);
    Ok(Value::None)
}

pub(crate) fn bytecode_modifier_stack_add(
    stack: Value,
    args: Vec<(Value, Span)>,
    span: Span,
) -> Result<Value, VerseError> {
    let Value::ModifierStack {
        item_type: _,
        entries,
        next_order,
    } = stack
    else {
        return Err(VerseError::runtime_at(
            "AddModifier expected modifier_stack receiver",
            span,
        ));
    };
    let [(modifier, modifier_span), (position, position_span)]: [(Value, Span); 2] =
        args.try_into().map_err(|args: Vec<(Value, Span)>| {
            VerseError::runtime_at(
                format!("`AddModifier` expected 2 arguments, got {}", args.len()),
                span,
            )
        })?;
    let type_matches = matches!(
        modifier,
        Value::External
            | Value::Modifier { .. }
            | Value::ModifierStack { .. }
            | Value::ClassInstance { .. }
    );
    if !type_matches {
        return Err(VerseError::runtime_at(
            format!("`AddModifier` expected modifier, got {modifier}"),
            modifier_span,
        ));
    }
    let position = expect_runtime_rational(&position, "`AddModifier` Position", position_span)?;
    let id = {
        let mut next = next_order.borrow_mut();
        let id = *next;
        *next = next.saturating_add(1);
        id
    };
    entries.borrow_mut().push(RuntimeModifierEntry {
        id,
        position,
        order: id,
        modifier,
    });
    Ok(Value::ModifierCancelHandle {
        entries,
        entry_id: id,
    })
}

pub(crate) fn bytecode_modifier_stack_ordered_modifiers(
    stack: &Value,
) -> Option<(TypeName, Vec<Value>)> {
    let Value::ModifierStack {
        item_type, entries, ..
    } = stack
    else {
        return None;
    };
    let mut ordered = entries.borrow().clone();
    ordered.sort_by(|left, right| {
        compare_rational(left.position, right.position).then(left.order.cmp(&right.order))
    });
    Some((
        item_type.clone(),
        ordered
            .into_iter()
            .map(|entry| value_copy(&entry.modifier))
            .collect(),
    ))
}
