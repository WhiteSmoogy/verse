use crate::ast::{
    ArchetypeConstructorCall, ArchetypeEntry, ArchetypeField, ArchetypeLet, AssignOp,
    AttributeArgument, BinaryOp, CallArg, CaseArm, CasePattern, ClassBlock, ClassMethod,
    ConcurrentOp, EnumVariant, Expr, ExprKind, ExtensionMethod, FieldAttribute, ForBinding,
    ForClause, InterpolatedStringPart, Param, ParamPattern, Program, Stmt, StmtKind, StructField,
    TypeAnnotation, TypeName, TypeParam, TypeParamConstraint, UnaryOp,
};
use crate::error::VerseError;
use crate::lexer::lex;
use crate::token::{NumberKind, NumberLiteral, Span, StringLiteralPart, Token, TokenKind};

pub fn parse_source(source: &str) -> Result<Program, VerseError> {
    let tokens = lex(source)?;
    Parser::new(tokens).parse_program()
}

fn parse_interpolation_expression(source: &str, span: Span) -> Result<Expr, VerseError> {
    let mut padded = String::new();
    for _ in 1..span.line {
        padded.push('\n');
    }
    padded.push_str(&" ".repeat(span.column.saturating_sub(1)));
    padded.push_str(source);

    let tokens = lex(&padded)?;
    let mut parser = Parser::new(tokens);
    parser.skip_separators();
    let expr = parser.parse_expression()?;
    parser.skip_separators();
    if !parser.is_at_end() {
        return Err(parser.error_at_current("expected end of string interpolation expression"));
    }
    Ok(expr)
}

fn render_tuple_param_name(params: &[Param]) -> String {
    let rendered = params
        .iter()
        .map(render_param_name)
        .collect::<Vec<_>>()
        .join(", ");
    format!("({rendered})")
}

