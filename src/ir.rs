pub mod bytecode;

use crate::error::VerseError;
use crate::native::InjectedNativeFunction;
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

#[derive(Debug, Default, Clone)]
pub struct IRGenerator {
    injected_native_functions: Vec<InjectedNativeFunction>,
}

impl IRGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_injected_native_functions(mut self, functions: &[InjectedNativeFunction]) -> Self {
        self.injected_native_functions = functions.to_vec();
        self
    }

    pub fn generate(self, semantic: SemanticProgram) -> Result<IrProgram, VerseError> {
        let bytecode = bytecode::BytecodeGenerator::new()
            .with_injected_native_functions(&self.injected_native_functions)
            .generate(&semantic)?;
        Ok(IrProgram { semantic, bytecode })
    }
}
