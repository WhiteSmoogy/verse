use crate::ast::{BinaryOp, CaseArm, CasePattern, Expr, ExprKind, UnaryOp};

pub(super) fn is_failable_condition_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::UnwrapOption(_) | ExprKind::BracketCall { .. } => true,
        ExprKind::Unary {
            op: UnaryOp::Not,
            expr,
        } => is_failable_condition_expr(expr),
        ExprKind::Binary { left, op, right } => {
            is_failure_binary_op(*op)
                || is_failable_condition_expr(left)
                || is_failable_condition_expr(right)
        }
        ExprKind::Profile { body, .. } => is_failable_condition_expr(body),
        ExprKind::Spawn { .. } => false,
        ExprKind::Concurrent { .. } => false,
        ExprKind::Case { subject, arms } => {
            is_failable_condition_expr(subject)
                || !case_arms_have_wildcard(arms)
                || arms.iter().any(|arm| {
                    (match &arm.pattern {
                        CasePattern::Wildcard { .. } => false,
                        CasePattern::Expr(pattern) => is_failable_condition_expr(pattern),
                    }) || is_failable_condition_expr(&arm.expr)
                })
        }
        ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
            is_failable_condition_expr(object)
        }
        ExprKind::Call { callee, .. } => is_failable_condition_expr(callee),
        ExprKind::Var { expr, .. } => is_failable_condition_expr(expr),
        _ => false,
    }
}

fn case_arms_have_wildcard(arms: &[CaseArm]) -> bool {
    arms.iter()
        .any(|arm| matches!(arm.pattern, CasePattern::Wildcard { .. }))
}

fn is_failure_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Divide
            | BinaryOp::Remainder
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::And
            | BinaryOp::Or
    )
}

pub(super) fn is_comparison_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

pub(super) fn has_runtime_effect(effects: &[String], name: &str) -> bool {
    effects.iter().any(|effect| effect == name)
}
