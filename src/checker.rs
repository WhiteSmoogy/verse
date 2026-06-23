use std::collections::{HashMap, HashSet};

use crate::ast::{
    ArchetypeConstructorCall, ArchetypeEntry, ArchetypeLet, AssignOp, BinaryOp, CallArg, CaseArm,
    CasePattern, ClassBlock, ClassMethod, ConcurrentOp, Expr, ExprKind, ExtensionMethod,
    FieldAttribute, ForBinding, ForClause, InterpolatedStringPart, Param, ParamPattern, Program,
    Stmt, StmtKind, StructField, TypeAnnotation, TypeName, TypeParam, TypeParamConstraint, UnaryOp,
};
use crate::desugar::Desugarer;
use crate::error::{Diagnostic, DiagnosticCode, VerseError};
use crate::parser::parse_source;
use crate::semantics::{SemanticFacts, SemanticProgram};
use crate::token::{CharacterKind, NumberKind, NumberLiteral, Span};

mod semantic_class;
use semantic_class::{
    AccessLevel, AggregateKind, ClassMethodInfo, ExtensionMethodInfo, FieldOwnerKind,
    InterfaceInfo, StructFieldInfo, StructInfo, class_constructor_effects,
};

mod definition;
use definition::{ModuleInfo, ParametricTypeInfo, TypeAliasInfo, type_to_constraint_type_name};

mod effects;
pub use effects::{Effect, EffectSet};
use effects::{
    ensure_callable_in_failure_context, function_effects_are_assignable, has_effect,
    validate_function_effect_combination,
};

mod semantic_expression;

mod semantic_function;
use semantic_function::{
    class_is_subtype_of, collect_function_type_params, extension_method_has_qualifier,
    function_signatures_conflict, function_signatures_match_exactly,
    inherited_method_duplicate_index, inherited_method_override_index, method_binding_types,
    method_group_type, method_has_qualifier, method_qualifiers_conflict,
    method_signatures_conflict, positional_call_args, push_distinct_local_method_info,
    substitute_type_params, type_contains_type_param, unresolved_type_function_inferred_param,
};

mod semantic_scope;
use semantic_scope::Symbol;

mod semantic_statement;

mod semantic_types;
pub use crate::ast::FloatRange;
use semantic_types::*;
pub use semantic_types::{IntRange, ParamSpec, Type};

mod type_variables;
pub use type_variables::{TypeVariable, TypeVariableBounds};

pub fn check_source(source: &str) -> Result<Type, VerseError> {
    Ok(check_source_with_diagnostics(source)?.value_type)
}

pub fn check_source_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<Type, VerseError> {
    Ok(check_source_with_diagnostics_in_package(source, package_name)?.value_type)
}

pub fn check_source_with_diagnostics(source: &str) -> Result<CheckResult, VerseError> {
    let typed_program = check_source_to_typed_program(source)?;
    Ok(CheckResult {
        value_type: typed_program.value_type,
        warnings: typed_program.warnings,
    })
}

pub fn check_source_with_diagnostics_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<CheckResult, VerseError> {
    let typed_program = check_source_to_typed_program_in_package(source, package_name)?;
    Ok(CheckResult {
        value_type: typed_program.value_type,
        warnings: typed_program.warnings,
    })
}

pub fn check_source_to_typed_program(source: &str) -> Result<SemanticProgram, VerseError> {
    check_source_to_typed_program_in_package(source, None)
}

pub fn check_source_to_typed_program_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<SemanticProgram, VerseError> {
    let program = parse_source(source)?;
    Checker::new()
        .with_package(package_name.map(str::to_string))
        .check_program_to_semantic_program(&program)
}

pub fn check_source_with_recovery(source: &str) -> Result<RecoveredCheckResult, VerseError> {
    let program = parse_source(source)?;
    Ok(Checker::new().check_program_with_recovery(&program))
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub value_type: Type,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct RecoveredCheckResult {
    pub value_type: Type,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Clone)]
struct EnumInfo {
    variants: Vec<String>,
    open: bool,
    persistable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParametricTypeKind {
    Struct,
    Class,
    Interface,
    Alias,
}

#[derive(Clone)]
struct PlayerWeakMapInfo {
    value_type: Type,
}

#[derive(Clone)]
struct TypeFunctionInfo {
    params: Vec<TypeParam>,
    inferred_params: Vec<TypeParam>,
    target: TypeName,
    module_path: Vec<String>,
    signature: Type,
}

#[derive(Clone)]
struct DataMemberDefaultContext {
    aggregate_name: String,
    field_name: String,
}

type ClassMemberInfosResult = (
    Vec<StructFieldInfo>,
    Vec<ClassMethodInfo>,
    bool,
    bool,
    Option<String>,
    Vec<String>,
);

struct ClassDefinitionParts<'a> {
    definition_access: AccessLevel,
    specifiers: &'a [String],
    base: Option<&'a TypeAnnotation>,
    interfaces: &'a [TypeAnnotation],
    fields: &'a [StructField],
    methods: &'a [ClassMethod],
    extension_methods: &'a [ExtensionMethod],
    blocks: &'a [ClassBlock],
}

#[derive(Clone)]
struct AsyncExprMarker {
    function_depth: usize,
    seen: bool,
}

#[derive(Clone)]
pub struct Checker {
    scopes: Vec<HashMap<String, Symbol>>,
    scope_imports: Vec<Vec<String>>,
    enum_types: HashMap<String, EnumInfo>,
    struct_types: HashMap<String, StructInfo>,
    interface_types: HashMap<String, InterfaceInfo>,
    module_types: HashMap<String, ModuleInfo>,
    extension_methods: HashMap<String, Vec<ExtensionMethodInfo>>,
    parametric_types: HashMap<String, ParametricTypeInfo>,
    parametric_type_instances: HashMap<String, Vec<Type>>,
    predeclared_aggregate_values: HashSet<String>,
    type_alias_defs: HashMap<String, TypeAliasInfo>,
    type_aliases: HashMap<String, Type>,
    type_functions: HashMap<String, Vec<TypeFunctionInfo>>,
    type_param_scopes: Vec<HashMap<String, Type>>,
    player_weak_maps: Vec<PlayerWeakMapInfo>,
    module_path: Vec<String>,
    module_scope_depths: Vec<usize>,
    break_depth: usize,
    function_returns: Vec<Type>,
    function_effects: Vec<Vec<String>>,
    failure_context_depth: usize,
    range_context_depth: usize,
    defer_depth: usize,
    data_member_default_depth: usize,
    data_member_default_stack: Vec<DataMemberDefaultContext>,
    async_expr_markers: Vec<AsyncExprMarker>,
    suppressed_async_expr_markers: usize,
    class_context: Vec<String>,
    class_member_shadow_names: Vec<HashSet<String>>,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    semantic_facts: SemanticFacts,
    package_name: Option<String>,
    recovering: bool,
}

impl Checker {
    pub fn check_program(self, program: &Program) -> Result<Type, VerseError> {
        Ok(self.check_program_with_diagnostics(program)?.value_type)
    }

    pub fn check_program_with_diagnostics(
        self,
        program: &Program,
    ) -> Result<CheckResult, VerseError> {
        let typed_program = self.check_program_to_typed_program(program)?;
        Ok(CheckResult {
            value_type: typed_program.value_type,
            warnings: typed_program.warnings,
        })
    }

    pub fn check_program_to_typed_program(
        mut self,
        program: &Program,
    ) -> Result<SemanticProgram, VerseError> {
        self.recovering = false;
        let program = Desugarer::new().desugar_program(program);
        self.check_desugared_program_to_semantic_program(program)
    }

    pub fn check_program_to_semantic_program(
        self,
        program: &Program,
    ) -> Result<SemanticProgram, VerseError> {
        self.check_program_to_typed_program(program)
    }

    pub fn check_desugared_program_to_semantic_program(
        mut self,
        program: Program,
    ) -> Result<SemanticProgram, VerseError> {
        self.predeclare_top_level_modules(&program);
        self.predeclare_top_level_module_member_access(&program)?;
        self.predeclare_top_level_enums(&program);
        self.predeclare_top_level_aggregate_names(&program)?;
        self.predeclare_top_level_aggregate_values(&program)?;
        self.predeclare_top_level_parametric_types(&program)?;
        self.predeclare_using_imports_recursive(&program.statements)?;
        self.predeclare_top_level_type_aliases(&program)?;
        self.predeclare_top_level_type_functions(&program)?;
        self.resolve_predeclared_type_aliases()?;
        self.predeclare_extension_methods_in_current_scope(&program.statements)?;
        self.predeclare_top_level_functions(&program)?;
        self.validate_public_module_surface_access(&program.statements)?;
        self.define_top_level_interface_members(&program)?;
        self.define_top_level_aggregate_members(&program)?;
        self.validate_function_overloads_in_current_scope()?;
        let value_type = self.check_statements(&program.statements)?;
        Ok(SemanticProgram {
            program,
            value_type,
            warnings: self.warnings,
            facts: self.semantic_facts,
        })
    }

    pub fn check_program_with_recovery(mut self, program: &Program) -> RecoveredCheckResult {
        self.recovering = true;
        let program = Desugarer::new().desugar_program(program);
        self.predeclare_top_level_modules(&program);
        self.run_recovering_pass(|checker| {
            checker.predeclare_top_level_module_member_access(&program)
        });
        self.predeclare_top_level_enums(&program);
        self.run_recovering_pass(|checker| checker.predeclare_top_level_aggregate_names(&program));
        self.run_recovering_pass(|checker| checker.predeclare_top_level_aggregate_values(&program));
        self.run_recovering_pass(|checker| checker.predeclare_top_level_parametric_types(&program));
        self.run_recovering_pass(|checker| {
            checker.predeclare_using_imports_recursive(&program.statements)
        });
        self.run_recovering_pass(|checker| checker.predeclare_top_level_type_aliases(&program));
        self.run_recovering_pass(|checker| checker.predeclare_top_level_type_functions(&program));
        self.run_recovering_pass(|checker| checker.resolve_predeclared_type_aliases());
        self.run_recovering_pass(|checker| {
            checker.predeclare_extension_methods_in_current_scope(&program.statements)
        });
        self.run_recovering_pass(|checker| checker.predeclare_top_level_functions(&program));
        self.run_recovering_pass(|checker| {
            checker.validate_public_module_surface_access(&program.statements)
        });
        self.run_recovering_pass(|checker| checker.define_top_level_interface_members(&program));
        self.run_recovering_pass(|checker| checker.define_top_level_aggregate_members(&program));
        self.run_recovering_pass(|checker| checker.validate_function_overloads_in_current_scope());
        let value_type =
            self.run_recovering_type_pass(|checker| checker.check_statements(&program.statements));
        RecoveredCheckResult {
            value_type,
            errors: self.errors,
            warnings: self.warnings,
        }
    }

