use super::Value;

pub(crate) const PLAYER_MAP_RECORD_LIMIT_BYTES: usize = 256 * 1024;

const PLAYER_MAP_MAX_SIZE_DEPTH: usize = 128;

pub(crate) fn player_map_value_size(value: &Value) -> Option<usize> {
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
        | Value::NativeArrayMethod { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. } => None,
    }
}
