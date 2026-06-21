use crate::ast::Program;
use crate::checker::Type;
use crate::error::Diagnostic;

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub program: Program,
    pub value_type: Type,
    pub warnings: Vec<Diagnostic>,
}