    fn run_recovering_pass(&mut self, pass: impl FnOnce(&mut Self) -> Result<(), VerseError>) {
        let snapshot = self.clone();
        if let Err(error) = pass(self) {
            *self = snapshot;
            self.record_error(error);
        }
    }

    fn run_recovering_type_pass(
        &mut self,
        pass: impl FnOnce(&mut Self) -> Result<Type, VerseError>,
    ) -> Type {
        let snapshot = self.clone();
        match pass(self) {
            Ok(value_type) => value_type,
            Err(error) => {
                *self = snapshot;
                self.record_error(error);
                Type::Unknown
            }
        }
    }

    fn record_error(&mut self, error: VerseError) {
        self.errors.push(error.diagnostic().clone());
    }

    fn warn_at(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.warnings
            .push(Diagnostic::warning(code, message, Some(span)));
    }

    fn warn_unreachable(&mut self, message: impl Into<String>, span: Span) {
        self.warn_at(DiagnosticCode::UnreachableCode, message, span);
    }

    fn warn_empty_block(&mut self, span: Span) {
        self.warn_at(DiagnosticCode::EmptyBlock, "empty block", span);
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

fn call_arg_expr(arg: &CallArg) -> &Expr {
    match arg {
        CallArg::Positional(expr) => expr,
        CallArg::Named { expr, .. } => expr,
    }
}

fn type_returns_never(value_type: &Type) -> bool {
    match value_type {
        Type::Function { return_type, .. } => return_type.as_ref() == &Type::Never,
        Type::Overload(overloads) => {
            !overloads.is_empty() && overloads.iter().all(type_returns_never)
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminationKind {
    Return,
    Break,
    Never,
}

impl TerminationKind {
    fn merge(self, other: Self) -> Self {
        if self == other { self } else { Self::Never }
    }

    fn unreachable_message(self) -> &'static str {
        match self {
            Self::Return => "unreachable code after `return`",
            Self::Break => "unreachable code after `break`",
            Self::Never => "unreachable code after never-returning expression",
        }
    }
}

fn spawn_body_expr(body: &Expr) -> Result<&Expr, VerseError> {
    let ExprKind::Block(statements) = &body.kind else {
        return Err(VerseError::check_at(
            "`spawn` expects a braced expression body",
            body.span,
        ));
    };
    let [statement] = statements.as_slice() else {
        return Err(VerseError::check_at(
            "`spawn` body must contain exactly one expression",
            body.span,
        ));
    };
    let StmtKind::Expr(expr) = &statement.kind else {
        return Err(VerseError::check_at(
            "`spawn` body must contain exactly one expression",
            statement.span,
        ));
    };
    Ok(expr)
}

fn concurrent_body_statements(body: &Expr) -> Result<&[Stmt], VerseError> {
    let ExprKind::ColonBlock(statements) = &body.kind else {
        return Err(VerseError::check_at(
            "concurrency expression expects an indented block body",
            body.span,
        ));
    };
    Ok(statements)
}

fn concurrent_op_name(op: ConcurrentOp) -> &'static str {
    match op {
        ConcurrentOp::Sync => "sync",
        ConcurrentOp::Race => "race",
        ConcurrentOp::Rush => "rush",
        ConcurrentOp::Branch => "branch",
    }
}

fn official_event_archetype_name_and_args(callee: &Expr) -> Option<(&str, &[CallArg])> {
    let ExprKind::Call { callee, args } = &callee.kind else {
        return None;
    };
    match &callee.kind {
        ExprKind::Ident(name) if matches!(name.as_str(), "event" | "sticky_event") => {
            Some((name.as_str(), args.as_slice()))
        }
        _ => None,
    }
}

fn official_event_archetype_args(callee: &Expr) -> Option<&[CallArg]> {
    official_event_archetype_name_and_args(callee).map(|(_, args)| args)
}

fn is_official_event_archetype_callee(callee: &Expr) -> bool {
    official_event_archetype_args(callee).is_some()
}

fn expr_to_type_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Member { object, name } => {
            let mut path = expr_to_type_path(object)?;
            path.push('.');
            path.push_str(name);
            Some(path)
        }
        ExprKind::QualifiedName { qualifier, name } => Some(format!("{qualifier}.{name}")),
        _ => None,
    }
}

fn class_definition_diagnostic_span(
    base: Option<&TypeAnnotation>,
    fields: &[StructField],
    methods: &[ClassMethod],
    extension_methods: &[ExtensionMethod],
    blocks: &[ClassBlock],
) -> Span {
    base.map(|base| base.span)
        .or_else(|| fields.first().map(|field| field.span))
        .or_else(|| methods.first().map(|method| method.span))
        .or_else(|| extension_methods.first().map(|method| method.span))
        .or_else(|| blocks.first().map(|block| block.span))
        .unwrap_or_else(|| Span::new(0, 0, 1, 1))
}

fn enum_case_variant<'a>(expr: &'a Expr, enum_name: &str) -> Option<&'a str> {
    let ExprKind::Member { object, name } = &expr.kind else {
        return None;
    };
    let ExprKind::Ident(object_name) = &object.kind else {
        return None;
    };
    (object_name == enum_name).then_some(name.as_str())
}

#[derive(Clone, PartialEq, Eq)]
enum CaseConstant {
    Int(i128),
    Bool(bool),
    String(String),
    Char(char),
}

fn scalar_case_constant(expr: &Expr, subject_type: &Type) -> Option<CaseConstant> {
    match subject_type {
        Type::Int => int_case_constant(expr).map(CaseConstant::Int),
        Type::Bool => match &expr.kind {
            ExprKind::Bool(value) => Some(CaseConstant::Bool(*value)),
            _ => None,
        },
        Type::String => match &expr.kind {
            ExprKind::String(value) => Some(CaseConstant::String(value.clone())),
            _ => None,
        },
        Type::Char | Type::Char8 | Type::Char32 => match &expr.kind {
            ExprKind::Char { value, .. } => Some(CaseConstant::Char(*value)),
            _ => None,
        },
        _ => None,
    }
}

fn int_case_constant(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Number {
            value: NumberLiteral::Int(value),
            kind: NumberKind::Int,
        } => Some(*value),
        ExprKind::Unary {
            op: UnaryOp::Positive,
            expr,
        } => int_case_constant(expr),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => int_case_constant(expr).map(|value| -value),
        _ => None,
    }
}

fn scalar_case_is_exhaustive(subject_type: &Type, covered: &[CaseConstant]) -> bool {
    matches!(subject_type, Type::Bool)
        && covered.contains(&CaseConstant::Bool(true))
        && covered.contains(&CaseConstant::Bool(false))
}

fn case_arms_have_wildcard(arms: &[CaseArm]) -> bool {
    arms.iter()
        .any(|arm| matches!(arm.pattern, CasePattern::Wildcard { .. }))
}

fn is_failable_condition_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } | ExprKind::Index { .. } => true,
        ExprKind::Unary {
            op: UnaryOp::Not, ..
        } => true,
        ExprKind::Unary { expr, .. } => is_failable_condition_expr(expr),
        ExprKind::Binary { left, op, right } => {
            is_failure_binary_op(*op)
                || is_failable_condition_expr(left)
                || is_failable_condition_expr(right)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            failure_condition_has_failable_expr(condition)
                || is_failable_condition_expr(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(is_failable_condition_expr)
        }
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            failure_statements_have_failable_expr(statements)
        }
        ExprKind::Profile { description, body } => {
            is_failable_condition_expr(description) || is_failable_condition_expr(body)
        }
        ExprKind::Spawn { .. } => false,
        ExprKind::Concurrent { .. } => false,
        ExprKind::Case { subject, arms } => {
            is_failable_condition_expr(subject)
                || !case_arms_have_wildcard(arms)
                || arms.iter().any(|arm| {
                    (match &arm.pattern {
                        CasePattern::Wildcard { .. } => false,
                        CasePattern::Expr(pattern) => is_failable_condition_expr(pattern),
                    }) || is_failable_condition_expr(&arm.expr)
                })
        }
        ExprKind::For { clauses, body } => {
            clauses.iter().any(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => failure_condition_has_failable_expr(iterable),
            }) || is_failable_condition_expr(body)
        }
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            is_failable_condition_expr(object)
        }
        ExprKind::Call { callee, args } => {
            is_failable_condition_expr(callee)
                || args.iter().any(|arg| match arg {
                    CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                        is_failable_condition_expr(expr)
                    }
                })
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            items.iter().any(is_failable_condition_expr)
        }
        ExprKind::Map(entries) => entries.iter().any(|(key, value)| {
            is_failable_condition_expr(key) || is_failable_condition_expr(value)
        }),
        ExprKind::Var { expr, .. } => is_failable_condition_expr(expr),
        ExprKind::Set { target, expr, .. } => {
            assignment_target_has_failable_expr(target) || is_failable_condition_expr(expr)
        }
        ExprKind::TypeLiteral { expr } => is_failable_condition_expr(expr),
        ExprKind::Archetype {
            callee, entries, ..
        } => {
            is_failable_condition_expr(callee)
                || entries.iter().any(|entry| match entry {
                    ArchetypeEntry::Field(field) => is_failable_condition_expr(&field.expr),
                    ArchetypeEntry::Let(binding) => is_failable_condition_expr(&binding.expr),
                    ArchetypeEntry::Block(block) => is_failable_condition_expr(block),
                    ArchetypeEntry::ConstructorCall(call) => call
                        .args
                        .iter()
                        .any(|arg| is_failable_condition_expr(call_arg_expr(arg))),
                })
        }
        ExprKind::Option(Some(value)) => is_failable_condition_expr(value),
        ExprKind::InterpolatedString(parts) => parts.iter().any(|part| match part {
            InterpolatedStringPart::Text(_) => false,
            InterpolatedStringPart::Expr(expr) => is_failable_condition_expr(expr),
        }),
        ExprKind::StructDefinition { fields, .. } | ExprKind::ClassDefinition { fields, .. } => {
            fields.iter().any(|field| {
                field
                    .default
                    .as_ref()
                    .is_some_and(is_failable_condition_expr)
            })
        }
        _ => false,
    }
}

