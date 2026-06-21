use crate::ast::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct Desugarer;

impl Desugarer {
    pub fn new() -> Self {
        Self
    }

    pub fn desugar_program(self, program: &Program) -> Program {
        desugar_program(program)
    }
}

pub fn desugar_program(program: &Program) -> Program {
    Program {
        statements: desugar_statements(&program.statements),
    }
}

fn desugar_statements(statements: &[Stmt]) -> Vec<Stmt> {
    statements.iter().map(desugar_stmt).collect()
}

fn desugar_stmt(statement: &Stmt) -> Stmt {
    let kind = match &statement.kind {
        StmtKind::Using { path } => StmtKind::Using { path: path.clone() },
        StmtKind::Let {
            name,
            specifiers,
            annotation,
            expr,
        } => StmtKind::Let {
            name: name.clone(),
            specifiers: specifiers.clone(),
            annotation: annotation.clone(),
            expr: desugar_expr(expr),
        },
        StmtKind::ParametricType {
            name,
            specifiers,
            params,
            expr,
        } => StmtKind::ParametricType {
            name: name.clone(),
            specifiers: specifiers.clone(),
            params: params.clone(),
            expr: desugar_expr(expr),
        },
        StmtKind::TypeAlias { name, target } => StmtKind::TypeAlias {
            name: name.clone(),
            target: target.clone(),
        },
        StmtKind::ExtensionMethod(method) => {
            StmtKind::ExtensionMethod(Box::new(desugar_extension_method(method)))
        }
        StmtKind::Var {
            name,
            annotation,
            expr,
        } => StmtKind::Var {
            name: name.clone(),
            annotation: annotation.clone(),
            expr: desugar_expr(expr),
        },
        StmtKind::Set { target, op, expr } => StmtKind::Set {
            target: desugar_expr(target),
            op: *op,
            expr: desugar_expr(expr),
        },
        StmtKind::Return(expr) => StmtKind::Return(desugar_expr(expr)),
        StmtKind::Break => StmtKind::Break,
        StmtKind::Defer(expr) => StmtKind::Defer(desugar_expr(expr)),
        StmtKind::Expr(expr) => StmtKind::Expr(desugar_expr(expr)),
    };
    Stmt {
        kind,
        span: statement.span,
    }
}

fn desugar_expr(expr: &Expr) -> Expr {
    desugar_expr_inner(expr, false)
}

fn desugar_expr_preserving_colon_block(expr: &Expr) -> Expr {
    desugar_expr_inner(expr, true)
}

