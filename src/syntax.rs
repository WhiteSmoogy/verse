use crate::ast::Program;

/// Verse syntax tree produced by the parser.
///
/// The current Rust parser already builds the tree shape used by later passes.
/// This wrapper gives the pipeline the same explicit VST boundary as uLang's
/// `Syntax/VstNode` layer without changing the parser's internal node types.
#[derive(Debug, Clone, PartialEq)]
pub struct VstProgram {
    program: Program,
}

impl VstProgram {
    pub fn new(program: Program) -> Self {
        Self { program }
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn into_program(self) -> Program {
        self.program
    }
}

impl From<Program> for VstProgram {
    fn from(program: Program) -> Self {
        Self::new(program)
    }
}