fn assignment_target_has_failable_expr(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Index { .. } => true,
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            assignment_target_has_failable_expr(object) || is_failable_condition_expr(object)
        }
        _ => false,
    }
}

fn failure_condition_has_failable_expr(condition: &Expr) -> bool {
    match &condition.kind {
        ExprKind::FailureSequence(clauses) => {
            clauses.iter().any(failure_condition_has_failable_expr)
        }
        ExprKind::FailureBind { expr, .. } => is_failable_condition_expr(expr),
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            failure_statements_have_failable_expr(statements)
        }
        _ => is_failable_condition_expr(condition),
    }
}

fn failure_statements_have_failable_expr(statements: &[Stmt]) -> bool {
    statements.iter().any(failure_statement_has_failable_expr)
}

fn failure_statement_has_failable_expr(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Var { expr, .. }
        | StmtKind::Return(expr)
        | StmtKind::Defer(expr)
        | StmtKind::Expr(expr) => is_failable_condition_expr(expr),
        StmtKind::Set { target, expr, .. } => {
            assignment_target_has_failable_expr(target) || is_failable_condition_expr(expr)
        }
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ScopedAccessLevel { .. }
        | StmtKind::ParametricType { .. }
        | StmtKind::ParametricTypeAlias { .. }
        | StmtKind::ExtensionMethod(_) => false,
        StmtKind::Break => false,
    }
}

fn is_failure_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Divide
            | BinaryOp::Remainder
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::And
            | BinaryOp::Or
    )
}

fn enum_variant_names(variants: &[crate::ast::EnumVariant]) -> Vec<String> {
    variants
        .iter()
        .map(|variant| variant.name.clone())
        .collect()
}

fn validate_enum_variant_qualifiers(
    enum_name: &str,
    variants: &[crate::ast::EnumVariant],
) -> Result<(), VerseError> {
    for variant in variants {
        if let Some(qualifier) = &variant.qualifier
            && qualifier != enum_name
        {
            return Err(VerseError::check_at(
                format!(
                    "qualified enum value `{}` must use enum name `{enum_name}`",
                    variant.name
                ),
                variant.span,
            ));
        }
    }
    Ok(())
}

fn rendered_param_name(param: &ParamSpec) -> String {
    if param.named {
        format!("?{}", param.name)
    } else {
        param.name.clone()
    }
}

fn tuple_param_specs_have_named_or_default(params: &[ParamSpec]) -> bool {
    params.iter().any(|param| {
        param.named
            || param.has_default
            || param
                .tuple_items
                .as_deref()
                .is_some_and(tuple_param_specs_have_named_or_default)
    })
}

fn rendered_argument_name(name: &str, optional: bool) -> String {
    if optional {
        format!("?{name}")
    } else {
        name.to_string()
    }
}

impl CallArg {
    fn is_named(&self) -> bool {
        matches!(self, Self::Named { .. })
    }
}

fn loop_body_has_non_break_statement(body: &Expr) -> bool {
    match &body.kind {
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
            .iter()
            .any(|statement| !matches!(statement.kind, StmtKind::Break)),
        _ => true,
    }
}

fn is_supported_using_path(path: &str) -> bool {
    path.starts_with("/Verse.org/")
        || path.starts_with("/Fortnite.com/")
        || path.starts_with("/UnrealEngine.com/")
}

fn is_absolute_module_path(path: &str) -> bool {
    path.starts_with('/')
}

fn is_reserved_type_alias_name(name: &str) -> bool {
    matches!(
        name,
        "number"
            | "nat"
            | "nat8"
            | "nat16"
            | "nat32"
            | "nat64"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "float"
            | "float16"
            | "float32"
            | "float64"
            | "float128"
            | "rational"
            | "bool"
            | "logic"
            | "string"
            | "message"
            | "char"
            | "char8"
            | "char32"
            | "none"
            | "void"
            | "any"
            | "comparable"
            | "array"
            | "function"
            | "tuple"
            | "type"
            | "weak_map"
            | "diagnostic"
            | "entity"
            | "component"
            | "tag"
            | "agent"
            | "session"
            | "player"
            | "team"
            | "event"
            | "subscribable_event"
            | "subscribable_event_intrnl"
            | "sticky_event"
            | "option"
            | "task"
            | "generator"
            | "subtype"
            | "castable_subtype"
            | "concrete_subtype"
            | "castable_concrete_subtype"
            | "classifiable_subset"
            | "classifiable_subset_key"
            | "classifiable_subset_var"
            | "modifier"
            | "modifier_stack"
            | "result"
            | "success_result"
            | "error_result"
            | "awaitable"
            | "signalable"
            | "listenable"
            | "subscribable"
    )
}

fn nat_type() -> Type {
    int_range_type(0, i64::MAX)
}

fn int_range_type(min: i64, max: i64) -> Type {
    Type::IntRange(IntRange::new(min, max))
}

fn builtin_numeric_alias_type(name: &str) -> Option<Type> {
    match name {
        "nat" | "nat64" => Some(nat_type()),
        "nat8" => Some(int_range_type(0, u8::MAX.into())),
        "nat16" => Some(int_range_type(0, u16::MAX.into())),
        "nat32" => Some(int_range_type(0, u32::MAX.into())),
        "int8" => Some(int_range_type(i8::MIN.into(), i8::MAX.into())),
        "int16" => Some(int_range_type(i16::MIN.into(), i16::MAX.into())),
        "int32" => Some(int_range_type(i32::MIN.into(), i32::MAX.into())),
        "int64" => Some(int_range_type(i64::MIN, i64::MAX)),
        _ => None,
    }
}

fn defer_body_is_empty(body: &Expr) -> bool {
    matches!(
        &body.kind,
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) if statements.is_empty()
    )
}

fn defer_body_failable_expr(body: &Expr) -> Option<Span> {
    match &body.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } => Some(body.span),
        ExprKind::Unary {
            op: UnaryOp::Not, ..
        } => Some(body.span),
        ExprKind::Unary { expr, .. } => defer_body_failable_expr(expr),
        ExprKind::Binary { left, op, right } => {
            if is_failure_binary_op(*op) {
                Some(body.span)
            } else {
                defer_body_failable_expr(left).or_else(|| defer_body_failable_expr(right))
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            if failure_condition_has_failable_expr(condition) {
                Some(condition.span)
            } else {
                defer_body_failable_expr(then_branch)
                    .or_else(|| else_branch.as_deref().and_then(defer_body_failable_expr))
            }
        }
        ExprKind::FailureBind { expr, .. } => Some(expr.span),
        ExprKind::FailureSequence(items) => items.iter().find_map(|item| {
            if failure_condition_has_failable_expr(item) {
                Some(item.span)
            } else {
                defer_body_failable_expr(item)
            }
        }),
        ExprKind::Set { target, expr, .. } => {
            if assignment_target_has_failable_expr(target) {
                Some(target.span)
            } else {
                defer_body_failable_expr(expr)
            }
        }
        ExprKind::Var { expr, .. } | ExprKind::TypeLiteral { expr } => {
            defer_body_failable_expr(expr)
        }
        ExprKind::Loop { body } => defer_body_failable_expr(body),
        ExprKind::For { clauses, body } => clauses
            .iter()
            .find_map(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => {
                    if failure_condition_has_failable_expr(iterable) {
                        Some(iterable.span)
                    } else {
                        defer_body_failable_expr(iterable)
                    }
                }
            })
            .or_else(|| defer_body_failable_expr(body)),
        ExprKind::Profile { description, body } => {
            defer_body_failable_expr(description).or_else(|| defer_body_failable_expr(body))
        }
        ExprKind::Spawn { body } | ExprKind::Concurrent { body, .. } => {
            defer_body_failable_expr(body)
        }
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            statements.iter().find_map(defer_statement_failable_expr)
        }
        ExprKind::Function { .. } => None,
        ExprKind::Call { callee, args } => defer_body_failable_expr(callee).or_else(|| {
            args.iter().find_map(|arg| match arg {
                CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                    defer_body_failable_expr(expr)
                }
            })
        }),
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            items.iter().find_map(defer_body_failable_expr)
        }
        ExprKind::Map(entries) => entries.iter().find_map(|(key, value)| {
            defer_body_failable_expr(key).or_else(|| defer_body_failable_expr(value))
        }),
        ExprKind::StructDefinition { fields, .. } | ExprKind::ClassDefinition { fields, .. } => {
            fields
                .iter()
                .find_map(|field| field.default.as_ref().and_then(defer_body_failable_expr))
        }
        ExprKind::Archetype {
            callee, entries, ..
        } => defer_body_failable_expr(callee).or_else(|| {
            entries.iter().find_map(|entry| match entry {
                ArchetypeEntry::Field(field) => defer_body_failable_expr(&field.expr),
                ArchetypeEntry::Let(binding) => defer_body_failable_expr(&binding.expr),
                ArchetypeEntry::Block(block) => defer_body_failable_expr(block),
                ArchetypeEntry::ConstructorCall(call) => call
                    .args
                    .iter()
                    .find_map(|arg| defer_body_failable_expr(call_arg_expr(arg))),
            })
        }),
        ExprKind::Case { subject, arms } => defer_body_failable_expr(subject).or_else(|| {
            if !case_arms_have_wildcard(arms) {
                return Some(body.span);
            }
            arms.iter().find_map(|arm| match &arm.pattern {
                CasePattern::Wildcard { .. } => defer_body_failable_expr(&arm.expr),
                CasePattern::Expr(pattern) => defer_body_failable_expr(pattern)
                    .or_else(|| defer_body_failable_expr(&arm.expr)),
            })
        }),
        ExprKind::Option(Some(value)) => defer_body_failable_expr(value),
        ExprKind::InterpolatedString(parts) => parts.iter().find_map(|part| match part {
            InterpolatedStringPart::Text(_) => None,
            InterpolatedStringPart::Expr(expr) => defer_body_failable_expr(expr),
        }),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            defer_body_failable_expr(object)
        }
        ExprKind::Index { .. } => Some(body.span),
        ExprKind::QualifiedName { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Ident(_)
        | ExprKind::TypeAnnotationLiteral { .. }
        | ExprKind::EnumDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. }
        | ExprKind::Option(None) => None,
    }
}

