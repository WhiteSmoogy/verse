use super::semantic_function::merge_type_param_lists;
use super::*;

impl Checker {
    pub(super) fn check_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        let value_type = self.check_expr_inner(expr)?;
        self.semantic_facts
            .record_expression_type(expr.span, value_type.clone());
        Ok(value_type)
    }

    fn check_expr_inner(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        match &expr.kind {
            ExprKind::Number { kind, .. } => match kind {
                NumberKind::Int => Ok(Type::Int),
                NumberKind::Float => Ok(Type::Float),
            },
            ExprKind::Char { kind, .. } => match kind {
                CharacterKind::Char => Ok(Type::Char),
                CharacterKind::Char32 => Ok(Type::Char32),
            },
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::String(_) => Ok(Type::String),
            ExprKind::InterpolatedString(parts) => {
                for part in parts {
                    if let InterpolatedStringPart::Expr(expr) = part {
                        self.check_expr(expr)?;
                    }
                }
                Ok(Type::String)
            }
            ExprKind::None => Ok(Type::None),
            ExprKind::Ident(name) => self.check_ident(name, expr.span, false, false),
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::Unary { op, expr } => self.check_unary(*op, expr),
            ExprKind::Binary { op, .. } if is_failure_binary_op(*op) => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::Binary { left, op, right } => self.check_binary(left, *op, right),
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.push_scope();
                let then_result = (|| {
                    self.check_if_condition(condition)?;
                    self.check_expr(then_branch)
                })();
                self.pop_scope();
                let then_type = then_result?;

                let else_type = if let Some(else_branch) = else_branch {
                    self.check_expr(else_branch)?
                } else {
                    Type::None
                };

                unify_types(&then_type, &else_type, expr.span)
            }
            ExprKind::FailureBind { .. } | ExprKind::FailureSequence(_) => {
                Err(VerseError::check_at(
                    "failure binding is only valid in an `if` condition",
                    expr.span,
                ))
            }
            ExprKind::Set { target, op, expr } => {
                self.check_set_expression(target, *op, expr, false, expr.span)
            }
            ExprKind::Var {
                name,
                annotation,
                expr,
            } => self.check_var_expression(name, annotation, expr, false, expr.span),
            ExprKind::TypeLiteral { expr } => {
                let value_type = self.check_expr(expr)?;
                Ok(Type::TypeValueOf(Box::new(value_type)))
            }
            ExprKind::TypeAnnotationLiteral { annotation } => {
                let value_type = self.type_name_to_type(annotation)?;
                Ok(Type::TypeValueOf(Box::new(value_type)))
            }
            ExprKind::External => Ok(Type::Unknown),
            ExprKind::Case { subject, arms } => self.check_case(subject, arms, expr.span),
            ExprKind::Loop { body } => {
                if !loop_body_has_non_break_statement(body) {
                    return Err(VerseError::check_at(
                        "`loop` body must contain at least one non-break statement",
                        body.span,
                    ));
                }
                self.break_depth += 1;
                self.check_expr(body)?;
                self.break_depth -= 1;
                Ok(Type::None)
            }
            ExprKind::For { clauses, body } => self.check_for(clauses, body),
            ExprKind::Profile { description, body } => {
                let description_type = self.check_expr(description)?;
                self.ensure_expr_assignable(&Type::String, &description_type, description, || {
                    format!("profile description expected `string`, got `{description_type}`")
                })?;
                self.check_expr(body)
            }
            ExprKind::Spawn { body } => self.check_spawn(body),
            ExprKind::Concurrent { op, body } => {
                let value_type = self.check_concurrent(*op, body, expr.span)?;
                self.mark_async_expression();
                Ok(value_type)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                if statements.is_empty() {
                    self.warn_empty_block(expr.span);
                }
                self.push_scope();
                let result = self.check_statements(statements);
                self.pop_scope();
                result
            }
            ExprKind::Function {
                params,
                effects,
                return_type,
                body,
            } => self.without_enclosing_failure_context(|checker| {
                checker.check_function(params, effects, return_type.as_ref(), body)
            }),
            ExprKind::Call { callee, args } => {
                if let Some(value_type) =
                    self.check_type_function_value_call(callee, args, expr.span)?
                {
                    self.semantic_facts
                        .record_static_type_function_call(expr.span);
                    return Ok(value_type);
                }

                let callee_type = self.check_callee_expr(callee)?;
                if let Type::ParametricType { name, kind, .. } = &callee_type {
                    let type_args = self.check_parametric_type_call_args(name, args, expr.span)?;
                    let instance = self.instantiate_parametric_type(name, &type_args, expr.span)?;
                    if *kind == ParametricTypeKind::Alias {
                        return Ok(Type::TypeValueOf(Box::new(instance)));
                    }
                    return match (kind, instance) {
                        (ParametricTypeKind::Struct, Type::Struct(name)) => {
                            Ok(Type::StructType(name))
                        }
                        (ParametricTypeKind::Class, Type::Class(name)) => Ok(Type::ClassType(name)),
                        (ParametricTypeKind::Interface, Type::Interface(name)) => {
                            Ok(Type::InterfaceType(name))
                        }
                        (_, other) => Err(VerseError::check_at(
                            format!("parametric type `{name}` produced `{other}`"),
                            expr.span,
                        )),
                    };
                }

                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.check_expr(call_arg_expr(arg))?);
                }

                if is_length_member_callee(callee)
                    && matches!(callee_type, Type::Int | Type::Unknown | Type::Any)
                {
                    self.check_call_arguments(
                        (Some(0), None),
                        Some(&[]),
                        None,
                        args,
                        &arg_types,
                        expr.span,
                    )?;
                    return Ok(Type::Int);
                }

                if is_shuffle_callee(callee) && is_shuffle_function_type(&callee_type) {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_shuffle_call(args, &arg_types, expr.span);
                }

                if is_concatenate_callee(callee) && is_concatenate_function_type(&callee_type) {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_concatenate_call(args, &arg_types, expr.span);
                }

                if is_make_classifiable_subset_callee(callee)
                    && is_make_classifiable_subset_function_type(&callee_type)
                {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self
                        .check_make_classifiable_subset_call(args, &arg_types, expr.span, false);
                }

                if is_make_classifiable_subset_var_callee(callee)
                    && is_make_classifiable_subset_var_function_type(&callee_type)
                {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self
                        .check_make_classifiable_subset_call(args, &arg_types, expr.span, true);
                }

                if is_make_result_callee(callee) {
                    self.ensure_callee_type_effects_allowed(&callee_type, expr.span)?;
                    return self.check_make_result_call(callee, args, &arg_types, expr.span);
                }

                match callee_type {
                    Type::Function {
                        arity,
                        arity_range,
                        effects,
                        mut param_types,
                        mut param_specs,
                        return_type,
                    } => {
                        if has_effect(&effects, "decides") {
                            return Err(VerseError::check_at(
                                "functions with `<decides>` must be called with `[]`",
                                expr.span,
                            ));
                        }
                        if self.in_failure_context() {
                            ensure_callable_in_failure_context(&effects, expr.span)?;
                        }
                        self.ensure_callable_in_async_context(&effects, expr.span)?;
                        self.ensure_current_function_allows_call_effects(&effects, expr.span)?;
                        let mut return_type = *return_type;
                        if let Some(inferred) = self
                            .infer_function_type_params(
                                param_types.as_deref(),
                                Some(&return_type),
                                &arg_types,
                            )
                            .filter(|inferred| !inferred.is_empty())
                        {
                            self.ensure_inferred_type_param_constraints(
                                param_types.as_deref(),
                                Some(&return_type),
                                &inferred,
                                expr.span,
                            )?;
                            if let Some(types) = param_types.as_mut() {
                                for value_type in types {
                                    *value_type = self.substitute_type_params_runtime(
                                        value_type, &inferred, expr.span,
                                    )?;
                                }
                            }
                            if let Some(specs) = param_specs.as_mut() {
                                for spec in specs {
                                    *spec = self.substitute_param_spec_runtime(
                                        spec, &inferred, expr.span,
                                    )?;
                                }
                            }
                            return_type = self.substitute_type_params_runtime(
                                &return_type,
                                &inferred,
                                expr.span,
                            )?;
                        }
                        self.check_call_arguments(
                            (arity, arity_range),
                            param_types.as_deref(),
                            param_specs.as_deref(),
                            args,
                            &arg_types,
                            expr.span,
                        )?;
                        Ok(return_type)
                    }
                    Type::Overload(overloads) => self.check_overloaded_call(
                        &overloads,
                        false,
                        self.in_failure_context(),
                        args,
                        &arg_types,
                        expr.span,
                    ),
                    Type::Tuple(items) => {
                        self.check_tuple_access(&items, args, &arg_types, expr.span)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot call value of type `{other}`"),
                        callee.span,
                    )),
                }
            }
            ExprKind::BracketCall { callee, args } => self.check_bracket_call(expr, callee, args),
            ExprKind::Array(items) => {
                let mut item_type = Type::Unknown;
                let mut pending_empty_options = Vec::new();
                for item in items {
                    self.merge_collection_item_type(
                        &mut item_type,
                        &mut pending_empty_options,
                        item,
                    )?;
                }
                finalize_collection_item_type(&mut item_type, &mut pending_empty_options)?;
                Ok(Type::Array(Box::new(item_type)))
            }
            ExprKind::Map(entries) => {
                let mut key_type = Type::Unknown;
                let mut value_type = Type::Unknown;
                let mut pending_empty_keys = Vec::new();
                let mut pending_empty_values = Vec::new();
                for (key, value) in entries {
                    self.merge_collection_item_type(&mut key_type, &mut pending_empty_keys, key)?;
                    self.merge_collection_item_type(
                        &mut value_type,
                        &mut pending_empty_values,
                        value,
                    )?;
                }
                finalize_collection_item_type(&mut key_type, &mut pending_empty_keys)?;
                finalize_collection_item_type(&mut value_type, &mut pending_empty_values)?;
                if !entries.is_empty() {
                    ensure_comparable_key(&key_type, &self.struct_types, expr.span)?;
                }
                Ok(Type::Map(Box::new(key_type), Box::new(value_type)))
            }
            ExprKind::EnumDefinition { .. } => Err(VerseError::check_at(
                "enum definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::StructDefinition { .. } => Err(VerseError::check_at(
                "struct definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::ClassDefinition { .. } => Err(VerseError::check_at(
                "class definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::InterfaceDefinition { .. } => Err(VerseError::check_at(
                "interface definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::ModuleDefinition { .. } => Err(VerseError::check_at(
                "module definitions must be direct right-hand side of definitions",
                expr.span,
            )),
            ExprKind::Archetype {
                callee, entries, ..
            } => self.check_archetype(callee, entries),
            ExprKind::Tuple(items) => {
                let mut item_types = Vec::with_capacity(items.len());
                for item in items {
                    item_types.push(self.check_expr(item)?);
                }
                Ok(Type::Tuple(item_types))
            }
            ExprKind::Option(value) => {
                let item_type = if let Some(value) = value {
                    self.check_failure_expr(value)?
                } else {
                    Type::Unknown
                };
                Ok(Type::Option(Box::new(item_type)))
            }
            ExprKind::UnwrapOption(_) => {
                self.ensure_failable_expression_allowed(expr.span)?;
                self.check_failure_expr(expr)
            }
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, expr.span, false, false)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => self.check_qualified_member_expr(object, qualifier, name, expr.span, false),
            ExprKind::Member { object, name } => {
                self.check_member_expr(object, name, expr.span, false)
            }
            ExprKind::Index { collection, index } => {
                self.ensure_failable_expression_allowed(expr.span)?;
                let collection_type = self.check_expr(collection)?;
                let index_type = self.check_expr(index)?;
                match collection_type {
                    Type::Array(item) => {
                        ensure_int_index_type(&index_type, "array index", index.span)?;
                        Ok(*item)
                    }
                    Type::String => {
                        ensure_int_index_type(&index_type, "string index", index.span)?;
                        Ok(Type::Char)
                    }
                    Type::Map(key, value) | Type::WeakMap(key, value) => {
                        self.check_map_like_lookup(&key, &value, &index_type, index)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot index value of type `{other}`"),
                        collection.span,
                    )),
                }
            }
        }
    }

    pub(super) fn check_callee_expr(&mut self, callee: &Expr) -> Result<Type, VerseError> {
        match &callee.kind {
            ExprKind::Ident(name) => self.check_ident(name, callee.span, true, true),
            ExprKind::Member { object, name } => {
                self.check_member_expr(object, name, callee.span, true)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => self.check_qualified_member_expr(object, qualifier, name, callee.span, true),
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, callee.span, true, true)
            }
            _ => self.check_expr(callee),
        }
    }

    pub(super) fn check_failure_callee_expr(&mut self, callee: &Expr) -> Result<Type, VerseError> {
        match &callee.kind {
            ExprKind::Ident(name) => self.check_ident(name, callee.span, true, true),
            ExprKind::Member { object, name } => {
                self.check_failure_member_expr(object, name, callee.span, true)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => {
                self.check_failure_qualified_member_expr(object, qualifier, name, callee.span, true)
            }
            ExprKind::QualifiedName { qualifier, name } => {
                self.check_qualified_name(qualifier, name, callee.span, true, true)
            }
            _ if is_failable_condition_expr(callee) => self.check_failure_expr(callee),
            _ => self.check_expr(callee),
        }
    }

    pub(super) fn check_ident(
        &mut self,
        name: &str,
        span: Span,
        allow_overload: bool,
        _allow_parametric_alias_value: bool,
    ) -> Result<Type, VerseError> {
        let Some(symbol) = self.lookup_accessible(name, span)? else {
            if let Some(value_type) = self.resolve_type_param(name) {
                return Ok(Type::TypeValueOf(Box::new(value_type)));
            }
            if let Some(alias_name) = self.resolve_type_alias_reference(name, span)? {
                let value_type = self.resolve_type_alias(&alias_name, &mut Vec::new())?;
                return Ok(Type::TypeValueOf(Box::new(value_type)));
            }
            if is_builtin_class_type_name(name) {
                return Ok(Type::ClassType(name.to_string()));
            }
            if let Some(value_type) = Self::builtin_type_value(name) {
                return Ok(Type::TypeValueOf(Box::new(value_type)));
            }
            if let Some(qualified) = self.resolve_parametric_type_reference(name)
                && let Some(info) = self.parametric_types.get(&qualified)
            {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::ParametricType {
                    name: qualified,
                    params: info.params.iter().map(|param| param.name.clone()).collect(),
                    kind: info.kind,
                });
            }
            return Err(VerseError::check_at(
                format!("undefined name `{name}`"),
                span,
            ));
        };
        if matches!(symbol.value_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded function `{name}` must be called"),
                span,
            ));
        }
        Ok(symbol.value_type)
    }

    pub(super) fn check_qualified_name(
        &mut self,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
        _allow_parametric_alias_value: bool,
    ) -> Result<Type, VerseError> {
        if qualifier != "super" {
            let Some(module_info) = self.module_types.get(qualifier) else {
                return Err(VerseError::check_at(
                    format!("unknown qualifier `{qualifier}`"),
                    span,
                ));
            };
            let Some(value_type) = module_info.members.get(name) else {
                let qualified = format!("{qualifier}.{name}");
                if let Some(alias_name) = self.resolve_type_alias_reference(&qualified, span)? {
                    let value_type = self.resolve_type_alias(&alias_name, &mut Vec::new())?;
                    return Ok(Type::TypeValueOf(Box::new(value_type)));
                }
                return Err(VerseError::check_at(
                    format!("module `{qualifier}` has no member `{name}`"),
                    span,
                ));
            };
            let access = module_info
                .member_access
                .get(name)
                .copied()
                .unwrap_or(AccessLevel::Internal);
            self.ensure_module_member_accessible(qualifier, access, name, span)?;
            if matches!(value_type, Type::Overload(_)) && !allow_overload {
                return Err(VerseError::check_at(
                    format!("overloaded function `{name}` must be called"),
                    span,
                ));
            }
            return Ok(value_type.clone());
        }
        let Some(symbol) = self.lookup("super") else {
            return Err(VerseError::check_at("undefined name `super`", span));
        };
        let Type::ClassType(base_name) = symbol.value_type else {
            return Err(VerseError::check_at(
                "qualifier `super` is not a class type",
                span,
            ));
        };
        let Some(info) = self.struct_types.get(&base_name) else {
            return Err(VerseError::check_at(
                format!("unknown class `{base_name}`"),
                span,
            ));
        };
        let methods = info
            .methods
            .iter()
            .filter(|method| method.name == name)
            .collect::<Vec<_>>();
        if methods.is_empty() {
            return Err(VerseError::check_at(
                format!("class `{base_name}` has no method `{name}`"),
                span,
            ));
        }
        for method in &methods {
            let owner = method.owner.as_deref().unwrap_or(&base_name);
            self.ensure_aggregate_member_accessible(
                owner,
                method.access,
                &method.scopes,
                name,
                "method",
                span,
            )?;
        }
        let method_type = method_group_type(methods).expect("non-empty method group should type");
        if matches!(method_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded method `{name}` must be called"),
                span,
            ));
        }
        Ok(method_type)
    }

    pub(super) fn check_member_expr(
        &mut self,
        object: &Expr,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let ExprKind::Ident(enum_name) = &object.kind
            && let Some(info) = self.enum_types.get(enum_name)
        {
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        let object_type = if is_failable_condition_expr(object) {
            self.ensure_failable_expression_allowed(span)?;
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_member(&object_type, name, span, allow_extension_method)
    }

    pub(super) fn check_failure_member_expr(
        &mut self,
        object: &Expr,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let ExprKind::Ident(enum_name) = &object.kind
            && let Some(info) = self.enum_types.get(enum_name)
        {
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        let object_type = if is_failable_condition_expr(object) {
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_member(&object_type, name, span, allow_extension_method)
    }

    pub(super) fn check_qualified_member_expr(
        &mut self,
        object: &Expr,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        let object_type = if is_failable_condition_expr(object) {
            self.ensure_failable_expression_allowed(span)?;
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_qualified_member(&object_type, qualifier, name, span, allow_overload)
    }

    pub(super) fn check_failure_qualified_member_expr(
        &mut self,
        object: &Expr,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        let object_type = if is_failable_condition_expr(object) {
            self.check_failure_expr(object)?
        } else {
            self.check_expr(object)?
        };
        self.check_qualified_member(&object_type, qualifier, name, span, allow_overload)
    }

    pub(super) fn check_qualified_member(
        &mut self,
        object_type: &Type,
        qualifier: &str,
        name: &str,
        span: Span,
        allow_overload: bool,
    ) -> Result<Type, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.check_qualified_member(&supertype, qualifier, name, span, allow_overload);
        }

        let (label, aggregate_name, methods) = match object_type {
            Type::Class(class_name) => {
                let Some(info) = self.struct_types.get(class_name) else {
                    return Err(VerseError::check_at(
                        format!("unknown class `{class_name}`"),
                        span,
                    ));
                };
                ("class", class_name.as_str(), info.methods.as_slice())
            }
            Type::Interface(interface_name) => {
                let Some(info) = self.interface_types.get(interface_name) else {
                    return Err(VerseError::check_at(
                        format!("unknown interface `{interface_name}`"),
                        span,
                    ));
                };
                (
                    "interface",
                    interface_name.as_str(),
                    info.methods.as_slice(),
                )
            }
            other => {
                if let Some(method_type) =
                    self.qualified_extension_member_type(object_type, qualifier, name, span)?
                {
                    if !allow_overload {
                        if let Some(return_type) = self.type_value_extension_accessor_return_type(
                            object_type,
                            &method_type,
                            span,
                        )? {
                            return Ok(return_type);
                        }
                        return Err(VerseError::check_at(
                            format!("extension method `({qualifier}:){name}` must be called"),
                            span,
                        ));
                    }
                    return Ok(method_type);
                }
                return Err(VerseError::check_at(
                    format!("cannot use qualified member on type `{other}`"),
                    span,
                ));
            }
        };

        let methods = methods
            .iter()
            .filter(|method| method.name == name && method_has_qualifier(method, qualifier))
            .collect::<Vec<_>>();
        if methods.is_empty() {
            if let Some(method_type) =
                self.qualified_extension_member_type(object_type, qualifier, name, span)?
            {
                if !allow_overload {
                    if let Some(return_type) = self.type_value_extension_accessor_return_type(
                        object_type,
                        &method_type,
                        span,
                    )? {
                        return Ok(return_type);
                    }
                    return Err(VerseError::check_at(
                        format!("extension method `({qualifier}:){name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            return Err(VerseError::check_at(
                format!("{label} `{aggregate_name}` has no method `({qualifier}:){name}`"),
                span,
            ));
        }
        for method in &methods {
            let owner = method.owner.as_deref().unwrap_or(aggregate_name);
            self.ensure_aggregate_member_accessible(
                owner,
                method.access,
                &method.scopes,
                name,
                "method",
                span,
            )?;
        }
        let method_type = method_group_type(methods).expect("non-empty method group should type");
        if matches!(method_type, Type::Overload(_)) && !allow_overload {
            return Err(VerseError::check_at(
                format!("overloaded method `({qualifier}:){name}` must be called"),
                span,
            ));
        }
        Ok(method_type)
    }

    pub(super) fn check_assignment_target(&mut self, target: &Expr) -> Result<Type, VerseError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_accessible(name, target.span)? else {
                    return Err(VerseError::check_at(
                        format!("undefined name `{name}`"),
                        target.span,
                    ));
                };
                if !symbol.mutable {
                    return Err(VerseError::check_at(
                        format!("cannot assign to immutable binding `{name}`"),
                        target.span,
                    ));
                }
                Ok(symbol.value_type)
            }
            ExprKind::Index { collection, index } => {
                let collection_type = self.check_assignment_target(collection)?;
                let index_type = self.check_expr(index)?;
                match collection_type {
                    Type::Array(item) => {
                        ensure_int_index_type(&index_type, "array index", index.span)?;
                        Ok(*item)
                    }
                    Type::String => {
                        ensure_int_index_type(&index_type, "string index", index.span)?;
                        Ok(Type::Char)
                    }
                    Type::Map(key, value) | Type::WeakMap(key, value) => {
                        self.check_map_like_lookup(&key, &value, &index_type, index)
                    }
                    Type::Unknown | Type::Any => Ok(Type::Unknown),
                    other => Err(VerseError::check_at(
                        format!("cannot index value of type `{other}`"),
                        collection.span,
                    )),
                }
            }
            ExprKind::Member { object, name } => {
                let object_type = self.check_expr(object)?;
                let field = match &object_type {
                    Type::Struct(struct_name) => {
                        self.check_assignment_target(object)?;
                        let Some(info) = self.struct_types.get(struct_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown struct `{struct_name}`"),
                                target.span,
                            ));
                        };
                        if !info.computes {
                            return Err(VerseError::check_at(
                                format!(
                                    "struct `{struct_name}` must be `<computes>` to mutate fields"
                                ),
                                target.span,
                            ));
                        }
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("struct `{struct_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })?
                    }
                    Type::Class(class_name) => {
                        let Some(info) = self.struct_types.get(class_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown class `{class_name}`"),
                                target.span,
                            ));
                        };
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("class `{class_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })
                            .and_then(|field| {
                                let owner = field.owner.as_deref().unwrap_or(class_name);
                                self.ensure_aggregate_member_accessible(
                                    owner,
                                    field.mutation_access,
                                    &field.mutation_scopes,
                                    name,
                                    "field",
                                    target.span,
                                )?;
                                Ok(field)
                            })?
                    }
                    Type::Interface(interface_name) => {
                        let Some(info) = self.interface_types.get(interface_name) else {
                            return Err(VerseError::check_at(
                                format!("unknown interface `{interface_name}`"),
                                target.span,
                            ));
                        };
                        info.fields
                            .iter()
                            .find(|field| field.name == *name)
                            .ok_or_else(|| {
                                VerseError::check_at(
                                    format!("interface `{interface_name}` has no field `{name}`"),
                                    target.span,
                                )
                            })
                            .and_then(|field| {
                                let owner = field.owner.as_deref().unwrap_or(interface_name);
                                self.ensure_aggregate_member_accessible(
                                    owner,
                                    field.mutation_access,
                                    &field.mutation_scopes,
                                    name,
                                    "field",
                                    target.span,
                                )?;
                                Ok(field)
                            })?
                    }
                    _ => {
                        return Err(VerseError::check_at(
                            format!("cannot assign to member `{name}` on type `{object_type}`"),
                            target.span,
                        ));
                    }
                };
                if !matches!(object_type, Type::Struct(_)) && !field.mutable {
                    return Err(VerseError::check_at(
                        format!("cannot assign to immutable field `{name}`"),
                        target.span,
                    ));
                }

                Ok(field.value_type.clone())
            }
            _ => Err(VerseError::check_at(
                "invalid assignment target",
                target.span,
            )),
        }
    }

    fn builtin_type_value(name: &str) -> Option<Type> {
        if let Some(value_type) = builtin_numeric_alias_type(name) {
            return Some(value_type);
        }

        match name {
            "int" => Some(Type::Int),
            "float" => Some(Type::Float),
            "rational" => Some(Type::Rational),
            "number" => Some(Type::Number),
            "logic" | "bool" => Some(Type::Bool),
            "string" => Some(Type::String),
            "message" => Some(Type::Message),
            "char" => Some(Type::Char),
            "char8" => Some(Type::Char8),
            "char32" => Some(Type::Char32),
            "void" => Some(Type::None),
            "any" => Some(Type::Any),
            "comparable" => Some(Type::Comparable),
            "type" => Some(Type::TypeValue),
            "function" => Some(Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            }),
            _ => None,
        }
    }
    pub(super) fn check_set_expression(
        &mut self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        failure_context: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !failure_context && assignment_target_has_failable_expr(target) {
            self.ensure_failable_expression_allowed(target.span)?;
        }
        let target_type = self.check_assignment_target(target)?;
        self.ensure_current_function_allows_mutation(span)?;
        let expr_type = if failure_context {
            self.check_failure_expr(expr)?
        } else {
            self.check_expr(expr)?
        };
        match op {
            AssignOp::Assign => {
                self.ensure_expr_assignable(&target_type, &expr_type, expr, || {
                    format!("cannot assign `{expr_type}` to target of type `{target_type}`")
                })?;
            }
            AssignOp::AddAssign => {
                let result_type = check_add(&target_type, target.span, &expr_type, expr.span)?;
                self.ensure_assignable(&target_type, &result_type, expr.span, || {
                    format!(
                        "cannot assign `+=` result `{result_type}` to target of type `{target_type}`"
                    )
                })?;
            }
            AssignOp::SubAssign | AssignOp::MulAssign | AssignOp::DivAssign => {
                ensure_number_like(&target_type, "assignment target", target.span)?;
                ensure_number_like(&expr_type, "assignment value", expr.span)?;
                let result_type = if op == AssignOp::DivAssign {
                    divide_numeric_type(&target_type, &expr_type)
                } else {
                    unify_numeric_types(&target_type, &expr_type)
                };
                self.ensure_assignable(&target_type, &result_type, expr.span, || {
                    format!(
                        "cannot assign compound result `{result_type}` to target of type `{target_type}`"
                    )
                })?;
            }
        }
        Ok(Type::None)
    }

    pub(super) fn check_map_like_lookup(
        &mut self,
        key: &Type,
        value: &Type,
        index_type: &Type,
        index: &Expr,
    ) -> Result<Type, VerseError> {
        self.ensure_expr_assignable(key, index_type, index, || {
            format!("map key expects `{key}`, got `{index_type}`")
        })?;
        Ok(value.clone())
    }

    pub(super) fn iter_item_type(&mut self, iterable: &Expr) -> Result<Type, VerseError> {
        let iterable_type = self.with_range_context(|checker| checker.check_expr(iterable))?;
        match iterable_type {
            Type::Range => Ok(Type::Int),
            Type::Array(item) => Ok(*item),
            Type::Map(_, value) => Ok(*value),
            Type::Generator(Some(item)) => Ok(*item),
            Type::Generator(None) => Ok(Type::Unknown),
            Type::String => Ok(Type::Char),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot iterate over value of type `{other}`"),
                iterable.span,
            )),
        }
    }

    pub(super) fn iter_binding_types(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
    ) -> Result<Vec<(String, Type)>, VerseError> {
        match binding {
            ForBinding::Value(name) => Ok(vec![(name.clone(), self.iter_item_type(iterable)?)]),
            ForBinding::Pair { key, value } => {
                match self.with_range_context(|checker| checker.check_expr(iterable))? {
                    Type::Array(item) => Ok(vec![(key.clone(), Type::Int), (value.clone(), *item)]),
                    Type::Map(key_type, value_type) => {
                        Ok(vec![(key.clone(), *key_type), (value.clone(), *value_type)])
                    }
                    Type::Generator(_) => Err(VerseError::check_at(
                        "`->` pair iteration is not supported for `generator`",
                        iterable.span,
                    )),
                    Type::Unknown | Type::Any => Ok(vec![
                        (key.clone(), Type::Unknown),
                        (value.clone(), Type::Unknown),
                    ]),
                    other => Err(VerseError::check_at(
                        format!("cannot use `->` for iteration over type `{other}`"),
                        iterable.span,
                    )),
                }
            }
        }
    }

    pub(super) fn check_for(
        &mut self,
        clauses: &[ForClause],
        body: &Expr,
    ) -> Result<Type, VerseError> {
        self.push_scope();
        let result = (|| {
            for clause in clauses {
                match clause {
                    ForClause::Generator {
                        binding, iterable, ..
                    } => {
                        for (name, item_type) in self.iter_binding_types(binding, iterable)? {
                            self.define(&name, item_type, false, iterable.span)?;
                        }
                    }
                    ForClause::RangeOrLet { name, expr, span } => {
                        let expr_type =
                            self.with_range_context(|checker| checker.check_failure_expr(expr))?;
                        if expr_type == Type::Range {
                            self.define(name, Type::Int, false, *span)?;
                        } else {
                            self.define(name, expr_type, false, *span)?;
                        }
                    }
                    ForClause::Let { name, expr, span } => {
                        let expr_type = self.check_failure_expr(expr)?;
                        self.define(name, expr_type, false, *span)?;
                    }
                    ForClause::Filter(expr) => {
                        self.check_failure_expr(expr)?;
                        if !failure_condition_has_failable_expr(expr) {
                            return Err(VerseError::check_at(
                                "for filter must contain at least one failable expression",
                                expr.span,
                            ));
                        }
                    }
                }
            }

            let item_type = self.check_expr(body)?;
            Ok(Type::Array(Box::new(item_type)))
        })();
        self.pop_scope();
        result
    }

    pub(super) fn check_spawn(&mut self, body: &Expr) -> Result<Type, VerseError> {
        if self.data_member_default_depth > 0 {
            return Err(VerseError::check_at(
                "data-member default value cannot contain `spawn` expressions",
                body.span,
            ));
        }

        self.push_scope();
        let result = self.with_suppressed_async_expr_marker(|checker| {
            let spawned_expr = spawn_body_expr(body)?;
            let ExprKind::Call { callee, args } = &spawned_expr.kind else {
                return Err(VerseError::check_at(
                    "`spawn` body must be a single async function call",
                    spawned_expr.span,
                ));
            };

            let return_type = checker.check_spawn_call(callee, args, spawned_expr.span)?;
            Ok(Type::Task(Box::new(return_type)))
        });
        self.pop_scope();
        result
    }

    pub(super) fn check_spawn_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Type, VerseError> {
        let callee_type = self.check_callee_expr(callee)?;
        let arg_types = args
            .iter()
            .map(|arg| self.check_expr(call_arg_expr(arg)))
            .collect::<Result<Vec<_>, _>>()?;

        match callee_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        span,
                    ));
                }
                if !has_effect(&effects, "suspends") {
                    return Err(VerseError::check_at(
                        "`spawn` body must call a function with `<suspends>` effect",
                        span,
                    ));
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    &arg_types,
                    span,
                )?;
                Ok(*return_type)
            }
            Type::Overload(overloads) => {
                self.check_spawn_overloaded_call(&overloads, args, &arg_types, span)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot spawn value of type `{other}`"),
                callee.span,
            )),
        }
    }

    pub(super) fn check_spawn_overloaded_call(
        &mut self,
        overloads: &[Type],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut saw_matching_immediate = false;
        for overload in overloads {
            let Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } = overload
            else {
                continue;
            };

            if has_effect(effects, "decides") {
                continue;
            }

            if self
                .check_call_arguments(
                    (*arity, *arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    arg_types,
                    span,
                )
                .is_err()
            {
                continue;
            }

            if !has_effect(effects, "suspends") {
                saw_matching_immediate = true;
                continue;
            }

            return Ok(return_type.as_ref().clone());
        }

        if saw_matching_immediate {
            return Err(VerseError::check_at(
                "`spawn` body must call a function with `<suspends>` effect",
                span,
            ));
        }

        Err(VerseError::check_at(
            format!(
                "no overload matches spawn call with argument types ({})",
                render_type_list(arg_types)
            ),
            span,
        ))
    }

    pub(super) fn check_concurrent(
        &mut self,
        op: ConcurrentOp,
        body: &Expr,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !self.current_function_has_effect("suspends") {
            return Err(VerseError::check_at(
                format!(
                    "`{}` expression requires an async context",
                    concurrent_op_name(op)
                ),
                span,
            ));
        }

        let statements = concurrent_body_statements(body)?;
        let minimum = if matches!(op, ConcurrentOp::Branch) {
            1
        } else {
            2
        };
        if statements.len() < minimum {
            return Err(VerseError::check_at(
                format!(
                    "`{}` expression expects at least {minimum} subexpressions",
                    concurrent_op_name(op)
                ),
                body.span,
            ));
        }

        self.push_scope();
        let result = (|| {
            let mut item_types = Vec::with_capacity(statements.len());
            for statement in statements {
                self.push_scope();
                self.push_async_expr_marker();
                let item_type = self.check_stmt(statement);
                let has_async_expr = self.pop_async_expr_marker();
                self.pop_scope();
                let item_type = item_type?;
                if !has_async_expr {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}` branch must contain an async expression",
                            concurrent_op_name(op)
                        ),
                        statement.span,
                    ));
                }
                item_types.push(item_type);
            }

            match op {
                ConcurrentOp::Sync => Ok(Type::Tuple(item_types)),
                ConcurrentOp::Race | ConcurrentOp::Rush => {
                    let mut result_type = Type::Unknown;
                    for item_type in item_types {
                        result_type = unify_types(&result_type, &item_type, span)?;
                    }
                    Ok(result_type)
                }
                ConcurrentOp::Branch => Ok(Type::None),
            }
        })();
        self.pop_scope();
        result
    }

    pub(super) fn check_function(
        &mut self,
        params: &[Param],
        effects: &[String],
        return_type: Option<&TypeAnnotation>,
        body: &Expr,
    ) -> Result<Type, VerseError> {
        self.check_function_with_static_type_params(params, effects, return_type, body, &[])
    }

    pub(super) fn check_function_with_static_type_params(
        &mut self,
        params: &[Param],
        effects: &[String],
        return_type: Option<&TypeAnnotation>,
        body: &Expr,
        static_type_params: &[TypeParam],
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(effects, body.span)?;
        let type_params =
            merge_type_param_lists(&collect_function_type_params(params)?, static_type_params)?;
        self.validate_type_parameter_constraints(&type_params, body.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            let (param_types, param_specs, body_param_types) =
                self.dependent_type_value_param_signature(params, return_type)?;
            let checked_return = self.annotation_to_type(return_type)?;
            let static_type_body_type = if type_can_be_used_as_type_value(&checked_return)
                && static_type_function_body_needs_type_value_short_circuit(body)
            {
                match self.expr_to_type_name(body) {
                    Ok(name) => Some(Type::TypeValueOf(Box::new(
                        self.type_name_to_type_name(&name, body.span)?,
                    ))),
                    Err(_) => None,
                }
            } else {
                None
            };
            self.push_scope();
            self.function_returns.push(checked_return.clone());
            self.function_effects.push(effects.to_vec());
            let body_type = (|| {
                for (param, param_type) in params.iter().zip(&body_param_types) {
                    self.define_param_pattern(param, &param_type)?;
                }

                if let Some(static_type_body_type) = &static_type_body_type {
                    self.semantic_facts
                        .record_expression_type(body.span, static_type_body_type.clone());
                    return Ok(static_type_body_type.clone());
                }

                if has_effect(effects, "decides") {
                    self.check_failure_expr(body)
                } else {
                    self.check_expr(body)
                }
            })();
            self.function_returns.pop();
            self.function_effects.pop();
            self.pop_scope();
            let body_type = body_type?;

            let checked_return = if let Some(return_type) = return_type {
                let expected = self.type_name_to_type(return_type)?;
                if expected == Type::None && has_effect(effects, "decides") {
                    expected
                } else if body_type != Type::Never {
                    self.ensure_expr_assignable(&expected, &body_type, body, || {
                        format!(
                            "function is annotated to return `{expected}` but body has type `{body_type}`"
                        )
                    })?;
                    expected
                } else {
                    expected
                }
            } else {
                body_type
            };

            if has_effect(effects, "localizes") && checked_return != Type::Message {
                return Err(VerseError::check_at(
                    "`localizes` function specifier requires a `message` return type",
                    body.span,
                ));
            }

            let function_type = Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.to_vec(),
                param_types: Some(param_types),
                param_specs: Some(param_specs),
                return_type: Box::new(checked_return),
            };
            if has_effect(effects, "native") {
                self.ensure_native_function_signature(&function_type, body.span)?;
            }
            Ok(function_type)
        })();
        self.pop_type_param_scope();
        result
    }

    pub(super) fn define_param_pattern(
        &mut self,
        param: &Param,
        value_type: &Type,
    ) -> Result<(), VerseError> {
        match &param.pattern {
            ParamPattern::Binding => {
                if let Some(default) = &param.default {
                    let default_type = self.check_expr(default)?;
                    self.ensure_expr_assignable(value_type, &default_type, default, || {
                        format!(
                            "default value for parameter `{}` must be `{value_type}`, got `{default_type}`",
                            param.name
                        )
                    })?;
                }
                self.semantic_facts
                    .record_binding_type(param.span, value_type.clone());
                self.define(&param.name, value_type.clone(), false, param.span)
            }
            ParamPattern::Anonymous => Ok(()),
            ParamPattern::Tuple(params) => match value_type {
                Type::Tuple(items) if items.len() == params.len() => {
                    for (param, item_type) in params.iter().zip(items) {
                        self.define_param_pattern(param, item_type)?;
                    }
                    Ok(())
                }
                Type::Unknown | Type::Any => {
                    for param in params {
                        let item_type = self.annotation_to_type(param.annotation.as_ref())?;
                        self.define_param_pattern(param, &item_type)?;
                    }
                    Ok(())
                }
                Type::Tuple(items) => Err(VerseError::check_at(
                    format!(
                        "destructured tuple parameter expected {} elements, got {}",
                        params.len(),
                        items.len()
                    ),
                    param.span,
                )),
                other => Err(VerseError::check_at(
                    format!("destructured tuple parameter requires a tuple type, got `{other}`"),
                    param.span,
                )),
            },
        }
    }

    pub(super) fn check_call_arguments(
        &mut self,
        arity_spec: (Option<usize>, Option<(usize, usize)>),
        param_types: Option<&[Type]>,
        param_specs: Option<&[ParamSpec]>,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        let (arity, arity_range) = arity_spec;
        if let Some(param_specs) = param_specs {
            return self.check_spec_call_arguments(param_specs, args, arg_types, span);
        }

        if args.iter().any(CallArg::is_named) {
            return Err(VerseError::check_at(
                "named arguments are not supported for this callable",
                span,
            ));
        }

        let positional_args = args.iter().map(call_arg_expr).collect::<Vec<_>>();

        let Some(param_types) = param_types else {
            if let Some(expected) = arity
                && expected != args.len()
            {
                return Err(VerseError::check_at(
                    format!("expected {expected} arguments, got {}", args.len()),
                    span,
                ));
            }
            if let Some((min, max)) = arity_range
                && !(min..=max).contains(&args.len())
            {
                return Err(VerseError::check_at(
                    format!("expected {min}..={max} arguments, got {}", args.len()),
                    span,
                ));
            }
            return Ok(());
        };

        if let Some((min, max)) = arity_range
            && let [param_type] = param_types
        {
            if !(min..=max).contains(&args.len()) {
                return Err(VerseError::check_at(
                    format!("expected {min}..={max} arguments, got {}", args.len()),
                    span,
                ));
            }
            for (index, (arg, arg_type)) in positional_args.iter().zip(arg_types).enumerate() {
                self.ensure_expr_assignable(param_type, arg_type, arg, || {
                    format!(
                        "argument {} expected `{param_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if let [Type::Array(item_type)] = param_types {
            if args.len() == 1
                && self.expr_is_assignable_to_expected(
                    &param_types[0],
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }

            for (index, (arg, arg_type)) in positional_args.iter().zip(arg_types).enumerate() {
                self.ensure_expr_assignable(item_type, arg_type, arg, || {
                    format!(
                        "array argument item {} expected `{item_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if let [Type::Tuple(items)] = param_types {
            if args.len() == 1
                && self.expr_is_assignable_to_expected(
                    &param_types[0],
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }

            if args.len() != items.len() {
                return Err(VerseError::check_at(
                    format!(
                        "expected {} arguments for tuple parameter, got {}",
                        items.len(),
                        args.len()
                    ),
                    span,
                ));
            }

            for (index, ((arg, arg_type), item_type)) in
                positional_args.iter().zip(arg_types).zip(items).enumerate()
            {
                self.ensure_expr_assignable(item_type, arg_type, arg, || {
                    format!(
                        "tuple argument item {} expected `{item_type}`, got `{arg_type}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        let expanded_tuple = match arg_types {
            [Type::Tuple(items)] if items.len() == param_types.len() => Some(items.as_slice()),
            _ => None,
        };

        if let Some(items) = expanded_tuple {
            for (index, (expected, actual)) in param_types.iter().zip(items).enumerate() {
                self.ensure_expr_assignable(expected, actual, positional_args[0], || {
                    format!(
                        "argument {} expected `{expected}`, got `{actual}`",
                        index + 1
                    )
                })?;
            }
            return Ok(());
        }

        if args.len() != param_types.len() {
            return Err(VerseError::check_at(
                format!(
                    "expected {} arguments, got {}",
                    param_types.len(),
                    args.len()
                ),
                span,
            ));
        }

        for (index, ((arg, arg_type), param_type)) in positional_args
            .iter()
            .zip(arg_types)
            .zip(param_types)
            .enumerate()
        {
            self.ensure_expr_assignable(param_type, arg_type, arg, || {
                format!(
                    "argument {} expected `{param_type}`, got `{arg_type}`",
                    index + 1
                )
            })?;
        }

        Ok(())
    }

    pub(super) fn check_overloaded_call(
        &mut self,
        overloads: &[Type],
        require_decides: bool,
        require_rollback: bool,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut best_match = None;
        let mut saw_defer_suspends_match = false;
        let mut saw_suspends_match = false;
        let mut saw_rollback_mismatch = None;
        let mut saw_effect_mismatch = None;

        for overload in overloads {
            let Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } = overload
            else {
                continue;
            };

            if has_effect(effects, "decides") != require_decides {
                continue;
            }

            if self
                .check_call_arguments(
                    (*arity, *arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    arg_types,
                    span,
                )
                .is_err()
            {
                continue;
            }

            let has_suspends_effect = has_effect(effects, "suspends");
            if has_suspends_effect {
                if self.defer_depth > 0 {
                    saw_defer_suspends_match = true;
                    continue;
                }
                if !self.current_function_has_effect("suspends") {
                    saw_suspends_match = true;
                    continue;
                }
            }

            if require_rollback {
                if let Err(error) = ensure_callable_in_failure_context(effects, span) {
                    saw_rollback_mismatch.get_or_insert(error);
                    continue;
                }
            }

            if let Err(error) = self.ensure_current_function_allows_call_effects(effects, span) {
                saw_effect_mismatch = Some(error);
                continue;
            }

            let score = self.overload_match_score(
                param_types.as_deref(),
                param_specs.as_deref(),
                args,
                arg_types,
            );
            if best_match
                .as_ref()
                .is_none_or(|(best_score, _, _)| score < *best_score)
            {
                best_match = Some((score, return_type.as_ref().clone(), has_suspends_effect));
            }
        }

        if let Some((_, return_type, has_suspends_effect)) = best_match {
            if has_suspends_effect {
                self.mark_async_expression();
            }
            return Ok(return_type);
        }

        if saw_defer_suspends_match {
            return Err(VerseError::check_at(
                "`defer` block cannot contain suspend expressions",
                span,
            ));
        }

        if saw_suspends_match {
            return Err(VerseError::check_at(
                "function with `<suspends>` effect can only be called in an async context",
                span,
            ));
        }

        if let Some(error) = saw_rollback_mismatch {
            return Err(error);
        }

        if let Some(error) = saw_effect_mismatch {
            return Err(error);
        }

        let call_style = if require_decides { "[]" } else { "()" };
        Err(VerseError::check_at(
            format!(
                "no overload matches {call_style} call with argument types ({})",
                render_type_list(arg_types)
            ),
            span,
        ))
    }

    pub(super) fn overload_match_score(
        &self,
        param_types: Option<&[Type]>,
        param_specs: Option<&[ParamSpec]>,
        args: &[CallArg],
        arg_types: &[Type],
    ) -> usize {
        if let Some(param_specs) = param_specs
            && args.iter().any(CallArg::is_named)
        {
            return self
                .spec_overload_match_score(param_specs, args, arg_types)
                .unwrap_or(usize::MAX / 2);
        }

        if args.iter().any(CallArg::is_named) {
            return usize::MAX;
        }

        let Some(param_types) = param_types else {
            return usize::MAX / 2;
        };

        if param_types.len() != arg_types.len() {
            return usize::MAX / 2;
        }

        param_types
            .iter()
            .zip(arg_types)
            .map(|(expected, actual)| {
                self.type_match_score(expected, actual)
                    .unwrap_or(usize::MAX / 4)
            })
            .sum()
    }

    pub(super) fn spec_overload_match_score(
        &self,
        param_specs: &[ParamSpec],
        args: &[CallArg],
        arg_types: &[Type],
    ) -> Option<usize> {
        let mut assigned = vec![false; param_specs.len()];
        let mut positional_index = 0usize;
        let mut score = 0usize;

        for (arg, arg_type) in args.iter().zip(arg_types) {
            match arg {
                CallArg::Positional(_) => {
                    let (param_index, param) = param_specs
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)?;
                    positional_index = param_index + 1;
                    if assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    score += self.type_match_score(&param.value_type, arg_type)?;
                }
                CallArg::Named { name, optional, .. } => {
                    let (param_index, param) = param_specs
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == *name)?;
                    if (*optional && !param.named) || assigned[param_index] {
                        return None;
                    }
                    assigned[param_index] = true;
                    score += self.type_match_score(&param.value_type, arg_type)?;
                }
            }
        }

        for (index, param) in param_specs.iter().enumerate() {
            if !assigned[index] && !param.has_default {
                return None;
            }
        }

        Some(score)
    }

    pub(super) fn type_match_score(&self, expected: &Type, actual: &Type) -> Option<usize> {
        if expected == actual {
            return Some(0);
        }

        match (expected, actual) {
            (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown | Type::Never) => {
                Some(50)
            }
            (Type::Message, Type::String) => Some(1),
            (Type::Array(expected), Type::String) if is_string_char_type(expected) => Some(1),
            (Type::String, Type::Array(actual)) if is_string_char_type(actual) => Some(1),
            (Type::Class(expected), Type::Class(actual))
                if self.is_class_subtype(actual, expected) =>
            {
                Some(1)
            }
            (Type::Array(expected), Type::Array(actual)) => self
                .type_match_score(expected, actual)
                .map(|score| score + 1),
            (
                Type::Map(expected_key, expected_value)
                | Type::WeakMap(expected_key, expected_value),
                Type::Map(actual_key, actual_value) | Type::WeakMap(actual_key, actual_value),
            ) => Some(
                self.type_match_score(expected_key, actual_key)?
                    + self.type_match_score(expected_value, actual_value)?
                    + 1,
            ),
            (
                Type::Result(expected_success, expected_error),
                Type::Result(actual_success, actual_error),
            ) => Some(
                self.type_match_score(expected_success, actual_success)?
                    + self.type_match_score(expected_error, actual_error)?
                    + 1,
            ),
            (Type::Result(expected_success, _), Type::SuccessResult(actual_success)) => self
                .type_match_score(expected_success, actual_success)
                .map(|score| score + 1),
            (Type::Result(_, expected_error), Type::ErrorResult(actual_error)) => self
                .type_match_score(expected_error, actual_error)
                .map(|score| score + 1),
            (Type::SuccessResult(expected_success), Type::SuccessResult(actual_success))
            | (Type::ErrorResult(expected_success), Type::ErrorResult(actual_success)) => self
                .type_match_score(expected_success, actual_success)
                .map(|score| score + 1),
            (Type::SuccessResult(expected_success), Type::Result(actual_success, actual_error))
                if matches!(actual_error.as_ref(), Type::Never) =>
            {
                self.type_match_score(expected_success, actual_success)
                    .map(|score| score + 1)
            }
            (Type::ErrorResult(expected_error), Type::Result(actual_success, actual_error))
                if matches!(actual_success.as_ref(), Type::Never) =>
            {
                self.type_match_score(expected_error, actual_error)
                    .map(|score| score + 1)
            }
            (Type::Tuple(expected_items), Type::Tuple(actual_items))
                if expected_items.len() == actual_items.len() =>
            {
                expected_items
                    .iter()
                    .zip(actual_items)
                    .try_fold(1, |score, (expected, actual)| {
                        Some(score + self.type_match_score(expected, actual)?)
                    })
            }
            (Type::Option(expected), Type::Option(actual)) => self
                .type_match_score(expected, actual)
                .map(|score| score + 1),
            _ if self.is_assignable(expected, actual) => Some(10),
            _ => None,
        }
    }

    pub(super) fn check_spec_call_arguments(
        &mut self,
        param_specs: &[ParamSpec],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        if let [param] = param_specs
            && let Some(tuple_items) = param.tuple_items.as_deref()
            && tuple_param_specs_have_named_or_default(tuple_items)
        {
            let positional_args = args.iter().map(call_arg_expr).collect::<Vec<_>>();
            if args.len() == 1
                && !args[0].is_named()
                && self.expr_is_assignable_to_expected(
                    &param.value_type,
                    &arg_types[0],
                    positional_args[0],
                )?
            {
                return Ok(());
            }
            return self.check_spec_call_arguments(tuple_items, args, arg_types, span);
        }

        if args.iter().all(|arg| !arg.is_named()) && param_specs.iter().all(|param| !param.named) {
            let param_types = param_specs
                .iter()
                .map(|param| param.value_type.clone())
                .collect::<Vec<_>>();
            return self.check_call_arguments(
                (Some(param_specs.len()), None),
                Some(&param_types),
                None,
                args,
                arg_types,
                span,
            );
        }

        let mut assigned = vec![false; param_specs.len()];
        let mut positional_index = 0usize;

        for (arg, arg_type) in args.iter().zip(arg_types) {
            match arg {
                CallArg::Positional(expr) => {
                    let Some((param_index, param)) = param_specs
                        .iter()
                        .enumerate()
                        .skip(positional_index)
                        .find(|(_, param)| !param.named)
                    else {
                        return Err(VerseError::check_at(
                            "positional argument does not match any positional parameter",
                            expr.span,
                        ));
                    };
                    positional_index = param_index + 1;
                    assigned[param_index] = true;
                    self.ensure_expr_assignable(&param.value_type, arg_type, expr, || {
                        format!(
                            "argument `{}` expected `{}`, got `{arg_type}`",
                            param.name, param.value_type
                        )
                    })?;
                }
                CallArg::Named {
                    name,
                    expr,
                    optional,
                    span,
                } => {
                    let Some((param_index, param)) = param_specs
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.name == *name)
                    else {
                        let rendered = rendered_argument_name(name, *optional);
                        return Err(VerseError::check_at(
                            format!("unknown named argument `{rendered}`"),
                            *span,
                        ));
                    };
                    if *optional && !param.named {
                        return Err(VerseError::check_at(
                            format!("parameter `{name}` is not a named parameter"),
                            *span,
                        ));
                    }
                    if assigned[param_index] {
                        let rendered = rendered_argument_name(name, *optional);
                        return Err(VerseError::check_at(
                            format!("duplicate argument for parameter `{rendered}`"),
                            *span,
                        ));
                    }
                    assigned[param_index] = true;
                    let rendered = rendered_argument_name(&param.name, *optional);
                    self.ensure_expr_assignable(&param.value_type, arg_type, expr, || {
                        format!(
                            "argument `{rendered}` expected `{}`, got `{arg_type}`",
                            param.value_type
                        )
                    })?;
                }
            }
        }

        for (param, assigned) in param_specs.iter().zip(assigned) {
            if !assigned && !param.has_default {
                return Err(VerseError::check_at(
                    format!("missing required argument `{}`", rendered_param_name(param)),
                    span,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn check_tuple_access(
        &self,
        items: &[Type],
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        if args.iter().any(CallArg::is_named) {
            return Err(VerseError::check_at(
                "tuple index does not accept named arguments",
                span,
            ));
        }

        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("tuple index expects 1 argument, got {}", args.len()),
                span,
            ));
        }

        let arg = call_arg_expr(&args[0]);
        ensure_int_index_type(&arg_types[0], "tuple index", arg.span)?;

        if let Some(index) = tuple_index_literal(arg) {
            if index < 0 {
                return Err(VerseError::check_at(
                    format!("tuple index must be a non-negative integer, got {index}"),
                    arg.span,
                ));
            }
            let Ok(index) = usize::try_from(index) else {
                return Err(VerseError::check_at(
                    "tuple index is outside the supported index range",
                    arg.span,
                ));
            };
            return items.get(index).cloned().ok_or_else(|| {
                VerseError::check_at(
                    format!(
                        "tuple index {index} out of bounds for length {}",
                        items.len()
                    ),
                    arg.span,
                )
            });
        }

        items.iter().try_fold(Type::Unknown, |current, item| {
            unify_types(&current, item, arg.span)
        })
    }

    pub(super) fn check_archetype(
        &mut self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
    ) -> Result<Type, VerseError> {
        if is_official_event_archetype_callee(callee) {
            return self.check_event_archetype(callee, entries);
        }

        let callee_type = self.check_callee_expr(callee)?;
        let (aggregate_name, result_type, expected_kind, label) = match callee_type {
            Type::StructType(name) => (
                name.clone(),
                Type::Struct(name),
                AggregateKind::Struct,
                "struct",
            ),
            Type::ClassType(name) => (
                name.clone(),
                Type::Class(name),
                AggregateKind::Class,
                "class",
            ),
            Type::TypeValueOf(item) => match *item {
                Type::Struct(name) => (
                    name.clone(),
                    Type::Struct(name),
                    AggregateKind::Struct,
                    "struct",
                ),
                Type::Class(name) => (
                    name.clone(),
                    Type::Class(name),
                    AggregateKind::Class,
                    "class",
                ),
                other => {
                    return Err(VerseError::check_at(
                        format!("cannot construct value from type `{other}`"),
                        callee.span,
                    ));
                }
            },
            other => {
                return Err(VerseError::check_at(
                    format!("cannot construct value from type `{other}`"),
                    callee.span,
                ));
            }
        };
        self.ensure_data_member_default_archetype_not_recursive(&aggregate_name, callee.span)?;

        let Some(info) = self.struct_types.get(&aggregate_name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown {label} `{aggregate_name}`"),
                callee.span,
            ));
        };
        if info.kind != expected_kind {
            return Err(VerseError::check_at(
                format!("unknown {label} `{aggregate_name}`"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class && info.abstract_class {
            return Err(VerseError::check_at(
                format!("abstract class `{aggregate_name}` cannot be instantiated"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class && info.epic_internal_class {
            return Err(VerseError::check_at(
                format!("epic_internal class `{aggregate_name}` cannot be instantiated"),
                callee.span,
            ));
        }
        if info.kind == AggregateKind::Class {
            self.ensure_class_constructor_accessible(
                &aggregate_name,
                info.constructor_access,
                &info.constructor_scopes,
                callee.span,
            )?;
        }
        if info.kind == AggregateKind::Class && info.unique {
            self.ensure_current_function_allows_allocation(callee.span)?;
        }
        if info.kind == AggregateKind::Class && !info.constructor_effects.is_empty() {
            self.ensure_current_function_allows_call_effects(
                &info.constructor_effects,
                callee.span,
            )?;
        }
        if let Some(span) = entries.iter().find_map(archetype_entry_escape) {
            return Err(VerseError::check_at(
                "`return` and `break` are disallowed in archetype instantiation.",
                span,
            ));
        }

        self.push_scope();
        let result = (|| {
            let mut provided = vec![false; info.fields.len()];
            let mut direct_fields = vec![false; info.fields.len()];
            let mut constructor_delegation_seen = false;
            for entry in entries {
                match entry {
                    ArchetypeEntry::Let(binding) => {
                        self.check_archetype_let(binding)?;
                    }
                    ArchetypeEntry::Block(body) => {
                        self.check_expr(body)?;
                    }
                    ArchetypeEntry::Field(field) => {
                        if info.kind == AggregateKind::Class && constructor_delegation_seen {
                            return Err(VerseError::check_at(
                                format!(
                                    "field initializer `{}` cannot appear after constructor delegation",
                                    field.name
                                ),
                                field.span,
                            ));
                        }

                        let Some((index, expected)) = info
                            .fields
                            .iter()
                            .enumerate()
                            .find(|(_, candidate)| candidate.name == field.name)
                        else {
                            return Err(VerseError::check_at(
                                format!("{label} `{aggregate_name}` has no field `{}`", field.name),
                                field.span,
                            ));
                        };

                        if info.kind == AggregateKind::Class {
                            let owner = expected.owner.as_deref().unwrap_or(&aggregate_name);
                            self.ensure_aggregate_member_accessible(
                                owner,
                                expected.access,
                                &expected.scopes,
                                &field.name,
                                "field",
                                field.span,
                            )?;
                        }

                        if info.kind == AggregateKind::Class && expected.final_member {
                            return Err(VerseError::check_at(
                                format!(
                                    "final field `{}` cannot be overridden by an archetype",
                                    field.name
                                ),
                                field.span,
                            ));
                        }

                        if direct_fields[index] {
                            return Err(VerseError::check_at(
                                format!("duplicate value for field `{}`", field.name),
                                field.span,
                            ));
                        }
                        direct_fields[index] = true;
                        provided[index] = true;

                        let actual = self.check_expr(&field.expr)?;
                        self.ensure_expr_assignable(
                            &expected.value_type,
                            &actual,
                            &field.expr,
                            || {
                                format!(
                                    "field `{}` expected `{}`, got `{actual}`",
                                    field.name, expected.value_type
                                )
                            },
                        )?;
                    }
                    ArchetypeEntry::ConstructorCall(call) => {
                        if info.kind != AggregateKind::Class {
                            return Err(VerseError::check_at(
                                "constructor delegation is only valid in class archetypes",
                                call.span,
                            ));
                        }
                        constructor_delegation_seen = true;

                        let delegated_class = self.check_archetype_constructor_call(call)?;
                        let same_class = delegated_class == aggregate_name;
                        if !same_class
                            && !class_is_subtype_of(
                                &aggregate_name,
                                &delegated_class,
                                &self.struct_types,
                            )
                        {
                            return Err(VerseError::check_at(
                                format!(
                                    "constructor `{}` returns `{delegated_class}`, which is not `{aggregate_name}` or a superclass",
                                    call.name
                                ),
                                call.span,
                            ));
                        }

                        if same_class {
                            provided.fill(false);
                            direct_fields.fill(false);
                        }

                        if let Some(delegated_info) = self.struct_types.get(&delegated_class) {
                            for delegated_field in &delegated_info.fields {
                                if let Some(index) = info
                                    .fields
                                    .iter()
                                    .position(|field| field.name == delegated_field.name)
                                {
                                    provided[index] = true;
                                }
                            }
                        }
                    }
                }
            }

            for (field, provided) in info.fields.iter().zip(provided) {
                if !provided && !field.has_default {
                    return Err(VerseError::check_at(
                        format!(
                            "missing required field `{}` for `{aggregate_name}`",
                            field.name
                        ),
                        callee.span,
                    ));
                }
            }

            Ok(result_type)
        })();
        self.pop_scope();
        result
    }

    pub(super) fn check_archetype_constructor_call(
        &mut self,
        call: &ArchetypeConstructorCall,
    ) -> Result<String, VerseError> {
        let symbol = self
            .lookup_accessible(&call.name, call.span)?
            .ok_or_else(|| {
                VerseError::check_at(format!("undefined constructor `{}`", call.name), call.span)
            })?;

        let mut arg_types = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            arg_types.push(self.check_expr(call_arg_expr(arg))?);
        }

        let return_type = match symbol.value_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "constructor") {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}<constructor>` expects a constructor function",
                            call.name
                        ),
                        call.span,
                    ));
                }
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        call.span,
                    ));
                }
                if self.in_failure_context() {
                    ensure_callable_in_failure_context(&effects, call.span)?;
                }
                self.ensure_callable_in_async_context(&effects, call.span)?;
                self.ensure_current_function_allows_call_effects(&effects, call.span)?;
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call.args,
                    &arg_types,
                    call.span,
                )?;
                *return_type
            }
            Type::Overload(overloads) => {
                let constructors = overloads
                    .into_iter()
                    .filter(|overload| {
                        matches!(
                            overload,
                            Type::Function { effects, .. } if has_effect(effects, "constructor")
                        )
                    })
                    .collect::<Vec<_>>();
                if constructors.is_empty() {
                    return Err(VerseError::check_at(
                        format!(
                            "`{}<constructor>` expects a constructor function",
                            call.name
                        ),
                        call.span,
                    ));
                }
                self.check_overloaded_call(
                    &constructors,
                    false,
                    self.in_failure_context(),
                    &call.args,
                    &arg_types,
                    call.span,
                )?
            }
            other => {
                return Err(VerseError::check_at(
                    format!(
                        "`{}<constructor>` cannot call value of type `{other}`",
                        call.name
                    ),
                    call.span,
                ));
            }
        };

        match return_type {
            Type::Class(name) => Ok(name),
            other => Err(VerseError::check_at(
                format!("constructor delegation must return a class, got `{other}`"),
                call.span,
            )),
        }
    }

    pub(super) fn ensure_data_member_default_archetype_not_recursive(
        &self,
        aggregate_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(context) = self
            .data_member_default_stack
            .iter()
            .rev()
            .find(|context| context.aggregate_name == aggregate_name)
        {
            return Err(VerseError::check_at(
                format!(
                    "field default `{}.{}` recursively constructs `{aggregate_name}`",
                    context.aggregate_name, context.field_name
                ),
                span,
            ));
        }

        Ok(())
    }

    pub(super) fn check_event_archetype(
        &mut self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
    ) -> Result<Type, VerseError> {
        if !entries.is_empty() {
            return Err(VerseError::check_at(
                "`event` archetype construction expects an empty body",
                callee.span,
            ));
        }

        let (name, args) = official_event_archetype_name_and_args(callee)
            .expect("event archetype callee should have been recognized");
        let mut type_args = Vec::with_capacity(args.len());
        for arg in args {
            let CallArg::Positional(expr) = arg else {
                return Err(VerseError::check_at(
                    "`event` type arguments do not accept named arguments",
                    call_arg_expr(arg).span,
                ));
            };
            let type_name = self.expr_to_type_name(expr)?;
            type_args.push(self.type_name_to_type_name(&type_name, expr.span)?);
        }

        official_parametric_type(name, &type_args, callee.span)
    }

    pub(super) fn expr_to_type_name(&self, expr: &Expr) -> Result<TypeName, VerseError> {
        match &expr.kind {
            ExprKind::Ident(name) => Ok(TypeName::parse(name.clone())),
            ExprKind::TypeAnnotationLiteral { annotation } => Ok(annotation.name.clone()),
            ExprKind::TypeLiteral { expr } => self.type_literal_expr_to_type_name(expr),
            ExprKind::Member { .. } => expr_to_type_path(expr)
                .map(TypeName::parse)
                .ok_or_else(|| VerseError::check_at("expected type argument", expr.span)),
            ExprKind::QualifiedName { qualifier, name } => {
                Ok(TypeName::Named(format!("{qualifier}.{name}")))
            }
            ExprKind::Tuple(items) => Ok(TypeName::Tuple(
                items
                    .iter()
                    .map(|item| self.expr_to_type_name(item))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ExprKind::Call { callee, args } => {
                let Some(name) = expr_to_type_path(callee) else {
                    return Err(VerseError::check_at(
                        "expected parametric type name",
                        callee.span,
                    ));
                };

                let mut type_args = Vec::with_capacity(args.len());
                for arg in args {
                    let CallArg::Positional(expr) = arg else {
                        return Err(VerseError::check_at(
                            "parametric type arguments do not accept named arguments",
                            call_arg_expr(arg).span,
                        ));
                    };
                    type_args.push(self.expr_to_type_name(expr)?);
                }

                match name.as_str() {
                    "type" => {
                        if type_args.len() != 2 {
                            return Err(VerseError::check_at(
                                format!(
                                    "`type` bounds former expected 2 type arguments, got {}",
                                    type_args.len()
                                ),
                                expr.span,
                            ));
                        }
                        Ok(TypeName::TypeBounds {
                            lower: Box::new(type_args[0].clone()),
                            upper: Box::new(type_args[1].clone()),
                        })
                    }
                    "tuple" => {
                        if type_args.len() < 2 {
                            return Err(VerseError::check_at(
                                "tuple type expects at least two element types",
                                expr.span,
                            ));
                        }
                        Ok(TypeName::Tuple(type_args))
                    }
                    "weak_map" => {
                        if type_args.len() != 2 {
                            return Err(VerseError::check_at(
                                format!(
                                    "parametric type `weak_map` expected 2 type arguments, got {}",
                                    type_args.len()
                                ),
                                expr.span,
                            ));
                        }
                        Ok(TypeName::WeakMap(
                            Box::new(type_args[0].clone()),
                            Box::new(type_args[1].clone()),
                        ))
                    }
                    "event"
                    | "subscribable_event"
                    | "subscribable_event_intrnl"
                    | "sticky_event"
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
                    | "subscribable" => Ok(TypeName::Applied {
                        name,
                        args: type_args,
                    }),
                    _ if self.resolve_type_function_reference(&name).is_some() => {
                        Ok(TypeName::Applied {
                            name,
                            args: type_args,
                        })
                    }
                    _ if self.resolve_parametric_type_reference(&name).is_some() => {
                        Ok(TypeName::Applied {
                            name,
                            args: type_args,
                        })
                    }
                    _ => Err(VerseError::check_at(
                        format!("unknown parametric type `{name}`"),
                        callee.span,
                    )),
                }
            }
            _ => Err(VerseError::check_at("expected type argument", expr.span)),
        }
    }

    fn check_type_function_value_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        let Some(display_name) = expr_to_type_path(callee) else {
            return Ok(None);
        };
        let Some(qualified) = self.resolve_type_function_reference(&display_name) else {
            return Ok(None);
        };

        let mut type_args = Vec::with_capacity(args.len());
        for arg in args {
            let CallArg::Positional(expr) = arg else {
                if self.callee_has_non_static_type_function_overload(&qualified, callee) {
                    return Ok(None);
                }
                return Err(VerseError::check_at(
                    format!("type function `{display_name}` does not accept named type arguments"),
                    call_arg_expr(arg).span,
                ));
            };
            let Ok(type_name) = self.expr_to_type_name(expr) else {
                return Ok(None);
            };
            type_args.push(self.type_name_to_type_name(&type_name, expr.span)?);
        }

        match self.instantiate_type_function(&display_name, &qualified, &type_args, span) {
            Ok(result) => Ok(Some(Type::TypeValueOf(Box::new(result)))),
            Err(error)
                if self.non_static_overload_can_handle_type_function_value_call(
                    &qualified, callee, args, span,
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn non_static_overload_can_handle_type_function_value_call(
        &self,
        qualified: &str,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> bool {
        let mut checker = self.clone();
        let Ok(callee_type) = checker.check_callee_expr(callee) else {
            return false;
        };
        let Ok(arg_types) = args
            .iter()
            .map(|arg| checker.check_expr(call_arg_expr(arg)))
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };

        let overloads = match callee_type {
            Type::Function { .. } => {
                if self.function_type_is_registered_type_function(qualified, &callee_type) {
                    return false;
                }
                vec![callee_type]
            }
            Type::Overload(overloads) => overloads
                .into_iter()
                .filter(|overload| {
                    matches!(overload, Type::Function { .. })
                        && !self.function_type_is_registered_type_function(qualified, overload)
                })
                .collect::<Vec<_>>(),
            _ => return false,
        };
        if overloads.is_empty() {
            return false;
        }

        let require_rollback = checker.in_failure_context();
        checker
            .check_overloaded_call(&overloads, false, require_rollback, args, &arg_types, span)
            .is_ok()
    }

    fn callee_has_non_static_type_function_overload(&self, qualified: &str, callee: &Expr) -> bool {
        let mut checker = self.clone();
        match checker.check_callee_expr(callee) {
            Ok(callee_type @ Type::Function { .. }) => {
                !self.function_type_is_registered_type_function(qualified, &callee_type)
            }
            Ok(Type::Overload(overloads)) => overloads.iter().any(|overload| {
                matches!(overload, Type::Function { .. })
                    && !self.function_type_is_registered_type_function(qualified, overload)
            }),
            _ => false,
        }
    }

    fn function_type_is_registered_type_function(
        &self,
        qualified: &str,
        value_type: &Type,
    ) -> bool {
        self.type_functions
            .get(qualified)
            .is_some_and(|infos| infos.iter().any(|info| info.signature == *value_type))
    }

    fn type_literal_expr_to_type_name(&self, expr: &Expr) -> Result<TypeName, VerseError> {
        match &expr.kind {
            ExprKind::Number { kind, .. } => match kind {
                NumberKind::Int => Ok(TypeName::Int),
                NumberKind::Float => Ok(TypeName::Float),
            },
            ExprKind::Unary {
                op: UnaryOp::Positive | UnaryOp::Negate,
                expr: inner,
            } => match self.type_literal_expr_to_type_name(inner)? {
                TypeName::Int => Ok(TypeName::Int),
                TypeName::Float => Ok(TypeName::Float),
                _ => Err(VerseError::check_at(
                    "static type literal sign can only be applied to a number",
                    expr.span,
                )),
            },
            ExprKind::Char { kind, .. } => match kind {
                CharacterKind::Char => Ok(TypeName::Char),
                CharacterKind::Char32 => Ok(TypeName::Char32),
            },
            ExprKind::Bool(_) => Ok(TypeName::Bool),
            ExprKind::String(_) | ExprKind::InterpolatedString(_) => Ok(TypeName::String),
            ExprKind::None => Ok(TypeName::None),
            ExprKind::TypeAnnotationLiteral { annotation } => Ok(annotation.name.clone()),
            ExprKind::Array(items) => {
                let mut item_types = items
                    .iter()
                    .map(|item| self.type_literal_expr_to_type_name(item));
                let Some(first) = item_types.next() else {
                    return Ok(TypeName::Array(None));
                };
                let first = first?;
                if item_types.all(|item| item.is_ok_and(|item| item == first)) {
                    Ok(TypeName::Array(Some(Box::new(first))))
                } else {
                    Err(VerseError::check_at(
                        "static type literal array elements must have one type",
                        expr.span,
                    ))
                }
            }
            ExprKind::Map(entries) => {
                let Some((first_key, first_value)) = entries.first() else {
                    return Err(VerseError::check_at(
                        "static type literal map cannot be empty",
                        expr.span,
                    ));
                };
                let key_type = self.type_literal_expr_to_type_name(first_key)?;
                let value_type = self.type_literal_expr_to_type_name(first_value)?;
                if entries.iter().skip(1).all(|(key, value)| {
                    self.type_literal_expr_to_type_name(key)
                        .is_ok_and(|item| item == key_type)
                        && self
                            .type_literal_expr_to_type_name(value)
                            .is_ok_and(|item| item == value_type)
                }) {
                    Ok(TypeName::Map(Box::new(key_type), Box::new(value_type)))
                } else {
                    Err(VerseError::check_at(
                        "static type literal map entries must have one key type and one value type",
                        expr.span,
                    ))
                }
            }
            ExprKind::Tuple(items) => Ok(TypeName::Tuple(
                items
                    .iter()
                    .map(|item| self.type_literal_expr_to_type_name(item))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ExprKind::Option(Some(item)) => Ok(TypeName::Option(Box::new(
                self.type_literal_expr_to_type_name(item)?,
            ))),
            ExprKind::Archetype { callee, .. } => self.expr_to_type_name(callee),
            _ => Err(VerseError::check_at(
                "static type function cannot use this `type{...}` expression yet",
                expr.span,
            )),
        }
    }

    pub(super) fn check_archetype_let(&mut self, binding: &ArchetypeLet) -> Result<(), VerseError> {
        self.check_binding(
            &binding.name,
            &[],
            binding.annotation.as_ref(),
            &binding.expr,
            false,
            binding.span,
        )?;
        Ok(())
    }

    pub(super) fn check_if_condition(&mut self, condition: &Expr) -> Result<(), VerseError> {
        self.check_if_condition_inner(condition)?;

        if !failure_condition_has_failable_expr(condition) {
            return Err(VerseError::check_at(
                "`if` condition must contain at least one failable expression",
                condition.span,
            ));
        }

        Ok(())
    }

    pub(super) fn check_if_condition_inner(&mut self, condition: &Expr) -> Result<(), VerseError> {
        match &condition.kind {
            ExprKind::FailureSequence(clauses) => {
                for clause in clauses {
                    self.check_if_condition_inner(clause)?;
                }
                Ok(())
            }
            ExprKind::FailureBind { name, expr } => {
                let value_type = self.check_failure_expr(expr)?;
                self.define(name, value_type, false, condition.span)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.check_failure_statements(statements)?;
                Ok(())
            }
            ExprKind::Set { .. } => {
                self.check_failure_expr(condition)?;
                Ok(())
            }
            _ => {
                let is_failable = is_failable_condition_expr(condition);
                let condition_type = self.check_failure_expr(condition)?;
                if is_failable {
                    Ok(())
                } else {
                    ensure_bool_like(&condition_type, "if condition", condition.span)
                }
            }
        }
    }

    pub(super) fn check_failure_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        self.failure_context_depth += 1;
        let result = self.check_failure_expr_inner(expr);
        self.failure_context_depth -= 1;
        result
    }

    pub(super) fn check_failure_expr_inner(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        match &expr.kind {
            ExprKind::UnwrapOption(value) => match self.check_failure_expr(value)? {
                Type::Option(item) => Ok(*item),
                Type::Bool => Ok(Type::Bool),
                Type::Unknown | Type::Any => Ok(Type::Unknown),
                other => Err(VerseError::check_at(
                    format!("query operator expected `bool` or option, got `{other}`"),
                    value.span,
                )),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => {
                self.check_failure_expr(expr)?;
                Ok(Type::Bool)
            }
            ExprKind::Binary { left, op, right } if is_failure_binary_op(*op) => {
                self.check_failure_binary(left, *op, right)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                if statements.is_empty() {
                    self.warn_empty_block(expr.span);
                }
                self.push_scope();
                let result = self.check_failure_statements(statements);
                self.pop_scope();
                result
            }
            ExprKind::Profile { description, body } => {
                let description_type = self.check_expr(description)?;
                self.ensure_expr_assignable(&Type::String, &description_type, description, || {
                    format!("profile description expected `string`, got `{description_type}`")
                })?;
                self.check_failure_expr(body)
            }
            ExprKind::Call { callee, args } => self.check_failure_call(callee, args, expr.span),
            ExprKind::BracketCall { callee, args } => self.check_failure_bracket_call(callee, args),
            ExprKind::Set {
                target,
                op,
                expr: value,
            } => self.check_set_expression(target, *op, value, true, expr.span),
            ExprKind::Var {
                name,
                annotation,
                expr: value,
            } => self.check_var_expression(name, annotation, value, true, expr.span),
            ExprKind::Member { object, name } => {
                self.check_failure_member_expr(object, name, expr.span, false)
            }
            _ => self.check_expr(expr),
        }
    }

    pub(super) fn check_failure_statements(
        &mut self,
        statements: &[Stmt],
    ) -> Result<Type, VerseError> {
        let mut last = Type::None;
        let mut unreachable_after: Option<(&'static str, Span)> = None;

        for statement in statements {
            if let Some((message, span)) = unreachable_after {
                last = Type::Never;
                self.warn_unreachable(message, span.through(statement.span));
                break;
            }
            last = match &statement.kind {
                StmtKind::Let {
                    name,
                    annotation,
                    expr,
                    ..
                } => self.check_failure_binding(
                    name,
                    annotation.as_ref(),
                    expr,
                    false,
                    statement.span,
                )?,
                StmtKind::Var {
                    name,
                    annotation,
                    expr,
                } => self.check_failure_binding(
                    name,
                    annotation.as_ref(),
                    expr,
                    true,
                    statement.span,
                )?,
                StmtKind::Return(_) => {
                    return Err(VerseError::check_at(
                        "Explicit return out of a failure context is not allowed",
                        statement.span,
                    ));
                }
                StmtKind::Break => {
                    return Err(VerseError::check_at(
                        "`break` may not be used in a failure context",
                        statement.span,
                    ));
                }
                StmtKind::Set { target, op, expr } => {
                    self.check_set_expression(target, *op, expr, true, statement.span)?
                }
                StmtKind::Expr(expr) => self.check_failure_expr(expr)?,
                _ => self.check_stmt(statement)?,
            };
            if last == Type::Never || self.statement_never_completes(statement) {
                unreachable_after =
                    Some((unreachable_statement_message(statement), statement.span));
            }
        }

        Ok(last)
    }

    pub(super) fn check_failure_binding(
        &mut self,
        name: &str,
        annotation: Option<&TypeAnnotation>,
        expr: &Expr,
        mutable: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        if matches!(
            &expr.kind,
            ExprKind::EnumDefinition { .. }
                | ExprKind::StructDefinition { .. }
                | ExprKind::ClassDefinition { .. }
                | ExprKind::InterfaceDefinition { .. }
                | ExprKind::ModuleDefinition { .. }
        ) {
            return self.check_binding(name, &[], annotation, expr, mutable, span);
        }

        let inferred = self.check_failure_expr(expr)?;
        let checked_type = if let Some(annotation) = annotation {
            let expected = self.type_name_to_type(annotation)?;
            self.ensure_expr_assignable(&expected, &inferred, expr, || {
                format!(
                    "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
                )
            })?;
            expected
        } else {
            inferred
        };

        self.define(name, checked_type.clone(), mutable, span)?;
        self.semantic_facts
            .record_binding_type(span, checked_type.clone());
        Ok(checked_type)
    }

    pub(super) fn statement_never_completes(&self, statement: &Stmt) -> bool {
        match &statement.kind {
            StmtKind::Return(_) | StmtKind::Break => true,
            StmtKind::Let { expr, .. } | StmtKind::Var { expr, .. } | StmtKind::Expr(expr) => {
                self.expr_never_completes(expr)
            }
            StmtKind::Set { target, expr, .. } => {
                self.expr_never_completes(target) || self.expr_never_completes(expr)
            }
            StmtKind::Using { .. }
            | StmtKind::ParametricType { .. }
            | StmtKind::ParametricTypeAlias { .. }
            | StmtKind::TypeAlias { .. }
            | StmtKind::ScopedAccessLevel { .. }
            | StmtKind::ExtensionMethod(_)
            | StmtKind::Defer(_) => false,
        }
    }

    pub(super) fn expr_never_completes(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Unary { expr, .. } => self.expr_never_completes(expr),
            ExprKind::Binary { left, op, right } => {
                self.expr_never_completes(left)
                    || (!matches!(op, BinaryOp::And | BinaryOp::Or)
                        && self.expr_never_completes(right))
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr_never_completes(condition)
                    || else_branch.as_deref().is_some_and(|else_branch| {
                        self.expr_never_completes(then_branch)
                            && self.expr_never_completes(else_branch)
                    })
            }
            ExprKind::FailureBind { expr, .. } => self.expr_never_completes(expr),
            ExprKind::FailureSequence(items) => items
                .first()
                .is_some_and(|item| self.expr_never_completes(item)),
            ExprKind::Set { target, expr, .. } => {
                self.expr_never_completes(target) || self.expr_never_completes(expr)
            }
            ExprKind::Var { expr, .. } => self.expr_never_completes(expr),
            ExprKind::TypeLiteral { expr } => self.expr_never_completes(expr),
            ExprKind::TypeAnnotationLiteral { .. } => false,
            ExprKind::For { clauses, .. } => clauses
                .iter()
                .any(|clause| self.for_clause_expr_never_completes(clause)),
            ExprKind::Profile { description, body } => {
                self.expr_never_completes(description) || self.expr_never_completes(body)
            }
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
                .last()
                .is_some_and(|statement| self.statement_never_completes(statement)),
            ExprKind::Call { callee, args } => {
                self.expr_never_completes(callee)
                    || args
                        .iter()
                        .any(|arg| self.expr_never_completes(call_arg_expr(arg)))
                    || self.callee_returns_never(callee)
            }
            ExprKind::BracketCall { callee, args } => {
                self.expr_never_completes(callee)
                    || args.iter().any(|arg| self.expr_never_completes(arg))
                    || self.callee_returns_never(callee)
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => {
                items.iter().any(|item| self.expr_never_completes(item))
            }
            ExprKind::Map(entries) => entries.iter().any(|(key, value)| {
                self.expr_never_completes(key) || self.expr_never_completes(value)
            }),
            ExprKind::Archetype {
                callee, entries, ..
            } => {
                self.expr_never_completes(callee)
                    || entries
                        .iter()
                        .any(|entry| self.archetype_entry_never_completes(entry))
            }
            ExprKind::Case { subject, arms } => {
                self.expr_never_completes(subject)
                    || (case_arms_have_wildcard(arms)
                        && arms.iter().all(|arm| self.expr_never_completes(&arm.expr)))
            }
            ExprKind::Option(Some(value)) | ExprKind::UnwrapOption(value) => {
                self.expr_never_completes(value)
            }
            ExprKind::InterpolatedString(parts) => parts.iter().any(|part| match part {
                InterpolatedStringPart::Text(_) => false,
                InterpolatedStringPart::Expr(expr) => self.expr_never_completes(expr),
            }),
            ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
                self.expr_never_completes(object)
            }
            ExprKind::Index { collection, index } => {
                self.expr_never_completes(collection) || self.expr_never_completes(index)
            }
            ExprKind::Number { .. }
            | ExprKind::Char { .. }
            | ExprKind::Bool(_)
            | ExprKind::String(_)
            | ExprKind::None
            | ExprKind::Ident(_)
            | ExprKind::External
            | ExprKind::Loop { .. }
            | ExprKind::Spawn { .. }
            | ExprKind::Concurrent { .. }
            | ExprKind::Function { .. }
            | ExprKind::EnumDefinition { .. }
            | ExprKind::StructDefinition { .. }
            | ExprKind::ClassDefinition { .. }
            | ExprKind::InterfaceDefinition { .. }
            | ExprKind::ModuleDefinition { .. }
            | ExprKind::Option(None)
            | ExprKind::QualifiedName { .. } => false,
        }
    }

    pub(super) fn for_clause_expr_never_completes(&self, clause: &ForClause) -> bool {
        match clause {
            ForClause::Generator { iterable, .. }
            | ForClause::Let { expr: iterable, .. }
            | ForClause::RangeOrLet { expr: iterable, .. }
            | ForClause::Filter(iterable) => self.expr_never_completes(iterable),
        }
    }

    pub(super) fn archetype_entry_never_completes(&self, entry: &ArchetypeEntry) -> bool {
        match entry {
            ArchetypeEntry::Field(field) => self.expr_never_completes(&field.expr),
            ArchetypeEntry::Let(binding) => self.expr_never_completes(&binding.expr),
            ArchetypeEntry::Block(block) => self.expr_never_completes(block),
            ArchetypeEntry::ConstructorCall(call) => call
                .args
                .iter()
                .any(|arg| self.expr_never_completes(call_arg_expr(arg))),
        }
    }

    pub(super) fn callee_returns_never(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Ident(name) => self
                .lookup(name)
                .is_some_and(|symbol| type_returns_never(&symbol.value_type)),
            _ => false,
        }
    }

    pub(super) fn check_var_expression(
        &mut self,
        name: &str,
        annotation: &TypeAnnotation,
        expr: &Expr,
        failure_context: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        let expected = self.type_name_to_type(annotation)?;
        let inferred = if failure_context {
            self.check_failure_expr(expr)?
        } else {
            self.check_expr(expr)?
        };
        self.ensure_expr_assignable(&expected, &inferred, expr, || {
            format!(
                "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
            )
        })?;
        self.define(name, expected.clone(), true, span)?;
        Ok(expected)
    }

    pub(super) fn check_failure_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        match op {
            BinaryOp::And => {
                self.check_failure_expr(left)?;
                self.check_failure_expr(right)
            }
            BinaryOp::Or => {
                let left_type = self.check_failure_expr(left)?;
                let right_type = self.check_failure_expr(right)?;
                unify_types(&left_type, &right_type, right.span)
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => self.check_failure_comparison_binary(left, op, right),
            BinaryOp::Divide | BinaryOp::Remainder => self.check_binary(left, op, right),
            _ => self.check_expr(&Expr::new(
                ExprKind::Binary {
                    left: Box::new(left.clone()),
                    op,
                    right: Box::new(right.clone()),
                },
                left.span.through(right.span),
            )),
        }
    }

    pub(super) fn check_failure_comparison_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match op {
            BinaryOp::Equal | BinaryOp::NotEqual => {
                ensure_equality_comparable(&left_type, &self.struct_types, left.span)?;
                ensure_equality_comparable(&right_type, &self.struct_types, right.span)?;
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
            }
            _ => unreachable!("only comparison operators are handled here"),
        }

        Ok(left_type)
    }

    pub(super) fn check_failure_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Type, VerseError> {
        let callee_type = self.check_failure_callee_expr(callee)?;
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expr(call_arg_expr(arg))?);
        }

        if is_shuffle_callee(callee) && is_shuffle_function_type(&callee_type) {
            self.ensure_callee_type_failure_context_allowed(&callee_type, span)?;
            return self.check_shuffle_call(args, &arg_types, span);
        }

        if is_concatenate_callee(callee) && is_concatenate_function_type(&callee_type) {
            self.ensure_callee_type_failure_context_allowed(&callee_type, span)?;
            return self.check_concatenate_call(args, &arg_types, span);
        }

        if is_make_classifiable_subset_callee(callee)
            && is_make_classifiable_subset_function_type(&callee_type)
        {
            self.ensure_callee_type_failure_context_allowed(&callee_type, span)?;
            return self.check_make_classifiable_subset_call(args, &arg_types, span, false);
        }

        if is_make_classifiable_subset_var_callee(callee)
            && is_make_classifiable_subset_var_function_type(&callee_type)
        {
            self.ensure_callee_type_failure_context_allowed(&callee_type, span)?;
            return self.check_make_classifiable_subset_call(args, &arg_types, span, true);
        }

        if is_make_result_callee(callee) {
            self.ensure_callee_type_failure_context_allowed(&callee_type, span)?;
            return self.check_make_result_call(callee, args, &arg_types, span);
        }

        if is_length_member_callee(callee)
            && matches!(callee_type, Type::Int | Type::Unknown | Type::Any)
        {
            self.check_call_arguments((Some(0), None), Some(&[]), None, args, &arg_types, span)?;
            return Ok(Type::Int);
        }

        match callee_type {
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => {
                if has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions with `<decides>` must be called with `[]`",
                        span,
                    ));
                }
                ensure_callable_in_failure_context(&effects, span)?;
                self.ensure_callable_in_async_context(&effects, span)?;
                self.ensure_current_function_allows_call_effects(&effects, span)?;
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    args,
                    &arg_types,
                    span,
                )?;
                Ok(*return_type)
            }
            Type::Overload(overloads) => {
                self.check_overloaded_call(&overloads, false, true, args, &arg_types, span)
            }
            Type::Tuple(items) => self.check_tuple_access(&items, args, &arg_types, span),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot call value of type `{other}`"),
                callee.span,
            )),
        }
    }

    pub(super) fn check_failure_bracket_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Type, VerseError> {
        let mut checked_arg_types = None;
        if let ExprKind::Member { object, name } = &callee.kind {
            let object_type = if is_failable_condition_expr(object) {
                self.check_failure_expr(object)?
            } else {
                self.check_expr(object)?
            };
            let arg_types = args
                .iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?;

            match object_type {
                Type::Array(item) => {
                    return self.check_array_method(
                        name,
                        item.as_ref(),
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::String => {
                    return self.check_array_method(
                        name,
                        &Type::Char,
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::Int
                | Type::IntRange(_)
                | Type::Float
                | Type::FloatRange(_)
                | Type::Rational
                | Type::Number => {
                    return self.check_number_method(
                        name,
                        &object_type,
                        args,
                        &arg_types,
                        callee.span,
                    );
                }
                Type::Unknown | Type::Any => return Ok(Type::Unknown),
                Type::Struct(_)
                | Type::Class(_)
                | Type::Module(_)
                | Type::Result(_, _)
                | Type::SuccessResult(_)
                | Type::ErrorResult(_)
                | Type::Event(_)
                | Type::SubscribableEventIntrnl(_)
                | Type::StickyEvent(_)
                | Type::Task(_)
                | Type::Generator(_)
                | Type::Subtype(_)
                | Type::CastableSubtype(_)
                | Type::ConcreteSubtype(_)
                | Type::ClassifiableSubset(_)
                | Type::ClassifiableSubsetVar(_)
                | Type::Awaitable(_)
                | Type::Signalable(_)
                | Type::Subscribable(_)
                | Type::Listenable(_)
                | Type::Param(
                    _,
                    TypeParamConstraint::Subtype(_) | TypeParamConstraint::TypeBounds { .. },
                ) => checked_arg_types = Some(arg_types),
                other => {
                    return Err(VerseError::check_at(
                        format!("type `{other}` has no bracket method `{name}`"),
                        callee.span,
                    ));
                }
            }
        }

        let callee_type = self.check_failure_callee_expr(callee)?;
        let arg_types = if let Some(arg_types) = checked_arg_types {
            arg_types
        } else {
            args.iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?
        };

        if is_fits_in_player_map_callee(callee) {
            ensure_exact_arg_count("FitsInPlayerMap", args, 1, callee.span)?;
            return Ok(arg_types[0].clone());
        }

        match callee_type {
            Type::Array(item) => {
                ensure_exact_arg_count("array index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "array index", args[0].span)?;
                Ok(*item)
            }
            Type::String => {
                ensure_exact_arg_count("string index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "string index", args[0].span)?;
                Ok(Type::Char)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) => {
                ensure_exact_arg_count("map lookup", args, 1, callee.span)?;
                self.check_map_like_lookup(&key, &value, &arg_types[0], &args[0])
            }
            Type::Function {
                arity,
                arity_range,
                effects,
                mut param_types,
                mut param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions without `<decides>` must be called with `()`",
                        callee.span,
                    ));
                }
                ensure_callable_in_failure_context(&effects, callee.span)?;
                self.ensure_callable_in_async_context(&effects, callee.span)?;
                self.ensure_current_function_allows_call_effects(&effects, callee.span)?;
                let call_args = positional_call_args(args);
                let mut return_type = *return_type;
                if let Some(inferred) = self
                    .infer_function_type_params(
                        param_types.as_deref(),
                        Some(&return_type),
                        &arg_types,
                    )
                    .filter(|inferred| !inferred.is_empty())
                {
                    self.ensure_inferred_type_param_constraints(
                        param_types.as_deref(),
                        Some(&return_type),
                        &inferred,
                        callee.span,
                    )?;
                    if let Some(types) = param_types.as_mut() {
                        for value_type in types {
                            *value_type = self.substitute_type_params_runtime(
                                value_type,
                                &inferred,
                                callee.span,
                            )?;
                        }
                    }
                    if let Some(specs) = param_specs.as_mut() {
                        for spec in specs {
                            *spec =
                                self.substitute_param_spec_runtime(spec, &inferred, callee.span)?;
                        }
                    }
                    return_type =
                        self.substitute_type_params_runtime(&return_type, &inferred, callee.span)?;
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                Ok(return_type)
            }
            Type::Overload(overloads) => {
                let call_args = positional_call_args(args);
                self.check_overloaded_call(
                    &overloads,
                    true,
                    true,
                    &call_args,
                    &arg_types,
                    callee.span,
                )
            }
            Type::ClassType(target) => {
                self.check_class_cast(&target, args, &arg_types, callee.span)
            }
            Type::Subtype(_)
            | Type::CastableSubtype(_)
            | Type::ConcreteSubtype(_)
            | Type::TypeValueOf(_) => {
                self.check_type_value_cast(callee, &callee_type, args, &arg_types, callee.span)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot use `[]` with value of type `{other}`"),
                callee.span,
            )),
        }
    }

    pub(super) fn check_case(
        &mut self,
        subject: &Expr,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let subject_type = self.check_expr(subject)?;
        match subject_type {
            Type::Enum(enum_name) => self.check_enum_case(&enum_name, arms, span, subject.span),
            Type::Int | Type::Bool | Type::String | Type::Char | Type::Char8 | Type::Char32 => {
                self.check_scalar_case(&subject_type, arms, span)
            }
            Type::Unknown | Type::Any => self.check_unknown_case_arms(arms, span),
            other => Err(VerseError::check_at(
                format!(
                    "case subject must be `int`, `logic`, `string`, `char`, or enum, got `{other}`"
                ),
                subject.span,
            )),
        }
    }

    pub(super) fn check_enum_case(
        &mut self,
        enum_name: &str,
        arms: &[CaseArm],
        span: Span,
        subject_span: Span,
    ) -> Result<Type, VerseError> {
        let Some(info) = self.enum_types.get(enum_name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown enum `{enum_name}`"),
                subject_span,
            ));
        };

        let mut result_type = None;
        let mut covered = Vec::<String>::new();
        let mut saw_wildcard = false;

        for arm in arms {
            let unreachable_after_wildcard = saw_wildcard;
            if unreachable_after_wildcard && !arm.ignore_unreachable {
                self.warn_unreachable("case after wildcard is unreachable", arm.span);
                continue;
            }

            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    saw_wildcard = true;
                }
                CasePattern::Expr(pattern) => {
                    let pattern_type = self.check_expr(pattern)?;
                    self.ensure_assignable(
                        &Type::Enum(enum_name.to_string()),
                        &pattern_type,
                        pattern.span,
                        || {
                            format!(
                                "case pattern for `{enum_name}` must be `{enum_name}`, got `{pattern_type}`"
                            )
                        },
                    )?;

                    let variant = enum_case_variant(pattern, enum_name).ok_or_else(|| {
                        VerseError::check_at(
                            format!(
                                "case pattern for `{enum_name}` must be an explicit enum value"
                            ),
                            pattern.span,
                        )
                    })?;

                    let duplicate = covered.iter().any(|existing| existing == variant);
                    if duplicate && !arm.ignore_unreachable {
                        self.warn_unreachable(
                            format!("duplicate case `{enum_name}.{variant}` is unreachable"),
                            pattern.span,
                        );
                        continue;
                    }
                    if !duplicate && !unreachable_after_wildcard {
                        covered.push(variant.to_string());
                    }
                }
            }

            let next = self.check_case_arm_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, arm.expr.span)?,
                None => next,
            });
        }

        if !saw_wildcard && !self.case_failure_allowed() {
            if info.open {
                return Err(VerseError::check_at(
                    format!("case over open enum `{enum_name}` requires wildcard or `<decides>`"),
                    span,
                ));
            }

            let missing = info
                .variants
                .iter()
                .filter(|variant| !covered.iter().any(|covered| covered == *variant))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(VerseError::check_at(
                    format!(
                        "case expression for enum `{enum_name}` is missing cases: {}",
                        missing.join(", ")
                    ),
                    span,
                ));
            }
        }

        Ok(result_type.unwrap_or(Type::Unknown))
    }

    pub(super) fn check_scalar_case(
        &mut self,
        subject_type: &Type,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut result_type = None;
        let mut covered = Vec::<CaseConstant>::new();
        let mut saw_wildcard = false;

        for arm in arms {
            let unreachable_after_wildcard = saw_wildcard;
            if unreachable_after_wildcard && !arm.ignore_unreachable {
                self.warn_unreachable("case after wildcard is unreachable", arm.span);
                continue;
            }

            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    saw_wildcard = true;
                }
                CasePattern::Expr(pattern) => {
                    let pattern_type = self.check_expr(pattern)?;
                    self.ensure_assignable(subject_type, &pattern_type, pattern.span, || {
                        format!(
                            "case pattern for `{subject_type}` must be `{subject_type}`, got `{pattern_type}`"
                        )
                    })?;
                    let constant =
                        scalar_case_constant(pattern, subject_type).ok_or_else(|| {
                            VerseError::check_at(
                                format!(
                                    "case pattern for `{subject_type}` must be an `int`, `logic`, `string`, or `char` constant"
                                ),
                                pattern.span,
                            )
                        })?;
                    let duplicate = covered.iter().any(|existing| existing == &constant);
                    if duplicate && !arm.ignore_unreachable {
                        self.warn_unreachable("duplicate case is unreachable", pattern.span);
                        continue;
                    }
                    if !duplicate && !unreachable_after_wildcard {
                        covered.push(constant);
                    }
                }
            }

            let next = self.check_case_arm_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, arm.expr.span)?,
                None => next,
            });
        }

        if !saw_wildcard
            && !self.case_failure_allowed()
            && !scalar_case_is_exhaustive(subject_type, &covered)
        {
            return Err(VerseError::check_at(
                format!("case over `{subject_type}` requires wildcard or failure context"),
                span,
            ));
        }

        Ok(result_type.unwrap_or(Type::Unknown))
    }

    pub(super) fn check_case_arm_expr(&mut self, expr: &Expr) -> Result<Type, VerseError> {
        if self.failure_context_depth > 0 {
            self.check_failure_expr(expr)
        } else {
            self.check_expr(expr)
        }
    }

    pub(super) fn case_failure_allowed(&self) -> bool {
        self.failure_context_depth > 0 || self.current_function_has_effect("decides")
    }

    pub(super) fn check_unknown_case_arms(
        &mut self,
        arms: &[CaseArm],
        span: Span,
    ) -> Result<Type, VerseError> {
        let mut result_type = None;
        let mut saw_wildcard = false;
        for arm in arms {
            if saw_wildcard && !arm.ignore_unreachable {
                self.warn_unreachable("case after wildcard is unreachable", arm.span);
                continue;
            }
            match &arm.pattern {
                CasePattern::Wildcard { .. } => saw_wildcard = true,
                CasePattern::Expr(pattern) => {
                    self.check_expr(pattern)?;
                }
            }
            let next = self.check_expr(&arm.expr)?;
            result_type = Some(match result_type {
                Some(current) => unify_types(&current, &next, span)?,
                None => next,
            });
        }
        Ok(result_type.unwrap_or(Type::Unknown))
    }

    pub(super) fn ensure_assignable(
        &self,
        expected: &Type,
        actual: &Type,
        span: Span,
        message: impl FnOnce() -> String,
    ) -> Result<(), VerseError> {
        if self.is_assignable(expected, actual) {
            Ok(())
        } else {
            Err(VerseError::check_at(message(), span))
        }
    }

    pub(super) fn ensure_expr_assignable(
        &mut self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
        message: impl FnOnce() -> String,
    ) -> Result<(), VerseError> {
        if self.expr_is_assignable_to_expected(expected, actual, expr)? {
            Ok(())
        } else {
            Err(VerseError::check_at(message(), expr.span))
        }
    }

    pub(super) fn expr_is_assignable_to_expected(
        &mut self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
    ) -> Result<bool, VerseError> {
        if let Type::IntRange(range) = expected
            && let Some(value) = expr_int_literal_value(expr)
        {
            return Ok(range.contains(value));
        }
        if let Type::FloatRange(range) = expected
            && let Some(value) = expr_float_literal_value(expr)
        {
            return Ok(range.contains(value));
        }

        if is_empty_option_literal(expected, expr) {
            return Ok(true);
        }

        match (expected, &expr.kind) {
            (Type::Array(expected_item), ExprKind::Array(items)) => {
                for item in items {
                    let actual_item = self.check_expr(item)?;
                    if !self.expr_is_assignable_to_expected(expected_item, &actual_item, item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (
                Type::Map(expected_key, expected_value)
                | Type::WeakMap(expected_key, expected_value),
                ExprKind::Map(entries),
            ) => {
                for (key, value) in entries {
                    let actual_key = self.check_expr(key)?;
                    if !self.expr_is_assignable_to_expected(expected_key, &actual_key, key)? {
                        return Ok(false);
                    }

                    let actual_value = self.check_expr(value)?;
                    if !self.expr_is_assignable_to_expected(expected_value, &actual_value, value)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Type::Tuple(expected_items), ExprKind::Tuple(items))
                if expected_items.len() == items.len() =>
            {
                for (expected_item, item) in expected_items.iter().zip(items) {
                    let actual_item = self.check_expr(item)?;
                    if !self.expr_is_assignable_to_expected(expected_item, &actual_item, item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Type::Option(expected_item), ExprKind::Option(Some(value))) => {
                let actual_value = self.check_failure_expr(value)?;
                self.expr_is_assignable_to_expected(expected_item, &actual_value, value)
            }
            (Type::Option(_), ExprKind::Option(None)) => Ok(true),
            _ => Ok(self.is_assignable(expected, actual)),
        }
    }

    pub(super) fn is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        matches!(expected, Type::Any | Type::Unknown)
            || matches!(actual, Type::Any | Type::Unknown | Type::Never)
            || expected == actual
            || self
                .constrained_type_param_supertype_for_assignability(actual)
                .is_some_and(|supertype| self.is_assignable(expected, &supertype))
            || matches!(expected, Type::Comparable)
                && ensure_comparable_key(actual, &self.struct_types, Span::new(0, 0, 0, 0)).is_ok()
            || matches!((expected, actual), (Type::Int, Type::IntRange(_)))
            || matches!((expected, actual), (Type::IntRange(expected), Type::IntRange(actual)) if expected.contains_range(*actual))
            || matches!((expected, actual), (Type::Float, Type::FloatRange(_)))
            || matches!((expected, actual), (Type::FloatRange(expected), Type::FloatRange(actual)) if expected.contains_range(*actual))
            || matches!(expected, Type::Number) && is_numeric_type(actual)
            || matches!((expected, actual), (Type::Rational, Type::Int))
            || matches!((expected, actual), (Type::Float, Type::Int))
            || matches!(expected, Type::TypeValueOf(expected) if self.type_value_instance_is_assignable(expected, actual))
            || matches!(expected, Type::TypeValueBounds { lower, upper } if self.type_value_satisfies_bounds(lower, upper, actual))
            || matches!(expected, Type::TypeValue) && Self::is_type_value_type(actual)
            || matches!((expected, actual), (Type::Message, Type::String))
            || is_byte_char_type(expected) && is_byte_char_type(actual)
            || matches!((expected, actual), (Type::Array(item), Type::String) if is_string_char_type(item))
            || matches!((expected, actual), (Type::String, Type::Array(item)) if is_string_char_type(item))
            || matches!(
                (expected, actual),
                (Type::Class(expected), Type::Class(actual))
                    if self.is_class_subtype(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Interface(expected), Type::Class(actual))
                    if self.class_implements_interface(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Modifier(expected), Type::Class(actual))
                    if self.class_implements_modifier(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Interface(expected), Type::Interface(actual))
                    if self.is_interface_subtype(actual, expected)
            )
            || matches!(
                (expected, actual),
                (Type::Array(expected), Type::Array(actual))
                    if self.is_assignable(expected, actual)
            )
            || matches!(
                (expected, actual),
                (Type::Array(expected), Type::Tuple(actual_items))
                    if actual_items
                        .iter()
                        .all(|actual| self.is_assignable(expected, actual))
            )
            || matches!(
                (expected, actual),
                (
                    Type::Map(expected_key, expected_value)
                    | Type::WeakMap(expected_key, expected_value),
                    Type::Map(actual_key, actual_value) | Type::WeakMap(actual_key, actual_value),
                )
                    if self.is_assignable(expected_key, actual_key)
                        && self.is_assignable(expected_value, actual_value)
            )
            || matches!(
                (expected, actual),
                (
                    Type::Result(expected_success, expected_error),
                    Type::Result(actual_success, actual_error),
                )
                    if self.is_assignable(expected_success, actual_success)
                        && self.is_assignable(expected_error, actual_error)
            )
            || matches!(
                (expected, actual),
                (Type::Result(expected_success, _), Type::SuccessResult(actual_success))
                    if self.is_assignable(expected_success, actual_success)
            )
            || matches!(
                (expected, actual),
                (Type::Result(_, expected_error), Type::ErrorResult(actual_error))
                    if self.is_assignable(expected_error, actual_error)
            )
            || matches!(
                (expected, actual),
                (Type::SuccessResult(expected_success), Type::Result(actual_success, actual_error))
                    if matches!(actual_error.as_ref(), Type::Never)
                        && self.is_assignable(expected_success, actual_success)
            )
            || matches!(
                (expected, actual),
                (Type::ErrorResult(expected_error), Type::Result(actual_success, actual_error))
                    if matches!(actual_success.as_ref(), Type::Never)
                        && self.is_assignable(expected_error, actual_error)
            )
            || self.parametric_builtin_is_assignable(expected, actual)
            || matches!(
                (expected, actual),
                (Type::Tuple(expected_items), Type::Tuple(actual_items))
                    if expected_items.len() == actual_items.len()
                        && expected_items
                            .iter()
                            .zip(actual_items)
                            .all(|(expected, actual)| self.is_assignable(expected, actual))
            )
            || matches!(
                (expected, actual),
                (Type::Option(expected), Type::Option(actual))
                    if self.is_assignable(expected, actual)
            )
            || self.function_is_assignable(expected, actual)
    }

    pub(super) fn parametric_builtin_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Event(expected), Type::Event(actual))
            | (Type::SubscribableEventIntrnl(expected), Type::SubscribableEventIntrnl(actual))
            | (Type::StickyEvent(expected), Type::StickyEvent(actual))
            | (Type::Awaitable(expected), Type::Awaitable(actual))
            | (Type::Subscribable(expected), Type::Subscribable(actual))
            | (Type::Listenable(expected), Type::Listenable(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::SubscribableEvent(expected), Type::SubscribableEvent(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Task(expected), Type::Task(actual)) => self.is_assignable(expected, actual),
            (Type::Generator(expected), Type::Generator(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::TypeValueOf(expected), actual) => {
                self.type_value_instance_is_assignable(expected, actual)
            }
            (Type::CastableSubtype(expected), Type::ClassType(actual))
                if self.classifiable_subset_query_class_value_is_assignable(expected, actual) =>
            {
                true
            }
            (Type::Subtype(expected), Type::ClassType(actual)) => {
                self.class_type_value_satisfies_subtype(actual, expected)
            }
            (Type::CastableSubtype(expected), Type::ClassType(actual)) => {
                self.class_type_value_is_castable_subtype(actual, expected)
            }
            (Type::ConcreteSubtype(expected), Type::ClassType(actual)) => {
                self.class_type_value_is_concrete_subtype(actual, expected)
            }
            (Type::Subtype(expected), Type::Subtype(actual)) => {
                self.subtype_constraint_implies(actual, expected)
            }
            (Type::Subtype(expected), Type::CastableSubtype(actual))
            | (Type::Subtype(expected), Type::ConcreteSubtype(actual)) => {
                self.subtype_constraint_implies(actual, expected)
            }
            (Type::CastableSubtype(expected), Type::CastableSubtype(actual))
                if self.classifiable_subset_query_base_is_assignable(expected, actual) =>
            {
                true
            }
            (Type::CastableSubtype(expected), Type::CastableSubtype(actual))
            | (Type::ConcreteSubtype(expected), Type::ConcreteSubtype(actual))
            | (Type::ClassifiableSubset(expected), Type::ClassifiableSubset(actual))
            | (Type::ClassifiableSubsetKey(expected), Type::ClassifiableSubsetKey(actual))
            | (Type::ClassifiableSubsetVar(expected), Type::ClassifiableSubsetVar(actual))
            | (Type::SuccessResult(expected), Type::SuccessResult(actual))
            | (Type::ErrorResult(expected), Type::ErrorResult(actual))
            | (Type::Modifier(expected), Type::Modifier(actual))
            | (Type::ModifierStack(expected), Type::ModifierStack(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Modifier(expected), Type::ModifierStack(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Signalable(expected), Type::Signalable(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Awaitable(expected), Type::Task(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), Some(actual.as_ref()))
            }
            (Type::Awaitable(expected), Type::SubscribableEvent(actual))
            | (Type::SubscribableEventIntrnl(expected), Type::SubscribableEvent(actual))
            | (Type::Event(expected), Type::SubscribableEvent(actual))
            | (Type::Listenable(expected), Type::SubscribableEvent(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), Some(actual.as_ref()))
            }
            (Type::Awaitable(expected), Type::Event(actual))
            | (Type::Awaitable(expected), Type::SubscribableEventIntrnl(actual))
            | (Type::Awaitable(expected), Type::StickyEvent(actual))
            | (Type::Event(expected), Type::SubscribableEventIntrnl(actual))
            | (Type::Event(expected), Type::StickyEvent(actual))
            | (Type::Awaitable(expected), Type::Listenable(actual))
            | (Type::Listenable(expected), Type::SubscribableEventIntrnl(actual))
            | (Type::Subscribable(expected), Type::Listenable(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            (Type::Signalable(expected), Type::Event(actual)) => actual
                .as_deref()
                .is_some_and(|actual| self.is_assignable(expected, actual)),
            (Type::Signalable(expected), Type::SubscribableEvent(actual)) => {
                self.is_assignable(expected, actual)
            }
            (Type::Signalable(expected), Type::SubscribableEventIntrnl(actual)) => actual
                .as_deref()
                .is_some_and(|actual| self.is_assignable(expected, actual)),
            (Type::Signalable(expected), Type::StickyEvent(actual)) => actual
                .as_deref()
                .is_some_and(|actual| self.is_assignable(expected, actual)),
            (Type::Subscribable(expected), Type::SubscribableEvent(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), Some(actual.as_ref()))
            }
            (Type::Subscribable(expected), Type::SubscribableEventIntrnl(actual)) => {
                self.optional_payload_is_assignable(expected.as_deref(), actual.as_deref())
            }
            _ => false,
        }
    }

    fn type_value_instance_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        let Some(actual) = type_value_instance_type(actual) else {
            return false;
        };
        self.is_assignable(expected, &actual)
    }

    fn type_value_satisfies_bounds(&self, lower: &Type, upper: &Type, actual: &Type) -> bool {
        let Some(actual) = type_value_instance_type(actual) else {
            return false;
        };
        self.is_assignable(upper, &actual) && self.is_assignable(&actual, lower)
    }

    fn classifiable_subset_query_base_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        let Type::TypeValueBounds { lower, upper } = expected else {
            return false;
        };
        self.is_assignable(upper, actual)
            && (self.is_assignable(actual, lower) || self.is_assignable(lower, actual))
    }

    fn classifiable_subset_query_class_value_is_assignable(
        &self,
        expected: &Type,
        actual: &str,
    ) -> bool {
        self.class_type_value_is_castable_subtype(actual, &Type::Any)
            && self.classifiable_subset_query_base_is_assignable(
                expected,
                &Type::Class(actual.to_string()),
            )
    }

    fn is_type_value_type(actual: &Type) -> bool {
        matches!(
            actual,
            Type::TypeValueOf(_)
                | Type::TypeValueBounds { .. }
                | Type::StructType(_)
                | Type::ClassType(_)
                | Type::InterfaceType(_)
                | Type::ParametricType { .. }
                | Type::Subtype(_)
                | Type::CastableSubtype(_)
                | Type::ConcreteSubtype(_)
        )
    }

    pub(super) fn class_type_value_is_castable_subtype(
        &self,
        actual: &str,
        expected: &Type,
    ) -> bool {
        self.struct_types.get(actual).is_some_and(|info| {
            info.kind == AggregateKind::Class
                && info.castable
                && self.class_type_value_satisfies_subtype(actual, expected)
        })
    }

    pub(super) fn class_type_value_is_concrete_subtype(
        &self,
        actual: &str,
        expected: &Type,
    ) -> bool {
        self.struct_types.get(actual).is_some_and(|info| {
            info.kind == AggregateKind::Class
                && info.concrete
                && self.class_type_value_satisfies_subtype(actual, expected)
        })
    }

    pub(super) fn class_type_value_satisfies_subtype(&self, actual: &str, expected: &Type) -> bool {
        match expected {
            Type::Subtype(expected) => self.class_type_value_satisfies_subtype(actual, expected),
            Type::CastableSubtype(expected) => {
                self.class_type_value_is_castable_subtype(actual, expected)
            }
            Type::ConcreteSubtype(expected) => {
                self.class_type_value_is_concrete_subtype(actual, expected)
            }
            _ => self.is_assignable(expected, &Type::Class(actual.to_string())),
        }
    }

    pub(super) fn subtype_constraint_implies(&self, actual: &Type, expected: &Type) -> bool {
        match expected {
            Type::Subtype(expected) => self.subtype_constraint_implies(actual, expected),
            Type::CastableSubtype(expected) => match actual {
                Type::Subtype(actual) => self.subtype_constraint_implies(actual, expected),
                Type::CastableSubtype(actual) => self.subtype_constraint_implies(actual, expected),
                Type::ConcreteSubtype(actual) => self.subtype_constraint_implies(actual, expected),
                _ => false,
            },
            Type::ConcreteSubtype(expected) => match actual {
                Type::ConcreteSubtype(actual) => self.subtype_constraint_implies(actual, expected),
                _ => false,
            },
            _ => match actual {
                Type::Subtype(actual)
                | Type::CastableSubtype(actual)
                | Type::ConcreteSubtype(actual) => {
                    self.subtype_constraint_implies(actual, expected)
                }
                _ => self.is_assignable(expected, actual),
            },
        }
    }

    pub(super) fn optional_payload_is_assignable(
        &self,
        expected: Option<&Type>,
        actual: Option<&Type>,
    ) -> bool {
        match (expected, actual) {
            (None, None) => true,
            (Some(expected), Some(actual)) => self.is_assignable(expected, actual),
            _ => false,
        }
    }

    pub(super) fn is_persistable_type(&self, value_type: &Type) -> bool {
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
            Type::Array(item) | Type::Option(item) => self.is_persistable_type(item),
            Type::Map(key, value) => {
                self.is_persistable_type(key) && self.is_persistable_type(value)
            }
            Type::Tuple(items) => items.iter().all(|item| self.is_persistable_type(item)),
            Type::Enum(name) => self
                .enum_types
                .get(name)
                .is_some_and(|info| info.persistable),
            Type::Struct(name) | Type::Class(name) => self
                .struct_types
                .get(name)
                .is_some_and(|info| info.persistable),
            Type::Any
            | Type::Comparable
            | Type::TypeValue
            | Type::TypeValueOf(_)
            | Type::TypeValueBounds { .. }
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

    pub(super) fn is_predicts_var_data_type(&self, value_type: &Type) -> bool {
        match value_type {
            Type::Int | Type::IntRange(_) | Type::Float | Type::FloatRange(_) | Type::Bool => true,
            Type::Option(item) | Type::Array(item) => self.is_predicts_var_data_type(item),
            Type::Map(key, value) => {
                self.is_predicts_var_data_type(key) && self.is_predicts_var_data_type(value)
            }
            Type::Enum(_) => true,
            Type::Class(name) => self
                .struct_types
                .get(name)
                .is_some_and(|info| info.kind == AggregateKind::Class),
            Type::Rational
            | Type::Number
            | Type::String
            | Type::Message
            | Type::Char
            | Type::Char8
            | Type::Char32
            | Type::None
            | Type::Any
            | Type::Comparable
            | Type::TypeValue
            | Type::TypeValueOf(_)
            | Type::TypeValueBounds { .. }
            | Type::Unknown
            | Type::Never
            | Type::Range
            | Type::EnumType(_)
            | Type::Struct(_)
            | Type::StructType(_)
            | Type::ClassType(_)
            | Type::Interface(_)
            | Type::InterfaceType(_)
            | Type::Module(_)
            | Type::Param(_, _)
            | Type::ParametricType { .. }
            | Type::WeakMap(_, _)
            | Type::Tuple(_)
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

    pub(super) fn function_is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        if let Type::Overload(overloads) = actual {
            return overloads
                .iter()
                .any(|actual| self.function_is_assignable(expected, actual));
        }

        let (
            Type::Function {
                arity: expected_arity,
                arity_range: None,
                effects: expected_effects,
                param_types: expected_params,
                return_type: expected_return,
                ..
            },
            Type::Function {
                arity: actual_arity,
                arity_range: None,
                effects: actual_effects,
                param_types: actual_params,
                return_type: actual_return,
                ..
            },
        ) = (expected, actual)
        else {
            return false;
        };

        if let (Some(expected_arity), Some(actual_arity)) = (expected_arity, actual_arity)
            && expected_arity != actual_arity
        {
            return false;
        }

        if !function_effects_are_assignable(expected_effects, actual_effects) {
            return false;
        }

        let params_match = match (expected_params, actual_params) {
            (Some(expected_params), Some(actual_params)) => {
                expected_params.len() == actual_params.len()
                    && expected_params
                        .iter()
                        .zip(actual_params)
                        .all(|(expected, actual)| self.is_assignable(actual, expected))
            }
            _ => true,
        };

        params_match && self.is_assignable(expected_return, actual_return)
    }

    pub(super) fn is_class_subtype(&self, actual: &str, expected: &str) -> bool {
        if self.is_builtin_class_subtype(actual, expected) {
            return true;
        }

        let mut current = Some(actual);
        while let Some(name) = current {
            if name == expected {
                return true;
            }
            current = self
                .struct_types
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        false
    }

    pub(super) fn is_builtin_class_subtype(&self, actual: &str, expected: &str) -> bool {
        if self.struct_types.contains_key(actual) || self.struct_types.contains_key(expected) {
            return false;
        }
        matches!(
            (actual, expected),
            ("player", "agent") | ("agent", "entity") | ("player", "entity")
        )
    }

    pub(super) fn is_interface_subtype(&self, actual: &str, expected: &str) -> bool {
        if actual == expected {
            return true;
        }
        let Some(info) = self.interface_types.get(actual) else {
            return false;
        };
        info.parents
            .iter()
            .any(|parent| self.is_interface_subtype(parent, expected))
    }

    pub(super) fn class_implements_interface(&self, actual: &str, expected: &str) -> bool {
        let mut current = Some(actual);
        while let Some(name) = current {
            let Some(info) = self.struct_types.get(name) else {
                return false;
            };
            if info
                .interfaces
                .iter()
                .any(|interface| self.is_interface_subtype(interface, expected))
            {
                return true;
            }
            current = info.base.as_deref();
        }
        false
    }

    pub(super) fn class_implements_modifier(&self, actual: &str, expected: &Type) -> bool {
        let required = modifier_evaluate_type(expected);
        let mut current = Some(actual);
        while let Some(name) = current {
            let Some(info) = self.struct_types.get(name) else {
                return false;
            };
            if info.methods.iter().any(|method| {
                method.name == "Evaluate" && self.is_assignable(&required, &method.value_type)
            }) {
                return true;
            }
            current = info.base.as_deref();
        }
        false
    }

    pub(super) fn classes_are_cast_related(&self, target: &str, source: &str) -> bool {
        self.is_class_subtype(source, target) || self.is_class_subtype(target, source)
    }

    pub(super) fn check_class_cast(
        &self,
        target: &str,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        ensure_exact_arg_count("class cast", args, 1, span)?;
        let actual_type = self
            .constrained_type_param_supertype_for_assignability(&arg_types[0])
            .unwrap_or_else(|| arg_types[0].clone());
        match &actual_type {
            Type::Class(source) if self.classes_are_cast_related(target, source) => {
                Ok(Type::Class(target.to_string()))
            }
            Type::Class(source) => Err(VerseError::check_at(
                format!("cannot cast class `{source}` to unrelated class `{target}`"),
                args[0].span,
            )),
            Type::Unknown | Type::Any => Ok(Type::Class(target.to_string())),
            other => Err(VerseError::check_at(
                format!("class cast expected class instance, got `{other}`"),
                args[0].span,
            )),
        }
    }

    fn check_type_value_cast(
        &self,
        callee: &Expr,
        callee_type: &Type,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        ensure_exact_arg_count("type value cast", args, 1, span)?;
        let target = self.type_value_cast_target(callee, callee_type);
        let Some(bound) = self.type_value_cast_bound(&target) else {
            return Err(VerseError::check_at(
                format!("type value cast target `{target}` is not a class or interface type"),
                callee.span,
            ));
        };

        let actual_type = self
            .constrained_type_param_supertype_for_assignability(&arg_types[0])
            .unwrap_or_else(|| arg_types[0].clone());
        match (&actual_type, bound) {
            (Type::Class(source), Type::Class(bound))
                if self.classes_are_cast_related(&bound, source) =>
            {
                Ok(target)
            }
            (Type::Class(source), Type::Interface(bound))
                if self.class_implements_interface(source, &bound) =>
            {
                Ok(target)
            }
            (Type::Class(_), Type::Any | Type::Unknown) => Ok(target),
            (Type::Class(source), Type::Class(bound)) => Err(VerseError::check_at(
                format!("cannot cast class `{source}` to unrelated class `{bound}`"),
                args[0].span,
            )),
            (Type::Class(source), Type::Interface(bound)) => Err(VerseError::check_at(
                format!("cannot cast class `{source}` to unrelated interface `{bound}`"),
                args[0].span,
            )),
            (Type::Unknown | Type::Any, _) => Ok(target),
            (other, _) => Err(VerseError::check_at(
                format!("type value cast expected class instance, got `{other}`"),
                args[0].span,
            )),
        }
    }

    fn type_value_cast_target(&self, callee: &Expr, callee_type: &Type) -> Type {
        if let ExprKind::Ident(name) = &callee.kind
            && let Some(
                target @ Type::Param(
                    _,
                    TypeParamConstraint::Subtype(_) | TypeParamConstraint::TypeBounds { .. },
                ),
            ) = self.resolve_type_param(name)
        {
            return target;
        }

        match callee_type {
            Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::TypeValueOf(item) => item.as_ref().clone(),
            other => other.clone(),
        }
    }

    fn type_value_cast_bound(&self, value_type: &Type) -> Option<Type> {
        match value_type {
            Type::Class(name) => Some(Type::Class(name.clone())),
            Type::Interface(name) => Some(Type::Interface(name.clone())),
            Type::Any | Type::Unknown => Some(value_type.clone()),
            Type::Param(_, TypeParamConstraint::Subtype(parent)) => self
                .type_name_to_type_name_for_assignability(parent)
                .and_then(|parent| self.type_value_cast_bound(&parent)),
            Type::Param(_, TypeParamConstraint::TypeBounds { upper, .. }) => self
                .type_name_to_type_name_for_assignability(upper)
                .and_then(|parent| self.type_value_cast_bound(&parent)),
            Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::TypeValueOf(item) => self.type_value_cast_bound(item),
            _ => None,
        }
    }

    pub(super) fn check_shuffle_call(
        &self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("`Shuffle` expected 1 arguments, got {}", args.len()),
                span,
            ));
        }
        if let CallArg::Named {
            name,
            optional,
            span,
            ..
        } = &args[0]
        {
            if name != "Input" {
                let rendered = rendered_argument_name(name, *optional);
                return Err(VerseError::check_at(
                    format!("unknown named argument `{rendered}`"),
                    *span,
                ));
            }
            if *optional {
                return Err(VerseError::check_at(
                    "parameter `Input` is not a named parameter",
                    *span,
                ));
            }
        }

        match &arg_types[0] {
            Type::Array(item_type) => Ok(Type::Array(item_type.clone())),
            Type::Unknown | Type::Any => Ok(Type::Array(Box::new(Type::Unknown))),
            other => Err(VerseError::check_at(
                format!("argument 1 expected `array`, got `{other}`"),
                call_arg_expr(&args[0]).span,
            )),
        }
    }

    pub(super) fn check_concatenate_call(
        &mut self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let arrays_type = Type::Array(Box::new(Type::Array(Box::new(Type::Unknown))));
        let param_types = vec![arrays_type.clone()];
        let param_specs = vec![ParamSpec {
            name: "Arrays".to_string(),
            value_type: arrays_type,
            named: false,
            has_default: false,
            tuple_items: None,
        }];
        self.check_call_arguments(
            (Some(1), None),
            Some(&param_types),
            Some(&param_specs),
            args,
            arg_types,
            span,
        )?;

        let item_type = infer_concatenate_item_type(args, arg_types, span)?;
        Ok(Type::Array(Box::new(item_type)))
    }

    pub(super) fn check_make_classifiable_subset_call(
        &self,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
        as_var: bool,
    ) -> Result<Type, VerseError> {
        let function_name = if as_var {
            "MakeClassifiableSubsetVar"
        } else {
            "MakeClassifiableSubset"
        };
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("`{function_name}` expected 1 arguments, got {}", args.len()),
                span,
            ));
        }
        if args[0].is_named() {
            return Err(VerseError::check_at(
                format!("`{function_name}` does not accept named arguments"),
                span,
            ));
        }

        let wrap = |item_type| {
            if as_var {
                Type::ClassifiableSubsetVar(Box::new(item_type))
            } else {
                Type::ClassifiableSubset(Box::new(item_type))
            }
        };
        match &arg_types[0] {
            Type::Array(item_type) => {
                Ok(wrap(classifiable_subset_element_type(item_type.as_ref())))
            }
            Type::Unknown | Type::Any => Ok(wrap(Type::Unknown)),
            other => Err(VerseError::check_at(
                format!("argument 1 expected `array`, got `{other}`"),
                call_arg_expr(&args[0]).span,
            )),
        }
    }

    pub(super) fn check_make_result_call(
        &self,
        callee: &Expr,
        args: &[CallArg],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let name = make_result_callee_name(callee).expect("caller should check result callee");
        if args.len() != 1 {
            return Err(VerseError::check_at(
                format!("`{name}` expected 1 arguments, got {}", args.len()),
                span,
            ));
        }
        if args[0].is_named() {
            return Err(VerseError::check_at(
                format!("`{name}` does not accept named arguments"),
                span,
            ));
        }

        let payload_type = arg_types
            .first()
            .cloned()
            .expect("arity check should leave one argument type");
        match name {
            "MakeSuccess" => Ok(Type::Result(Box::new(payload_type), Box::new(Type::Never))),
            "MakeError" => Ok(Type::Result(Box::new(Type::Never), Box::new(payload_type))),
            _ => unreachable!("only MakeSuccess and MakeError are result constructors"),
        }
    }

    pub(super) fn check_bracket_call(
        &mut self,
        call: &Expr,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Type, VerseError> {
        let mut checked_arg_types = None;
        if let ExprKind::Member { object, name } = &callee.kind {
            let object_type = self.check_expr(object)?;
            let extension_type = self.extension_member_type(&object_type, name, callee.span)?;
            let arg_types = args
                .iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?;

            match &object_type {
                Type::Array(item) => {
                    match self.check_array_method(name, item, args, &arg_types, callee.span) {
                        Ok(value_type) => {
                            if Self::array_method_is_failable(name) {
                                self.ensure_failable_expression_allowed(call.span)?;
                            }
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::String => {
                    match self.check_array_method(name, &Type::Char, args, &arg_types, callee.span)
                    {
                        Ok(value_type) => {
                            if Self::array_method_is_failable(name) {
                                self.ensure_failable_expression_allowed(call.span)?;
                            }
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::Int
                | Type::IntRange(_)
                | Type::Float
                | Type::FloatRange(_)
                | Type::Rational
                | Type::Number => {
                    match self.check_number_method(
                        name,
                        &object_type,
                        args,
                        &arg_types,
                        callee.span,
                    ) {
                        Ok(value_type) => {
                            self.ensure_failable_expression_allowed(call.span)?;
                            return Ok(value_type);
                        }
                        Err(error) if extension_type.is_none() => return Err(error),
                        Err(_) => checked_arg_types = Some(arg_types),
                    }
                }
                Type::Unknown | Type::Any => return Ok(Type::Unknown),
                Type::Struct(_)
                | Type::Class(_)
                | Type::Module(_)
                | Type::Result(_, _)
                | Type::SuccessResult(_)
                | Type::ErrorResult(_)
                | Type::Event(_)
                | Type::SubscribableEventIntrnl(_)
                | Type::StickyEvent(_)
                | Type::Task(_)
                | Type::Generator(_)
                | Type::Subtype(_)
                | Type::CastableSubtype(_)
                | Type::ConcreteSubtype(_)
                | Type::ClassifiableSubset(_)
                | Type::ClassifiableSubsetVar(_)
                | Type::Awaitable(_)
                | Type::Signalable(_)
                | Type::Subscribable(_)
                | Type::Listenable(_)
                | Type::Param(
                    _,
                    TypeParamConstraint::Subtype(_) | TypeParamConstraint::TypeBounds { .. },
                ) => checked_arg_types = Some(arg_types),
                other => {
                    if extension_type.is_some() {
                        checked_arg_types = Some(arg_types);
                    } else {
                        return Err(VerseError::check_at(
                            format!("type `{other}` has no bracket method `{name}`"),
                            callee.span,
                        ));
                    }
                }
            }
        }

        let callee_type = self.check_callee_expr(callee)?;
        let arg_types = if let Some(arg_types) = checked_arg_types {
            arg_types
        } else {
            args.iter()
                .map(|arg| self.check_expr(arg))
                .collect::<Result<Vec<_>, _>>()?
        };

        if is_fits_in_player_map_callee(callee) {
            ensure_exact_arg_count("FitsInPlayerMap", args, 1, callee.span)?;
            self.ensure_failable_expression_allowed(call.span)?;
            return Ok(arg_types[0].clone());
        }

        match callee_type {
            Type::Array(item) => {
                ensure_exact_arg_count("array index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "array index", args[0].span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(*item)
            }
            Type::String => {
                ensure_exact_arg_count("string index", args, 1, callee.span)?;
                ensure_int_index_type(&arg_types[0], "string index", args[0].span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(Type::Char)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) => {
                ensure_exact_arg_count("map lookup", args, 1, callee.span)?;
                let value_type =
                    self.check_map_like_lookup(&key, &value, &arg_types[0], &args[0])?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(value_type)
            }
            Type::Function {
                arity,
                arity_range,
                effects,
                mut param_types,
                mut param_specs,
                return_type,
            } => {
                if !has_effect(&effects, "decides") {
                    return Err(VerseError::check_at(
                        "functions without `<decides>` must be called with `()`",
                        callee.span,
                    ));
                }
                if self.in_failure_context() {
                    ensure_callable_in_failure_context(&effects, callee.span)?;
                }
                self.ensure_callable_in_async_context(&effects, callee.span)?;
                self.ensure_current_function_allows_call_effects(&effects, callee.span)?;
                let call_args = positional_call_args(args);
                let mut return_type = *return_type;
                if let Some(inferred) = self
                    .infer_function_type_params(
                        param_types.as_deref(),
                        Some(&return_type),
                        &arg_types,
                    )
                    .filter(|inferred| !inferred.is_empty())
                {
                    self.ensure_inferred_type_param_constraints(
                        param_types.as_deref(),
                        Some(&return_type),
                        &inferred,
                        callee.span,
                    )?;
                    if let Some(types) = param_types.as_mut() {
                        for value_type in types {
                            *value_type = self.substitute_type_params_runtime(
                                value_type,
                                &inferred,
                                callee.span,
                            )?;
                        }
                    }
                    if let Some(specs) = param_specs.as_mut() {
                        for spec in specs {
                            *spec =
                                self.substitute_param_spec_runtime(spec, &inferred, callee.span)?;
                        }
                    }
                    return_type =
                        self.substitute_type_params_runtime(&return_type, &inferred, callee.span)?;
                }
                self.check_call_arguments(
                    (arity, arity_range),
                    param_types.as_deref(),
                    param_specs.as_deref(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(return_type)
            }
            Type::Overload(overloads) => {
                let call_args = positional_call_args(args);
                let return_type = self.check_overloaded_call(
                    &overloads,
                    true,
                    self.in_failure_context(),
                    &call_args,
                    &arg_types,
                    callee.span,
                )?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(return_type)
            }
            Type::ClassType(target) => {
                let value_type = self.check_class_cast(&target, args, &arg_types, callee.span)?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(value_type)
            }
            Type::Subtype(_)
            | Type::CastableSubtype(_)
            | Type::ConcreteSubtype(_)
            | Type::TypeValueOf(_) => {
                let value_type = self.check_type_value_cast(
                    callee,
                    &callee_type,
                    args,
                    &arg_types,
                    callee.span,
                )?;
                self.ensure_failable_expression_allowed(call.span)?;
                Ok(value_type)
            }
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("cannot use `[]` with value of type `{other}`"),
                callee.span,
            )),
        }
    }

    pub(super) fn check_array_method(
        &mut self,
        name: &str,
        item_type: &Type,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        match name {
            "Slice" => {
                ensure_arg_count_range(name, args, 1, 2, span)?;
                for (arg, arg_type) in args.iter().zip(arg_types) {
                    ensure_int_index_type(arg_type, "`Slice` argument", arg.span)?;
                }
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Find" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!("`Find` expected `{item_type}`, got `{}`", arg_types[0])
                })?;
                Ok(Type::Int)
            }
            "RemoveFirstElement" | "RemoveAllElements" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!("`{name}` expected `{item_type}`, got `{}`", arg_types[0])
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "RemoveElement" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_int_index_type(&arg_types[0], "`RemoveElement` index", args[0].span)?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Remove" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`Remove` start", args[0].span)?;
                ensure_int_index_type(&arg_types[1], "`Remove` end", args[1].span)?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceFirstElement" | "ReplaceAllElements" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_comparable_key(item_type, &self.struct_types, args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[0], &args[0], || {
                    format!(
                        "`{name}` old value expected `{item_type}`, got `{}`",
                        arg_types[0]
                    )
                })?;
                self.ensure_expr_assignable(item_type, &arg_types[1], &args[1], || {
                    format!(
                        "`{name}` new value expected `{item_type}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceElement" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`ReplaceElement` index", args[0].span)?;
                self.ensure_expr_assignable(item_type, &arg_types[1], &args[1], || {
                    format!(
                        "`ReplaceElement` new value expected `{item_type}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "Insert" => {
                ensure_exact_arg_count(name, args, 2, span)?;
                ensure_int_index_type(&arg_types[0], "`Insert` index", args[0].span)?;
                let expected = Type::Array(Box::new(item_type.clone()));
                self.ensure_expr_assignable(&expected, &arg_types[1], &args[1], || {
                    format!(
                        "`Insert` values expected `{expected}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            "ReplaceAll" => {
                let effects = vec!["transacts".to_string()];
                self.ensure_current_function_allows_call_effects(&effects, span)?;
                ensure_exact_arg_count(name, args, 2, span)?;
                let expected = Type::Array(Box::new(item_type.clone()));
                self.ensure_expr_assignable(&expected, &arg_types[0], &args[0], || {
                    format!(
                        "`ReplaceAll` pattern expected `{expected}`, got `{}`",
                        arg_types[0]
                    )
                })?;
                self.ensure_expr_assignable(&expected, &arg_types[1], &args[1], || {
                    format!(
                        "`ReplaceAll` replacement expected `{expected}`, got `{}`",
                        arg_types[1]
                    )
                })?;
                Ok(Type::Array(Box::new(item_type.clone())))
            }
            _ => Err(VerseError::check_at(
                format!("unknown array method `{name}`"),
                span,
            )),
        }
    }

    pub(super) fn check_number_method(
        &self,
        name: &str,
        receiver_type: &Type,
        args: &[Expr],
        arg_types: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        match name {
            "IsFinite" => {
                ensure_exact_arg_count(name, args, 0, span)?;
                Ok(receiver_type.clone())
            }
            "IsAlmostZero" => {
                ensure_exact_arg_count(name, args, 1, span)?;
                ensure_number_like(
                    &arg_types[0],
                    "`IsAlmostZero` AbsoluteTolerance",
                    args[0].span,
                )?;
                Ok(Type::None)
            }
            _ => Err(VerseError::check_at(
                format!("unknown number method `{name}`"),
                span,
            )),
        }
    }

    pub(super) fn array_method_is_failable(name: &str) -> bool {
        matches!(
            name,
            "Slice"
                | "Find"
                | "RemoveFirstElement"
                | "RemoveElement"
                | "Remove"
                | "ReplaceFirstElement"
                | "ReplaceElement"
                | "Insert"
        )
    }

    pub(super) fn ensure_aggregate_member_accessible(
        &self,
        owner: &str,
        access: AccessLevel,
        scopes: &[String],
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if self.interface_types.contains_key(owner) {
            self.ensure_interface_member_accessible(
                owner,
                access,
                scopes,
                member_name,
                member_kind,
                span,
            )
        } else {
            self.ensure_class_member_accessible(
                owner,
                access,
                scopes,
                member_name,
                member_kind,
                span,
            )
        }
    }

    pub(super) fn ensure_class_member_accessible(
        &self,
        owner_class: &str,
        access: AccessLevel,
        scopes: &[String],
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if self
            .inaccessible_scoped_enclosing_aggregate(owner_class)
            .is_some()
        {
            return Err(VerseError::check_at(
                format!("{member_kind} `{member_name}` is scoped to class `{owner_class}`"),
                span,
            ));
        }
        if let Some(scope) = self.inaccessible_internal_enclosing_aggregate(owner_class) {
            let parent = aggregate_module_name(&scope).unwrap_or("<root module>");
            let hidden_member = aggregate_unqualified_name(&scope);
            return Err(VerseError::check_at(
                format!("member `{hidden_member}` is internal to module `{parent}`"),
                span,
            ));
        }

        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal => {
                if aggregate_module_name(owner_class).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| {
                        self.current_module_is_same_or_child_of(module)
                            || self.aggregate_or_parent_scoped_accessible(owner_class)
                    },
                ) {
                    Ok(())
                } else {
                    let module_name = aggregate_module_name(owner_class).unwrap_or("<root module>");
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is internal to module `{module_name}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Scoped => {
                if aggregate_module_name(owner_class).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| self.current_module_is_same_or_child_of(module),
                ) || self.scoped_accessible(scopes)
                    || self.aggregate_or_parent_scoped_accessible(owner_class)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("{member_kind} `{member_name}` is scoped to class `{owner_class}`"),
                        span,
                    ))
                }
            }
            AccessLevel::Private => {
                if self
                    .class_context
                    .last()
                    .is_some_and(|current| current == owner_class)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is private to class `{owner_class}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Protected => {
                if self.class_context.last().is_some_and(|current| {
                    current == owner_class || self.is_class_subtype(current, owner_class)
                }) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is protected in class `{owner_class}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    pub(super) fn ensure_class_constructor_accessible(
        &self,
        owner_class: &str,
        access: AccessLevel,
        scopes: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_class_constructor_accessible_with_protected_subclass(
            owner_class,
            access,
            scopes,
            false,
            span,
        )
    }

    pub(super) fn ensure_base_class_constructor_accessible(
        &self,
        owner_class: &str,
        access: AccessLevel,
        scopes: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_class_constructor_accessible_with_protected_subclass(
            owner_class,
            access,
            scopes,
            true,
            span,
        )
    }

    fn ensure_class_constructor_accessible_with_protected_subclass(
        &self,
        owner_class: &str,
        access: AccessLevel,
        scopes: &[String],
        allow_protected_subclass: bool,
        span: Span,
    ) -> Result<(), VerseError> {
        if self
            .inaccessible_scoped_enclosing_aggregate(owner_class)
            .is_some()
        {
            return Err(VerseError::check_at(
                format!("class constructor `{owner_class}` is scoped to class `{owner_class}`"),
                span,
            ));
        }
        if let Some(scope) = self.inaccessible_internal_enclosing_aggregate(owner_class) {
            let parent = aggregate_module_name(&scope).unwrap_or("<root module>");
            let hidden_member = aggregate_unqualified_name(&scope);
            return Err(VerseError::check_at(
                format!("member `{hidden_member}` is internal to module `{parent}`"),
                span,
            ));
        }

        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal => {
                if aggregate_module_name(owner_class).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| {
                        self.current_module_is_same_or_child_of(module)
                            || self.aggregate_or_parent_scoped_accessible(owner_class)
                    },
                ) {
                    Ok(())
                } else {
                    let module_name = aggregate_module_name(owner_class).unwrap_or("<root module>");
                    Err(VerseError::check_at(
                        format!(
                            "class constructor `{owner_class}` is internal to module `{module_name}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Scoped => {
                if aggregate_module_name(owner_class).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| self.current_module_is_same_or_child_of(module),
                ) || self.scoped_accessible(scopes)
                    || self.aggregate_or_parent_scoped_accessible(owner_class)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "class constructor `{owner_class}` is scoped to class `{owner_class}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Private => {
                if self
                    .class_context
                    .last()
                    .is_some_and(|current| current == owner_class)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("class constructor `{owner_class}` is private"),
                        span,
                    ))
                }
            }
            AccessLevel::Protected => {
                if allow_protected_subclass
                    || self.class_context.last().is_some_and(|current| {
                        current == owner_class || self.is_class_subtype(current, owner_class)
                    })
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("class constructor `{owner_class}` is protected"),
                        span,
                    ))
                }
            }
        }
    }

    pub(super) fn ensure_interface_member_accessible(
        &self,
        owner_interface: &str,
        access: AccessLevel,
        scopes: &[String],
        member_name: &str,
        member_kind: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if self
            .inaccessible_scoped_enclosing_aggregate(owner_interface)
            .is_some()
        {
            return Err(VerseError::check_at(
                format!("{member_kind} `{member_name}` is scoped to interface `{owner_interface}`"),
                span,
            ));
        }
        if let Some(scope) = self.inaccessible_internal_enclosing_aggregate(owner_interface) {
            let parent = aggregate_module_name(&scope).unwrap_or("<root module>");
            let hidden_member = aggregate_unqualified_name(&scope);
            return Err(VerseError::check_at(
                format!("member `{hidden_member}` is internal to module `{parent}`"),
                span,
            ));
        }

        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Internal => {
                if aggregate_module_name(owner_interface).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| {
                        self.current_module_is_same_or_child_of(module)
                            || self.aggregate_or_parent_scoped_accessible(owner_interface)
                    },
                ) {
                    Ok(())
                } else {
                    let module_name =
                        aggregate_module_name(owner_interface).unwrap_or("<root module>");
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is internal to module `{module_name}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Scoped => {
                if aggregate_module_name(owner_interface).map_or_else(
                    || self.current_module_name().is_none(),
                    |module| self.current_module_is_same_or_child_of(module),
                ) || self.scoped_accessible(scopes)
                    || self.aggregate_or_parent_scoped_accessible(owner_interface)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is scoped to interface `{owner_interface}`"
                        ),
                        span,
                    ))
                }
            }
            AccessLevel::Private => Err(VerseError::check_at(
                format!(
                    "{member_kind} `{member_name}` is private to interface `{owner_interface}`"
                ),
                span,
            )),
            AccessLevel::Protected => {
                if self.class_context.last().is_some_and(|current| {
                    self.class_implements_interface(current, owner_interface)
                }) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "{member_kind} `{member_name}` is protected in interface `{owner_interface}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    pub(super) fn ensure_module_member_accessible(
        &self,
        module_name: &str,
        access: AccessLevel,
        member_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(scope) = self.inaccessible_scoped_enclosing_module(module_name) {
            return Err(VerseError::check_at(
                format!("member `{member_name}` is scoped to `{scope}`"),
                span,
            ));
        }
        if let Some(scope) = self.inaccessible_internal_enclosing_module(module_name) {
            let parent = aggregate_module_name(&scope).unwrap_or("<root module>");
            let hidden_member = aggregate_unqualified_name(&scope);
            return Err(VerseError::check_at(
                format!("member `{hidden_member}` is internal to module `{parent}`"),
                span,
            ));
        }

        match access {
            AccessLevel::Public => Ok(()),
            AccessLevel::Scoped => {
                if self.current_module_is_same_or_child_of(module_name)
                    || self.module_member_scoped_accessible(module_name, member_name)
                    || self.module_or_parent_scoped_accessible(module_name)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("member `{member_name}` is scoped to `{module_name}`"),
                        span,
                    ))
                }
            }
            AccessLevel::Internal | AccessLevel::Private | AccessLevel::Protected => {
                if self.current_module_is_same_or_child_of(module_name)
                    || self.module_or_parent_scoped_accessible(module_name)
                {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!("member `{member_name}` is internal to module `{module_name}`"),
                        span,
                    ))
                }
            }
        }
    }

    pub(super) fn check_member(
        &mut self,
        object_type: &Type,
        name: &str,
        span: Span,
        allow_extension_method: bool,
    ) -> Result<Type, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.check_member(&supertype, name, span, allow_extension_method);
        }

        if let Type::Module(module_name) = object_type {
            let Some(info) = self.module_types.get(module_name) else {
                return Err(VerseError::check_at(
                    format!("unknown module `{module_name}`"),
                    span,
                ));
            };
            let Some(value_type) = info.members.get(name) else {
                let qualified = format!("{module_name}.{name}");
                if let Some(alias_name) = self.resolve_type_alias_reference(&qualified, span)? {
                    let value_type = self.resolve_type_alias(&alias_name, &mut Vec::new())?;
                    return Ok(Type::TypeValueOf(Box::new(value_type)));
                }
                return Err(VerseError::check_at(
                    format!("module `{module_name}` has no member `{name}`"),
                    span,
                ));
            };
            let access = info
                .member_access
                .get(name)
                .copied()
                .unwrap_or(AccessLevel::Internal);
            self.ensure_module_member_accessible(module_name, access, name, span)?;
            return Ok(value_type.clone());
        }

        if let Type::EnumType(enum_name) = object_type {
            let Some(info) = self.enum_types.get(enum_name) else {
                return Err(VerseError::check_at(
                    format!("unknown enum `{enum_name}`"),
                    span,
                ));
            };
            if info.variants.iter().any(|variant| variant == name) {
                return Ok(Type::Enum(enum_name.clone()));
            }
            return Err(VerseError::check_at(
                format!("enum `{enum_name}` has no value `{name}`"),
                span,
            ));
        }

        if let Type::Result(success_type, error_type) = object_type {
            return match name {
                "GetSuccess" => Ok(result_accessor_type(success_type.as_ref())),
                "GetError" => Ok(result_accessor_type(error_type.as_ref())),
                _ => Err(VerseError::check_at(
                    format!("interface `{object_type}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Type::SuccessResult(success_type) = object_type {
            return match name {
                "Success" => Ok(success_type.as_ref().clone()),
                "GetSuccess" => Ok(result_present_accessor_type(success_type.as_ref())),
                "GetError" => Ok(result_accessor_type(&Type::Never)),
                _ => Err(VerseError::check_at(
                    format!("class `{object_type}` has no member `{name}`"),
                    span,
                )),
            };
        }

        if let Type::ErrorResult(error_type) = object_type {
            return match name {
                "Error" => Ok(error_type.as_ref().clone()),
                "GetSuccess" => Ok(result_accessor_type(&Type::Never)),
                "GetError" => Ok(result_present_accessor_type(error_type.as_ref())),
                _ => Err(VerseError::check_at(
                    format!("class `{object_type}` has no member `{name}`"),
                    span,
                )),
            };
        }

        match object_type {
            Type::Event(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Signal" => Ok(signal_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::SubscribableEvent(payload) => {
                return match name {
                    "Await" => Ok(await_type(Some(payload.as_ref()))),
                    "Signal" => Ok(subscribable_event_signal_type(Some(payload.as_ref()))),
                    "Subscribe" => Ok(subscribe_type(Some(payload.as_ref()))),
                    "Broadcast" => Ok(subscribable_event_broadcast_type(payload.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::SubscribableEventIntrnl(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Signal" => Ok(subscribable_event_signal_type(payload.as_deref())),
                    "Subscribe" => Ok(subscribe_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::StickyEvent(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Signal" => Ok(signal_type(payload.as_deref())),
                    "IsSignaled" => Ok(sticky_event_is_signaled_type()),
                    "ClearSignal" => Ok(sticky_event_clear_signal_type()),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Task(payload) => {
                return match name {
                    "Await" => Ok(await_type(Some(payload.as_ref()))),
                    "Cancel" => Ok(task_cancel_type()),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::ClassifiableSubset(item) => {
                return match name {
                    "Contains" => Ok(classifiable_subset_contains_type(item.as_ref())),
                    "NotContains" => Ok(classifiable_subset_not_contains_type(item.as_ref())),
                    "ContainsAny" | "ContainsAll" => {
                        Ok(classifiable_subset_contains_many_type(item.as_ref()))
                    }
                    "ContainsNone" => Ok(classifiable_subset_contains_none_type(item.as_ref())),
                    "FilterByType" => Ok(classifiable_subset_filter_by_type_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::ClassifiableSubsetVar(item) => {
                return match name {
                    "Read" => Ok(classifiable_subset_var_read_type(item.as_ref())),
                    "Write" => Ok(classifiable_subset_var_write_type(item.as_ref())),
                    "Add" => Ok(classifiable_subset_var_add_type(item.as_ref())),
                    "Remove" => Ok(classifiable_subset_var_remove_type(item.as_ref())),
                    "Contains" => Ok(classifiable_subset_contains_type(item.as_ref())),
                    "NotContains" => Ok(classifiable_subset_not_contains_type(item.as_ref())),
                    "ContainsAny" | "ContainsAll" => {
                        Ok(classifiable_subset_contains_many_type(item.as_ref()))
                    }
                    "ContainsNone" => Ok(classifiable_subset_contains_none_type(item.as_ref())),
                    "FilterByType" => Ok(classifiable_subset_filter_by_type_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Modifier(item) => {
                return match name {
                    "Evaluate" => Ok(modifier_evaluate_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::ModifierStack(item) => {
                return match name {
                    "FirstPosition" | "LastPosition" => Ok(Type::Rational),
                    "Evaluate" => Ok(modifier_evaluate_type(item.as_ref())),
                    "AddModifier" => Ok(modifier_stack_add_modifier_type(item.as_ref())),
                    _ => Err(VerseError::check_at(
                        format!("class `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Awaitable(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Signalable(payload) => {
                return match name {
                    "Signal" => Ok(signal_type(Some(payload.as_ref()))),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Subscribable(payload) => {
                return match name {
                    "Subscribe" => Ok(subscribe_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            Type::Listenable(payload) => {
                return match name {
                    "Await" => Ok(await_type(payload.as_deref())),
                    "Subscribe" => Ok(subscribe_type(payload.as_deref())),
                    _ => Err(VerseError::check_at(
                        format!("interface `{object_type}` has no member `{name}`"),
                        span,
                    )),
                };
            }
            _ => {}
        }

        if let Type::Interface(interface_name) = object_type {
            let Some(info) = self.interface_types.get(interface_name) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface_name}`"),
                    span,
                ));
            };
            if let Some(field) = info.fields.iter().find(|field| field.name == name) {
                let owner = field.owner.as_deref().unwrap_or(interface_name);
                self.ensure_aggregate_member_accessible(
                    owner,
                    field.access,
                    &field.scopes,
                    name,
                    "field",
                    span,
                )?;
                return Ok(field.value_type.clone());
            }
            if let Some(method_type) =
                method_group_type(info.methods.iter().filter(|method| method.name == name))
            {
                for method in info.methods.iter().filter(|method| method.name == name) {
                    let owner = method.owner.as_deref().unwrap_or(interface_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        method.access,
                        &method.scopes,
                        name,
                        "method",
                        span,
                    )?;
                }
                if matches!(method_type, Type::Overload(_)) && !allow_extension_method {
                    return Err(VerseError::check_at(
                        format!("overloaded method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            return Err(VerseError::check_at(
                format!("interface `{interface_name}` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "session" && !self.struct_types.contains_key(class_name))
        {
            if name == "Environment" {
                return Ok(Type::Function {
                    arity: Some(0),
                    arity_range: None,
                    effects: vec!["transacts".to_string()],
                    param_types: Some(Vec::new()),
                    param_specs: None,
                    return_type: Box::new(Type::Enum("session_environment".to_string())),
                });
            }
            return Err(VerseError::check_at(
                format!("class `session` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "player" && !self.struct_types.contains_key(class_name))
        {
            if name == "IsActive" {
                return Ok(Type::Function {
                    arity: Some(0),
                    arity_range: None,
                    effects: vec![
                        "reads".to_string(),
                        "computes".to_string(),
                        "decides".to_string(),
                    ],
                    param_types: Some(Vec::new()),
                    param_specs: None,
                    return_type: Box::new(Type::None),
                });
            }
            return Err(VerseError::check_at(
                format!("class `player` has no member `{name}`"),
                span,
            ));
        }

        if matches!(object_type, Type::Class(class_name) if class_name == "team" && !self.struct_types.contains_key(class_name))
        {
            return Err(VerseError::check_at(
                format!("class `team` has no member `{name}`"),
                span,
            ));
        }

        if let Type::Struct(aggregate_name) | Type::Class(aggregate_name) = object_type {
            let Some(info) = self.struct_types.get(aggregate_name) else {
                let label = if matches!(object_type, Type::Class(_)) {
                    "class"
                } else {
                    "struct"
                };
                return Err(VerseError::check_at(
                    format!("unknown {label} `{aggregate_name}`"),
                    span,
                ));
            };
            let label = match info.kind {
                AggregateKind::Struct => "struct",
                AggregateKind::Class => "class",
            };
            let member_label = if info.kind == AggregateKind::Class {
                "member"
            } else {
                "field"
            };
            if let Some(field) = info.fields.iter().find(|field| field.name == name) {
                if info.kind == AggregateKind::Class {
                    let owner = field.owner.as_deref().unwrap_or(aggregate_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        field.access,
                        &field.scopes,
                        name,
                        "field",
                        span,
                    )?;
                }
                return Ok(field.value_type.clone());
            }
            if info.kind == AggregateKind::Class
                && let Some(method_type) =
                    method_group_type(info.methods.iter().filter(|method| method.name == name))
            {
                for method in info.methods.iter().filter(|method| method.name == name) {
                    let owner = method.owner.as_deref().unwrap_or(aggregate_name);
                    self.ensure_aggregate_member_accessible(
                        owner,
                        method.access,
                        &method.scopes,
                        name,
                        "method",
                        span,
                    )?;
                }
                if matches!(method_type, Type::Overload(_)) && !allow_extension_method {
                    return Err(VerseError::check_at(
                        format!("overloaded method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }
            if info.kind == AggregateKind::Class && info.castable && name == "IsOfType" {
                return Ok(castable_instance_is_of_type_type());
            }
            if let Some(method_type) = self.extension_member_type(object_type, name, span)? {
                if !allow_extension_method {
                    if let Some(return_type) = self.type_value_extension_accessor_return_type(
                        object_type,
                        &method_type,
                        span,
                    )? {
                        return Ok(return_type);
                    }
                    return Err(VerseError::check_at(
                        format!("extension method `{name}` must be called"),
                        span,
                    ));
                }
                return Ok(method_type);
            }

            return Err(VerseError::check_at(
                format!("{label} `{aggregate_name}` has no {member_label} `{name}`"),
                span,
            ));
        }

        if let Some(method_type) = self.extension_member_type(object_type, name, span)? {
            if !allow_extension_method {
                if let Some(return_type) =
                    self.type_value_extension_accessor_return_type(object_type, &method_type, span)?
                {
                    return Ok(return_type);
                }
                return Err(VerseError::check_at(
                    format!("extension method `{name}` must be called"),
                    span,
                ));
            }
            return Ok(method_type);
        }

        if name != "Length" {
            return Err(VerseError::check_at(
                format!("unknown member `{name}` on type `{object_type}`"),
                span,
            ));
        }

        match object_type {
            Type::Array(_) | Type::Map(_, _) | Type::String => Ok(Type::Int),
            Type::Unknown | Type::Any => Ok(Type::Unknown),
            other => Err(VerseError::check_at(
                format!("type `{other}` has no member `Length`"),
                span,
            )),
        }
    }

    fn type_value_extension_accessor_return_type(
        &mut self,
        object_type: &Type,
        method_type: &Type,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if !Self::is_type_value_type(object_type) {
            return Ok(None);
        }
        let Type::Function {
            arity,
            effects,
            param_types,
            return_type,
            ..
        } = method_type
        else {
            return Ok(None);
        };
        if *arity != Some(0)
            || param_types
                .as_ref()
                .is_some_and(|params| !params.is_empty())
        {
            return Ok(None);
        }
        if has_effect(effects, "decides") {
            return Ok(None);
        }
        if self.in_failure_context() {
            ensure_callable_in_failure_context(effects, span)?;
        }
        self.ensure_callable_in_async_context(effects, span)?;
        self.ensure_current_function_allows_call_effects(effects, span)?;
        Ok(Some(return_type.as_ref().clone()))
    }

    pub(super) fn extension_member_type(
        &mut self,
        object_type: &Type,
        name: &str,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.extension_member_type(&supertype, name, span);
        }

        let Some(methods) = self.extension_methods.get(name) else {
            return Ok(None);
        };
        let methods = methods.clone();
        let mut candidates = Vec::new();
        for method in methods.iter() {
            if self.extension_method_is_visible(method, name, span)? {
                if let Some(method_type) =
                    self.extension_method_type_for_receiver(method, object_type, span)?
                {
                    candidates.push(method_type);
                }
            }
        }

        Ok(match candidates.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Type::Overload(candidates)),
        })
    }

    pub(super) fn qualified_extension_member_type(
        &mut self,
        object_type: &Type,
        qualifier: &str,
        name: &str,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if let Some(supertype) = self.constrained_type_param_supertype(object_type, span)? {
            return self.qualified_extension_member_type(&supertype, qualifier, name, span);
        }

        let Some(methods) = self.extension_methods.get(name) else {
            return Ok(None);
        };
        let methods = methods.clone();
        let mut candidates = Vec::new();
        for method in methods
            .iter()
            .filter(|method| extension_method_has_qualifier(method, qualifier))
        {
            if self.extension_method_is_visible(method, name, span)? {
                if let Some(method_type) =
                    self.extension_method_type_for_receiver(method, object_type, span)?
                {
                    candidates.push(method_type);
                }
            }
        }

        Ok(match candidates.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Type::Overload(candidates)),
        })
    }

    fn extension_method_type_for_receiver(
        &mut self,
        method: &ExtensionMethodInfo,
        object_type: &Type,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        if self.is_assignable(&method.receiver_type, object_type) {
            return Ok(Some(method.method_type.clone()));
        }

        let receiver_types = [method.receiver_type.clone()];
        let object_types = [object_type.clone()];
        let Some(inferred) = self
            .infer_function_type_params(Some(&receiver_types), None, &object_types)
            .filter(|inferred| !inferred.is_empty())
        else {
            return Ok(None);
        };

        let receiver_type =
            self.substitute_type_params_runtime(&method.receiver_type, &inferred, span)?;
        if !self.is_assignable(&receiver_type, object_type) {
            return Ok(None);
        }

        Ok(Some(self.substitute_type_params_runtime(
            &method.method_type,
            &inferred,
            span,
        )?))
    }

    pub(super) fn extension_method_is_visible(
        &self,
        method: &ExtensionMethodInfo,
        name: &str,
        span: Span,
    ) -> Result<bool, VerseError> {
        match method.module_name.as_deref() {
            None => Ok(true),
            Some(module_name) if self.current_module_name().as_deref() == Some(module_name) => {
                Ok(true)
            }
            Some(module_name) => {
                let imported = self
                    .scope_imports
                    .iter()
                    .rev()
                    .any(|imports| imports.iter().any(|import| import == module_name));
                if !imported {
                    return Ok(false);
                }
                if method.access == AccessLevel::Scoped {
                    if self.scoped_accessible(&method.scopes) {
                        return Ok(true);
                    }
                    return Err(VerseError::check_at(
                        format!("member `{name}` is scoped to `{module_name}`"),
                        span,
                    ));
                }
                self.ensure_module_member_accessible(module_name, method.access, name, span)?;
                Ok(true)
            }
        }
    }

    pub(super) fn check_unary(&mut self, op: UnaryOp, expr: &Expr) -> Result<Type, VerseError> {
        let value = self.check_expr(expr)?;
        match op {
            UnaryOp::Positive => {
                ensure_number_like(&value, "unary `+`", expr.span)?;
                Ok(value)
            }
            UnaryOp::Negate => {
                ensure_number_like(&value, "unary `-`", expr.span)?;
                Ok(value)
            }
            UnaryOp::Not => {
                ensure_bool_like(&value, "`not`", expr.span)?;
                Ok(Type::Bool)
            }
        }
    }

    pub(super) fn check_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
    ) -> Result<Type, VerseError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match op {
            BinaryOp::Add => {
                let result = check_add(&left_type, left.span, &right_type, right.span)?;
                if matches!(
                    (&left_type, &right_type),
                    (Type::ClassifiableSubset(_), Type::ClassifiableSubset(_))
                        | (
                            Type::ClassifiableSubsetVar(_),
                            Type::ClassifiableSubsetVar(_)
                        )
                ) {
                    self.ensure_current_function_allows_call_effects(
                        &["transacts".to_string()],
                        left.span.through(right.span),
                    )?;
                }
                Ok(result)
            }
            BinaryOp::Subtract => check_subtract(&left_type, left.span, &right_type, right.span),
            BinaryOp::Multiply => check_multiply(&left_type, left.span, &right_type, right.span),
            BinaryOp::Remainder => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
                Ok(unify_numeric_types(&left_type, &right_type))
            }
            BinaryOp::Divide => check_divide(&left_type, left.span, &right_type, right.span),
            BinaryOp::Range => {
                if self.range_context_depth == 0 {
                    return Err(VerseError::check_at(
                        "range expressions are only valid in `for` expressions",
                        left.span.through(right.span),
                    ));
                }
                ensure_number_like(&left_type, "range start", left.span)?;
                ensure_number_like(&right_type, "range end", right.span)?;
                Ok(Type::Range)
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                ensure_equality_comparable(&left_type, &self.struct_types, left.span)?;
                ensure_equality_comparable(&right_type, &self.struct_types, right.span)?;
                Ok(Type::Bool)
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                ensure_number_like(&left_type, "left operand", left.span)?;
                ensure_number_like(&right_type, "right operand", right.span)?;
                Ok(Type::Bool)
            }
            BinaryOp::And | BinaryOp::Or => {
                ensure_bool_like(&left_type, "left operand", left.span)?;
                ensure_bool_like(&right_type, "right operand", right.span)?;
                Ok(Type::Bool)
            }
        }
    }
}

fn static_type_function_body_needs_type_value_short_circuit(body: &Expr) -> bool {
    matches!(&body.kind, ExprKind::Tuple(_))
        || matches!(&body.kind, ExprKind::Call { callee, .. } if expr_to_type_path(callee)
        .is_some_and(|name| is_static_type_function_body_type_value_former(&name)))
}

fn is_static_type_function_body_type_value_former(name: &str) -> bool {
    matches!(
        name,
        "type"
            | "tuple"
            | "weak_map"
            | "subtype"
            | "castable_subtype"
            | "concrete_subtype"
            | "castable_concrete_subtype"
            | "event"
            | "subscribable_event"
            | "subscribable_event_intrnl"
            | "sticky_event"
            | "task"
            | "generator"
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
