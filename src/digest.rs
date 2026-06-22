use std::path::Path;

use crate::ast::{
    ClassMethod, ExprKind, ExtensionMethod, Param, Program, Stmt, StmtKind, StructField,
    TypeAnnotation, TypeName, TypeParam, TypeParamConstraint,
};
use crate::error::VerseError;
use crate::parser::parse_source;
use crate::project::load_project_own_source;

pub fn generate_digest(source: &str) -> Result<String, VerseError> {
    let program = parse_source(source)?;
    Ok(generate_digest_for_program(&program))
}

pub fn generate_project_digest(path: impl AsRef<Path>) -> Result<String, VerseError> {
    let source = load_project_own_source(path)?;
    generate_digest(&source)
}

pub fn generate_digest_for_program(program: &Program) -> String {
    let lines = render_digest_statements(&program.statements, 0);
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn render_digest_statements(statements: &[Stmt], indent: usize) -> Vec<String> {
    statements
        .iter()
        .filter_map(|statement| render_digest_statement(statement, indent))
        .collect()
}

fn render_digest_statement(statement: &Stmt, indent: usize) -> Option<String> {
    match &statement.kind {
        StmtKind::Let {
            name,
            specifiers,
            annotation,
            expr,
        } => render_public_let(name, specifiers, annotation.as_ref(), expr, indent),
        StmtKind::ParametricType {
            name,
            specifiers,
            params,
            expr,
        } if has_public_specifier(specifiers) => {
            render_public_parametric_type(name, specifiers, params, expr, indent)
        }
        StmtKind::ExtensionMethod(method) if has_public_specifier(&method.method.effects) => {
            Some(render_extension_method(method, indent))
        }
        _ => None,
    }
}

fn render_public_let(
    name: &str,
    specifiers: &[String],
    annotation: Option<&TypeAnnotation>,
    expr: &crate::ast::Expr,
    indent: usize,
) -> Option<String> {
    match &expr.kind {
        ExprKind::Function {
            params,
            effects,
            return_type,
            ..
        } if has_public_specifier(specifiers) || has_public_specifier(effects) => {
            Some(render_function_binding(
                name,
                specifiers,
                params,
                effects,
                return_type.as_ref(),
                indent,
            ))
        }
        ExprKind::ModuleDefinition { statements, .. } if has_public_specifier(specifiers) => {
            Some(render_module_binding(name, specifiers, statements, indent))
        }
        ExprKind::StructDefinition { fields, .. } if has_public_specifier(specifiers) => {
            Some(render_struct_binding(name, specifiers, fields, indent))
        }
        ExprKind::ClassDefinition {
            specifiers: class_specifiers,
            fields,
            methods,
            ..
        } if has_public_specifier(specifiers) || has_public_specifier(class_specifiers) => Some(
            render_class_binding(name, specifiers, class_specifiers, fields, methods, indent),
        ),
        ExprKind::InterfaceDefinition {
            parents,
            fields,
            methods,
            ..
        } if has_public_specifier(specifiers) => Some(render_interface_binding(
            name, specifiers, parents, fields, methods, indent,
        )),
        _ if has_public_specifier(specifiers) => annotation.map(|annotation| {
            format!(
                "{}{}{}:{} = external {{}}",
                indent_text(indent),
                name,
                render_specifiers(specifiers),
                render_type_annotation(annotation)
            )
        }),
        _ => None,
    }
}

fn render_public_parametric_type(
    name: &str,
    specifiers: &[String],
    params: &[TypeParam],
    expr: &crate::ast::Expr,
    indent: usize,
) -> Option<String> {
    let params = render_type_params(params);
    match &expr.kind {
        ExprKind::StructDefinition { fields, .. } => {
            let body = render_public_fields(fields, indent + 1, false);
            Some(render_parametric_aggregate_binding(
                name, specifiers, &params, "struct", "", body, indent,
            ))
        }
        ExprKind::ClassDefinition {
            specifiers: class_specifiers,
            fields,
            methods,
            ..
        } => {
            let mut body = render_public_fields(fields, indent + 1, false);
            body.extend(render_public_methods(methods, indent + 1));
            Some(render_parametric_aggregate_binding(
                name,
                specifiers,
                &params,
                "class",
                &render_specifiers(class_specifiers),
                body,
                indent,
            ))
        }
        ExprKind::InterfaceDefinition {
            parents,
            fields,
            methods,
            ..
        } => {
            let mut body = render_public_fields(fields, indent + 1, true);
            body.extend(render_public_methods(methods, indent + 1));
            let parents = render_parents(parents);
            Some(render_parametric_aggregate_binding(
                name,
                specifiers,
                &params,
                "interface",
                &parents,
                body,
                indent,
            ))
        }
        _ => None,
    }
}

fn render_parametric_aggregate_binding(
    name: &str,
    specifiers: &[String],
    params: &str,
    keyword: &str,
    suffix: &str,
    body: Vec<String>,
    indent: usize,
) -> String {
    if body.is_empty() {
        format!(
            "{}{}{}{} := {}{} {{}}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            params,
            keyword,
            suffix
        )
    } else {
        format!(
            "{}{}{}{} := {}{}:\n{}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            params,
            keyword,
            suffix,
            body.join("\n")
        )
    }
}

fn render_module_binding(
    name: &str,
    specifiers: &[String],
    statements: &[Stmt],
    indent: usize,
) -> String {
    let body = render_digest_statements(statements, indent + 1);
    if body.is_empty() {
        format!(
            "{}{}{} := module {{}}",
            indent_text(indent),
            name,
            render_specifiers(specifiers)
        )
    } else {
        format!(
            "{}{}{} := module:\n{}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            body.join("\n")
        )
    }
}

fn render_struct_binding(
    name: &str,
    specifiers: &[String],
    fields: &[StructField],
    indent: usize,
) -> String {
    let fields = render_public_fields(fields, indent + 1, false);
    if fields.is_empty() {
        format!(
            "{}{}{} := struct {{}}",
            indent_text(indent),
            name,
            render_specifiers(specifiers)
        )
    } else {
        format!(
            "{}{}{} := struct:\n{}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            fields.join("\n")
        )
    }
}

fn render_class_binding(
    name: &str,
    specifiers: &[String],
    class_specifiers: &[String],
    fields: &[StructField],
    methods: &[ClassMethod],
    indent: usize,
) -> String {
    let mut body = render_public_fields(fields, indent + 1, false);
    body.extend(render_public_methods(methods, indent + 1));
    if body.is_empty() {
        format!(
            "{}{}{} := class{} {{}}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            render_specifiers(class_specifiers)
        )
    } else {
        format!(
            "{}{}{} := class{}:\n{}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            render_specifiers(class_specifiers),
            body.join("\n")
        )
    }
}

fn render_interface_binding(
    name: &str,
    specifiers: &[String],
    parents: &[TypeAnnotation],
    fields: &[StructField],
    methods: &[ClassMethod],
    indent: usize,
) -> String {
    let mut body = render_public_fields(fields, indent + 1, true);
    body.extend(render_public_methods(methods, indent + 1));
    let parents = render_parents(parents);
    if body.is_empty() {
        format!(
            "{}{}{} := interface{} {{}}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            parents
        )
    } else {
        format!(
            "{}{}{} := interface{}:\n{}",
            indent_text(indent),
            name,
            render_specifiers(specifiers),
            parents,
            body.join("\n")
        )
    }
}

fn render_public_fields(
    fields: &[StructField],
    indent: usize,
    interface_field: bool,
) -> Vec<String> {
    fields
        .iter()
        .filter(|field| has_public_specifier(&field.specifiers))
        .filter_map(|field| {
            let annotation = field.annotation.as_ref()?;
            let prefix = if field.mutable { "var " } else { "" };
            let signature = format!(
                "{}{}{}{}:{}",
                indent_text(indent),
                prefix,
                field.name,
                render_specifiers(&field.specifiers),
                render_type_annotation(annotation)
            );
            if interface_field {
                Some(signature)
            } else {
                Some(format!("{signature} = external {{}}"))
            }
        })
        .collect()
}

fn render_public_methods(methods: &[ClassMethod], indent: usize) -> Vec<String> {
    methods
        .iter()
        .filter(|method| has_public_specifier(&method.effects))
        .map(|method| render_method(method, indent))
        .collect()
}

fn render_function_binding(
    name: &str,
    specifiers: &[String],
    params: &[Param],
    effects: &[String],
    return_type: Option<&TypeAnnotation>,
    indent: usize,
) -> String {
    let mut name_specifiers = specifiers.to_vec();
    name_specifiers.extend(
        effects
            .iter()
            .filter(|effect| !is_effect_specifier(effect))
            .cloned(),
    );
    format!(
        "{}{}{}({}){}:{} = external {{}}",
        indent_text(indent),
        name,
        render_name_specifiers(&name_specifiers),
        render_params(params),
        render_effect_specifiers(effects),
        render_optional_return_type(return_type)
    )
}

fn render_method(method: &ClassMethod, indent: usize) -> String {
    format!(
        "{}{}{}({}){}:{} = external {{}}",
        indent_text(indent),
        method.name,
        render_name_specifiers(&method.effects),
        render_params(&method.params),
        render_effect_specifiers(&method.effects),
        render_optional_return_type(method.return_type.as_ref())
    )
}

fn render_extension_method(method: &ExtensionMethod, indent: usize) -> String {
    format!(
        "{}({}:{}).{}{}({}){}:{} = external {{}}",
        indent_text(indent),
        method.receiver.name,
        method
            .receiver
            .annotation
            .as_ref()
            .map(render_type_annotation)
            .unwrap_or_else(|| "any".to_string()),
        method.method.name,
        render_name_specifiers(&method.method.effects),
        render_params(&method.method.params),
        render_effect_specifiers(&method.method.effects),
        render_optional_return_type(method.method.return_type.as_ref())
    )
}

fn render_params(params: &[Param]) -> String {
    params
        .iter()
        .map(render_param)
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_param(param: &Param) -> String {
    let name = if param.named {
        format!("?{}", param.name)
    } else {
        param.name.clone()
    };
    let annotation = param
        .annotation
        .as_ref()
        .map(render_type_annotation)
        .unwrap_or_else(|| "any".to_string());
    format!("{name}:{annotation}")
}

fn render_type_params(params: &[TypeParam]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            params
                .iter()
                .map(render_type_param)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn render_type_param(param: &TypeParam) -> String {
    match &param.constraint {
        TypeParamConstraint::Type => format!("{}:type", param.name),
        TypeParamConstraint::Subtype(target) => {
            format!("{}:subtype({})", param.name, render_type_name(target))
        }
        TypeParamConstraint::TypeBounds { lower, upper } => format!(
            "{}:type({}, {})",
            param.name,
            render_type_name(lower),
            render_type_name(upper)
        ),
    }
}

fn render_parents(parents: &[TypeAnnotation]) -> String {
    if parents.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            parents
                .iter()
                .map(render_type_annotation)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn render_optional_return_type(return_type: Option<&TypeAnnotation>) -> String {
    return_type
        .map(render_type_annotation)
        .unwrap_or_else(|| "any".to_string())
}

fn render_type_annotation(annotation: &TypeAnnotation) -> String {
    render_type_name(&annotation.name)
}

fn render_type_name(name: &TypeName) -> String {
    match name {
        TypeName::Int => "int".to_string(),
        TypeName::Float => "float".to_string(),
        TypeName::Rational => "rational".to_string(),
        TypeName::Number => "number".to_string(),
        TypeName::Bool => "logic".to_string(),
        TypeName::String => "string".to_string(),
        TypeName::Message => "message".to_string(),
        TypeName::Char => "char".to_string(),
        TypeName::Char8 => "char8".to_string(),
        TypeName::Char32 => "char32".to_string(),
        TypeName::None => "void".to_string(),
        TypeName::Any => "any".to_string(),
        TypeName::Comparable => "comparable".to_string(),
        TypeName::Type => "type".to_string(),
        TypeName::TypeBounds { lower, upper } => {
            format!(
                "type({}, {})",
                render_type_name(lower),
                render_type_name(upper)
            )
        }
        TypeName::IntRange { min, max } => format!("int_range({min}, {max})"),
        TypeName::FloatRange(range) => {
            format!(
                "float_range({}, {})",
                range.min.render(),
                range.max.render()
            )
        }
        TypeName::Array(Some(item)) => format!("[]{}", render_type_name(item)),
        TypeName::Array(None) => "[]any".to_string(),
        TypeName::Map(key, value) => {
            format!("[{}]{}", render_type_name(key), render_type_name(value))
        }
        TypeName::WeakMap(key, value) => {
            format!(
                "weak_map({}, {})",
                render_type_name(key),
                render_type_name(value)
            )
        }
        TypeName::Tuple(items) => {
            let items = items
                .iter()
                .map(render_type_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("tuple({items})")
        }
        TypeName::Option(item) => format!("?{}", render_type_name(item)),
        TypeName::Function => "type{_():any}".to_string(),
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => format!(
            "type{{_({}){}:{}}}",
            params
                .iter()
                .map(|param| format!(":{}", render_type_name(param)))
                .collect::<Vec<_>>()
                .join(", "),
            render_specifiers(effects),
            render_type_name(return_type)
        ),
        TypeName::Applied { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}({})",
                    name,
                    args.iter()
                        .map(render_type_name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        TypeName::Named(name) => name.clone(),
    }
}

fn render_specifiers(specifiers: &[String]) -> String {
    specifiers
        .iter()
        .map(|specifier| format!("<{specifier}>"))
        .collect::<String>()
}

fn render_name_specifiers(specifiers: &[String]) -> String {
    specifiers
        .iter()
        .filter(|specifier| !is_effect_specifier(specifier))
        .map(|specifier| format!("<{specifier}>"))
        .collect::<String>()
}

fn render_effect_specifiers(specifiers: &[String]) -> String {
    specifiers
        .iter()
        .filter(|specifier| is_effect_specifier(specifier))
        .map(|specifier| format!("<{specifier}>"))
        .collect::<String>()
}

fn is_effect_specifier(specifier: &str) -> bool {
    matches!(
        specifier,
        "converges"
            | "computes"
            | "varies"
            | "transacts"
            | "suspends"
            | "decides"
            | "reads"
            | "writes"
            | "allocates"
    )
}

fn has_public_specifier(specifiers: &[String]) -> bool {
    specifiers.iter().any(|specifier| specifier == "public")
}

fn indent_text(indent: usize) -> String {
    "    ".repeat(indent)
}
