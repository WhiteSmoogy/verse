use super::Value;

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