fn defer_statement_failable_expr(statement: &Stmt) -> Option<Span> {
    match &statement.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Var { expr, .. }
        | StmtKind::Return(expr)
        | StmtKind::Defer(expr)
        | StmtKind::Expr(expr) => defer_body_failable_expr(expr),
        StmtKind::Set { target, expr, .. } => {
            if assignment_target_has_failable_expr(target) {
                Some(target.span)
            } else {
                defer_body_failable_expr(expr)
            }
        }
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ScopedAccessLevel { .. }
        | StmtKind::ParametricType { .. }
        | StmtKind::ParametricTypeAlias { .. }
        | StmtKind::ExtensionMethod(_)
        | StmtKind::Break => None,
    }
}

fn archetype_entry_escape(entry: &ArchetypeEntry) -> Option<Span> {
    match entry {
        ArchetypeEntry::Field(field) => archetype_body_escape(&field.expr, 0),
        ArchetypeEntry::Let(binding) => archetype_body_escape(&binding.expr, 0),
        ArchetypeEntry::Block(block) => archetype_body_escape(block, 0),
        ArchetypeEntry::ConstructorCall(call) => call
            .args
            .iter()
            .find_map(|arg| archetype_body_escape(call_arg_expr(arg), 0)),
    }
}

fn archetype_body_escape(body: &Expr, loop_depth: usize) -> Option<Span> {
    match &body.kind {
        ExprKind::Unary { expr, .. } | ExprKind::UnwrapOption(expr) => {
            archetype_body_escape(expr, loop_depth)
        }
        ExprKind::Binary { left, right, .. } => archetype_body_escape(left, loop_depth)
            .or_else(|| archetype_body_escape(right, loop_depth)),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => archetype_body_escape(condition, loop_depth)
            .or_else(|| archetype_body_escape(then_branch, loop_depth))
            .or_else(|| {
                else_branch
                    .as_deref()
                    .and_then(|branch| archetype_body_escape(branch, loop_depth))
            }),
        ExprKind::FailureBind { expr, .. } => archetype_body_escape(expr, loop_depth),
        ExprKind::Set { target, expr, .. } => archetype_body_escape(target, loop_depth)
            .or_else(|| archetype_body_escape(expr, loop_depth)),
        ExprKind::Var { expr, .. } | ExprKind::TypeLiteral { expr } => {
            archetype_body_escape(expr, loop_depth)
        }
        ExprKind::FailureSequence(items) | ExprKind::Array(items) | ExprKind::Tuple(items) => items
            .iter()
            .find_map(|item| archetype_body_escape(item, loop_depth)),
        ExprKind::Loop { body } => archetype_body_escape(body, loop_depth + 1),
        ExprKind::For { clauses, body } => clauses
            .iter()
            .find_map(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => archetype_body_escape(iterable, loop_depth),
            })
            .or_else(|| archetype_body_escape(body, loop_depth)),
        ExprKind::Profile { description, body } => archetype_body_escape(description, loop_depth)
            .or_else(|| archetype_body_escape(body, loop_depth)),
        ExprKind::Spawn { body } => archetype_body_escape(body, loop_depth),
        ExprKind::Concurrent { body, .. } => archetype_body_escape(body, loop_depth),
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
            .iter()
            .find_map(|statement| archetype_statement_escape(statement, loop_depth)),
        ExprKind::Function { .. } => None,
        ExprKind::Call { callee, args } => {
            archetype_body_escape(callee, loop_depth).or_else(|| {
                args.iter().find_map(|arg| match arg {
                    CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                        archetype_body_escape(expr, loop_depth)
                    }
                })
            })
        }
        ExprKind::BracketCall { callee, args } => archetype_body_escape(callee, loop_depth)
            .or_else(|| {
                args.iter()
                    .find_map(|arg| archetype_body_escape(arg, loop_depth))
            }),
        ExprKind::Map(entries) => entries.iter().find_map(|(key, value)| {
            archetype_body_escape(key, loop_depth)
                .or_else(|| archetype_body_escape(value, loop_depth))
        }),
        ExprKind::Archetype {
            callee, entries, ..
        } => archetype_body_escape(callee, loop_depth).or_else(|| {
            entries.iter().find_map(|entry| match entry {
                ArchetypeEntry::Field(field) => archetype_body_escape(&field.expr, loop_depth),
                ArchetypeEntry::Let(binding) => archetype_body_escape(&binding.expr, loop_depth),
                ArchetypeEntry::Block(block) => archetype_body_escape(block, loop_depth),
                ArchetypeEntry::ConstructorCall(call) => call
                    .args
                    .iter()
                    .find_map(|arg| archetype_body_escape(call_arg_expr(arg), loop_depth)),
            })
        }),
        ExprKind::Case { subject, arms } => {
            archetype_body_escape(subject, loop_depth).or_else(|| {
                arms.iter().find_map(|arm| match &arm.pattern {
                    CasePattern::Wildcard { .. } => archetype_body_escape(&arm.expr, loop_depth),
                    CasePattern::Expr(pattern) => archetype_body_escape(pattern, loop_depth)
                        .or_else(|| archetype_body_escape(&arm.expr, loop_depth)),
                })
            })
        }
        ExprKind::Option(Some(value)) => archetype_body_escape(value, loop_depth),
        ExprKind::InterpolatedString(parts) => parts.iter().find_map(|part| match part {
            InterpolatedStringPart::Text(_) => None,
            InterpolatedStringPart::Expr(expr) => archetype_body_escape(expr, loop_depth),
        }),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            archetype_body_escape(object, loop_depth)
        }
        ExprKind::Index { collection, index } => archetype_body_escape(collection, loop_depth)
            .or_else(|| archetype_body_escape(index, loop_depth)),
        ExprKind::StructDefinition { .. }
        | ExprKind::ClassDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. }
        | ExprKind::QualifiedName { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Ident(_)
        | ExprKind::TypeAnnotationLiteral { .. }
        | ExprKind::EnumDefinition { .. }
        | ExprKind::Option(None) => None,
    }
}

fn archetype_statement_escape(statement: &Stmt, loop_depth: usize) -> Option<Span> {
    match &statement.kind {
        StmtKind::Return(_) => Some(statement.span),
        StmtKind::Break if loop_depth == 0 => Some(statement.span),
        StmtKind::Break => None,
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ScopedAccessLevel { .. }
        | StmtKind::ParametricType { .. }
        | StmtKind::ParametricTypeAlias { .. }
        | StmtKind::ExtensionMethod(_) => None,
        StmtKind::Let { expr, .. } | StmtKind::Var { expr, .. } | StmtKind::Expr(expr) => {
            archetype_body_escape(expr, loop_depth)
        }
        StmtKind::Set { target, expr, .. } => archetype_body_escape(target, loop_depth)
            .or_else(|| archetype_body_escape(expr, loop_depth)),
        StmtKind::Defer(body) => archetype_body_escape(body, loop_depth),
    }
}

