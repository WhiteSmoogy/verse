use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, TypeAnnotation, TypeParam};
use crate::colors::NAMED_COLORS;
use crate::token::Span;

use super::*;

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

impl Checker {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert(
            "print".to_string(),
            Symbol::immutable(Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::None),
            }),
        );
        globals.insert(
            "Print".to_string(),
            Symbol::immutable(Type::Overload(vec![
                print_function_type(char_array_type()),
                print_function_type(Type::Message),
                print_function_type(diagnostic_type()),
            ])),
        );
        globals.insert(
            "color".to_string(),
            Symbol::immutable(Type::StructType("color".to_string())),
        );
        globals.insert(
            "color_alpha".to_string(),
            Symbol::immutable(Type::StructType("color_alpha".to_string())),
        );
        globals.insert(
            "locale".to_string(),
            Symbol::immutable(Type::StructType("locale".to_string())),
        );
        globals.insert(
            "session_environment".to_string(),
            Symbol::immutable(Type::EnumType("session_environment".to_string())),
        );
        globals.insert(
            "NamedColors".to_string(),
            Symbol::immutable(Type::Module("NamedColors".to_string())),
        );
        for name in BUILTIN_INTERFACE_NAMES {
            globals.insert(
                (*name).to_string(),
                Symbol::immutable(Type::InterfaceType((*name).to_string())),
            );
        }
        globals.insert(
            "assert_eq".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("expected", Type::Any), ("actual", Type::Any)],
                Type::None,
            )),
        );
        globals.insert(
            "str".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("value", Type::Any)],
                Type::String,
            )),
        );
        globals.insert(
            "Err".to_string(),
            Symbol::immutable(native_function_type(
                &["computes"],
                vec![("Message", Type::String)],
                Type::Never,
            )),
        );
        globals.insert(
            "ToDiagnostic".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Value", Type::Any)],
                Type::Class("diagnostic".to_string()),
            )),
        );
        globals.insert(
            "GetSecondsSinceEpoch".to_string(),
            Symbol::immutable(native_function_type(&[], Vec::new(), Type::Float)),
        );
        globals.insert(
            "MakeColorFromSRGB".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Red", Type::Float),
                    ("Green", Type::Float),
                    ("Blue", Type::Float),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeColorFromSRGBValues".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Red", Type::Int),
                    ("Green", Type::Int),
                    ("Blue", Type::Int),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeSRGBFromColor".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Color", color_type())],
                Type::Tuple(vec![Type::Float, Type::Float, Type::Float]),
            )),
        );
        globals.insert(
            "MakeColorFromHex".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("hexString", char_array_type())],
                color_type(),
            )),
        );
        globals.insert(
            "MakeColorFromHSV".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("Hue", Type::Float),
                    ("Saturation", Type::Float),
                    ("Value", Type::Float),
                ],
                color_type(),
            )),
        );
        globals.insert(
            "MakeHSVFromColor".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Color", color_type())],
                Type::Tuple(vec![Type::Float, Type::Float, Type::Float]),
            )),
        );
        globals.insert(
            "MakeColorAlpha".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![
                    ("R", Type::Float),
                    ("G", Type::Float),
                    ("B", Type::Float),
                    ("A", Type::Float),
                ],
                color_alpha_type(),
            )),
        );
        globals.insert(
            "Over".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("CA1", color_alpha_type()), ("CA2", color_alpha_type())],
                color_alpha_type(),
            )),
        );
        globals.insert(
            "ToString".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&[], vec![("Val", Type::Int)], char_array_type()),
                native_function_type(&[], vec![("Val", Type::Float)], char_array_type()),
                native_function_type(
                    &["computes"],
                    vec![("String", char_array_type())],
                    char_array_type(),
                ),
                native_function_type(
                    &["computes"],
                    vec![("Character", Type::Char)],
                    char_array_type(),
                ),
                native_function_type(
                    &["computes"],
                    vec![("Character", Type::Char32)],
                    char_array_type(),
                ),
            ])),
        );
        globals.insert(
            "Localize".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![("Message", Type::Message)],
                Type::String,
            )),
        );
        globals.insert(
            "Join".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &[],
                    vec![
                        ("Strings", Type::Array(Box::new(Type::String))),
                        ("Separator", Type::String),
                    ],
                    Type::String,
                ),
                native_function_type(
                    &["transacts"],
                    vec![
                        ("Messages", Type::Array(Box::new(Type::Message))),
                        ("Separator", Type::Message),
                    ],
                    Type::Message,
                ),
            ])),
        );
        globals.insert(
            "GetRandomFloat".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Low", Type::Float), ("High", Type::Float)],
                Type::Float,
            )),
        );
        globals.insert(
            "GetRandomInt".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Low", Type::Int), ("High", Type::Int)],
                Type::Int,
            )),
        );
        globals.insert(
            "Shuffle".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                vec![("Input", Type::Array(Box::new(Type::Unknown)))],
                Type::Array(Box::new(Type::Unknown)),
            )),
        );
        globals.insert("Inf".to_string(), Symbol::immutable(Type::Float));
        globals.insert("NaN".to_string(), Symbol::immutable(Type::Float));
        globals.insert("PiFloat".to_string(), Symbol::immutable(Type::Float));
        globals.insert(
            "Concatenate".to_string(),
            Symbol::immutable(native_function_type(
                &[],
                vec![(
                    "Arrays",
                    Type::Array(Box::new(Type::Array(Box::new(Type::Unknown)))),
                )],
                Type::Array(Box::new(Type::Unknown)),
            )),
        );
        globals.insert(
            "ConcatenateMaps".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                ]),
                param_specs: None,
                return_type: Box::new(Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown))),
            }),
        );
        globals.insert(
            "MakeClassifiableSubset".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: vec!["transacts".to_string()],
                param_types: Some(vec![Type::Array(Box::new(Type::Unknown))]),
                param_specs: None,
                return_type: Box::new(Type::ClassifiableSubset(Box::new(Type::Unknown))),
            }),
        );
        globals.insert(
            "GetSession".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(0),
                arity_range: None,
                effects: vec!["transacts".to_string()],
                param_types: Some(Vec::new()),
                param_specs: None,
                return_type: Box::new(Type::Class("session".to_string())),
            }),
        );
        globals.insert(
            "GetSimulationElapsedTime".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts"],
                Vec::new(),
                Type::Float,
            )),
        );
        globals.insert(
            "Sleep".to_string(),
            Symbol::immutable(native_function_type(
                &["transacts", "suspends", "no_rollback"],
                vec![("Seconds", Type::Float)],
                Type::None,
            )),
        );
        globals.insert(
            "MakeSuccess".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Result(Box::new(Type::Any), Box::new(Type::Never))),
            }),
        );
        globals.insert(
            "MakeError".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: Vec::new(),
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Result(Box::new(Type::Never), Box::new(Type::Any))),
            }),
        );
        globals.insert(
            "FitsInPlayerMap".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: vec!["reads".to_string(), "decides".to_string()],
                param_types: Some(vec![Type::Any]),
                param_specs: None,
                return_type: Box::new(Type::Any),
            }),
        );
        globals.insert(
            "Mod".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: vec!["decides".to_string(), "computes".to_string()],
                param_types: Some(vec![Type::Int, Type::Int]),
                param_specs: None,
                return_type: Box::new(Type::Int),
            }),
        );
        globals.insert(
            "Quotient".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(2),
                arity_range: None,
                effects: vec!["decides".to_string(), "computes".to_string()],
                param_types: Some(vec![Type::Int, Type::Int]),
                param_specs: None,
                return_type: Box::new(Type::Int),
            }),
        );
        globals.insert(
            "Clamp".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &[],
                    vec![("Value", Type::Int), ("A", Type::Int), ("B", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &[],
                    vec![
                        ("Value", Type::Float),
                        ("A", Type::Float),
                        ("B", Type::Float),
                    ],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Lerp".to_string(),
            Symbol::immutable(native_function_type(
                &["computes"],
                vec![
                    ("From", Type::Float),
                    ("To", Type::Float),
                    ("Parameter", Type::Float),
                ],
                Type::Float,
            )),
        );
        globals.insert(
            "Abs".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&[], vec![("Value", Type::Int)], Type::Int),
                native_function_type(&[], vec![("Value", Type::Float)], Type::Float),
            ])),
        );
        globals.insert(
            "Min".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Int), ("Y", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Float), ("Y", Type::Float)],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Max".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Int), ("Y", Type::Int)],
                    Type::Int,
                ),
                native_function_type(
                    &["computes"],
                    vec![("X", Type::Float), ("Y", Type::Float)],
                    Type::Float,
                ),
            ])),
        );
        globals.insert(
            "Ceil".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Value", Type::Rational)], Type::Int),
                native_function_type(
                    &["reads", "computes", "decides"],
                    vec![("Val", Type::Float)],
                    Type::Int,
                ),
            ])),
        );
        globals.insert(
            "Floor".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Value", Type::Rational)], Type::Int),
                native_function_type(
                    &["reads", "computes", "decides"],
                    vec![("Val", Type::Float)],
                    Type::Int,
                ),
            ])),
        );
        globals.insert(
            "Round".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![("Val", Type::Float)],
                Type::Int,
            )),
        );
        globals.insert(
            "Int".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![("Val", Type::Float)],
                Type::Int,
            )),
        );
        for name in [
            "Sqrt", "Sin", "Cos", "Tan", "ArcSin", "ArcCos", "Sinh", "Cosh", "Tanh", "ArSinh",
            "ArCosh", "ArTanh", "Exp", "Ln",
        ] {
            globals.insert(
                name.to_string(),
                Symbol::immutable(native_function_type(
                    &["computes"],
                    vec![("X", Type::Float)],
                    Type::Float,
                )),
            );
        }
        globals.insert(
            "Sgn".to_string(),
            Symbol::immutable(Type::Overload(vec![
                native_function_type(&["computes"], vec![("Val", Type::Int)], Type::Int),
                native_function_type(&["computes"], vec![("Val", Type::Float)], Type::Float),
            ])),
        );
        for (name, params) in [
            ("Pow", vec![("A", Type::Float), ("B", Type::Float)]),
            ("Log", vec![("B", Type::Float), ("X", Type::Float)]),
        ] {
            globals.insert(
                name.to_string(),
                Symbol::immutable(native_function_type(&["computes"], params, Type::Float)),
            );
        }
        globals.insert(
            "ArcTan".to_string(),
            Symbol::immutable(Type::Function {
                arity: None,
                arity_range: Some((1, 2)),
                effects: vec!["computes".to_string()],
                param_types: Some(vec![Type::Float]),
                param_specs: None,
                return_type: Box::new(Type::Float),
            }),
        );
        globals.insert(
            "IsAlmostEqual".to_string(),
            Symbol::immutable(native_function_type(
                &["decides", "computes"],
                vec![
                    ("Val1", Type::Number),
                    ("Val2", Type::Number),
                    ("AbsoluteTolerance", Type::Number),
                ],
                Type::None,
            )),
        );

        let mut struct_types = HashMap::new();
        struct_types.insert("color".to_string(), builtin_color_info());
        struct_types.insert("color_alpha".to_string(), builtin_color_alpha_info());
        struct_types.insert("locale".to_string(), builtin_locale_info());
        let interface_types = builtin_interface_infos();

        let mut module_types = HashMap::new();
        let mut named_color_members = HashMap::new();
        let mut named_color_access = HashMap::new();
        for color in NAMED_COLORS {
            named_color_members.insert(color.name.to_string(), color_type());
            named_color_access.insert(color.name.to_string(), AccessLevel::Public);
        }
        module_types.insert(
            "NamedColors".to_string(),
            ModuleInfo {
                members: named_color_members,
                member_access: named_color_access,
                imports: Vec::new(),
            },
        );

        let mut enum_types = HashMap::new();
        enum_types.insert(
            "session_environment".to_string(),
            builtin_session_environment_info(),
        );

        Self {
            scopes: vec![globals],
            scope_imports: vec![Vec::new()],
            enum_types,
            struct_types,
            interface_types,
            module_types,
            extension_methods: HashMap::new(),
            parametric_types: HashMap::new(),
            predeclared_aggregate_values: HashSet::new(),
            type_alias_defs: HashMap::new(),
            type_aliases: HashMap::new(),
            type_param_scopes: Vec::new(),
            player_weak_maps: Vec::new(),
            module_path: Vec::new(),
            module_scope_depths: Vec::new(),
            break_depth: 0,
            function_returns: Vec::new(),
            function_effects: Vec::new(),
            failure_context_depth: 0,
            range_context_depth: 0,
            defer_depth: 0,
            data_member_default_depth: 0,
            data_member_default_stack: Vec::new(),
            async_expr_markers: Vec::new(),
            suppressed_async_expr_markers: 0,
            class_context: Vec::new(),
            class_member_shadow_names: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            semantic_facts: SemanticFacts::default(),
            recovering: false,
        }
    }
}