fn render_param_name(param: &Param) -> String {
    match &param.pattern {
        ParamPattern::Binding => param.name.clone(),
        ParamPattern::Anonymous => "_".to_string(),
        ParamPattern::Tuple(params) => render_tuple_param_name(params),
    }
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

#[derive(Default)]
struct StructSpecifiers {
    persistable: bool,
    computes: bool,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn parse_program(mut self) -> Result<Program, VerseError> {
        let mut statements = Vec::new();
        self.skip_separators();

        while !self.is_at_end() {
            let statement = self.parse_stmt()?;
            let consumed_trailing_separator = stmt_consumed_trailing_separator(&statement);
            statements.push(statement);
            if self.is_at_end() {
                break;
            }

            let consumed = self.skip_separators();
            if consumed == 0 && !consumed_trailing_separator && !self.is_at_end() {
                return Err(self.error_at_current(
                    "expected newline, semicolon, or end of file after statement",
                ));
            }
        }

        Ok(Program { statements })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, VerseError> {
        if self.is_using_statement() {
            return self.parse_using_statement();
        }

        if self.match_var() {
            let var_span = self.previous_span();
            let (name, annotation, expr) = self.parse_var_declaration_parts()?;
            let span = var_span.through(expr.span);
            return Ok(Stmt::new(
                StmtKind::Var {
                    name,
                    annotation: Some(annotation),
                    expr,
                },
                span,
            ));
        }

        if self.match_set() {
            let set_span = self.previous_span();
            let target = self.parse_assignment_target()?;
            let op = self.consume_assignment_operator(
                "expected `=`, `+=`, `-=`, `*=`, or `/=` after assignment target",
            )?;
            let expr = self.parse_expression()?;
            let span = set_span.through(expr.span);
            return Ok(Stmt::new(StmtKind::Set { target, op, expr }, span));
        }

        if self.match_return() {
            let return_span = self.previous_span();
            let expr = if self.at_statement_boundary() {
                Expr::new(ExprKind::None, return_span)
            } else {
                self.parse_expression()?
            };
            let span = return_span.through(expr.span);
            return Ok(Stmt::new(StmtKind::Return(expr), span));
        }

        if self.match_break() {
            let span = self.previous_span();
            return Ok(Stmt::new(StmtKind::Break, span));
        }

        if self.match_defer() {
            let defer_span = self.previous_span();
            let body = if self.match_colon() {
                self.finish_colon_block(self.previous_span())?
            } else if self.match_lbrace() {
                self.finish_block(self.previous_span())?
            } else {
                return Err(self.error_at_current("expected `:` or `{` after `defer`"));
            };
            let span = defer_span.through(body.span);
            return Ok(Stmt::new(StmtKind::Defer(body), span));
        }

        if self.is_extension_method_definition() {
            return self.parse_extension_method_definition();
        }

        if self.is_parametric_type_definition() {
            return self.parse_parametric_type_definition();
        }

        if self.is_function_definition() {
            let (name, expr, span) = self.parse_named_function_definition()?;
            let body = expr
                .body
                .expect("top-level function definition should have a body");
            let expr = Expr::new(
                ExprKind::Function {
                    params: expr.params,
                    effects: expr.effects,
                    return_type: expr.return_type,
                    body: Box::new(body),
                },
                span,
            );

            return Ok(Stmt::new(
                StmtKind::Let {
                    name,
                    specifiers: Vec::new(),
                    annotation: None,
                    expr,
                },
                span,
            ));
        }

        if self.is_type_alias_definition() {
            return self.parse_type_alias_definition();
        }

        if self.is_binding_definition() {
            let (name, name_span) = self.consume_ident("expected binding name")?;
            let specifiers = self.parse_data_specifiers()?;
            let annotation = self.parse_optional_type_annotation()?;
            self.consume_definition_operator("expected `=` or `:=` after binding name")?;
            self.skip_separators();
            let expr = self.parse_expression()?;
            let span = name_span.through(expr.span);
            return Ok(Stmt::new(
                StmtKind::Let {
                    name,
                    specifiers,
                    annotation,
                    expr,
                },
                span,
            ));
        }

        let expr = self.parse_expression()?;
        let span = expr.span;
        Ok(Stmt::new(StmtKind::Expr(expr), span))
    }

    fn parse_using_statement(&mut self) -> Result<Stmt, VerseError> {
        let (_, using_span) = self.consume_ident("expected `using`")?;
        self.consume_lbrace("expected `{` after `using`")?;
        let path = self.parse_module_path()?;
        let close_span = self.consume_rbrace("expected `}` after module path")?;
        Ok(Stmt::new(
            StmtKind::Using { path },
            using_span.through(close_span),
        ))
    }

    fn parse_type_alias_definition(&mut self) -> Result<Stmt, VerseError> {
        let (name, name_span) = self.consume_ident("expected type alias name")?;
        self.consume_colon_equal("expected `:=` after type alias name")?;
        self.skip_separators();
        let target = self.consume_type_name("expected type name after type alias `:=`")?;
        let span = name_span.through(target.span);
        Ok(Stmt::new(StmtKind::TypeAlias { name, target }, span))
    }

    fn parse_parametric_type_definition(&mut self) -> Result<Stmt, VerseError> {
        let (name, name_span) = self.consume_ident("expected parametric type name")?;
        let specifiers = self.parse_data_specifiers()?;
        self.consume_lparen("expected `(` before parametric type parameters")?;
        let params = self.parse_type_param_list("parametric type")?;
        self.consume_colon_equal("expected `:=` after parametric type parameters")?;
        self.skip_separators();
        let expr = self.parse_expression()?;
        let span = name_span.through(expr.span);
        Ok(Stmt::new(
            StmtKind::ParametricType {
                name,
                specifiers,
                params,
                expr,
            },
            span,
        ))
    }

    fn parse_extension_method_definition(&mut self) -> Result<Stmt, VerseError> {
        let open_span = self.consume_lparen("expected `(` before extension method receiver")?;
        let (receiver_name, receiver_name_span) =
            self.consume_ident("expected extension method receiver name")?;
        self.consume_colon("expected `:` after extension method receiver name")?;
        let receiver_type =
            self.consume_type_name("expected extension method receiver type after `:`")?;
        let receiver_close_span =
            self.consume_rparen("expected `)` after extension method receiver")?;
        self.consume_dot("expected `.` after extension method receiver")?;

        let (name, name_span) = self.consume_ident("expected extension method name")?;
        let mut effects = self.parse_function_specifiers()?;
        self.consume_lparen("expected `(` after extension method name")?;
        let params = self.parse_param_list()?;
        effects.extend(self.parse_effect_specifiers()?);
        let return_type = self.parse_optional_type_annotation()?;
        let definition_span = self
            .consume_definition_operator("expected `=` or `:=` after extension method signature")?;
        let body = self.parse_definition_body(definition_span)?;
        let body_span = body.span;
        let span = open_span.through(body.span);

        Ok(Stmt::new(
            StmtKind::ExtensionMethod(Box::new(ExtensionMethod {
                receiver: Param {
                    name: receiver_name,
                    annotation: Some(receiver_type),
                    type_params: Vec::new(),
                    named: false,
                    default: None,
                    pattern: ParamPattern::Binding,
                    span: receiver_name_span.through(receiver_close_span),
                },
                method: ClassMethod {
                    qualifier: None,
                    name,
                    params,
                    effects,
                    return_type,
                    body: Some(body),
                    span: name_span.through(body_span),
                },
                span,
            })),
            span,
        ))
    }

    fn parse_expression(&mut self) -> Result<Expr, VerseError> {
        self.parse_range()
    }

    fn parse_named_function_definition(
        &mut self,
    ) -> Result<(String, ClassMethod, Span), VerseError> {
        let (name, name_span) = self.consume_ident("expected function name")?;
        let mut effects = self.parse_function_specifiers()?;
        self.consume_lparen("expected `(` after function name")?;
        let params = self.parse_param_list()?;
        effects.extend(self.parse_effect_specifiers()?);
        let return_type = self.parse_optional_type_annotation()?;
        let definition_span =
            self.consume_definition_operator("expected `=` or `:=` after function signature")?;
        let body = self.parse_definition_body(definition_span)?;
        let span = name_span.through(body.span);
        Ok((
            name.clone(),
            ClassMethod {
                qualifier: None,
                name,
                params,
                effects,
                return_type,
                body: Some(body),
                span,
            },
            span,
        ))
    }

    fn parse_class_method(&mut self) -> Result<ClassMethod, VerseError> {
        let (qualifier, name, name_span) = self.parse_method_name("expected class method name")?;
        let mut effects = self.parse_function_specifiers()?;
        self.consume_lparen("expected `(` after class method name")?;
        let params = self.parse_param_list()?;
        effects.extend(self.parse_effect_specifiers()?);
        let signature_end = self.previous_span();
        let return_type = self.parse_optional_type_annotation()?;
        let signature_end = return_type
            .as_ref()
            .map_or(signature_end, |annotation| annotation.span);
        let body = if self.match_colon_equal() || self.match_equal() {
            let definition_span = self.previous_span();
            Some(self.parse_definition_body(definition_span)?)
        } else {
            None
        };
        let span = body.as_ref().map_or_else(
            || name_span.through(signature_end),
            |body| name_span.through(body.span),
        );

        Ok(ClassMethod {
            qualifier,
            name,
            params,
            effects,
            return_type,
            body,
            span,
        })
    }

    fn parse_method_name(
        &mut self,
        message: &str,
    ) -> Result<(Option<String>, String, Span), VerseError> {
        if self.match_lparen() {
            let open_span = self.previous_span();
            let (qualifier, name, span) = self.parse_qualified_name_after_open(open_span)?;
            return Ok((Some(qualifier), name, span));
        }

        let (name, span) = self.consume_ident(message)?;
        Ok((None, name, span))
    }

    fn parse_definition_body(&mut self, definition_span: Span) -> Result<Expr, VerseError> {
        let consumed = self.skip_separators();
        if consumed > 0 && !self.is_at_end() && self.peek().span.column > 1 {
            return self.finish_indented_block(definition_span);
        }
        self.parse_expression()
    }

    fn parse_range(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_or()?;

        if self.match_dot_dot() {
            let right = self.parse_or()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::Range,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_and()?;

        while self.match_or() {
            let right = self.parse_and()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::Or,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_equality()?;

        while self.match_and() {
            let right = self.parse_equality()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::And,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_comparison()?;

        loop {
            let op = if self.match_equal() || self.match_equal_equal() {
                Some(BinaryOp::Equal)
            } else if self.match_not_equal() {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_comparison()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_term()?;

        loop {
            let op = if self.match_less() {
                Some(BinaryOp::Less)
            } else if self.match_less_equal() {
                Some(BinaryOp::LessEqual)
            } else if self.match_greater() {
                Some(BinaryOp::Greater)
            } else if self.match_greater_equal() {
                Some(BinaryOp::GreaterEqual)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_term()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_factor()?;

        loop {
            let op = if self.match_plus() {
                Some(BinaryOp::Add)
            } else if self.match_minus() {
                Some(BinaryOp::Subtract)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_factor()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_unary()?;

        loop {
            let op = if self.match_star() {
                Some(BinaryOp::Multiply)
            } else if self.match_slash() {
                Some(BinaryOp::Divide)
            } else if self.match_percent() {
                Some(BinaryOp::Remainder)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_unary()?;
            let span = expr.span.through(right.span);
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, VerseError> {
        if self.match_set() {
            let set_span = self.previous_span();
            let target = self.parse_assignment_target()?;
            let op = self.consume_assignment_operator(
                "expected `=`, `+=`, `-=`, `*=`, or `/=` after assignment target",
            )?;
            let expr = self.parse_expression()?;
            let span = set_span.through(expr.span);
            return Ok(Expr::new(
                ExprKind::Set {
                    target: Box::new(target),
                    op,
                    expr: Box::new(expr),
                },
                span,
            ));
        }

        if self.match_not() {
            let op_span = self.previous_span();
            let expr = self.parse_unary()?;
            let span = op_span.through(expr.span);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                },
                span,
            ));
        }

        if self.match_plus() {
            let op_span = self.previous_span();
            let expr = self.parse_unary()?;
            let span = op_span.through(expr.span);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnaryOp::Positive,
                    expr: Box::new(expr),
                },
                span,
            ));
        }

        if self.match_minus() {
            let op_span = self.previous_span();
            if let TokenKind::Number {
                value: NumberLiteral::Int(value),
                kind: NumberKind::Int,
            } = self.peek_kind()
                && *value == i128::from(i64::MAX) + 1
            {
                let number_span = self.advance().span;
                return Ok(Expr::new(
                    ExprKind::Number {
                        value: NumberLiteral::Int(i128::from(i64::MIN)),
                        kind: NumberKind::Int,
                    },
                    op_span.through(number_span),
                ));
            }
            let expr = self.parse_unary()?;
            let span = op_span.through(expr.span);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnaryOp::Negate,
                    expr: Box::new(expr),
                },
                span,
            ));
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expr, VerseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if expr_consumed_trailing_separator(&expr) {
                break;
            }

            if self.match_lparen() {
                let (args, close_span) = self.parse_arg_list()?;
                let span = expr.span.through(close_span);
                expr = Expr::new(
                    ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
            } else if self.match_lbracket() {
                let (args, close_span) = self.parse_bracket_arg_list()?;
                let span = expr.span.through(close_span);
                expr = Expr::new(
                    ExprKind::BracketCall {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
            } else if matches!(self.peek_kind(), TokenKind::LBrace) && is_archetype_callee(&expr) {
                self.advance();
                let lbrace_span = self.previous_span();
                expr = self.finish_archetype(expr, lbrace_span)?;
            } else if matches!(self.peek_kind(), TokenKind::Colon)
                && matches!(self.kind_at(self.current + 1), Some(TokenKind::Newline))
                && is_archetype_callee(&expr)
            {
                self.advance();
                let colon_span = self.previous_span();
                expr = self.finish_colon_archetype(expr, colon_span)?;
            } else if self.match_question() {
                let question_span = self.previous_span();
                let span = expr.span.through(question_span);
                expr = Expr::new(ExprKind::UnwrapOption(Box::new(expr)), span);
            } else if self.match_dot() {
                if self.check_lparen() {
                    let open_span = self.consume_lparen("expected `(` after `.`")?;
                    if !self.is_qualified_name_after_open() {
                        return Err(self.error_at_current("expected qualified member name"));
                    }
                    let (qualifier, name, name_span) =
                        self.parse_qualified_name_after_open(open_span)?;
                    let span = expr.span.through(name_span);
                    expr = Expr::new(
                        ExprKind::QualifiedMember {
                            object: Box::new(expr),
                            qualifier,
                            name,
                        },
                        span,
                    );
                } else {
                    let (name, name_span) =
                        self.consume_member_name("expected member name after `.`")?;
                    if is_archetype_callee(&expr) && self.match_colon_equal() {
                        let value = self.parse_expression()?;
                        let field_span = name_span.through(value.span);
                        let span = expr.span.through(value.span);
                        expr = Expr::new(
                            ExprKind::Archetype {
                                block: false,
                                callee: Box::new(expr),
                                entries: vec![ArchetypeEntry::Field(ArchetypeField {
                                    name,
                                    expr: value,
                                    span: field_span,
                                })],
                            },
                            span,
                        );
                    } else {
                        let span = expr.span.through(name_span);
                        expr = Expr::new(
                            ExprKind::Member {
                                object: Box::new(expr),
                                name,
                            },
                            span,
                        );
                    }
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_assignment_target(&mut self) -> Result<Expr, VerseError> {
        let token = self.advance().clone();
        let mut expr = match token.kind {
            TokenKind::Ident(name) => Expr::new(ExprKind::Ident(name), token.span),
            _ => return Err(VerseError::parse("expected assignment target", token.span)),
        };

        loop {
            if self.match_lbracket() {
                let index = self.parse_expression()?;
                let close_span = self.consume_rbracket("expected `]` after index")?;
                let span = expr.span.through(close_span);
                expr = Expr::new(
                    ExprKind::Index {
                        collection: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                );
            } else if self.match_dot() {
                let (name, name_span) =
                    self.consume_member_name("expected member name after `.`")?;
                let span = expr.span.through(name_span);
                expr = Expr::new(
                    ExprKind::Member {
                        object: Box::new(expr),
                        name,
                    },
                    span,
                );
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, VerseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Number {
                value: NumberLiteral::Int(value),
                kind: NumberKind::Int,
            } if value > i128::from(i64::MAX) => Err(VerseError::parse(
                format!("integer literal `{value}` is outside the 64-bit signed range"),
                token.span,
            )),
            TokenKind::Number { value, kind } => {
                Ok(Expr::new(ExprKind::Number { value, kind }, token.span))
            }
            TokenKind::Char { value, kind } => {
                Ok(Expr::new(ExprKind::Char { value, kind }, token.span))
            }
            TokenKind::String(parts) => self.finish_string_literal(parts, token.span),
            TokenKind::True => Ok(Expr::new(ExprKind::Bool(true), token.span)),
            TokenKind::False => Ok(Expr::new(ExprKind::Bool(false), token.span)),
            TokenKind::None => Ok(Expr::new(ExprKind::None, token.span)),
            TokenKind::Ident(name) if name == "array" && self.match_lbrace() => {
                self.finish_array_brace(token.span)
            }
            TokenKind::Ident(name) if name == "map" && self.match_lbrace() => {
                self.finish_map_brace(token.span)
            }
            TokenKind::Ident(name) if name == "enum" => self.finish_enum_definition(token.span),
            TokenKind::Ident(name) if name == "struct" => self.finish_struct_definition(token.span),
            TokenKind::Ident(name) if name == "class" => self.finish_class_definition(token.span),
            TokenKind::Ident(name) if name == "interface" => {
                self.finish_interface_definition(token.span)
            }
            TokenKind::Ident(name) if name == "module" => self.finish_module_definition(token.span),
            TokenKind::Ident(name) if name == "option" && self.match_lbrace() => {
                self.finish_option_brace(token.span)
            }
            TokenKind::Ident(name) if name == "external" && self.match_lbrace() => {
                self.finish_external(token.span)
            }
            TokenKind::Ident(name) if name == "case" => self.finish_case(token.span),
            TokenKind::Ident(name) if name == "block" && self.match_colon() => {
                self.finish_colon_block(self.previous_span())
            }
            TokenKind::Ident(name) if name == "profile" && self.check_lparen() => {
                self.finish_profile(token.span)
            }
            TokenKind::Ident(name) if name == "spawn" && self.match_lbrace() => {
                self.finish_spawn(token.span, self.previous_span())
            }
            TokenKind::Ident(name) => {
                if let Some(op) = concurrent_op_from_name(&name)
                    && self.match_colon()
                {
                    return self.finish_concurrent(op, token.span, self.previous_span());
                }
                Ok(Expr::new(ExprKind::Ident(name), token.span))
            }
            TokenKind::LParen => self.finish_paren_or_tuple(token.span),
            TokenKind::LBrace => self.finish_block(token.span),
            TokenKind::If => self.finish_if(token.span),
            TokenKind::Loop => self.finish_loop(token.span),
            TokenKind::For => self.finish_for(token.span),
            TokenKind::Var => self.finish_var_expression(token.span),
            TokenKind::Eof => Err(VerseError::parse("unexpected end of file", token.span)),
            _ => Err(VerseError::parse("expected expression", token.span)),
        }
    }

    fn parse_var_declaration_parts(
        &mut self,
    ) -> Result<(String, TypeAnnotation, Expr), VerseError> {
        let (name, _) = self.consume_ident("expected variable name after `var`")?;
        let Some(annotation) = self.parse_optional_type_annotation()? else {
            return Err(self.error_at_current("expected explicit type annotation after `var` name"));
        };
        self.consume_equal("expected `=` after variable declaration")?;
        self.skip_separators();
        let expr = self.parse_expression()?;
        Ok((name, annotation, expr))
    }

    fn finish_var_expression(&mut self, var_span: Span) -> Result<Expr, VerseError> {
        let (name, annotation, expr) = self.parse_var_declaration_parts()?;
        let span = var_span.through(expr.span);
        Ok(Expr::new(
            ExprKind::Var {
                name,
                annotation,
                expr: Box::new(expr),
            },
            span,
        ))
    }

    fn finish_string_literal(
        &mut self,
        parts: Vec<StringLiteralPart>,
        span: Span,
    ) -> Result<Expr, VerseError> {
        let mut has_interpolation = false;
        let mut ast_parts = Vec::new();
        let mut plain = String::new();

        for part in parts {
            match part {
                StringLiteralPart::Text(text) => {
                    plain.push_str(&text);
                    ast_parts.push(InterpolatedStringPart::Text(text));
                }
                StringLiteralPart::Interpolation {
                    source,
                    span: interpolation_span,
                } => {
                    has_interpolation = true;
                    let expr = parse_interpolation_expression(&source, interpolation_span)?;
                    ast_parts.push(InterpolatedStringPart::Expr(Box::new(expr)));
                }
            }
        }

        if has_interpolation {
            Ok(Expr::new(ExprKind::InterpolatedString(ast_parts), span))
        } else {
            Ok(Expr::new(ExprKind::String(plain), span))
        }
    }

    fn finish_if(&mut self, if_span: Span) -> Result<Expr, VerseError> {
        if self.match_colon() {
            return self.finish_if_then_block(if_span, self.previous_span());
        }

        let condition = if self.match_lparen() {
            let condition = self.parse_if_condition()?;
            self.consume_rparen("expected `)` after if condition")?;
            condition
        } else {
            self.parse_expression()?
        };

        let then_branch = self.parse_block_or_expression()?;
        let else_branch = if self.match_else() {
            let branch = if self.match_if() {
                let nested_if_span = self.previous_span();
                self.finish_if(nested_if_span)?
            } else {
                self.parse_block_or_expression()?
            };
            Some(Box::new(branch))
        } else {
            None
        };

        let end_span = else_branch
            .as_ref()
            .map(|branch| branch.span)
            .unwrap_or(then_branch.span);
        Ok(Expr::new(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            if_span.through(end_span),
        ))
    }

    fn finish_if_then_block(
        &mut self,
        if_span: Span,
        condition_colon_span: Span,
    ) -> Result<Expr, VerseError> {
        let condition = self.finish_colon_block(condition_colon_span)?;
        let then_colon_span =
            self.consume_then_colon("expected `then:` after `if:` condition block")?;
        let then_branch = self.finish_colon_block(then_colon_span)?;
        let else_branch = if self.match_else() {
            let branch = if self.match_if() {
                let nested_if_span = self.previous_span();
                self.finish_if(nested_if_span)?
            } else {
                self.parse_block_or_expression()?
            };
            Some(Box::new(branch))
        } else {
            None
        };

        let end_span = else_branch
            .as_ref()
            .map(|branch| branch.span)
            .unwrap_or(then_branch.span);
        Ok(Expr::new(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            if_span.through(end_span),
        ))
    }

    fn finish_profile(&mut self, profile_span: Span) -> Result<Expr, VerseError> {
        self.consume_lparen("expected `(` after `profile`")?;
        self.skip_separators();
        if self.check_rparen() {
            return Err(self.error_at_current("profile expression expects a description argument"));
        }
        let description = self.parse_expression()?;
        self.skip_separators();
        if self.match_comma() {
            return Err(self
                .error_at_previous("profile expression expects exactly one description argument"));
        }
        self.consume_rparen("expected `)` after profile description")?;
        let colon_span = self.consume_colon("expected `:` after profile description")?;
        let body = self.finish_colon_block(colon_span)?;
        let span = profile_span.through(body.span);
        Ok(Expr::new(
            ExprKind::Profile {
                description: Box::new(description),
                body: Box::new(body),
            },
            span,
        ))
    }

    fn finish_spawn(&mut self, spawn_span: Span, lbrace_span: Span) -> Result<Expr, VerseError> {
        let body = self.finish_block(lbrace_span)?;
        let span = spawn_span.through(body.span);
        Ok(Expr::new(
            ExprKind::Spawn {
                body: Box::new(body),
            },
            span,
        ))
    }

    fn finish_concurrent(
        &mut self,
        op: ConcurrentOp,
        op_span: Span,
        colon_span: Span,
    ) -> Result<Expr, VerseError> {
        let body = self.finish_colon_block(colon_span)?;
        let span = op_span.through(body.span);
        Ok(Expr::new(
            ExprKind::Concurrent {
                op,
                body: Box::new(body),
            },
            span,
        ))
    }

    fn parse_if_condition(&mut self) -> Result<Expr, VerseError> {
        let mut clauses = Vec::new();

        loop {
            clauses.push(self.parse_if_condition_clause()?);
            self.skip_separators();
            if !self.match_comma() {
                break;
            }
            self.skip_separators();
        }

        if clauses.len() == 1 {
            Ok(clauses.remove(0))
        } else {
            let span = clauses
                .first()
                .map(|first| first.span)
                .unwrap()
                .through(clauses.last().map(|last| last.span).unwrap());
            Ok(Expr::new(ExprKind::FailureSequence(clauses), span))
        }
    }

    fn parse_if_condition_clause(&mut self) -> Result<Expr, VerseError> {
        if matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::ColonEqual))
        {
            let (name, name_span) = self.consume_ident("expected failure binding name")?;
            self.consume_colon_equal("expected `:=` after failure binding name")?;
            let expr = self.parse_expression()?;
            let span = name_span.through(expr.span);
            return Ok(Expr::new(
                ExprKind::FailureBind {
                    name,
                    expr: Box::new(expr),
                },
                span,
            ));
        }

        self.parse_expression()
    }

    fn finish_case(&mut self, case_span: Span) -> Result<Expr, VerseError> {
        self.consume_lparen("expected `(` after `case`")?;
        let subject = self.parse_expression()?;
        self.consume_rparen("expected `)` after case subject")?;
        self.consume_colon("expected `:` after case subject")?;

        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after case subject"));
        }
        if self.is_at_end() {
            return Err(self.error_at_current("expected case arm"));
        }

        let arm_indent = self.peek().span.column;
        if arm_indent == 1 {
            return Err(self.error_at_current("expected indented case arm"));
        }

        let mut arms = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < arm_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != arm_indent {
                return Err(self.error_at_current("unexpected indentation in case block"));
            }

            let ignore_unreachable = self.match_ignore_unreachable_attribute()?;
            let pattern = if matches!(self.peek_kind(), TokenKind::Ident(name) if name == "_") {
                let token = self.advance().clone();
                CasePattern::Wildcard { span: token.span }
            } else {
                CasePattern::Expr(Box::new(self.parse_expression()?))
            };
            self.consume_fat_arrow("expected `=>` after case pattern")?;
            let arm_expr = self.parse_expression()?;
            let arm_span = case_pattern_span(&pattern).through(arm_expr.span);
            arms.push(CaseArm {
                ignore_unreachable,
                pattern,
                expr: arm_expr,
                span: arm_span,
            });

            if self.skip_separators() == 0 {
                break;
            }
        }

        if arms.is_empty() {
            return Err(VerseError::parse("expected case arm", subject.span));
        }

        let span = case_span.through(arms.last().map_or(subject.span, |arm| arm.span));
        Ok(Expr::new(
            ExprKind::Case {
                subject: Box::new(subject),
                arms,
            },
            span,
        ))
    }

    fn finish_loop(&mut self, loop_span: Span) -> Result<Expr, VerseError> {
        let body = self.parse_block_or_expression()?;
        let span = loop_span.through(body.span);
        Ok(Expr::new(
            ExprKind::Loop {
                body: Box::new(body),
            },
            span,
        ))
    }

    fn finish_for(&mut self, for_span: Span) -> Result<Expr, VerseError> {
        if self.match_colon() {
            let colon_span = self.previous_span();
            return self.finish_for_do_block(for_span, colon_span);
        }

        self.consume_lparen("expected `(` after `for`")?;
        let mut clauses = vec![self.parse_for_generator_clause()?];

        while self.match_comma() {
            clauses.push(self.parse_for_clause()?);
        }

        self.consume_rparen("expected `)` after for iterator")?;
        let body = self.parse_block_or_expression()?;
        let span = for_span.through(body.span);
        Ok(Expr::new(
            ExprKind::For {
                clauses,
                body: Box::new(body),
            },
            span,
        ))
    }

    fn finish_for_do_block(
        &mut self,
        for_span: Span,
        colon_span: Span,
    ) -> Result<Expr, VerseError> {
        if self.skip_separators() == 0 {
            return Err(VerseError::parse(
                "expected newline after `for:`",
                colon_span,
            ));
        }
        if self.is_at_end() {
            return Err(self.error_at_current("expected for clause after `for:`"));
        }

        let clause_indent = self.peek().span.column;
        if clause_indent == 1 {
            return Err(self.error_at_current("expected indented for clause"));
        }

        let mut clauses = vec![self.parse_for_generator_clause()?];

        loop {
            let consumed = self.skip_separators();
            if self.is_at_end() {
                return Err(self.error_at_current("expected `do:` after for clauses"));
            }
            if self.is_for_do_separator(clause_indent) {
                break;
            }
            if consumed == 0 {
                return Err(self.error_at_current("expected newline after for clause"));
            }

            let column = self.peek().span.column;
            if column < clause_indent {
                break;
            }
            if column != clause_indent {
                return Err(self.error_at_current("unexpected indentation in for clause block"));
            }

            clauses.push(self.parse_for_clause()?);
        }

        self.consume_do("expected `do` after for clauses")?;
        let do_colon_span = self.consume_colon("expected `:` after `do`")?;
        let body = self.finish_colon_block(do_colon_span)?;
        let span = for_span.through(body.span);
        Ok(Expr::new(
            ExprKind::For {
                clauses,
                body: Box::new(body),
            },
            span,
        ))
    }

    fn parse_for_clause(&mut self) -> Result<ForClause, VerseError> {
        if self.is_for_generator_clause() {
            return self.parse_for_generator_clause();
        }

        if self.is_for_range_or_let_clause() {
            let (name, name_span) = self.consume_ident("expected for binding name")?;
            self.consume_colon_equal("expected `:=` after for binding name")?;
            let expr = self.parse_expression()?;
            return Ok(ForClause::RangeOrLet {
                name,
                span: name_span.through(expr.span),
                expr,
            });
        }

        Ok(ForClause::Filter(self.parse_expression()?))
    }

    fn parse_for_generator_clause(&mut self) -> Result<ForClause, VerseError> {
        let (first_name, first_span) =
            self.consume_ident("expected loop variable name after `for (`")?;
        let binding = if self.match_arrow() {
            let (value_name, _) = self.consume_ident("expected value variable name after `->`")?;
            self.consume_colon("expected `:` after for pair binding")?;
            ForBinding::Pair {
                key: first_name,
                value: value_name,
            }
        } else {
            self.consume_for_value_separator("expected `:` or `:=` after for variable")?;
            ForBinding::Value(first_name)
        };
        let iterable = self.parse_expression()?;
        let span = first_span.through(iterable.span);
        Ok(ForClause::Generator {
            binding,
            iterable,
            span,
        })
    }

    fn finish_paren_or_tuple(&mut self, lparen_span: Span) -> Result<Expr, VerseError> {
        self.skip_separators();
        if self.is_qualified_name_after_open() {
            let (qualifier, name, span) = self.parse_qualified_name_after_open(lparen_span)?;
            return Ok(Expr::new(ExprKind::QualifiedName { qualifier, name }, span));
        }

        let first = self.parse_expression()?;
        self.skip_separators();

        if !self.match_comma() {
            self.consume_rparen("expected `)` after expression")?;
            return Ok(first);
        }

        let mut items = vec![first];
        self.skip_separators();

        loop {
            if self.check_rparen() {
                break;
            }

            items.push(self.parse_expression()?);
            self.skip_separators();

            if self.match_comma() {
                self.skip_separators();
                continue;
            }

            break;
        }

        if items.len() < 2 {
            return Err(self.error_at_current("tuple literal expects at least two elements"));
        }

        let rparen_span = self.consume_rparen("expected `)` after tuple literal")?;
        Ok(Expr::new(
            ExprKind::Tuple(items),
            lparen_span.through(rparen_span),
        ))
    }

    fn is_qualified_name_after_open(&self) -> bool {
        self.skip_qualified_name_after_open_at(self.current)
            .is_some()
    }

    fn parse_qualified_name_after_open(
        &mut self,
        open_span: Span,
    ) -> Result<(String, String, Span), VerseError> {
        let qualifier = self.parse_module_path()?;
        self.consume_colon("expected `:` after qualifier")?;
        self.consume_rparen("expected `)` after qualifier")?;
        let (name, name_span) = self.consume_member_name("expected name after qualifier")?;
        Ok((qualifier, name, open_span.through(name_span)))
    }

    fn finish_array_brace(&mut self, array_span: Span) -> Result<Expr, VerseError> {
        let mut items = Vec::new();
        self.skip_separators();

        if self.match_rbrace() {
            return Ok(Expr::new(
                ExprKind::Array(items),
                array_span.through(self.previous_span()),
            ));
        }

        loop {
            items.push(self.parse_expression()?);
            self.skip_separators();

            if self.match_comma() {
                self.skip_separators();
                if self.check_rbrace() {
                    break;
                }
                continue;
            }

            break;
        }

        self.skip_separators();
        let rbrace_span = self.consume_rbrace("expected `}` after array literal")?;
        Ok(Expr::new(
            ExprKind::Array(items),
            array_span.through(rbrace_span),
        ))
    }

    fn finish_map_brace(&mut self, map_span: Span) -> Result<Expr, VerseError> {
        let mut entries = Vec::new();
        self.skip_separators();

        if self.match_rbrace() {
            return Ok(Expr::new(
                ExprKind::Map(entries),
                map_span.through(self.previous_span()),
            ));
        }

        loop {
            let key = self.parse_expression()?;
            self.consume_fat_arrow("expected `=>` between map key and value")?;
            let value = self.parse_expression()?;
            entries.push((key, value));
            self.skip_separators();

            if self.match_comma() {
                self.skip_separators();
                if self.check_rbrace() {
                    break;
                }
                continue;
            }

            break;
        }

        self.skip_separators();
        let rbrace_span = self.consume_rbrace("expected `}` after map literal")?;
        Ok(Expr::new(
            ExprKind::Map(entries),
            map_span.through(rbrace_span),
        ))
    }

    fn finish_enum_definition(&mut self, enum_span: Span) -> Result<Expr, VerseError> {
        let (open, persistable) = self.parse_enum_specifiers()?;

        if self.match_colon() {
            return self.finish_enum_block_definition(enum_span, open, persistable);
        }

        self.consume_lbrace("expected `{` or `:` after `enum`")?;
        self.skip_separators();
        let mut variants = Vec::new();

        if !self.match_rbrace() {
            loop {
                let variant = self.parse_enum_variant()?;
                if variants
                    .iter()
                    .any(|existing: &EnumVariant| existing.name == variant.name)
                {
                    return Err(VerseError::parse("duplicate enum value", variant.span));
                }
                variants.push(variant);
                self.skip_separators();

                if self.match_comma() {
                    self.skip_separators();
                    if self.check_rbrace() {
                        break;
                    }
                    continue;
                }

                break;
            }

            self.skip_separators();
            self.consume_rbrace("expected `}` after enum definition")?;
        }

        let rbrace_span = self.previous_span();
        Ok(Expr::new(
            ExprKind::EnumDefinition {
                open,
                persistable,
                block: false,
                variants,
            },
            enum_span.through(rbrace_span),
        ))
    }

    fn finish_enum_block_definition(
        &mut self,
        enum_span: Span,
        open: bool,
        persistable: bool,
    ) -> Result<Expr, VerseError> {
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `enum:`"));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected enum value after `enum:`"));
        }

        let variant_indent = self.peek().span.column;
        if variant_indent == 1 {
            return Err(self.error_at_current("expected indented enum value"));
        }

        let mut variants = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < variant_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != variant_indent {
                return Err(self.error_at_current("unexpected indentation in enum block"));
            }

            let variant = self.parse_enum_variant()?;
            if variants
                .iter()
                .any(|existing: &EnumVariant| existing.name == variant.name)
            {
                return Err(VerseError::parse("duplicate enum value", variant.span));
            }
            variants.push(variant);

            let consumed = self.skip_separators();
            if consumed == 0 {
                return Err(self.error_at_current("expected newline or semicolon after enum value"));
            }
        }

        let span = enum_span.through(self.previous_span());
        Ok(Expr::new(
            ExprKind::EnumDefinition {
                open,
                persistable,
                block: true,
                variants,
            },
            span,
        ))
    }

    fn parse_enum_specifiers(&mut self) -> Result<(bool, bool), VerseError> {
        let mut open = false;
        let mut saw_openness = false;
        let mut persistable = false;

        while self.match_less() {
            let (specifier, specifier_span) =
                self.consume_ident("expected enum specifier after `<`")?;
            validate_enum_specifier(&specifier, specifier_span)?;
            self.consume_greater("expected `>` after enum specifier")?;

            match specifier.as_str() {
                "open" | "closed" => {
                    if saw_openness {
                        return Err(VerseError::parse(
                            "duplicate enum openness specifier",
                            specifier_span,
                        ));
                    }
                    saw_openness = true;
                    open = specifier == "open";
                }
                "persistable" => {
                    if persistable {
                        return Err(VerseError::parse(
                            "duplicate enum specifier `persistable`",
                            specifier_span,
                        ));
                    }
                    persistable = true;
                }
                _ => unreachable!("enum specifier should be validated"),
            }
        }

        Ok((open, persistable))
    }

    fn parse_enum_variant(&mut self) -> Result<EnumVariant, VerseError> {
        if self.match_lparen() {
            let open_span = self.previous_span();
            let (qualifier, _) =
                self.consume_ident("expected enum name in qualified enum value")?;
            self.consume_colon("expected `:` after enum name in qualified enum value")?;
            self.consume_rparen("expected `)` after qualified enum value")?;
            let (name, name_span) =
                self.consume_member_name("expected enum value name after qualification")?;
            return Ok(EnumVariant {
                name,
                qualifier: Some(qualifier),
                span: open_span.through(name_span),
            });
        }

        let (name, span) = self.consume_ident("expected enum value name")?;
        Ok(EnumVariant {
            name,
            qualifier: None,
            span,
        })
    }

    fn finish_struct_definition(&mut self, struct_span: Span) -> Result<Expr, VerseError> {
        let specifiers = self.parse_struct_specifiers()?;
        if self.match_lbrace() {
            self.skip_separators();
            let rbrace_span = self.consume_rbrace("expected `}` after empty struct definition")?;
            return Ok(Expr::new(
                ExprKind::StructDefinition {
                    persistable: specifiers.persistable,
                    computes: specifiers.computes,
                    block: false,
                    fields: Vec::new(),
                },
                struct_span.through(rbrace_span),
            ));
        }

        self.consume_colon("expected `:` or `{}` after `struct`")?;
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `struct:`"));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected struct field after `struct:`"));
        }

        let field_indent = self.peek().span.column;
        if field_indent == 1 {
            return Err(self.error_at_current("expected indented struct field"));
        }
        let mut fields = Vec::new();

        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < field_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != field_indent {
                return Err(self.error_at_current("unexpected indentation in struct field block"));
            }

            let (name, name_span) = self.consume_ident("expected struct field name")?;
            let annotation = self.parse_optional_type_annotation()?;
            if annotation.is_none() {
                return Err(VerseError::parse(
                    "expected explicit type annotation after struct field name",
                    name_span,
                ));
            }

            let default = if self.match_equal() {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let field_span = default
                .as_ref()
                .map_or(name_span, |expr| name_span.through(expr.span));

            if fields.iter().any(|field: &StructField| field.name == name) {
                return Err(VerseError::parse("duplicate struct field", name_span));
            }
            fields.push(StructField {
                name,
                attributes: Vec::new(),
                var_specifiers: Vec::new(),
                specifiers: Vec::new(),
                annotation,
                default,
                mutable: false,
                span: field_span,
            });

            let consumed = self.skip_separators();
            if consumed == 0 {
                return Err(
                    self.error_at_current("expected newline or semicolon after struct field")
                );
            }
        }

        let span = fields
            .last()
            .map_or(struct_span, |field| struct_span.through(field.span));
        Ok(Expr::new(
            ExprKind::StructDefinition {
                persistable: specifiers.persistable,
                computes: specifiers.computes,
                block: true,
                fields,
            },
            span,
        ))
    }

    fn parse_struct_specifiers(&mut self) -> Result<StructSpecifiers, VerseError> {
        let mut specifiers = StructSpecifiers::default();
        while self.match_less() {
            let (specifier, specifier_span) =
                self.consume_ident("expected struct specifier after `<`")?;
            validate_struct_specifier(&specifier, specifier_span)?;
            self.consume_greater("expected `>` after struct specifier")?;
            match specifier.as_str() {
                "persistable" => {
                    if specifiers.persistable {
                        return Err(VerseError::parse(
                            "duplicate struct specifier `persistable`",
                            specifier_span,
                        ));
                    }
                    specifiers.persistable = true;
                }
                "computes" => {
                    if specifiers.computes {
                        return Err(VerseError::parse(
                            "duplicate struct specifier `computes`",
                            specifier_span,
                        ));
                    }
                    specifiers.computes = true;
                }
                _ => unreachable!("struct specifier should be validated"),
            }
        }
        Ok(specifiers)
    }

    fn finish_class_definition(&mut self, class_span: Span) -> Result<Expr, VerseError> {
        let specifiers = self.parse_class_specifiers()?;
        let parents = if self.match_lparen() {
            self.parse_type_argument_list("expected class parent type after `(`")?
        } else {
            Vec::new()
        };
        let mut parents = parents.into_iter();
        let base = parents.next();
        let interfaces = parents.collect::<Vec<_>>();

        if self.match_lbrace() {
            self.skip_separators();
            let rbrace_span = self.consume_rbrace("expected `}` after empty class definition")?;
            return Ok(Expr::new(
                ExprKind::ClassDefinition {
                    block: false,
                    specifiers,
                    base,
                    interfaces,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    extension_methods: Vec::new(),
                    blocks: Vec::new(),
                },
                class_span.through(rbrace_span),
            ));
        }

        self.consume_colon("expected `:` or `{}` after `class`")?;
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `class:`"));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected class field after `class:`"));
        }

        let field_indent = self.peek().span.column;
        if field_indent == 1 {
            return Err(self.error_at_current("expected indented class field"));
        }
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut extension_methods = Vec::new();
        let mut blocks = Vec::new();

        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < field_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != field_indent {
                return Err(self.error_at_current("unexpected indentation in class field block"));
            }

            let attributes = self.parse_field_attributes(field_indent)?;

            if self.is_class_method() {
                if !attributes.is_empty() {
                    return Err(self.error_at_current("field attributes cannot apply to methods"));
                }
                let method = self.parse_class_method()?;
                if fields
                    .iter()
                    .any(|field: &StructField| field.name == method.name)
                {
                    return Err(VerseError::parse(
                        format!("duplicate class member `{}`", method.name),
                        method.span,
                    ));
                }
                methods.push(method);

                let consumed = self.skip_separators();
                let consumed_trailing_separator = methods
                    .last()
                    .and_then(|method| method.body.as_ref())
                    .is_some_and(expr_consumed_trailing_separator);
                if consumed == 0
                    && !consumed_trailing_separator
                    && !self.is_at_end()
                    && self.peek().span.column == field_indent
                {
                    return Err(
                        self.error_at_current("expected newline or semicolon after class method")
                    );
                }
                continue;
            }

            if self.is_extension_method_definition() {
                if !attributes.is_empty() {
                    return Err(self.error_at_current("field attributes cannot apply to methods"));
                }
                let statement = self.parse_extension_method_definition()?;
                let StmtKind::ExtensionMethod(extension) = statement.kind else {
                    unreachable!("extension method parser should produce extension method stmt")
                };
                if methods
                    .iter()
                    .any(|method: &ClassMethod| method.name == extension.method.name)
                    || extension_methods.iter().any(|existing: &ExtensionMethod| {
                        existing.method.name == extension.method.name
                            && existing.receiver.annotation == extension.receiver.annotation
                    })
                {
                    return Err(VerseError::parse(
                        format!("duplicate class member `{}`", extension.method.name),
                        extension.span,
                    ));
                }
                extension_methods.push(*extension);

                let consumed = self.skip_separators();
                let consumed_trailing_separator = extension_methods
                    .last()
                    .and_then(|extension| extension.method.body.as_ref())
                    .is_some_and(expr_consumed_trailing_separator);
                if consumed == 0
                    && !consumed_trailing_separator
                    && !self.is_at_end()
                    && self.peek().span.column == field_indent
                {
                    return Err(self
                        .error_at_current("expected newline or semicolon after extension method"));
                }
                continue;
            }

            if self.is_class_block_clause() {
                if !attributes.is_empty() {
                    return Err(self.error_at_current("field attributes cannot apply to `block`"));
                }
                let (_, block_keyword_span) =
                    self.consume_ident("expected `block` in class block clause")?;
                let colon_span = self.consume_colon("expected `:` after `block`")?;
                let body = self.finish_colon_block(colon_span)?;
                let span = block_keyword_span.through(body.span);
                blocks.push(ClassBlock { body, span });

                let consumed = self.skip_separators();
                let consumed_trailing_separator = blocks
                    .last()
                    .is_some_and(|block| expr_consumed_trailing_separator(&block.body));
                if consumed == 0
                    && !consumed_trailing_separator
                    && !self.is_at_end()
                    && self.peek().span.column == field_indent
                {
                    return Err(
                        self.error_at_current("expected newline or semicolon after class block")
                    );
                }
                continue;
            }

            let field = self.parse_class_like_field(attributes, "class")?;
            if fields
                .iter()
                .any(|existing: &StructField| existing.name == field.name)
            {
                return Err(VerseError::parse("duplicate class field", field.span));
            }
            if methods
                .iter()
                .any(|method: &ClassMethod| method.name == field.name)
                || extension_methods
                    .iter()
                    .any(|extension| extension.method.name == field.name)
            {
                return Err(VerseError::parse("duplicate class member", field.span));
            }
            fields.push(field);

            let consumed = self.skip_separators();
            if consumed == 0 {
                return Err(
                    self.error_at_current("expected newline or semicolon after class field")
                );
            }
        }

        let span = fields
            .last()
            .map(|field| field.span)
            .into_iter()
            .chain(methods.last().map(|method| method.span))
            .chain(extension_methods.last().map(|extension| extension.span))
            .chain(blocks.last().map(|block| block.span))
            .max_by_key(|span| span.end)
            .map_or(class_span, |member_span| class_span.through(member_span));
        Ok(Expr::new(
            ExprKind::ClassDefinition {
                block: true,
                specifiers,
                base,
                interfaces,
                fields,
                methods,
                extension_methods,
                blocks,
            },
            span,
        ))
    }

    fn finish_interface_definition(&mut self, interface_span: Span) -> Result<Expr, VerseError> {
        let parents = if self.match_lparen() {
            self.parse_type_argument_list("expected interface parent type after `(`")?
        } else {
            Vec::new()
        };

        if !self.match_colon() {
            return Ok(Expr::new(
                ExprKind::InterfaceDefinition {
                    block: false,
                    parents,
                    fields: Vec::new(),
                    methods: Vec::new(),
                },
                interface_span,
            ));
        }

        let colon_span = self.previous_span();
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `interface:`"));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected interface member after `interface:`"));
        }

        let member_indent = self.peek().span.column;
        if member_indent == 1 {
            return Err(self.error_at_current("expected indented interface member"));
        }

        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < member_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != member_indent {
                return Err(self.error_at_current("unexpected indentation in interface block"));
            }

            let attributes = self.parse_field_attributes(member_indent)?;

            if self.is_class_method() {
                if !attributes.is_empty() {
                    return Err(self.error_at_current("field attributes cannot apply to methods"));
                }
                let method = self.parse_class_method()?;
                if fields
                    .iter()
                    .any(|field: &StructField| field.name == method.name)
                {
                    return Err(VerseError::parse(
                        format!("duplicate interface member `{}`", method.name),
                        method.span,
                    ));
                }
                methods.push(method);

                let consumed = self.skip_separators();
                let consumed_trailing_separator = methods
                    .last()
                    .and_then(|method| method.body.as_ref())
                    .is_some_and(expr_consumed_trailing_separator);
                if consumed == 0
                    && !consumed_trailing_separator
                    && !self.is_at_end()
                    && self.peek().span.column == member_indent
                {
                    return Err(self
                        .error_at_current("expected newline or semicolon after interface method"));
                }
                continue;
            }

            if self.is_class_block_clause() {
                return Err(self.error_at_current("interface definitions cannot contain `block`"));
            }

            let field = self.parse_class_like_field(attributes, "interface")?;
            if fields
                .iter()
                .any(|existing: &StructField| existing.name == field.name)
                || methods
                    .iter()
                    .any(|method: &ClassMethod| method.name == field.name)
            {
                return Err(VerseError::parse(
                    format!("duplicate interface member `{}`", field.name),
                    field.span,
                ));
            }
            fields.push(field);

            let consumed = self.skip_separators();
            if consumed == 0 {
                return Err(
                    self.error_at_current("expected newline or semicolon after interface field")
                );
            }
        }

        let span = fields
            .last()
            .map(|field| field.span)
            .into_iter()
            .chain(methods.last().map(|method| method.span))
            .max_by_key(|span| span.end)
            .map_or(interface_span.through(colon_span), |member_span| {
                interface_span.through(member_span)
            });
        Ok(Expr::new(
            ExprKind::InterfaceDefinition {
                block: true,
                parents,
                fields,
                methods,
            },
            span,
        ))
    }

    fn parse_class_like_field(
        &mut self,
        attributes: Vec<FieldAttribute>,
        context: &str,
    ) -> Result<StructField, VerseError> {
        let mutable = self.match_var();
        let var_specifiers = if mutable {
            self.parse_var_field_specifiers()?
        } else {
            Vec::new()
        };
        let (name, name_span) = self.consume_ident(&format!("expected {context} field name"))?;
        let mut specifiers = Vec::new();
        for specifier in self.parse_class_field_specifiers()? {
            if specifiers.iter().any(|existing| existing == &specifier) {
                return Err(VerseError::parse(
                    format!("duplicate class field specifier `{specifier}`"),
                    name_span,
                ));
            }
            specifiers.push(specifier);
        }
        let annotation = self.parse_optional_type_annotation()?;
        if annotation.is_none() {
            return Err(VerseError::parse(
                format!("expected explicit type annotation after {context} field name"),
                name_span,
            ));
        }

        let default = if self.match_equal() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let field_span = default
            .as_ref()
            .map_or(name_span, |expr| name_span.through(expr.span));

        Ok(StructField {
            name,
            attributes,
            var_specifiers,
            specifiers,
            annotation,
            default,
            mutable,
            span: field_span,
        })
    }

    fn parse_type_argument_list(
        &mut self,
        item_message: &str,
    ) -> Result<Vec<TypeAnnotation>, VerseError> {
        let mut items = Vec::new();
        if self.match_rparen() {
            return Ok(items);
        }

        loop {
            items.push(self.consume_type_name(item_message)?);
            if self.match_comma() {
                continue;
            }
            self.consume_rparen("expected `)` after type list")?;
            return Ok(items);
        }
    }

    fn finish_module_definition(&mut self, module_span: Span) -> Result<Expr, VerseError> {
        if self.match_lbrace() {
            let lbrace_span = self.previous_span();
            let body = self.finish_block(lbrace_span)?;
            let ExprKind::Block(statements) = body.kind else {
                unreachable!("finish_block should return a block expression")
            };
            return Ok(Expr::new(
                ExprKind::ModuleDefinition {
                    block: false,
                    statements,
                },
                module_span.through(body.span),
            ));
        }

        self.consume_colon("expected `:` or `{}` after `module`")?;
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `module:`"));
        }
        if self.is_at_end() {
            return Err(self.error_at_current("expected module member after `module:`"));
        }

        let body = self.finish_indented_block(module_span)?;
        let ExprKind::ColonBlock(statements) = body.kind else {
            unreachable!("finish_indented_block should return a colon block expression")
        };
        Ok(Expr::new(
            ExprKind::ModuleDefinition {
                block: true,
                statements,
            },
            module_span.through(body.span),
        ))
    }

    fn parse_class_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut specifiers = Vec::new();
        while self.match_less() {
            let (name, name_span) = self.consume_ident("expected class specifier after `<`")?;
            validate_class_specifier(&name, name_span)?;
            self.consume_greater("expected `>` after class specifier")?;
            if specifiers.iter().any(|specifier| specifier == &name) {
                return Err(VerseError::parse(
                    format!("duplicate class specifier `{name}`"),
                    name_span,
                ));
            }
            specifiers.push(name);
        }
        Ok(specifiers)
    }

    fn parse_class_field_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut specifiers = Vec::new();
        while self.match_less() {
            let (name, name_span) =
                self.consume_ident("expected class field specifier after `<`")?;
            validate_class_field_specifier(&name, name_span)?;
            self.consume_greater("expected `>` after class field specifier")?;
            if specifiers.iter().any(|specifier| specifier == &name) {
                return Err(VerseError::parse(
                    format!("duplicate class field specifier `{name}`"),
                    name_span,
                ));
            }
            specifiers.push(name);
        }
        Ok(specifiers)
    }

    fn parse_var_field_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut specifiers = Vec::new();
        while self.match_less() {
            let (name, name_span) = self.consume_ident("expected var field specifier after `<`")?;
            validate_var_field_specifier(&name, name_span)?;
            self.consume_greater("expected `>` after var field specifier")?;
            if specifiers.iter().any(|specifier| specifier == &name) {
                return Err(VerseError::parse(
                    format!("duplicate var field specifier `{name}`"),
                    name_span,
                ));
            }
            specifiers.push(name);
        }
        Ok(specifiers)
    }

    fn parse_field_attributes(
        &mut self,
        field_indent: usize,
    ) -> Result<Vec<FieldAttribute>, VerseError> {
        let mut attributes = Vec::new();
        while self.match_at() {
            let attribute_start = self.previous_span();
            let (name, name_span) = self.consume_ident("expected attribute name after `@`")?;
            validate_field_attribute(&name, name_span)?;
            if attributes
                .iter()
                .any(|attribute: &FieldAttribute| attribute.name == name)
            {
                return Err(VerseError::parse(
                    format!("duplicate field attribute `@{name}`"),
                    name_span,
                ));
            }

            let arguments = if self.match_lbrace() {
                self.finish_field_attribute_brace_arguments()?
            } else if self.match_colon() {
                self.finish_field_attribute_colon_arguments(field_indent)?
            } else {
                Vec::new()
            };
            let span = arguments
                .last()
                .map_or(attribute_start.through(name_span), |arg| {
                    attribute_start.through(arg.span)
                });
            attributes.push(FieldAttribute {
                name,
                arguments,
                span,
            });

            if self.skip_separators() == 0 {
                if matches!(self.peek_kind(), TokenKind::At) {
                    continue;
                }
                break;
            }
            if self.is_at_end() || self.peek().span.column < field_indent {
                return Err(self.error_at_current("expected field after attribute"));
            }
            if self.peek().span.column > field_indent {
                return Err(self.error_at_current("unexpected indentation after field attribute"));
            }
        }
        Ok(attributes)
    }

    fn finish_field_attribute_brace_arguments(
        &mut self,
    ) -> Result<Vec<AttributeArgument>, VerseError> {
        let mut arguments = Vec::new();
        self.skip_separators();

        if self.match_rbrace() {
            return Err(
                self.error_at_previous("field attribute braces require at least one argument")
            );
        }

        loop {
            let argument = self.parse_field_attribute_argument()?;
            if arguments
                .iter()
                .any(|existing: &AttributeArgument| existing.name == argument.name)
            {
                return Err(VerseError::parse(
                    format!("duplicate field attribute argument `{}`", argument.name),
                    argument.span,
                ));
            }
            arguments.push(argument);

            let consumed = self.skip_separators();
            if self.match_rbrace() {
                break;
            }
            if self.match_comma() {
                self.skip_separators();
                if self.check_rbrace() {
                    return Err(self.error_at_previous("trailing comma in field attribute"));
                }
                continue;
            }
            if consumed > 0 {
                continue;
            }
            return Err(self.error_at_current(
                "expected comma, newline, semicolon, or `}` after field attribute argument",
            ));
        }

        Ok(arguments)
    }

    fn finish_field_attribute_colon_arguments(
        &mut self,
        field_indent: usize,
    ) -> Result<Vec<AttributeArgument>, VerseError> {
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after field attribute `:`"));
        }
        if self.is_at_end() || self.peek().span.column <= field_indent {
            return Err(self.error_at_current("expected indented field attribute arguments"));
        }

        let argument_indent = self.peek().span.column;
        let mut arguments = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column <= field_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != argument_indent {
                return Err(self.error_at_current("unexpected indentation in field attribute"));
            }
            let argument = self.parse_field_attribute_argument()?;
            if arguments
                .iter()
                .any(|existing: &AttributeArgument| existing.name == argument.name)
            {
                return Err(VerseError::parse(
                    format!("duplicate field attribute argument `{}`", argument.name),
                    argument.span,
                ));
            }
            arguments.push(argument);

            let consumed = self.skip_separators();
            if consumed == 0 {
                return Err(self.error_at_current(
                    "expected newline or semicolon after field attribute argument",
                ));
            }
        }

        Ok(arguments)
    }

    fn parse_field_attribute_argument(&mut self) -> Result<AttributeArgument, VerseError> {
        let (name, name_span) = self.consume_ident("expected field attribute argument name")?;
        self.consume_colon_equal("expected `:=` after field attribute argument name")?;
        let expr = self.parse_expression()?;
        let span = name_span.through(expr.span);
        Ok(AttributeArgument { name, expr, span })
    }

    fn finish_archetype(&mut self, callee: Expr, _lbrace_span: Span) -> Result<Expr, VerseError> {
        let callee_span = callee.span;
        let mut entries = Vec::new();
        self.skip_separators();

        if self.match_rbrace() {
            return Ok(Expr::new(
                ExprKind::Archetype {
                    block: false,
                    callee: Box::new(callee),
                    entries,
                },
                callee_span.through(self.previous_span()),
            ));
        }

        loop {
            let (name, name_span) = self.consume_ident("expected archetype field name")?;
            self.consume_colon_equal("expected `:=` after archetype field name")?;
            let expr = self.parse_expression()?;
            let field_span = name_span.through(expr.span);
            if archetype_entries_have_field(&entries, &name) {
                return Err(VerseError::parse("duplicate archetype field", name_span));
            }
            entries.push(ArchetypeEntry::Field(ArchetypeField {
                name,
                expr,
                span: field_span,
            }));
            let consumed = self.skip_separators();

            if self.check_rbrace() {
                break;
            }

            if self.match_comma() {
                self.skip_separators();
                if self.check_rbrace() {
                    return Err(self.error_at_previous("trailing comma in archetype"));
                }
                continue;
            }

            if consumed > 0 {
                continue;
            }

            break;
        }

        self.skip_separators();
        let rbrace_span = self.consume_rbrace("expected `}` after archetype")?;
        Ok(Expr::new(
            ExprKind::Archetype {
                block: false,
                callee: Box::new(callee),
                entries,
            },
            callee_span.through(rbrace_span),
        ))
    }

    fn finish_colon_archetype(
        &mut self,
        callee: Expr,
        colon_span: Span,
    ) -> Result<Expr, VerseError> {
        if self.skip_separators() == 0 {
            return Err(VerseError::parse(
                "expected newline after archetype `:`",
                colon_span,
            ));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected indented archetype body after `:`"));
        }

        let entry_indent = self.peek().span.column;
        if entry_indent == 1 {
            return Err(self.error_at_current("expected indented archetype body after `:`"));
        }

        let callee_span = callee.span;
        let mut entries = Vec::new();

        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < entry_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != entry_indent {
                return Err(self.error_at_current("unexpected indentation in archetype"));
            }

            if matches!(self.peek_kind(), TokenKind::Ident(name) if name == "let")
                && matches!(self.kind_at(self.current + 1), Some(TokenKind::Colon))
            {
                self.advance();
                self.consume_colon("expected `:` after `let`")?;
                let lets = self.finish_archetype_let_clause(entry_indent)?;
                entries.extend(lets.into_iter().map(ArchetypeEntry::Let));
                if self.is_at_end() || matches!(self.peek_kind(), TokenKind::RBrace) {
                    break;
                }
                continue;
            } else if matches!(self.peek_kind(), TokenKind::Ident(name) if name == "block")
                && matches!(self.kind_at(self.current + 1), Some(TokenKind::Colon))
            {
                self.advance();
                let colon_span = self.consume_colon("expected `:` after `block`")?;
                let body = self.finish_colon_block(colon_span)?;
                entries.push(ArchetypeEntry::Block(body));
                if self.is_at_end() || matches!(self.peek_kind(), TokenKind::RBrace) {
                    break;
                }
                continue;
            } else if self.is_archetype_constructor_call_entry() {
                let call = self.parse_archetype_constructor_call_entry()?;
                entries.push(ArchetypeEntry::ConstructorCall(call));
                if self.match_comma() {
                    if self.skip_separators() == 0 {
                        return Err(self.error_at_current(
                            "expected newline after archetype constructor comma",
                        ));
                    }
                    if self.is_at_end()
                        || matches!(self.peek_kind(), TokenKind::RBrace)
                        || self.peek().span.column < entry_indent
                    {
                        return Err(self.error_at_previous("trailing comma in archetype"));
                    }
                    continue;
                }
            } else {
                let (name, name_span) = self.consume_ident("expected archetype field name")?;
                self.consume_colon_equal("expected `:=` after archetype field name")?;
                let expr = self.parse_expression()?;
                let field_span = name_span.through(expr.span);
                if archetype_entries_have_field(&entries, &name) {
                    return Err(VerseError::parse("duplicate archetype field", name_span));
                }
                entries.push(ArchetypeEntry::Field(ArchetypeField {
                    name,
                    expr,
                    span: field_span,
                }));
                if self.match_comma() {
                    if self.skip_separators() == 0 {
                        return Err(
                            self.error_at_current("expected newline after archetype field comma")
                        );
                    }
                    if self.is_at_end()
                        || matches!(self.peek_kind(), TokenKind::RBrace)
                        || self.peek().span.column < entry_indent
                    {
                        return Err(self.error_at_previous("trailing comma in archetype"));
                    }
                    continue;
                }
            }

            let consumed = self.skip_separators();
            if self.is_at_end() || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if consumed == 0 {
                return Err(
                    self.error_at_current("expected newline or semicolon after archetype entry")
                );
            }
        }

        let span = entries
            .last()
            .map(archetype_entry_span)
            .map_or(callee_span, |entry_span| callee_span.through(entry_span));
        Ok(Expr::new(
            ExprKind::Archetype {
                block: true,
                callee: Box::new(callee),
                entries,
            },
            span,
        ))
    }

    fn is_archetype_constructor_call_entry(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::Less))
            && matches!(self.kind_at(self.current + 2), Some(TokenKind::Ident(name)) if name == "constructor")
            && matches!(self.kind_at(self.current + 3), Some(TokenKind::Greater))
            && matches!(self.kind_at(self.current + 4), Some(TokenKind::LParen))
    }

    fn parse_archetype_constructor_call_entry(
        &mut self,
    ) -> Result<ArchetypeConstructorCall, VerseError> {
        let (name, name_span) = self.consume_ident("expected constructor function name")?;
        if !self.match_less() {
            return Err(self.error_at_current("expected `<constructor>`"));
        }
        let (specifier, _) = self.consume_ident("expected `constructor` specifier")?;
        if specifier != "constructor" {
            return Err(self.error_at_previous("expected `constructor` specifier"));
        }
        self.consume_greater("expected `>` after `constructor`")?;
        self.consume_lparen("expected `(` after constructor specifier")?;
        let (args, close_span) = self.parse_arg_list()?;
        Ok(ArchetypeConstructorCall {
            name,
            args,
            span: name_span.through(close_span),
        })
    }

    fn finish_archetype_let_clause(
        &mut self,
        parent_indent: usize,
    ) -> Result<Vec<ArchetypeLet>, VerseError> {
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `let:`"));
        }

        if self.is_at_end() {
            return Err(self.error_at_current("expected indented let binding after `let:`"));
        }

        let binding_indent = self.peek().span.column;
        if binding_indent <= parent_indent {
            return Err(self.error_at_current("expected indented let binding after `let:`"));
        }

        let mut lets = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < binding_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != binding_indent {
                return Err(self.error_at_current("unexpected indentation in let clause"));
            }

            let (name, name_span) = self.consume_ident("expected let binding name")?;
            let annotation = self.parse_optional_type_annotation()?;
            self.consume_definition_operator("expected `=` or `:=` after let binding name")?;
            let expr = self.parse_expression()?;
            let span = name_span.through(expr.span);
            lets.push(ArchetypeLet {
                name,
                annotation,
                expr,
                span,
            });

            let consumed = self.skip_separators();
            if self.is_at_end() || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if consumed == 0 {
                return Err(
                    self.error_at_current("expected newline or semicolon after let binding")
                );
            }
        }

        Ok(lets)
    }

    fn finish_option_brace(&mut self, option_span: Span) -> Result<Expr, VerseError> {
        let lbrace_span = self.previous_span();
        let mut statements = Vec::new();
        self.skip_separators();

        if self.match_rbrace() {
            return Ok(Expr::new(
                ExprKind::Option(None),
                option_span.through(self.previous_span()),
            ));
        }

        while !self.check_rbrace() && !self.is_at_end() {
            let statement = self.parse_stmt()?;
            let consumed_trailing_separator = stmt_consumed_trailing_separator(&statement);
            statements.push(statement);
            let consumed = self.skip_separators();

            if self.check_rbrace() || self.is_at_end() {
                break;
            }

            if consumed == 0 && !consumed_trailing_separator {
                return Err(self.error_at_current(
                    "expected newline, semicolon, or `}` after option block statement",
                ));
            }
        }

        let rbrace_span = self.consume_rbrace("expected `}` after option literal")?;
        let value = match statements.as_slice() {
            [
                Stmt {
                    kind: StmtKind::Expr(expr),
                    ..
                },
            ] => expr.clone(),
            _ => Expr::new(
                ExprKind::Block(statements),
                lbrace_span.through(rbrace_span),
            ),
        };
        Ok(Expr::new(
            ExprKind::Option(Some(Box::new(value))),
            option_span.through(rbrace_span),
        ))
    }

    fn finish_external(&mut self, external_span: Span) -> Result<Expr, VerseError> {
        let rbrace_span = self.consume_rbrace("expected `}` after `external {`")?;
        Ok(Expr::new(
            ExprKind::External,
            external_span.through(rbrace_span),
        ))
    }

    fn parse_block_or_expression(&mut self) -> Result<Expr, VerseError> {
        if self.match_lbrace() {
            let lbrace_span = self.previous_span();
            self.finish_block(lbrace_span)
        } else if self.match_colon() {
            let colon_span = self.previous_span();
            self.finish_colon_block(colon_span)
        } else if self.match_dot() {
            let dot_span = self.previous_span();
            self.finish_dot_block(dot_span)
        } else {
            self.parse_expression()
        }
    }

    fn finish_dot_block(&mut self, dot_span: Span) -> Result<Expr, VerseError> {
        if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
            return Err(self.error_at_current("expected statement after `.`"));
        }

        let statement = self.parse_stmt()?;
        let span = dot_span.through(statement.span);
        Ok(Expr::new(ExprKind::Block(vec![statement]), span))
    }

    fn finish_colon_block(&mut self, colon_span: Span) -> Result<Expr, VerseError> {
        if self.skip_separators() == 0 {
            return Err(self.error_at_current("expected newline after `:`"));
        }
        self.finish_indented_block(colon_span)
    }

    fn finish_indented_block(&mut self, anchor_span: Span) -> Result<Expr, VerseError> {
        if self.is_at_end() {
            return Err(self.error_at_current("expected indented block after `:`"));
        }

        let block_indent = self.peek().span.column;
        if block_indent == 1 {
            return Err(self.error_at_current("expected indented block after `:`"));
        }

        let mut statements = Vec::new();
        while !self.is_at_end() {
            let column = self.peek().span.column;
            if column < block_indent || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if column != block_indent {
                return Err(self.error_at_current("unexpected indentation in block"));
            }

            let statement = self.parse_stmt()?;
            let consumed_trailing_separator = stmt_consumed_trailing_separator(&statement);
            statements.push(statement);
            let consumed = self.skip_separators();

            if self.is_at_end() || matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }

            if consumed == 0 && !consumed_trailing_separator {
                return Err(
                    self.error_at_current("expected newline or semicolon after block statement")
                );
            }
        }

        let span = statements
            .last()
            .map_or(anchor_span, |statement| anchor_span.through(statement.span));
        Ok(Expr::new(ExprKind::ColonBlock(statements), span))
    }

    fn finish_block(&mut self, lbrace_span: Span) -> Result<Expr, VerseError> {
        let mut statements = Vec::new();
        self.skip_separators();

        while !self.check_rbrace() && !self.is_at_end() {
            let statement = self.parse_stmt()?;
            let consumed_trailing_separator = stmt_consumed_trailing_separator(&statement);
            statements.push(statement);
            let consumed = self.skip_separators();

            if self.check_rbrace() || self.is_at_end() {
                break;
            }

            if consumed == 0 && !consumed_trailing_separator {
                return Err(self.error_at_current(
                    "expected newline, semicolon, or `}` after block statement",
                ));
            }
        }

        let rbrace_span = self.consume_rbrace("expected `}` to close block")?;
        Ok(Expr::new(
            ExprKind::Block(statements),
            lbrace_span.through(rbrace_span),
        ))
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, VerseError> {
        let mut params = Vec::new();
        let mut seen_named = false;
        let mut binding_names = Vec::new();

        if self.match_rparen() {
            return Ok(params);
        }

        loop {
            let param = if self.check_lparen() {
                if seen_named {
                    return Err(self
                        .error_at_current("positional parameters cannot follow named parameters"));
                }
                self.parse_tuple_param_pattern()?
            } else if matches!(self.peek_kind(), TokenKind::Colon) {
                if seen_named {
                    return Err(self
                        .error_at_current("positional parameters cannot follow named parameters"));
                }
                self.parse_anonymous_param(params.len())?
            } else {
                let named = self.match_question();
                if named {
                    seen_named = true;
                } else if seen_named {
                    return Err(self
                        .error_at_current("positional parameters cannot follow named parameters"));
                }
                self.parse_binding_param(named)?
            };
            self.ensure_unique_param_names(&param, &mut binding_names)?;
            params.push(param);

            if self.match_comma() {
                continue;
            }

            self.consume_rparen("expected `)` after parameter list")?;
            break;
        }

        Ok(params)
    }

    fn parse_anonymous_param(&mut self, index: usize) -> Result<Param, VerseError> {
        let colon_span = self.consume_colon("expected `:` before anonymous parameter type")?;
        let annotation = self.consume_type_name("expected anonymous parameter type")?;
        let type_params = self.parse_optional_where_type_params()?;
        let span = type_params
            .last()
            .map(|param| colon_span.through(param.span))
            .unwrap_or_else(|| colon_span.through(annotation.span));
        Ok(Param {
            name: format!("$anonymous{index}"),
            annotation: Some(annotation),
            type_params,
            named: false,
            default: None,
            pattern: ParamPattern::Anonymous,
            span,
        })
    }

    fn parse_binding_param(&mut self, named: bool) -> Result<Param, VerseError> {
        let (name, name_span) = self.consume_ident("expected parameter name")?;
        let annotation = if self.check_lparen() {
            Some(self.parse_inline_function_param_annotation(name_span)?)
        } else {
            self.parse_optional_type_annotation()?
        };
        let type_params = self.parse_optional_where_type_params()?;
        let default = if named && self.match_equal() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let span = default
            .as_ref()
            .map(|default| name_span.through(default.span))
            .or_else(|| {
                type_params
                    .last()
                    .map(|param| name_span.through(param.span))
            })
            .or_else(|| {
                annotation
                    .as_ref()
                    .map(|annotation| name_span.through(annotation.span))
            })
            .unwrap_or(name_span);
        Ok(Param {
            name,
            annotation,
            type_params,
            named,
            default,
            pattern: ParamPattern::Binding,
            span,
        })
    }

    fn parse_tuple_param_pattern(&mut self) -> Result<Param, VerseError> {
        let open_span = self.consume_lparen("expected `(` before destructured tuple parameter")?;
        if self.check_rparen() {
            return Err(
                self.error_at_current("destructured tuple parameter expects at least two elements")
            );
        }

        let mut params = Vec::new();
        let mut item_types = Vec::new();
        let mut binding_names = Vec::new();
        let mut seen_named = false;

        loop {
            let param = if self.check_lparen() {
                if seen_named {
                    return Err(self
                        .error_at_current("positional parameters cannot follow named parameters"));
                }
                self.parse_tuple_param_pattern()?
            } else {
                let named = self.match_question();
                if named {
                    seen_named = true;
                } else if seen_named {
                    return Err(self
                        .error_at_current("positional parameters cannot follow named parameters"));
                }
                self.parse_binding_param(named)?
            };
            let Some(annotation) = param.annotation.as_ref() else {
                return Err(VerseError::parse(
                    "destructured tuple parameter elements require type annotations",
                    param.span,
                ));
            };
            item_types.push(annotation.name.clone());
            self.ensure_unique_param_names(&param, &mut binding_names)?;
            params.push(param);

            if self.match_comma() {
                continue;
            }

            let close_span =
                self.consume_rparen("expected `)` after destructured tuple parameter")?;
            if params.len() < 2 {
                return Err(VerseError::parse(
                    "destructured tuple parameter expects at least two elements",
                    open_span.through(close_span),
                ));
            }
            let span = open_span.through(close_span);
            let name = render_tuple_param_name(&params);
            return Ok(Param {
                name,
                annotation: Some(TypeAnnotation {
                    name: TypeName::Tuple(item_types),
                    span,
                }),
                type_params: Vec::new(),
                named: false,
                default: None,
                pattern: ParamPattern::Tuple(params),
                span,
            });
        }
    }

    fn parse_optional_where_type_params(&mut self) -> Result<Vec<TypeParam>, VerseError> {
        if !matches!(self.peek_kind(), TokenKind::Ident(name) if name == "where") {
            return Ok(Vec::new());
        }

        let where_span = self.advance().span;
        let params = self.parse_type_param_constraints("where clause")?;
        if params.is_empty() {
            return Err(VerseError::parse(
                "expected type parameter after `where`",
                where_span,
            ));
        }
        Ok(params)
    }

    fn parse_type_param_list(&mut self, context: &str) -> Result<Vec<TypeParam>, VerseError> {
        if self.check_rparen() {
            return Err(
                self.error_at_current(&format!("{context} expects at least one type parameter"))
            );
        }

        let params = self.parse_type_param_constraints(context)?;
        self.consume_rparen("expected `)` after type parameter list")?;
        Ok(params)
    }

    fn parse_type_param_constraints(
        &mut self,
        context: &str,
    ) -> Result<Vec<TypeParam>, VerseError> {
        let mut params = Vec::new();
        loop {
            let (name, name_span) =
                self.consume_ident(&format!("expected {context} type parameter name"))?;
            self.consume_colon("expected `:` after type parameter name")?;
            let (constraint, constraint_span) =
                self.consume_ident("expected type parameter constraint")?;
            let constraint = match constraint.as_str() {
                "type" => TypeParamConstraint::Type,
                "subtype" => {
                    self.consume_lparen("expected `(` after `subtype` type parameter constraint")?;
                    let parent = self.consume_type_name("expected supertype in `subtype(...)`")?;
                    self.consume_rparen("expected `)` after `subtype` type parameter constraint")?;
                    TypeParamConstraint::Subtype(parent.name)
                }
                _ => {
                    return Err(VerseError::parse(
                        "type parameter constraints must be `type` or `subtype(...)`",
                        constraint_span,
                    ));
                }
            };
            if params.iter().any(|param: &TypeParam| param.name == name) {
                return Err(VerseError::parse(
                    format!("duplicate type parameter `{name}`"),
                    name_span,
                ));
            }
            params.push(TypeParam {
                name,
                constraint,
                span: name_span,
            });

            if self.match_comma() {
                continue;
            }
            break;
        }
        Ok(params)
    }

    fn parse_inline_function_param_annotation(
        &mut self,
        name_span: Span,
    ) -> Result<TypeAnnotation, VerseError> {
        self.consume_lparen("expected `(` before function parameter signature")?;
        let mut params = Vec::new();
        if !self.match_rparen() {
            loop {
                if self.match_question() {
                    return Err(self.error_at_previous(
                        "function parameter signatures do not use named-argument `?`",
                    ));
                }
                if !self.match_colon() {
                    self.consume_ident("expected function parameter name or `:`")?;
                    self.consume_colon("expected `:` before function parameter type")?;
                }
                let param = self.consume_type_name("expected function parameter type")?;
                params.push(param.name);

                if self.match_comma() {
                    continue;
                }

                self.consume_rparen("expected `)` after function parameter signature")?;
                break;
            }
        }

        let effects = self.parse_effect_specifiers()?;
        self.consume_colon("expected `:` before function parameter return type")?;
        let return_type = self.consume_type_name("expected function parameter return type")?;
        Ok(TypeAnnotation {
            name: TypeName::FunctionSignature {
                params,
                effects,
                return_type: Box::new(return_type.name),
            },
            span: name_span.through(return_type.span),
        })
    }

    fn ensure_unique_param_names(
        &self,
        param: &Param,
        seen: &mut Vec<String>,
    ) -> Result<(), VerseError> {
        match &param.pattern {
            ParamPattern::Binding => {
                if seen.iter().any(|name| name == &param.name) {
                    return Err(VerseError::parse("duplicate parameter name", param.span));
                }
                seen.push(param.name.clone());
            }
            ParamPattern::Anonymous => {}
            ParamPattern::Tuple(params) => {
                for param in params {
                    self.ensure_unique_param_names(param, seen)?;
                }
            }
        }
        Ok(())
    }

    fn parse_optional_type_annotation(&mut self) -> Result<Option<TypeAnnotation>, VerseError> {
        if !self.match_colon() {
            return Ok(None);
        }

        self.consume_type_name("expected type name after `:`")
            .map(Some)
    }

    fn consume_type_name(&mut self, message: &str) -> Result<TypeAnnotation, VerseError> {
        if self.match_question() {
            let question_span = self.previous_span();
            let item = self.consume_type_name("expected option item type after `?`")?;
            return Ok(TypeAnnotation {
                name: TypeName::Option(Box::new(item.name)),
                span: question_span.through(item.span),
            });
        }

        if self.match_lbracket() {
            let lbracket_span = self.previous_span();
            if self.match_rbracket() {
                let item = self.consume_type_name(message)?;
                return Ok(TypeAnnotation {
                    name: TypeName::Array(Some(Box::new(item.name))),
                    span: lbracket_span.through(item.span),
                });
            }

            let key = self.consume_type_name("expected map key type after `[`")?;
            self.consume_rbracket("expected `]` after map key type")?;
            let value = self.consume_type_name("expected map value type after `]`")?;
            return Ok(TypeAnnotation {
                name: TypeName::Map(Box::new(key.name), Box::new(value.name)),
                span: lbracket_span.through(value.span),
            });
        }

        if self.match_lparen() {
            let open_span = self.previous_span();
            if self.is_qualified_name_after_open() {
                let (qualifier, name, span) = self.parse_qualified_name_after_open(open_span)?;
                let qualified_name = format!("{qualifier}.{name}");
                if self.match_lparen() {
                    let (args, close_span) = self.finish_parametric_type_args()?;
                    return Ok(TypeAnnotation {
                        name: TypeName::Applied {
                            name: qualified_name,
                            args,
                        },
                        span: open_span.through(close_span),
                    });
                }
                return Ok(TypeAnnotation {
                    name: TypeName::Named(qualified_name),
                    span,
                });
            }

            let mut items = Vec::new();

            if self.match_rparen() {
                return Err(VerseError::parse(
                    "tuple type expects at least two element types",
                    open_span.through(self.previous_span()),
                ));
            }

            loop {
                let item = self.consume_type_name("expected tuple element type")?;
                items.push(item.name);

                if self.match_comma() {
                    continue;
                }

                let close_span = self.consume_rparen("expected `)` after tuple type")?;
                if items.len() < 2 {
                    return Err(VerseError::parse(
                        "tuple type expects at least two element types",
                        open_span.through(close_span),
                    ));
                }
                return Ok(TypeAnnotation {
                    name: TypeName::Tuple(items),
                    span: open_span.through(close_span),
                });
            }
        }

        let token = self.advance().clone();
        if let TokenKind::Ident(name) = &token.kind
            && name == "tuple"
            && self.match_lparen()
        {
            let mut items = Vec::new();

            if self.match_rparen() {
                return Err(VerseError::parse(
                    "tuple type expects at least two element types",
                    token.span.through(self.previous_span()),
                ));
            }

            loop {
                let item = self.consume_type_name("expected tuple element type")?;
                items.push(item.name);

                if self.match_comma() {
                    continue;
                }

                let close_span = self.consume_rparen("expected `)` after tuple type")?;
                if items.len() < 2 {
                    return Err(VerseError::parse(
                        "tuple type expects at least two element types",
                        token.span.through(close_span),
                    ));
                }
                return Ok(TypeAnnotation {
                    name: TypeName::Tuple(items),
                    span: token.span.through(close_span),
                });
            }
        }

        if let TokenKind::Ident(name) = &token.kind
            && name == "weak_map"
            && self.match_lparen()
        {
            let key = self.consume_type_name("expected weak_map key type")?;
            if !self.match_comma() {
                return Err(self.error_at_current("expected `,` after weak_map key type"));
            }
            let value = self.consume_type_name("expected weak_map value type")?;
            let close_span = self.consume_rparen("expected `)` after weak_map value type")?;
            return Ok(TypeAnnotation {
                name: TypeName::WeakMap(Box::new(key.name), Box::new(value.name)),
                span: token.span.through(close_span),
            });
        }

        if let TokenKind::Ident(name) = &token.kind
            && name == "int_range"
            && self.match_lparen()
        {
            let (min, min_span) = self.consume_int_range_bound("expected int_range minimum")?;
            if !self.match_comma() {
                return Err(self.error_at_current("expected `,` after int_range minimum"));
            }
            let (max, max_span) = self.consume_int_range_bound("expected int_range maximum")?;
            let close_span = self.consume_rparen("expected `)` after int_range maximum")?;
            if min > max {
                return Err(VerseError::parse(
                    "int_range minimum cannot be greater than maximum",
                    min_span.through(max_span),
                ));
            }
            return Ok(TypeAnnotation {
                name: TypeName::IntRange { min, max },
                span: token.span.through(close_span),
            });
        }

        if let TokenKind::Ident(name) = &token.kind
            && name == "type"
            && self.match_lbrace()
        {
            return self.finish_function_type(token.span);
        }

        let mut name = match token.kind {
            TokenKind::Ident(name) => TypeName::parse(name),
            TokenKind::None => TypeName::None,
            _ => return Err(VerseError::parse(message, token.span)),
        };
        let mut end_span = token.span;
        if let TypeName::Named(path) = &mut name {
            while self.match_dot() {
                let (member, _) = self.consume_ident("expected type name after `.`")?;
                path.push('.');
                path.push_str(&member);
                end_span = self.previous_span();
            }
            if self.match_lparen() {
                let (args, close_span) = self.finish_parametric_type_args()?;
                let name = path.clone();
                return Ok(TypeAnnotation {
                    name: TypeName::Applied { name, args },
                    span: token.span.through(close_span),
                });
            }
        }
        Ok(TypeAnnotation {
            name,
            span: token.span.through(end_span),
        })
    }

    fn consume_int_range_bound(&mut self, message: &str) -> Result<(i64, Span), VerseError> {
        let minus_span = self.match_minus().then(|| self.previous_span());
        let token = self.advance().clone();
        let TokenKind::Number {
            value: NumberLiteral::Int(value),
            kind: NumberKind::Int,
        } = token.kind
        else {
            return Err(VerseError::parse(message, token.span));
        };

        let span = minus_span
            .map(|minus_span| minus_span.through(token.span))
            .unwrap_or(token.span);
        let signed = if minus_span.is_some() {
            let min_magnitude = i128::from(i64::MAX) + 1;
            if value > min_magnitude {
                return Err(VerseError::parse(
                    format!("integer literal `-{value}` is outside the 64-bit signed range"),
                    span,
                ));
            }
            if value == min_magnitude {
                i64::MIN
            } else {
                -(value as i64)
            }
        } else {
            if value > i128::from(i64::MAX) {
                return Err(VerseError::parse(
                    format!("integer literal `{value}` is outside the 64-bit signed range"),
                    span,
                ));
            }
            value as i64
        };

        Ok((signed, span))
    }

    fn finish_parametric_type_args(&mut self) -> Result<(Vec<TypeName>, Span), VerseError> {
        let mut args = Vec::new();
        if self.match_rparen() {
            return Ok((args, self.previous_span()));
        }

        loop {
            let arg = self.consume_type_name("expected parametric type argument")?;
            args.push(arg.name);

            if self.match_comma() {
                continue;
            }

            let close_span = self.consume_rparen("expected `)` after parametric type arguments")?;
            return Ok((args, close_span));
        }
    }

    fn finish_function_type(&mut self, type_span: Span) -> Result<TypeAnnotation, VerseError> {
        let (placeholder, _) =
            self.consume_raw_ident("expected `_` placeholder in function type")?;
        if placeholder != "_" {
            return Err(self.error_at_previous("expected `_` placeholder in function type"));
        }

        self.consume_lparen("expected `(` after function type placeholder")?;
        let mut params = Vec::new();
        if !self.match_rparen() {
            loop {
                self.consume_colon("expected `:` before function type parameter")?;
                let param = self.consume_type_name("expected function type parameter")?;
                params.push(param.name);

                if self.match_comma() {
                    continue;
                }

                self.consume_rparen("expected `)` after function type parameters")?;
                break;
            }
        }

        let effects = self.parse_effect_specifiers()?;
        self.consume_colon("expected `:` before function type return type")?;
        let return_type = self.consume_type_name("expected function type return type")?;
        let close_span = self.consume_rbrace("expected `}` after function type")?;

        Ok(TypeAnnotation {
            name: TypeName::FunctionSignature {
                params,
                effects,
                return_type: Box::new(return_type.name),
            },
            span: type_span.through(close_span),
        })
    }

    fn parse_effect_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut effects = Vec::new();
        while self.match_less() {
            let (name, name_span) = self.consume_ident("expected effect name after `<`")?;
            validate_effect_specifier(&name, name_span)?;
            self.consume_greater("expected `>` after effect name")?;
            effects.push(name);
        }
        Ok(effects)
    }

    fn parse_function_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut specifiers = Vec::new();
        while self.match_less() {
            let (name, name_span) = self.consume_ident("expected function specifier after `<`")?;
            validate_function_specifier(&name, name_span)?;
            self.consume_greater("expected `>` after function specifier")?;
            if specifiers.iter().any(|specifier| specifier == &name) {
                return Err(VerseError::parse(
                    format!("duplicate function specifier `{name}`"),
                    name_span,
                ));
            }
            specifiers.push(name);
        }
        Ok(specifiers)
    }

    fn parse_data_specifiers(&mut self) -> Result<Vec<String>, VerseError> {
        let mut specifiers = Vec::new();
        while self.match_less() {
            let (name, name_span) = self.consume_ident("expected data specifier after `<`")?;
            if name == "scoped" {
                self.consume_lbrace("expected `{` after `scoped` data specifier")?;
                self.consume_ident("expected scope name inside `scoped {}`")?;
                self.consume_rbrace("expected `}` after scoped specifier name")?;
            } else {
                validate_data_specifier(&name, name_span)?;
            }
            self.consume_greater("expected `>` after data specifier")?;
            if specifiers.iter().any(|specifier| specifier == &name) {
                return Err(VerseError::parse(
                    format!("duplicate data specifier `{name}`"),
                    name_span,
                ));
            }
            specifiers.push(name);
        }
        Ok(specifiers)
    }

    fn match_ignore_unreachable_attribute(&mut self) -> Result<bool, VerseError> {
        if !self.match_at() {
            return Ok(false);
        }

        let (name, span) = self.consume_ident("expected attribute name after `@`")?;
        if name != "ignore_unreachable" {
            return Err(VerseError::parse(
                format!("unknown case arm attribute `@{name}`"),
                span,
            ));
        }
        Ok(true)
    }

    fn parse_arg_list(&mut self) -> Result<(Vec<CallArg>, Span), VerseError> {
        let mut args = Vec::new();
        let mut seen_named = false;

        if self.match_rparen() {
            return Ok((args, self.previous_span()));
        }

        loop {
            let arg = if self.match_question() {
                seen_named = true;
                let (name, name_span) = self.consume_ident("expected named argument after `?`")?;
                self.consume_colon_equal("expected `:=` after named argument")?;
                let expr = self.parse_expression()?;
                CallArg::Named {
                    name,
                    optional: true,
                    span: name_span.through(expr.span),
                    expr,
                }
            } else if self.is_named_argument_start() {
                seen_named = true;
                let (name, name_span) = self.consume_ident("expected named argument")?;
                self.consume_colon_equal("expected `:=` after named argument")?;
                let expr = self.parse_expression()?;
                CallArg::Named {
                    name,
                    optional: false,
                    span: name_span.through(expr.span),
                    expr,
                }
            } else {
                if seen_named {
                    return Err(
                        self.error_at_current("positional arguments cannot follow named arguments")
                    );
                }
                CallArg::Positional(self.parse_expression()?)
            };
            args.push(arg);
            if self.match_comma() {
                continue;
            }

            let close_span = self.consume_rparen("expected `)` after argument list")?;
            return Ok((args, close_span));
        }
    }

    fn is_named_argument_start(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::ColonEqual))
    }

    fn parse_bracket_arg_list(&mut self) -> Result<(Vec<Expr>, Span), VerseError> {
        let mut args = Vec::new();

        if self.match_rbracket() {
            return Ok((args, self.previous_span()));
        }

        loop {
            args.push(self.parse_expression()?);
            if self.match_comma() {
                continue;
            }

            let close_span = self.consume_rbracket("expected `]` after bracket argument list")?;
            return Ok((args, close_span));
        }
    }

    fn is_function_definition(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Ident(_)) {
            return false;
        }

        let open_index = self.skip_specifiers_at(self.current + 1);
        if !matches!(self.kind_at(open_index), Some(TokenKind::LParen)) {
            return false;
        }

        let Some(close_index) = self.find_matching_delimiter_at(open_index) else {
            return false;
        };
        let index = self.skip_effect_specifiers_at(close_index + 1);
        let index = self.skip_optional_type_annotation_at(index);
        self.is_definition_operator_at(index)
    }

    fn is_extension_method_definition(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::LParen)
            || !matches!(self.kind_at(self.current + 1), Some(TokenKind::Ident(_)))
            || !matches!(self.kind_at(self.current + 2), Some(TokenKind::Colon))
            || !self.is_type_name_at(self.current + 3)
        {
            return false;
        }

        let receiver_type_end = self.skip_type_name_at(self.current + 3);
        if receiver_type_end == self.current + 3
            || !matches!(self.kind_at(receiver_type_end), Some(TokenKind::RParen))
            || !matches!(self.kind_at(receiver_type_end + 1), Some(TokenKind::Dot))
            || !matches!(
                self.kind_at(receiver_type_end + 2),
                Some(TokenKind::Ident(_))
            )
        {
            return false;
        }

        let open_index = self.skip_specifiers_at(receiver_type_end + 3);
        if !matches!(self.kind_at(open_index), Some(TokenKind::LParen)) {
            return false;
        }

        let Some(close_index) = self.find_matching_delimiter_at(open_index) else {
            return false;
        };
        let index = self.skip_effect_specifiers_at(close_index + 1);
        let index = self.skip_optional_type_annotation_at(index);
        self.is_definition_operator_at(index)
    }

    fn is_class_method(&self) -> bool {
        let name_end = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            self.current + 1
        } else if matches!(self.peek_kind(), TokenKind::LParen) {
            let mut index = self.current + 1;
            if !matches!(self.kind_at(index), Some(TokenKind::Ident(_))) {
                return false;
            }
            index += 1;
            while matches!(self.kind_at(index), Some(TokenKind::Dot))
                && matches!(self.kind_at(index + 1), Some(TokenKind::Ident(_)))
            {
                index += 2;
            }
            if !matches!(self.kind_at(index), Some(TokenKind::Colon))
                || !matches!(self.kind_at(index + 1), Some(TokenKind::RParen))
                || !self.kind_at(index + 2).is_some_and(is_member_name_kind)
            {
                return false;
            }
            index + 3
        } else {
            return false;
        };

        let open_index = self.skip_specifiers_at(name_end);
        if !matches!(self.kind_at(open_index), Some(TokenKind::LParen)) {
            return false;
        }

        let Some(close_index) = self.find_matching_delimiter_at(open_index) else {
            return false;
        };
        let index = self.skip_effect_specifiers_at(close_index + 1);
        let index = self.skip_optional_type_annotation_at(index);
        self.is_definition_operator_at(index)
            || matches!(
                self.kind_at(index),
                Some(TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof)
            )
    }

    fn find_matching_delimiter_at(&self, open_index: usize) -> Option<usize> {
        let (open, close) = match self.kind_at(open_index)? {
            TokenKind::LParen => (TokenKind::LParen, TokenKind::RParen),
            TokenKind::LBracket => (TokenKind::LBracket, TokenKind::RBracket),
            TokenKind::LBrace => (TokenKind::LBrace, TokenKind::RBrace),
            _ => return None,
        };
        let mut depth = 0usize;
        let mut index = open_index;
        loop {
            match self.kind_at(index)? {
                kind if *kind == open => depth += 1,
                kind if *kind == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                TokenKind::Eof => return None,
                _ => {}
            }
            index += 1;
        }
    }

    fn is_binding_definition(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Ident(_)) {
            return false;
        }

        let index = self.skip_specifiers_at(self.current + 1);

        if matches!(self.kind_at(index), Some(TokenKind::ColonEqual)) {
            return true;
        }

        let type_end = self.skip_optional_type_annotation_at(index);
        type_end != index && self.is_definition_operator_at(type_end)
    }

    fn is_type_alias_definition(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Ident(_))
            || !matches!(self.kind_at(self.current + 1), Some(TokenKind::ColonEqual))
        {
            return false;
        }

        let start = self.skip_separators_at(self.current + 2);
        if !self.is_type_alias_target_start_at(start) {
            return false;
        }

        let end = self.skip_type_name_at(start);
        end != start && self.is_statement_boundary_at(end)
    }

    fn is_parametric_type_definition(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Ident(_)) {
            return false;
        }

        let after_name = self.skip_specifiers_at(self.current + 1);
        if !matches!(self.kind_at(after_name), Some(TokenKind::LParen)) {
            return false;
        }
        let Some(after_params) = self.skip_type_parameter_list_at(after_name) else {
            return false;
        };
        if !matches!(self.kind_at(after_params), Some(TokenKind::ColonEqual)) {
            return false;
        }
        let body_start = self.skip_separators_at(after_params + 1);
        matches!(
            self.kind_at(body_start),
            Some(TokenKind::Ident(name)) if matches!(name.as_str(), "class" | "struct" | "interface")
        )
    }

    fn skip_type_parameter_list_at(&self, open_index: usize) -> Option<usize> {
        let mut index = open_index + 1;
        if matches!(self.kind_at(index), Some(TokenKind::RParen)) {
            return None;
        }

        loop {
            if !matches!(self.kind_at(index), Some(TokenKind::Ident(_))) {
                return None;
            }
            if !matches!(self.kind_at(index + 1), Some(TokenKind::Colon)) {
                return None;
            }
            if !matches!(self.kind_at(index + 2), Some(TokenKind::Ident(name)) if name == "type") {
                return None;
            }
            index += 3;
            if matches!(self.kind_at(index), Some(TokenKind::Comma)) {
                index += 1;
                continue;
            }
            if matches!(self.kind_at(index), Some(TokenKind::RParen)) {
                return Some(index + 1);
            }
            return None;
        }
    }

    fn is_type_alias_target_start_at(&self, index: usize) -> bool {
        match self.kind_at(index) {
            Some(TokenKind::LBracket | TokenKind::Question | TokenKind::LParen) => true,
            Some(TokenKind::Ident(name)) if is_builtin_type_alias_target_name(name) => true,
            Some(TokenKind::Ident(name)) if name == "tuple" => {
                matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
            }
            Some(TokenKind::Ident(name)) if name == "weak_map" => {
                matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
            }
            Some(TokenKind::Ident(name)) if name == "int_range" => {
                matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
            }
            Some(TokenKind::Ident(name)) if name == "type" => {
                matches!(self.kind_at(index + 1), Some(TokenKind::LBrace))
            }
            _ => false,
        }
    }

    fn is_using_statement(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(name) if name == "using")
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::LBrace))
    }

    fn parse_module_path(&mut self) -> Result<String, VerseError> {
        if self.match_slash() {
            let slash_span = self.previous_span();
            if matches!(self.peek_kind(), TokenKind::RBrace) {
                return Err(VerseError::parse(
                    "expected module path component after `/`",
                    slash_span,
                ));
            }

            let mut path = String::from("/");
            path.push_str(&self.parse_module_path_component()?);

            while self.match_slash() {
                if matches!(self.peek_kind(), TokenKind::RBrace) {
                    return Err(self.error_at_previous("expected module path component after `/`"));
                }
                path.push('/');
                path.push_str(&self.parse_module_path_component()?);
            }

            return Ok(path);
        }

        let mut path = self.parse_module_path_component()?;
        while self.match_dot() {
            let (next, _) = self.consume_ident("expected module path component after `.`")?;
            path.push('.');
            path.push_str(&next);
        }

        Ok(path)
    }

    fn parse_module_path_component(&mut self) -> Result<String, VerseError> {
        let (first, _) = self.consume_ident("expected module path component")?;
        let mut component = first;

        while self.match_dot() {
            let (next, _) = self.consume_ident("expected name after `.` in module path")?;
            component.push('.');
            component.push_str(&next);
        }

        Ok(component)
    }

    fn skip_qualified_name_after_open_at(&self, index: usize) -> Option<usize> {
        let path_end = self.skip_module_path_at(index)?;
        if !matches!(self.kind_at(path_end), Some(TokenKind::Colon))
            || !matches!(self.kind_at(path_end + 1), Some(TokenKind::RParen))
            || !self.kind_at(path_end + 2).is_some_and(is_member_name_kind)
        {
            return None;
        }
        Some(path_end + 3)
    }

    fn skip_module_path_at(&self, index: usize) -> Option<usize> {
        if matches!(self.kind_at(index), Some(TokenKind::Slash)) {
            let mut cursor = self.skip_module_path_component_at(index + 1)?;
            while matches!(self.kind_at(cursor), Some(TokenKind::Slash)) {
                cursor = self.skip_module_path_component_at(cursor + 1)?;
            }
            return Some(cursor);
        }

        self.skip_module_path_component_at(index)
    }

    fn skip_module_path_component_at(&self, index: usize) -> Option<usize> {
        if !matches!(self.kind_at(index), Some(TokenKind::Ident(_))) {
            return None;
        }
        let mut cursor = index + 1;
        while matches!(self.kind_at(cursor), Some(TokenKind::Dot))
            && matches!(self.kind_at(cursor + 1), Some(TokenKind::Ident(_)))
        {
            cursor += 2;
        }
        Some(cursor)
    }

    fn is_class_block_clause(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(name) if name == "block")
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::Colon))
    }

    fn is_for_generator_clause(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(
                self.kind_at(self.current + 1),
                Some(TokenKind::Colon | TokenKind::Arrow)
            )
    }

    fn is_for_range_or_let_clause(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(self.current + 1), Some(TokenKind::ColonEqual))
    }

    fn is_for_do_separator(&self, clause_indent: usize) -> bool {
        matches!(self.peek_kind(), TokenKind::Do) && self.peek().span.column < clause_indent
    }

    fn skip_optional_type_annotation_at(&self, index: usize) -> usize {
        if matches!(self.kind_at(index), Some(TokenKind::Colon)) && self.is_type_name_at(index + 1)
        {
            self.skip_type_name_at(index + 1)
        } else {
            index
        }
    }

    fn skip_effect_specifiers_at(&self, mut index: usize) -> usize {
        while matches!(self.kind_at(index), Some(TokenKind::Less))
            && matches!(self.kind_at(index + 1), Some(TokenKind::Ident(_)))
            && matches!(self.kind_at(index + 2), Some(TokenKind::Greater))
        {
            index += 3;
        }
        index
    }

    fn skip_specifiers_at(&self, index: usize) -> usize {
        let mut index = index;
        loop {
            if matches!(self.kind_at(index), Some(TokenKind::Less))
                && matches!(self.kind_at(index + 1), Some(TokenKind::Ident(_)))
                && matches!(self.kind_at(index + 2), Some(TokenKind::Greater))
            {
                index += 3;
            } else if matches!(self.kind_at(index), Some(TokenKind::Less))
                && matches!(self.kind_at(index + 1), Some(TokenKind::Ident(_)))
                && matches!(self.kind_at(index + 2), Some(TokenKind::LBrace))
                && matches!(self.kind_at(index + 3), Some(TokenKind::Ident(_)))
                && matches!(self.kind_at(index + 4), Some(TokenKind::RBrace))
                && matches!(self.kind_at(index + 5), Some(TokenKind::Greater))
            {
                index += 6;
            } else {
                return index;
            }
        }
    }

    fn skip_separators_at(&self, mut index: usize) -> usize {
        while matches!(
            self.kind_at(index),
            Some(TokenKind::Newline | TokenKind::Semicolon)
        ) {
            index += 1;
        }
        index
    }

    fn is_type_name_at(&self, index: usize) -> bool {
        matches!(
            self.kind_at(index),
            Some(TokenKind::Ident(_))
                | Some(TokenKind::None)
                | Some(TokenKind::LBracket)
                | Some(TokenKind::LParen)
                | Some(TokenKind::Question)
        )
    }

    fn skip_type_name_at(&self, index: usize) -> usize {
        if matches!(self.kind_at(index), Some(TokenKind::Question))
            && self.is_type_name_at(index + 1)
        {
            return self.skip_type_name_at(index + 1);
        }

        if matches!(self.kind_at(index), Some(TokenKind::LBracket)) {
            if matches!(self.kind_at(index + 1), Some(TokenKind::RBracket))
                && self.is_type_name_at(index + 2)
            {
                return self.skip_type_name_at(index + 2);
            }

            if self.is_type_name_at(index + 1) {
                let key_end = self.skip_type_name_at(index + 1);
                if matches!(self.kind_at(key_end), Some(TokenKind::RBracket))
                    && self.is_type_name_at(key_end + 1)
                {
                    return self.skip_type_name_at(key_end + 1);
                }
            }
        }

        if matches!(self.kind_at(index), Some(TokenKind::LParen)) {
            if let Some(end) = self.skip_qualified_name_after_open_at(index + 1) {
                return self.skip_parametric_type_args_after_name_at(end);
            }

            let mut cursor = index + 1;

            if matches!(self.kind_at(cursor), Some(TokenKind::RParen)) {
                return cursor + 1;
            }

            loop {
                if !self.is_type_name_at(cursor) {
                    return index;
                }

                cursor = self.skip_type_name_at(cursor);

                match self.kind_at(cursor) {
                    Some(TokenKind::Comma) => cursor += 1,
                    Some(TokenKind::RParen) => return cursor + 1,
                    _ => return index,
                }
            }
        }

        if matches!(self.kind_at(index), Some(TokenKind::Ident(name)) if name == "tuple")
            && matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
        {
            let mut cursor = index + 2;

            if matches!(self.kind_at(cursor), Some(TokenKind::RParen)) {
                return cursor + 1;
            }

            loop {
                if !self.is_type_name_at(cursor) {
                    return index + 1;
                }

                cursor = self.skip_type_name_at(cursor);

                match self.kind_at(cursor) {
                    Some(TokenKind::Comma) => cursor += 1,
                    Some(TokenKind::RParen) => return cursor + 1,
                    _ => return index + 1,
                }
            }
        }

        if matches!(self.kind_at(index), Some(TokenKind::Ident(name)) if name == "weak_map")
            && matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
        {
            let key_index = index + 2;
            if !self.is_type_name_at(key_index) {
                return index + 1;
            }
            let key_end = self.skip_type_name_at(key_index);
            if !matches!(self.kind_at(key_end), Some(TokenKind::Comma))
                || !self.is_type_name_at(key_end + 1)
            {
                return index + 1;
            }
            let value_end = self.skip_type_name_at(key_end + 1);
            if matches!(self.kind_at(value_end), Some(TokenKind::RParen)) {
                return value_end + 1;
            }
            return index + 1;
        }

        if matches!(self.kind_at(index), Some(TokenKind::Ident(name)) if name == "int_range")
            && matches!(self.kind_at(index + 1), Some(TokenKind::LParen))
        {
            let Some(min_end) = self.skip_int_range_bound_at(index + 2) else {
                return index + 1;
            };
            if !matches!(self.kind_at(min_end), Some(TokenKind::Comma)) {
                return index + 1;
            }
            let Some(max_end) = self.skip_int_range_bound_at(min_end + 1) else {
                return index + 1;
            };
            if matches!(self.kind_at(max_end), Some(TokenKind::RParen)) {
                return max_end + 1;
            }
            return index + 1;
        }

        if matches!(self.kind_at(index), Some(TokenKind::Ident(name)) if name == "type")
            && matches!(self.kind_at(index + 1), Some(TokenKind::LBrace))
            && let Some(close_index) = self.find_matching_delimiter_at(index + 1)
        {
            return close_index + 1;
        }

        let mut end = index + 1;
        while matches!(self.kind_at(end), Some(TokenKind::Dot))
            && matches!(self.kind_at(end + 1), Some(TokenKind::Ident(_)))
        {
            end += 2;
        }

        self.skip_parametric_type_args_after_name_at(end)
    }

    fn skip_int_range_bound_at(&self, index: usize) -> Option<usize> {
        let index = if matches!(self.kind_at(index), Some(TokenKind::Minus)) {
            index + 1
        } else {
            index
        };
        matches!(
            self.kind_at(index),
            Some(TokenKind::Number {
                value: NumberLiteral::Int(_),
                kind: NumberKind::Int
            })
        )
        .then_some(index + 1)
    }

    fn skip_parametric_type_args_after_name_at(&self, end: usize) -> usize {
        if !matches!(self.kind_at(end), Some(TokenKind::LParen)) {
            return end;
        }

        let mut cursor = end + 1;

        if matches!(self.kind_at(cursor), Some(TokenKind::RParen)) {
            return cursor + 1;
        }

        loop {
            if !self.is_type_name_at(cursor) {
                return end;
            }

            cursor = self.skip_type_name_at(cursor);

            match self.kind_at(cursor) {
                Some(TokenKind::Comma) => cursor += 1,
                Some(TokenKind::RParen) => return cursor + 1,
                _ => return end,
            }
        }
    }

    fn is_definition_operator_at(&self, index: usize) -> bool {
        matches!(
            self.kind_at(index),
            Some(TokenKind::ColonEqual) | Some(TokenKind::Equal)
        )
    }

    fn is_statement_boundary_at(&self, index: usize) -> bool {
        matches!(
            self.kind_at(index),
            Some(TokenKind::Newline | TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof)
        )
    }

    fn at_statement_boundary(&self) -> bool {
        self.is_statement_boundary_at(self.current)
    }

    fn skip_separators(&mut self) -> usize {
        let mut count = 0;
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
            self.advance();
            count += 1;
        }
        count
    }

    fn consume_ident(&mut self, message: &str) -> Result<(String, Span), VerseError> {
        let (name, span) = self.consume_raw_ident(message)?;
        if name == "_" {
            return Err(VerseError::parse(
                "reserved identifier `_` cannot be used as a name",
                span,
            ));
        }
        Ok((name, span))
    }

    fn consume_raw_ident(&mut self, message: &str) -> Result<(String, Span), VerseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Ident(name) => Ok((name, token.span)),
            _ => Err(VerseError::parse(message, token.span)),
        }
    }

    fn consume_member_name(&mut self, message: &str) -> Result<(String, Span), VerseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Ident(name) => Ok((name, token.span)),
            kind => keyword_member_name(&kind)
                .map(|name| (name.to_string(), token.span))
                .ok_or_else(|| VerseError::parse(message, token.span)),
        }
    }

    fn consume_lparen(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_lparen() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_rparen(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_rparen() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_rbracket(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_rbracket() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_lbrace(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_lbrace() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_rbrace(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_rbrace() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_definition_operator(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_colon_equal() || self.match_equal() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_equal(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_equal() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_assignment_operator(&mut self, message: &str) -> Result<AssignOp, VerseError> {
        if self.match_equal() {
            Ok(AssignOp::Assign)
        } else if self.match_plus_equal() {
            Ok(AssignOp::AddAssign)
        } else if self.match_minus_equal() {
            Ok(AssignOp::SubAssign)
        } else if self.match_star_equal() {
            Ok(AssignOp::MulAssign)
        } else if self.match_slash_equal() {
            Ok(AssignOp::DivAssign)
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_fat_arrow(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_fat_arrow() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_colon(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_colon() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_colon_equal(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_colon_equal() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_dot(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_dot() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_do(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_do() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_greater(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_greater() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_for_value_separator(&mut self, message: &str) -> Result<Span, VerseError> {
        if self.match_colon() || self.match_colon_equal() {
            Ok(self.previous_span())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn consume_then_colon(&mut self, message: &str) -> Result<Span, VerseError> {
        if matches!(self.peek_kind(), TokenKind::Ident(name) if name == "then") {
            self.advance();
            self.consume_colon("expected `:` after `then`")
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn match_if(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::If))
    }

    fn match_var(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Var))
    }

    fn match_set(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Set))
    }

    fn match_break(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Break))
    }

    fn match_return(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Return))
    }

    fn match_defer(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Defer))
    }

    fn match_else(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Else))
    }

    fn match_do(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Do))
    }

    fn match_or(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Or))
    }

    fn match_and(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::And))
    }

    fn match_not(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Not))
    }

    fn match_dot(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Dot))
    }

    fn match_dot_dot(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::DotDot))
    }

    fn match_fat_arrow(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::FatArrow))
    }

    fn match_arrow(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Arrow))
    }

    fn match_question(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Question))
    }

    fn match_at(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::At))
    }

    fn match_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Equal))
    }

    fn match_equal_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::EqualEqual))
    }

    fn match_not_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::NotEqual))
    }

    fn match_less(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Less))
    }

    fn match_less_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::LessEqual))
    }

    fn match_greater(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Greater))
    }

    fn match_greater_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::GreaterEqual))
    }

    fn match_plus(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Plus))
    }

    fn match_plus_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::PlusEqual))
    }

    fn match_minus_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::MinusEqual))
    }

    fn match_star_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::StarEqual))
    }

    fn match_slash_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::SlashEqual))
    }

    fn match_minus(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Minus))
    }

    fn match_star(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Star))
    }

    fn match_slash(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Slash))
    }

    fn match_percent(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Percent))
    }

    fn match_lparen(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::LParen))
    }

    fn match_rparen(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::RParen))
    }

    fn match_lbracket(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::LBracket))
    }

    fn match_rbracket(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::RBracket))
    }

    fn match_lbrace(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::LBrace))
    }

    fn match_rbrace(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::RBrace))
    }

    fn match_comma(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Comma))
    }

    fn match_colon(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::Colon))
    }

    fn match_colon_equal(&mut self) -> bool {
        self.match_kind(|kind| matches!(kind, TokenKind::ColonEqual))
    }

    fn match_kind(&mut self, predicate: impl FnOnce(&TokenKind) -> bool) -> bool {
        if predicate(self.peek_kind()) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_rbrace(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::RBrace)
    }

    fn check_lparen(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::LParen)
    }

    fn check_rparen(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::RParen)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn kind_at(&self, index: usize) -> Option<&TokenKind> {
        self.tokens.get(index).map(|token| &token.kind)
    }

    fn error_at_current(&self, message: &str) -> VerseError {
        VerseError::parse(message, self.peek().span)
    }

    fn error_at_previous(&self, message: &str) -> VerseError {
        VerseError::parse(message, self.previous_span())
    }

    fn previous_span(&self) -> Span {
        self.previous().span
    }
}

