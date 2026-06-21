use verse_rs::ast::{ExprKind, StmtKind};
use verse_rs::{desugar_program, parse_source};

#[test]
fn desugars_function_colon_body_to_core_block() {
    let program = parse_source(
        r#"
Answer():int =
    42
"#,
    )
    .expect("source should parse");
    let desugared = desugar_program(&program);

    let StmtKind::Let { expr, .. } = &desugared.statements[0].kind else {
        panic!("expected function binding");
    };
    let ExprKind::Function { body, .. } = &expr.kind else {
        panic!("expected function expression");
    };
    assert!(matches!(body.kind, ExprKind::Block(_)));
}

#[test]
fn preserves_structured_concurrency_colon_body() {
    let program = parse_source(
        r#"
Main()<suspends>:void =
    sync:
        Sleep(0.0)
"#,
    )
    .expect("source should parse");
    let desugared = desugar_program(&program);

    let StmtKind::Let { expr, .. } = &desugared.statements[0].kind else {
        panic!("expected function binding");
    };
    let ExprKind::Function { body, .. } = &expr.kind else {
        panic!("expected function expression");
    };
    let ExprKind::Block(statements) = &body.kind else {
        panic!("expected outer function body to be desugared to a block");
    };
    let StmtKind::Expr(expr) = &statements[0].kind else {
        panic!("expected sync expression statement");
    };
    let ExprKind::Concurrent { body, .. } = &expr.kind else {
        panic!("expected concurrent expression");
    };
    assert!(matches!(body.kind, ExprKind::ColonBlock(_)));
}
