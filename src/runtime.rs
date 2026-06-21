use crate::error::VerseError;
pub use crate::eval::Value;
use crate::ir::IrProgram;
use crate::ir::bytecode::BytecodeProgram;

pub(crate) mod bytecode_vm;

#[derive(Default)]
pub struct VerseVm;

impl VerseVm {
    pub fn new() -> Self {
        Self
    }

    pub fn run_ir_program(&mut self, program: &IrProgram) -> Result<Value, VerseError> {
        run_bytecode_program(program.bytecode_program())
    }
}

pub(crate) fn run_bytecode_program(program: &BytecodeProgram) -> Result<Value, VerseError> {
    crate::eval::with_stable_runtime_epoch(|| bytecode_vm::BytecodeExecutor::new(program).run())
}
