use crate::token::Span;

use super::Type;

#[derive(Clone)]
pub(super) struct StructInfo {
    pub(super) kind: AggregateKind,
    pub(super) base: Option<String>,
    pub(super) interfaces: Vec<String>,
    pub(super) unique: bool,
    pub(super) abstract_class: bool,
    pub(super) epic_internal_class: bool,
    pub(super) final_class: bool,
    pub(super) concrete: bool,
    pub(super) castable: bool,
    pub(super) persistable: bool,
    pub(super) computes: bool,
    pub(super) fields: Vec<StructFieldInfo>,
    pub(super) methods: Vec<ClassMethodInfo>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AggregateKind {
    Struct,
    Class,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AccessLevel {
    Public,
    Internal,
    Protected,
    Private,
}

#[derive(Clone)]
pub(super) struct StructFieldInfo {
    pub(super) name: String,
    pub(super) value_type: Type,
    pub(super) has_default: bool,
    pub(super) mutable: bool,
    pub(super) final_member: bool,
    pub(super) access: AccessLevel,
    pub(super) mutation_access: AccessLevel,
    pub(super) owner: Option<String>,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ClassMethodInfo {
    pub(super) qualifier: Option<String>,
    pub(super) name: String,
    pub(super) value_type: Type,
    pub(super) final_member: bool,
    pub(super) abstract_member: bool,
    pub(super) access: AccessLevel,
    pub(super) owner: Option<String>,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct InterfaceInfo {
    pub(super) parents: Vec<String>,
    pub(super) fields: Vec<StructFieldInfo>,
    pub(super) methods: Vec<ClassMethodInfo>,
}

#[derive(Clone)]
pub(super) struct ExtensionMethodInfo {
    pub(super) receiver_type: Type,
    pub(super) method_type: Type,
    pub(super) module_name: Option<String>,
    pub(super) access: AccessLevel,
    pub(super) span: Span,
}