fn desugar_expr_inner(expr: &Expr, preserve_colon_block: bool) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Number { value, kind } => ExprKind::Number {
            value: *value,
            kind: *kind,
        },
        ExprKind::Char { value, kind } => ExprKind::Char {
            value: *value,
            kind: *kind,
        },
        ExprKind::Bool(value) => ExprKind::Bool(*value),
        ExprKind::String(value) => ExprKind::String(value.clone()),
        ExprKind::InterpolatedString(parts) => ExprKind::InterpolatedString(
            parts
                .iter()
                .map(|part| match part {
                    InterpolatedStringPart::Text(text) => {
                        InterpolatedStringPart::Text(text.clone())
                    }
                    InterpolatedStringPart::Expr(expr) => {
                        InterpolatedStringPart::Expr(Box::new(desugar_expr(expr)))
                    }
                })
                .collect(),
        ),
        ExprKind::None => ExprKind::None,
        ExprKind::Ident(name) => ExprKind::Ident(name.clone()),
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(desugar_expr(expr)),
        },
        ExprKind::Binary { left, op, right } => ExprKind::Binary {
            left: Box::new(desugar_expr(left)),
            op: *op,
            right: Box::new(desugar_expr(right)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(desugar_expr(condition)),
            then_branch: Box::new(desugar_expr(then_branch)),
            else_branch: else_branch.as_deref().map(desugar_expr).map(Box::new),
        },
        ExprKind::FailureBind { name, expr } => ExprKind::FailureBind {
            name: name.clone(),
            expr: Box::new(desugar_expr(expr)),
        },
        ExprKind::FailureSequence(clauses) => ExprKind::FailureSequence(
            clauses
                .iter()
                .flat_map(|clause| {
                    let clause = desugar_expr(clause);
                    match clause.kind {
                        ExprKind::FailureSequence(nested) => nested,
                        _ => vec![clause],
                    }
                })
                .collect(),
        ),
        ExprKind::Set { target, op, expr } => ExprKind::Set {
            target: Box::new(desugar_expr(target)),
            op: *op,
            expr: Box::new(desugar_expr(expr)),
        },
        ExprKind::Var {
            name,
            annotation,
            expr,
        } => ExprKind::Var {
            name: name.clone(),
            annotation: annotation.clone(),
            expr: Box::new(desugar_expr(expr)),
        },
        ExprKind::External => ExprKind::External,
        ExprKind::Loop { body } => ExprKind::Loop {
            body: Box::new(desugar_expr(body)),
        },
        ExprKind::For { clauses, body } => ExprKind::For {
            clauses: clauses.iter().map(desugar_for_clause).collect(),
            body: Box::new(desugar_expr(body)),
        },
        ExprKind::Profile { description, body } => ExprKind::Profile {
            description: Box::new(desugar_expr(description)),
            body: Box::new(desugar_expr(body)),
        },
        ExprKind::Spawn { body } => ExprKind::Spawn {
            body: Box::new(desugar_expr(body)),
        },
        ExprKind::Concurrent { op, body } => ExprKind::Concurrent {
            op: *op,
            body: Box::new(desugar_expr_preserving_colon_block(body)),
        },
        ExprKind::Block(statements) => ExprKind::Block(desugar_statements(statements)),
        ExprKind::ColonBlock(statements) if preserve_colon_block => {
            ExprKind::ColonBlock(desugar_statements(statements))
        }
        ExprKind::ColonBlock(statements) => ExprKind::Block(desugar_statements(statements)),
        ExprKind::Function {
            params,
            effects,
            return_type,
            body,
        } => ExprKind::Function {
            params: params.iter().map(desugar_param).collect(),
            effects: effects.clone(),
            return_type: return_type.clone(),
            body: Box::new(desugar_expr(body)),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: Box::new(desugar_expr(callee)),
            args: args.iter().map(desugar_call_arg).collect(),
        },
        ExprKind::BracketCall { callee, args } => ExprKind::BracketCall {
            callee: Box::new(desugar_expr(callee)),
            args: args.iter().map(desugar_expr).collect(),
        },
        ExprKind::Array(items) => ExprKind::Array(items.iter().map(desugar_expr).collect()),
        ExprKind::Map(entries) => ExprKind::Map(
            entries
                .iter()
                .map(|(key, value)| (desugar_expr(key), desugar_expr(value)))
                .collect(),
        ),
        ExprKind::EnumDefinition {
            open,
            persistable,
            block,
            variants,
        } => ExprKind::EnumDefinition {
            open: *open,
            persistable: *persistable,
            block: *block,
            variants: variants.clone(),
        },
        ExprKind::StructDefinition {
            persistable,
            computes,
            block,
            fields,
        } => ExprKind::StructDefinition {
            persistable: *persistable,
            computes: *computes,
            block: *block,
            fields: fields.iter().map(desugar_struct_field).collect(),
        },
        ExprKind::ClassDefinition {
            block,
            specifiers,
            base,
            interfaces,
            fields,
            methods,
            extension_methods,
            blocks,
        } => ExprKind::ClassDefinition {
            block: *block,
            specifiers: specifiers.clone(),
            base: base.clone(),
            interfaces: interfaces.clone(),
            fields: fields.iter().map(desugar_struct_field).collect(),
            methods: methods.iter().map(desugar_class_method).collect(),
            extension_methods: extension_methods
                .iter()
                .map(desugar_extension_method)
                .collect(),
            blocks: blocks.iter().map(desugar_class_block).collect(),
        },
        ExprKind::InterfaceDefinition {
            block,
            parents,
            fields,
            methods,
        } => ExprKind::InterfaceDefinition {
            block: *block,
            parents: parents.clone(),
            fields: fields.iter().map(desugar_struct_field).collect(),
            methods: methods.iter().map(desugar_class_method).collect(),
        },
        ExprKind::ModuleDefinition { block, statements } => ExprKind::ModuleDefinition {
            block: *block,
            statements: desugar_statements(statements),
        },
        ExprKind::Archetype {
            block,
            callee,
            entries,
        } => ExprKind::Archetype {
            block: *block,
            callee: Box::new(desugar_expr(callee)),
            entries: entries.iter().map(desugar_archetype_entry).collect(),
        },
        ExprKind::Case { subject, arms } => ExprKind::Case {
            subject: Box::new(desugar_expr(subject)),
            arms: arms.iter().map(desugar_case_arm).collect(),
        },
        ExprKind::Tuple(items) => ExprKind::Tuple(items.iter().map(desugar_expr).collect()),
        ExprKind::Option(value) => {
            ExprKind::Option(value.as_deref().map(desugar_expr).map(Box::new))
        }
        ExprKind::UnwrapOption(value) => ExprKind::UnwrapOption(Box::new(desugar_expr(value))),
        ExprKind::QualifiedName { qualifier, name } => ExprKind::QualifiedName {
            qualifier: qualifier.clone(),
            name: name.clone(),
        },
        ExprKind::QualifiedMember {
            object,
            qualifier,
            name,
        } => ExprKind::QualifiedMember {
            object: Box::new(desugar_expr(object)),
            qualifier: qualifier.clone(),
            name: name.clone(),
        },
        ExprKind::Member { object, name } => ExprKind::Member {
            object: Box::new(desugar_expr(object)),
            name: name.clone(),
        },
        ExprKind::Index { collection, index } => ExprKind::Index {
            collection: Box::new(desugar_expr(collection)),
            index: Box::new(desugar_expr(index)),
        },
    };
    Expr {
        kind,
        span: expr.span,
    }
}