fn defer_body_escape(body: &Expr, loop_depth: usize) -> Option<(&'static str, Span)> {
    match &body.kind {
        ExprKind::Unary { expr, .. } | ExprKind::UnwrapOption(expr) => {
            defer_body_escape(expr, loop_depth)
        }
        ExprKind::Binary { left, right, .. } => {
            defer_body_escape(left, loop_depth).or_else(|| defer_body_escape(right, loop_depth))
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => defer_body_escape(condition, loop_depth)
            .or_else(|| defer_body_escape(then_branch, loop_depth))
            .or_else(|| {
                else_branch
                    .as_deref()
                    .and_then(|branch| defer_body_escape(branch, loop_depth))
            }),
        ExprKind::FailureBind { expr, .. } => defer_body_escape(expr, loop_depth),
        ExprKind::Set { target, expr, .. } => {
            defer_body_escape(target, loop_depth).or_else(|| defer_body_escape(expr, loop_depth))
        }
        ExprKind::Var { expr, .. } | ExprKind::TypeLiteral { expr } => {
            defer_body_escape(expr, loop_depth)
        }
        ExprKind::FailureSequence(items) | ExprKind::Array(items) | ExprKind::Tuple(items) => items
            .iter()
            .find_map(|item| defer_body_escape(item, loop_depth)),
        ExprKind::Loop { body } => defer_body_escape(body, loop_depth + 1),
        ExprKind::For { clauses, body } => clauses
            .iter()
            .find_map(|clause| match clause {
                ForClause::Generator { iterable, .. }
                | ForClause::Let { expr: iterable, .. }
                | ForClause::RangeOrLet { expr: iterable, .. }
                | ForClause::Filter(iterable) => defer_body_escape(iterable, loop_depth),
            })
            .or_else(|| defer_body_escape(body, loop_depth)),
        ExprKind::Profile { description, body } => defer_body_escape(description, loop_depth)
            .or_else(|| defer_body_escape(body, loop_depth)),
        ExprKind::Spawn { body } => defer_body_escape(body, loop_depth),
        ExprKind::Concurrent { body, .. } => defer_body_escape(body, loop_depth),
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
            .iter()
            .find_map(|statement| defer_statement_escape(statement, loop_depth)),
        ExprKind::Function { .. } => None,
        ExprKind::Call { callee, args } => defer_body_escape(callee, loop_depth).or_else(|| {
            args.iter().find_map(|arg| match arg {
                CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
                    defer_body_escape(expr, loop_depth)
                }
            })
        }),
        ExprKind::BracketCall { callee, args } => {
            defer_body_escape(callee, loop_depth).or_else(|| {
                args.iter()
                    .find_map(|arg| defer_body_escape(arg, loop_depth))
            })
        }
        ExprKind::Map(entries) => entries.iter().find_map(|(key, value)| {
            defer_body_escape(key, loop_depth).or_else(|| defer_body_escape(value, loop_depth))
        }),
        ExprKind::StructDefinition { fields, .. } | ExprKind::ClassDefinition { fields, .. } => {
            fields.iter().find_map(|field| {
                field
                    .default
                    .as_ref()
                    .and_then(|default| defer_body_escape(default, loop_depth))
            })
        }
        ExprKind::Archetype {
            callee, entries, ..
        } => defer_body_escape(callee, loop_depth).or_else(|| {
            entries.iter().find_map(|entry| match entry {
                ArchetypeEntry::Field(field) => defer_body_escape(&field.expr, loop_depth),
                ArchetypeEntry::Let(binding) => defer_body_escape(&binding.expr, loop_depth),
                ArchetypeEntry::Block(block) => defer_body_escape(block, loop_depth),
                ArchetypeEntry::ConstructorCall(call) => call
                    .args
                    .iter()
                    .find_map(|arg| defer_body_escape(call_arg_expr(arg), loop_depth)),
            })
        }),
        ExprKind::Case { subject, arms } => defer_body_escape(subject, loop_depth).or_else(|| {
            arms.iter().find_map(|arm| match &arm.pattern {
                CasePattern::Wildcard { .. } => defer_body_escape(&arm.expr, loop_depth),
                CasePattern::Expr(pattern) => defer_body_escape(pattern, loop_depth)
                    .or_else(|| defer_body_escape(&arm.expr, loop_depth)),
            })
        }),
        ExprKind::Option(Some(value)) => defer_body_escape(value, loop_depth),
        ExprKind::Option(None) => None,
        ExprKind::InterpolatedString(parts) => parts.iter().find_map(|part| match part {
            InterpolatedStringPart::Text(_) => None,
            InterpolatedStringPart::Expr(expr) => defer_body_escape(expr, loop_depth),
        }),
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            defer_body_escape(object, loop_depth)
        }
        ExprKind::Index { collection, index } => defer_body_escape(collection, loop_depth)
            .or_else(|| defer_body_escape(index, loop_depth)),
        ExprKind::QualifiedName { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Ident(_)
        | ExprKind::TypeAnnotationLiteral { .. }
        | ExprKind::EnumDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. } => None,
    }
}

fn defer_statement_escape(statement: &Stmt, loop_depth: usize) -> Option<(&'static str, Span)> {
    match &statement.kind {
        StmtKind::Return(_) => Some(("`return` cannot be used inside `defer`", statement.span)),
        StmtKind::Break if loop_depth == 0 => {
            Some(("`break` cannot exit a `defer` body", statement.span))
        }
        StmtKind::Break => None,
        StmtKind::Using { .. } => None,
        StmtKind::TypeAlias { .. } => None,
        StmtKind::ScopedAccessLevel { .. } => None,
        StmtKind::ParametricType { .. } => None,
        StmtKind::ParametricTypeAlias { .. } => None,
        StmtKind::ExtensionMethod(_) => None,
        StmtKind::Let { expr, .. } | StmtKind::Var { expr, .. } | StmtKind::Expr(expr) => {
            defer_body_escape(expr, loop_depth)
        }
        StmtKind::Set { target, expr, .. } => {
            defer_body_escape(target, loop_depth).or_else(|| defer_body_escape(expr, loop_depth))
        }
        StmtKind::Defer(body) => defer_body_escape(body, loop_depth),
    }
}

fn aggregate_module_name(aggregate_name: &str) -> Option<&str> {
    let uninstantiated = aggregate_name
        .split_once('(')
        .map_or(aggregate_name, |(name, _)| name);
    uninstantiated.rsplit_once('.').map(|(module, _)| module)
}

fn scoped_scope_contains(scope: &str, candidate: &str) -> bool {
    if scope.starts_with('/') || candidate.starts_with('/') {
        return candidate == scope
            || candidate
                .strip_prefix(scope)
                .is_some_and(|rest| rest.starts_with('/'));
    }

    candidate == scope
        || candidate
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn aggregate_unqualified_name(aggregate_name: &str) -> &str {
    let uninstantiated = aggregate_name
        .split_once('(')
        .map_or(aggregate_name, |(name, _)| name);
    uninstantiated
        .rsplit_once('.')
        .map_or(uninstantiated, |(_, name)| name)
}

fn is_access_specifier(specifier: &str) -> bool {
    access_specifier_name(specifier).is_some()
}

fn has_access_specifier(specifiers: &[String]) -> bool {
    specifiers
        .iter()
        .any(|specifier| is_access_specifier(specifier))
}

fn private_or_protected_access_specifier(specifiers: &[String]) -> Option<&str> {
    specifiers
        .iter()
        .map(String::as_str)
        .find(|specifier| matches!(*specifier, "protected" | "private"))
}

fn ensure_private_protected_access_only_in_classes(
    specifiers: &[String],
    span: Span,
) -> Result<(), VerseError> {
    if private_or_protected_access_specifier(specifiers).is_some() {
        return Err(VerseError::check_at(
            "Access levels protected and private are only allowed inside classes",
            span,
        ));
    }
    Ok(())
}

fn module_member_specifiers<'a>(binding_specifiers: &'a [String], expr: &'a Expr) -> &'a [String] {
    if has_access_specifier(binding_specifiers) {
        return binding_specifiers;
    }

    match &expr.kind {
        ExprKind::Function { effects, .. } if has_access_specifier(effects) => effects,
        ExprKind::ClassDefinition { specifiers, .. } if has_access_specifier(specifiers) => {
            specifiers
        }
        _ => binding_specifiers,
    }
}

fn access_level_from_specifiers(
    specifiers: &[String],
    _context: &str,
    span: Span,
) -> Result<AccessLevel, VerseError> {
    let mut access_specifiers = specifiers
        .iter()
        .filter_map(|specifier| access_specifier_name(specifier));
    let Some(first) = access_specifiers.next() else {
        return Ok(AccessLevel::Internal);
    };
    for access in access_specifiers {
        if access == first {
            return Err(VerseError::check_at(
                "Duplicate access levels: [access levels]. Only one access level may be used or omit for default access.",
                span,
            ));
        }
        return Err(VerseError::check_at(
            "Conflicting access levels: [access levels]. Only one access level may be used or omit for default access.",
            span,
        ));
    }

    Ok(match first {
        "public" => AccessLevel::Public,
        "protected" => AccessLevel::Protected,
        "private" => AccessLevel::Private,
        "internal" => AccessLevel::Internal,
        "scoped" => AccessLevel::Scoped,
        _ => unreachable!("filtered access specifiers"),
    })
}

fn access_requires_dependency_validation(access: AccessLevel) -> bool {
    matches!(
        access,
        AccessLevel::Public | AccessLevel::Protected | AccessLevel::Scoped
    )
}

fn access_is_more_visible_than(left: AccessLevel, right: AccessLevel) -> bool {
    match left {
        AccessLevel::Public => !matches!(right, AccessLevel::Public),
        AccessLevel::Scoped => matches!(right, AccessLevel::Internal | AccessLevel::Private),
        AccessLevel::Protected => matches!(
            right,
            AccessLevel::Scoped | AccessLevel::Internal | AccessLevel::Private
        ),
        AccessLevel::Internal => matches!(right, AccessLevel::Private),
        AccessLevel::Private => false,
    }
}

fn access_level_name(access: AccessLevel) -> &'static str {
    match access {
        AccessLevel::Public => "public",
        AccessLevel::Scoped => "scoped",
        AccessLevel::Protected => "protected",
        AccessLevel::Internal => "internal",
        AccessLevel::Private => "private",
    }
}

fn class_constructor_access_from_specifiers(
    specifiers: &[String],
    span: Span,
) -> Result<(AccessLevel, Vec<String>), VerseError> {
    let access = if has_access_specifier(specifiers) {
        access_level_from_specifiers(specifiers, "class constructor", span)?
    } else {
        AccessLevel::Public
    };
    let scopes = if access == AccessLevel::Scoped {
        scoped_access_scopes(specifiers).unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok((access, scopes))
}

fn access_specifier_name(specifier: &str) -> Option<&str> {
    match specifier {
        "public" | "internal" | "protected" | "private" => Some(specifier),
        _ if specifier
            .strip_prefix("scoped{")
            .and_then(|rest| rest.strip_suffix('}'))
            .is_some() =>
        {
            Some("scoped")
        }
        _ => None,
    }
}

fn scoped_access_specifier_scopes(specifier: &str) -> Option<Vec<String>> {
    let inner = specifier
        .strip_prefix("scoped{")
        .and_then(|rest| rest.strip_suffix('}'))?;
    let scopes = inner
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!scopes.is_empty()).then_some(scopes)
}

fn scoped_access_scopes(specifiers: &[String]) -> Option<Vec<String>> {
    specifiers
        .iter()
        .find_map(|specifier| scoped_access_specifier_scopes(specifier))
}

fn dependency_base_name(name: &str) -> &str {
    name.split_once('(').map_or(name, |(base, _)| base)
}

fn is_fits_in_player_map_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "FitsInPlayerMap")
}

fn is_shuffle_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "Shuffle")
}

fn is_concatenate_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "Concatenate")
}

fn is_replace_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "Replace")
}

fn is_make_classifiable_subset_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "MakeClassifiableSubset")
}

fn is_make_classifiable_subset_var_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Ident(name) if name == "MakeClassifiableSubsetVar")
}