fn validate_effect_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if name == "no_rollback" {
        return Err(VerseError::parse(
            "`no_rollback` effect cannot be manually specified",
            span,
        ));
    }

    if is_known_effect_specifier(name) {
        Ok(())
    } else {
        Err(VerseError::parse(
            format!("unknown effect specifier `{name}`"),
            span,
        ))
    }
}

fn is_known_effect_specifier(name: &str) -> bool {
    matches!(
        name,
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

fn validate_function_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if name == "no_rollback" {
        return Err(VerseError::parse(
            "`no_rollback` effect cannot be manually specified",
            span,
        ));
    }

    if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Ok(())
    } else {
        Err(VerseError::parse(
            format!("unknown function specifier `{name}`"),
            span,
        ))
    }
}

fn validate_data_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if is_known_access_specifier(name) || matches!(name, "localizes" | "native" | "scoped") {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported data specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown data specifier `{name}`"),
            span,
        ))
    }
}

fn validate_enum_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if matches!(name, "open" | "closed" | "persistable") {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported enum specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown enum specifier `{name}`"),
            span,
        ))
    }
}

fn validate_struct_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if matches!(name, "persistable" | "computes") {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported struct specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown struct specifier `{name}`"),
            span,
        ))
    }
}

fn is_known_declaration_specifier(name: &str) -> bool {
    matches!(
        name,
        "public"
            | "protected"
            | "private"
            | "internal"
            | "epic_internal"
            | "native"
            | "native_callable"
            | "override"
            | "abstract"
            | "final"
            | "final_super"
            | "unique"
            | "concrete"
            | "persistable"
            | "constructor"
            | "localizes"
    )
}

