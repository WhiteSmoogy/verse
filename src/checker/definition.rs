use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, TypeAnnotation, TypeParam};
use crate::colors::NAMED_COLORS;
use crate::token::Span;

use super::semantic_function::merge_type_param_lists;
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
    pub(super) target: Option<TypeAnnotation>,
    pub(super) kind: ParametricTypeKind,
    pub(super) access: AccessLevel,
    pub(super) native: bool,
    pub(super) module_path: Vec<String>,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ModuleInfo {
    pub(super) members: HashMap<String, Type>,
    pub(super) member_access: HashMap<String, AccessLevel>,
    pub(super) member_scopes: HashMap<String, Vec<String>>,
    pub(super) access: AccessLevel,
    pub(super) scopes: Vec<String>,
    pub(super) imports: Vec<String>,
}

fn get_castable_final_super_class_type(from_type: bool) -> Type {
    let base_type = Type::Param("base_type".to_string(), TypeParamConstraint::Type);
    let second_param = if from_type {
        ("sub_type", Type::Subtype(Box::new(base_type.clone())))
    } else {
        ("Instance", base_type.clone())
    };
    native_function_type(
        &["reads", "decides"],
        vec![
            ("base_type", Type::TypeValueOf(Box::new(base_type.clone()))),
            second_param,
        ],
        Type::CastableSubtype(Box::new(base_type)),
    )
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
            "MakeClassifiableSubsetVar".to_string(),
            Symbol::immutable(Type::Function {
                arity: Some(1),
                arity_range: None,
                effects: vec!["reads".to_string()],
                param_types: Some(vec![Type::Array(Box::new(Type::Unknown))]),
                param_specs: None,
                return_type: Box::new(Type::ClassifiableSubsetVar(Box::new(Type::Unknown))),
            }),
        );
        globals.insert(
            "GetCastableFinalSuperClass".to_string(),
            Symbol::immutable(get_castable_final_super_class_type(false)),
        );
        globals.insert(
            "GetCastableFinalSuperClassFromType".to_string(),
            Symbol::immutable(get_castable_final_super_class_type(true)),
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
        for name in ["BitAnd", "BitOr", "BitXor"] {
            globals.insert(
                name.to_string(),
                Symbol::immutable(native_function_type(
                    &["computes"],
                    vec![("X", Type::Int), ("Y", Type::Int)],
                    Type::Int,
                )),
            );
        }
        globals.insert(
            "BitNot".to_string(),
            Symbol::immutable(native_function_type(
                &["computes"],
                vec![("X", Type::Int)],
                Type::Int,
            )),
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
                member_scopes: HashMap::new(),
                access: AccessLevel::Public,
                scopes: Vec::new(),
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
            parametric_type_instances: HashMap::new(),
            predeclared_aggregate_values: HashSet::new(),
            type_alias_defs: HashMap::new(),
            type_aliases: HashMap::new(),
            type_functions: HashMap::new(),
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
            package_name: None,
            recovering: false,
        }
    }

    pub fn with_package(mut self, package_name: Option<String>) -> Self {
        self.package_name = package_name;
        self
    }
}

pub(super) fn type_to_constraint_type_name(value_type: &Type) -> Option<TypeName> {
    match value_type {
        Type::Int => Some(TypeName::Int),
        Type::IntRange { .. } => None,
        Type::Float => Some(TypeName::Float),
        Type::FloatRange(range) => Some(TypeName::FloatRange(*range)),
        Type::Rational => Some(TypeName::Rational),
        Type::Number => Some(TypeName::Number),
        Type::Bool => Some(TypeName::Bool),
        Type::String => Some(TypeName::String),
        Type::Message => Some(TypeName::Message),
        Type::Char => Some(TypeName::Char),
        Type::Char8 => Some(TypeName::Char8),
        Type::Char32 => Some(TypeName::Char32),
        Type::None => Some(TypeName::None),
        Type::Any => Some(TypeName::Any),
        Type::Comparable => Some(TypeName::Comparable),
        Type::TypeValue => Some(TypeName::Type),
        Type::TypeValueOf(_) => Some(TypeName::Type),
        Type::TypeValueBounds { lower, upper } => Some(TypeName::TypeBounds {
            lower: Box::new(type_to_constraint_type_name(lower)?),
            upper: Box::new(type_to_constraint_type_name(upper)?),
        }),
        Type::Enum(name)
        | Type::Struct(name)
        | Type::Class(name)
        | Type::Interface(name)
        | Type::Module(name) => Some(TypeName::Named(name.clone())),
        Type::Param(name, _) => Some(TypeName::Named(name.clone())),
        Type::Array(item) => Some(TypeName::Array(Some(Box::new(
            type_to_constraint_type_name(item)?,
        )))),
        Type::Map(key, value) => Some(TypeName::Map(
            Box::new(type_to_constraint_type_name(key)?),
            Box::new(type_to_constraint_type_name(value)?),
        )),
        Type::WeakMap(key, value) => Some(TypeName::WeakMap(
            Box::new(type_to_constraint_type_name(key)?),
            Box::new(type_to_constraint_type_name(value)?),
        )),
        Type::Tuple(items) => Some(TypeName::Tuple(
            items
                .iter()
                .map(type_to_constraint_type_name)
                .collect::<Option<Vec<_>>>()?,
        )),
        Type::Option(item) => Some(TypeName::Option(Box::new(type_to_constraint_type_name(
            item,
        )?))),
        Type::Function {
            param_types,
            return_type,
            effects,
            ..
        } => {
            let params = param_types
                .as_ref()?
                .iter()
                .map(type_to_constraint_type_name)
                .collect::<Option<Vec<_>>>()?;
            Some(TypeName::FunctionSignature {
                params,
                effects: effects.clone(),
                return_type: Box::new(type_to_constraint_type_name(return_type)?),
            })
        }
        Type::Subtype(item) => Some(TypeName::Applied {
            name: "subtype".to_string(),
            args: vec![type_to_constraint_type_name(item)?],
        }),
        Type::CastableSubtype(item) => Some(TypeName::Applied {
            name: "castable_subtype".to_string(),
            args: vec![type_to_constraint_type_name(item)?],
        }),
        Type::ConcreteSubtype(item) => Some(TypeName::Applied {
            name: "concrete_subtype".to_string(),
            args: vec![type_to_constraint_type_name(item)?],
        }),
        Type::Unknown
        | Type::Never
        | Type::Range
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::InterfaceType(_)
        | Type::ParametricType { .. }
        | Type::Result(_, _)
        | Type::SuccessResult(_)
        | Type::ErrorResult(_)
        | Type::Event(_)
        | Type::SubscribableEvent(_)
        | Type::SubscribableEventIntrnl(_)
        | Type::StickyEvent(_)
        | Type::Task(_)
        | Type::Generator(_)
        | Type::ClassifiableSubset(_)
        | Type::ClassifiableSubsetKey(_)
        | Type::ClassifiableSubsetVar(_)
        | Type::Modifier(_)
        | Type::ModifierStack(_)
        | Type::Awaitable(_)
        | Type::Signalable(_)
        | Type::Subscribable(_)
        | Type::Listenable(_)
        | Type::Overload(_) => None,
    }
}

fn substitute_type_name_params(
    type_name: &TypeName,
    inferred: &HashMap<String, Type>,
) -> Option<TypeName> {
    match type_name {
        TypeName::Int => Some(TypeName::Int),
        TypeName::Float => Some(TypeName::Float),
        TypeName::Rational => Some(TypeName::Rational),
        TypeName::Number => Some(TypeName::Number),
        TypeName::Bool => Some(TypeName::Bool),
        TypeName::String => Some(TypeName::String),
        TypeName::Message => Some(TypeName::Message),
        TypeName::Char => Some(TypeName::Char),
        TypeName::Char8 => Some(TypeName::Char8),
        TypeName::Char32 => Some(TypeName::Char32),
        TypeName::None => Some(TypeName::None),
        TypeName::Any => Some(TypeName::Any),
        TypeName::Comparable => Some(TypeName::Comparable),
        TypeName::Type => Some(TypeName::Type),
        TypeName::TypeBounds { lower, upper } => Some(TypeName::TypeBounds {
            lower: Box::new(substitute_type_name_params(lower, inferred)?),
            upper: Box::new(substitute_type_name_params(upper, inferred)?),
        }),
        TypeName::IntRange { min, max } => Some(TypeName::IntRange {
            min: *min,
            max: *max,
        }),
        TypeName::FloatRange(range) => Some(TypeName::FloatRange(*range)),
        TypeName::Array(item) => Some(TypeName::Array(match item.as_ref() {
            Some(item) => Some(Box::new(substitute_type_name_params(item, inferred)?)),
            None => None,
        })),
        TypeName::Map(key, value) => Some(TypeName::Map(
            Box::new(substitute_type_name_params(key, inferred)?),
            Box::new(substitute_type_name_params(value, inferred)?),
        )),
        TypeName::WeakMap(key, value) => Some(TypeName::WeakMap(
            Box::new(substitute_type_name_params(key, inferred)?),
            Box::new(substitute_type_name_params(value, inferred)?),
        )),
        TypeName::Tuple(items) => Some(TypeName::Tuple(
            items
                .iter()
                .map(|item| substitute_type_name_params(item, inferred))
                .collect::<Option<Vec<_>>>()?,
        )),
        TypeName::Option(item) => Some(TypeName::Option(Box::new(substitute_type_name_params(
            item, inferred,
        )?))),
        TypeName::Function => Some(TypeName::Function),
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => Some(TypeName::FunctionSignature {
            params: params
                .iter()
                .map(|param| substitute_type_name_params(param, inferred))
                .collect::<Option<Vec<_>>>()?,
            effects: effects.clone(),
            return_type: Box::new(substitute_type_name_params(return_type, inferred)?),
        }),
        TypeName::Applied { name, args } => Some(TypeName::Applied {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_type_name_params(arg, inferred))
                .collect::<Option<Vec<_>>>()?,
        }),
        TypeName::Named(name) => inferred
            .get(name)
            .map(type_to_constraint_type_name)
            .unwrap_or_else(|| Some(TypeName::Named(name.clone()))),
    }
}