fn make_result_callee_name(callee: &Expr) -> Option<&str> {
    let ExprKind::Ident(name) = &callee.kind else {
        return None;
    };
    matches!(name.as_str(), "MakeSuccess" | "MakeError").then_some(name.as_str())
}

fn is_make_result_callee(callee: &Expr) -> bool {
    make_result_callee_name(callee).is_some()
}

fn is_shuffle_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            effects,
            param_types: Some(param_types),
            return_type,
            ..
        } if has_effect(effects, "transacts")
            && matches!(param_types.as_slice(), [Type::Array(_)])
            && matches!(return_type.as_ref(), Type::Array(_))
    )
}

fn is_concatenate_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            param_types: Some(param_types),
            return_type,
            ..
        } if matches!(param_types.as_slice(), [Type::Array(item)] if matches!(item.as_ref(), Type::Array(_)))
            && matches!(return_type.as_ref(), Type::Array(_))
    )
}

fn is_replace_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(4),
            effects,
            param_types: Some(param_types),
            return_type,
            ..
        } if has_effect(effects, "computes")
            && has_effect(effects, "decides")
            && matches!(
                param_types.as_slice(),
                [Type::Array(_), Type::Int, Type::Int, Type::Array(_)]
            )
            && matches!(return_type.as_ref(), Type::Array(_))
    )
}

fn replace_arg_positions(args: &[CallArg], span: Span) -> Result<[usize; 4], VerseError> {
    let param_names = ["Input", "StartIndex", "StopIndex", "ElementsToReplaceWith"];
    let mut positions = [None; 4];
    let mut next_positional = 0usize;

    for (arg_index, arg) in args.iter().enumerate() {
        match arg {
            CallArg::Positional(expr) => {
                let Some(param_index) =
                    (next_positional..param_names.len()).find(|index| positions[*index].is_none())
                else {
                    return Err(VerseError::check_at(
                        "positional argument does not match any positional parameter",
                        expr.span,
                    ));
                };
                positions[param_index] = Some(arg_index);
                next_positional = param_index + 1;
            }
            CallArg::Named {
                name,
                optional,
                span,
                ..
            } => {
                let Some(param_index) = param_names.iter().position(|param| param == name) else {
                    let rendered = rendered_argument_name(name, *optional);
                    return Err(VerseError::check_at(
                        format!("unknown named argument `{rendered}`"),
                        *span,
                    ));
                };
                if *optional {
                    return Err(VerseError::check_at(
                        format!("parameter `{name}` is not a named parameter"),
                        *span,
                    ));
                }
                if positions[param_index].is_some() {
                    return Err(VerseError::check_at(
                        format!("duplicate argument for parameter `{name}`"),
                        *span,
                    ));
                }
                positions[param_index] = Some(arg_index);
            }
        }
    }

    let [Some(input), Some(start), Some(stop), Some(replacement)] = positions else {
        return Err(VerseError::check_at("`Replace` expected 4 arguments", span));
    };
    Ok([input, start, stop, replacement])
}

fn infer_concatenate_item_type(
    args: &[CallArg],
    arg_types: &[Type],
    span: Span,
) -> Result<Type, VerseError> {
    if args.len() == 1
        && let Some(item_type) = concatenate_arrays_argument_item_type(&arg_types[0], span)?
    {
        return Ok(item_type);
    }

    let mut item_type = Type::Unknown;
    for arg_type in arg_types {
        let next = concatenate_packed_argument_item_type(arg_type, span)?;
        item_type = unify_types(&item_type, &next, span)?;
    }
    Ok(item_type)
}

fn concatenate_arrays_argument_item_type(
    value_type: &Type,
    span: Span,
) -> Result<Option<Type>, VerseError> {
    match value_type {
        Type::Array(item) => match item.as_ref() {
            Type::Array(nested) => Ok(Some(nested.as_ref().clone())),
            Type::Unknown | Type::Any => Ok(Some(Type::Unknown)),
            _ => Ok(None),
        },
        Type::Tuple(items) => {
            let mut item_type = Type::Unknown;
            for item in items {
                let next = match item {
                    Type::Array(nested) => nested.as_ref().clone(),
                    Type::Unknown | Type::Any => Type::Unknown,
                    _ => return Ok(None),
                };
                item_type = unify_types(&item_type, &next, span)?;
            }
            Ok(Some(item_type))
        }
        Type::Unknown | Type::Any => Ok(Some(Type::Unknown)),
        _ => Ok(None),
    }
}

fn concatenate_packed_argument_item_type(
    value_type: &Type,
    span: Span,
) -> Result<Type, VerseError> {
    match value_type {
        Type::Array(item) => Ok(item.as_ref().clone()),
        Type::Tuple(items) => {
            let mut item_type = Type::Unknown;
            for item in items {
                item_type = unify_types(&item_type, item, span)?;
            }
            Ok(item_type)
        }
        Type::Unknown | Type::Any => Ok(Type::Unknown),
        other => Ok(other.clone()),
    }
}

fn is_make_classifiable_subset_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            param_types: Some(param_types),
            return_type,
            ..
        } if matches!(param_types.as_slice(), [Type::Array(_)])
            && matches!(return_type.as_ref(), Type::ClassifiableSubset(_))
    )
}

fn is_make_classifiable_subset_var_function_type(callee_type: &Type) -> bool {
    matches!(
        callee_type,
        Type::Function {
            arity: Some(1),
            param_types: Some(params),
            return_type,
            ..
        } if params.len() == 1
            && matches!(params[0], Type::Array(_))
            && matches!(return_type.as_ref(), Type::ClassifiableSubsetVar(_))
    )
}

fn is_length_member_callee(callee: &Expr) -> bool {
    matches!(&callee.kind, ExprKind::Member { name, .. } if name == "Length")
}

fn class_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn field_has_specifier(specifiers: &[String], name: &str) -> bool {
    specifiers.iter().any(|specifier| specifier == name)
}

fn field_has_attribute(attributes: &[FieldAttribute], name: &str) -> bool {
    attributes.iter().any(|attribute| attribute.name == name)
}

fn render_effects(effects: &[String]) -> String {
    effects
        .iter()
        .map(|effect| format!("<{effect}>"))
        .collect::<String>()
}

fn render_type_list(types: &[Type]) -> String {
    types
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_parametric_instance_type_name(name: &str, args: &[Type]) -> String {
    format!("{name}({})", render_type_list(args))
}

fn replace_type_param_atoms(name: &str, inferred: &HashMap<String, Type>) -> String {
    let mut result = String::new();
    let mut token = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            token.push(ch);
            continue;
        }
        if !token.is_empty() {
            if let Some(value_type) = inferred.get(&token) {
                result.push_str(&value_type.to_string());
            } else {
                result.push_str(&token);
            }
            token.clear();
        }
        result.push(ch);
    }
    if !token.is_empty() {
        if let Some(value_type) = inferred.get(&token) {
            result.push_str(&value_type.to_string());
        } else {
            result.push_str(&token);
        }
    }
    result
}

fn split_parametric_instance_type_name(name: &str) -> Option<(String, Vec<String>)> {
    let (head, rest) = name.split_once('(')?;
    let args = rest.strip_suffix(')')?;
    Some((
        head.to_string(),
        split_parametric_instance_type_args(args)
            .into_iter()
            .map(str::to_string)
            .collect(),
    ))
}

fn split_parametric_instance_type_args(args: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut paren_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in args.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if paren_depth == 0 && angle_depth == 0 && bracket_depth == 0 => {
                items.push(args[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < args.len() {
        items.push(args[start..].trim());
    }
    items
}

fn parametric_type_kind(expr: &Expr) -> Option<ParametricTypeKind> {
    match expr.kind {
        ExprKind::StructDefinition { .. } => Some(ParametricTypeKind::Struct),
        ExprKind::ClassDefinition { .. } => Some(ParametricTypeKind::Class),
        ExprKind::InterfaceDefinition { .. } => Some(ParametricTypeKind::Interface),
        _ => None,
    }
}

fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        if !deduped.iter().any(|existing| existing == &item) {
            deduped.push(item);
        }
    }
    deduped
}

fn char_array_type() -> Type {
    Type::Array(Box::new(Type::Char))
}

fn native_function_type(
    effects: &[&str],
    params: Vec<(&'static str, Type)>,
    return_type: Type,
) -> Type {
    Type::Function {
        arity: Some(params.len()),
        arity_range: None,
        effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
        param_types: Some(
            params
                .iter()
                .map(|(_, value_type)| value_type.clone())
                .collect(),
        ),
        param_specs: Some(
            params
                .into_iter()
                .map(|(name, value_type)| ParamSpec {
                    name: name.to_string(),
                    value_type,
                    named: false,
                    has_default: false,
                    tuple_items: None,
                })
                .collect(),
        ),
        return_type: Box::new(return_type),
    }
}

fn result_accessor_type(return_type: &Type) -> Type {
    result_accessor_type_with_effects(
        return_type,
        vec!["computes".to_string(), "decides".to_string()],
    )
}

fn result_present_accessor_type(return_type: &Type) -> Type {
    result_accessor_type_with_effects(return_type, vec!["computes".to_string()])
}

fn result_accessor_type_with_effects(return_type: &Type, effects: Vec<String>) -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects,
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(return_type.clone()),
    }
}

fn await_type(payload: Option<&Type>) -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec![
            "transacts".to_string(),
            "suspends".to_string(),
            "no_rollback".to_string(),
        ],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(payload.cloned().unwrap_or(Type::None)),
    }
}