fn validate_class_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if matches!(
        name,
        "abstract"
            | "final"
            | "final_super"
            | "unique"
            | "concrete"
            | "castable"
            | "computes"
            | "persistable"
            | "epic_internal"
    ) || is_known_access_specifier(name)
    {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported class specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown class specifier `{name}`"),
            span,
        ))
    }
}

fn validate_class_field_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if matches!(name, "override" | "final" | "localizes" | "native")
        || is_known_access_specifier(name)
    {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported class field specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown class field specifier `{name}`"),
            span,
        ))
    }
}

fn validate_var_field_specifier(name: &str, span: Span) -> Result<(), VerseError> {
    if is_known_access_specifier(name) {
        Ok(())
    } else if is_known_effect_specifier(name) || is_known_declaration_specifier(name) {
        Err(VerseError::parse(
            format!("unsupported var field specifier `{name}`"),
            span,
        ))
    } else {
        Err(VerseError::parse(
            format!("unknown var field specifier `{name}`"),
            span,
        ))
    }
}

fn is_known_access_specifier(name: &str) -> bool {
    matches!(name, "public" | "protected" | "private" | "internal")
}

fn validate_field_attribute(name: &str, span: Span) -> Result<(), VerseError> {
    if name == "editable" {
        Ok(())
    } else {
        Err(VerseError::parse(
            format!("unknown field attribute `@{name}`"),
            span,
        ))
    }
}

