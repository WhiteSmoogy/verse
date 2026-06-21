use std::rc::Rc;

use crate::ast::{Param, ParamPattern, TypeAnnotation, TypeName};
use crate::colors::NAMED_COLORS;
use crate::token::Span;

use super::{
    Env, RuntimeAccessLevel, RuntimeClassField, RuntimeClassMethod, RuntimeStructField, Value,
};

pub(super) fn color_struct_type() -> Value {
    Value::StructType {
        name: "color".to_string(),
        computes: false,
        fields: ["R", "G", "B"]
            .into_iter()
            .map(|name| RuntimeStructField {
                name: name.to_string(),
                default: None,
            })
            .collect(),
    }
}

pub(super) fn color_alpha_struct_type() -> Value {
    Value::StructType {
        name: "color_alpha".to_string(),
        computes: false,
        fields: ["Color", "A"]
            .into_iter()
            .map(|name| RuntimeStructField {
                name: name.to_string(),
                default: None,
            })
            .collect(),
    }
}

pub(super) fn locale_struct_type() -> Value {
    Value::StructType {
        name: "locale".to_string(),
        computes: false,
        fields: Vec::new(),
    }
}

pub(super) fn session_environment_enum_type() -> Value {
    Value::EnumType {
        name: "session_environment".to_string(),
        variants: vec![
            "Edit".to_string(),
            "Private".to_string(),
            "Live".to_string(),
        ],
        open: false,
    }
}

pub(super) fn color_value(red: f64, green: f64, blue: f64) -> Value {
    Value::StructInstance {
        struct_name: "color".to_string(),
        computes: false,
        fields: vec![
            ("R".to_string(), Value::Float(red)),
            ("G".to_string(), Value::Float(green)),
            ("B".to_string(), Value::Float(blue)),
        ],
    }
}

fn color_value_from_srgb_values(red: u8, green: u8, blue: u8) -> Value {
    color_value(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    )
}

pub(super) fn color_alpha_value(color: Value, alpha: f64) -> Value {
    Value::StructInstance {
        struct_name: "color_alpha".to_string(),
        computes: false,
        fields: vec![
            ("Color".to_string(), color),
            ("A".to_string(), Value::Float(alpha)),
        ],
    }
}

pub(super) fn named_colors_module() -> Value {
    let env = Env::new();
    for color in NAMED_COLORS {
        env.define(
            color.name,
            color_value_from_srgb_values(color.red, color.green, color.blue),
            false,
        );
    }
    Value::Module {
        name: "NamedColors".to_string(),
        env,
    }
}

pub(super) fn builtin_interface_types() -> Vec<(&'static str, Value)> {
    vec![
        (
            "cancelable",
            builtin_interface_type(
                "cancelable",
                Vec::new(),
                Vec::new(),
                vec![builtin_interface_method("Cancel", &["transacts"])],
            ),
        ),
        (
            "disposable",
            builtin_interface_type(
                "disposable",
                Vec::new(),
                Vec::new(),
                vec![builtin_interface_method("Dispose", &["transacts"])],
            ),
        ),
        (
            "enableable",
            builtin_interface_type(
                "enableable",
                Vec::new(),
                Vec::new(),
                vec![
                    builtin_interface_method("Enable", &["transacts"]),
                    builtin_interface_method("Disable", &["transacts"]),
                    builtin_interface_method("IsEnabled", &["transacts", "decides"]),
                ],
            ),
        ),
        (
            "invalidatable",
            builtin_interface_type(
                "invalidatable",
                vec!["disposable".to_string()],
                Vec::new(),
                vec![
                    builtin_interface_method("Dispose", &["transacts"]),
                    builtin_interface_method("IsValid", &["transacts", "decides"]),
                ],
            ),
        ),
        (
            "showable",
            builtin_interface_type(
                "showable",
                Vec::new(),
                vec![RuntimeClassField {
                    name: "Show".to_string(),
                    mutable: true,
                    final_member: false,
                    access: RuntimeAccessLevel::Public,
                    owner: Some("showable".to_string()),
                    default: None,
                }],
                Vec::new(),
            ),
        ),
    ]
}

fn builtin_interface_type(
    name: &str,
    parents: Vec<String>,
    fields: Vec<RuntimeClassField>,
    methods: Vec<RuntimeClassMethod>,
) -> Value {
    Value::InterfaceType {
        name: name.to_string(),
        parents,
        fields,
        methods,
    }
}

fn builtin_interface_method(name: &str, effects: &[&str]) -> RuntimeClassMethod {
    RuntimeClassMethod {
        qualifier: None,
        name: name.to_string(),
        final_member: false,
        params: Vec::new(),
        effects: effects.iter().map(|effect| (*effect).to_string()).collect(),
        body: None,
        closure: Env::new(),
        super_type: None,
        extension_methods: Rc::new(Vec::new()),
    }
}

pub(super) fn runtime_modifier_method(item_type: TypeName) -> RuntimeClassMethod {
    RuntimeClassMethod {
        qualifier: None,
        name: "Evaluate".to_string(),
        final_member: false,
        params: vec![Param {
            name: "InValue".to_string(),
            annotation: Some(TypeAnnotation {
                name: item_type,
                span: Span::new(0, 0, 1, 1),
            }),
            type_params: Vec::new(),
            named: false,
            default: None,
            pattern: ParamPattern::Binding,
            span: Span::new(0, 0, 1, 1),
        }],
        effects: Vec::new(),
        body: None,
        closure: Env::new(),
        super_type: None,
        extension_methods: Rc::new(Vec::new()),
    }
}
