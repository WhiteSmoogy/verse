use super::scalar_ops::expect_index_integer;
use super::validation::char_value_to_byte;
use super::{Value, array_value};
use crate::error::VerseError;
use crate::token::Span;

pub(super) fn string_char_values(value: &str) -> Vec<Value> {
    value
        .as_bytes()
        .iter()
        .map(|byte| Value::Char(char::from(*byte)))
        .collect()
}

pub(super) fn string_equals_char_array(text: &str, items: &[Value]) -> bool {
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

pub(crate) fn replace_string_byte_failable(
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

pub(super) fn string_value_to_char_array(value: Value) -> Value {
    match value {
        Value::String(text) => array_value(string_char_values(&text)),
        other => other,
    }
}