fn keyword_member_name(kind: &TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::If => Some("if"),
        TokenKind::Else => Some("else"),
        TokenKind::True => Some("true"),
        TokenKind::False => Some("false"),
        TokenKind::None => Some("none"),
        TokenKind::Var => Some("var"),
        TokenKind::Set => Some("set"),
        TokenKind::Loop => Some("loop"),
        TokenKind::For => Some("for"),
        TokenKind::Do => Some("do"),
        TokenKind::Break => Some("break"),
        TokenKind::Return => Some("return"),
        TokenKind::Defer => Some("defer"),
        TokenKind::And => Some("and"),
        TokenKind::Or => Some("or"),
        TokenKind::Not => Some("not"),
        _ => None,
    }
}

fn is_member_name_kind(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Ident(_)) || keyword_member_name(kind).is_some()
}

fn is_builtin_type_alias_target_name(name: &str) -> bool {
    matches!(
        name,
        "number"
            | "int"
            | "float"
            | "rational"
            | "bool"
            | "logic"
            | "string"
            | "message"
            | "char"
            | "char8"
            | "char32"
            | "void"
            | "any"
            | "comparable"
            | "array"
            | "function"
            | "diagnostic"
            | "entity"
            | "component"
            | "tag"
            | "session"
            | "player"
            | "agent"
            | "team"
            | "event"
            | "task"
            | "generator"
            | "castable_subtype"
            | "concrete_subtype"
            | "classifiable_subset"
            | "modifier"
            | "modifier_stack"
            | "result"
            | "awaitable"
            | "signalable"
            | "listenable"
            | "subscribable"
    )
}

