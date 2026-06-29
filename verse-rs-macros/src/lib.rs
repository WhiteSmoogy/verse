use proc_macro::{Delimiter, Group, Literal, Punct, TokenStream, TokenTree};

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
    let functions = parse_native_functions(&config.source, &runtime_prefix)?;
    if functions.is_empty() {
        return Err("native_api! source did not contain any native function declarations".into());
    }
    ensure_unique_methods(&functions)?;
    let digest = render_digest(&components, &functions);
    Ok(render_module(&config, &functions, &digest))
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

fn parse_native_functions(source: &str, runtime_prefix: &str) -> Result<Vec<NativeFunction>, String> {
    let mut functions = Vec::new();
    for (line_index, raw_line) in source.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() || line.starts_with("using ") {
            continue;
        }
        let line = line
            .split_once(" = ")
            .map_or(line, |(signature, _)| signature)
            .trim();
        if line.contains(":=") {
            return Err(format!(
                "native_api! only supports top-level function declarations; unsupported declaration on line {}",
                line_index + 1
            ));
        }
        functions.push(parse_native_function(line, runtime_prefix, line_index + 1)?);
    }
    Ok(functions)
}

fn parse_native_function(
    line: &str,
    runtime_prefix: &str,
    line_number: usize,
) -> Result<NativeFunction, String> {
    let mut cursor = Cursor::new(line);
    let name = cursor.ident().ok_or_else(|| {
        format!("expected native function name on line {line_number}: `{line}`")
    })?;
    let mut specifiers = cursor.angle_specifiers()?;
    if !specifiers.iter().any(|specifier| specifier == "native") {
        return Err(format!(
            "function `{name}` on line {line_number} must include `<native>`"
        ));
    }
    specifiers.retain(|specifier| specifier != "native");
    let params_source = cursor.parenthesized()?;
    let params = parse_params(&params_source, &name)?;
    let effects = cursor.angle_specifiers()?;
    cursor.expect_char(':')?;
    let return_type = cursor.remaining().trim().to_string();
    if return_type.is_empty() {
        return Err(format!("function `{name}` is missing a return type"));
    }
    let rust_return_type = rust_type_for(&return_type).ok_or_else(|| {
        format!(
            "function `{name}` return type `{return_type}` is not supported by native_api! yet"
        )
    })?;
    Ok(NativeFunction {
        rust_name: snake_case(&name),
        runtime_name: format!("{runtime_prefix}.{name}"),
        name,
        specifiers,
        effects,
        params,
        return_type,
        rust_return_type,
    })
}

fn parse_params(source: &str, function_name: &str) -> Result<Vec<NativeParam>, String> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_top_level(source, ',')
        .into_iter()
        .map(|param| parse_param(param.trim(), function_name))
        .collect()
}

fn parse_param(source: &str, function_name: &str) -> Result<NativeParam, String> {
    let (left, right) = source
        .split_once(':')
        .ok_or_else(|| format!("parameter `{source}` in `{function_name}` is missing `:`"))?;
    let left = left.trim();
    let named = left.starts_with('?');
    let source_name = left.trim_start_matches('?').trim().to_string();
    if source_name.is_empty() {
        return Err(format!("parameter `{source}` in `{function_name}` is missing a name"));
    }
    let type_name = right
        .split_once('=')
        .map_or(right, |(type_name, _)| type_name)
        .trim()
        .to_string();
    let rust_type = rust_type_for(&type_name).ok_or_else(|| {
        format!(
            "parameter `{source_name}` in `{function_name}` has unsupported type `{type_name}`"
        )
    })?;
    Ok(NativeParam {
        rust_name: sanitize_rust_ident(&snake_case(&source_name)),
        source_name,
        type_name,
        rust_type,
        named,
    })
}