fn type_param_constraint_instance_supertype(value_type: Type) -> Type {
    match value_type {
        Type::Subtype(item) | Type::CastableSubtype(item) | Type::ConcreteSubtype(item) => {
            type_param_constraint_instance_supertype(*item)
        }
        other => other,
    }
}

fn type_value_param_used_as_type(
    name: &str,
    later_params: &[Param],
    return_type: Option<&TypeName>,
) -> bool {
    later_params.iter().any(|param| {
        param
            .annotation
            .as_ref()
            .is_some_and(|annotation| type_name_contains_name(&annotation.name, name))
            || match &param.pattern {
                ParamPattern::Tuple(items) => type_value_param_used_as_type(name, items, None),
                ParamPattern::Binding | ParamPattern::Anonymous => false,
            }
    }) || return_type.is_some_and(|return_type| type_name_contains_name(return_type, name))
}

fn type_name_contains_name(type_name: &TypeName, name: &str) -> bool {
    match type_name {
        TypeName::Named(candidate) => candidate == name,
        TypeName::Array(item) => item
            .as_deref()
            .is_some_and(|item| type_name_contains_name(item, name)),
        TypeName::Map(key, value) | TypeName::WeakMap(key, value) => {
            type_name_contains_name(key, name) || type_name_contains_name(value, name)
        }
        TypeName::Tuple(items) => items.iter().any(|item| type_name_contains_name(item, name)),
        TypeName::Option(item) => type_name_contains_name(item, name),
        TypeName::FunctionSignature {
            params,
            return_type,
            ..
        } => {
            params
                .iter()
                .any(|param| type_name_contains_name(param, name))
                || type_name_contains_name(return_type, name)
        }
        TypeName::TypeBounds { lower, upper } => {
            type_name_contains_name(lower, name) || type_name_contains_name(upper, name)
        }
        TypeName::Applied { args, .. } => args.iter().any(|arg| type_name_contains_name(arg, name)),
        TypeName::Int
        | TypeName::Float
        | TypeName::Rational
        | TypeName::Number
        | TypeName::Bool
        | TypeName::String
        | TypeName::Message
        | TypeName::Char
        | TypeName::Char8
        | TypeName::Char32
        | TypeName::None
        | TypeName::Any
        | TypeName::Comparable
        | TypeName::Type
        | TypeName::IntRange { .. }
        | TypeName::FloatRange(_)
        | TypeName::Function => false,
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
                        self.record_module_definition_access(name, specifiers, statement.span)?;
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
                StmtKind::TypeAlias {
                    name, specifiers, ..
                } => {
                    self.record_current_module_member_access(name, specifiers, statement.span)?;
                }
                StmtKind::ParametricTypeAlias {
                    name, specifiers, ..
                } => {
                    self.record_current_module_member_access(name, specifiers, statement.span)?;
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
                member_scopes: HashMap::new(),
                access: AccessLevel::Internal,
                scopes: Vec::new(),
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

    pub(super) fn predeclare_top_level_aggregate_names(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_aggregate_names(&program.statements)
    }

    pub(super) fn predeclare_aggregate_names(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let {
                name,
                specifiers,
                expr,
                ..
            } = &statement.kind
            else {
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

            let (kind, persistable, computes, constructor_access, constructor_scopes) =
                match &expr.kind {
                    ExprKind::StructDefinition {
                        persistable,
                        computes,
                        ..
                    } => (
                        AggregateKind::Struct,
                        *persistable,
                        *computes,
                        AccessLevel::Public,
                        Vec::new(),
                    ),
                    ExprKind::ClassDefinition { specifiers, .. } => {
                        let (constructor_access, constructor_scopes) =
                            class_constructor_access_from_specifiers(specifiers, statement.span)?;
                        (
                            AggregateKind::Class,
                            class_has_specifier(specifiers, "persistable"),
                            false,
                            constructor_access,
                            constructor_scopes,
                        )
                    }
                    ExprKind::ModuleDefinition { statements, .. } => {
                        self.module_path.push(name.clone());
                        let result = self.predeclare_aggregate_names(statements);
                        self.module_path.pop();
                        result?;
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
                native: field_has_specifier(specifiers, "native"),
                persistable,
                computes,
                constructor_effects: Vec::new(),
                constructor_access,
                constructor_scopes,
                fields: Vec::new(),
                methods: Vec::new(),
            });
        }
        Ok(())
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
                    specifiers,
                    params,
                    expr,
                } => {
                    self.validate_data_specifiers(name, specifiers, None, false, statement.span)?;
                    let access = access_level_from_specifiers(
                        specifiers,
                        "parametric type",
                        statement.span,
                    )?;
                    ensure_private_protected_access_only_in_classes(specifiers, statement.span)?;
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
                            target: None,
                            kind,
                            access,
                            native: field_has_specifier(specifiers, "native"),
                            module_path: self.module_path.clone(),
                            span: statement.span,
                        },
                    );
                }
                StmtKind::ParametricTypeAlias {
                    name,
                    specifiers,
                    params,
                    target,
                } => {
                    self.validate_data_specifiers(name, specifiers, None, false, statement.span)?;
                    let access = access_level_from_specifiers(
                        specifiers,
                        "parametric type",
                        statement.span,
                    )?;
                    ensure_private_protected_access_only_in_classes(specifiers, statement.span)?;
                    let qualified = self.current_qualified_name(name);
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
                            expr: Expr::new(ExprKind::External, target.span),
                            target: Some(target.clone()),
                            kind: ParametricTypeKind::Alias,
                            access,
                            native: field_has_specifier(specifiers, "native"),
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
        self.predeclare_type_aliases(&program.statements)
    }

    pub(super) fn resolve_predeclared_type_aliases(&mut self) -> Result<(), VerseError> {
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

            let StmtKind::TypeAlias {
                name,
                specifiers,
                target,
            } = &statement.kind
            else {
                continue;
            };

            self.validate_data_specifiers(name, specifiers, None, false, statement.span)?;
            access_level_from_specifiers(specifiers, "type alias", statement.span)?;
            ensure_private_protected_access_only_in_classes(specifiers, statement.span)?;
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
            TypeName::Type => Type::TypeValue,
            TypeName::TypeBounds { lower, upper } => Type::TypeValueBounds {
                lower: Box::new(self.resolve_type_alias_target(lower, span, visiting)?),
                upper: Box::new(self.resolve_type_alias_target(upper, span, visiting)?),
            },
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::FloatRange(range) => Type::FloatRange(*range),
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
                } else if let Some(target) = self.resolve_type_function_reference(name) {
                    self.instantiate_type_function(name, &target, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = builtin_numeric_alias_type(name) {
                    value_type
                } else if let Some(value_type) = self.resolve_type_param(name) {
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

    pub(super) fn validate_public_module_surface_access(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    specifiers,
                    annotation,
                    expr,
                } => {
                    let access = access_level_from_specifiers(
                        module_member_specifiers(specifiers, expr),
                        "module member",
                        statement.span,
                    )?;
                    let dependee = self.current_qualified_name(name);
                    if let Some(annotation) = annotation {
                        self.ensure_type_annotation_dependencies_accessible(
                            &dependee, access, annotation,
                        )?;
                    }
                    self.validate_public_expression_surface_access(&dependee, access, expr)?;
                }
                StmtKind::TypeAlias {
                    name,
                    specifiers,
                    target,
                } => {
                    let access =
                        access_level_from_specifiers(specifiers, "type alias", statement.span)?;
                    let dependee = self.current_qualified_name(name);
                    self.ensure_type_annotation_dependencies_accessible(&dependee, access, target)?;
                }
                StmtKind::ParametricType {
                    name,
                    specifiers,
                    params,
                    expr,
                } => {
                    let access = access_level_from_specifiers(
                        specifiers,
                        "parametric type",
                        statement.span,
                    )?;
                    let dependee = self.current_qualified_name(name);
                    let result = (|| {
                        self.push_type_param_scope(params.iter().map(|param| {
                            (
                                param.name.clone(),
                                Type::Param(param.name.clone(), param.constraint.clone()),
                            )
                        }));
                        for param in params {
                            self.ensure_type_param_constraint_dependencies_accessible(
                                &dependee,
                                access,
                                &param.constraint,
                                param.span,
                            )?;
                        }
                        self.validate_public_expression_surface_access(&dependee, access, expr)
                    })();
                    self.pop_type_param_scope();
                    result?;
                }
                StmtKind::ParametricTypeAlias {
                    name,
                    specifiers,
                    params,
                    target,
                } => {
                    let access = access_level_from_specifiers(
                        specifiers,
                        "parametric type",
                        statement.span,
                    )?;
                    let dependee = self.current_qualified_name(name);
                    let result = (|| {
                        self.push_type_param_scope(params.iter().map(|param| {
                            (
                                param.name.clone(),
                                Type::Param(param.name.clone(), param.constraint.clone()),
                            )
                        }));
                        for param in params {
                            self.ensure_type_param_constraint_dependencies_accessible(
                                &dependee,
                                access,
                                &param.constraint,
                                param.span,
                            )?;
                        }
                        self.ensure_type_annotation_dependencies_accessible(
                            &dependee, access, target,
                        )
                    })();
                    self.pop_type_param_scope();
                    result?;
                }
                StmtKind::ExtensionMethod(extension) => {
                    let access = access_level_from_specifiers(
                        &extension.method.effects,
                        "extension method",
                        extension.span,
                    )?;
                    let dependee = self.current_qualified_name(&extension.method.name);
                    self.ensure_param_dependencies_accessible(
                        &dependee,
                        access,
                        &extension.receiver,
                    )?;
                    self.ensure_function_signature_dependencies_accessible(
                        &dependee,
                        access,
                        &extension.method.params,
                        extension.method.return_type.as_ref(),
                        extension.span,
                    )?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn validate_public_expression_surface_access(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        expr: &Expr,
    ) -> Result<(), VerseError> {
        match &expr.kind {
            ExprKind::Function {
                params,
                return_type,
                body,
                ..
            } => {
                self.ensure_function_signature_dependencies_accessible(
                    dependee,
                    access,
                    params,
                    return_type.as_ref(),
                    expr.span,
                )?;
                self.ensure_type_function_target_dependencies_accessible(
                    dependee,
                    access,
                    params,
                    return_type.as_ref(),
                    body,
                )
            }
            ExprKind::StructDefinition { fields, .. } => {
                if !access_requires_dependency_validation(access) {
                    return Ok(());
                }
                for field in fields {
                    let field_name = format!("{dependee}.{}", field.name);
                    if let Some(annotation) = field.annotation.as_ref() {
                        self.ensure_type_annotation_dependencies_accessible(
                            &field_name,
                            access,
                            annotation,
                        )?;
                    }
                }
                Ok(())
            }
            ExprKind::ClassDefinition {
                base,
                interfaces,
                fields,
                methods,
                extension_methods,
                ..
            } => {
                self.ensure_class_parent_dependencies_accessible(
                    dependee, access, base, interfaces,
                )?;
                if !access_requires_dependency_validation(access) {
                    return Ok(());
                }
                for field in fields {
                    let member_access =
                        access_level_from_specifiers(&field.specifiers, "class field", field.span)?;
                    if access_requires_dependency_validation(member_access)
                        && let Some(annotation) = field.annotation.as_ref()
                    {
                        let field_name = format!("{dependee}.{}", field.name);
                        self.ensure_type_annotation_dependencies_accessible(
                            &field_name,
                            member_access,
                            annotation,
                        )?;
                    }
                }
                for method in methods {
                    let member_access =
                        access_level_from_specifiers(&method.effects, "class method", method.span)?;
                    if access_requires_dependency_validation(member_access) {
                        let method_name = format!("{dependee}.{}", method.name);
                        self.ensure_function_signature_dependencies_accessible(
                            &method_name,
                            member_access,
                            &method.params,
                            method.return_type.as_ref(),
                            method.span,
                        )?;
                    }
                }
                for extension in extension_methods {
                    let member_access = access_level_from_specifiers(
                        &extension.method.effects,
                        "extension method",
                        extension.span,
                    )?;
                    if access_requires_dependency_validation(member_access) {
                        let method_name = format!("{dependee}.{}", extension.method.name);
                        self.ensure_param_dependencies_accessible(
                            &method_name,
                            member_access,
                            &extension.receiver,
                        )?;
                        self.ensure_function_signature_dependencies_accessible(
                            &method_name,
                            member_access,
                            &extension.method.params,
                            extension.method.return_type.as_ref(),
                            extension.span,
                        )?;
                    }
                }
                Ok(())
            }
            ExprKind::InterfaceDefinition {
                parents,
                fields,
                methods,
                ..
            } => {
                self.ensure_class_parent_dependencies_accessible(dependee, access, &None, parents)?;
                if !access_requires_dependency_validation(access) {
                    return Ok(());
                }
                for field in fields {
                    let member_access = access_level_from_specifiers(
                        &field.specifiers,
                        "interface field",
                        field.span,
                    )?;
                    if access_requires_dependency_validation(member_access)
                        && let Some(annotation) = field.annotation.as_ref()
                    {
                        let field_name = format!("{dependee}.{}", field.name);
                        self.ensure_type_annotation_dependencies_accessible(
                            &field_name,
                            member_access,
                            annotation,
                        )?;
                    }
                }
                for method in methods {
                    let member_access = access_level_from_specifiers(
                        &method.effects,
                        "interface method",
                        method.span,
                    )?;
                    if access_requires_dependency_validation(member_access) {
                        let method_name = format!("{dependee}.{}", method.name);
                        self.ensure_function_signature_dependencies_accessible(
                            &method_name,
                            member_access,
                            &method.params,
                            method.return_type.as_ref(),
                            method.span,
                        )?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn ensure_type_function_target_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        params: &[Param],
        return_type: Option<&TypeAnnotation>,
        body: &Expr,
    ) -> Result<(), VerseError> {
        if !access_requires_dependency_validation(access) {
            return Ok(());
        }
        let Some((type_params, inferred_type_params)) = self.type_function_params(params)? else {
            return Ok(());
        };
        let Some(return_type) = return_type else {
            return Ok(());
        };

        let all_type_params = merge_type_param_lists(&inferred_type_params, &type_params)?;
        self.push_type_param_scope(all_type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            let return_value_type = self.annotation_to_type(Some(return_type))?;
            if !type_can_be_used_as_type_value(&return_value_type) {
                return Ok(());
            }
            let Ok(target) = self.expr_to_type_name(body) else {
                return Ok(());
            };
            self.ensure_type_name_dependencies_accessible(dependee, access, &target, body.span)?;
            let target_type = self.type_name_to_type_name(&target, body.span)?;
            self.ensure_type_dependencies_accessible(dependee, access, &target_type, body.span)
        })();
        self.pop_type_param_scope();
        result
    }

    fn ensure_class_parent_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        base: &Option<TypeAnnotation>,
        interfaces: &[TypeAnnotation],
    ) -> Result<(), VerseError> {
        if !access_requires_dependency_validation(access) {
            return Ok(());
        }
        if let Some(base) = base {
            self.ensure_type_annotation_dependencies_accessible(dependee, access, base)?;
        }
        for interface in interfaces {
            self.ensure_type_annotation_dependencies_accessible(dependee, access, interface)?;
        }
        Ok(())
    }

    fn ensure_function_signature_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        params: &[Param],
        return_type: Option<&TypeAnnotation>,
        span: Span,
    ) -> Result<(), VerseError> {
        if !access_requires_dependency_validation(access) {
            return Ok(());
        }
        let type_params = collect_function_type_params(params)?;
        self.push_type_param_scope(type_params.iter().map(|param| {
            (
                param.name.clone(),
                Type::Param(param.name.clone(), param.constraint.clone()),
            )
        }));
        let result = (|| {
            self.dependent_type_value_param_signature(params, return_type)?;
            for type_param in &type_params {
                self.ensure_type_param_constraint_dependencies_accessible(
                    dependee,
                    access,
                    &type_param.constraint,
                    type_param.span,
                )?;
            }
            for param in params {
                self.ensure_param_dependencies_accessible(dependee, access, param)?;
            }
            if let Some(return_type) = return_type {
                self.ensure_type_annotation_dependencies_accessible(dependee, access, return_type)?;
            }
            Ok(())
        })();
        self.pop_type_param_scope();
        result.map_err(|error: VerseError| {
            if error.diagnostic().span.is_some() {
                error
            } else {
                VerseError::check_at(error.to_string(), span)
            }
        })
    }

    fn ensure_param_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        param: &Param,
    ) -> Result<(), VerseError> {
        if let Some(annotation) = param.annotation.as_ref() {
            self.ensure_type_annotation_dependencies_accessible(dependee, access, annotation)?;
        }
        if let ParamPattern::Tuple(items) = &param.pattern {
            for item in items {
                self.ensure_param_dependencies_accessible(dependee, access, item)?;
            }
        }
        Ok(())
    }

    fn ensure_type_param_constraint_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        constraint: &TypeParamConstraint,
        span: Span,
    ) -> Result<(), VerseError> {
        match constraint {
            TypeParamConstraint::Type => Ok(()),
            TypeParamConstraint::Subtype(parent) => {
                self.ensure_type_name_dependencies_accessible(dependee, access, parent, span)
            }
            TypeParamConstraint::TypeBounds { lower, upper } => {
                self.ensure_type_name_dependencies_accessible(dependee, access, lower, span)?;
                self.ensure_type_name_dependencies_accessible(dependee, access, upper, span)
            }
        }
    }

    fn ensure_type_annotation_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        annotation: &TypeAnnotation,
    ) -> Result<(), VerseError> {
        if !access_requires_dependency_validation(access) {
            return Ok(());
        }
        self.ensure_type_name_dependencies_accessible(
            dependee,
            access,
            &annotation.name,
            annotation.span,
        )?;
        let value_type = self.type_name_to_type(annotation)?;
        self.ensure_type_dependencies_accessible(dependee, access, &value_type, annotation.span)
    }

    fn ensure_type_name_dependencies_accessible(
        &mut self,
        dependee: &str,
        access: AccessLevel,
        name: &TypeName,
        span: Span,
    ) -> Result<(), VerseError> {
        match name {
            TypeName::Array(item) => {
                if let Some(item) = item.as_deref() {
                    self.ensure_type_name_dependencies_accessible(dependee, access, item, span)?;
                }
            }
            TypeName::Map(key, value) | TypeName::WeakMap(key, value) => {
                self.ensure_type_name_dependencies_accessible(dependee, access, key, span)?;
                self.ensure_type_name_dependencies_accessible(dependee, access, value, span)?;
            }
            TypeName::Tuple(items) => {
                for item in items {
                    self.ensure_type_name_dependencies_accessible(dependee, access, item, span)?;
                }
            }
            TypeName::Option(item) => {
                self.ensure_type_name_dependencies_accessible(dependee, access, item, span)?;
            }
            TypeName::FunctionSignature {
                params,
                return_type,
                ..
            } => {
                for param in params {
                    self.ensure_type_name_dependencies_accessible(dependee, access, param, span)?;
                }
                self.ensure_type_name_dependencies_accessible(dependee, access, return_type, span)?;
            }
            TypeName::TypeBounds { lower, upper } => {
                self.ensure_type_name_dependencies_accessible(dependee, access, lower, span)?;
                self.ensure_type_name_dependencies_accessible(dependee, access, upper, span)?;
            }
            TypeName::Applied { name, args } => {
                if !is_official_parametric_type_name(name)
                    && let Some(qualified) = self.resolve_parametric_type_reference(name)
                {
                    self.ensure_named_type_dependency_accessible(
                        dependee, access, &qualified, span,
                    )?;
                }
                for arg in args {
                    self.ensure_type_name_dependencies_accessible(dependee, access, arg, span)?;
                }
            }
            TypeName::Named(name) => {
                if self.resolve_type_alias_dependency_name(name).is_none()
                    && let Some(qualified) = self.resolve_type_dependency_name(name)
                {
                    self.ensure_named_type_dependency_accessible(
                        dependee, access, &qualified, span,
                    )?;
                }
            }
            TypeName::Int
            | TypeName::Float
            | TypeName::Rational
            | TypeName::Number
            | TypeName::Bool
            | TypeName::String
            | TypeName::Message
            | TypeName::Char
            | TypeName::Char8
            | TypeName::Char32
            | TypeName::None
            | TypeName::Any
            | TypeName::Comparable
            | TypeName::Type
            | TypeName::IntRange { .. }
            | TypeName::FloatRange(_)
            | TypeName::Function => {}
        }
        Ok(())
    }

    pub(super) fn ensure_type_dependencies_accessible(
        &self,
        dependee: &str,
        access: AccessLevel,
        value_type: &Type,
        span: Span,
    ) -> Result<(), VerseError> {
        match value_type {
            Type::Enum(name)
            | Type::EnumType(name)
            | Type::Struct(name)
            | Type::StructType(name)
            | Type::Class(name)
            | Type::ClassType(name)
            | Type::Interface(name)
            | Type::InterfaceType(name) => {
                self.ensure_named_type_dependency_accessible(dependee, access, name, span)?;
            }
            Type::ParametricType { name, .. } => {
                self.ensure_named_type_dependency_accessible(dependee, access, name, span)?;
            }
            Type::Param(_, TypeParamConstraint::Subtype(parent)) => {
                if let Some(parent_type) = self.type_name_to_type_name_for_assignability(parent) {
                    self.ensure_type_dependencies_accessible(dependee, access, &parent_type, span)?;
                }
            }
            Type::Param(_, TypeParamConstraint::TypeBounds { lower, upper }) => {
                if let Some(lower_type) = self.type_name_to_type_name_for_assignability(lower) {
                    self.ensure_type_dependencies_accessible(dependee, access, &lower_type, span)?;
                }
                if let Some(upper_type) = self.type_name_to_type_name_for_assignability(upper) {
                    self.ensure_type_dependencies_accessible(dependee, access, &upper_type, span)?;
                }
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Event(Some(item))
            | Type::SubscribableEvent(item)
            | Type::SubscribableEventIntrnl(Some(item))
            | Type::StickyEvent(Some(item))
            | Type::Task(item)
            | Type::Generator(Some(item))
            | Type::TypeValueOf(item)
            | Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::ClassifiableSubsetKey(item)
            | Type::ClassifiableSubsetVar(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Awaitable(Some(item))
            | Type::Signalable(item)
            | Type::Subscribable(Some(item))
            | Type::Listenable(Some(item)) => {
                self.ensure_type_dependencies_accessible(dependee, access, item, span)?;
            }
            Type::TypeValueBounds { lower, upper } => {
                self.ensure_type_dependencies_accessible(dependee, access, lower, span)?;
                self.ensure_type_dependencies_accessible(dependee, access, upper, span)?;
            }
            Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
                self.ensure_type_dependencies_accessible(dependee, access, key, span)?;
                self.ensure_type_dependencies_accessible(dependee, access, value, span)?;
            }
            Type::SuccessResult(item) | Type::ErrorResult(item) => {
                self.ensure_type_dependencies_accessible(dependee, access, item, span)?;
            }
            Type::Tuple(items) => {
                for item in items {
                    self.ensure_type_dependencies_accessible(dependee, access, item, span)?;
                }
            }
            Type::Function {
                param_types,
                param_specs,
                return_type,
                ..
            } => {
                if let Some(param_types) = param_types {
                    for param_type in param_types {
                        self.ensure_type_dependencies_accessible(
                            dependee, access, param_type, span,
                        )?;
                    }
                }
                if let Some(param_specs) = param_specs {
                    for param_spec in param_specs {
                        self.ensure_param_spec_dependencies_accessible(
                            dependee, access, param_spec, span,
                        )?;
                    }
                }
                self.ensure_type_dependencies_accessible(dependee, access, return_type, span)?;
            }
            Type::Overload(overloads) => {
                for overload in overloads {
                    self.ensure_type_dependencies_accessible(dependee, access, overload, span)?;
                }
            }
            Type::Param(_, TypeParamConstraint::Type)
            | Type::Int
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
            | Type::None
            | Type::Any
            | Type::Comparable
            | Type::TypeValue
            | Type::Unknown
            | Type::Never
            | Type::Range
            | Type::Module(_)
            | Type::Event(None)
            | Type::SubscribableEventIntrnl(None)
            | Type::StickyEvent(None)
            | Type::Generator(None)
            | Type::Awaitable(None)
            | Type::Subscribable(None)
            | Type::Listenable(None) => {}
        }
        Ok(())
    }

    fn ensure_param_spec_dependencies_accessible(
        &self,
        dependee: &str,
        access: AccessLevel,
        param_spec: &ParamSpec,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_type_dependencies_accessible(dependee, access, &param_spec.value_type, span)?;
        if let Some(items) = &param_spec.tuple_items {
            for item in items {
                self.ensure_param_spec_dependencies_accessible(dependee, access, item, span)?;
            }
        }
        Ok(())
    }

    fn resolve_type_dependency_name(&self, name: &str) -> Option<String> {
        if self.resolve_type_param(name).is_some() {
            return None;
        }
        if self.enum_types.contains_key(name)
            || self.struct_types.contains_key(name)
            || self.interface_types.contains_key(name)
            || self.parametric_types.contains_key(name)
        {
            return Some(name.to_string());
        }
        if !name.contains('.') {
            return self.resolve_contextual_type_name(name);
        }
        None
    }

    fn resolve_type_alias_dependency_name(&self, name: &str) -> Option<String> {
        if self.type_alias_defs.contains_key(name) {
            return Some(name.to_string());
        }
        if !name.contains('.') {
            return self
                .resolve_contextual_type_name(name)
                .filter(|qualified| self.type_alias_defs.contains_key(qualified));
        }
        None
    }

    fn ensure_named_type_dependency_accessible(
        &self,
        dependee: &str,
        dependee_access: AccessLevel,
        dependency: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let dependency = dependency_base_name(dependency);
        let dependee_access = self.definition_effective_access(dependee, dependee_access);
        let Some(dependency_access) = self.module_member_dependency_access(dependency) else {
            return Ok(());
        };
        if access_is_more_visible_than(dependee_access, dependency_access) {
            return Err(VerseError::check_at(
                format!(
                    "definition `{dependee}` is {} but depends on `{dependency}`, which is {}",
                    access_level_name(dependee_access),
                    access_level_name(dependency_access)
                ),
                span,
            ));
        }
        Ok(())
    }

    fn module_member_dependency_access(&self, name: &str) -> Option<AccessLevel> {
        let (module_name, member_name) = name.rsplit_once('.')?;
        let module_info = self.module_types.get(module_name)?;
        let access = module_info
            .member_access
            .get(member_name)
            .copied()
            .unwrap_or(AccessLevel::Internal);
        Some(self.definition_effective_access(name, access))
    }

    fn definition_effective_access(&self, name: &str, declared: AccessLevel) -> AccessLevel {
        let Some(module_name) = self.containing_module_name(name) else {
            return declared;
        };
        self.access_constrained_by_enclosing_modules(declared, module_name)
    }

    fn containing_module_name<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        let mut current = aggregate_module_name(name);
        while let Some(candidate) = current {
            if self.module_types.contains_key(candidate) {
                return Some(candidate);
            }
            current = candidate.rsplit_once('.').map(|(parent, _)| parent);
        }
        None
    }

    fn access_constrained_by_enclosing_modules(
        &self,
        mut access: AccessLevel,
        module_name: &str,
    ) -> AccessLevel {
        let mut current = Some(module_name);
        while let Some(name) = current {
            if let Some(info) = self.module_types.get(name)
                && self.module_access_constrains_surface(name, info.access)
                && access_is_more_visible_than(access, info.access)
            {
                access = info.access;
            }
            current = name.rsplit_once('.').map(|(parent, _)| parent);
        }
        access
    }

    fn module_access_constrains_surface(&self, module_name: &str, access: AccessLevel) -> bool {
        match access {
            AccessLevel::Scoped => true,
            AccessLevel::Internal => module_name.contains('.'),
            AccessLevel::Public | AccessLevel::Protected | AccessLevel::Private => false,
        }
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

    pub(super) fn module_member_scoped_accessible(
        &self,
        module_name: &str,
        member_name: &str,
    ) -> bool {
        self.module_types
            .get(module_name)
            .and_then(|module| module.member_scopes.get(member_name))
            .is_some_and(|scopes| self.scoped_accessible(scopes))
    }

    pub(super) fn scoped_accessible(&self, scopes: &[String]) -> bool {
        let package_name = self.package_name.as_deref();
        let current_module = self.current_module_name();
        let current_absolute_module = self.current_absolute_module_name();
        scopes.iter().any(|scope| {
            package_name.is_some_and(|package| scoped_scope_contains(scope, package))
                || current_module
                    .as_deref()
                    .is_some_and(|module| scoped_scope_contains(scope, module))
                || current_absolute_module
                    .as_deref()
                    .is_some_and(|module| scoped_scope_contains(scope, module))
        })
    }

    pub(super) fn current_module_is_same_or_child_of(&self, module_name: &str) -> bool {
        self.current_module_name()
            .as_deref()
            .is_some_and(|current| scoped_scope_contains(module_name, current))
    }

    fn current_absolute_module_name(&self) -> Option<String> {
        let package = self.package_name.as_deref()?;
        if !package.starts_with('/') {
            return None;
        }
        let package = package.trim_end_matches('/');
        let module = self.current_module_name()?.replace('.', "/");
        if package.is_empty() {
            Some(format!("/{module}"))
        } else {
            Some(format!("{package}/{module}"))
        }
    }

    pub(super) fn module_or_parent_scoped_accessible(&self, module_name: &str) -> bool {
        let mut current = Some(module_name);
        while let Some(name) = current {
            if self.module_types.get(name).is_some_and(|info| {
                info.access == AccessLevel::Scoped && self.scoped_accessible(&info.scopes)
            }) {
                return true;
            }
            current = name.rsplit_once('.').map(|(parent, _)| parent);
        }
        false
    }

    pub(super) fn inaccessible_scoped_enclosing_module(&self, module_name: &str) -> Option<String> {
        let mut current = Some(module_name);
        while let Some(name) = current {
            if self.module_types.get(name).is_some_and(|info| {
                info.access == AccessLevel::Scoped
                    && !self.current_module_is_same_or_child_of(name)
                    && !self.module_or_parent_scoped_accessible(name)
            }) {
                return Some(name.to_string());
            }
            current = name.rsplit_once('.').map(|(parent, _)| parent);
        }
        None
    }

    pub(super) fn inaccessible_internal_enclosing_module(
        &self,
        module_name: &str,
    ) -> Option<String> {
        let mut current = Some(module_name);
        while let Some(name) = current {
            if let Some((parent, _)) = name.rsplit_once('.')
                && self.module_types.get(name).is_some_and(|info| {
                    info.access == AccessLevel::Internal
                        && !self.current_module_is_same_or_child_of(parent)
                        && !self.module_or_parent_scoped_accessible(parent)
                })
            {
                return Some(name.to_string());
            }
            current = name.rsplit_once('.').map(|(parent, _)| parent);
        }
        None
    }

    pub(super) fn aggregate_or_parent_scoped_accessible(&self, aggregate_name: &str) -> bool {
        let Some(module_name) = aggregate_module_name(aggregate_name) else {
            return false;
        };
        let member_name = aggregate_unqualified_name(aggregate_name);
        if self.module_types.get(module_name).is_some_and(|module| {
            module.member_access.get(member_name) == Some(&AccessLevel::Scoped)
                && module
                    .member_scopes
                    .get(member_name)
                    .is_some_and(|scopes| self.scoped_accessible(scopes))
        }) {
            return true;
        }
        self.module_or_parent_scoped_accessible(module_name)
    }

    pub(super) fn inaccessible_scoped_enclosing_aggregate(
        &self,
        aggregate_name: &str,
    ) -> Option<String> {
        let module_name = aggregate_module_name(aggregate_name)?;
        if let Some(inaccessible) = self.inaccessible_scoped_enclosing_module(module_name) {
            return Some(inaccessible);
        }

        let member_name = aggregate_unqualified_name(aggregate_name);
        if self.module_types.get(module_name).is_some_and(|module| {
            module.member_access.get(member_name) == Some(&AccessLevel::Scoped)
                && !self.current_module_is_same_or_child_of(module_name)
                && !self.aggregate_or_parent_scoped_accessible(aggregate_name)
        }) {
            return Some(aggregate_name.to_string());
        }

        None
    }

    pub(super) fn inaccessible_internal_enclosing_aggregate(
        &self,
        aggregate_name: &str,
    ) -> Option<String> {
        let module_name = aggregate_module_name(aggregate_name)?;
        self.inaccessible_internal_enclosing_module(module_name)
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

    pub(super) fn resolve_type_function_reference(&self, name: &str) -> Option<String> {
        if self.type_functions.contains_key(name) {
            return Some(name.to_string());
        }
        if name.contains('.') {
            return None;
        }

        if !self.module_path.is_empty() {
            let qualified = self.current_qualified_name(name);
            if self.type_functions.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        if let Some(module_name) = self.current_module_name()
            && let Some(module_info) = self.module_types.get(&module_name)
            && let Some(qualified) = module_info.imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                self.type_functions
                    .contains_key(&qualified)
                    .then_some(qualified)
            })
        {
            return Some(qualified);
        }

        self.scope_imports.iter().rev().find_map(|imports| {
            imports.iter().find_map(|module_name| {
                let qualified = format!("{module_name}.{name}");
                self.type_functions
                    .contains_key(&qualified)
                    .then_some(qualified)
            })
        })
    }

    fn ensure_type_function_reference_accessible(
        &self,
        qualified: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some((module_name, member_name)) = qualified.rsplit_once('.') else {
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

    pub(super) fn instantiate_type_function(
        &mut self,
        display_name: &str,
        qualified: &str,
        args: &[Type],
        span: Span,
    ) -> Result<Type, VerseError> {
        self.ensure_type_function_reference_accessible(qualified, span)?;
        let Some(infos) = self.type_functions.get(qualified).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown type function `{display_name}`"),
                span,
            ));
        };
        let (info, inferred) =
            self.select_type_function_candidate(display_name, &infos, args, span)?;
        self.push_type_param_scope(inferred);
        let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
        let result = self.type_name_to_type_name(&info.target, span);
        self.module_path = previous_module_path;
        self.pop_type_param_scope();
        result
    }

    pub(super) fn instantiate_type_function_target_name(
        &mut self,
        display_name: &str,
        qualified: &str,
        args: &[Type],
        span: Span,
    ) -> Result<Option<TypeName>, VerseError> {
        self.ensure_type_function_reference_accessible(qualified, span)?;
        let Some(infos) = self.type_functions.get(qualified).cloned() else {
            return Err(VerseError::check_at(
                format!("unknown type function `{display_name}`"),
                span,
            ));
        };
        let (info, inferred) =
            self.select_type_function_candidate(display_name, &infos, args, span)?;
        Ok(substitute_type_name_params(&info.target, &inferred))
    }

    fn with_type_function_module_path<T>(
        &mut self,
        info: &TypeFunctionInfo,
        action: impl FnOnce(&mut Self) -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        let previous_module_path =
            std::mem::replace(&mut self.module_path, info.module_path.clone());
        let result = action(self);
        self.module_path = previous_module_path;
        result
    }

    pub(super) fn select_type_function_candidate(
        &mut self,
        display_name: &str,
        infos: &[TypeFunctionInfo],
        args: &[Type],
        span: Span,
    ) -> Result<(TypeFunctionInfo, HashMap<String, Type>), VerseError> {
        if let [info] = infos {
            let info = info.clone();
            if info.params.len() != args.len() {
                return Err(VerseError::check_at(
                    format!(
                        "type function `{display_name}` expected {} type arguments, got {}",
                        info.params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            let inferred = self.with_type_function_module_path(&info, |checker| {
                checker.infer_type_function_type_params(display_name, &info, args, span)
            })?;
            return Ok((info, inferred));
        }

        let mut matches = Vec::new();
        for info in infos.iter().filter(|info| info.params.len() == args.len()) {
            let mut checker = self.clone();
            if let Ok((_inferred, score)) =
                checker.with_type_function_module_path(info, |checker| {
                    let inferred =
                        checker.infer_type_function_type_params(display_name, info, args, span)?;
                    let score =
                        checker.type_function_candidate_match_score(info, args, &inferred, span)?;
                    Ok((inferred, score))
                })
            {
                matches.push((score, info.clone()));
            }
        }

        let info = match matches.as_slice() {
            [] => {
                return Err(VerseError::check_at(
                    format!(
                        "no overload of type function `{display_name}` matches type arguments ({})",
                        render_type_list(args)
                    ),
                    span,
                ));
            }
            matches => {
                let best_score = matches
                    .iter()
                    .map(|(score, _)| *score)
                    .min()
                    .expect("non-empty matches");
                let mut best = matches
                    .iter()
                    .filter(|(score, _)| *score == best_score)
                    .map(|(_, info)| info.clone())
                    .collect::<Vec<_>>();
                if best.len() == 1 {
                    best.pop().expect("one best match")
                } else {
                    return Err(VerseError::check_at(
                        format!(
                            "type function `{display_name}` overload is ambiguous for type arguments ({})",
                            render_type_list(args)
                        ),
                        span,
                    ));
                }
            }
        };

        let inferred = self.with_type_function_module_path(&info, |checker| {
            checker.infer_type_function_type_params(display_name, &info, args, span)
        })?;
        Ok((info, inferred))
    }

    fn type_function_candidate_match_score(
        &mut self,
        info: &TypeFunctionInfo,
        args: &[Type],
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<usize, VerseError> {
        let mut score = 0usize;
        for (param, actual) in info.params.iter().zip(args) {
            score += self.type_function_constraint_match_score(
                &param.constraint,
                actual,
                Some(inferred),
                span,
            )?;
        }
        for param in &info.inferred_params {
            let Some(actual) = inferred.get(&param.name) else {
                continue;
            };
            score += self.type_function_constraint_match_score(
                &param.constraint,
                actual,
                Some(inferred),
                span,
            )?;
        }
        Ok(score)
    }

    fn type_function_constraint_match_score(
        &mut self,
        constraint: &TypeParamConstraint,
        actual: &Type,
        inferred: Option<&HashMap<String, Type>>,
        span: Span,
    ) -> Result<usize, VerseError> {
        match constraint {
            TypeParamConstraint::Type => Ok(1_000),
            TypeParamConstraint::Subtype(expected_name) => {
                let expected =
                    self.type_name_to_type_name_for_constraint(expected_name, inferred, span)?;
                Ok(self
                    .type_function_type_argument_match_score(&expected, actual)
                    .unwrap_or(500))
            }
            TypeParamConstraint::TypeBounds { lower, upper } => {
                let upper = self.type_name_to_type_name_for_constraint(upper, inferred, span)?;
                let lower = self.type_name_to_type_name_for_constraint(lower, inferred, span)?;
                let upper_score = self
                    .type_function_type_argument_match_score(&upper, actual)
                    .unwrap_or(500);
                let lower_score = self
                    .type_function_type_argument_match_score(actual, &lower)
                    .unwrap_or(500);
                Ok(upper_score + lower_score + 10)
            }
        }
    }

    fn type_function_type_argument_match_score(
        &self,
        expected: &Type,
        actual: &Type,
    ) -> Option<usize> {
        if expected == actual {
            return Some(0);
        }

        match (expected, actual) {
            (Type::Class(expected), Type::Class(actual)) => {
                self.class_subtype_distance(actual, expected)
            }
            (Type::Interface(expected), Type::Interface(actual)) => {
                self.interface_subtype_distance(actual, expected)
            }
            (Type::Interface(expected), Type::Class(actual)) => {
                self.class_interface_distance(actual, expected)
            }
            (Type::CastableSubtype(expected), Type::Class(actual))
                if self.class_type_value_is_castable_subtype(actual, expected) =>
            {
                let actual = Type::Class(actual.clone());
                self.type_function_type_argument_match_score(expected, &actual)
                    .map(|score| score + 1)
                    .or(Some(100))
            }
            (Type::ConcreteSubtype(expected), Type::Class(actual))
                if self.class_type_value_is_concrete_subtype(actual, expected) =>
            {
                let actual = Type::Class(actual.clone());
                self.type_function_type_argument_match_score(expected, &actual)
                    .map(|score| score + 1)
                    .or(Some(100))
            }
            _ if self.type_argument_satisfies_constraint(expected, actual) => Some(100),
            _ => None,
        }
    }

    fn class_subtype_distance(&self, actual: &str, expected: &str) -> Option<usize> {
        if actual == expected {
            return Some(0);
        }

        let builtin_distance = match (actual, expected) {
            ("player", "agent") | ("agent", "entity") => Some(1),
            ("player", "entity") => Some(2),
            _ => None,
        };
        if builtin_distance.is_some() {
            return builtin_distance;
        }

        let mut distance = 0usize;
        let mut current = Some(actual);
        while let Some(name) = current {
            if name == expected {
                return Some(distance);
            }
            distance += 1;
            current = self
                .struct_types
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        None
    }

    fn interface_subtype_distance(&self, actual: &str, expected: &str) -> Option<usize> {
        if actual == expected {
            return Some(0);
        }

        let info = self.interface_types.get(actual)?;
        info.parents
            .iter()
            .filter_map(|parent| {
                self.interface_subtype_distance(parent, expected)
                    .map(|distance| distance + 1)
            })
            .min()
    }

    fn class_interface_distance(&self, actual: &str, expected: &str) -> Option<usize> {
        let mut best = None;
        let mut class_distance = 0usize;
        let mut current = Some(actual);

        while let Some(name) = current {
            let info = self.struct_types.get(name)?;
            for interface in &info.interfaces {
                if let Some(interface_distance) =
                    self.interface_subtype_distance(interface, expected)
                {
                    let distance = class_distance + interface_distance;
                    best = Some(best.map_or(distance, |best: usize| best.min(distance)));
                }
            }
            class_distance += 1;
            current = info.base.as_deref();
        }

        best
    }

    fn infer_type_function_type_params(
        &mut self,
        display_name: &str,
        info: &TypeFunctionInfo,
        args: &[Type],
        span: Span,
    ) -> Result<HashMap<String, Type>, VerseError> {
        let mut inferred = info
            .inferred_params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    Type::Param(param.name.clone(), param.constraint.clone()),
                )
            })
            .chain(
                info.params
                    .iter()
                    .zip(args.iter())
                    .map(|(param, arg)| (param.name.clone(), arg.clone())),
            )
            .collect::<HashMap<_, _>>();
        for (param, arg) in info.params.iter().zip(args) {
            self.infer_type_params_from_constraint(&param.constraint, arg, &mut inferred)
                .ok_or_else(|| {
                    VerseError::check_at(
                        format!(
                            "could not infer type parameters for type function `{display_name}`"
                        ),
                        span,
                    )
                })?;
        }
        for param in &info.inferred_params {
            if inferred
                .get(&param.name)
                .is_none_or(|actual| unresolved_type_function_inferred_param(actual, &param.name))
            {
                return Err(VerseError::check_at(
                    format!(
                        "could not infer type parameter `{}` for type function `{display_name}`",
                        param.name
                    ),
                    span,
                ));
            }
        }
        for param in info.inferred_params.iter().chain(&info.params) {
            let Some(actual) = inferred.get(&param.name).cloned() else {
                return Err(VerseError::check_at(
                    format!(
                        "could not infer type parameter `{}` for type function `{display_name}`",
                        param.name
                    ),
                    span,
                ));
            };
            self.ensure_type_arg_satisfies_constraint_with_inferred(
                &param.name,
                &param.constraint,
                &actual,
                Some(&inferred),
                span,
            )?;
        }
        Ok(inferred)
    }

    fn instantiate_type_function_for_assignability(
        &self,
        qualified: &str,
        args: &[Type],
    ) -> Option<Type> {
        let infos = self.type_functions.get(qualified)?;
        let (info, inferred) =
            self.select_type_function_candidate_for_assignability(infos, args)?;
        let mut checker = self.clone();
        checker.push_type_param_scope(inferred);
        let previous_module_path =
            std::mem::replace(&mut checker.module_path, info.module_path.clone());
        let result = checker.type_name_to_type_name_for_assignability(&info.target);
        checker.module_path = previous_module_path;
        checker.pop_type_param_scope();
        result
    }

    fn select_type_function_candidate_for_assignability(
        &self,
        infos: &[TypeFunctionInfo],
        args: &[Type],
    ) -> Option<(TypeFunctionInfo, HashMap<String, Type>)> {
        if let [info] = infos {
            if info.params.len() != args.len() {
                return None;
            }
            let mut checker = self.clone();
            let inferred = checker
                .with_type_function_module_path(info, |checker| {
                    checker.infer_type_function_type_params("", info, args, Span::new(0, 0, 0, 0))
                })
                .ok()?;
            return Some((info.clone(), inferred));
        }

        let span = Span::new(0, 0, 0, 0);
        let matches = infos
            .iter()
            .filter(|info| info.params.len() == args.len())
            .filter_map(|info| {
                let mut checker = self.clone();
                let (inferred, score) = checker
                    .with_type_function_module_path(info, |checker| {
                        let inferred =
                            checker.infer_type_function_type_params("", info, args, span)?;
                        let score = checker
                            .type_function_candidate_match_score(info, args, &inferred, span)?;
                        Ok((inferred, score))
                    })
                    .ok()?;
                Some((score, info.clone(), inferred))
            })
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [] => None,
            matches => {
                let best_score = matches.iter().map(|(score, _, _)| *score).min()?;
                let mut best = matches
                    .iter()
                    .filter(|(score, _, _)| *score == best_score)
                    .map(|(_, info, inferred)| (info.clone(), inferred.clone()))
                    .collect::<Vec<_>>();
                if best.len() == 1 { best.pop() } else { None }
            }
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

    pub(super) fn define_type_param(
        &mut self,
        name: &str,
        value_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        if name.contains('.') {
            return Ok(());
        }
        let Some(scope) = self.type_param_scopes.last_mut() else {
            return Ok(());
        };
        if scope.contains_key(name) {
            return Err(VerseError::check_at(
                format!("duplicate type parameter `{name}`"),
                span,
            ));
        }
        scope.insert(name.to_string(), value_type);
        Ok(())
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

    pub(super) fn ensure_module_import_accessible(
        &self,
        module_name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(scope) = self.inaccessible_scoped_enclosing_module(module_name) {
            return Err(VerseError::check_at(
                format!("module `{module_name}` is scoped to `{scope}`"),
                span,
            ));
        }
        if let Some(scope) = self.inaccessible_internal_enclosing_module(module_name) {
            let parent = aggregate_module_name(&scope).unwrap_or("<root module>");
            return Err(VerseError::check_at(
                format!("module `{module_name}` is internal to module `{parent}`"),
                span,
            ));
        }
        Ok(())
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
            self.ensure_module_import_accessible(&module_name, statement.span)?;
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
            TypeName::Type => Type::TypeValue,
            TypeName::TypeBounds { lower, upper } => Type::TypeValueBounds {
                lower: Box::new(self.type_name_to_type_name(lower, span)?),
                upper: Box::new(self.type_name_to_type_name(upper, span)?),
            },
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::FloatRange(range) => Type::FloatRange(*range),
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
                } else if let Some(target) = self.resolve_type_function_reference(name) {
                    self.instantiate_type_function(name, &target, &args, span)?
                } else {
                    self.instantiate_parametric_type(name, &args, span)?
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = builtin_numeric_alias_type(name) {
                    value_type
                } else if let Some(value_type) = self.resolve_type_param(name) {
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
                } else if let Some(alias_name) = self.resolve_type_alias_reference(name, span)? {
                    self.resolve_type_alias(&alias_name, &mut Vec::new())?
                } else if let Some(value_type) =
                    self.type_value_binding_annotation_type(name, span)?
                {
                    value_type
                } else {
                    self.named_type_to_type(name, span)?
                }
            }
        };
        Ok(value_type)
    }

    fn type_value_binding_annotation_type(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Option<Type>, VerseError> {
        let Some((value_type, mutable)) = self.type_value_binding_type(name, span)? else {
            return Ok(None);
        };

        if mutable && type_can_be_used_as_type_value(&value_type) {
            return Err(VerseError::check_at(
                format!("mutable type value `{name}` cannot be used as a type annotation"),
                span,
            ));
        }

        if let Some(instance_type) = type_value_instance_type(&value_type) {
            return Ok(Some(instance_type));
        }

        if type_can_be_used_as_type_value(&value_type) {
            return Err(VerseError::check_at(
                format!("type value `{name}` is not precise enough for a type annotation"),
                span,
            ));
        }

        Ok(None)
    }

    fn type_value_binding_type(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Option<(Type, bool)>, VerseError> {
        if let Some((module_name, member_name)) = name.rsplit_once('.') {
            let Some(module_info) = self.module_types.get(module_name) else {
                return Ok(None);
            };
            let Some(value_type) = module_info.members.get(member_name) else {
                return Ok(None);
            };
            let access = module_info
                .member_access
                .get(member_name)
                .copied()
                .unwrap_or(AccessLevel::Internal);
            self.ensure_module_member_accessible(module_name, access, member_name, span)?;
            return Ok(Some((value_type.clone(), false)));
        }

        Ok(self
            .lookup_accessible(name, span)?
            .map(|symbol| (symbol.value_type, symbol.mutable)))
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
            TypeName::Type => Type::TypeValue,
            TypeName::TypeBounds { lower, upper } => Type::TypeValueBounds {
                lower: Box::new(self.type_name_to_type_name_for_assignability(lower)?),
                upper: Box::new(self.type_name_to_type_name_for_assignability(upper)?),
            },
            TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
            TypeName::FloatRange(range) => Type::FloatRange(*range),
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
                } else if let Some(target) = self.resolve_type_function_reference(name) {
                    self.instantiate_type_function_for_assignability(&target, &args)?
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
                if let Some(value_type) = builtin_numeric_alias_type(name) {
                    value_type
                } else if let Some(value_type) = self.resolve_type_param(name) {
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
        let parent = match value_type {
            Type::Param(_, TypeParamConstraint::Subtype(parent)) => parent,
            Type::Param(_, TypeParamConstraint::TypeBounds { upper, .. }) => upper,
            _ => return Ok(None),
        };
        let supertype = self.type_name_to_type_name(parent, span)?;
        let supertype = type_param_constraint_instance_supertype(supertype);
        Ok((!matches!(supertype, Type::Param(_, _))).then_some(supertype))
    }

    pub(super) fn constrained_type_param_supertype_for_assignability(
        &self,
        value_type: &Type,
    ) -> Option<Type> {
        let parent = match value_type {
            Type::Param(_, TypeParamConstraint::Subtype(parent)) => parent,
            Type::Param(_, TypeParamConstraint::TypeBounds { upper, .. }) => upper,
            _ => return None,
        };
        let supertype = self.type_name_to_type_name_for_assignability(parent)?;
        let supertype = type_param_constraint_instance_supertype(supertype);
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

    pub(super) fn dependent_type_value_param_signature(
        &mut self,
        params: &[Param],
        return_type: Option<&TypeAnnotation>,
    ) -> Result<(Vec<Type>, Vec<ParamSpec>, Vec<Type>), VerseError> {
        let mut signature_types = Vec::with_capacity(params.len());
        let mut specs = Vec::with_capacity(params.len());
        let mut body_types = Vec::with_capacity(params.len());

        for (index, param) in params.iter().enumerate() {
            let body_type = self.annotation_to_type(param.annotation.as_ref())?;
            let used_as_type = type_value_param_used_as_type(
                &param.name,
                &params[index + 1..],
                return_type.map(|annotation| &annotation.name),
            );
            let signature_type = if used_as_type {
                self.register_dependent_type_value_param(param, &body_type)?
            } else {
                body_type.clone()
            };
            let tuple_items = match &param.pattern {
                ParamPattern::Tuple(items) => {
                    Some(self.dependent_type_value_param_signature(items, None)?.1)
                }
                ParamPattern::Binding | ParamPattern::Anonymous => None,
            };
            specs.push(ParamSpec {
                name: param.name.clone(),
                value_type: signature_type.clone(),
                named: param.named,
                has_default: param.default.is_some(),
                tuple_items,
            });
            signature_types.push(signature_type);
            body_types.push(body_type);
        }

        Ok((signature_types, specs, body_types))
    }

    fn register_dependent_type_value_param(
        &mut self,
        param: &Param,
        value_type: &Type,
    ) -> Result<Type, VerseError> {
        let ParamPattern::Binding = &param.pattern else {
            return Ok(value_type.clone());
        };
        let Some((type_param, signature_type)) =
            Self::dependent_type_value_param_pattern(&param.name, value_type)
        else {
            return Ok(value_type.clone());
        };
        if let Some(existing) = self.resolve_type_param(&param.name) {
            if existing == type_param {
                return Ok(signature_type);
            }
        }
        self.define_type_param(&param.name, type_param, param.span)?;
        Ok(signature_type)
    }

    fn dependent_type_value_param_pattern(name: &str, value_type: &Type) -> Option<(Type, Type)> {
        let (constraint_name, parent) = match value_type {
            Type::Subtype(parent) => ("subtype", parent.as_ref()),
            Type::CastableSubtype(parent) => ("castable_subtype", parent.as_ref()),
            Type::ConcreteSubtype(parent) => ("concrete_subtype", parent.as_ref()),
            Type::TypeValue => {
                let type_param = Type::Param(name.to_string(), TypeParamConstraint::Type);
                return Some((type_param.clone(), Type::TypeValueOf(Box::new(type_param))));
            }
            Type::TypeValueBounds { lower, upper } => {
                let type_param = Type::Param(
                    name.to_string(),
                    TypeParamConstraint::TypeBounds {
                        lower: type_to_constraint_type_name(lower)?,
                        upper: type_to_constraint_type_name(upper)?,
                    },
                );
                return Some((type_param.clone(), Type::TypeValueOf(Box::new(type_param))));
            }
            _ => return None,
        };
        let parent_name = type_to_constraint_type_name(parent)?;
        let constraint_parent = if constraint_name == "subtype" {
            parent_name
        } else {
            TypeName::Applied {
                name: constraint_name.to_string(),
                args: vec![parent_name],
            }
        };
        let type_param = Type::Param(
            name.to_string(),
            TypeParamConstraint::Subtype(constraint_parent),
        );
        let signature_type = match value_type {
            Type::Subtype(_) => Type::Subtype(Box::new(type_param.clone())),
            Type::CastableSubtype(_) => Type::CastableSubtype(Box::new(type_param.clone())),
            Type::ConcreteSubtype(_) => Type::ConcreteSubtype(Box::new(type_param.clone())),
            _ => return None,
        };
        Some((type_param, signature_type))
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

        if info.kind == ParametricTypeKind::Alias {
            let Some(target) = info.target.as_ref() else {
                return Err(VerseError::check_at(
                    format!("unknown parametric type `{name}`"),
                    span,
                ));
            };
            let previous_module_path = std::mem::replace(&mut self.module_path, info.module_path);
            self.push_type_param_scope(
                info.params
                    .iter()
                    .zip(args)
                    .map(|(param, arg)| (param.name.clone(), arg.clone())),
            );
            let result = self.type_name_to_type(target);
            self.pop_type_param_scope();
            self.module_path = previous_module_path;
            return result;
        }

        let instance_name = render_parametric_instance_type_name(&qualified, args);
        self.parametric_type_instances
            .entry(instance_name.clone())
            .or_insert_with(|| args.to_vec());
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
                        native: info.native,
                        persistable: *persistable,
                        computes: *computes,
                        constructor_effects: Vec::new(),
                        constructor_access: AccessLevel::Public,
                        constructor_scopes: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let fields = self.struct_field_infos_with_owner(
                    fields,
                    Some(&instance_name),
                    FieldOwnerKind::Struct,
                )?;
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
                        native: info.native,
                        persistable: *persistable,
                        computes: *computes,
                        constructor_effects: Vec::new(),
                        constructor_access: AccessLevel::Public,
                        constructor_scopes: Vec::new(),
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
                let (constructor_access, constructor_scopes) =
                    class_constructor_access_from_specifiers(specifiers, info.span)?;
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
                        native: info.native,
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        constructor_effects: class_constructor_effects(blocks),
                        constructor_access,
                        constructor_scopes: constructor_scopes.clone(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                    },
                );
                let (fields, methods, unique, castable, base, implemented_interfaces) = self
                    .class_member_infos(
                        &instance_name,
                        ClassDefinitionParts {
                            definition_access: info.access,
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
                        native: info.native,
                        persistable: class_has_specifier(specifiers, "persistable"),
                        computes: false,
                        constructor_effects: class_constructor_effects(blocks),
                        constructor_access,
                        constructor_scopes,
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
                let local_fields = self.struct_field_infos_with_owner(
                    fields,
                    Some(&instance_name),
                    FieldOwnerKind::Interface,
                )?;
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
                self.check_interface_method_bodies(&instance_name, info.access, &fields, methods)?;
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
        let inferred = params
            .iter()
            .zip(args.iter())
            .map(|(param, arg)| (param.name.clone(), arg.clone()))
            .collect::<HashMap<_, _>>();
        for (param, arg) in params.iter().zip(args) {
            self.ensure_type_arg_satisfies_constraint_with_inferred(
                &param.name,
                &param.constraint,
                arg,
                Some(&inferred),
                span,
            )?;
        }
        Ok(())
    }

    fn ensure_type_arg_satisfies_constraint_with_inferred(
        &mut self,
        param_name: &str,
        constraint: &TypeParamConstraint,
        actual: &Type,
        inferred: Option<&HashMap<String, Type>>,
        span: Span,
    ) -> Result<(), VerseError> {
        match constraint {
            TypeParamConstraint::Type => Ok(()),
            TypeParamConstraint::Subtype(expected_name) => {
                let expected =
                    self.type_name_to_type_name_for_constraint(expected_name, inferred, span)?;
                if self.type_argument_satisfies_constraint(&expected, actual) {
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
            TypeParamConstraint::TypeBounds { lower, upper } => {
                let upper = self.type_name_to_type_name_for_constraint(upper, inferred, span)?;
                if !self.type_argument_satisfies_constraint(&upper, actual) {
                    return Err(VerseError::check_at(
                        format!(
                            "type argument `{actual}` for `{param_name}` must be a subtype of `{upper}`"
                        ),
                        span,
                    ));
                }

                let lower = self.type_name_to_type_name_for_constraint(lower, inferred, span)?;
                if self.type_argument_satisfies_constraint(actual, &lower) {
                    Ok(())
                } else {
                    Err(VerseError::check_at(
                        format!(
                            "type argument `{actual}` for `{param_name}` must be a supertype of `{lower}`"
                        ),
                        span,
                    ))
                }
            }
        }
    }

    fn type_argument_satisfies_constraint(&self, expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Subtype(expected), Type::Class(actual)) => {
                self.class_type_value_satisfies_subtype(actual, expected)
            }
            (Type::CastableSubtype(expected), Type::Class(actual)) => {
                self.class_type_value_is_castable_subtype(actual, expected)
            }
            (Type::ConcreteSubtype(expected), Type::Class(actual)) => {
                self.class_type_value_is_concrete_subtype(actual, expected)
            }
            _ => self.is_assignable(expected, actual),
        }
    }

    fn type_name_to_type_name_for_constraint(
        &mut self,
        name: &TypeName,
        inferred: Option<&HashMap<String, Type>>,
        span: Span,
    ) -> Result<Type, VerseError> {
        let Some(inferred) = inferred else {
            return self.type_name_to_type_name(name, span);
        };
        self.push_type_param_scope(
            inferred
                .iter()
                .map(|(name, value_type)| (name.clone(), value_type.clone())),
        );
        let result = self.type_name_to_type_name(name, span);
        self.pop_type_param_scope();
        result
    }

    pub(super) fn ensure_inferred_type_param_constraints(
        &mut self,
        param_types: Option<&[Type]>,
        return_type: Option<&Type>,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<(), VerseError> {
        let mut checked = Vec::new();
        if let Some(param_types) = param_types {
            for param_type in param_types {
                self.ensure_inferred_type_param_constraints_inner(
                    param_type,
                    inferred,
                    span,
                    &mut checked,
                )?;
            }
        }
        if let Some(return_type) = return_type {
            self.ensure_inferred_type_param_constraints_inner(
                return_type,
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
                        self.ensure_type_arg_satisfies_constraint_with_inferred(
                            name,
                            constraint,
                            actual,
                            Some(inferred),
                            span,
                        )?;
                    }
                    checked.push(name.clone());
                }
                Ok(())
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Task(item)
            | Type::TypeValueOf(item)
            | Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::ClassifiableSubsetKey(item)
            | Type::ClassifiableSubsetVar(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Signalable(item) => {
                self.ensure_inferred_type_param_constraints_inner(item, inferred, span, checked)
            }
            Type::TypeValueBounds { lower, upper } => {
                self.ensure_inferred_type_param_constraints_inner(lower, inferred, span, checked)?;
                self.ensure_inferred_type_param_constraints_inner(upper, inferred, span, checked)
            }
            Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
                self.ensure_inferred_type_param_constraints_inner(key, inferred, span, checked)?;
                self.ensure_inferred_type_param_constraints_inner(value, inferred, span, checked)
            }
            Type::SuccessResult(item) | Type::ErrorResult(item) => {
                self.ensure_inferred_type_param_constraints_inner(item, inferred, span, checked)
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
            | Type::SubscribableEventIntrnl(payload)
            | Type::StickyEvent(payload)
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
            Type::SubscribableEvent(payload) => {
                self.ensure_inferred_type_param_constraints_inner(payload, inferred, span, checked)
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
        self.substitute_parametric_instance_names_runtime(substituted, inferred, span)
    }

    fn substitute_parametric_instance_names_runtime(
        &mut self,
        value_type: Type,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<Type, VerseError> {
        match value_type {
            Type::Struct(name) => Ok(Type::Struct(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::StructType(name) => Ok(Type::StructType(
                self.substitute_parametric_instance_name(&name, inferred, span)?,
            )),
            Type::TypeValueOf(item) => Ok(Type::TypeValueOf(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::TypeValueBounds { lower, upper } => Ok(Type::TypeValueBounds {
                lower: Box::new(
                    self.substitute_parametric_instance_names_runtime(*lower, inferred, span)?,
                ),
                upper: Box::new(
                    self.substitute_parametric_instance_names_runtime(*upper, inferred, span)?,
                ),
            }),
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
            Type::Array(item) => Ok(Type::Array(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::Map(key, value) => Ok(Type::Map(
                Box::new(self.substitute_parametric_instance_names_runtime(*key, inferred, span)?),
                Box::new(
                    self.substitute_parametric_instance_names_runtime(*value, inferred, span)?,
                ),
            )),
            Type::WeakMap(key, value) => Ok(Type::WeakMap(
                Box::new(self.substitute_parametric_instance_names_runtime(*key, inferred, span)?),
                Box::new(
                    self.substitute_parametric_instance_names_runtime(*value, inferred, span)?,
                ),
            )),
            Type::Tuple(items) => Ok(Type::Tuple(
                items
                    .into_iter()
                    .map(|item| {
                        self.substitute_parametric_instance_names_runtime(item, inferred, span)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Type::Option(item) => Ok(Type::Option(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::Result(success, error) => Ok(Type::Result(
                Box::new(
                    self.substitute_parametric_instance_names_runtime(*success, inferred, span)?,
                ),
                Box::new(
                    self.substitute_parametric_instance_names_runtime(*error, inferred, span)?,
                ),
            )),
            Type::SuccessResult(item) => Ok(Type::SuccessResult(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ErrorResult(item) => Ok(Type::ErrorResult(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::Event(payload) => Ok(Type::Event(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::SubscribableEvent(payload) => Ok(Type::SubscribableEvent(Box::new(
                self.substitute_parametric_instance_names_runtime(*payload, inferred, span)?,
            ))),
            Type::SubscribableEventIntrnl(payload) => Ok(Type::SubscribableEventIntrnl(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::StickyEvent(payload) => Ok(Type::StickyEvent(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::Task(payload) => Ok(Type::Task(Box::new(
                self.substitute_parametric_instance_names_runtime(*payload, inferred, span)?,
            ))),
            Type::Generator(payload) => Ok(Type::Generator(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::Subtype(item) => Ok(Type::Subtype(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::CastableSubtype(item) => Ok(Type::CastableSubtype(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ConcreteSubtype(item) => Ok(Type::ConcreteSubtype(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ClassifiableSubset(item) => Ok(Type::ClassifiableSubset(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ClassifiableSubsetKey(item) => Ok(Type::ClassifiableSubsetKey(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ClassifiableSubsetVar(item) => Ok(Type::ClassifiableSubsetVar(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::Modifier(item) => Ok(Type::Modifier(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::ModifierStack(item) => Ok(Type::ModifierStack(Box::new(
                self.substitute_parametric_instance_names_runtime(*item, inferred, span)?,
            ))),
            Type::Awaitable(payload) => Ok(Type::Awaitable(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::Signalable(payload) => Ok(Type::Signalable(Box::new(
                self.substitute_parametric_instance_names_runtime(*payload, inferred, span)?,
            ))),
            Type::Subscribable(payload) => Ok(Type::Subscribable(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::Listenable(payload) => Ok(Type::Listenable(
                payload
                    .map(|payload| {
                        self.substitute_parametric_instance_names_runtime(*payload, inferred, span)
                            .map(Box::new)
                    })
                    .transpose()?,
            )),
            Type::Function {
                arity,
                arity_range,
                effects,
                param_types,
                param_specs,
                return_type,
            } => Ok(Type::Function {
                arity,
                arity_range,
                effects,
                param_types: param_types
                    .map(|params| {
                        params
                            .into_iter()
                            .map(|param| {
                                self.substitute_parametric_instance_names_runtime(
                                    param, inferred, span,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
                param_specs: param_specs
                    .map(|specs| {
                        specs
                            .iter()
                            .map(|spec| self.substitute_param_spec_runtime(spec, inferred, span))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
                return_type: Box::new(self.substitute_parametric_instance_names_runtime(
                    *return_type,
                    inferred,
                    span,
                )?),
            }),
            Type::Overload(overloads) => Ok(Type::Overload(
                overloads
                    .into_iter()
                    .map(|overload| {
                        self.substitute_parametric_instance_names_runtime(overload, inferred, span)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            other => Ok(other),
        }
    }

    pub(super) fn substitute_param_spec_runtime(
        &mut self,
        spec: &ParamSpec,
        inferred: &HashMap<String, Type>,
        span: Span,
    ) -> Result<ParamSpec, VerseError> {
        Ok(ParamSpec {
            name: spec.name.clone(),
            value_type: self.substitute_type_params_runtime(&spec.value_type, inferred, span)?,
            named: spec.named,
            has_default: spec.has_default,
            tuple_items: spec
                .tuple_items
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| self.substitute_param_spec_runtime(item, inferred, span))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
        })
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
        let replaced = replace_type_param_atoms(name, inferred);
        if replaced != name
            && let Some((head, args)) = split_parametric_instance_type_name(&replaced)
            && self.resolve_parametric_type_reference(&head).is_some()
        {
            let args = args
                .iter()
                .map(|arg| self.type_name_to_type_name(&TypeName::parse(arg.clone()), span))
                .collect::<Result<Vec<_>, _>>()?;
            self.instantiate_parametric_type(&head, &args, span)?;
        }
        Ok(replaced)
    }
}