fn concurrent_op_from_name(name: &str) -> Option<ConcurrentOp> {
    match name {
        "sync" => Some(ConcurrentOp::Sync),
        "race" => Some(ConcurrentOp::Race),
        "rush" => Some(ConcurrentOp::Rush),
        "branch" => Some(ConcurrentOp::Branch),
        _ => None,
    }
}

fn stmt_consumed_trailing_separator(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::ParametricType { expr, .. }
        | StmtKind::Var { expr, .. }
        | StmtKind::Expr(expr) => expr_consumed_trailing_separator(expr),
        StmtKind::Set { expr, .. } | StmtKind::Return(expr) => {
            expr_consumed_trailing_separator(expr)
        }
        StmtKind::ExtensionMethod(method) => method
            .method
            .body
            .as_ref()
            .is_some_and(expr_consumed_trailing_separator),
        StmtKind::TypeAlias { .. } => false,
        StmtKind::Using { .. } => false,
        StmtKind::Break => false,
        StmtKind::Defer(expr) => expr_consumed_trailing_separator(expr),
    }
}

fn case_pattern_span(pattern: &CasePattern) -> Span {
    match pattern {
        CasePattern::Wildcard { span } => *span,
        CasePattern::Expr(expr) => expr.span,
    }
}

fn archetype_entries_have_field(entries: &[ArchetypeEntry], name: &str) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            ArchetypeEntry::Field(ArchetypeField { name: field_name, .. }) if field_name == name
        )
    })
}