impl Checker {
    pub(super) fn predeclare_top_level_module_member_access(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_module_member_access(&program.statements)
    }

    pub(super) fn predeclare_module_member_access(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    specifiers,
                    expr,
                    ..
                } => match &expr.kind {
                    ExprKind::ModuleDefinition {
                        statements: module_statements,
                        ..
                    } => {
                        self.record_current_module_member_access(name, specifiers, statement.span)?;
                        self.module_path.push(name.clone());
                        self.predeclare_module_member_access(module_statements)?;
                        self.module_path.pop();
                    }
                    ExprKind::EnumDefinition { .. }
                    | ExprKind::StructDefinition { .. }
                    | ExprKind::ClassDefinition { .. }
                    | ExprKind::InterfaceDefinition { .. } => {
                        self.record_current_module_member_access(
                            name,
                            module_member_specifiers(specifiers, expr),
                            statement.span,
                        )?;
                    }
                    _ => {}
                },
                StmtKind::ParametricType {
                    name, specifiers, ..
                } => self.record_current_module_member_access(name, specifiers, statement.span)?,
                StmtKind::TypeAlias { name, .. } => {
                    self.record_current_module_member_access(name, &[], statement.span)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub(super) fn predeclare_top_level_modules(&mut self, program: &Program) {
        self.predeclare_modules(&program.statements);
    }

    pub(super) fn predeclare_modules(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::ModuleDefinition { statements, .. } = &expr.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            self.module_types.entry(qualified).or_insert(ModuleInfo {
                members: HashMap::new(),
                member_access: HashMap::new(),
                imports: Vec::new(),
            });

            self.module_path.push(name.clone());
            self.predeclare_modules(statements);
            self.module_path.pop();
        }
    }

