use crate::Checker;
use crate::desugar::Desugarer;
use crate::error::VerseError;
use crate::ir::{IRGenerator, IrProgram};
use crate::native::{InjectedNativeApi, NativeApiBundle};
use crate::parser::parse_source;
use crate::runtime::{Value, VerseVm};
use crate::semantic_analyzer::SemanticAnalyzer;
use crate::semantics::SemanticProgram;
use crate::syntax::VstProgram;

pub fn parse_vst_source(source: &str) -> Result<VstProgram, VerseError> {
    let parsed = parse_source(source)?;
    Ok(VstProgram::new(parsed))
}

pub fn desugar_vst(vst: &VstProgram) -> VstProgram {
    VstProgram::new(Desugarer::new().desugar_program(vst.program()))
}

pub fn analyze_vst(vst: VstProgram) -> Result<SemanticProgram, VerseError> {
    SemanticAnalyzer::new().analyze_desugared_program(vst.into_program())
}

pub fn analyze_source(source: &str) -> Result<SemanticProgram, VerseError> {
    analyze_source_in_package(source, None)
}

pub fn analyze_source_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<SemanticProgram, VerseError> {
    let vst = parse_vst_source(source)?;
    let desugared = desugar_vst(&vst);
    Checker::new()
        .with_package(package_name.map(str::to_string))
        .check_desugared_program_to_semantic_program(desugared.into_program())
}

pub fn analyze_source_with_native_apis(
    source: &str,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<SemanticProgram, VerseError> {
    analyze_source_with_native_apis_in_package(source, None, apis)
}

pub fn analyze_source_with_native_apis_in_package(
    source: &str,
    package_name: Option<&str>,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<SemanticProgram, VerseError> {
    let bundle = NativeApiBundle::from_apis(apis);
    analyze_source_with_native_bundle(source, package_name, &bundle)
}

pub(crate) fn analyze_source_with_native_bundle(
    source: &str,
    package_name: Option<&str>,
    bundle: &NativeApiBundle,
) -> Result<SemanticProgram, VerseError> {
    let source = bundle.source_with_digests(source);
    let vst = parse_vst_source(&source)?;
    let desugared = desugar_vst(&vst);
    Checker::new()
        .with_package(package_name.map(str::to_string))
        .with_supported_absolute_using_paths(bundle.absolute_paths())
        .check_desugared_program_to_semantic_program(desugared.into_program())
}

pub fn compile_source(source: &str) -> Result<IrProgram, VerseError> {
    compile_source_in_package(source, None)
}

pub fn compile_source_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<IrProgram, VerseError> {
    let semantic = analyze_source_in_package(source, package_name)?;
    IRGenerator::new().generate(semantic)
}

pub fn compile_source_with_native_apis(
    source: &str,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<IrProgram, VerseError> {
    compile_source_with_native_apis_in_package(source, None, apis)
}

pub fn compile_source_with_native_apis_in_package(
    source: &str,
    package_name: Option<&str>,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<IrProgram, VerseError> {
    let bundle = NativeApiBundle::from_apis(apis);
    compile_source_with_native_bundle(source, package_name, &bundle)
}

pub(crate) fn compile_source_with_native_bundle(
    source: &str,
    package_name: Option<&str>,
    bundle: &NativeApiBundle,
) -> Result<IrProgram, VerseError> {
    let semantic = analyze_source_with_native_bundle(source, package_name, bundle)?;
    IRGenerator::new()
        .with_injected_native_functions(bundle.functions())
        .generate(semantic)
}

pub fn run_source(source: &str) -> Result<Value, VerseError> {
    run_source_in_package(source, None)
}

pub fn run_source_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<Value, VerseError> {
    let ir = compile_source_in_package(source, package_name)?;
    VerseVm::new().run_ir_program(&ir)
}

pub fn run_source_with_native_apis(
    source: &str,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<Value, VerseError> {
    run_source_with_native_apis_in_package(source, None, apis)
}

pub fn run_source_with_native_apis_in_package(
    source: &str,
    package_name: Option<&str>,
    apis: impl IntoIterator<Item = InjectedNativeApi>,
) -> Result<Value, VerseError> {
    let bundle = NativeApiBundle::from_apis(apis);
    run_source_with_native_bundle(source, package_name, &bundle)
}

pub(crate) fn run_source_with_native_bundle(
    source: &str,
    package_name: Option<&str>,
    bundle: &NativeApiBundle,
) -> Result<Value, VerseError> {
    let ir = compile_source_with_native_bundle(source, package_name, bundle)?;
    VerseVm::with_native_registry(bundle.registry().clone()).run_ir_program(&ir)
}

#[cfg(feature = "tokio-host")]
pub fn run_source_with_tokio_host(source: &str) -> Result<Value, VerseError> {
    run_source_with_tokio_host_in_package(source, None)
}

#[cfg(feature = "tokio-host")]
pub fn run_source_with_tokio_host_in_package(
    source: &str,
    package_name: Option<&str>,
) -> Result<Value, VerseError> {
    let ir = compile_source_in_package(source, package_name)?;
    VerseVm::new().run_ir_program_with_tokio_host(&ir)
}
