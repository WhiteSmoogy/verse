use verse_rs::ast::{ExprKind, StmtKind};
use verse_rs::{Interpreter, Type, Value, check_source_to_typed_program};

#[test]
fn checked_typed_program_can_be_evaluated() {
    let source = r#"
Answer():int =
    40 + 2

Answer()
"#;
    let typed = check_source_to_typed_program(source).expect("source should check");

    assert_eq!(typed.value_type, Type::Int);
    let StmtKind::Let { expr, .. } = &typed.program.statements[0].kind else {
        panic!("expected function binding");
    };
    let ExprKind::Function { body, .. } = &expr.kind else {
        panic!("expected function expression");
    };
    assert!(matches!(body.kind, ExprKind::Block(_)));

    let mut interpreter = Interpreter::new();
    assert_eq!(
        interpreter
            .eval_typed_program(&typed)
            .expect("typed program should run"),
        Value::Int(42)
    );
}
