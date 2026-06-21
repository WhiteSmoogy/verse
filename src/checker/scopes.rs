use super::Type;

#[derive(Clone)]
pub(super) struct Symbol {
    pub(super) value_type: Type,
    pub(super) mutable: bool,
}

impl Symbol {
    pub(super) fn immutable(value_type: Type) -> Self {
        Self {
            value_type,
            mutable: false,
        }
    }
}
