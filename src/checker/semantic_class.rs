use crate::token::Span;

use super::effects::has_explicit_call_effect_specifier;
use super::*;

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
    pub(super) native: bool,
    pub(super) persistable: bool,
    pub(super) computes: bool,
    pub(super) constructor_effects: Vec<String>,
    pub(super) constructor_access: AccessLevel,
    pub(super) constructor_scopes: Vec<String>,
    pub(super) fields: Vec<StructFieldInfo>,
    pub(super) methods: Vec<ClassMethodInfo>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AggregateKind {
    Struct,
    Class,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldOwnerKind {
    Struct,
    Class,
    Interface,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeTypeContext {
    Function,
    TypeMember,
    NativeMember,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AccessLevel {
    Public,
    Internal,
    Protected,
    Private,
    Scoped,
}

#[derive(Clone)]
pub(super) struct StructFieldInfo {
    pub(super) name: String,
    pub(super) value_type: Type,
    pub(super) has_default: bool,
    pub(super) mutable: bool,
    pub(super) final_member: bool,
    pub(super) access: AccessLevel,
    pub(super) scopes: Vec<String>,
    pub(super) mutation_access: AccessLevel,
    pub(super) mutation_scopes: Vec<String>,
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
    pub(super) scopes: Vec<String>,
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
    pub(super) scopes: Vec<String>,
    pub(super) span: Span,
}

impl Checker {
    pub(super) fn define_top_level_interface_members(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.define_interface_members(&program.statements)
    }

    pub(super) fn define_interface_members(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let {
                name,
                specifiers: binding_specifiers,
                expr,
                ..
            } = &statement.kind
            else {
                continue;
            };
            if let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.define_interface_members(module_statements)?;
                self.module_path.pop();
                continue;
            }

            let ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } = &expr.kind
            else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            let parent_names = self.interface_parent_names(parents)?;
            let inherited_fields = self.interface_field_requirements(&parent_names)?;
            let local_fields = self.struct_field_infos_with_owner(
                fields,
                Some(&qualified),
                FieldOwnerKind::Interface,
            )?;
            let fields =
                self.merge_interface_field_set(inherited_fields, local_fields, statement.span)?;
            let inherited_methods = self.interface_method_requirements(&parent_names)?;
            let local_methods = self.interface_local_method_infos(&qualified, methods)?;
            let method_infos =
                self.merge_interface_method_set(inherited_methods, local_methods, statement.span)?;
            let definition_access =
                access_level_from_specifiers(binding_specifiers, "module member", statement.span)?;
            self.interface_types.insert(
                qualified.clone(),
                InterfaceInfo {
                    parents: parent_names,
                    fields: fields.clone(),
                    methods: method_infos,
                },
            );
            self.check_interface_method_bodies(&qualified, definition_access, &fields, methods)?;
        }

        Ok(())
    }

    pub(super) fn interface_parent_names(
        &mut self,
        parents: &[TypeAnnotation],
    ) -> Result<Vec<String>, VerseError> {
        let mut names = Vec::with_capacity(parents.len());
        for parent in parents {
            let parent_type = self.type_name_to_type(parent)?;
            let Type::Interface(parent_name) = parent_type else {
                return Err(VerseError::check_at(
                    format!("interface parent must be an interface, got `{parent_type}`"),
                    parent.span,
                ));
            };
            names.push(parent_name);
        }
        Ok(dedupe_strings(names))
    }

    pub(super) fn interface_field_requirements(
        &self,
        interfaces: &[String],
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        let mut fields = Vec::new();
        for interface in interfaces {
            let Some(info) = self.interface_types.get(interface) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface}`"),
                    Span::new(0, 0, 1, 1),
                ));
            };
            fields =
                self.merge_interface_field_set(fields, info.fields.clone(), Span::new(0, 0, 1, 1))?;
        }
        Ok(fields)
    }

    pub(super) fn interface_method_requirements(
        &self,
        interfaces: &[String],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut methods = Vec::new();
        for interface in interfaces {
            let Some(info) = self.interface_types.get(interface) else {
                return Err(VerseError::check_at(
                    format!("unknown interface `{interface}`"),
                    Span::new(0, 0, 1, 1),
                ));
            };
            methods = self.merge_interface_method_set(
                methods,
                info.methods.clone(),
                Span::new(0, 0, 1, 1),
            )?;
        }
        Ok(methods)
    }

    pub(super) fn interface_local_method_infos(
        &mut self,
        interface_name: &str,
        methods: &[ClassMethod],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        for method in methods {
            if has_effect(&method.effects, "native") {
                return Err(VerseError::check_at(
                    "interface functions cannot be marked as `<native>`",
                    method.span,
                ));
            }
            let info = ClassMethodInfo {
                qualifier: method
                    .qualifier
                    .clone()
                    .or_else(|| Some(interface_name.to_string())),
                name: method.name.clone(),
                value_type: self.class_method_declared_type(
                    method,
                    class_or_interface_default_method_effects(&method.effects),
                )?,
                final_member: false,
                abstract_member: method.body.is_none(),
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                scopes: scoped_access_scopes(&method.effects).unwrap_or_default(),
                owner: Some(interface_name.to_string()),
                span: method.span,
            };
            push_distinct_local_method_info(&mut infos, info, "interface", &self.struct_types)?;
        }
        Ok(infos)
    }

    pub(super) fn merge_interface_field_set(
        &self,
        mut base: Vec<StructFieldInfo>,
        fields: Vec<StructFieldInfo>,
        span: Span,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        for field in fields {
            if let Some(index) = base.iter().position(|existing| existing.name == field.name) {
                let existing = &base[index];
                if existing.final_member && existing.owner != field.owner {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, existing.name
                        ),
                        field.span,
                    ));
                }
                if existing.value_type != field.value_type || existing.mutable != field.mutable {
                    return Err(VerseError::check_at(
                        format!(
                            "interface field `{}` has incompatible inherited definitions",
                            field.name
                        ),
                        span,
                    ));
                }
                if !existing.has_default && field.has_default {
                    base[index] = field;
                }
                continue;
            }
            base.push(field);
        }
        Ok(base)
    }

    pub(super) fn merge_interface_method_set(
        &self,
        mut base: Vec<ClassMethodInfo>,
        methods: Vec<ClassMethodInfo>,
        span: Span,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        for method in methods {
            if let Some(existing_index) = base.iter().position(|existing| {
                existing.name == method.name
                    && method_qualifiers_conflict(existing, &method)
                    && function_signatures_conflict(
                        &existing.value_type,
                        &method.value_type,
                        &self.struct_types,
                    )
            }) {
                let existing = &base[existing_index];
                if existing.value_type != method.value_type {
                    return Err(VerseError::check_at(
                        format!(
                            "interface method `{}` has incompatible inherited signatures",
                            method.name
                        ),
                        span,
                    ));
                }
                if existing.abstract_member && !method.abstract_member {
                    base[existing_index] = method;
                }
                continue;
            }
            base.push(method);
        }
        Ok(base)
    }

    pub(super) fn check_interface_method_bodies(
        &mut self,
        interface_name: &str,
        definition_access: AccessLevel,
        fields: &[StructFieldInfo],
        methods: &[ClassMethod],
    ) -> Result<(), VerseError> {
        let method_bindings = self
            .interface_types
            .get(interface_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for method in methods {
            let Some(body) = method.body.as_ref() else {
                continue;
            };

            self.push_scope();
            let method_type = (|| {
                self.define(
                    "Self",
                    Type::Interface(interface_name.to_string()),
                    false,
                    method.span,
                )?;
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        method.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, method.span)?;
                }
                let effects = class_or_interface_default_method_effects(&method.effects);
                self.check_function(&method.params, &effects, method.return_type.as_ref(), body)
            })();
            self.pop_scope();
            let method_type = method_type?;
            let access = access_level_from_specifiers(&method.effects, "method", method.span)?;
            self.ensure_aggregate_member_surface_dependencies_accessible(
                interface_name,
                definition_access,
                &method.name,
                access,
                &method_type,
                method.span,
            )?;
        }

        Ok(())
    }

    fn ensure_aggregate_member_surface_dependencies_accessible(
        &self,
        aggregate_name: &str,
        aggregate_access: AccessLevel,
        member_name: &str,
        member_access: AccessLevel,
        member_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        if !access_requires_dependency_validation(aggregate_access)
            || !access_requires_dependency_validation(member_access)
        {
            return Ok(());
        }
        let dependee = format!("{aggregate_name}.{member_name}");
        self.ensure_type_dependencies_accessible(&dependee, member_access, member_type, span)
    }

    fn class_or_interface_method_effects(
        &mut self,
        method: &ClassMethod,
        inherited_methods: &[ClassMethodInfo],
    ) -> Result<Vec<String>, VerseError> {
        let inherited_effects = if has_effect(&method.effects, "override")
            && !has_effect(&method.effects, "decides")
            && !has_explicit_call_effect_specifier(&method.effects)
        {
            let probe = ClassMethodInfo {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                value_type: self.class_method_declared_type(method, method.effects.clone())?,
                final_member: false,
                abstract_member: self.class_method_is_abstract(method),
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                scopes: scoped_access_scopes(&method.effects).unwrap_or_default(),
                owner: None,
                span: method.span,
            };
            inherited_method_override_index(inherited_methods, &probe, &self.struct_types)?
                .map(|index| inherited_methods[index].value_type.clone())
        } else {
            None
        };

        Ok(class_or_interface_method_effects_with_inherited(
            &method.effects,
            inherited_effects.as_ref(),
        ))
    }

    pub(super) fn define_top_level_aggregate_members(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.define_aggregate_members(&program.statements)
    }

    pub(super) fn define_aggregate_members(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let {
                name,
                specifiers: binding_specifiers,
                expr,
                ..
            } = &statement.kind
            else {
                continue;
            };
            if let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.define_aggregate_members(module_statements)?;
                self.module_path.pop();
                continue;
            }
            let (
                fields,
                methods,
                kind,
                base,
                interfaces,
                unique,
                abstract_class,
                epic_internal_class,
                final_class,
                concrete,
                castable,
                persistable,
                computes,
                constructor_effects,
                constructor_access,
                constructor_scopes,
            ) = match &expr.kind {
                ExprKind::StructDefinition {
                    fields,
                    persistable,
                    computes,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    (
                        self.struct_field_infos_with_owner(
                            fields,
                            Some(&qualified),
                            FieldOwnerKind::Struct,
                        )?,
                        Vec::new(),
                        AggregateKind::Struct,
                        None,
                        Vec::new(),
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        *persistable,
                        *computes,
                        Vec::new(),
                        AccessLevel::Public,
                        Vec::new(),
                    )
                }
                ExprKind::ClassDefinition {
                    base,
                    interfaces,
                    specifiers,
                    fields,
                    methods,
                    extension_methods,
                    blocks,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    let (constructor_access, constructor_scopes) =
                        class_constructor_access_from_specifiers(
                            specifiers,
                            class_definition_diagnostic_span(
                                base.as_ref(),
                                fields,
                                methods,
                                extension_methods,
                                blocks,
                            ),
                        )?;
                    let definition_access = access_level_from_specifiers(
                        module_member_specifiers(binding_specifiers, expr),
                        "module member",
                        statement.span,
                    )?;
                    let (fields, methods, unique, castable, base, implemented_interfaces) = self
                        .class_member_infos(
                            &qualified,
                            ClassDefinitionParts {
                                definition_access,
                                specifiers,
                                base: base.as_ref(),
                                interfaces,
                                fields,
                                methods,
                                extension_methods,
                                blocks,
                            },
                        )?;
                    (
                        fields,
                        methods,
                        AggregateKind::Class,
                        base,
                        implemented_interfaces,
                        unique,
                        class_has_specifier(specifiers, "abstract"),
                        class_has_specifier(specifiers, "epic_internal"),
                        class_has_specifier(specifiers, "final"),
                        class_has_specifier(specifiers, "concrete"),
                        castable,
                        class_has_specifier(specifiers, "persistable"),
                        false,
                        class_constructor_effects(blocks),
                        constructor_access,
                        constructor_scopes,
                    )
                }
                _ => continue,
            };
            let qualified = self.current_qualified_name(name);
            if kind == AggregateKind::Struct && persistable {
                self.ensure_persistable_struct(&qualified, &fields)?;
            }
            self.struct_types.insert(
                qualified,
                StructInfo {
                    kind,
                    base,
                    interfaces,
                    unique,
                    abstract_class,
                    epic_internal_class,
                    final_class,
                    concrete,
                    castable,
                    native: field_has_specifier(binding_specifiers, "native"),
                    persistable,
                    computes,
                    constructor_effects,
                    constructor_access,
                    constructor_scopes,
                    fields,
                    methods,
                },
            );
        }

        Ok(())
    }

    pub(super) fn struct_field_infos_with_owner(
        &mut self,
        fields: &[StructField],
        owner: Option<&str>,
        owner_kind: FieldOwnerKind,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        fields
            .iter()
            .map(|field| {
                let Some(annotation) = field.annotation.as_ref() else {
                    return Err(VerseError::check_at(
                        "expected explicit type annotation after struct field name",
                        field.span,
                    ));
                };
                let value_type = self.type_name_to_type(annotation)?;
                let owner_native = owner.is_some_and(|owner| self.aggregate_is_native(owner));
                if owner_native {
                    self.ensure_native_context_type(
                        &value_type,
                        NativeTypeContext::TypeMember,
                        field.span,
                    )?;
                }
                if field_has_specifier(&field.specifiers, "native") {
                    if !owner_native {
                        return Err(VerseError::check_at(
                            "`native` field specifier requires a native enclosing type",
                            field.span,
                        ));
                    }
                    self.ensure_native_context_type(
                        &value_type,
                        NativeTypeContext::NativeMember,
                        field.span,
                    )?;
                }
                if field_has_specifier(&field.specifiers, "localizes")
                    && !matches!(annotation.name, TypeName::Message)
                {
                    return Err(VerseError::check_at(
                        "`localizes` field specifier requires a `message` annotation",
                        field.span,
                    ));
                }
                if field_has_attribute(&field.attributes, "predicts_extern")
                    && !field_has_specifier(&field.specifiers, "predicts")
                {
                    return Err(VerseError::check_at(
                        "@predicts_extern requires <predicts> on the same data member",
                        field.span,
                    ));
                }
                if field_has_specifier(&field.specifiers, "predicts") {
                    if owner_kind != FieldOwnerKind::Class {
                        return Err(VerseError::check_at(
                            "`predicts` field specifier can only be used on class fields",
                            field.span,
                        ));
                    }
                    if field_has_specifier(&field.specifiers, "override") {
                        return Err(VerseError::check_at(
                            "<override> cannot be used with <predicts> yet",
                            field.span,
                        ));
                    }
                }
                self.ensure_predicts_specifier_type(
                    "field",
                    &field.specifiers,
                    &value_type,
                    field.span,
                )?;
                if let Some(default) = &field.default {
                    let default_type =
                        self.check_data_member_default(owner, &field.name, default)?;
                    self.ensure_expr_assignable(&value_type, &default_type, default, || {
                        format!(
                            "default value for field `{}` must be `{value_type}`, got `{default_type}`",
                            field.name
                        )
                    })?;
                }
                if field_has_specifier(&field.specifiers, "final") && field.default.is_none() {
                    return Err(VerseError::check_at(
                        format!("final field `{}` must have a default value", field.name),
                        field.span,
                    ));
                }
                let access = access_level_from_specifiers(&field.specifiers, "field", field.span)?;
                let scopes = scoped_access_scopes(&field.specifiers).unwrap_or_default();
                let (mutation_access, mutation_scopes) = if field.mutable {
                    if !field.var_specifiers.is_empty() {
                        (
                            access_level_from_specifiers(
                                &field.var_specifiers,
                                "var field",
                                field.span,
                            )?,
                            scoped_access_scopes(&field.var_specifiers).unwrap_or_default(),
                        )
                    } else {
                        (AccessLevel::Public, Vec::new())
                    }
                } else {
                    (access, scopes.clone())
                };
                Ok(StructFieldInfo {
                    name: field.name.clone(),
                    value_type,
                    has_default: field.default.is_some(),
                    mutable: field.mutable,
                    final_member: field_has_specifier(&field.specifiers, "final"),
                    access,
                    scopes,
                    mutation_access,
                    mutation_scopes,
                    owner: owner.map(str::to_string),
                    span: field.span,
                })
            })
            .collect()
    }

    pub(super) fn aggregate_is_native(&self, name: &str) -> bool {
        self.struct_types.get(name).is_some_and(|info| info.native)
    }

    pub(super) fn ensure_native_context_type(
        &self,
        value_type: &Type,
        context: NativeTypeContext,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some((kind, name)) = self.non_native_aggregate_in_type(value_type, context) {
            let message = match context {
                NativeTypeContext::Function => {
                    format!(
                        "`{kind} {name}` used as a parameter/result in a native function must also be native"
                    )
                }
                NativeTypeContext::TypeMember => {
                    format!(
                        "`{kind} {name}` contained as a member in a native type must also be native"
                    )
                }
                NativeTypeContext::NativeMember => {
                    format!(
                        "`{kind} {name}` contained in a native member must have a native representation"
                    )
                }
            };
            return Err(VerseError::check_at(message, span));
        }
        Ok(())
    }

    pub(super) fn ensure_native_function_signature(
        &self,
        function_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let Type::Function {
            param_types,
            return_type,
            ..
        } = function_type
        else {
            return Ok(());
        };
        if let Some(param_types) = param_types {
            for param_type in param_types {
                self.ensure_native_context_type(param_type, NativeTypeContext::Function, span)?;
            }
        }
        self.ensure_native_context_type(return_type, NativeTypeContext::Function, span)
    }

    fn non_native_aggregate_in_type(
        &self,
        value_type: &Type,
        context: NativeTypeContext,
    ) -> Option<(&'static str, String)> {
        match value_type {
            Type::Struct(name) | Type::StructType(name) => self
                .struct_types
                .get(name)
                .filter(|info| info.kind == AggregateKind::Struct && !info.native)
                .map(|_| ("struct", name.clone())),
            Type::Class(name) | Type::ClassType(name) => {
                if context == NativeTypeContext::TypeMember {
                    None
                } else {
                    self.struct_types
                        .get(name)
                        .filter(|info| info.kind == AggregateKind::Class && !info.native)
                        .map(|_| ("class", name.clone()))
                }
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Generator(Some(item))
            | Type::Task(item)
            | Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Awaitable(Some(item))
            | Type::Signalable(item)
            | Type::Subscribable(Some(item))
            | Type::Listenable(Some(item)) => self.non_native_aggregate_in_type(item, context),
            Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => self
                .non_native_aggregate_in_type(key, context)
                .or_else(|| self.non_native_aggregate_in_type(value, context)),
            Type::Tuple(items) => items
                .iter()
                .find_map(|item| self.non_native_aggregate_in_type(item, context)),
            Type::Function { .. }
            | Type::Generator(None)
            | Type::Awaitable(None)
            | Type::Subscribable(None)
            | Type::Listenable(None)
            | Type::Event(_)
            | Type::Int
            | Type::IntRange(_)
            | Type::Float
            | Type::Rational
            | Type::Number
            | Type::Bool
            | Type::String
            | Type::Message
            | Type::Char
            | Type::Char8
            | Type::Char32
            | Type::None
            | Type::Any
            | Type::Comparable
            | Type::TypeValue
            | Type::Unknown
            | Type::Never
            | Type::Range
            | Type::Enum(_)
            | Type::EnumType(_)
            | Type::Interface(_)
            | Type::InterfaceType(_)
            | Type::Module(_)
            | Type::Param(_, _)
            | Type::ParametricType { .. }
            | Type::Overload(_) => None,
        }
    }

    pub(super) fn check_data_member_default(
        &mut self,
        owner: Option<&str>,
        field_name: &str,
        default: &Expr,
    ) -> Result<Type, VerseError> {
        self.data_member_default_depth += 1;
        if let Some(owner) = owner {
            self.data_member_default_stack
                .push(DataMemberDefaultContext {
                    aggregate_name: owner.to_string(),
                    field_name: field_name.to_string(),
                });
        }
        self.function_effects.push(vec!["converges".to_string()]);
        let result = if let Some(owner) = owner {
            self.push_scope();
            let result = self
                .define_current_aggregate_type_if_unshadowed(owner, default.span)
                .and_then(|_| self.check_expr(default));
            self.pop_scope();
            result
        } else {
            self.check_expr(default)
        };
        self.function_effects.pop();
        if owner.is_some() {
            self.data_member_default_stack.pop();
        }
        self.data_member_default_depth -= 1;
        result
    }

    pub(super) fn define_current_aggregate_type_if_unshadowed(
        &mut self,
        aggregate_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if aggregate_name.contains('(') {
            return Ok(());
        }
        let name = aggregate_unqualified_name(aggregate_name);
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            return Ok(());
        }
        if let Some(info) = self.struct_types.get(aggregate_name) {
            let value_type = match info.kind {
                AggregateKind::Struct => Type::StructType(aggregate_name.to_string()),
                AggregateKind::Class => Type::ClassType(aggregate_name.to_string()),
            };
            return self.define(name, value_type, false, span);
        }
        if self.interface_types.contains_key(aggregate_name) {
            return self.define(
                name,
                Type::InterfaceType(aggregate_name.to_string()),
                false,
                span,
            );
        }
        Ok(())
    }

    pub(super) fn check_class_field_attributes(
        &mut self,
        fields: &[StructField],
    ) -> Result<(), VerseError> {
        for field in fields {
            if field
                .attributes
                .iter()
                .any(|attribute| attribute.name == "editable")
                && let Some(annotation) = field.annotation.as_ref()
            {
                let value_type = self.type_name_to_type(annotation)?;
                if type_contains_type_param(&value_type) {
                    return Err(VerseError::check_at(
                        format!(
                            "`@editable` field `{}` cannot use a type parameter in its annotation",
                            field.name
                        ),
                        field.span,
                    ));
                }
            }
            for attribute in &field.attributes {
                for argument in &attribute.arguments {
                    self.check_expr(&argument.expr)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn class_member_infos(
        &mut self,
        class_name: &str,
        parts: ClassDefinitionParts<'_>,
    ) -> Result<ClassMemberInfosResult, VerseError> {
        let ClassDefinitionParts {
            definition_access,
            specifiers,
            base,
            interfaces,
            fields,
            methods,
            extension_methods,
            blocks,
        } = parts;

        if class_has_specifier(specifiers, "abstract")
            && class_has_specifier(specifiers, "concrete")
        {
            return Err(VerseError::check_at(
                format!("class `{class_name}` cannot be both `abstract` and `concrete`"),
                fields
                    .first()
                    .map_or_else(|| Span::new(0, 0, 1, 1), |field| field.span),
            ));
        }

        let mut implemented_interfaces = Vec::new();
        let (mut inherited_fields, mut inherited_methods, base_name, base_unique, base_castable) =
            if let Some(base) = base {
                match self.type_name_to_type(base)? {
                    Type::Class(base_name) => {
                        if is_builtin_class_base_name(&base_name) {
                            (Vec::new(), Vec::new(), Some(base_name), false, false)
                        } else {
                            let Some(info) = self.struct_types.get(&base_name).cloned() else {
                                return Err(VerseError::check_at(
                                    format!("unknown class `{base_name}`"),
                                    base.span,
                                ));
                            };
                            if info.kind != AggregateKind::Class {
                                return Err(VerseError::check_at(
                                    format!("class base must be a class, got `{base_name}`"),
                                    base.span,
                                ));
                            }
                            if info.final_class {
                                return Err(VerseError::check_at(
                                    format!(
                                        "class `{base_name}` is `final` and cannot be inherited"
                                    ),
                                    base.span,
                                ));
                            }
                            if self.aggregate_is_native(class_name) && !info.native {
                                return Err(VerseError::check_at(
                                    format!(
                                        "native class `{class_name}` cannot inherit from non-native class `{base_name}`"
                                    ),
                                    base.span,
                                ));
                            }
                            self.ensure_base_class_constructor_accessible(
                                &base_name,
                                info.constructor_access,
                                &info.constructor_scopes,
                                base.span,
                            )?;
                            (
                                info.fields,
                                info.methods,
                                Some(base_name),
                                info.unique,
                                info.castable,
                            )
                        }
                    }
                    Type::Interface(interface_name) => {
                        implemented_interfaces.push(interface_name);
                        (Vec::new(), Vec::new(), None, false, false)
                    }
                    Type::Modifier(item_type) => (
                        Vec::new(),
                        vec![modifier_method_info(item_type.as_ref(), base.span)],
                        None,
                        false,
                        false,
                    ),
                    other => {
                        return Err(VerseError::check_at(
                            format!("class parent must be a class or interface, got `{other}`"),
                            base.span,
                        ));
                    }
                }
            } else {
                (Vec::new(), Vec::new(), None, false, false)
            };
        let class_span =
            class_definition_diagnostic_span(base, fields, methods, extension_methods, blocks);
        let has_final_super = class_has_specifier(specifiers, "final_super");
        if has_final_super && base_name.as_deref() != Some("component") {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` with `<final_super>` must directly inherit from `component`"
                ),
                class_span,
            ));
        }
        if base_name.as_deref() == Some("component") && !has_final_super {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` directly inheriting from `component` must specify `<final_super>`"
                ),
                class_span,
            ));
        }
        for interface in interfaces {
            let interface_type = self.type_name_to_type(interface)?;
            let Type::Interface(interface_name) = interface_type else {
                if let Type::Modifier(item_type) = interface_type {
                    inherited_methods
                        .push(modifier_method_info(item_type.as_ref(), interface.span));
                    continue;
                }
                return Err(VerseError::check_at(
                    format!("additional class parent must be an interface, got `{interface_type}`"),
                    interface.span,
                ));
            };
            implemented_interfaces.push(interface_name);
        }
        if let Some(base_name) = &base_name
            && let Some(base_info) = self.struct_types.get(base_name)
        {
            implemented_interfaces.extend(base_info.interfaces.clone());
        }
        implemented_interfaces = dedupe_strings(implemented_interfaces);
        let interface_fields = self.interface_field_requirements(&implemented_interfaces)?;
        inherited_fields =
            self.merge_interface_fields(inherited_fields, interface_fields, class_name, base)?;
        let interface_methods = self.interface_method_requirements(&implemented_interfaces)?;
        inherited_methods =
            self.merge_interface_methods(inherited_methods, interface_methods, class_name, base)?;
        let unique = class_has_specifier(specifiers, "unique") || base_unique;
        let castable = class_has_specifier(specifiers, "castable") || base_castable;

        let local =
            self.struct_field_infos_with_owner(fields, Some(class_name), FieldOwnerKind::Class)?;
        for mut field in local {
            let source_field = fields
                .iter()
                .find(|candidate| candidate.name == field.name)
                .expect("field info should come from class field");
            let override_field = field_has_specifier(&source_field.specifiers, "override");
            if let Some(index) = inherited_fields
                .iter()
                .position(|candidate| candidate.name == field.name)
            {
                if !override_field {
                    return Err(VerseError::check_at(
                        format!("duplicate inherited class field `{}`", field.name),
                        source_field.span,
                    ));
                }
                let inherited = &inherited_fields[index];
                if inherited.final_member {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides final inherited field `{}`",
                            field.name, inherited.name
                        ),
                        source_field.span,
                    ));
                }
                let inherited_owner_is_interface = inherited
                    .owner
                    .as_deref()
                    .is_some_and(|owner| self.interface_types.contains_key(owner));
                if !inherited_owner_is_interface {
                    if !has_access_specifier(&source_field.specifiers) {
                        field.access = inherited.access;
                        field.scopes = inherited.scopes.clone();
                    }
                    if field.mutable && !has_access_specifier(&source_field.var_specifiers) {
                        field.mutation_access = inherited.mutation_access;
                        field.mutation_scopes = inherited.mutation_scopes.clone();
                    }
                }
                if field.access != inherited.access
                    || (field.access == AccessLevel::Scoped && field.scopes != inherited.scopes)
                {
                    return Err(VerseError::check_at(
                        "An overridden field cannot change the inherited access level",
                        source_field.span,
                    ));
                }
                if !inherited_owner_is_interface {
                    field.owner = inherited.owner.clone();
                }
                if !self.is_field_override_assignable(
                    &inherited.value_type,
                    &field.value_type,
                    class_name,
                    base_name.as_deref(),
                ) {
                    return Err(VerseError::check_at(
                        format!(
                            "field `{}` overrides `{}` but has incompatible type `{}`",
                            field.name, inherited.value_type, field.value_type
                        ),
                        source_field.span,
                    ));
                }
                inherited_fields[index] = field;
                continue;
            } else if override_field {
                return Err(VerseError::check_at(
                    format!(
                        "field `{}` does not override an inherited field",
                        field.name
                    ),
                    source_field.span,
                ));
            }
            if inherited_methods
                .iter()
                .any(|candidate| candidate.name == field.name)
            {
                return Err(VerseError::check_at(
                    format!("duplicate inherited class member `{}`", field.name),
                    fields
                        .iter()
                        .find(|candidate| candidate.name == field.name)
                        .map_or_else(|| Span::new(0, 0, 1, 1), |candidate| candidate.span),
                ));
            }
            inherited_fields.push(field);
        }

        if !class_has_specifier(specifiers, "abstract") {
            self.ensure_interface_required_fields_initializable(
                class_name,
                specifiers,
                &inherited_fields,
            )?;
        }

        if class_has_specifier(specifiers, "concrete") {
            self.ensure_concrete_class_fields(class_name, &inherited_fields)?;
        }

        if class_has_specifier(specifiers, "persistable") {
            self.ensure_persistable_class(class_name, specifiers, base, &inherited_fields)?;
        }

        let local_method_signatures =
            self.class_method_signature_infos(class_name, methods, &inherited_methods)?;
        let method_signatures = self.merge_class_methods(
            &inherited_fields,
            inherited_methods.clone(),
            methods,
            local_method_signatures,
        )?;
        let (constructor_access, constructor_scopes) =
            class_constructor_access_from_specifiers(specifiers, class_span)?;
        let previous_info = self.struct_types.insert(
            class_name.to_string(),
            StructInfo {
                kind: AggregateKind::Class,
                base: base_name.clone(),
                interfaces: implemented_interfaces.clone(),
                unique,
                abstract_class: class_has_specifier(specifiers, "abstract"),
                epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                final_class: class_has_specifier(specifiers, "final"),
                concrete: class_has_specifier(specifiers, "concrete"),
                castable,
                native: self.aggregate_is_native(class_name),
                persistable: class_has_specifier(specifiers, "persistable"),
                computes: false,
                constructor_effects: class_constructor_effects(blocks),
                constructor_access,
                constructor_scopes,
                fields: inherited_fields.clone(),
                methods: method_signatures,
            },
        );
        let local_checks = self.with_local_extension_methods(extension_methods, |checker| {
            checker.check_class_extension_methods(
                class_name,
                base_name.as_deref(),
                &inherited_fields,
                extension_methods,
            )?;
            let local_methods = checker.class_method_infos(
                class_name,
                base_name.as_deref(),
                definition_access,
                &inherited_fields,
                &inherited_methods,
                methods,
            )?;
            let blocks_result = checker.check_class_blocks(
                class_name,
                base_name.as_deref(),
                &inherited_fields,
                blocks,
            );
            Ok((local_methods, blocks_result))
        });
        if let Some(previous_info) = previous_info {
            self.struct_types
                .insert(class_name.to_string(), previous_info);
        } else {
            self.struct_types.remove(class_name);
        }
        let (local_methods, blocks_result) = local_checks?;
        blocks_result?;
        let inherited_methods =
            self.merge_class_methods(&inherited_fields, inherited_methods, methods, local_methods)?;
        self.ensure_abstract_methods_implemented(
            class_name,
            class_has_specifier(specifiers, "abstract"),
            class_definition_diagnostic_span(base, fields, methods, extension_methods, blocks),
            &inherited_methods,
        )?;

        Ok((
            inherited_fields,
            inherited_methods,
            unique,
            castable,
            base_name,
            implemented_interfaces,
        ))
    }

    pub(super) fn ensure_abstract_methods_implemented(
        &self,
        class_name: &str,
        is_abstract: bool,
        class_span: Span,
        methods: &[ClassMethodInfo],
    ) -> Result<(), VerseError> {
        if is_abstract {
            return Ok(());
        }

        if let Some(method) = methods.iter().find(|method| method.abstract_member) {
            return Err(VerseError::check_at(
                format!(
                    "class `{class_name}` must be `abstract` or implement method `{}`",
                    method.name
                ),
                class_span,
            ));
        }

        Ok(())
    }

    pub(super) fn ensure_interface_required_fields_initializable(
        &self,
        class_name: &str,
        class_specifiers: &[String],
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        let class_is_public = class_has_specifier(class_specifiers, "public");
        for field in fields {
            let Some(owner) = field.owner.as_deref() else {
                continue;
            };
            if field.has_default || !self.interface_types.contains_key(owner) {
                continue;
            }
            let inaccessible_from_constructor =
                matches!(field.access, AccessLevel::Private | AccessLevel::Protected)
                    || (field.access == AccessLevel::Internal && class_is_public);
            if inaccessible_from_constructor {
                return Err(VerseError::check_at(
                    format!(
                        "class `{class_name}` must be `abstract` or provide a default value for interface field `{}`",
                        field.name
                    ),
                    field.span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn ensure_concrete_class_fields(
        &self,
        class_name: &str,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        for field in fields {
            if !field.has_default {
                return Err(VerseError::check_at(
                    format!(
                        "concrete class `{class_name}` field `{}` must have a default value",
                        field.name
                    ),
                    field.span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn ensure_persistable_class(
        &self,
        class_name: &str,
        specifiers: &[String],
        base: Option<&TypeAnnotation>,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        let span = fields
            .first()
            .map_or_else(|| Span::new(0, 0, 1, 1), |field| field.span);

        if !class_has_specifier(specifiers, "final") {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` must also be `final`"),
                span,
            ));
        }

        if class_has_specifier(specifiers, "unique") {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` cannot be `unique`"),
                span,
            ));
        }

        if let Some(base) = base {
            return Err(VerseError::check_at(
                format!("persistable class `{class_name}` cannot have a superclass"),
                base.span,
            ));
        }

        for field in fields {
            if field.mutable {
                return Err(VerseError::check_at(
                    format!(
                        "persistable class `{class_name}` field `{}` cannot be variable",
                        field.name
                    ),
                    field.span,
                ));
            }

            if !self.is_persistable_type(&field.value_type) {
                return Err(VerseError::check_at(
                    format!(
                        "persistable class `{class_name}` field `{}` has non-persistable type `{}`",
                        field.name, field.value_type
                    ),
                    field.span,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn ensure_persistable_struct(
        &self,
        struct_name: &str,
        fields: &[StructFieldInfo],
    ) -> Result<(), VerseError> {
        for field in fields {
            if !self.is_persistable_type(&field.value_type) {
                return Err(VerseError::check_at(
                    format!(
                        "persistable struct `{struct_name}` field `{}` has non-persistable type `{}`",
                        field.name, field.value_type
                    ),
                    field.span,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn check_class_blocks(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        fields: &[StructFieldInfo],
        blocks: &[ClassBlock],
    ) -> Result<(), VerseError> {
        for block in blocks {
            self.push_scope();
            self.class_context.push(class_name.to_string());
            let result = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    block.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        block.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        block.span,
                    )?;
                }
                self.define_current_class_type_if_unshadowed(class_name, block.span)?;
                if let Some(span) = defer_body_failable_expr(&block.body) {
                    return Err(VerseError::check_at(
                        "class block cannot contain failable expressions",
                        span,
                    ));
                }
                self.push_class_member_shadow_names(class_name, fields);
                self.function_effects
                    .push(class_constructor_effects(std::slice::from_ref(block)));
                let block_result = self.check_expr(&block.body);
                self.function_effects.pop();
                self.pop_class_member_shadow_names();
                block_result?;
                Ok(())
            })();
            self.class_context.pop();
            self.pop_scope();
            result?;
        }
        Ok(())
    }

    pub(super) fn is_field_override_assignable(
        &self,
        expected: &Type,
        actual: &Type,
        class_name: &str,
        base_name: Option<&str>,
    ) -> bool {
        self.is_assignable(expected, actual)
            || matches!(
                (expected, actual, base_name),
                (Type::Class(expected), Type::Class(actual), Some(base))
                    if expected == base && actual == class_name
            )
            || matches!(
                (expected, actual),
                (Type::Option(expected), Type::Option(actual))
                    if self.is_field_override_assignable(expected, actual, class_name, base_name)
            )
    }

    pub(super) fn merge_class_methods(
        &self,
        fields: &[StructFieldInfo],
        mut inherited_methods: Vec<ClassMethodInfo>,
        source_methods: &[ClassMethod],
        local_methods: Vec<ClassMethodInfo>,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        for method in local_methods {
            let source_method = source_methods
                .iter()
                .find(|candidate| candidate.span == method.span)
                .expect("local method info should have a source method");
            if fields.iter().any(|candidate| candidate.name == method.name) {
                return Err(VerseError::check_at(
                    format!("duplicate class member `{}`", method.name),
                    source_method.span,
                ));
            }

            let override_method = has_effect(&source_method.effects, "override");
            let matching_index =
                inherited_method_override_index(&inherited_methods, &method, &self.struct_types)?;
            let duplicate_index =
                inherited_method_duplicate_index(&inherited_methods, &method, &self.struct_types);

            if override_method {
                let Some(index) = matching_index else {
                    return Err(VerseError::check_at(
                        format!(
                            "method `{}` does not override an inherited method",
                            method.name
                        ),
                        source_method.span,
                    ));
                };
                let inherited = &inherited_methods[index];
                if inherited.final_member {
                    return Err(VerseError::check_at(
                        format!(
                            "method `{}` overrides final inherited method `{}`",
                            method.name, inherited.name
                        ),
                        source_method.span,
                    ));
                }
                self.ensure_assignable(
                    &inherited.value_type,
                    &method.value_type,
                    source_method.span,
                    || {
                        format!(
                            "override method `{}` must be assignable to inherited method type `{}`",
                            method.name, inherited.value_type
                        )
                    },
                )?;
                let mut replacement = method;
                let inherited_owner_is_interface = inherited
                    .owner
                    .as_deref()
                    .is_some_and(|owner| self.interface_types.contains_key(owner));
                if !inherited_owner_is_interface {
                    if !has_access_specifier(&source_method.effects) {
                        replacement.access = inherited.access;
                        replacement.scopes = inherited.scopes.clone();
                    }
                    if replacement.access != inherited.access
                        || (replacement.access == AccessLevel::Scoped
                            && replacement.scopes != inherited.scopes)
                    {
                        return Err(VerseError::check_at(
                            "An overridden method cannot change the inherited access level",
                            source_method.span,
                        ));
                    }
                    replacement.owner = inherited.owner.clone();
                }
                if replacement.qualifier.is_none() {
                    replacement.qualifier = inherited.qualifier.clone();
                }
                inherited_methods[index] = replacement;
            } else {
                if duplicate_index.is_some() {
                    return Err(VerseError::check_at(
                        format!("duplicate inherited class method `{}`", method.name),
                        source_method.span,
                    ));
                }
                inherited_methods.push(method);
            }
        }

        Ok(inherited_methods)
    }

    pub(super) fn merge_interface_methods(
        &self,
        mut inherited_methods: Vec<ClassMethodInfo>,
        interface_methods: Vec<ClassMethodInfo>,
        class_name: &str,
        base: Option<&TypeAnnotation>,
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let span = base.map_or_else(|| Span::new(0, 0, 1, 1), |base| base.span);
        for method in interface_methods {
            if let Some(existing_index) = inherited_methods
                .iter()
                .position(|candidate| {
                    method_signatures_conflict(candidate, &method, &self.struct_types)
                        && method_qualifiers_conflict(candidate, &method)
                })
                .or_else(|| {
                    inherited_methods.iter().position(|candidate| {
                        candidate.qualifier.is_none()
                            && method_signatures_conflict(candidate, &method, &self.struct_types)
                    })
                })
            {
                let existing = &inherited_methods[existing_index];
                self.ensure_assignable(&method.value_type, &existing.value_type, span, || {
                    format!(
                        "class `{class_name}` inherited method `{}` is not assignable to interface method type `{}`",
                        existing.name, method.value_type
                    )
                })?;
                if inherited_methods[existing_index].qualifier.is_none() {
                    inherited_methods[existing_index].qualifier = method.qualifier.clone();
                }
                continue;
            }
            inherited_methods.push(method);
        }
        Ok(inherited_methods)
    }

    pub(super) fn merge_interface_fields(
        &self,
        mut inherited_fields: Vec<StructFieldInfo>,
        interface_fields: Vec<StructFieldInfo>,
        class_name: &str,
        base: Option<&TypeAnnotation>,
    ) -> Result<Vec<StructFieldInfo>, VerseError> {
        let span = base.map_or_else(|| Span::new(0, 0, 1, 1), |base| base.span);
        for field in interface_fields {
            if let Some(existing) = inherited_fields
                .iter()
                .find(|candidate| candidate.name == field.name)
            {
                if existing.value_type != field.value_type || existing.mutable != field.mutable {
                    return Err(VerseError::check_at(
                        format!(
                            "class `{class_name}` inherited field `{}` is not assignable to interface field type `{}`",
                            existing.name, field.value_type
                        ),
                        span,
                    ));
                }
                continue;
            }
            inherited_fields.push(field);
        }
        Ok(inherited_fields)
    }

    pub(super) fn class_method_signature_infos(
        &mut self,
        class_name: &str,
        methods: &[ClassMethod],
        inherited_methods: &[ClassMethodInfo],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        let class_native = self.aggregate_is_native(class_name);
        for method in methods {
            self.validate_abstract_class_method_shape(method)?;
            let effects = self.class_or_interface_method_effects(method, inherited_methods)?;
            if has_effect(&method.effects, "native") && !class_native {
                return Err(VerseError::check_at(
                    "`native` method specifier requires a native enclosing class",
                    method.span,
                ));
            }
            let value_type = self.class_method_declared_type(method, effects)?;
            if has_effect(&method.effects, "native") {
                self.ensure_native_function_signature(&value_type, method.span)?;
            }
            let info = ClassMethodInfo {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                final_member: has_effect(&method.effects, "final"),
                abstract_member: self.class_method_is_abstract(method),
                access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                scopes: scoped_access_scopes(&method.effects).unwrap_or_default(),
                owner: Some(class_name.to_string()),
                value_type,
                span: method.span,
            };
            push_distinct_local_method_info(&mut infos, info, "class", &self.struct_types)?;
        }
        Ok(infos)
    }

    pub(super) fn class_method_declared_type(
        &mut self,
        method: &ClassMethod,
        effects: Vec<String>,
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(&effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects,
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(self.annotation_to_type(method.return_type.as_ref())?),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    pub(super) fn extension_receiver_type(
        &mut self,
        extension: &ExtensionMethod,
    ) -> Result<Type, VerseError> {
        let Some(annotation) = extension.receiver.annotation.as_ref() else {
            return Err(VerseError::check_at(
                "extension method receiver requires an explicit type annotation",
                extension.receiver.span,
            ));
        };
        self.type_name_to_type(annotation)
    }

    pub(super) fn ensure_extension_method_not_conflicting_with_member(
        &self,
        receiver_type: &Type,
        method: &ClassMethod,
        span: Span,
    ) -> Result<(), VerseError> {
        match receiver_type {
            Type::Class(class_name) => {
                let Some(info) = self.struct_types.get(class_name) else {
                    return Ok(());
                };
                if info.methods.iter().any(|member| member.name == method.name) {
                    return Err(VerseError::check_at(
                        format!(
                            "extension method `{}` conflicts with class `{class_name}` method `{}`",
                            method.name, method.name
                        ),
                        span,
                    ));
                }
            }
            Type::Interface(interface_name) => {
                let Some(info) = self.interface_types.get(interface_name) else {
                    return Ok(());
                };
                if info.methods.iter().any(|member| member.name == method.name) {
                    return Err(VerseError::check_at(
                        format!(
                            "extension method `{}` conflicts with interface `{interface_name}` method `{}`",
                            method.name, method.name
                        ),
                        span,
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub(super) fn extension_method_declared_type(
        &mut self,
        method: &ClassMethod,
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(&method.effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects: method.effects.clone(),
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(self.annotation_to_type(method.return_type.as_ref())?),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    pub(super) fn extension_method_type_with_return(
        &mut self,
        method: &ClassMethod,
        return_type: Type,
    ) -> Result<Type, VerseError> {
        validate_function_effect_combination(&method.effects, method.span)?;
        let type_params = collect_function_type_params(&method.params)?;
        self.validate_type_parameter_constraints(&type_params, method.span)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            Ok(Type::Function {
                arity: Some(method.params.len()),
                arity_range: None,
                effects: method.effects.clone(),
                param_types: Some(self.param_types(&method.params)?),
                param_specs: Some(self.param_specs(&method.params)?),
                return_type: Box::new(return_type.clone()),
            })
        })();
        self.pop_type_param_scope();
        result
    }

    pub(super) fn class_method_is_abstract(&self, method: &ClassMethod) -> bool {
        method.body.is_none() || has_effect(&method.effects, "abstract")
    }

    pub(super) fn validate_abstract_class_method_shape(
        &self,
        method: &ClassMethod,
    ) -> Result<(), VerseError> {
        if !self.class_method_is_abstract(method) {
            return Ok(());
        }

        if method.return_type.is_none() {
            return Err(VerseError::check_at(
                format!(
                    "abstract class method `{}` requires an explicit return type",
                    method.name
                ),
                method.span,
            ));
        }

        if has_effect(&method.effects, "final") {
            return Err(VerseError::check_at(
                format!("abstract class method `{}` cannot be `final`", method.name),
                method.span,
            ));
        }

        if method.body.is_some() && has_effect(&method.effects, "abstract") {
            return Err(VerseError::check_at(
                format!("abstract class method `{}` cannot have a body", method.name),
                method.span,
            ));
        }

        Ok(())
    }

    pub(super) fn class_method_infos(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        definition_access: AccessLevel,
        fields: &[StructFieldInfo],
        inherited_methods: &[ClassMethodInfo],
        methods: &[ClassMethod],
    ) -> Result<Vec<ClassMethodInfo>, VerseError> {
        let mut infos = Vec::with_capacity(methods.len());
        let class_native = self.aggregate_is_native(class_name);
        let method_bindings = self
            .struct_types
            .get(class_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for method in methods {
            self.validate_abstract_class_method_shape(method)?;
            if has_effect(&method.effects, "native") && !class_native {
                return Err(VerseError::check_at(
                    "`native` method specifier requires a native enclosing class",
                    method.span,
                ));
            }
            if method.body.is_none() {
                let effects = self.class_or_interface_method_effects(method, inherited_methods)?;
                let value_type = self.class_method_declared_type(method, effects)?;
                if has_effect(&method.effects, "native") {
                    self.ensure_native_function_signature(&value_type, method.span)?;
                }
                infos.push(ClassMethodInfo {
                    qualifier: method.qualifier.clone(),
                    name: method.name.clone(),
                    value_type,
                    final_member: has_effect(&method.effects, "final"),
                    abstract_member: true,
                    access: access_level_from_specifiers(&method.effects, "method", method.span)?,
                    scopes: scoped_access_scopes(&method.effects).unwrap_or_default(),
                    owner: Some(class_name.to_string()),
                    span: method.span,
                });
                continue;
            }

            self.push_scope();
            self.class_context.push(class_name.to_string());
            let method_type = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    method.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        method.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        method.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, method.span)?;
                }
                self.define_current_class_type_if_unshadowed(class_name, method.span)?;

                self.push_class_member_shadow_names(class_name, fields);
                let effects = self.class_or_interface_method_effects(method, inherited_methods)?;
                let function_result = self.check_function(
                    &method.params,
                    &effects,
                    method.return_type.as_ref(),
                    method
                        .body
                        .as_ref()
                        .expect("concrete class method should have a body"),
                );
                self.pop_class_member_shadow_names();
                function_result
            })();
            self.class_context.pop();
            self.pop_scope();

            let method_type = method_type?;
            if has_effect(&method.effects, "native") {
                self.ensure_native_function_signature(&method_type, method.span)?;
            }
            let access = access_level_from_specifiers(&method.effects, "method", method.span)?;
            self.ensure_aggregate_member_surface_dependencies_accessible(
                class_name,
                definition_access,
                &method.name,
                access,
                &method_type,
                method.span,
            )?;
            infos.push(ClassMethodInfo {
                qualifier: method.qualifier.clone(),
                name: method.name.clone(),
                value_type: method_type,
                final_member: has_effect(&method.effects, "final"),
                abstract_member: false,
                access,
                scopes: scoped_access_scopes(&method.effects).unwrap_or_default(),
                owner: Some(class_name.to_string()),
                span: method.span,
            });
        }

        Ok(infos)
    }

    pub(super) fn check_class_extension_methods(
        &mut self,
        class_name: &str,
        base_name: Option<&str>,
        fields: &[StructFieldInfo],
        extensions: &[ExtensionMethod],
    ) -> Result<(), VerseError> {
        let method_bindings = self
            .struct_types
            .get(class_name)
            .map(|info| method_binding_types(&info.methods))
            .unwrap_or_default();

        for extension in extensions {
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

            self.push_scope();
            self.class_context.push(class_name.to_string());
            let checked_type = (|| {
                self.define(
                    "Self",
                    Type::Class(class_name.to_string()),
                    false,
                    extension.span,
                )?;
                if let Some(base_name) = base_name {
                    self.define(
                        "super",
                        Type::ClassType(base_name.to_string()),
                        false,
                        extension.span,
                    )?;
                }
                for field in fields {
                    self.define(
                        &field.name,
                        field.value_type.clone(),
                        field.mutable,
                        extension.span,
                    )?;
                }
                for (name, value_type) in &method_bindings {
                    self.define(name, value_type.clone(), false, extension.span)?;
                }
                self.define_current_class_type_if_unshadowed(class_name, extension.span)?;

                self.push_class_member_shadow_names(class_name, fields);
                let function_result = self.check_function(
                    &params,
                    &extension.method.effects,
                    extension.method.return_type.as_ref(),
                    body,
                );
                self.pop_class_member_shadow_names();
                function_result
            })();
            self.class_context.pop();
            self.pop_scope();

            let Type::Function { return_type, .. } = checked_type? else {
                unreachable!("check_function should always return a function type");
            };
            let visible_type =
                self.extension_method_type_with_return(&extension.method, *return_type)?;
            self.update_local_extension_method_type(
                &extension.method.name,
                &receiver_type,
                visible_type,
                extension.span,
            )?;
        }

        Ok(())
    }

    pub(super) fn define_current_class_type_if_unshadowed(
        &mut self,
        class_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if class_name.contains('(') {
            return Ok(());
        }
        let name = aggregate_unqualified_name(class_name);
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            return Ok(());
        }
        self.define(name, Type::ClassType(class_name.to_string()), false, span)
    }
}

fn class_or_interface_default_method_effects(effects: &[String]) -> Vec<String> {
    class_or_interface_method_effects_with_inherited(effects, None)
}

pub(super) fn class_constructor_effects(blocks: &[ClassBlock]) -> Vec<String> {
    if blocks.is_empty() {
        Vec::new()
    } else {
        vec!["transacts".to_string()]
    }
}

fn class_or_interface_method_effects_with_inherited(
    effects: &[String],
    inherited: Option<&Type>,
) -> Vec<String> {
    let mut effective = effects.to_vec();
    if !has_effect(effects, "decides") && !has_explicit_call_effect_specifier(effects) {
        match inherited.and_then(function_call_effects) {
            Some(inherited_effects) => effective.extend(inherited_effects),
            None => effective.push("transacts".to_string()),
        }
    }
    effective
}

fn function_call_effects(value_type: &Type) -> Option<Vec<String>> {
    let Type::Function { effects, .. } = value_type else {
        return None;
    };
    Some(
        effects
            .iter()
            .filter(|effect| is_explicit_call_effect_name(effect))
            .cloned()
            .collect(),
    )
}

fn is_explicit_call_effect_name(effect: &str) -> bool {
    matches!(
        effect,
        "transacts" | "varies" | "computes" | "converges" | "reads" | "writes" | "allocates"
    )
}