fn desugar_param(param: &Param) -> Param {
    let pattern = match &param.pattern {
        ParamPattern::Binding => ParamPattern::Binding,
        ParamPattern::Anonymous => ParamPattern::Anonymous,
        ParamPattern::Tuple(items) => {
            ParamPattern::Tuple(items.iter().map(desugar_param).collect())
        }
    };
    Param {
        name: param.name.clone(),
        annotation: param.annotation.clone(),
        type_params: param.type_params.clone(),
        named: param.named,
        default: param.default.as_ref().map(desugar_expr),
        pattern,
        span: param.span,
    }
}

fn desugar_for_clause(clause: &ForClause) -> ForClause {
    match clause {
        ForClause::Generator {
            binding,
            iterable,
            span,
        } => ForClause::Generator {
            binding: binding.clone(),
            iterable: desugar_expr(iterable),
            span: *span,
        },
        ForClause::Filter(expr) => ForClause::Filter(desugar_expr(expr)),
        ForClause::Let { name, expr, span } => ForClause::Let {
            name: name.clone(),
            expr: desugar_expr(expr),
            span: *span,
        },
        ForClause::RangeOrLet { name, expr, span } => ForClause::RangeOrLet {
            name: name.clone(),
            expr: desugar_expr(expr),
            span: *span,
        },
    }
}

fn desugar_call_arg(arg: &CallArg) -> CallArg {
    match arg {
        CallArg::Positional(expr) => CallArg::Positional(desugar_expr(expr)),
        CallArg::Named {
            name,
            expr,
            optional,
            span,
        } => CallArg::Named {
            name: name.clone(),
            expr: desugar_expr(expr),
            optional: *optional,
            span: *span,
        },
    }
}

fn desugar_struct_field(field: &StructField) -> StructField {
    StructField {
        name: field.name.clone(),
        attributes: field
            .attributes
            .iter()
            .map(desugar_field_attribute)
            .collect(),
        var_specifiers: field.var_specifiers.clone(),
        specifiers: field.specifiers.clone(),
        annotation: field.annotation.clone(),
        default: field.default.as_ref().map(desugar_expr),
        mutable: field.mutable,
        span: field.span,
    }
}

fn desugar_field_attribute(attribute: &FieldAttribute) -> FieldAttribute {
    FieldAttribute {
        name: attribute.name.clone(),
        arguments: attribute
            .arguments
            .iter()
            .map(desugar_attribute_argument)
            .collect(),
        span: attribute.span,
    }
}

fn desugar_attribute_argument(argument: &AttributeArgument) -> AttributeArgument {
    AttributeArgument {
        name: argument.name.clone(),
        expr: desugar_expr(&argument.expr),
        span: argument.span,
    }
}

fn desugar_class_method(method: &ClassMethod) -> ClassMethod {
    ClassMethod {
        qualifier: method.qualifier.clone(),
        name: method.name.clone(),
        params: method.params.iter().map(desugar_param).collect(),
        effects: method.effects.clone(),
        return_type: method.return_type.clone(),
        body: method.body.as_ref().map(desugar_expr),
        span: method.span,
    }
}

fn desugar_extension_method(method: &ExtensionMethod) -> ExtensionMethod {
    ExtensionMethod {
        receiver: desugar_param(&method.receiver),
        method: desugar_class_method(&method.method),
        span: method.span,
    }
}

fn desugar_class_block(block: &ClassBlock) -> ClassBlock {
    ClassBlock {
        body: desugar_expr(&block.body),
        span: block.span,
    }
}

fn desugar_archetype_entry(entry: &ArchetypeEntry) -> ArchetypeEntry {
    match entry {
        ArchetypeEntry::Field(field) => ArchetypeEntry::Field(ArchetypeField {
            name: field.name.clone(),
            expr: desugar_expr(&field.expr),
            span: field.span,
        }),
        ArchetypeEntry::Let(binding) => ArchetypeEntry::Let(ArchetypeLet {
            name: binding.name.clone(),
            annotation: binding.annotation.clone(),
            expr: desugar_expr(&binding.expr),
            span: binding.span,
        }),
        ArchetypeEntry::Block(body) => ArchetypeEntry::Block(desugar_expr(body)),
        ArchetypeEntry::ConstructorCall(call) => {
            ArchetypeEntry::ConstructorCall(ArchetypeConstructorCall {
                name: call.name.clone(),
                args: call.args.iter().map(desugar_call_arg).collect(),
                span: call.span,
            })
        }
    }
}

fn desugar_case_arm(arm: &CaseArm) -> CaseArm {
    let pattern = match &arm.pattern {
        CasePattern::Wildcard { span } => CasePattern::Wildcard { span: *span },
        CasePattern::Expr(expr) => CasePattern::Expr(Box::new(desugar_expr(expr))),
    };
    CaseArm {
        ignore_unreachable: arm.ignore_unreachable,
        pattern,
        expr: desugar_expr(&arm.expr),
        span: arm.span,
    }
}
