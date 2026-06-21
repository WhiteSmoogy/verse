use std::collections::HashMap;

use crate::ast::{Expr, TypeAnnotation, TypeParam};
use crate::token::Span;

use super::{AccessLevel, ParametricTypeKind, Type};

#[derive(Clone)]
pub(super) struct TypeAliasInfo {
    pub(super) target: TypeAnnotation,
    pub(super) span: Span,
    pub(super) module_path: Vec<String>,
}

#[derive(Clone)]
pub(super) struct ParametricTypeInfo {
    pub(super) params: Vec<TypeParam>,
    pub(super) expr: Expr,
    pub(super) kind: ParametricTypeKind,
    pub(super) module_path: Vec<String>,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ModuleInfo {
    pub(super) members: HashMap<String, Type>,
    pub(super) member_access: HashMap<String, AccessLevel>,
    pub(super) imports: Vec<String>,
}