fn archetype_entry_span(entry: &ArchetypeEntry) -> Span {
    match entry {
        ArchetypeEntry::Field(field) => field.span,
        ArchetypeEntry::Let(binding) => binding.span,
        ArchetypeEntry::Block(body) => body.span,
        ArchetypeEntry::ConstructorCall(call) => call.span,
    }
}

fn expr_consumed_trailing_separator(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::EnumDefinition { block, .. }
        | ExprKind::StructDefinition { block, .. }
        | ExprKind::ClassDefinition { block, .. }
        | ExprKind::InterfaceDefinition { block, .. }
        | ExprKind::ModuleDefinition { block, .. }
        | ExprKind::Archetype { block, .. } => *block,
        ExprKind::Case { .. } | ExprKind::ColonBlock(_) => true,
        ExprKind::Function { body, .. }
        | ExprKind::Loop { body }
        | ExprKind::For { body, .. }
        | ExprKind::Profile { body, .. }
        | ExprKind::Spawn { body }
        | ExprKind::Concurrent { body, .. } => expr_consumed_trailing_separator(body),
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            expr_consumed_trailing_separator(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(expr_consumed_trailing_separator)
        }
        _ => false,
    }
}

fn is_archetype_callee(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::QualifiedName { .. } => true,
        ExprKind::Member { object, .. } => is_archetype_callee(object),
        ExprKind::Call { callee, args } => {
            if args.is_empty() {
                return is_named_type_constructor(callee, "event");
            }
            is_type_constructor_callee(callee)
                && args.iter().all(|arg| {
                    let CallArg::Positional(expr) = arg else {
                        return false;
                    };
                    is_type_argument_expr(expr)
                })
        }
        _ => false,
    }
}

fn is_type_constructor_callee(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::QualifiedName { .. } => true,
        ExprKind::Member { object, .. } => is_type_constructor_callee(object),
        _ => false,
    }
}

fn is_type_argument_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::QualifiedName { .. } => true,
        ExprKind::Member { object, .. } => is_type_argument_expr(object),
        ExprKind::Call { callee, args } => {
            is_type_constructor_callee(callee)
                && (!args.is_empty() || is_zero_arg_type_constructor(callee))
                && args.iter().all(|arg| {
                    let CallArg::Positional(expr) = arg else {
                        return false;
                    };
                    is_type_argument_expr(expr)
                })
        }
        _ => false,
    }
}

fn is_named_type_constructor(expr: &Expr, expected: &str) -> bool {
    matches!(&expr.kind, ExprKind::Ident(name) if name == expected)
}

fn is_zero_arg_type_constructor(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Ident(name) if matches!(name.as_str(), "event" | "generator" | "subscribable")
    )
}
