use proc_macro::{Delimiter, Group, Literal, Punct, TokenStream, TokenTree};
use std::collections::{HashMap, HashSet};

use verse_rs::ast::{
    EnumVariant, ExprKind, Param, ParamPattern, Program, StmtKind, StructField, TypeName,
};

#[proc_macro]
pub fn native_api(input: TokenStream) -> TokenStream {
    match expand_native_api(input) {
        Ok(output) => output.parse().expect("generated native API should parse"),
        Err(error) => compile_error(&error),
    }
}

fn expand_native_api(input: TokenStream) -> Result<String, String> {
    let config = MacroConfig::parse(input)?;
    let components = module_components_from_path(&config.path)?;
    let runtime_prefix = components.join(".");
    let program = verse_rs::parse_source(&config.source)
        .map_err(|error| format!("failed to parse native_api! source: {error}"))?;
    let api = collect_native_api(&program, &runtime_prefix)?;
    if api.functions.is_empty() {
        return Err("native_api! source did not contain any native function declarations".into());
    }
    ensure_unique_methods(&api.functions)?;
    let digest = render_digest(&components, &api);
    Ok(render_module(&config, &api, &digest))
}

struct MacroConfig {
    visibility: String,
    module_name: String,
    trait_name: String,
    path: String,
    source: String,
}

impl MacroConfig {
    fn parse(input: TokenStream) -> Result<Self, String> {
        let mut parser = TokenParser::new(input);
        let visibility = if parser.peek_ident("pub") {
            parser.expect_ident("pub")?;
            "pub ".to_string()
        } else {
            String::new()
        };
        parser.expect_ident("mod")?;
        let module_name = parser.expect_any_ident("expected generated module name after `mod`")?;
        parser.expect_punct(';')?;
        parser.expect_ident("trait")?;
        let trait_name = parser.expect_any_ident("expected trait name after `trait`")?;
        parser.expect_punct(';')?;
        parser.expect_ident("path")?;
        let path = parser.expect_string_literal("expected string literal after `path`")?;
        parser.expect_punct(';')?;

        let source = if parser.peek_ident("source") {
            parser.expect_ident("source")?;
            let source = parser.expect_string_literal("expected string literal after `source`")?;
            parser.expect_punct(';')?;
            source
        } else if parser.peek_ident("file") {
            parser.expect_ident("file")?;
            let file = parser.expect_string_literal("expected string literal after `file`")?;
            parser.expect_punct(';')?;
            read_source_file(&file)?
        } else {
            return Err("expected `source \"...\";` or `file \"...\";`".into());
        };

        parser.expect_end()?;
        Ok(Self {
            visibility,
            module_name,
            trait_name,
            path,
            source,
        })
    }
}

struct TokenParser {
    tokens: Vec<TokenTree>,
    index: usize,
}

impl TokenParser {
    fn new(input: TokenStream) -> Self {
        Self {
            tokens: input.into_iter().collect(),
            index: 0,
        }
    }

    fn peek_ident(&self, expected: &str) -> bool {
        matches!(self.tokens.get(self.index), Some(TokenTree::Ident(ident)) if ident.to_string() == expected)
    }

    fn expect_ident(&mut self, expected: &str) -> Result<(), String> {
        match self.tokens.get(self.index) {
            Some(TokenTree::Ident(ident)) if ident.to_string() == expected => {
                self.index += 1;
                Ok(())
            }
            _ => Err(format!("expected `{expected}`")),
        }
    }

    fn expect_any_ident(&mut self, message: &str) -> Result<String, String> {
        match self.tokens.get(self.index) {
            Some(TokenTree::Ident(ident)) => {
                self.index += 1;
                Ok(ident.to_string())
            }
            _ => Err(message.to_string()),
        }
    }

    fn expect_punct(&mut self, expected: char) -> Result<(), String> {
        match self.tokens.get(self.index) {
            Some(TokenTree::Punct(punct)) if punct.as_char() == expected => {
                self.index += 1;
                Ok(())
            }
            _ => Err(format!("expected `{expected}`")),
        }
    }

    fn expect_string_literal(&mut self, message: &str) -> Result<String, String> {
        match self.tokens.get(self.index) {
            Some(TokenTree::Literal(literal)) => {
                self.index += 1;
                literal_to_string(literal)
            }
            _ => Err(message.to_string()),
        }
    }