struct Cursor<'a> {
    source: &'a str,
    index: usize,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, index: 0 }
    }

    fn ident(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.index;
        while let Some(ch) = self.peek() {
            if ch == '_' || ch.is_ascii_alphanumeric() {
                self.index += ch.len_utf8();
            } else {
                break;
            }
        }
        (self.index > start).then(|| self.source[start..self.index].to_string())
    }

    fn angle_specifiers(&mut self) -> Result<Vec<String>, String> {
        let mut specifiers = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some('<') {
                break;
            }
            self.index += 1;
            let start = self.index;
            let mut depth = 0usize;
            while let Some(ch) = self.peek() {
                match ch {
                    '<' | '{' | '(' => depth += 1,
                    '}' | ')' if depth > 0 => depth -= 1,
                    '>' if depth == 0 => {
                        let specifier = self.source[start..self.index].trim().to_string();
                        self.index += 1;
                        specifiers.push(specifier);
                        break;
                    }
                    _ => {}
                }
                if ch != '>' || depth != 0 {
                    self.index += ch.len_utf8();
                }
            }
            if specifiers.last().is_none_or(|specifier| specifier.is_empty()) {
                return Err("empty or unterminated specifier".into());
            }
        }
        Ok(specifiers)
    }

    fn parenthesized(&mut self) -> Result<String, String> {
        self.skip_ws();
        self.expect_char('(')?;
        let start = self.index;
        let mut depth = 0usize;
        while let Some(ch) = self.peek() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' if depth == 0 => {
                    let body = self.source[start..self.index].to_string();
                    self.index += 1;
                    return Ok(body);
                }
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                _ => {}
            }
            self.index += ch.len_utf8();
        }
        Err("unterminated parameter list".into())
    }

    fn expect_char(&mut self, expected: char) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some(expected) {
            self.index += expected.len_utf8();
            Ok(())
        } else {
            Err(format!("expected `{expected}`"))
        }
    }

    fn remaining(&self) -> &'a str {
        &self.source[self.index..]
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.index += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }
}

fn split_top_level(source: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    for (index, ch) in source.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' if paren > 0 => paren -= 1,
            '[' => bracket += 1,
            ']' if bracket > 0 => bracket -= 1,
            '{' => brace += 1,
            '}' if brace > 0 => brace -= 1,
            _ if ch == delimiter && paren == 0 && bracket == 0 && brace == 0 => {
                parts.push(&source[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&source[start..]);
    parts
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn rust_type_for(type_name: &str) -> Option<String> {
    match type_name.trim() {
        "void" | "none" => Some("()".into()),
        "int" => Some("i128".into()),
        "float" => Some("f64".into()),
        "logic" | "bool" => Some("bool".into()),
        "string" | "message" => Some("String".into()),
        "any" => Some("::verse_rs::Value".into()),
        _ => None,
    }
}

fn ensure_unique_methods(functions: &[NativeFunction]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
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

fn render_digest(components: &[String], functions: &[NativeFunction]) -> String {
    let mut lines = Vec::new();
    for (index, component) in components.iter().enumerate() {
        lines.push(format!(
            "{}{}<public> := module:",
            "    ".repeat(index),
            component
        ));
    }
    let indent = "    ".repeat(components.len());
    for function in functions {
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

fn render_module(config: &MacroConfig, functions: &[NativeFunction], digest: &str) -> String {
    let trait_methods = functions
        .iter()
        .map(render_trait_method)
        .collect::<Vec<_>>()
        .join("\n\n");
    let registrations = functions
        .iter()
        .map(render_registration)
        .collect::<Vec<_>>()
        .join("\n");
    let signatures = functions
        .iter()
        .map(render_signature)
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"{visibility}mod {module_name} {{
    use std::sync::Arc;

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
        trait_methods = indent_block(&trait_methods, 2),
        path = config.path,
        digest = digest,
        signatures = indent_block(&signatures, 2),
        registrations = indent_block(&registrations, 2),
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
    stream.extend([TokenTree::Punct(Punct::new('!', proc_macro::Spacing::Alone))]);
    let mut group_stream = TokenStream::new();
    group_stream.extend([TokenTree::Literal(Literal::string(message))]);
    stream.extend([TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        group_stream,
    ))]);
    stream.extend([TokenTree::Punct(Punct::new(';', proc_macro::Spacing::Alone))]);
    stream
}
