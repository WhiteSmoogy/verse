//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn check_errors_include_source_locations() {
    let source = r#"x: number := "not a number""#;
    let pretty = check_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("line 1, column 14"));
    assert!(pretty.contains(r#""not a number""#));
    assert!(pretty.contains("^"));
}

#[test]
fn runtime_errors_include_source_locations() {
    let source = "1 / 0";
    let mut interpreter = Interpreter::new();
    let pretty = interpreter
        .eval_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("division by zero at line 1, column 1"));
    assert!(pretty.contains("1 / 0"));
    assert!(pretty.contains("^^^^^"));
}
