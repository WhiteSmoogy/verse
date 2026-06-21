use crate::error::VerseError;
use crate::token::Span;

use super::Value;

pub(crate) fn char_value_to_byte(value: &Value) -> Option<u8> {
    match value {
        Value::Char(value) => u8::try_from(*value as u32).ok(),
        _ => None,
    }
}

pub(crate) fn char_array_to_string(items: &[Value]) -> Option<String> {
    let bytes = items
        .iter()
        .map(char_value_to_byte)
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

pub(crate) fn expect_color_value(value: &Value, span: Span) -> Result<(), VerseError> {
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
