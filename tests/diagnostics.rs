//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;
use verse_rs::DIAGNOSTIC_DESCRIPTORS;

#[test]
fn check_errors_include_source_locations() {
    let source = r#"x: number := "not a number""#;
    let pretty = check_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("error[V1002]"));
    assert!(pretty.contains("line 1, column 14"));
    assert!(pretty.contains(r#""not a number""#));
    assert!(pretty.contains("^"));
}

#[test]
fn failable_expression_check_errors_include_source_locations() {
    let source = "1 / 0";
    let pretty = run_source(source)
        .expect_err("source should fail")
        .pretty(source);

    assert!(pretty.contains("error[V1002]"));
    assert!(pretty.contains("failable expression must be used in a failure context"));
    assert!(pretty.contains("1 / 0"));
    assert!(pretty.contains("^^^^^"));
}

#[test]
fn errors_expose_stable_code_and_severity() {
    let error = check_source(r#"x:int = "bad""#).expect_err("source should fail");

    assert_eq!(error.code(), DiagnosticCode::CheckError);
    assert_eq!(error.severity(), DiagnosticSeverity::Error);
    assert_eq!(error.diagnostic().code.as_str(), "V1002");
}

#[test]
fn descriptor_table_contains_warning_band_entries() {
    assert!(DIAGNOSTIC_DESCRIPTORS.iter().any(|descriptor| {
        descriptor.code == DiagnosticCode::UnreachableCode
            && descriptor.severity == DiagnosticSeverity::Warning
            && descriptor.code.number() == 2000
            && descriptor.symbol == "unreachable-code"
    }));
    assert!(DIAGNOSTIC_DESCRIPTORS.iter().any(|descriptor| {
        descriptor.code == DiagnosticCode::EmptyBlock
            && descriptor.severity == DiagnosticSeverity::Warning
            && descriptor.code.number() == 2001
            && descriptor.symbol == "empty-block"
    }));
}

#[test]
fn check_reports_unreachable_code_warning() {
    assert_check_warning(
        r#"
Bad():int = {
    return 1
    UnknownName
}
"#,
        DiagnosticCode::UnreachableCode,
        "unreachable code after `return`",
    );
}

#[test]
fn check_reports_empty_block_warning() {
    assert_check_warning(
        r#"
Empty():void = {}
"#,
        DiagnosticCode::EmptyBlock,
        "empty block",
    );
}

#[test]
fn recovery_collects_multiple_independent_check_errors() {
    let result = check_source_with_recovery(
        r#"
A:int = "bad"
B:float = false
C:int = 3
"#,
    )
    .expect("source should parse");

    assert_eq!(result.value_type, Type::Int);
    assert_eq!(result.errors.len(), 2);
    assert!(result.errors.iter().all(|error| {
        error.code == DiagnosticCode::CheckError && error.severity == DiagnosticSeverity::Error
    }));
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("binding `A`"))
    );
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("binding `B`"))
    );
}

#[test]
fn recovery_uses_unknown_type_for_failed_bindings_to_suppress_cascades() {
    let result = check_source_with_recovery(
        r#"
A:int = MissingName
B:int = A
C:int = false
"#,
    )
    .expect("source should parse");

    assert_eq!(result.errors.len(), 2, "{:?}", result.errors);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("undefined name `MissingName`"))
    );
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("binding `C`"))
    );
    assert!(
        result
            .errors
            .iter()
            .all(|error| !error.message.contains("undefined name `A`")),
        "{:?}",
        result.errors
    );
}