    fn expect_end(&self) -> Result<(), String> {
        if self.index == self.tokens.len() {
            Ok(())
        } else {
            Err("unexpected tokens at end of native_api! input".into())
        }
    }
}

fn literal_to_string(literal: &Literal) -> Result<String, String> {
    let text = literal.to_string();
    if text.starts_with('r') {
        let start = text
            .find('"')
            .ok_or_else(|| "expected raw string literal".to_string())?;
        let end = text
            .rfind('"')
            .ok_or_else(|| "expected raw string literal".to_string())?;
        return Ok(text[start + 1..end].to_string());
    }
    if !text.starts_with('"') || !text.ends_with('"') {
        return Err("expected string literal".into());
    }
    unescape_string_literal(&text[1..text.len() - 1])
}

fn unescape_string_literal(text: &str) -> Result<String, String> {
    let mut output = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| "unterminated escape in string literal".to_string())?;
        match escaped {
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '\\' => output.push('\\'),
            '"' => output.push('"'),
            other => {
                return Err(format!(
                    "unsupported escape `\\{other}` in native_api! string literal"
                ));
            }
        }
    }
    Ok(output)
}

fn read_source_file(file: &str) -> Result<String, String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| "CARGO_MANIFEST_DIR is not set".to_string())?;
    let path = std::path::Path::new(&manifest_dir).join(file);
    std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))
}

struct NativeApi {
    types: Vec<NativeTypeDefinition>,
    functions: Vec<NativeFunction>,
}

#[derive(Clone)]
enum NativeTypeDefinition {
    Struct(NativeStruct),
    Enum(NativeEnum),
}

#[derive(Clone)]
struct NativeStruct {
    source_name: String,
    rust_name: String,
    runtime_name: String,
    binding_specifiers: Vec<String>,
    struct_specifiers: Vec<String>,
    fields: Vec<NativeField>,
    computes: bool,
}

#[derive(Clone)]
struct NativeEnum {
    source_name: String,
    rust_name: String,
    runtime_name: String,
    binding_specifiers: Vec<String>,
    enum_specifiers: Vec<String>,
    variants: Vec<NativeEnumVariant>,
}

#[derive(Clone)]
struct NativeEnumVariant {
    source_name: String,
    rust_name: String,
}

#[derive(Clone)]
struct NativeField {
    source_name: String,
    rust_name: String,
    specifiers: Vec<String>,
    type_name: String,
    rust_type: String,
}

#[derive(Clone)]
struct NativeFunction {
    name: String,
    rust_name: String,
    runtime_name: String,
    specifiers: Vec<String>,
    effects: Vec<String>,
    params: Vec<NativeParam>,
    return_type: String,
    rust_return_type: String,
}

