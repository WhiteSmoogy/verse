use crate::desugar::Desugarer;
use crate::error::VerseError;
use crate::ir::{IRGenerator, IrProgram};
use crate::parser::parse_source;
use crate::semantic_analyzer::SemanticAnalyzer;
use crate::semantics::SemanticProgram;
use crate::syntax::VstProgram;

pub trait ParserPass {
    fn process_snippet(&self, source: &str) -> Result<VstProgram, VerseError>;
}

pub trait PostVstPass {
    fn process_vst(&self, vst: &VstProgram) -> VstProgram;
}

pub trait SemanticAnalyzerPass {
    fn process_vst(&self, vst: VstProgram) -> Result<SemanticProgram, VerseError>;
}

pub trait IrGeneratorPass {
    fn process_semantics(&self, program: SemanticProgram) -> Result<IrProgram, VerseError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultParserPass;

impl ParserPass for DefaultParserPass {
    fn process_snippet(&self, source: &str) -> Result<VstProgram, VerseError> {
        Ok(VstProgram::new(parse_source(source)?))
    }
}

impl PostVstPass for Desugarer {
    fn process_vst(&self, vst: &VstProgram) -> VstProgram {
        VstProgram::new(self.desugar_program(vst.program()))
    }
}

impl SemanticAnalyzerPass for SemanticAnalyzer {
    fn process_vst(&self, vst: VstProgram) -> Result<SemanticProgram, VerseError> {
        Self::new().analyze_desugared_program(vst.into_program())
    }
}

impl IrGeneratorPass for IRGenerator {
    fn process_semantics(&self, program: SemanticProgram) -> Result<IrProgram, VerseError> {
        Self::new().generate(program)
    }
}
