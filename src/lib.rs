pub mod ast;
pub mod checker;
pub(crate) mod colors;
pub mod compiler_passes;
pub mod desugar;
pub mod digest;
pub mod error;
pub mod eval;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod pipeline;
pub mod project;
pub mod runtime;
pub mod semantic_analyzer;
pub mod semantics;
pub mod syntax;
pub mod token;

pub use checker::{
    CheckResult, Checker, Effect, EffectSet, IntRange, RecoveredCheckResult, Type, TypeVariable,
    TypeVariableBounds, check_source, check_source_in_package, check_source_to_typed_program,
    check_source_to_typed_program_in_package, check_source_with_diagnostics,
    check_source_with_diagnostics_in_package, check_source_with_recovery,
};
pub use compiler_passes::{
    DefaultParserPass, IrGeneratorPass, ParserPass, PostVstPass, SemanticAnalyzerPass,
};
pub use desugar::{Desugarer, desugar_program};
pub use digest::{generate_digest, generate_digest_for_program, generate_project_digest};
pub use error::{
    DIAGNOSTIC_DESCRIPTORS, Diagnostic, DiagnosticCode, DiagnosticSeverity, VerseError,
};
pub use ir::{
    BytecodeChunk, BytecodeProgram, Constant, IRGenerator, Instruction, IrProgram, Opcode,
    RegisterIndex, ValueOperand,
};
pub use parser::parse_source;
pub use pipeline::{
    analyze_source, analyze_source_in_package, analyze_vst, compile_source,
    compile_source_in_package, desugar_vst, parse_vst_source, run_source_in_package,
};
pub use project::{SourceProject, check_project_file, load_project_source, run_project_file};
pub use runtime::{Value, VerseVm};
pub use semantic_analyzer::SemanticAnalyzer;
pub use semantics::{SemanticProgram, TypedProgram};
pub use syntax::VstProgram;

pub fn run_source(source: &str) -> Result<Value, VerseError> {
    pipeline::run_source(source)
}
