use crate::ast::Program;
use crate::checker::{CheckResult, Checker, RecoveredCheckResult, Type};
use crate::error::VerseError;
use crate::semantics::SemanticProgram;

#[derive(Debug, Default, Clone, Copy)]
pub struct SemanticAnalyzer;

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_program(self, program: &Program) -> Result<SemanticProgram, VerseError> {
        Checker::new().check_program_to_semantic_program(program)
    }

    pub fn analyze_desugared_program(
        self,
        program: Program,
    ) -> Result<SemanticProgram, VerseError> {
        Checker::new().check_desugared_program_to_semantic_program(program)
    }

    pub fn check_program(self, program: &Program) -> Result<Type, VerseError> {
        Ok(self.analyze_program(program)?.value_type)
    }

    pub fn check_program_with_diagnostics(
        self,
        program: &Program,
    ) -> Result<CheckResult, VerseError> {
        Checker::new().check_program_with_diagnostics(program)
    }

    pub fn check_program_with_recovery(self, program: &Program) -> RecoveredCheckResult {
        Checker::new().check_program_with_recovery(program)
    }
}