fn task_cancel_type() -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn signal_type(payload: Option<&Type>) -> Type {
    let param_types = payload.iter().copied().cloned().collect::<Vec<_>>();
    Type::Function {
        arity: Some(param_types.len()),
        arity_range: None,
        effects: vec!["transacts".to_string(), "no_rollback".to_string()],
        param_types: Some(param_types),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn subscribable_event_signal_type(payload: Option<&Type>) -> Type {
    let param_types = payload.iter().copied().cloned().collect::<Vec<_>>();
    Type::Function {
        arity: Some(param_types.len()),
        arity_range: None,
        effects: vec!["predicts".to_string()],
        param_types: Some(param_types),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn subscribable_event_broadcast_type(payload: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["predicts".to_string()],
        param_types: Some(vec![payload.clone()]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn sticky_event_is_signaled_type() -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec!["reads".to_string(), "decides".to_string()],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn sticky_event_clear_signal_type() -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec!["writes".to_string()],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_contains_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string(), "decides".to_string()],
        param_types: Some(vec![classifiable_subset_query_type(item_type)]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_not_contains_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["reads".to_string(), "decides".to_string()],
        param_types: Some(vec![classifiable_subset_query_type(item_type)]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_contains_many_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::Array(Box::new(classifiable_subset_query_type(
            item_type,
        )))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_contains_none_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["reads".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::Array(Box::new(classifiable_subset_query_type(
            item_type,
        )))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_filter_by_type_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![classifiable_subset_query_type(item_type)]),
        param_specs: None,
        return_type: Box::new(Type::ClassifiableSubset(Box::new(item_type.clone()))),
    }
}

fn classifiable_subset_query_type(item_type: &Type) -> Type {
    Type::CastableSubtype(Box::new(Type::TypeValueBounds {
        lower: Box::new(item_type.clone()),
        upper: Box::new(Type::Any),
    }))
}

fn castable_instance_is_of_type_type() -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["reads".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::CastableSubtype(Box::new(Type::Any))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_var_read_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(0),
        arity_range: None,
        effects: vec!["reads".to_string()],
        param_types: Some(Vec::new()),
        param_specs: None,
        return_type: Box::new(Type::ClassifiableSubset(Box::new(item_type.clone()))),
    }
}

fn classifiable_subset_var_write_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["writes".to_string()],
        param_types: Some(vec![Type::ClassifiableSubset(Box::new(item_type.clone()))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_var_add_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![item_type.clone()]),
        param_specs: None,
        return_type: Box::new(Type::ClassifiableSubsetKey(Box::new(item_type.clone()))),
    }
}

fn classifiable_subset_var_remove_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string(), "decides".to_string()],
        param_types: Some(vec![Type::ClassifiableSubsetKey(Box::new(
            item_type.clone(),
        ))]),
        param_specs: None,
        return_type: Box::new(Type::None),
    }
}

fn classifiable_subset_element_type(item_type: &Type) -> Type {
    match item_type {
        Type::CastableSubtype(item) => item.as_ref().clone(),
        other => other.clone(),
    }
}

fn modifier_evaluate_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: Vec::new(),
        param_types: Some(vec![item_type.clone()]),
        param_specs: None,
        return_type: Box::new(item_type.clone()),
    }
}

fn modifier_method_info(item_type: &Type, span: Span) -> ClassMethodInfo {
    ClassMethodInfo {
        qualifier: None,
        name: "Evaluate".to_string(),
        value_type: modifier_evaluate_type(item_type),
        final_member: false,
        abstract_member: true,
        access: AccessLevel::Public,
        scopes: Vec::new(),
        owner: Some(format!("modifier({item_type})")),
        span,
    }
}

fn modifier_stack_add_modifier_type(item_type: &Type) -> Type {
    Type::Function {
        arity: Some(2),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![
            Type::Modifier(Box::new(item_type.clone())),
            Type::Rational,
        ]),
        param_specs: None,
        return_type: Box::new(Type::Interface("cancelable".to_string())),
    }
}

fn subscribe_type(payload: Option<&Type>) -> Type {
    Type::Overload(vec![
        subscribe_type_with_callback_effects(payload, &[]),
        subscribe_type_with_callback_effects(payload, &["transacts"]),
    ])
}

fn subscribe_type_with_callback_effects(payload: Option<&Type>, callback_effects: &[&str]) -> Type {
    let callback_param_types = payload.iter().copied().cloned().collect::<Vec<_>>();
    Type::Function {
        arity: Some(1),
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: Some(vec![Type::Function {
            arity: Some(callback_param_types.len()),
            arity_range: None,
            effects: callback_effects
                .iter()
                .map(|effect| (*effect).to_string())
                .collect(),
            param_types: Some(callback_param_types),
            param_specs: None,
            return_type: Box::new(Type::None),
        }]),
        param_specs: None,
        return_type: Box::new(Type::Interface("cancelable".to_string())),
    }
}

fn is_official_parametric_type_name(name: &str) -> bool {
    matches!(
        name,
        "event"
            | "subscribable_event"
            | "subscribable_event_intrnl"
            | "sticky_event"
            | "option"
            | "task"
            | "generator"
            | "subtype"
            | "castable_subtype"
            | "concrete_subtype"
            | "castable_concrete_subtype"
            | "classifiable_subset"
            | "classifiable_subset_key"
            | "classifiable_subset_var"
            | "modifier"
            | "modifier_stack"
            | "result"
            | "success_result"
            | "error_result"
            | "awaitable"
            | "signalable"
            | "listenable"
            | "subscribable"
    )
}

fn official_parametric_type(name: &str, args: &[Type], span: Span) -> Result<Type, VerseError> {
    match name {
        "event" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Event(args.first().cloned().map(Box::new)))
        }
        "subscribable_event" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::SubscribableEvent(Box::new(args[0].clone())))
        }
        "subscribable_event_intrnl" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::SubscribableEventIntrnl(
                args.first().cloned().map(Box::new),
            ))
        }
        "sticky_event" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::StickyEvent(args.first().cloned().map(Box::new)))
        }
        "option" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Option(Box::new(args[0].clone())))
        }
        "result" => {
            ensure_parametric_type_arity(name, args, &[2], span)?;
            Ok(Type::Result(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
            ))
        }
        "success_result" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::SuccessResult(Box::new(args[0].clone())))
        }
        "error_result" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ErrorResult(Box::new(args[0].clone())))
        }
        "task" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Task(Box::new(args[0].clone())))
        }
        "generator" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Generator(args.first().cloned().map(Box::new)))
        }
        "subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Subtype(Box::new(args[0].clone())))
        }
        "castable_subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::CastableSubtype(Box::new(args[0].clone())))
        }
        "concrete_subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ConcreteSubtype(Box::new(args[0].clone())))
        }
        "castable_concrete_subtype" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ConcreteSubtype(Box::new(Type::CastableSubtype(
                Box::new(args[0].clone()),
            ))))
        }
        "classifiable_subset" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ClassifiableSubset(Box::new(args[0].clone())))
        }
        "classifiable_subset_key" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ClassifiableSubsetKey(Box::new(args[0].clone())))
        }
        "classifiable_subset_var" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ClassifiableSubsetVar(Box::new(args[0].clone())))
        }
        "modifier" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Modifier(Box::new(args[0].clone())))
        }
        "modifier_stack" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::ModifierStack(Box::new(args[0].clone())))
        }
        "signalable" => {
            ensure_parametric_type_arity(name, args, &[1], span)?;
            Ok(Type::Signalable(Box::new(args[0].clone())))
        }
        "awaitable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Awaitable(args.first().cloned().map(Box::new)))
        }
        "listenable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Listenable(args.first().cloned().map(Box::new)))
        }
        "subscribable" => {
            ensure_parametric_type_arity(name, args, &[0, 1], span)?;
            Ok(Type::Subscribable(args.first().cloned().map(Box::new)))
        }
        _ => Err(VerseError::check_at(
            format!("unknown parametric type `{name}`"),
            span,
        )),
    }
}

fn ensure_parametric_type_arity(
    name: &str,
    args: &[Type],
    expected: &[usize],
    span: Span,
) -> Result<(), VerseError> {
    if expected.contains(&args.len()) {
        return Ok(());
    }

    let expected = expected
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    Err(VerseError::check_at(
        format!(
            "parametric type `{name}` expected {expected} type arguments, got {}",
            args.len()
        ),
        span,
    ))
}

fn color_type() -> Type {
    Type::Struct("color".to_string())
}

fn color_alpha_type() -> Type {
    Type::Struct("color_alpha".to_string())
}

fn is_builtin_class_type_name(name: &str) -> bool {
    matches!(
        name,
        "diagnostic" | "entity" | "component" | "tag" | "session" | "player" | "agent" | "team"
    )
}

fn is_builtin_class_base_name(name: &str) -> bool {
    matches!(name, "component" | "tag")
}

fn is_builtin_comparable_class_name(name: &str) -> bool {
    matches!(name, "session" | "player" | "team")
}

fn ensure_int_index_type(value_type: &Type, context: &str, span: Span) -> Result<(), VerseError> {
    match value_type {
        Type::Int | Type::Unknown | Type::Any => Ok(()),
        other => Err(VerseError::check_at(
            format!("{context} expected `int`, got `{other}`"),
            span,
        )),
    }
}

fn tuple_index_literal(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Number {
            value: NumberLiteral::Int(index),
            ..
        } => Some(*index),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => match &expr.kind {
            ExprKind::Number {
                value: NumberLiteral::Int(index),
                ..
            } => index.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

const BUILTIN_INTERFACE_NAMES: &[&str] = &[
    "cancelable",
    "disposable",
    "enableable",
    "invalidatable",
    "showable",
];

fn builtin_session_environment_info() -> EnumInfo {
    EnumInfo {
        variants: vec![
            "Edit".to_string(),
            "Private".to_string(),
            "Live".to_string(),
        ],
        open: false,
        persistable: false,
    }
}

fn print_function_type(message_type: Type) -> Type {
    Type::Function {
        arity: None,
        arity_range: None,
        effects: vec!["transacts".to_string()],
        param_types: None,
        param_specs: Some(vec![
            ParamSpec {
                name: "Message".to_string(),
                value_type: message_type,
                named: false,
                has_default: false,
                tuple_items: None,
            },
            ParamSpec {
                name: "Duration".to_string(),
                value_type: Type::Float,
                named: true,
                has_default: true,
                tuple_items: None,
            },
            ParamSpec {
                name: "Color".to_string(),
                value_type: color_type(),
                named: true,
                has_default: true,
                tuple_items: None,
            },
        ]),
        return_type: Box::new(Type::None),
    }
}

fn builtin_interface_infos() -> HashMap<String, InterfaceInfo> {
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "cancelable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![builtin_interface_method(
                "cancelable",
                "Cancel",
                &["transacts"],
            )],
        },
    );
    interfaces.insert(
        "disposable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![builtin_interface_method(
                "disposable",
                "Dispose",
                &["transacts"],
            )],
        },
    );
    interfaces.insert(
        "enableable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: Vec::new(),
            methods: vec![
                builtin_interface_method("enableable", "Enable", &["transacts"]),
                builtin_interface_method("enableable", "Disable", &["transacts"]),
                builtin_interface_method("enableable", "IsEnabled", &["transacts", "decides"]),
            ],
        },
    );
    interfaces.insert(
        "invalidatable".to_string(),
        InterfaceInfo {
            parents: vec!["disposable".to_string()],
            fields: Vec::new(),
            methods: vec![
                builtin_interface_method("disposable", "Dispose", &["transacts"]),
                builtin_interface_method("invalidatable", "IsValid", &["transacts", "decides"]),
            ],
        },
    );
    interfaces.insert(
        "showable".to_string(),
        InterfaceInfo {
            parents: Vec::new(),
            fields: vec![builtin_interface_field(
                "showable",
                "Show",
                Type::Option(Box::new(Type::Bool)),
                true,
            )],
            methods: Vec::new(),
        },
    );
    interfaces
}

