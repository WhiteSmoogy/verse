use super::*;

impl Checker {
    pub(super) fn check_statements(&mut self, statements: &[Stmt]) -> Result<Type, VerseError> {
        let mut last = Type::None;
        let mut unreachable_after: Option<(TerminationKind, Span)> = None;

        for statement in statements {
            if let Some((termination, span)) = unreachable_after {
                last = Type::Never;
                self.warn_unreachable(
                    termination.unreachable_message(),
                    span.through(statement.span),
                );
                break;
            }
            let statement_snapshot = if self.recovering {
                Some(self.clone())
            } else {
                None
            };
            last = match self.check_stmt(statement) {
                Ok(value_type) => value_type,
                Err(error) => {
                    let Some(snapshot) = statement_snapshot else {
                        return Err(error);
                    };
                    *self = snapshot;
                    self.record_error(error);
                    self.recover_failed_statement_binding(statement);
                    Type::Unknown
                }
            };
            if let Some(termination) = self
                .statement_termination(statement)
                .or_else(|| (last == Type::Never).then_some(TerminationKind::Never))
            {
                unreachable_after = Some((termination, statement.span));
            }
        }

        Ok(last)
    }

    pub(super) fn recover_failed_statement_binding(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let { name, .. } => self.define_recovered_binding(name, false),
            StmtKind::Var { name, .. } => self.define_recovered_binding(name, true),
            _ => {}
        }
    }

    pub(super) fn define_recovered_binding(&mut self, name: &str, mutable: bool) {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        current.entry(name.to_string()).or_insert(Symbol {
            value_type: Type::Unknown,
            mutable,
        });
    }

    pub(super) fn check_stmt(&mut self, statement: &Stmt) -> Result<Type, VerseError> {
        match &statement.kind {
            StmtKind::Using { path } => {
                if !self.current_definition_level() {
                    return Err(VerseError::check_at(
                        "`using` statements are only supported at module level",
                        statement.span,
                    ));
                }
                if is_absolute_module_path(path) {
                    if !is_supported_using_path(path) {
                        return Err(VerseError::check_at(
                            format!("unsupported module path `{path}`"),
                            statement.span,
                        ));
                    }
                    return Ok(Type::None);
                }

                let Some(module_name) = self.resolve_module_path(path) else {
                    return Err(VerseError::check_at(
                        format!("unsupported module path `{path}`"),
                        statement.span,
                    ));
                };
                self.ensure_module_import_accessible(&module_name, statement.span)?;
                self.add_current_import(module_name);
                Ok(Type::None)
            }
            StmtKind::Let {
                name,
                specifiers,
                annotation,
                expr,
            } => self.check_binding(
                name,
                specifiers,
                annotation.as_ref(),
                expr,
                false,
                statement.span,
            ),
            StmtKind::ParametricType {
                name,
                specifiers,
                params,
                expr,
            } => self.check_parametric_type_definition(
                name,
                specifiers,
                params,
                expr,
                statement.span,
            ),
            StmtKind::ParametricTypeAlias {
                name,
                specifiers,
                params,
                target,
            } => self.check_parametric_type_alias_definition(
                name,
                specifiers,
                params,
                target,
                statement.span,
            ),
            StmtKind::TypeAlias {
                name, specifiers, ..
            } => {
                if !self.current_definition_level() {
                    return Err(VerseError::check_at(
                        "type aliases are only supported at module level",
                        statement.span,
                    ));
                }
                self.validate_data_specifiers(name, specifiers, None, false, statement.span)?;
                access_level_from_specifiers(specifiers, "type alias", statement.span)?;
                ensure_private_protected_access_only_in_classes(specifiers, statement.span)?;
                Ok(Type::None)
            }
            StmtKind::ScopedAccessLevel { .. } => {
                if !self.current_definition_level() {
                    return Err(VerseError::check_at(
                        "scoped access level definitions are only supported at module level",
                        statement.span,
                    ));
                }
                Ok(Type::None)
            }
            StmtKind::ExtensionMethod(method) => self.check_extension_method(method),
            StmtKind::Var {
                name,
                annotation,
                expr,
            } => self.check_binding(name, &[], annotation.as_ref(), expr, true, statement.span),
            StmtKind::Set { target, op, expr } => {
                self.check_set_expression(target, *op, expr, false, statement.span)
            }
            StmtKind::Return(expr) => {
                if self.in_failure_context() {
                    return Err(VerseError::check_at(
                        "Explicit return out of a failure context is not allowed",
                        statement.span,
                    ));
                }
                let Some(expected) = self.function_returns.last().cloned() else {
                    return Err(VerseError::check_at(
                        "`return` used outside a function",
                        statement.span,
                    ));
                };

                let actual = self.check_expr(expr)?;
                self.ensure_expr_assignable(&expected, &actual, expr, || {
                    format!("cannot return `{actual}` from function returning `{expected}`")
                })?;
                Ok(Type::Never)
            }
            StmtKind::Break => {
                if self.in_failure_context() {
                    return Err(VerseError::check_at(
                        "`break` may not be used in a failure context",
                        statement.span,
                    ));
                }
                if self.break_depth == 0 {
                    Err(VerseError::check_at(
                        "`break` used outside a loop",
                        statement.span,
                    ))
                } else {
                    Ok(Type::Never)
                }
            }
            StmtKind::Defer(body) => {
                if defer_body_is_empty(body) {
                    return Err(VerseError::check_at(
                        "`defer` block cannot be empty",
                        statement.span,
                    ));
                }
                if let Some((message, span)) = defer_body_escape(body, 0) {
                    return Err(VerseError::check_at(message, span));
                }
                if let Some(span) = defer_body_failable_expr(body) {
                    return Err(VerseError::check_at(
                        "`defer` block cannot contain failable expressions",
                        span,
                    ));
                }
                self.defer_depth += 1;
                let result = self.check_expr(body);
                self.defer_depth -= 1;
                result?;
                Ok(Type::None)
            }
            StmtKind::Expr(expr) => self.check_expr(expr),
        }
    }

    pub(super) fn check_parametric_type_definition(
        &mut self,
        name: &str,
        specifiers: &[String],
        params: &[TypeParam],
        expr: &Expr,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !self.current_definition_level() {
            return Err(VerseError::check_at(
                "parametric type definitions are only supported at module level",
                span,
            ));
        }
        self.validate_data_specifiers(name, specifiers, None, false, span)?;
        access_level_from_specifiers(specifiers, "parametric type", span)?;
        ensure_private_protected_access_only_in_classes(specifiers, span)?;
        let Some(kind) = parametric_type_kind(expr) else {
            return Err(VerseError::check_at(
                "parametric type definitions must define a class, struct, or interface",
                span,
            ));
        };
        self.validate_type_parameter_names(params, span)?;
        self.validate_type_parameter_constraints(params, span)?;
        self.check_parametric_type_field_attributes(params, expr)?;
        let qualified = self.current_qualified_name(name);
        let value_type = Type::ParametricType {
            name: qualified,
            params: params.iter().map(|param| param.name.clone()).collect(),
            kind,
        };
        self.define(name, value_type.clone(), false, span)?;
        self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
        Ok(value_type)
    }

    pub(super) fn check_parametric_type_alias_definition(
        &mut self,
        name: &str,
        specifiers: &[String],
        params: &[TypeParam],
        target: &TypeAnnotation,
        span: Span,
    ) -> Result<Type, VerseError> {
        if !self.current_definition_level() {
            return Err(VerseError::check_at(
                "parametric type definitions are only supported at module level",
                span,
            ));
        }
        self.validate_data_specifiers(name, specifiers, None, false, span)?;
        access_level_from_specifiers(specifiers, "parametric type", span)?;
        ensure_private_protected_access_only_in_classes(specifiers, span)?;
        self.validate_type_parameter_names(params, span)?;
        self.validate_type_parameter_constraints(params, span)?;
        self.push_type_param_scope(params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = self.type_name_to_type(target);
        self.pop_type_param_scope();
        result?;
        let qualified = self.current_qualified_name(name);
        let value_type = Type::ParametricType {
            name: qualified,
            params: params.iter().map(|param| param.name.clone()).collect(),
            kind: ParametricTypeKind::Alias,
        };
        self.define(name, value_type.clone(), false, span)?;
        self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
        Ok(value_type)
    }

    pub(super) fn check_parametric_type_field_attributes(
        &mut self,
        params: &[TypeParam],
        expr: &Expr,
    ) -> Result<(), VerseError> {
        self.push_type_param_scope(params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = match &expr.kind {
            ExprKind::ClassDefinition { fields, .. }
            | ExprKind::InterfaceDefinition { fields, .. } => {
                self.check_class_field_attributes(fields)
            }
            _ => Ok(()),
        };
        self.pop_type_param_scope();
        result
    }

    pub(super) fn validate_type_parameter_names(
        &self,
        params: &[TypeParam],
        span: Span,
    ) -> Result<(), VerseError> {
        if params.is_empty() {
            return Err(VerseError::check_at(
                "parametric type definitions expect at least one type parameter",
                span,
            ));
        }
        let mut seen = Vec::new();
        for param in params {
            if seen.iter().any(|name| name == &param.name) {
                return Err(VerseError::check_at(
                    format!("duplicate type parameter `{}`", param.name),
                    param.span,
                ));
            }
            seen.push(param.name.clone());
        }
        Ok(())
    }

    pub(super) fn validate_type_parameter_constraints(
        &mut self,
        params: &[TypeParam],
        span: Span,
    ) -> Result<(), VerseError> {
        self.push_type_param_scope(params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            for param in params {
                match &param.constraint {
                    TypeParamConstraint::Subtype(parent) => {
                        self.type_name_to_type_name(parent, span)?;
                    }
                    TypeParamConstraint::TypeBounds { lower, upper } => {
                        self.type_name_to_type_name(lower, span)?;
                        self.type_name_to_type_name(upper, span)?;
                    }
                    TypeParamConstraint::Type => {}
                }
            }
            Ok(())
        })();
        self.pop_type_param_scope();
        result
    }

    pub(super) fn check_extension_method(
        &mut self,
        extension: &ExtensionMethod,
    ) -> Result<Type, VerseError> {
        if !self.current_definition_level() {
            return Err(VerseError::check_at(
                "extension methods are only supported at module level",
                extension.span,
            ));
        }

        let Some(body) = extension.method.body.as_ref() else {
            return Err(VerseError::check_at(
                "extension method requires a body",
                extension.span,
            ));
        };

        let receiver_type = self.extension_receiver_type(extension)?;
        self.ensure_extension_method_not_conflicting_with_member(
            &receiver_type,
            &extension.method,
            extension.span,
        )?;
        let mut params = Vec::with_capacity(extension.method.params.len() + 1);
        params.push(extension.receiver.clone());
        params.extend(extension.method.params.clone());
        let checked_type = self.check_function(
            &params,
            &extension.method.effects,
            extension.method.return_type.as_ref(),
            body,
        )?;

        let Type::Function { return_type, .. } = checked_type else {
            unreachable!("check_function should always return a function type");
        };
        let visible_type = self.extension_type_with_return(extension, *return_type)?;
        let access = access_level_from_specifiers(
            &extension.method.effects,
            "extension method",
            extension.span,
        )?;
        let dependee = self.current_qualified_name(&extension.method.name);
        self.ensure_type_dependencies_accessible(&dependee, access, &visible_type, extension.span)?;
        self.update_extension_method_type(
            &extension.method.name,
            &receiver_type,
            visible_type,
            extension.span,
        )?;

        Ok(Type::None)
    }

    pub(super) fn update_extension_method_type(
        &mut self,
        name: &str,
        receiver_type: &Type,
        method_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let module_name = self.current_module_name();
        let methods = self.extension_methods.entry(name.to_string()).or_default();
        if let Some(method) = methods.iter_mut().find(|method| {
            method.module_name == module_name && &method.receiver_type == receiver_type
        }) {
            method.method_type = method_type;
            return Ok(());
        }

        methods.push(ExtensionMethodInfo {
            receiver_type: receiver_type.clone(),
            method_type,
            module_name,
            access: AccessLevel::Internal,
            scopes: Vec::new(),
            span,
        });
        Ok(())
    }

    pub(super) fn update_local_extension_method_type(
        &mut self,
        name: &str,
        receiver_type: &Type,
        method_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(methods) = self.extension_methods.get_mut(name) else {
            return Err(VerseError::check_at(
                format!("unknown local extension method `{name}`"),
                span,
            ));
        };
        if let Some(method) = methods
            .iter_mut()
            .rev()
            .find(|method| method.module_name.is_none() && &method.receiver_type == receiver_type)
        {
            method.method_type = method_type;
            return Ok(());
        }

        Err(VerseError::check_at(
            format!("unknown local extension method `{name}` for receiver type `{receiver_type}`"),
            span,
        ))
    }

    pub(super) fn check_binding(
        &mut self,
        name: &str,
        specifiers: &[String],
        annotation: Option<&TypeAnnotation>,
        expr: &Expr,
        mutable: bool,
        span: Span,
    ) -> Result<Type, VerseError> {
        self.validate_data_specifiers(name, specifiers, annotation, mutable, span)?;
        if self.current_definition_level() {
            access_level_from_specifiers(
                module_member_specifiers(specifiers, expr),
                "module member",
                span,
            )?;
            ensure_private_protected_access_only_in_classes(
                module_member_specifiers(specifiers, expr),
                span,
            )?;
        }

        if let ExprKind::EnumDefinition {
            open,
            persistable,
            variants,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "enum definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "enum definitions are only supported at module level",
                    span,
                ));
            }
            validate_enum_variant_qualifiers(name, variants)?;
            let qualified = self.current_qualified_name(name);
            self.enum_types
                .entry(qualified.clone())
                .or_insert(EnumInfo {
                    variants: enum_variant_names(variants),
                    open: *open,
                    persistable: *persistable,
                });
            let value_type = Type::EnumType(qualified);
            self.define(name, value_type.clone(), false, span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::StructDefinition {
            fields,
            persistable,
            computes,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "struct definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "struct definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            if !self.struct_types.contains_key(&qualified) {
                let fields = self.struct_field_infos_with_owner(
                    fields,
                    Some(&qualified),
                    FieldOwnerKind::Struct,
                )?;
                if *persistable {
                    self.ensure_persistable_struct(&qualified, &fields)?;
                }
                self.struct_types.insert(
                    qualified.clone(),
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
                        native: field_has_specifier(specifiers, "native"),
                        persistable: *persistable,
                        computes: *computes,
                        constructor_effects: Vec::new(),
                        constructor_access: AccessLevel::Public,
                        constructor_scopes: Vec::new(),
                        fields,
                        methods: Vec::new(),
                    },
                );
            }
            let value_type = Type::StructType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::ClassDefinition {
            base,
            interfaces,
            specifiers: class_specifiers,
            fields,
            methods,
            extension_methods,
            blocks,
            ..
        } = &expr.kind
        {
            if mutable {
                return Err(VerseError::check_at(
                    "class definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "class definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            if !self.struct_types.contains_key(&qualified) {
                let (constructor_access, constructor_scopes) =
                    class_constructor_access_from_specifiers(class_specifiers, span)?;
                let definition_access = access_level_from_specifiers(
                    module_member_specifiers(specifiers, expr),
                    "module member",
                    span,
                )?;
                let (fields, methods, unique, castable, base, implemented_interfaces) = self
                    .class_member_infos(
                        &qualified,
                        ClassDefinitionParts {
                            definition_access,
                            specifiers: class_specifiers,
                            base: base.as_ref(),
                            interfaces,
                            fields,
                            methods,
                            extension_methods,
                            blocks,
                        },
                    )?;
                let constructor_effects =
                    self.class_constructor_effects_with_base(base.as_deref(), blocks);
                self.struct_types.insert(
                    qualified.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base,
                        interfaces: implemented_interfaces,
                        unique,
                        abstract_class: class_has_specifier(class_specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(class_specifiers, "epic_internal"),
                        final_class: class_has_specifier(class_specifiers, "final"),
                        concrete: class_has_specifier(class_specifiers, "concrete"),
                        castable,
                        native: field_has_specifier(specifiers, "native"),
                        persistable: class_has_specifier(class_specifiers, "persistable"),
                        computes: false,
                        constructor_effects,
                        constructor_access,
                        constructor_scopes,
                        fields,
                        methods,
                    },
                );
            }
            self.check_class_field_attributes(fields)?;
            let value_type = Type::ClassType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(
                name,
                value_type.clone(),
                module_member_specifiers(specifiers, expr),
                span,
            )?;
            return Ok(value_type);
        }

        if let ExprKind::InterfaceDefinition { fields, .. } = &expr.kind {
            if mutable {
                return Err(VerseError::check_at(
                    "interface definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "interface definitions are only supported at module level",
                    span,
                ));
            }
            self.check_class_field_attributes(fields)?;
            let qualified = self.current_qualified_name(name);
            let value_type = Type::InterfaceType(qualified.clone());
            self.define_aggregate_value(name, &qualified, value_type.clone(), span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            return Ok(value_type);
        }

        if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
            if mutable {
                return Err(VerseError::check_at(
                    "module definitions cannot be mutable",
                    span,
                ));
            }
            if !self.current_definition_level() {
                return Err(VerseError::check_at(
                    "module definitions are only supported at module level",
                    span,
                ));
            }
            let qualified = self.current_qualified_name(name);
            self.module_types
                .entry(qualified.clone())
                .or_insert(ModuleInfo {
                    members: HashMap::new(),
                    member_access: HashMap::new(),
                    member_scopes: HashMap::new(),
                    access: AccessLevel::Internal,
                    scopes: Vec::new(),
                    imports: Vec::new(),
                });
            let value_type = Type::Module(qualified.clone());
            self.define(name, value_type.clone(), false, span)?;
            self.record_current_module_member(name, value_type.clone(), specifiers, span)?;
            self.check_module_body(&qualified, statements)?;
            return Ok(value_type);
        }

        if let ExprKind::Function { effects, .. } = &expr.kind
            && !self.current_definition_level()
        {
            if let Some(access) = effects
                .iter()
                .find(|specifier| is_access_specifier_name(specifier))
            {
                return Err(VerseError::check_at(
                    format!("local definition `{name}` cannot use access specifier `<{access}>`"),
                    span,
                ));
            }

            if has_effect(effects, "final") {
                return Err(VerseError::check_at(
                    "`final` specifier is not allowed on local definitions",
                    span,
                ));
            }
        }

        if annotation.is_none() && matches!(&expr.kind, ExprKind::External) {
            return Err(VerseError::check_at(
                "`external {}` requires an explicit type annotation",
                expr.span,
            ));
        }

        let inferred = self.check_binding_expr(name, expr)?;
        let checked_type = if let Some(annotation) = annotation {
            let expected = self.type_name_to_type(annotation)?;
            self.ensure_expr_assignable(&expected, &inferred, expr, || {
                format!(
                    "binding `{name}` is annotated as `{expected}` but expression has type `{inferred}`"
                )
            })?;
            expected
        } else {
            inferred.clone()
        };
        let binding_checked_type =
            precise_type_value_binding_type(&checked_type, &inferred, mutable);

        if specifiers.iter().any(|specifier| specifier == "predicts") {
            return Err(VerseError::check_at(
                "`predicts` data specifier can only be used on class fields",
                span,
            ));
        }

        if self.scopes.len() == 1 && self.type_aliases.contains_key(name) {
            return Err(VerseError::check_at(
                format!("binding `{name}` conflicts with type alias `{name}`"),
                span,
            ));
        }

        let binding_type = if !mutable && matches!(&expr.kind, ExprKind::Function { .. }) {
            if self.current_definition_level() && self.is_current_predeclared_function(name) {
                self.update_current_function_binding(name, binding_checked_type.clone(), span)?
            } else {
                self.define_or_overload_function(name, binding_checked_type.clone(), span)?
            }
        } else {
            self.define(name, binding_checked_type.clone(), mutable, span)?;
            binding_checked_type.clone()
        };
        if self.current_definition_level() {
            let access = access_level_from_specifiers(
                module_member_specifiers(specifiers, expr),
                "module member",
                span,
            )?;
            let dependee = self.current_qualified_name(name);
            self.ensure_type_dependencies_accessible(&dependee, access, &binding_type, span)?;
        }
        self.record_current_module_member(
            name,
            binding_type.clone(),
            module_member_specifiers(specifiers, expr),
            span,
        )?;
        self.record_player_weak_map_binding(&binding_type, span)?;
        self.semantic_facts
            .record_binding_type(span, binding_type.clone());
        Ok(binding_type)
    }

    fn check_binding_expr(&mut self, name: &str, expr: &Expr) -> Result<Type, VerseError> {
        let ExprKind::Function {
            params,
            effects,
            return_type,
            body,
        } = &expr.kind
        else {
            return self.check_expr(expr);
        };
        let qualified = self.current_qualified_name(name);
        let Some(infos) = self.type_functions.get(&qualified).cloned() else {
            return self.check_expr(expr);
        };
        let Some((static_type_params, inferred_type_params)) = self.type_function_params(params)?
        else {
            return self.check_expr(expr);
        };
        let Some(return_type_annotation) = return_type.as_ref() else {
            return self.check_expr(expr);
        };
        let mut all_type_params = inferred_type_params.clone();
        all_type_params.extend(static_type_params.iter().cloned());
        self.push_type_param_scope(all_type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let return_value_type = self.annotation_to_type(Some(return_type_annotation));
        self.pop_type_param_scope();
        let return_value_type = return_value_type?;
        if !type_can_be_used_as_type_value(&return_value_type) {
            return self.check_expr(expr);
        };
        if self.expr_to_type_name(body).is_err() {
            return self.check_expr(expr);
        }
        if !infos.iter().any(|info| {
            info.params.len() == static_type_params.len()
                && info
                    .params
                    .iter()
                    .zip(&static_type_params)
                    .all(|(left, right)| {
                        left.name == right.name && left.constraint == right.constraint
                    })
                && info.inferred_params.len() == inferred_type_params.len()
                && info
                    .inferred_params
                    .iter()
                    .zip(&inferred_type_params)
                    .all(|(left, right)| {
                        left.name == right.name && left.constraint == right.constraint
                    })
        }) {
            return self.check_expr(expr);
        }
        self.without_enclosing_failure_context(|checker| {
            checker.check_function_with_static_type_params(
                params,
                effects,
                return_type.as_ref(),
                body,
                &static_type_params,
            )
        })
    }

    pub(super) fn record_player_weak_map_binding(
        &mut self,
        value_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        if !self.current_definition_level() {
            return Ok(());
        }

        let Type::WeakMap(key_type, weak_value_type) = value_type else {
            return Ok(());
        };
        if !matches!(key_type.as_ref(), Type::Class(name) if name == "player") {
            return Ok(());
        }

        self.player_weak_maps.push(PlayerWeakMapInfo {
            value_type: weak_value_type.as_ref().clone(),
        });

        if self.player_weak_maps.len() > 4 {
            return Err(VerseError::check_at(
                "module-scoped `weak_map(player, ...)` variables are limited to four per island",
                span,
            ));
        }

        if self.player_weak_maps.len() == 4
            && !self
                .player_weak_maps
                .iter()
                .any(|weak_map| self.player_weak_map_value_is_class(&weak_map.value_type))
        {
            return Err(VerseError::check_at(
                "when four module-scoped `weak_map(player, ...)` variables are defined, at least one value type must be a persistable class",
                span,
            ));
        }

        Ok(())
    }

    pub(super) fn player_weak_map_value_is_class(&self, value_type: &Type) -> bool {
        matches!(value_type, Type::Class(name)
            if self
                .struct_types
                .get(name)
                .is_some_and(|info| info.kind == AggregateKind::Class && info.persistable))
    }

    pub(super) fn validate_data_specifiers(
        &self,
        name: &str,
        specifiers: &[String],
        annotation: Option<&TypeAnnotation>,
        mutable: bool,
        span: Span,
    ) -> Result<(), VerseError> {
        if specifiers.is_empty() {
            return Ok(());
        }

        if mutable {
            return Err(VerseError::check_at(
                format!("data specifiers are not supported on mutable binding `{name}`"),
                span,
            ));
        }

        if !self.current_definition_level()
            && let Some(access) = specifiers
                .iter()
                .find(|specifier| is_access_specifier_name(specifier))
        {
            return Err(VerseError::check_at(
                format!("local definition `{name}` cannot use access specifier `<{access}>`"),
                span,
            ));
        }

        if specifiers.iter().any(|specifier| specifier == "localizes")
            && !matches!(
                annotation.map(|annotation| &annotation.name),
                Some(TypeName::Message)
            )
        {
            return Err(VerseError::check_at(
                "`localizes` data specifier requires a `message` annotation",
                span,
            ));
        }

        Ok(())
    }

    pub(super) fn record_current_module_member(
        &mut self,
        name: &str,
        value_type: Type,
        specifiers: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        if !self.current_definition_level() {
            return Ok(());
        }
        let access = access_level_from_specifiers(specifiers, "module member", span)?;
        let scopes = scoped_access_scopes(specifiers).unwrap_or_default();
        if let Type::Module(qualified) = &value_type {
            self.update_module_definition_access(qualified, access, &scopes);
        }

        let Some(module_name) = self.current_module_name() else {
            return Ok(());
        };
        if let Some(info) = self.module_types.get_mut(&module_name) {
            info.members.insert(name.to_string(), value_type);
            info.member_access.insert(name.to_string(), access);
            if !scopes.is_empty() {
                info.member_scopes.insert(name.to_string(), scopes);
            } else {
                info.member_scopes.remove(name);
            }
        }
        Ok(())
    }

    pub(super) fn record_module_definition_access(
        &mut self,
        name: &str,
        specifiers: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        let access = access_level_from_specifiers(specifiers, "module member", span)?;
        let scopes = scoped_access_scopes(specifiers).unwrap_or_default();
        let qualified = self.current_qualified_name(name);
        self.update_module_definition_access(&qualified, access, &scopes);

        let Some(module_name) = self.current_module_name() else {
            return Ok(());
        };
        if let Some(info) = self.module_types.get_mut(&module_name) {
            info.member_access.insert(name.to_string(), access);
            if !scopes.is_empty() {
                info.member_scopes.insert(name.to_string(), scopes);
            } else {
                info.member_scopes.remove(name);
            }
        }
        Ok(())
    }

    pub(super) fn record_current_module_member_access(
        &mut self,
        name: &str,
        specifiers: &[String],
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(module_name) = self.current_module_name() else {
            return Ok(());
        };
        let access = access_level_from_specifiers(specifiers, "module member", span)?;
        if let Some(info) = self.module_types.get_mut(&module_name) {
            info.member_access.insert(name.to_string(), access);
            if let Some(scopes) = scoped_access_scopes(specifiers) {
                info.member_scopes.insert(name.to_string(), scopes);
            } else {
                info.member_scopes.remove(name);
            }
        }
        Ok(())
    }

    fn update_module_definition_access(
        &mut self,
        qualified: &str,
        access: AccessLevel,
        scopes: &[String],
    ) {
        if let Some(info) = self.module_types.get_mut(qualified) {
            info.access = access;
            info.scopes = scopes.to_vec();
        }
    }

    pub(super) fn ensure_predicts_specifier_type(
        &self,
        context: &str,
        specifiers: &[String],
        value_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        if !specifiers.iter().any(|specifier| specifier == "predicts") {
            return Ok(());
        }

        if self.is_predicts_var_data_type(value_type) {
            Ok(())
        } else {
            Err(VerseError::check_at(
                format!(
                    "`predicts` {context} specifier requires a prediction-compatible type, got `{value_type}`"
                ),
                span,
            ))
        }
    }

    pub(super) fn check_module_body(
        &mut self,
        module_name: &str,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        let previous_module_path = std::mem::replace(
            &mut self.module_path,
            module_name.split('.').map(str::to_string).collect(),
        );
        self.push_scope();
        self.module_scope_depths.push(self.scopes.len());
        let result = (|| {
            self.predeclare_using_imports(statements)?;
            self.predeclare_aggregate_values_in_current_scope(statements)?;
            self.predeclare_extension_methods_in_current_scope(statements)?;
            self.predeclare_functions_in_current_scope(statements)?;
            self.validate_public_module_surface_access(statements)?;
            self.check_statements(statements)?;
            Ok(())
        })();
        self.module_scope_depths.pop();
        self.pop_scope();
        self.module_path = previous_module_path;
        result
    }
}

fn precise_type_value_binding_type(expected: &Type, inferred: &Type, mutable: bool) -> Type {
    if mutable || type_value_instance_type(inferred).is_none() {
        return expected.clone();
    }

    match expected {
        Type::TypeValue | Type::TypeValueOf(_) | Type::TypeValueBounds { .. } => inferred.clone(),
        _ => expected.clone(),
    }
}

fn is_access_specifier_name(specifier: &str) -> bool {
    access_specifier_name(specifier).is_some()
}
