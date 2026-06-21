pub mod bytecode;

use crate::error::VerseError;
pub use crate::semantics::{SemanticProgram, TypedProgram};
pub use bytecode::{
    BytecodeChunk, BytecodeProgram, Constant, Instruction, Opcode, RegisterIndex, ValueOperand,
};

#[derive(Debug, Clone)]
pub struct IrProgram {
    semantic: SemanticProgram,
    bytecode: BytecodeProgram,
}

impl IrProgram {
    pub fn semantic_program(&self) -> &SemanticProgram {
        &self.semantic
    }

    pub fn bytecode_program(&self) -> &BytecodeProgram {
        &self.bytecode
    }

    pub fn into_semantic_program(self) -> SemanticProgram {
        self.semantic
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct IRGenerator;

impl IRGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(self, semantic: SemanticProgram) -> Result<IrProgram, VerseError> {
        let bytecode = bytecode::BytecodeGenerator::new().generate(&semantic)?;
        Ok(IrProgram { semantic, bytecode })
    }
}