#[derive(Clone)]
struct NativeParam {
    source_name: String,
    rust_name: String,
    type_name: String,
    rust_type: String,
    named: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeNamedTypeKind {
    Struct,
    Enum,
}

#[derive(Clone)]
struct NativeNamedType {
    rust_name: String,
    kind: NativeNamedTypeKind,
}

struct RustTypeInfo {
    code: String,
    hashable: bool,
}

fn collect_native_api(program: &Program, runtime_prefix: &str) -> Result<NativeApi, String> {
    let type_env = collect_native_type_names(program)?;
    let mut types = Vec::new();
    let mut functions = Vec::new();

    for statement in &program.statements {
        match &statement.kind {
            StmtKind::Using { .. } | StmtKind::ScopedAccessLevel { .. } => {}
            StmtKind::Let {
                name,
                specifiers,
                expr,
                ..
            } => match &expr.kind {
                ExprKind::StructDefinition {
                    persistable,
                    computes,
                    fields,
                    ..
                } if has_native(specifiers) => {
                    types.push(NativeTypeDefinition::Struct(resolve_native_struct(
                        name,
                        specifiers,
                        *persistable,
                        *computes,
                        fields,
                        runtime_prefix,
                        &type_env,
                    )?));
                }
                ExprKind::EnumDefinition {
                    open,
                    persistable,
                    variants,
                    ..
                } if has_native(specifiers) => {
                    types.push(NativeTypeDefinition::Enum(resolve_native_enum(
                        name,
                        specifiers,
                        *open,
                        *persistable,
                        variants,
                        runtime_prefix,
                        &type_env,
                    )?));
                }
                ExprKind::Function {
                    params,
                    effects,
                    return_type,
                    ..
                } if has_native(effects) => {
                    functions.push(resolve_native_function(
                        name,
                        params,
                        effects,
                        return_type.as_ref().map(|annotation| &annotation.name),
                        runtime_prefix,
                        &type_env,
                    )?);
                }
                _ if has_native(specifiers) => {
                    return Err(format!(
                        "native_api! only supports native struct, enum, and top-level function declarations; unsupported native binding `{name}`"
                    ));
                }
                _ => {
                    return Err(
                        "native_api! source may only contain `using`, scoped access definitions, and native declarations"
                            .into(),
                    );
                }
            },
            _ => {
                return Err(
                    "native_api! source may only contain `using`, scoped access definitions, and native declarations"
                        .into(),
                );
            }
        }
    }

    Ok(NativeApi { types, functions })
}

fn collect_native_type_names(
    program: &Program,
) -> Result<HashMap<String, NativeNamedType>, String> {
    let mut types = HashMap::new();
    let mut rust_names = HashSet::new();

    for statement in &program.statements {
        let StmtKind::Let {
            name,
            specifiers,
            expr,
            ..
        } = &statement.kind
        else {
            continue;
        };
        let kind = match &expr.kind {
            ExprKind::StructDefinition { .. } if has_native(specifiers) => {
                NativeNamedTypeKind::Struct
            }
            ExprKind::EnumDefinition { .. } if has_native(specifiers) => NativeNamedTypeKind::Enum,
            _ => continue,
        };
        let rust_name = pascal_case(name);
        if types
            .insert(
                name.clone(),
                NativeNamedType {
                    rust_name: rust_name.clone(),
                    kind,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate native type `{name}`"));
        }
        if !rust_names.insert(rust_name.clone()) {
            return Err(format!(
                "multiple native types generate the same Rust type `{rust_name}`"
            ));
        }
    }

    Ok(types)
}

fn resolve_native_struct(
    name: &str,
    binding_specifiers: &[String],
    persistable: bool,
    computes: bool,
    fields: &[StructField],
    runtime_prefix: &str,
    type_env: &HashMap<String, NativeNamedType>,
) -> Result<NativeStruct, String> {
    let mut resolved_fields = Vec::new();
    let mut rust_field_names = HashSet::new();

    for field in fields {
        if field.mutable {
            return Err(format!(
                "native_api! does not support mutable fields yet (`{name}.{}`)",
                field.name
            ));
        }
        let annotation = field.annotation.as_ref().ok_or_else(|| {
            format!(
                "field `{}` in native struct `{name}` is missing a type",
                field.name
            )
        })?;
        let rust_type = rust_type_for(&annotation.name, type_env)
            .map_err(|error| format!("field `{}` in `{name}`: {error}", field.name))?
            .code;
        let rust_name = snake_case(&field.name);
        if !rust_field_names.insert(rust_name.clone()) {
            return Err(format!(
                "native struct `{name}` has multiple fields that generate Rust field `{rust_name}`"
            ));
        }
        resolved_fields.push(NativeField {
            source_name: field.name.clone(),
            rust_name,
            specifiers: without_native(&field.specifiers),
            type_name: render_type_name(&annotation.name),
            rust_type,
        });
    }

    let rust_name = type_env
        .get(name)
        .expect("native type predeclared")
        .rust_name
        .clone();
    let mut struct_specifiers = Vec::new();
    if persistable {
        struct_specifiers.push("persistable".to_string());
    }
    if computes {
        struct_specifiers.push("computes".to_string());
    }

    Ok(NativeStruct {
        runtime_name: format!("{runtime_prefix}.{name}"),
        source_name: name.to_string(),
        rust_name,
        binding_specifiers: without_native(binding_specifiers),
        struct_specifiers,
        fields: resolved_fields,
        computes,
    })
}

fn resolve_native_enum(
    name: &str,
    binding_specifiers: &[String],
    open: bool,
    persistable: bool,
    variants: &[EnumVariant],
    runtime_prefix: &str,
    type_env: &HashMap<String, NativeNamedType>,
) -> Result<NativeEnum, String> {
    if variants.is_empty() {
        return Err(format!(
            "native enum `{name}` must have at least one variant"
        ));
    }

    let mut resolved_variants = Vec::new();
    let mut rust_variant_names = HashSet::new();
    for variant in variants {
        let rust_name = pascal_case(&variant.name);
        if !rust_variant_names.insert(rust_name.clone()) {
            return Err(format!(
                "native enum `{name}` has multiple variants that generate Rust variant `{rust_name}`"
            ));
        }
        resolved_variants.push(NativeEnumVariant {
            source_name: variant.name.clone(),
            rust_name,
        });
    }

    let rust_name = type_env
        .get(name)
        .expect("native type predeclared")
        .rust_name
        .clone();
    let mut enum_specifiers = Vec::new();
    if open {
        enum_specifiers.push("open".to_string());
    }
    if persistable {
        enum_specifiers.push("persistable".to_string());
    }

    Ok(NativeEnum {
        runtime_name: format!("{runtime_prefix}.{name}"),
        source_name: name.to_string(),
        rust_name,
        binding_specifiers: without_native(binding_specifiers),
        enum_specifiers,
        variants: resolved_variants,
    })
}

fn resolve_native_function(
    name: &str,
    params: &[Param],
    effects: &[String],
    return_type: Option<&TypeName>,
    runtime_prefix: &str,
    type_env: &HashMap<String, NativeNamedType>,
) -> Result<NativeFunction, String> {
    let mut resolved_params = Vec::new();
    let mut rust_param_names = HashSet::new();

    for param in params {
        if !matches!(param.pattern, ParamPattern::Binding) {
            return Err(format!(
                "native_api! only supports binding parameters in native function `{name}`"
            ));
        }
        if !param.type_params.is_empty() {
            return Err(format!(
                "native_api! does not support parametric parameters in native function `{name}`"
            ));
        }
        if param.default.is_some() {
            return Err(format!(
                "native_api! does not support default parameter values in native function `{name}`"
            ));
        }
        let annotation = param.annotation.as_ref().ok_or_else(|| {
            format!(
                "parameter `{}` in native function `{name}` is missing a type",
                param.name
            )
        })?;
        let rust_type = rust_type_for(&annotation.name, type_env)
            .map_err(|error| format!("parameter `{}` in `{name}`: {error}", param.name))?
            .code;
        let rust_name = snake_case(&param.name);
        if !rust_param_names.insert(rust_name.clone()) {
            return Err(format!(
                "native function `{name}` has multiple parameters that generate Rust parameter `{rust_name}`"
            ));
        }
        resolved_params.push(NativeParam {
            source_name: param.name.clone(),
            rust_name,
            type_name: render_type_name(&annotation.name),
            rust_type,
            named: param.named,
        });
    }

    let return_type = return_type.cloned().unwrap_or(TypeName::Any);
    let rust_return_type = rust_type_for(&return_type, type_env)
        .map_err(|error| format!("return type in `{name}`: {error}"))?
        .code;
    let (specifiers, effects) = split_function_specifiers(effects);

    Ok(NativeFunction {
        name: name.to_string(),
        rust_name: snake_case(name),
        runtime_name: format!("{runtime_prefix}.{name}"),
        specifiers,
        effects,
        params: resolved_params,
        return_type: render_type_name(&return_type),
        rust_return_type,
    })
}

fn rust_type_for(
    type_name: &TypeName,
    type_env: &HashMap<String, NativeNamedType>,
) -> Result<RustTypeInfo, String> {
    match type_name {
        TypeName::None => Ok(RustTypeInfo {
            code: "()".into(),
            hashable: false,
        }),
        TypeName::Int | TypeName::IntRange { .. } => Ok(RustTypeInfo {
            code: "::verse_rs::native::NativeInt".into(),
            hashable: true,
        }),
        TypeName::Float | TypeName::FloatRange(_) => Ok(RustTypeInfo {
            code: "f64".into(),
            hashable: false,
        }),
        TypeName::Bool => Ok(RustTypeInfo {
            code: "bool".into(),
            hashable: true,
        }),
        TypeName::String | TypeName::Message => Ok(RustTypeInfo {
            code: "String".into(),
            hashable: true,
        }),
        TypeName::Any => Ok(RustTypeInfo {
            code: "::verse_rs::Value".into(),
            hashable: false,
        }),
        TypeName::Array(item) => {
            let item = item
                .as_deref()
                .map(|item| rust_type_for(item, type_env))
                .unwrap_or_else(|| {
                    Ok(RustTypeInfo {
                        code: "::verse_rs::Value".into(),
                        hashable: false,
                    })
                })?;
            Ok(RustTypeInfo {
                code: format!("::std::vec::Vec<{}>", item.code),
                hashable: false,
            })
        }
        TypeName::Map(key, value) => {
            let key_info = rust_type_for(key, type_env)?;
            if !key_info.hashable {
                return Err(format!(
                    "map key type `{}` is not supported by native_api! yet; use int, logic, string, message, option/tuple of hashable types, or a native enum",
                    render_type_name(key)
                ));
            }
            let value_info = rust_type_for(value, type_env)?;
            Ok(RustTypeInfo {
                code: format!(
                    "::std::collections::HashMap<{}, {}>",
                    key_info.code, value_info.code
                ),
                hashable: false,
            })
        }
        TypeName::Tuple(items) => {
            let items = items
                .iter()
                .map(|item| rust_type_for(item, type_env))
                .collect::<Result<Vec<_>, _>>()?;
            let hashable = items.iter().all(|item| item.hashable);
            let code = match items.as_slice() {
                [] => "()".to_string(),
                [single] => format!("({},)", single.code),
                _ => format!(
                    "({})",
                    items
                        .iter()
                        .map(|item| item.code.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            Ok(RustTypeInfo { code, hashable })
        }
        TypeName::Option(item) => {
            let item = rust_type_for(item, type_env)?;
            Ok(RustTypeInfo {
                code: format!("::std::option::Option<{}>", item.code),
                hashable: item.hashable,
            })
        }
        TypeName::Named(name) => {
            let native_type = lookup_native_type(name, type_env)
                .ok_or_else(|| format!("type `{name}` is not supported by native_api! yet"))?;
            Ok(RustTypeInfo {
                code: native_type.rust_name.clone(),
                hashable: native_type.kind == NativeNamedTypeKind::Enum,
            })
        }
        unsupported => Err(format!(
            "type `{}` is not supported by native_api! yet",
            render_type_name(unsupported)
        )),
    }
}

fn lookup_native_type<'a>(
    name: &str,
    type_env: &'a HashMap<String, NativeNamedType>,
) -> Option<&'a NativeNamedType> {
    type_env.get(name).or_else(|| {
        name.rsplit_once('.')
            .and_then(|(_, local)| type_env.get(local))
    })
}

fn has_native(specifiers: &[String]) -> bool {
    specifiers.iter().any(|specifier| specifier == "native")
}

fn without_native(specifiers: &[String]) -> Vec<String> {
    specifiers
        .iter()
        .filter(|specifier| specifier.as_str() != "native")
        .cloned()
        .collect()
}

fn split_function_specifiers(specifiers: &[String]) -> (Vec<String>, Vec<String>) {
    let mut name_specifiers = Vec::new();
    let mut effect_specifiers = Vec::new();
    for specifier in specifiers {
        if specifier == "native" {
            continue;
        }
        if is_effect_specifier(specifier) {
            effect_specifiers.push(specifier.clone());
        } else {
            name_specifiers.push(specifier.clone());
        }
    }
    (name_specifiers, effect_specifiers)
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
            | "predicts"
    )
}

fn ensure_unique_methods(functions: &[NativeFunction]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for function in functions {
        if !seen.insert(function.rust_name.clone()) {
            return Err(format!(
                "native_api! does not support overloaded Rust method `{}` yet",
                function.rust_name
            ));
        }
    }
    Ok(())
}

fn module_components_from_path(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err("native_api! path must be absolute, for example `/Game.com/Host`".into());
    }
    let mut components = path
        .trim_start_matches('/')
        .split('/')
        .filter(|component| !component.is_empty());
    components
        .next()
        .ok_or_else(|| "native_api! path is missing a domain component".to_string())?;
    let modules = components.map(str::to_string).collect::<Vec<_>>();
    if modules.is_empty() {
        return Err("native_api! path must include a module after the domain".into());
    }
    Ok(modules)
}

fn render_digest(components: &[String], api: &NativeApi) -> String {
    let mut lines = Vec::new();
    for (index, component) in components.iter().enumerate() {
        lines.push(format!(
            "{}{}<public> := module:",
            "    ".repeat(index),
            component
        ));
    }
    let indent = "    ".repeat(components.len());
    for type_definition in &api.types {
        render_type_digest(type_definition, &indent, &mut lines);
    }
    for function in &api.functions {
        lines.push(format!(
            "{}{}{}({}){}:{} = external {{}}",
            indent,
            function.name,
            render_specifiers(&function.specifiers),
            render_params(&function.params),
            render_specifiers(&function.effects),
            function.return_type
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_type_digest(definition: &NativeTypeDefinition, indent: &str, lines: &mut Vec<String>) {
    match definition {
        NativeTypeDefinition::Struct(definition) => {
            if definition.fields.is_empty() {
                lines.push(format!(
                    "{}{}{} := struct{} {{}}",
                    indent,
                    definition.source_name,
                    render_specifiers(&definition.binding_specifiers),
                    render_specifiers(&definition.struct_specifiers)
                ));
                return;
            }
            lines.push(format!(
                "{}{}{} := struct{}:",
                indent,
                definition.source_name,
                render_specifiers(&definition.binding_specifiers),
                render_specifiers(&definition.struct_specifiers)
            ));
            for field in &definition.fields {
                lines.push(format!(
                    "{}    {}{}:{}",
                    indent,
                    field.source_name,
                    render_specifiers(&field.specifiers),
                    field.type_name
                ));
            }
        }
        NativeTypeDefinition::Enum(definition) => {
            lines.push(format!(
                "{}{}{} := enum{}:",
                indent,
                definition.source_name,
                render_specifiers(&definition.binding_specifiers),
                render_specifiers(&definition.enum_specifiers)
            ));
            for variant in &definition.variants {
                lines.push(format!("{}    {}", indent, variant.source_name));
            }
        }
    }
}

fn render_specifiers(specifiers: &[String]) -> String {
    specifiers
        .iter()
        .map(|specifier| format!("<{specifier}>"))
        .collect::<String>()
}

fn render_params(params: &[NativeParam]) -> String {
    params
        .iter()
        .map(|param| {
            let prefix = if param.named { "?" } else { "" };
            format!("{prefix}{}:{}", param.source_name, param.type_name)
        })
        .collect::<Vec<_>>()
        .join(", ")
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

fn render_module(config: &MacroConfig, api: &NativeApi, digest: &str) -> String {
    let type_definitions = api
        .types
        .iter()
        .map(render_type_definition)
        .collect::<Vec<_>>()
        .join("\n\n");
    let trait_methods = api
        .functions
        .iter()
        .map(render_trait_method)
        .collect::<Vec<_>>()
        .join("\n\n");
    let registrations = api
        .functions
        .iter()
        .map(render_registration)
        .collect::<Vec<_>>()
        .join("\n");
    let signatures = api
        .functions
        .iter()
        .map(render_signature)
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"{visibility}mod {module_name} {{
    use std::sync::Arc;

{type_definitions}

    pub trait {trait_name}: Send + Sync + 'static {{
{trait_methods}
    }}

    pub const PATH: &str = {path:?};
    pub const DIGEST: &str = {digest:?};

    pub const FUNCTIONS: &[::verse_rs::native::NativeFunctionSignature] = &[
{signatures}
    ];

    pub fn bind<T: {trait_name}>(api: T) -> ::verse_rs::native::InjectedNativeApi {{
        let api = Arc::new(api);
        let mut registry = ::verse_rs::native::NativeRegistry::builder();
{registrations}
        ::verse_rs::native::InjectedNativeApi::new(PATH, DIGEST, FUNCTIONS, registry.build())
    }}
}}"#,
        visibility = config.visibility,
        module_name = config.module_name,
        trait_name = config.trait_name,
        type_definitions = indent_block(&type_definitions, 1),
        trait_methods = indent_block(&trait_methods, 2),
        path = config.path,
        digest = digest,
        signatures = indent_block(&signatures, 2),
        registrations = indent_block(&registrations, 2),
    )
}

fn render_type_definition(definition: &NativeTypeDefinition) -> String {
    match definition {
        NativeTypeDefinition::Struct(definition) => render_struct_definition(definition),
        NativeTypeDefinition::Enum(definition) => render_enum_definition(definition),
    }
}

fn render_struct_definition(definition: &NativeStruct) -> String {
    let fields = definition
        .fields
        .iter()
        .map(|field| format!("pub {}: {},", field.rust_name, field.rust_type))
        .collect::<Vec<_>>()
        .join("\n");
    let from_fields = definition
        .fields
        .iter()
        .map(|field| {
            format!(
                r#"let {rust_name} = <{rust_type} as ::verse_rs::native::FromNativeValue>::from_native_value(
            ::verse_rs::native::take_native_struct_field(&mut __fields, {source_name:?}, Self::RUNTIME_NAME)?,
            {source_name:?},
        )?;"#,
                rust_name = field.rust_name,
                rust_type = field.rust_type,
                source_name = field.source_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let init_fields = definition
        .fields
        .iter()
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let value_fields = definition
        .fields
        .iter()
        .map(|field| {
            format!(
                r#"({source_name:?}.to_string(), <{rust_type} as ::verse_rs::native::IntoNativeValue>::into_native_value(self.{rust_name}))"#,
                source_name = field.source_name,
                rust_type = field.rust_type,
                rust_name = field.rust_name
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"#[derive(Debug, Clone, PartialEq)]
pub struct {rust_name} {{
{fields}
}}

impl {rust_name} {{
    pub const VERSE_NAME: &'static str = {source_name:?};
    pub const RUNTIME_NAME: &'static str = {runtime_name:?};
}}

impl ::verse_rs::native::FromNativeValue for {rust_name} {{
    fn from_native_value(value: ::verse_rs::Value, name: &str) -> ::verse_rs::native::NativeResult<Self> {{
        let mut __fields = ::verse_rs::native::native_struct_fields(value, Self::RUNTIME_NAME, name)?;
{from_fields}
        Ok(Self {{ {init_fields} }})
    }}
}}

impl ::verse_rs::native::IntoNativeValue for {rust_name} {{
    fn into_native_value(self) -> ::verse_rs::Value {{
        ::verse_rs::Value::StructInstance {{
            struct_name: Self::RUNTIME_NAME.to_string(),
            computes: {computes},
            fields: vec![
{value_fields}
            ],
        }}
    }}
}}"#,
        rust_name = definition.rust_name,
        fields = indent_block(&fields, 1),
        source_name = definition.source_name,
        runtime_name = definition.runtime_name,
        from_fields = indent_block(&from_fields, 2),
        init_fields = init_fields,
        computes = definition.computes,
        value_fields = indent_block(&value_fields, 4),
    )
}

fn render_enum_definition(definition: &NativeEnum) -> String {
    let variants = definition
        .variants
        .iter()
        .map(|variant| format!("{},", variant.rust_name))
        .collect::<Vec<_>>()
        .join("\n");
    let from_arms = definition
        .variants
        .iter()
        .map(|variant| {
            format!(
                "{:?} => Ok(Self::{}),",
                variant.source_name, variant.rust_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let into_arms = definition
        .variants
        .iter()
        .map(|variant| format!("Self::{} => {:?},", variant.rust_name, variant.source_name))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum {rust_name} {{
{variants}
}}

impl {rust_name} {{
    pub const VERSE_NAME: &'static str = {source_name:?};
    pub const RUNTIME_NAME: &'static str = {runtime_name:?};
}}

impl ::verse_rs::native::FromNativeValue for {rust_name} {{
    fn from_native_value(value: ::verse_rs::Value, name: &str) -> ::verse_rs::native::NativeResult<Self> {{
        let __variant = ::verse_rs::native::native_enum_variant(value, Self::RUNTIME_NAME, name)?;
        match __variant.as_str() {{
{from_arms}
            other => Err(::verse_rs::native::NativeError::runtime(format!(
                "`{{name}}` expected enum `{{}}`, got variant `{{other}}`",
                Self::RUNTIME_NAME,
            ))),
        }}
    }}
}}

impl ::verse_rs::native::IntoNativeValue for {rust_name} {{
    fn into_native_value(self) -> ::verse_rs::Value {{
        let variant = match self {{
{into_arms}
        }};
        ::verse_rs::Value::EnumValue {{
            enum_name: Self::RUNTIME_NAME.to_string(),
            variant: variant.to_string(),
        }}
    }}
}}"#,
        rust_name = definition.rust_name,
        variants = indent_block(&variants, 1),
        source_name = definition.source_name,
        runtime_name = definition.runtime_name,
        from_arms = indent_block(&from_arms, 3),
        into_arms = indent_block(&into_arms, 3),
    )
}

fn render_trait_method(function: &NativeFunction) -> String {
    let params = function
        .params
        .iter()
        .map(|param| format!(", {}: {}", param.rust_name, param.rust_type))
        .collect::<String>();
    format!(
        "fn {}(&self, ctx: ::verse_rs::native::NativeCallContext{}) -> ::verse_rs::native::NativeResult<{}>;",
        function.rust_name, params, function.rust_return_type
    )
}

fn render_signature(function: &NativeFunction) -> String {
    let effects = function
        .effects
        .iter()
        .map(|effect| format!("{effect:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "::verse_rs::native::NativeFunctionSignature {{ runtime_name: {:?}, arity: {}, effects: &[{}] }}",
        function.runtime_name,
        function.params.len(),
        effects
    )
}

fn render_registration(function: &NativeFunction) -> String {
    let conversions = function
        .params
        .iter()
        .map(|param| {
            format!(
                r#"let {rust_name} = match <{rust_type} as ::verse_rs::native::FromNativeValue>::from_native_value(__args.next().expect("native arity checked before dispatch"), {source_name:?}) {{
            Ok(value) => value,
            Err(error) => return ::verse_rs::native::NativeCallResult::from_result::<()>(Err(error)),
        }};"#,
                rust_name = param.rust_name,
                rust_type = param.rust_type,
                source_name = param.source_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let args = function
        .params
        .iter()
        .map(|param| param.rust_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let comma_args = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    format!(
        r#"{{
    let api = Arc::clone(&api);
    registry.register({runtime_name:?}, {arity}usize, move |args, _span| {{
        let mut __args = args.into_iter();
{conversions}
        let ctx = ::verse_rs::native::NativeCallContext {{ runtime_name: {runtime_name:?} }};
        ::verse_rs::native::NativeCallResult::from_result(api.{rust_name}(ctx{comma_args}))
    }});
}}"#,
        runtime_name = function.runtime_name,
        arity = function.params.len(),
        conversions = indent_block(&conversions, 2),
        rust_name = function.rust_name,
        comma_args = comma_args
    )
}

fn indent_block(text: &str, levels: usize) -> String {
    if text.is_empty() {
        return String::new();
    }
    let indent = "    ".repeat(levels);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if previous_lower && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_lower = false;
        } else if ch == '-' || ch == ' ' {
            if !out.ends_with('_') {
                out.push('_');
            }
            previous_lower = false;
        } else {
            out.push(ch);
            previous_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    sanitize_rust_ident(&out)
}

fn pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut uppercase_next = true;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            uppercase_next = true;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                out.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                out.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    sanitize_rust_ident(&out)
}

fn sanitize_rust_ident(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            if index == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if is_rust_keyword(&out) {
        out.push('_');
    }
    out
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
    )
}

fn compile_error(message: &str) -> TokenStream {
    let mut stream = TokenStream::new();
    stream.extend([TokenTree::Ident(proc_macro::Ident::new(
        "compile_error",
        proc_macro::Span::call_site(),
    ))]);
    stream.extend([TokenTree::Punct(Punct::new(
        '!',
        proc_macro::Spacing::Alone,
    ))]);
    let mut group_stream = TokenStream::new();
    group_stream.extend([TokenTree::Literal(Literal::string(message))]);
    stream.extend([TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        group_stream,
    ))]);
    stream.extend([TokenTree::Punct(Punct::new(
        ';',
        proc_macro::Spacing::Alone,
    ))]);
    stream
}