    pub(super) fn predeclare_top_level_enums(&mut self, program: &Program) {
        self.predeclare_enums(&program.statements);
    }

    pub(super) fn predeclare_enums(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            match &expr.kind {
                ExprKind::EnumDefinition {
                    open,
                    persistable,
                    variants,
                    ..
                } => {
                    let qualified = self.current_qualified_name(name);
                    self.enum_types.entry(qualified).or_insert(EnumInfo {
                        variants: enum_variant_names(variants),
                        open: *open,
                        persistable: *persistable,
                    });
                }
                ExprKind::ModuleDefinition { statements, .. } => {
                    self.module_path.push(name.clone());
                    self.predeclare_enums(statements);
                    self.module_path.pop();
                }
                _ => {}
            }
        }
    }

    pub(super) fn predeclare_top_level_aggregate_names(&mut self, program: &Program) {
        self.predeclare_aggregate_names(&program.statements);
    }

    pub(super) fn predeclare_aggregate_names(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            if matches!(expr.kind, ExprKind::InterfaceDefinition { .. }) {
                let qualified = self.current_qualified_name(name);
                self.interface_types
                    .entry(qualified)
                    .or_insert(InterfaceInfo {
                        parents: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    });
                continue;
            }

            let (kind, persistable, computes) = match &expr.kind {
                ExprKind::StructDefinition {
                    persistable,
                    computes,
                    ..
                } => (AggregateKind::Struct, *persistable, *computes),
                ExprKind::ClassDefinition { specifiers, .. } => (
                    AggregateKind::Class,
                    class_has_specifier(specifiers, "persistable"),
                    false,
                ),
                ExprKind::ModuleDefinition { statements, .. } => {
                    self.module_path.push(name.clone());
                    self.predeclare_aggregate_names(statements);
                    self.module_path.pop();
                    continue;
                }
                _ => continue,
            };
            let qualified = self.current_qualified_name(name);
            self.struct_types.entry(qualified).or_insert(StructInfo {
                kind,
                base: None,
                interfaces: Vec::new(),
                unique: false,
                abstract_class: false,
                epic_internal_class: false,
                final_class: false,
                concrete: false,
                castable: false,
                persistable,
                computes,
                fields: Vec::new(),
                methods: Vec::new(),
            });
        }
    }

    pub(super) fn predeclare_top_level_aggregate_values(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_aggregate_values_in_current_scope(&program.statements)
    }

    pub(super) fn predeclare_aggregate_values_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            let value_type = match &expr.kind {
                ExprKind::StructDefinition { .. } => Type::StructType(qualified.clone()),
                ExprKind::ClassDefinition { .. } => Type::ClassType(qualified.clone()),
                ExprKind::InterfaceDefinition { .. } => Type::InterfaceType(qualified.clone()),
                _ => continue,
            };

            self.define_predeclared_aggregate_value(name, &qualified, value_type, statement.span)?;
        }

        Ok(())
    }

    pub(super) fn predeclare_top_level_parametric_types(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_parametric_types(&program.statements)
    }

    pub(super) fn predeclare_parametric_types(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let { name, expr, .. } => {
                    if let ExprKind::ModuleDefinition {
                        statements: module_statements,
                        ..
                    } = &expr.kind
                    {
                        self.module_path.push(name.clone());
                        self.predeclare_parametric_types(module_statements)?;
                        self.module_path.pop();
                    }
                }
                StmtKind::ParametricType {
                    name,
                    specifiers: _,
                    params,
                    expr,
                } => {
                    let kind = parametric_type_kind(expr).ok_or_else(|| {
                        VerseError::check_at(
                            "parametric type definitions must define a class, struct, or interface",
                            statement.span,
                        )
                    })?;
                    let qualified = self.current_qualified_name(name);
                    if matches!(
                        &expr.kind,
                        ExprKind::ClassDefinition { specifiers, .. }
                            if class_has_specifier(specifiers, "persistable")
                    ) {
                        return Err(VerseError::check_at(
                            format!("persistable class `{qualified}` cannot be parametric"),
                            statement.span,
                        ));
                    }
                    if matches!(
                        &expr.kind,
                        ExprKind::StructDefinition {
                            persistable: true,
                            ..
                        }
                    ) {
                        return Err(VerseError::check_at(
                            format!("persistable struct `{qualified}` cannot be parametric"),
                            statement.span,
                        ));
                    }
                    self.validate_type_parameter_names(params, statement.span)?;
                    if self.parametric_types.contains_key(&qualified) {
                        return Err(VerseError::check_at(
                            format!("duplicate parametric type `{name}`"),
                            statement.span,
                        ));
                    }
                    if self.enum_types.contains_key(&qualified)
                        || self.struct_types.contains_key(&qualified)
                        || self.interface_types.contains_key(&qualified)
                        || self.type_alias_defs.contains_key(&qualified)
                    {
                        return Err(VerseError::check_at(
                            format!(
                                "parametric type `{name}` conflicts with existing type `{name}`"
                            ),
                            statement.span,
                        ));
                    }
                    self.parametric_types.insert(
                        qualified,
                        ParametricTypeInfo {
                            params: params.clone(),
                            expr: expr.clone(),
                            kind,
                            module_path: self.module_path.clone(),
                            span: statement.span,
                        },
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn predeclare_top_level_type_aliases(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_type_aliases(&program.statements)?;

        let names = self.type_alias_defs.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.resolve_type_alias(&name, &mut Vec::new())?;
        }

        Ok(())
    }

    pub(super) fn predeclare_type_aliases(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            if let StmtKind::Let { name, expr, .. } = &statement.kind
                && let ExprKind::ModuleDefinition {
                    statements: module_statements,
                    ..
                } = &expr.kind
            {
                self.module_path.push(name.clone());
                self.predeclare_type_aliases(module_statements)?;
                self.module_path.pop();
                continue;
            }

            let StmtKind::TypeAlias { name, target } = &statement.kind else {
                continue;
            };

            let qualified = self.current_qualified_name(name);
            self.validate_type_alias_name(name, &qualified, statement.span)?;
            if self.type_alias_defs.contains_key(&qualified) {
                return Err(VerseError::check_at(
                    format!("duplicate type alias `{name}`"),
                    statement.span,
                ));
            }

            self.type_alias_defs.insert(
                qualified,
                TypeAliasInfo {
                    target: target.clone(),
                    span: statement.span,
                    module_path: self.module_path.clone(),
                },
            );
        }

        Ok(())
    }

    pub(super) fn validate_type_alias_name(
        &self,
        name: &str,
        qualified: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if is_reserved_type_alias_name(name) {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with builtin type name"),
                span,
            ));
        }

        if self.enum_types.contains_key(qualified) || self.struct_types.contains_key(qualified) {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with existing type `{name}`"),
                span,
            ));
        }

        if self.module_types.contains_key(qualified)
            || (self.module_path.is_empty() && self.scopes[0].contains_key(name))
        {
            return Err(VerseError::check_at(
                format!("type alias `{name}` conflicts with existing value `{name}`"),
                span,
            ));
        }

        Ok(())
    }

    pub(super) fn resolve_type_alias(
        &mut self,
        name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<Type, VerseError> {
        if let Some(value_type) = self.type_aliases.get(name) {
            return Ok(value_type.clone());
        }

        let Some(info) = self.type_alias_defs.get(name).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown type `{name}`"),
                Span::new(0, 0, 1, 1),
            ));
        };

        if visiting.iter().any(|item| item == name) {
            let mut cycle = visiting.clone();
            cycle.push(name.to_string());
            return Err(VerseError::check_at(
                format!("cyclic type alias `{}`", cycle.join(" -> ")),
                info.span,
            ));
        }

        visiting.push(name.to_string());
        let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
        let value_type =
            self.resolve_type_alias_target(&info.target.name, info.target.span, visiting);
        self.module_path = previous_module_path;
        visiting.pop();
        let value_type = value_type?;

        self.type_aliases
            .insert(name.to_string(), value_type.clone());
        Ok(value_type)
    }

    pub(super) fn resolve_type_alias_target(
        &mut self,
        name: &TypeName,
        span: Span,
        visiting: &mut Vec<String>,
    ) -> Result<Type, VerseError> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.resolve_type_alias_target(item, span, visiting)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => {
                let key_type = self.resolve_type_alias_target(key, span, visiting)?;
                let value_type = self.resolve_type_alias_target(value, span, visiting)?;
                ensure_comparable_key(&key_type, &self.struct_types, span)?;
                Type::Map(Box::new(key_type), Box::new(value_type))
            }
            TypeName::WeakMap(key, value) => {
                let key_type = self.resolve_type_alias_target(key, span, visiting)?;
                let value_type = self.resolve_type_alias_target(value, span, visiting)?;
                validate_weak_map_type(
                    &key_type,
                    &value_type,
                    span,
                    &self.enum_types,
                    &self.struct_types,
                )?;
                Type::WeakMap(Box::new(key_type), Box::new(value_type))
            }
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.resolve_type_alias_target(item, span, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeName::Option(item) => Type::Option(Box::new(
                self.resolve_type_alias_target(item, span, visiting)?,
            )),
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.resolve_type_alias_target(param, span, visiting))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.resolve_type_alias_target(
                    return_type,
                    span,
                    visiting,
                )?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.resolve_type_alias_target(arg, span, visiting))
                    .collect::<Result<Vec<_>, _>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(alias_name) = self.resolve_type_alias_reference(name, span)? {
                    self.resolve_type_alias(&alias_name, visiting)?
                } else {
                    self.named_type_to_type(name, span)?
                }
            }
        };
        Ok(value_type)
    }

    pub(super) fn resolve_type_alias_reference(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Option<String>, VerseError> {
        if self.type_alias_defs.contains_key(name) {
            self.ensure_qualified_type_alias_accessible(name, span)?;
            return Ok(Some(name.to_string()));
        }

        if !name.contains('.') {
            if let Some(qualified) = self
                .resolve_contextual_type_name(name)
                .filter(|qualified| self.type_alias_defs.contains_key(qualified))
            {
                self.ensure_qualified_type_alias_accessible(&qualified, span)?;
                Ok(Some(qualified))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub(super) fn named_type_to_type(&self, name: &str, span: Span) -> Result<Type, VerseError> {
        if self.enum_types.contains_key(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            return Ok(Type::Enum(name.to_string()));
        }
        if self.interface_types.contains_key(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            return Ok(Type::Interface(name.to_string()));
        }
        if let Some(info) = self.struct_types.get(name) {
            self.ensure_qualified_type_name_accessible(name, span)?;
            match info.kind {
                AggregateKind::Struct => return Ok(Type::Struct(name.to_string())),
                AggregateKind::Class => return Ok(Type::Class(name.to_string())),
            }
        }

        if !name.contains('.')
            && let Some(qualified) = self.resolve_contextual_type_name(name)
        {
            if self.enum_types.contains_key(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::Enum(qualified));
            }
            if self.interface_types.contains_key(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(Type::Interface(qualified));
            }
            if let Some(info) = self.struct_types.get(&qualified) {
                self.ensure_qualified_type_name_accessible(&qualified, span)?;
                return Ok(match info.kind {
                    AggregateKind::Struct => Type::Struct(qualified),
                    AggregateKind::Class => Type::Class(qualified),
                });
            }
        }

        if is_builtin_class_type_name(name) {
            return Ok(Type::Class(name.to_string()));
        }

        Err(VerseError::check_at(format!("unknown type `{name}`"), span))
    }

    pub(super) fn ensure_qualified_type_name_accessible(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some((module_name, member_name)) = name.rsplit_once('.') else {
            return Ok(());
        };
        let Some(module_info) = self.module_types.get(module_name) else {
            return Ok(());
        };
        let access = module_info
            .member_access
            .get(member_name)
            .copied()
            .unwrap_or(AccessLevel::Internal);
        self.ensure_module_member_accessible(module_name, access, member_name, span)
    }

    pub(super) fn ensure_qualified_type_alias_accessible(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some((module_name, member_name)) = name.rsplit_once('.') else {
            return Ok(());
        };
        let Some(module_info) = self.module_types.get(module_name) else {
            return Ok(());
        };
        let access = module_info
            .member_access
            .get(member_name)
            .copied()
            .unwrap_or(AccessLevel::Internal);
        self.ensure_module_member_accessible(module_name, access, member_name, span)
    }

    pub(super) fn current_qualified_name(&self, name: &str) -> String {
        if self.module_path.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.module_path.join("."), name)
        }
    }

    pub(super) fn current_definition_level(&self) -> bool {
        self.scopes.len() == 1
            || self
                .module_scope_depths
                .last()
                .is_some_and(|depth| self.scopes.len() == *depth)
    }

    pub(super) fn current_module_name(&self) -> Option<String> {
        (!self.module_path.is_empty()).then(|| self.module_path.join("."))
    }

    pub(super) fn resolve_contextual_type_name(&self, name: &str) -> Option<String> {
        if !self.module_path.is_empty() {
            let qualified = self.current_qualified_name(name);
            if self.enum_types.contains_key(&qualified)
                || self.interface_types.contains_key(&qualified)
                || self.struct_types.contains_key(&qualified)
                || self.type_aliases.contains_key(&qualified)
                || self.type_alias_defs.contains_key(&qualified)
                || self.parametric_types.contains_key(&qualified)
            {
                return Some(qualified);
            }
        }

        if let Some(module_name) = self.current_module_name()
            && let Some(module_info) = self.module_types.get(&module_name)
            && let Some(qualified) = module_info.imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                (self.enum_types.contains_key(&qualified)
                    || self.interface_types.contains_key(&qualified)
                    || self.struct_types.contains_key(&qualified)
                    || self.type_aliases.contains_key(&qualified)
                    || self.type_alias_defs.contains_key(&qualified)
                    || self.parametric_types.contains_key(&qualified))
                .then_some(qualified)
            })
        {
            return Some(qualified);
        }

        self.scope_imports.iter().rev().find_map(|imports| {
            imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                (self.enum_types.contains_key(&qualified)
                    || self.interface_types.contains_key(&qualified)
                    || self.struct_types.contains_key(&qualified)
                    || self.type_aliases.contains_key(&qualified)
                    || self.type_alias_defs.contains_key(&qualified)
                    || self.parametric_types.contains_key(&qualified))
                .then_some(qualified)
            })
        })
    }

    pub(super) fn resolve_parametric_type_reference(&self, name: &str) -> Option<String> {
        if self.parametric_types.contains_key(name) {
            return Some(name.to_string());
        }
        if !name.contains('.') {
            self.resolve_contextual_type_name(name)
                .filter(|qualified| self.parametric_types.contains_key(qualified))
        } else {
            None
        }
    }

    pub(super) fn resolve_type_param(&self, name: &str) -> Option<Type> {
        if name.contains('.') {
            return None;
        }
        self.type_param_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    pub(super) fn push_type_param_scope(
        &mut self,
        params: impl IntoIterator<Item = (String, Type)>,
    ) {
        self.type_param_scopes.push(params.into_iter().collect());
    }

    pub(super) fn pop_type_param_scope(&mut self) {
        self.type_param_scopes.pop();
    }

    pub(super) fn resolve_module_path(&self, path: &str) -> Option<String> {
        if self.module_types.contains_key(path) {
            return Some(path.to_string());
        }

        if !path.contains('.') && !self.module_path.is_empty() {
            let qualified = self.current_qualified_name(path);
            if self.module_types.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        self.scope_imports.iter().rev().find_map(|imports| {
            imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{path}");
                self.module_types
                    .contains_key(&qualified)
                    .then_some(qualified)
            })
        })
    }

    pub(super) fn current_scope_imports_mut(&mut self) -> &mut Vec<String> {
        self.scope_imports
            .last_mut()
            .expect("checker should always have import scope")
    }

    pub(super) fn add_current_import(&mut self, module_name: String) {
        let imports = self.current_scope_imports_mut();
        if !imports.iter().any(|import| import == &module_name) {
            imports.push(module_name.clone());
        }
        if let Some(current_module) = self.current_module_name()
            && let Some(info) = self.module_types.get_mut(&current_module)
            && !info.imports.iter().any(|import| import == &module_name)
        {
            info.imports.push(module_name);
        }
    }

    pub(super) fn predeclare_using_imports(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Using { path } = &statement.kind else {
                continue;
            };
            if is_absolute_module_path(path) {
                continue;
            }
            let Some(module_name) = self.resolve_module_path(path) else {
                return Err(VerseError::check_at(
                    format!("unsupported module path `{path}`"),
                    statement.span,
                ));
            };
            self.add_current_import(module_name);
        }
        Ok(())
    }

    pub(super) fn predeclare_using_imports_recursive(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        self.predeclare_using_imports(statements)?;

        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            else {
                continue;
            };

            self.module_path.push(name.clone());
            self.push_scope();
            let result = self.predeclare_using_imports_recursive(module_statements);
            self.pop_scope();
            self.module_path.pop();
            result?;
        }

        Ok(())
    }

    pub(super) fn annotation_to_type(
        &mut self,
        annotation: Option<&TypeAnnotation>,
    ) -> Result<Type, VerseError> {
        annotation
            .map(|annotation| self.type_name_to_type(annotation))
            .unwrap_or(Ok(Type::Unknown))
    }

    pub(super) fn type_name_to_type(
        &mut self,
        annotation: &TypeAnnotation,
    ) -> Result<Type, VerseError> {
        self.type_name_to_type_name(&annotation.name, annotation.span)
    }

    pub(super) fn type_name_to_type_name(
        &mut self,
        name: &TypeName,
        span: Span,
    ) -> Result<Type, VerseError> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.type_name_to_type_name(item, span)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => {
                let key_type = self.type_name_to_type_name(key, span)?;
                let value_type = self.type_name_to_type_name(value, span)?;
                ensure_comparable_key(&key_type, &self.struct_types, span)?;
                Type::Map(Box::new(key_type), Box::new(value_type))
            }
            TypeName::WeakMap(key, value) => {
                let key_type = self.type_name_to_type_name(key, span)?;
                let value_type = self.type_name_to_type_name(value, span)?;
                validate_weak_map_type(
                    &key_type,
                    &value_type,
                    span,
                    &self.enum_types,
                    &self.struct_types,
                )?;
                Type::WeakMap(Box::new(key_type), Box::new(value_type))
            }
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.type_name_to_type_name(item, span))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeName::Option(item) => {
                Type::Option(Box::new(self.type_name_to_type_name(item, span)?))
            }
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.type_name_to_type_name(param, span))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.type_name_to_type_name(return_type, span)?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.type_name_to_type_name(arg, span))
                    .collect::<Result<Vec<_>, _>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(value_type) = self.type_aliases.get(name).cloned() {
                    self.ensure_qualified_type_alias_accessible(name, span)?;
                    value_type
                } else if !name.contains('.')
                    && let Some(qualified) = self.resolve_contextual_type_name(name)
                    && let Some(value_type) = self.type_aliases.get(&qualified).cloned()
                {
                    self.ensure_qualified_type_alias_accessible(&qualified, span)?;
                    value_type
                } else {
                    self.named_type_to_type(name, span)?
                }
            }
        };
        Ok(value_type)
    }

    pub(super) fn type_name_to_type_name_for_assignability(&self, name: &TypeName) -> Option<Type> {
        let value_type = match name {
            TypeName::Int => Type::Int,
            TypeName::Float => Type::Float,
            TypeName::Rational => Type::Rational,
            TypeName::Number => Type::Number,
            TypeName::Bool => Type::Bool,
            TypeName::String => Type::String,
            TypeName::Message => Type::Message,
            TypeName::Char => Type::Char,
            TypeName::Char8 => Type::Char8,
            TypeName::Char32 => Type::Char32,
            TypeName::None => Type::None,
            TypeName::Any => Type::Any,
            TypeName::Comparable => Type::Comparable,
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::Array(item) => Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.type_name_to_type_name_for_assignability(item)?,
                None => Type::Unknown,
            })),
            TypeName::Map(key, value) => Type::Map(
                Box::new(self.type_name_to_type_name_for_assignability(key)?),
                Box::new(self.type_name_to_type_name_for_assignability(value)?),
            ),
            TypeName::WeakMap(key, value) => Type::WeakMap(
                Box::new(self.type_name_to_type_name_for_assignability(key)?),
                Box::new(self.type_name_to_type_name_for_assignability(value)?),
            ),
            TypeName::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|item| self.type_name_to_type_name_for_assignability(item))
                    .collect::<Option<Vec<_>>>()?,
            ),
            TypeName::Option(item) => Type::Option(Box::new(
                self.type_name_to_type_name_for_assignability(item)?,
            )),
            TypeName::Function => Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            },
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.type_name_to_type_name_for_assignability(param))
                        .collect::<Option<Vec<_>>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.type_name_to_type_name_for_assignability(return_type)?),
            },
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.type_name_to_type_name_for_assignability(arg))
                    .collect::<Option<Vec<_>>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, Span::new(0, 0, 0, 0)).ok()?
                } else {
                    let qualified = self.resolve_parametric_type_reference(name)?;
                    let info = self.parametric_types.get(&qualified)?;
                    let instance_name = render_parametric_instance_type_name(&qualified, &args);
                    match info.kind {
                        ParametricTypeKind::Struct
                            if self.struct_types.contains_key(&instance_name) =>
                        {
                            Type::Struct(instance_name)
                        }
                        ParametricTypeKind::Class
                            if self.struct_types.contains_key(&instance_name) =>
                        {
                            Type::Class(instance_name)
                        }
                        ParametricTypeKind::Interface
                            if self.interface_types.contains_key(&instance_name) =>
                        {
                            Type::Interface(instance_name)
                        }
                        _ => return None,
                    }
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = self.resolve_type_param(name) {
                    value_type
                } else if let Some(value_type) = self.type_aliases.get(name).cloned() {
                    value_type
                } else if !name.contains('.')
                    && let Some(qualified) = self.resolve_contextual_type_name(name)
                    && let Some(value_type) = self.type_aliases.get(&qualified).cloned()
                {
                    value_type
                } else {
                    self.named_type_to_type(name, Span::new(0, 0, 0, 0)).ok()?
                }
            }
        };
        Some(value_type)
    }

    pub(super) fn constrained_type_param_supertype(
        &mut self,
        value_type: &Type,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        let Type::Param(_, TypeParamConstraint::Subtype(parent)) = value_type else {
            return Ok(None);
        };
        let supertype = self.type_name_to_type_name(parent, span)?;
        Ok((!matches!(supertype, Type::Param(_, _))).then_some(supertype))
    }

    pub(super) fn constrained_type_param_supertype_for_assignability(
        &self,
        value_type: &Type,
    ) -> Option<Type> {
        let Type::Param(_, TypeParamConstraint::Subtype(parent)) = value_type else {
            return None;
        };
        let supertype = self.type_name_to_type_name_for_assignability(parent)?;
        (!matches!(supertype, Type::Param(_, _))).then_some(supertype)
    }

    pub(super) fn param_types(&mut self, params: &[Param]) -> Result<Vec<Type>, VerseError> {
        params
            .iter()
            .map(|param| self.annotation_to_type(param.annotation.as_ref()))
            .collect()
    }

    pub(super) fn param_specs(&mut self, params: &[Param]) -> Result<Vec<ParamSpec>, VerseError> {
        params.iter().map(|param| self.param_spec(param)).collect()
    }

    pub(super) fn param_spec(&mut self, param: &Param) -> Result<ParamSpec, VerseError> {
        Ok(ParamSpec {
            name: param.name.clone(),
            value_type: self.annotation_to_type(param.annotation.as_ref())?,
            named: param.named,
            has_default: param.default.is_some(),
            tuple_items: match &param.pattern {
                ParamPattern::Tuple(params) => Some(self.param_specs(params)?),
                ParamPattern::Binding | ParamPattern::Anonymous => None,
            },
        })
    }

    pub(super) fn instantiate_parametric_type(
        &mut self,
        name: &str,
        args: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        let Some(qualified) = self.resolve_parametric_type_reference(name) else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        self.ensure_qualified_type_name_accessible(&qualified, span)?;
        let Some(info) = self.parametric_types.get(&qualified).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        if info.params.len() != args.len() {
            return Err(VerseError::check_at(
                format!(
                    "parametric type `{name}` expected {} type arguments, got {}",
                    info.params.len(),
                    args.len()
                ),
                span,
            ));
        }
        self.ensure_type_arguments_satisfy_constraints(&info.params, args, span)?;

        let instance_name = render_parametric_instance_type_name(&qualified, args);
        match info.kind {
            ParametricTypeKind::Struct if self.struct_types.contains_key(&instance_name) => {
                return Ok(Type::Struct(instance_name));
            }
            ParametricTypeKind::Class if self.struct_types.contains_key(&instance_name) => {
                return Ok(Type::Class(instance_name));
            }
            ParametricTypeKind::Interface if self.interface_types.contains_key(&instance_name) => {
                return Ok(Type::Interface(instance_name));
            }
            _ => {}
        }

        let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
        self.push_type_param_scope(
            info.params
                .iter()
                .zip(args)
                .map(|(param, arg)| (param.name.clone(), arg.clone())),
        );
        let result = (|| match &info.expr.kind {
            ExprKind::StructDefinition {
                fields,
                persistable,
                computes,
                ..
            } => {
                self.struct_types.insert(
                    instance_name.clone(),
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
                        persistable: *persistable,
                        computes: *computes,
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let fields = self.struct_field_infos_with_owner(fields, Some(&instance_name))?;
                if *persistable {
                    self.ensure_persistable_struct(&instance_name, &fields)?;
                }
                self.struct_types.insert(
                    instance_name.clone(),
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
                        persistable: *persistable,
                        computes: *computes,
                        fields,
                        methods: Vec::new(),
                    },
                );
                Ok(Type::Struct(instance_name.clone()))
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
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base: None,
                        interfaces: Vec::new(),
                        unique: class_has_specifier(specifiers, "unique"),
                        abstract_class: class_has_specifier(specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                        final_class: class_has_specifier(specifiers, "final"),
                        concrete: class_has_specifier(specifiers, "concrete"),
                        castable: class_has_specifier(specifiers, "castable"),
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let (fields, methods, unique, castable, base, implemented_interfaces) = self
                    .class_member_infos(
                        &instance_name,
                        ClassDefinitionParts {
                            specifiers,
                            base: base.as_ref(),
                            interfaces,
                            fields,
                            methods,
                            extension_methods,
                            blocks,
                        },
                    )?;
                self.struct_types.insert(
                    instance_name.clone(),
                    StructInfo {
                        kind: AggregateKind::Class,
                        base,
                        interfaces: implemented_interfaces,
                        unique,
                        abstract_class: class_has_specifier(specifiers, "abstract"),
                        epic_internal_class: class_has_specifier(specifiers, "epic_internal"),
                        final_class: class_has_specifier(specifiers, "final"),
                        concrete: class_has_specifier(specifiers, "concrete"),
                        castable,
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        fields,
                        methods,
                    },
                );
                Ok(Type::Class(instance_name.clone()))
            }
            ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } => {
                self.interface_types.insert(
                    instance_name.clone(),
                    InterfaceInfo {
                        parents: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let parent_names = self.interface_parent_names(parents)?;
                let inherited_fields = self.interface_field_requirements(&parent_names)?;
                let local_fields =
                    self.struct_field_infos_with_owner(fields, Some(&instance_name))?;
                let fields =
                    self.merge_interface_field_set(inherited_fields, local_fields, info.span)?;
                let inherited_methods = self.interface_method_requirements(&parent_names)?;
                let local_methods = self.interface_local_method_infos(&instance_name, methods)?;
                let method_infos =
                    self.merge_interface_method_set(inherited_methods, local_methods, info.span)?;
                self.interface_types.insert(
                    instance_name.clone(),
                    InterfaceInfo {
                        parents: parent_names,
                        fields: fields.clone(),
                        methods: method_infos,
                    },
                );
                self.check_interface_method_bodies(&instance_name, &fields, methods)?;
                Ok(Type::Interface(instance_name.clone()))
            }
            _ => Err(VerseError::check_at(
                "parametric type definitions must define a class, struct, or interface",
                info.span,
            )),
        })();
        self.pop_type_param_scope();
        self.module_path = previous_module_path;
        result
    }

    pub(super) fn check_parametric_type_call_args(
        &mut self,
        name: &str,
        args: &[CallArg],
        span: Span,
    ) -> Result<Vec<Type>, VerseError> {
        let Some(qualified) = self.resolve_parametric_type_reference(name) else {
            return Err(VerseError::check_at(
                format!("unknown parametric type `{name}`"),
                span,
            ));
        };
        self.ensure_qualified_type_name_accessible(&qualified, span)?;
        let expected = self
            .parametric_types
            .get(&qualified)
            .map(|info| info.params.len())
            .unwrap_or(0);
        if args.len() != expected {
            return Err(VerseError::check_at(
                format!(
                    "parametric type `{name}` expected {expected} type arguments, got {}",
                    args.len()
                ),
                span,
            ));
        }
        args.iter()
            .map(|arg| {
                let CallArg::Positional(expr) = arg else {
                    return Err(VerseError::check_at(
                        "parametric type arguments do not accept named arguments",
                        call_arg_expr(arg).span,
                    ));
                };
                let type_name = self.expr_to_type_name(expr)?;
                self.type_name_to_type_name(&type_name, expr.span)
            })
            .collect()
    }

    pub(super) fn ensure_type_arguments_satisfy_constraints(
        &mut self,
        params: &[TypeParam],
        args: &[Type],
        span: Span,
    ) -> Result<(), VerseError> {
        for (param, arg) in params.iter().zip(args) {
            self.ensure_type_arg_satisfies_constraint(&param.name, &param.constraint, arg, span)?;
        }
        Ok(())
    }

    pub(super) fn ensure_type_arg_satisfies_constraint(
        &mut self,
        param_name: &str,
        constraint: &TypeParamConstraint,
        actual: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        match constraint {
            TypeParamConstraint::Type => Ok(()),
            TypeParamConstraint::Subtype(expected_name) => {
                let expected = self.type_name_to_type_name(expected_name, span)?;
                if self.is_assignable(&expected, actual) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "type argument `{actual}` for `{param_name}` must be a subtype of `{expected}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    pub(super) fn ensure_inferred_type_param_constraints(
        &mut self,
        param_types: Option<&[Type]>,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(param_types) = param_types else {
            return Ok(());
        };
        let mut checked = Vec::new();
        for param_type in param_types {
            self.ensure_inferred_type_param_constraints_inner(
                param_type,
                inferred,
                span,
                &mut checked,
            )?;
        }
        Ok(())
    }

    pub(super) fn ensure_inferred_type_param_constraints_inner(
        &mut self,
        value_type: &Type,
        inferred: &HashMap<String, Type>,
        span: Span,
        checked: &mut Vec<String>,
    ) -> Result<(), VerseError> {
        match value_type {
            Type::Param(name, constraint) => {
                if !checked.iter().any(|checked_name| checked_name == name) {
                    if let Some(actual) = inferred.get(name) {
                        self.ensure_type_arg_satisfies_constraint(name, constraint, actual, span)?;
                    }
                    checked.push(name.clone());
                }
                Ok(())
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Task(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Signalable(item) => {
                self.ensure_inferred_type_param_constraints_inner(item, inferred, span, checked)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
                self.ensure_inferred_type_param_constraints_inner(key, inferred, span, checked)?;
                self.ensure_inferred_type_param_constraints_inner(value, inferred, span, checked)
            }
            Type::Tuple(items) => {
                for item in items {
                    self.ensure_inferred_type_param_constraints_inner(
                        item, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            Type::Event(payload)
            | Type::Generator(payload)
            | Type::Awaitable(payload)
            | Type::Subscribable(payload)
            | Type::Listenable(payload) => {
                if let Some(payload) = payload {
                    self.ensure_inferred_type_param_constraints_inner(
                        payload, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            Type::Function {
                param_types,
                param_specs,
                return_type,
                ..
            } => {
                if let Some(param_types) = param_types {
                    for param_type in param_types {
                        self.ensure_inferred_type_param_constraints_inner(
                            param_type, inferred, span, checked,
                        )?;
                    }
                }
                if let Some(param_specs) = param_specs {
                    for param in param_specs {
                        self.ensure_inferred_type_param_constraints_inner(
                            &param.value_type,
                            inferred,
                            span,
                            checked,
                        )?;
                    }
                }
                self.ensure_inferred_type_param_constraints_inner(
                    return_type,
                    inferred,
                    span,
                    checked,
                )
            }
            Type::Overload(overloads) => {
                for overload in overloads {
                    self.ensure_inferred_type_param_constraints_inner(
                        overload, inferred, span, checked,
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn substitute_type_params_runtime(
        &mut self,
        value_type: &Type,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<Type, VerseError> {
        let substituted = substitute_type_params(value_type, inferred);
        match substituted {
            Type::Struct(name) => Ok(Type::Struct(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::StructType(name) => Ok(Type::StructType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::Class(name) => Ok(Type::Class(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::ClassType(name) => Ok(Type::ClassType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::Interface(name) => Ok(Type::Interface(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::InterfaceType(name) => Ok(Type::InterfaceType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            other => Ok(other),
        }
    }

    pub(super) fn substitute_parametric_instance_name(
        &mut self,
        name: &str,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<String, VerseError> {
        let candidates = self
            .parametric_types
            .iter()
            .map(|(qualified, info)| (qualified.clone(), info.clone()))
            .collect::<Vec<_>>();
        for (qualified, info) in candidates {
            let generic_args = info
                .params
                .iter()
                .map(|param| Type::Param(param.name.clone(), param.constraint.clone()))
                .collect::<Vec<_>>();
            let generic_name = render_parametric_instance_type_name(&qualified, &generic_args);
            if generic_name != name {
                continue;
            }
            let mut actual_args = Vec::with_capacity(info.params.len());
            for param in &info.params {
                let Some(actual) = inferred.get(&param.name).cloned() else {
                    return Ok(name.to_string());
                };
                actual_args.push(actual);
            }
            let actual_name = render_parametric_instance_type_name(&qualified, &actual_args);
            self.instantiate_parametric_type(&qualified, &actual_args, span)?;
            return Ok(actual_name);
        }
        Ok(replace_type_param_atoms(name, inferred))
    }
}
