//! Executable inventory for finishing the Functions cross-scope unreachable FT.
//! Each test is a planned column; keep the columns concentrated and finish them together.

mod common;
use common::*;

fn assert_unreachable_warning(source: &str, message: &str) {
    let result = check_source_with_diagnostics(source).expect("source should check");
    assert!(
        result.warnings.iter().any(|warning| {
            warning.code == DiagnosticCode::UnreachableCode
                && warning.severity == DiagnosticSeverity::Warning
                && warning.message.contains(message)
        }),
        "expected unreachable warning containing `{message}`, got {:?}",
        result.warnings
    );
}

fn assert_no_unreachable_warning(source: &str) {
    let result = check_source_with_diagnostics(source).expect("source should check");
    assert!(
        result
            .warnings
            .iter()
            .all(|warning| warning.code != DiagnosticCode::UnreachableCode),
        "expected no unreachable-code warnings, got {:?}",
        result.warnings
    );
}

#[test]
fn warns_functions_unreachable_column_cross_scope_returns() {
    for (name, source) in [
        (
            "if expression whose branches both return",
            r#"
Bad(Ready:logic):int =
    if (Ready?):
        return 1
    else:
        return 2
    3
"#,
        ),
        (
            "braced block expression ending in return",
            r#"
Bad():int =
    {
        return 1
    }
    2
"#,
        ),
        (
            "case expression whose exhaustive arms return through blocks",
            r#"
state := enum{On, Off}

Bad(State:state):int =
    case (State):
        state.On => { return 1 }
        state.Off => { return 2 }
    3
"#,
        ),
    ] {
        assert_unreachable_warning(source, "unreachable code after `return`");
        let _ = name;
    }
}

#[test]
fn warns_functions_unreachable_column_cross_scope_breaks() {
    for source in [
        r#"
Bad(Ready:logic):void =
    loop:
        if (Ready?):
            break
        else:
            break
        Print("bad")
"#,
        r#"
Bad():void =
    loop:
        {
            break
        }
        Print("bad")
"#,
    ] {
        assert_unreachable_warning(source, "unreachable code after `break`");
    }
}

#[test]
fn warns_functions_unreachable_column_cross_scope_never_expressions() {
    for source in [
        r#"
Bad(Ready:logic):int =
    if (Ready?):
        Err("left")
    else:
        Err("right")
    42
"#,
        r#"
state := enum{On, Off}

Bad(State:state):int =
    case (State):
        state.On => Err("on")
        state.Off => Err("off")
    42
"#,
    ] {
        assert_unreachable_warning(source, "unreachable code after never-returning expression");
    }
}

#[test]
fn checks_functions_unreachable_column_partially_terminating_scopes_remain_reachable() {
    assert_no_unreachable_warning(
        r#"
Good(Ready:logic):int =
    if (Ready?):
        return 1
    else:
        40
    2
"#,
    );

    assert_no_unreachable_warning(
        r#"
state := enum{On, Off}

Good(State:state):int =
    Value:int = case (State):
        state.On => Err("on")
        state.Off => 40
    Value + 2
"#,
    );
}
