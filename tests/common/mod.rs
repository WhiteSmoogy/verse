#![allow(dead_code, unused_imports)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use verse_rs::{
    DiagnosticCode, DiagnosticSeverity, Effect, EffectSet, IntRange, SourceProject, Type,
    TypeVariable, Value, check_project_file, check_source, check_source_with_diagnostics,
    check_source_with_recovery, generate_digest, generate_project_digest, parse_source,
    run_project_file, run_source,
};

pub fn eval(source: &str) -> Value {
    run_source(source).expect("source should run")
}
pub fn assert_failable_context_error(source: &str) {
    let error = check_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("failable expression must be used in a failure context")
    );
}
pub fn assert_check_warning(source: &str, code: DiagnosticCode, message: &str) {
    let result = check_source_with_diagnostics(source).expect("source should check");
    assert!(
        result.warnings.iter().any(|warning| {
            warning.code == code
                && warning.severity == DiagnosticSeverity::Warning
                && warning.message.contains(message)
        }),
        "expected warning {code} containing `{message}`, got {:?}",
        result.warnings
    );
}
pub fn function_shape(value_type: Type) -> (Option<usize>, Vec<String>, Option<Vec<Type>>, Type) {
    let Type::Function {
        arity,
        effects,
        param_types,
        return_type,
        ..
    } = value_type
    else {
        panic!("expected function type, got {value_type:?}");
    };
    (arity, effects, param_types, *return_type)
}
pub fn temp_project_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("verse_rs_{name}_{nonce}"));
    fs::create_dir_all(&dir).expect("temp project directory should be created");
    dir
}
pub fn write_project_file(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("project subdirectory should be created");
    }
    fs::write(path, source).expect("project file should be written");
}
