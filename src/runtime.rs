use crate::error::VerseError;
pub use crate::eval::Value;
use crate::ir::IrProgram;
use crate::ir::bytecode::BytecodeProgram;
use crate::native::NativeRegistry;

pub(crate) mod bytecode_vm;
pub(crate) mod host;

#[derive(Default)]
pub struct VerseVm {
    native_registry: NativeRegistry,
}

impl VerseVm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_native_registry(native_registry: NativeRegistry) -> Self {
        Self { native_registry }
    }

    pub fn run_ir_program(&mut self, program: &IrProgram) -> Result<Value, VerseError> {
        run_bytecode_program_with_native_registry(
            program.bytecode_program(),
            self.native_registry.clone(),
        )
    }

    #[cfg(feature = "tokio-host")]
    pub fn run_ir_program_with_tokio_host(
        &mut self,
        program: &IrProgram,
    ) -> Result<Value, VerseError> {
        run_bytecode_program_with_tokio_host_and_native_registry(
            program.bytecode_program(),
            self.native_registry.clone(),
        )
    }
}

#[allow(dead_code)]
pub(crate) fn run_bytecode_program(program: &BytecodeProgram) -> Result<Value, VerseError> {
    run_bytecode_program_with_native_registry(program, NativeRegistry::new())
}

pub(crate) fn run_bytecode_program_with_native_registry(
    program: &BytecodeProgram,
    native_registry: NativeRegistry,
) -> Result<Value, VerseError> {
    crate::eval::with_stable_runtime_epoch(|| {
        let mut executor =
            bytecode_vm::BytecodeExecutor::with_native_registry(program, native_registry);
        executor.run()
    })
}

#[cfg(feature = "tokio-host")]
pub fn run_bytecode_program_with_tokio_host(
    program: &BytecodeProgram,
) -> Result<Value, VerseError> {
    run_bytecode_program_with_tokio_host_and_native_registry(program, NativeRegistry::new())
}

#[cfg(feature = "tokio-host")]
pub fn run_bytecode_program_with_tokio_host_and_native_registry(
    program: &BytecodeProgram,
    native_registry: NativeRegistry,
) -> Result<Value, VerseError> {
    crate::eval::with_stable_runtime_epoch(|| {
        let mut executor = bytecode_vm::BytecodeExecutor::with_host_and_native_registry(
            program,
            host::TokioHost::new(),
            native_registry,
        );
        executor.run()
    })
}