fn builtin_interface_method(
    interface_name: &str,
    method_name: &str,
    effects: &[&str],
) -> ClassMethodInfo {
    ClassMethodInfo {
        qualifier: None,
        name: method_name.to_string(),
        value_type: Type::Function {
            arity: Some(0),
            arity_range: None,
            effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
            param_types: Some(Vec::new()),
            param_specs: Some(Vec::new()),
            return_type: Box::new(Type::None),
        },
        final_member: false,
        abstract_member: true,
        access: AccessLevel::Public,
        scopes: Vec::new(),
        owner: Some(interface_name.to_string()),
        span: Span::new(0, 0, 1, 1),
    }
}

fn builtin_interface_field(
    interface_name: &str,
    field_name: &str,
    value_type: Type,
    mutable: bool,
) -> StructFieldInfo {
    StructFieldInfo {
        name: field_name.to_string(),
        value_type,
        has_default: false,
        mutable,
        predicts: false,
        final_member: false,
        access: AccessLevel::Public,
        scopes: Vec::new(),
        mutation_access: AccessLevel::Public,
        mutation_scopes: Vec::new(),
        owner: Some(interface_name.to_string()),
        span: Span::new(0, 0, 1, 1),
    }
}

fn builtin_color_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        native: true,
        persistable: true,
        computes: false,
        constructor_effects: Vec::new(),
        constructor_access: AccessLevel::Public,
        constructor_scopes: Vec::new(),
        fields: ["R", "G", "B"]
            .into_iter()
            .map(|name| StructFieldInfo {
                name: name.to_string(),
                value_type: Type::Float,
                has_default: false,
                mutable: false,
                predicts: false,
                final_member: false,
                access: AccessLevel::Public,
                scopes: Vec::new(),
                mutation_access: AccessLevel::Public,
                mutation_scopes: Vec::new(),
                owner: Some("color".to_string()),
                span: Span::new(0, 0, 1, 1),
            })
            .collect(),
        methods: Vec::new(),
    }
}

fn builtin_color_alpha_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        native: true,
        persistable: false,
        computes: false,
        constructor_effects: Vec::new(),
        constructor_access: AccessLevel::Public,
        constructor_scopes: Vec::new(),
        fields: [
            ("Color".to_string(), color_type()),
            ("A".to_string(), Type::Float),
        ]
        .into_iter()
        .map(|(name, value_type)| StructFieldInfo {
            name,
            value_type,
            has_default: false,
            mutable: false,
            predicts: false,
            final_member: false,
            access: AccessLevel::Public,
            scopes: Vec::new(),
            mutation_access: AccessLevel::Public,
            mutation_scopes: Vec::new(),
            owner: Some("color_alpha".to_string()),
            span: Span::new(0, 0, 1, 1),
        })
        .collect(),
        methods: Vec::new(),
    }
}

fn builtin_locale_info() -> StructInfo {
    StructInfo {
        kind: AggregateKind::Struct,
        base: None,
        interfaces: Vec::new(),
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        concrete: false,
        castable: false,
        native: true,
        persistable: false,
        computes: false,
        constructor_effects: Vec::new(),
        constructor_access: AccessLevel::Public,
        constructor_scopes: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
    }
}

fn is_char_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Char | Type::Char8 | Type::Char32)
}

fn is_byte_char_type(value_type: &Type) -> bool {
    matches!(value_type, Type::Char | Type::Char8)
}

fn is_string_char_type(value_type: &Type) -> bool {
    is_byte_char_type(value_type)
}

fn is_empty_option_literal(expected: &Type, expr: &Expr) -> bool {
    matches!(expected, Type::Option(_)) && matches!(&expr.kind, ExprKind::Bool(false))
}

fn is_empty_option_candidate(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Bool(false))
}

fn expr_int_literal_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::Number {
            value: NumberLiteral::Int(value),
            kind: NumberKind::Int,
        } => int_literal_value_to_i64(*value, false),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => match &expr.kind {
            ExprKind::Number {
                value: NumberLiteral::Int(value),
                kind: NumberKind::Int,
            } => int_literal_value_to_i64(*value, true),
            _ => None,
        },
        ExprKind::Unary {
            op: UnaryOp::Positive,
            expr,
        } => expr_int_literal_value(expr),
        _ => None,
    }
}

fn expr_float_literal_value(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Number { value, .. } => Some(number_literal_value_to_f64(*value, false)),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            expr,
        } => match &expr.kind {
            ExprKind::Number { value, .. } => Some(number_literal_value_to_f64(*value, true)),
            _ => None,
        },
        ExprKind::Unary {
            op: UnaryOp::Positive,
            expr,
        } => expr_float_literal_value(expr),
        _ => None,
    }
}

fn number_literal_value_to_f64(value: NumberLiteral, negative: bool) -> f64 {
    let value = match value {
        NumberLiteral::Int(value) => value as f64,
        NumberLiteral::Float(value) => value,
    };
    if negative { -value } else { value }
}

fn int_literal_value_to_i64(value: i128, negative: bool) -> Option<i64> {
    if negative {
        let min_magnitude = i128::from(i64::MAX) + 1;
        if value > min_magnitude {
            None
        } else if value == min_magnitude {
            Some(i64::MIN)
        } else {
            Some(-(value as i64))
        }
    } else {
        (value <= i128::from(i64::MAX)).then_some(value as i64)
    }
}

fn finalize_collection_item_type(
    current: &mut Type,
    pending_empty_options: &mut Vec<&Expr>,
) -> Result<(), VerseError> {
    if matches!(current, Type::Unknown) {
        if !pending_empty_options.is_empty() {
            *current = Type::Bool;
        }
        pending_empty_options.clear();
        return Ok(());
    }

    for expr in pending_empty_options.drain(..) {
        if !is_empty_option_literal(current, expr) {
            *current = unify_types(current, &Type::Bool, expr.span)?;
        }
    }
    Ok(())
}

fn validate_weak_map_type(
    key_type: &Type,
    value_type: &Type,
    span: Span,
    enum_types: &HashMap<String, EnumInfo>,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<(), VerseError> {
    match key_type {
        Type::Class(name) if name == "session" => Ok(()),
        Type::Class(name) if name == "player" => {
            if is_persistable_type_name(value_type, enum_types, struct_types) {
                Ok(())
            } else {
                Err(VerseError::check_at(
                    format!("weak_map(player, ...) value type `{value_type}` must be persistable"),
                    span,
                ))
            }
        }
        other => Err(VerseError::check_at(
            format!("weak_map key type must be `session` or `player`, got `{other}`"),
            span,
        )),
    }
}

fn is_persistable_type_name(
    value_type: &Type,
    enum_types: &HashMap<String, EnumInfo>,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match value_type {
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::FloatRange(_)
        | Type::Rational
        | Type::Number
        | Type::Bool
        | Type::String
        | Type::Message
        | Type::Char
        | Type::Char8
        | Type::Char32
        | Type::None => true,
        Type::Array(item) | Type::Option(item) => {
            is_persistable_type_name(item, enum_types, struct_types)
        }
        Type::Map(key, value) => {
            is_persistable_type_name(key, enum_types, struct_types)
                && is_persistable_type_name(value, enum_types, struct_types)
        }
        Type::Tuple(items) => items
            .iter()
            .all(|item| is_persistable_type_name(item, enum_types, struct_types)),
        Type::Enum(name) => enum_types.get(name).is_some_and(|info| info.persistable),
        Type::Struct(name) | Type::Class(name) => {
            struct_types.get(name).is_some_and(|info| info.persistable)
        }
        Type::Any
        | Type::Comparable
        | Type::Unknown
        | Type::Never
        | Type::Range
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::Param(_, _)
        | Type::ParametricType { .. }
        | Type::TypeValue
        | Type::TypeValueOf(_)
        | Type::TypeValueBounds { .. }
        | Type::WeakMap(_, _)
        | Type::Result(_, _)
        | Type::SuccessResult(_)
        | Type::ErrorResult(_)
        | Type::Event(_)
        | Type::SubscribableEvent(_)
        | Type::SubscribableEventIntrnl(_)
        | Type::StickyEvent(_)
        | Type::Task(_)
        | Type::Generator(_)
        | Type::Subtype(_)
        | Type::CastableSubtype(_)
        | Type::ConcreteSubtype(_)
        | Type::ClassifiableSubset(_)
        | Type::ClassifiableSubsetKey(_)
        | Type::ClassifiableSubsetVar(_)
        | Type::Modifier(_)
        | Type::ModifierStack(_)
        | Type::Awaitable(_)
        | Type::Signalable(_)
        | Type::Subscribable(_)
        | Type::Listenable(_)
        | Type::Function { .. }
        | Type::Overload(_) => false,
    }
}
