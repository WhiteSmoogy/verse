use std::collections::{HashMap, HashSet};

use crate::ast::{
    ArchetypeConstructorCall, ArchetypeEntry, AssignOp, BinaryOp, CallArg, CaseArm, CasePattern,
    ClassBlock, ClassMethod, ConcurrentOp, EnumVariant, Expr, ExprKind, ExtensionMethod,
    ForBinding, ForClause, InterpolatedStringPart, Param, ParamPattern, Program, Stmt, StmtKind,
    StructField, TypeAnnotation, TypeName, TypeParam, TypeParamConstraint, UnaryOp,
};
use crate::checker::{IntRange, ParametricTypeKind, Type};
use crate::colors::NAMED_COLORS;
use crate::error::VerseError;
use crate::semantics::{SemanticFacts, SemanticProgram};
use crate::token::{CharacterKind, NumberKind, NumberLiteral, Span};

#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeProgram {
    entry: usize,
    functions: Vec<FunctionDescriptor>,
    chunks: Vec<BytecodeChunk>,
    classes: Vec<ClassDescriptor>,
}

impl BytecodeProgram {
    pub fn new(
        chunks: Vec<BytecodeChunk>,
        functions: Vec<FunctionDescriptor>,
        classes: Vec<ClassDescriptor>,
        entry: usize,
    ) -> Self {
        Self {
            entry,
            functions,
            chunks,
            classes,
        }
    }

    pub fn entry(&self) -> usize {
        self.entry
    }

    pub fn chunks(&self) -> &[BytecodeChunk] {
        &self.chunks
    }

    pub fn functions(&self) -> &[FunctionDescriptor] {
        &self.functions
    }

    pub fn classes(&self) -> &[ClassDescriptor] {
        &self.classes
    }

    pub fn class(&self, name: &str) -> Option<&ClassDescriptor> {
        self.classes.iter().rev().find(|class| class.name == name)
    }

    pub fn entry_chunk(&self) -> &BytecodeChunk {
        &self.chunks[self.entry]
    }

    pub fn uses_runtime_fallback_instruction(&self) -> bool {
        false
    }

    #[cfg(test)]
    pub(crate) fn test_entry(
        instructions: Vec<Instruction>,
        constants: Vec<Constant>,
        register_count: usize,
    ) -> Self {
        Self {
            entry: 0,
            functions: Vec::new(),
            classes: Vec::new(),
            chunks: vec![BytecodeChunk {
                name: "test-entry".to_string(),
                register_count,
                constants,
                instructions,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDescriptor {
    name: String,
    unique: bool,
    base_class: Option<String>,
    interfaces: Vec<String>,
    abstract_class: bool,
    epic_internal_class: bool,
    final_class: bool,
    final_super: bool,
    concrete: bool,
    castable: bool,
    fields: Vec<String>,
    methods: Vec<ClassMethodDescriptor>,
    blocks: Vec<ClassBlockDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Class,
    Struct { computes: bool },
}

impl ClassDescriptor {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn unique(&self) -> bool {
        self.unique
    }

    pub fn base_class(&self) -> Option<&str> {
        self.base_class.as_deref()
    }

    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    pub fn abstract_class(&self) -> bool {
        self.abstract_class
    }

    pub fn epic_internal_class(&self) -> bool {
        self.epic_internal_class
    }

    pub fn final_class(&self) -> bool {
        self.final_class
    }

    pub fn final_super(&self) -> bool {
        self.final_super
    }

    pub fn concrete(&self) -> bool {
        self.concrete
    }

    pub fn castable(&self) -> bool {
        self.castable
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn methods(&self) -> &[ClassMethodDescriptor] {
        &self.methods
    }

    pub fn blocks(&self) -> &[ClassBlockDescriptor] {
        &self.blocks
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMethodDescriptor {
    qualifier: Option<String>,
    name: String,
    params: Vec<String>,
    param_types: Vec<Option<TypeName>>,
    function: usize,
    field_count: usize,
    decides: bool,
}

impl ClassMethodDescriptor {
    pub fn qualifier(&self) -> Option<&str> {
        self.qualifier.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &[String] {
        &self.params
    }

    pub fn param_types(&self) -> &[Option<TypeName>] {
        &self.param_types
    }

    pub fn function(&self) -> usize {
        self.function
    }

    pub fn field_count(&self) -> usize {
        self.field_count
    }

    pub fn decides(&self) -> bool {
        self.decides
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassBlockDescriptor {
    function: usize,
    field_count: usize,
}

impl ClassBlockDescriptor {
    pub fn function(&self) -> usize {
        self.function
    }

    pub fn field_count(&self) -> usize {
        self.field_count
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDescriptor {
    name: Option<String>,
    params: Vec<String>,
    source_params: Vec<Param>,
    param_types: Vec<Option<Type>>,
    param_defaults: Vec<Option<Expr>>,
    chunk: usize,
    decides: bool,
}

impl FunctionDescriptor {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn params(&self) -> &[String] {
        &self.params
    }

    pub fn source_params(&self) -> &[Param] {
        &self.source_params
    }

    pub fn param_types(&self) -> &[Option<Type>] {
        &self.param_types
    }

    pub fn param_defaults(&self) -> &[Option<Expr>] {
        &self.param_defaults
    }

    pub fn chunk(&self) -> usize {
        self.chunk
    }

    pub fn decides(&self) -> bool {
        self.decides
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeChunk {
    name: String,
    register_count: usize,
    constants: Vec<Constant>,
    instructions: Vec<Instruction>,
}

impl BytecodeChunk {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            register_count: RegisterIndex::PARAMETER_START.0,
            constants: Vec::new(),
            instructions: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn register_count(&self) -> usize {
        self.register_count
    }

    pub fn constants(&self) -> &[Constant] {
        &self.constants
    }

    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    fn set_register_count(&mut self, register_count: usize) {
        self.register_count = register_count;
    }

    fn push_constant(&mut self, constant: Constant) -> usize {
        self.constants.push(constant);
        self.constants.len() - 1
    }

    fn emit(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
    }

    fn emit_jump(&mut self, instruction: Instruction) -> usize {
        self.instructions.push(instruction);
        self.instructions.len() - 1
    }

    fn patch_jump(&mut self, index: usize, target: usize) {
        match self
            .instructions
            .get_mut(index)
            .expect("jump patch index should be valid")
        {
            Instruction::Jump { jump_offset } => *jump_offset = target,
            Instruction::JumpIfInitialized { jump_offset, .. } => *jump_offset = target,
            Instruction::BeginFailureContext { on_failure, .. }
            | Instruction::BeginTask {
                on_yield: on_failure,
                ..
            }
            | Instruction::CanFastAppendToArrayFastFail { on_failure, .. }
            | Instruction::ArrayIndexFastFail { on_failure, .. }
            | Instruction::EqFastFail { on_failure, .. }
            | Instruction::NeqFastFail { on_failure, .. }
            | Instruction::LtFastFail { on_failure, .. }
            | Instruction::LteFastFail { on_failure, .. }
            | Instruction::GtFastFail { on_failure, .. }
            | Instruction::GteFastFail { on_failure, .. }
            | Instruction::QueryFastFail { on_failure, .. } => *on_failure = target,
            Instruction::EndFailureContext { done, .. }
            | Instruction::EndFastFailureContext { on_done: done, .. }
            | Instruction::Yield {
                resume_offset: done,
                ..
            } => *done = target,
            _ => panic!("jump patch index must point at a jump instruction"),
        }
    }

    fn next_instruction_index(&self) -> usize {
        self.instructions.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Char {
        value: char,
        kind: CharacterKind,
    },
    Bool(bool),
    String(String),
    None,
    Type(TypeName),
    Option(Option<Box<Constant>>),
    Range {
        start: i64,
        end: i64,
    },
    Tuple(Vec<Constant>),
    EnumValue {
        enum_name: String,
        variant: String,
    },
    Function(usize),
    NativeFunction(String),
    StructType {
        name: String,
        computes: bool,
    },
    ClassType {
        name: String,
        base: Option<String>,
        interfaces: Vec<String>,
        unique: bool,
        abstract_class: bool,
        epic_internal_class: bool,
        final_class: bool,
        final_super: bool,
        concrete: bool,
        castable: bool,
    },
    InterfaceType {
        name: String,
    },
    ParametricType {
        name: String,
        params: Vec<String>,
    },
    External(TypeName),
    GlobalRef(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegisterIndex(pub usize);

impl RegisterIndex {
    pub const SELF: Self = Self(0);
    pub const SCOPE: Self = Self(1);
    pub const PARAMETER_START: Self = Self(2);

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueOperand {
    Register(RegisterIndex),
    Constant(usize),
    Uninitialized,
}

// This list is copied from Unreal's VerseVMBytecodeGenerator.cs DefineOps()
// and mirrors the generated C++ EOpcode names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    CanFastAppendToArrayFastFail,
    FastAppendToArray,
    MutableAdd,
    Neg,
    Query,
    Err,
    Tracepoint,
    Move,
    MoveTrailed,
    MoveNonComparable,
    Reset,
    ResetNonTrailed,
    Jump,
    JumpIfInitialized,
    Switch,
    LtFastFail,
    LteFastFail,
    GtFastFail,
    GteFastFail,
    EqFastFail,
    NeqFastFail,
    ArrayIndexFastFail,
    TypeCastFastFail,
    QueryFastFail,
    EndFastFailureContext,
    BeginFailureContext,
    EndFailureContext,
    SelfTask,
    BeginTask,
    CallTask,
    EndTask,
    BeginAwait,
    AwaitSuccess,
    EndAwait,
    BeginBatch,
    EndBatch,
    Yield,
    NewSemaphore,
    WaitSemaphore,
    Call,
    CallWithSelf,
    Return,
    ReturnTrailed,
    ResumeUnwind,
    NewRef,
    NewPersistentOrSessionWeakMapRef,
    RefGet,
    RefSet,
    RefSetLive,
    RefCallDomain,
    Freeze,
    FreezeIfAccessor,
    Melt,
    Length,
    LengthWithEffects,
    CallSet,
    CallSetLive,
    NewArray,
    NewMutableArray,
    NewMutableArrayWithCapacity,
    ArrayAdd,
    InPlaceMakeImmutable,
    NewOption,
    NewUnionVariant,
    GetUnionVariantPayload,
    GetUnionVariantTag,
    NewMap,
    MapKey,
    MapValue,
    NewClass,
    BindNativeClass,
    ConstructNativeDefaultObject,
    LoadImport,
    JumpIfDefaultSubObject,
    BeginModule,
    EndModule,
    EndModuleData,
    NewObject,
    NewObjectICClass,
    LoadField,
    LoadFieldICOffset,
    LoadFieldICConstant,
    LoadFieldICFunction,
    LoadFieldICNativeFunction,
    LoadFieldICAccessor,
    LoadFieldFromSuper,
    CreateField,
    CreateFieldICValueObjectConstant,
    CreateFieldICValueObjectField,
    CreateFieldICNativeStruct,
    CreateFieldICUObject,
    UnifyField,
    InitializeVar,
    SetField,
    SetFieldLive,
    UnifyNativeObject,
    UnwrapNativeConstructorWrapper,
    NewScope,
    NewFunction,
    LoadParentScope,
    LoadCapture,
    BeginProfileBlock,
    EndProfileBlock,
    LoadConstructor,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Move {
        dest: RegisterIndex,
        source: ValueOperand,
        span: Span,
    },
    MoveTrailed {
        dest: RegisterIndex,
        source: ValueOperand,
        span: Span,
    },
    MoveNonComparable {
        dest: RegisterIndex,
        source: ValueOperand,
        span: Span,
    },
    Reset {
        dest: RegisterIndex,
        span: Span,
    },
    ResetNonTrailed {
        dest: RegisterIndex,
        span: Span,
    },
    Jump {
        jump_offset: usize,
    },
    JumpIfInitialized {
        source: ValueOperand,
        jump_offset: usize,
        span: Span,
    },
    Switch {
        which: ValueOperand,
        jump_offsets: Vec<usize>,
        span: Span,
    },
    Err {
        span: Span,
    },
    Tracepoint {
        name: String,
        span: Span,
    },
    BeginFailureContext {
        on_failure: usize,
        id: usize,
        span: Span,
    },
    EndFailureContext {
        done: usize,
        id: usize,
        span: Span,
    },
    EndFastFailureContext {
        outer_leniency_indicator: RegisterIndex,
        leniency_indicator: ValueOperand,
        on_done: usize,
        span: Span,
    },
    BeginTask {
        dest: RegisterIndex,
        parent: ValueOperand,
        add_to_task_group: bool,
        on_yield: usize,
        span: Span,
    },
    EndTask {
        write: Option<RegisterIndex>,
        switch: Option<RegisterIndex>,
        value: ValueOperand,
        which: ValueOperand,
        signal: Option<ValueOperand>,
        span: Span,
    },
    BeginAwait {
        span: Span,
    },
    AwaitSuccess {
        span: Span,
    },
    EndAwait {
        span: Span,
    },
    Yield {
        resume_offset: usize,
        span: Span,
    },
    NewSemaphore {
        dest: RegisterIndex,
        span: Span,
    },
    WaitSemaphore {
        source: ValueOperand,
        count: i32,
        span: Span,
    },
    BeginBatch {
        span: Span,
    },
    EndBatch {
        span: Span,
    },
    Add {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Sub {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Mul {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Div {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Mod {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    CanFastAppendToArrayFastFail {
        leniency_indicator: RegisterIndex,
        ref_value: ValueOperand,
        maybe_mutable_array: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    FastAppendToArray {
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    MutableAdd {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Neg {
        dest: RegisterIndex,
        source: ValueOperand,
        span: Span,
    },
    Query {
        dest: RegisterIndex,
        source: ValueOperand,
        span: Span,
    },
    Neq {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Lt {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Lte {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Gt {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    Gte {
        dest: RegisterIndex,
        left_source: ValueOperand,
        right_source: ValueOperand,
        span: Span,
    },
    EqFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    NeqFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    LtFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    LteFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    GtFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    GteFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        lhs: ValueOperand,
        rhs: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    ArrayIndexFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        array: ValueOperand,
        index: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    QueryFastFail {
        dest: RegisterIndex,
        leniency_indicator: RegisterIndex,
        source: ValueOperand,
        on_failure: usize,
        span: Span,
    },
    Call {
        dest: RegisterIndex,
        callee: ValueOperand,
        arguments: Vec<ValueOperand>,
        named_arguments: Vec<String>,
        named_argument_values: Vec<ValueOperand>,
        callee_yields: bool,
        span: Span,
    },
    CallTask {
        dest: RegisterIndex,
        parent: ValueOperand,
        callee: ValueOperand,
        arguments: Vec<ValueOperand>,
        span: Span,
    },
    CallSet {
        container: ValueOperand,
        index: ValueOperand,
        value_to_set: ValueOperand,
        span: Span,
    },
    Return {
        value: ValueOperand,
        span: Span,
    },
    ReturnTrailed {
        value: ValueOperand,
        span: Span,
    },
    NewRef {
        dest: RegisterIndex,
        domain: Option<ValueOperand>,
        span: Span,
    },
    RefGet {
        dest: RegisterIndex,
        ref_value: ValueOperand,
        span: Span,
    },
    RefSet {
        ref_value: ValueOperand,
        value: ValueOperand,
        span: Span,
    },
    RefSetLive {
        ref_value: ValueOperand,
        value: ValueOperand,
        task: ValueOperand,
        span: Span,
    },
    Freeze {
        dest: RegisterIndex,
        value: ValueOperand,
        span: Span,
    },
    FreezeIfAccessor {
        dest: RegisterIndex,
        value: ValueOperand,
        span: Span,
    },
    Melt {
        dest: RegisterIndex,
        value: ValueOperand,
        span: Span,
    },
    NewArray {
        dest: RegisterIndex,
        values: Vec<ValueOperand>,
        span: Span,
    },
    NewMutableArray {
        dest: RegisterIndex,
        values: Vec<ValueOperand>,
        span: Span,
    },
    NewMutableArrayWithCapacity {
        dest: RegisterIndex,
        size: ValueOperand,
        span: Span,
    },
    ArrayAdd {
        dest: RegisterIndex,
        container: ValueOperand,
        value_to_add: ValueOperand,
        span: Span,
    },
    InPlaceMakeImmutable {
        dest: RegisterIndex,
        container: ValueOperand,
        span: Span,
    },
    Length {
        dest: RegisterIndex,
        container: ValueOperand,
        span: Span,
    },
    LengthWithEffects {
        dest: RegisterIndex,
        container: ValueOperand,
        span: Span,
    },
    CallSetLive {
        container: ValueOperand,
        index: ValueOperand,
        value_to_set: ValueOperand,
        task: ValueOperand,
        span: Span,
    },
    NewOption {
        dest: RegisterIndex,
        value: ValueOperand,
        span: Span,
    },
    NewMap {
        dest: RegisterIndex,
        keys: Vec<ValueOperand>,
        values: Vec<ValueOperand>,
        span: Span,
    },
    NewObject {
        dest: RegisterIndex,
        class_name: String,
        object_kind: ObjectKind,
        fields: Vec<(String, bool, ValueOperand)>,
        span: Span,
    },
    LoadField {
        dest: RegisterIndex,
        object: ValueOperand,
        name: String,
        span: Span,
    },
    LoadFieldFromSuper {
        dest: RegisterIndex,
        object: ValueOperand,
        base_class: String,
        name: String,
        span: Span,
    },
    SetField {
        object: ValueOperand,
        name: String,
        value: ValueOperand,
        span: Span,
    },
    MapKey {
        dest: RegisterIndex,
        map: ValueOperand,
        index: ValueOperand,
        span: Span,
    },
    MapValue {
        dest: RegisterIndex,
        map: ValueOperand,
        index: ValueOperand,
        span: Span,
    },
    NewFunction {
        dest: RegisterIndex,
        procedure: ValueOperand,
        self_value: ValueOperand,
        parent_scope: ValueOperand,
        span: Span,
    },
    NewScope {
        dest: RegisterIndex,
        values: Vec<ValueOperand>,
        span: Span,
    },
    LoadCapture {
        dest: RegisterIndex,
        scope: ValueOperand,
        index: usize,
        span: Span,
    },
    BeginProfileBlock {
        dest: RegisterIndex,
        span: Span,
    },
    EndProfileBlock {
        wall_time_start: ValueOperand,
        user_tag: ValueOperand,
        snippet_path: String,
        begin_row: ValueOperand,
        begin_column: ValueOperand,
        end_row: ValueOperand,
        end_column: ValueOperand,
        span: Span,
    },
}

impl Instruction {
    pub fn opcode(&self) -> Opcode {
        match self {
            Instruction::Move { .. } => Opcode::Move,
            Instruction::MoveTrailed { .. } => Opcode::MoveTrailed,
            Instruction::MoveNonComparable { .. } => Opcode::MoveNonComparable,
            Instruction::Reset { .. } => Opcode::Reset,
            Instruction::ResetNonTrailed { .. } => Opcode::ResetNonTrailed,
            Instruction::Jump { .. } => Opcode::Jump,
            Instruction::JumpIfInitialized { .. } => Opcode::JumpIfInitialized,
            Instruction::Switch { .. } => Opcode::Switch,
            Instruction::Err { .. } => Opcode::Err,
            Instruction::Tracepoint { .. } => Opcode::Tracepoint,
            Instruction::BeginFailureContext { .. } => Opcode::BeginFailureContext,
            Instruction::EndFailureContext { .. } => Opcode::EndFailureContext,
            Instruction::EndFastFailureContext { .. } => Opcode::EndFastFailureContext,
            Instruction::BeginTask { .. } => Opcode::BeginTask,
            Instruction::EndTask { .. } => Opcode::EndTask,
            Instruction::BeginAwait { .. } => Opcode::BeginAwait,
            Instruction::AwaitSuccess { .. } => Opcode::AwaitSuccess,
            Instruction::EndAwait { .. } => Opcode::EndAwait,
            Instruction::Yield { .. } => Opcode::Yield,
            Instruction::NewSemaphore { .. } => Opcode::NewSemaphore,
            Instruction::WaitSemaphore { .. } => Opcode::WaitSemaphore,
            Instruction::BeginBatch { .. } => Opcode::BeginBatch,
            Instruction::EndBatch { .. } => Opcode::EndBatch,
            Instruction::Add { .. } => Opcode::Add,
            Instruction::Sub { .. } => Opcode::Sub,
            Instruction::Mul { .. } => Opcode::Mul,
            Instruction::Div { .. } => Opcode::Div,
            Instruction::Mod { .. } => Opcode::Mod,
            Instruction::CanFastAppendToArrayFastFail { .. } => {
                Opcode::CanFastAppendToArrayFastFail
            }
            Instruction::FastAppendToArray { .. } => Opcode::FastAppendToArray,
            Instruction::MutableAdd { .. } => Opcode::MutableAdd,
            Instruction::Neg { .. } => Opcode::Neg,
            Instruction::Query { .. } => Opcode::Query,
            Instruction::Neq { .. } => Opcode::Neq,
            Instruction::Lt { .. } => Opcode::Lt,
            Instruction::Lte { .. } => Opcode::Lte,
            Instruction::Gt { .. } => Opcode::Gt,
            Instruction::Gte { .. } => Opcode::Gte,
            Instruction::EqFastFail { .. } => Opcode::EqFastFail,
            Instruction::NeqFastFail { .. } => Opcode::NeqFastFail,
            Instruction::LtFastFail { .. } => Opcode::LtFastFail,
            Instruction::LteFastFail { .. } => Opcode::LteFastFail,
            Instruction::GtFastFail { .. } => Opcode::GtFastFail,
            Instruction::GteFastFail { .. } => Opcode::GteFastFail,
            Instruction::ArrayIndexFastFail { .. } => Opcode::ArrayIndexFastFail,
            Instruction::QueryFastFail { .. } => Opcode::QueryFastFail,
            Instruction::Call { .. } => Opcode::Call,
            Instruction::CallTask { .. } => Opcode::CallTask,
            Instruction::CallSet { .. } => Opcode::CallSet,
            Instruction::CallSetLive { .. } => Opcode::CallSetLive,
            Instruction::Return { .. } => Opcode::Return,
            Instruction::ReturnTrailed { .. } => Opcode::ReturnTrailed,
            Instruction::NewRef { .. } => Opcode::NewRef,
            Instruction::RefGet { .. } => Opcode::RefGet,
            Instruction::RefSet { .. } => Opcode::RefSet,
            Instruction::RefSetLive { .. } => Opcode::RefSetLive,
            Instruction::Freeze { .. } => Opcode::Freeze,
            Instruction::FreezeIfAccessor { .. } => Opcode::FreezeIfAccessor,
            Instruction::Melt { .. } => Opcode::Melt,
            Instruction::NewArray { .. } => Opcode::NewArray,
            Instruction::NewMutableArray { .. } => Opcode::NewMutableArray,
            Instruction::NewMutableArrayWithCapacity { .. } => Opcode::NewMutableArrayWithCapacity,
            Instruction::ArrayAdd { .. } => Opcode::ArrayAdd,
            Instruction::InPlaceMakeImmutable { .. } => Opcode::InPlaceMakeImmutable,
            Instruction::Length { .. } => Opcode::Length,
            Instruction::LengthWithEffects { .. } => Opcode::LengthWithEffects,
            Instruction::NewOption { .. } => Opcode::NewOption,
            Instruction::NewMap { .. } => Opcode::NewMap,
            Instruction::NewObject { .. } => Opcode::NewObject,
            Instruction::LoadField { .. } => Opcode::LoadField,
            Instruction::LoadFieldFromSuper { .. } => Opcode::LoadFieldFromSuper,
            Instruction::SetField { .. } => Opcode::SetField,
            Instruction::MapKey { .. } => Opcode::MapKey,
            Instruction::MapValue { .. } => Opcode::MapValue,
            Instruction::NewFunction { .. } => Opcode::NewFunction,
            Instruction::NewScope { .. } => Opcode::NewScope,
            Instruction::LoadCapture { .. } => Opcode::LoadCapture,
            Instruction::BeginProfileBlock { .. } => Opcode::BeginProfileBlock,
            Instruction::EndProfileBlock { .. } => Opcode::EndProfileBlock,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BytecodeGenerator;

impl BytecodeGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(self, semantic: &SemanticProgram) -> Result<BytecodeProgram, VerseError> {
        let program = &semantic.program;
        let mut lowerer = Lowerer::new(semantic);
        if lowerer.lower_program(program).is_err() {
            return Err(VerseError::check(
                "bytecode generation does not support this construct yet",
            ));
        }

        Ok(lowerer.finish())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnsupportedBytecode;

#[derive(Debug, Clone, Copy)]
struct Binding {
    operand: ValueOperand,
    mutable: bool,
    ref_backed: bool,
    iterable_kind: Option<IterableKind>,
}

#[derive(Debug, Clone)]
struct CaptureBinding {
    name: String,
    binding: Binding,
    callable: bool,
}

#[derive(Debug, Clone, Copy)]
struct GlobalBinding {
    mutable: bool,
    iterable_kind: Option<IterableKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterableKind {
    Indexed,
    Map,
}

struct SlotStep<'expr> {
    index: &'expr Expr,
    span: Span,
}

struct SlotTarget<'expr> {
    base: &'expr Expr,
    steps: Vec<SlotStep<'expr>>,
}

struct SlotWriteback {
    container: ValueOperand,
    index: ValueOperand,
    value: ValueOperand,
    span: Span,
}

enum FieldWriteback {
    Ref {
        ref_value: ValueOperand,
        value: ValueOperand,
        span: Span,
    },
    Field {
        object: ValueOperand,
        name: String,
        value: ValueOperand,
        span: Span,
    },
}

struct LoweredSlotAccess {
    container: ValueOperand,
    read_container: ValueOperand,
    index: ValueOperand,
    writebacks: Vec<SlotWriteback>,
}

fn iterable_kind_from_type(value_type: &Type) -> Option<IterableKind> {
    match value_type {
        Type::Array(_) | Type::String | Type::Generator(_) => Some(IterableKind::Indexed),
        Type::Map(_, _) => Some(IterableKind::Map),
        _ => None,
    }
}

fn iterable_kind_from_type_name(value_type: &TypeName) -> Option<IterableKind> {
    match value_type {
        TypeName::Array(_) | TypeName::String => Some(IterableKind::Indexed),
        TypeName::Applied { name, .. } if name == "generator" => Some(IterableKind::Indexed),
        TypeName::Map(_, _) => Some(IterableKind::Map),
        _ => None,
    }
}

fn type_needs_binding_freeze(value_type: &Type) -> bool {
    matches!(
        value_type,
        Type::Array(_) | Type::Map(_, _) | Type::Tuple(_)
    )
}

struct ChunkState {
    chunk: BytecodeChunk,
    next_register: usize,
    scopes: Vec<HashMap<String, Binding>>,
    callable_scopes: Vec<HashSet<String>>,
    imports: Vec<String>,
    extensions: Vec<ExtensionBinding>,
    loop_breaks: Vec<LoopBreakContext>,
    defer_scope_depth: usize,
    super_class: Option<String>,
    last_value: ValueOperand,
}

struct LoopBreakContext {
    breaks: Vec<usize>,
    defer_scope_depth: usize,
}

#[derive(Debug, Clone)]
struct ExtensionBinding {
    name: String,
    module: Option<String>,
    function: usize,
    receiver_type: Option<Type>,
    params: Vec<String>,
    param_types: Vec<Option<Type>>,
    param_defaults: Vec<Option<Expr>>,
    source_params: Vec<Param>,
    fields: Vec<String>,
    captures_self: bool,
}

impl ChunkState {
    fn new(name: impl Into<String>) -> Self {
        Self {
            chunk: BytecodeChunk::new(name),
            next_register: RegisterIndex::PARAMETER_START.0,
            scopes: vec![HashMap::new()],
            callable_scopes: vec![HashSet::new()],
            imports: Vec::new(),
            extensions: Vec::new(),
            loop_breaks: Vec::new(),
            defer_scope_depth: 0,
            super_class: None,
            last_value: ValueOperand::Uninitialized,
        }
    }

    fn with_params(name: impl Into<String>, params: &[Param], facts: &SemanticFacts) -> Self {
        let mut state = Self::new(name);
        define_flattened_param_bindings(
            &mut state,
            params,
            facts,
            RegisterIndex::PARAMETER_START.0,
        );
        state
    }

    fn with_class_method_context(
        name: impl Into<String>,
        fields: &[StructField],
        params: &[Param],
        super_class: Option<&str>,
        facts: &SemanticFacts,
    ) -> Self {
        let mut state = Self::new(name);
        state.super_class = super_class.map(str::to_string);
        let self_register = RegisterIndex(RegisterIndex::PARAMETER_START.0);
        state.next_register = state.next_register.max(self_register.0 + 1);
        state.define(
            "Self".to_string(),
            Binding {
                operand: ValueOperand::Register(self_register),
                mutable: false,
                ref_backed: false,
                iterable_kind: None,
            },
        );

        for (index, field) in fields.iter().enumerate() {
            let register = RegisterIndex(RegisterIndex::PARAMETER_START.0 + 1 + index);
            state.next_register = state.next_register.max(register.0 + 1);
            state.define(
                field.name.clone(),
                Binding {
                    operand: ValueOperand::Register(register),
                    mutable: true,
                    ref_backed: true,
                    iterable_kind: field
                        .annotation
                        .as_ref()
                        .and_then(|annotation| iterable_kind_from_type_name(&annotation.name)),
                },
            );
        }

        let param_offset = RegisterIndex::PARAMETER_START.0 + 1 + fields.len();
        define_flattened_param_bindings(&mut state, params, facts, param_offset);
        state
    }

    fn with_class_extension_context(
        name: impl Into<String>,
        fields: &[StructField],
        receiver: &Param,
        params: &[Param],
        super_class: Option<&str>,
        facts: &SemanticFacts,
    ) -> Self {
        let mut state = Self::with_class_method_context(name, fields, &[], super_class, facts);
        let receiver_register = RegisterIndex(RegisterIndex::PARAMETER_START.0 + 1 + fields.len());
        state.next_register = state.next_register.max(receiver_register.0 + 1);
        state.define(
            receiver.name.clone(),
            Binding {
                operand: ValueOperand::Register(receiver_register),
                mutable: false,
                ref_backed: false,
                iterable_kind: receiver
                    .annotation
                    .as_ref()
                    .and_then(|annotation| iterable_kind_from_type_name(&annotation.name)),
            },
        );
        if receiver
            .annotation
            .as_ref()
            .map(|annotation| bytecode_type_from_annotation(annotation))
            .as_ref()
            .is_some_and(bytecode_type_is_callable)
        {
            state.mark_callable(&receiver.name);
        }

        let param_offset = receiver_register.0 + 1;
        define_flattened_param_bindings(&mut state, params, facts, param_offset);
        state
    }

    fn finish(mut self) -> BytecodeChunk {
        self.chunk.set_register_count(self.next_register);
        self.chunk
    }

    fn allocate_register(&mut self, span: Span) -> RegisterIndex {
        let register = RegisterIndex(self.next_register);
        self.next_register += 1;
        self.chunk.emit(Instruction::Reset {
            dest: register,
            span,
        });
        register
    }

    fn constant(&mut self, constant: Constant) -> ValueOperand {
        ValueOperand::Constant(self.chunk.push_constant(constant))
    }

    fn none(&mut self) -> ValueOperand {
        self.constant(Constant::None)
    }

    fn define(&mut self, name: String, binding: Binding) {
        self.scopes
            .last_mut()
            .expect("there should always be a current scope")
            .insert(name, binding);
    }

    fn mark_callable(&mut self, name: &str) {
        self.callable_scopes
            .last_mut()
            .expect("there should always be a current callable scope")
            .insert(name.to_string());
    }

    fn lookup_callable(&self, name: &str) -> bool {
        self.callable_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
            || self.imports.iter().rev().any(|module| {
                let qualified = format!("{module}.{name}");
                self.callable_scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.contains(&qualified))
            })
    }

    fn define_global(&mut self, name: String, binding: Binding) {
        self.scopes
            .first_mut()
            .expect("there should always be a root scope")
            .insert(name, binding);
    }

    fn define_global_if_absent(&mut self, name: String, binding: Binding) {
        self.scopes
            .first_mut()
            .expect("there should always be a root scope")
            .entry(name)
            .or_insert(binding);
    }

    fn is_entry_root(&self) -> bool {
        self.chunk.name() == "entry" && self.scopes.len() == 1
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        let direct = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied());
        if direct.is_some() {
            return direct;
        }

        self.imports.iter().rev().find_map(|module| {
            let qualified = format!("{module}.{name}");
            self.scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(&qualified).copied())
        })
    }

    fn import(&mut self, module: String) {
        if !self.imports.iter().any(|existing| existing == &module) {
            self.imports.push(module);
        }
    }

    fn define_extension(&mut self, extension: ExtensionBinding) {
        self.extensions.push(extension);
    }

    fn lookup_extensions(&self, name: &str) -> Vec<ExtensionBinding> {
        self.extensions
            .iter()
            .rev()
            .filter(|extension| {
                extension.name == name
                    && (extension.module.is_none()
                        || extension.module.as_ref().is_some_and(|module| {
                            self.imports.iter().any(|import| import == module)
                        }))
            })
            .cloned()
            .collect()
    }

    fn lookup_qualified_extensions(&self, qualifier: &str, name: &str) -> Vec<ExtensionBinding> {
        self.extensions
            .iter()
            .rev()
            .filter(|extension| {
                extension.name == name
                    && extension.module.as_ref().is_some_and(|module| {
                        module == qualifier
                            || self.imports.iter().any(|import| {
                                import == module && import.rsplit('.').next() == Some(qualifier)
                            })
                    })
            })
            .cloned()
            .collect()
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.callable_scopes.push(HashSet::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
        self.callable_scopes.pop();
    }

    fn emit_native_call_no_args(&mut self, name: &str, span: Span) {
        let dest = self.allocate_register(span);
        let callee = self.constant(Constant::NativeFunction(name.to_string()));
        self.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments: Vec::new(),
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: false,
            span,
        });
    }

    fn begin_defer_scope(&mut self, span: Span) {
        self.emit_native_call_no_args("__verse_begin_defer_scope", span);
        self.defer_scope_depth += 1;
    }

    fn end_defer_scope(&mut self, span: Span) {
        if self.defer_scope_depth == 0 {
            return;
        }
        self.emit_native_call_no_args("__verse_end_defer_scope", span);
        self.defer_scope_depth -= 1;
    }

    fn emit_defer_scope_exits(&mut self, count: usize, span: Span) {
        for _ in 0..count {
            self.emit_native_call_no_args("__verse_end_defer_scope", span);
        }
    }
}

struct Lowerer<'semantic> {
    callable_names: HashSet<String>,
    facts: &'semantic SemanticFacts,
    chunks: Vec<BytecodeChunk>,
    functions: Vec<FunctionDescriptor>,
    classes: Vec<ClassDescriptor>,
    class_layouts: HashMap<String, ClassLayout>,
    enum_layouts: HashMap<String, EnumLayout>,
    interface_layouts: HashMap<String, InterfaceLayout>,
    type_functions: HashMap<String, Vec<BytecodeTypeFunction>>,
    function_return_classes: HashMap<(String, usize), String>,
    global_bindings: HashMap<String, GlobalBinding>,
    next_failure_context_id: usize,
}

#[derive(Debug, Clone)]
struct BytecodeTypeFunction {
    params: Vec<BytecodeTypeFunctionParam>,
    target: TypeName,
}

#[derive(Debug, Clone)]
struct BytecodeTypeFunctionParam {
    name: String,
    constraint: TypeName,
}

#[derive(Debug, Clone)]
struct ClassLayout {
    runtime_name: String,
    base_class: Option<String>,
    interfaces: Vec<String>,
    object_kind: ObjectKind,
    unique: bool,
    abstract_class: bool,
    epic_internal_class: bool,
    final_class: bool,
    final_super: bool,
    concrete: bool,
    castable: bool,
    fields: Vec<StructField>,
}

impl ClassLayout {
    fn class_type_constant(&self) -> Constant {
        Constant::ClassType {
            name: self.runtime_name.clone(),
            base: self.base_class.clone(),
            interfaces: self.interfaces.clone(),
            unique: self.unique,
            abstract_class: self.abstract_class,
            epic_internal_class: self.epic_internal_class,
            final_class: self.final_class,
            final_super: self.final_super,
            concrete: self.concrete,
            castable: self.castable,
        }
    }
}

#[derive(Debug, Clone)]
struct EnumLayout {
    runtime_name: String,
    variants: HashSet<String>,
}

#[derive(Debug, Clone)]
struct InterfaceLayout {
    runtime_name: String,
    fields: Vec<StructField>,
    methods: Vec<ClassMethod>,
}

const BUILTIN_RUNTIME_CLASS_NAMES: &[&str] = &[
    "diagnostic",
    "entity",
    "component",
    "tag",
    "session",
    "player",
    "agent",
    "team",
];

fn builtin_class_layout(name: &str) -> ClassLayout {
    ClassLayout {
        runtime_name: name.to_string(),
        base_class: None,
        interfaces: Vec::new(),
        object_kind: ObjectKind::Class,
        unique: false,
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        final_super: false,
        concrete: false,
        castable: false,
        fields: Vec::new(),
    }
}

fn builtin_class_descriptor(name: &str) -> ClassDescriptor {
    ClassDescriptor {
        name: name.to_string(),
        unique: false,
        base_class: None,
        interfaces: Vec::new(),
        abstract_class: false,
        epic_internal_class: false,
        final_class: false,
        final_super: false,
        concrete: false,
        castable: false,
        fields: Vec::new(),
        methods: Vec::new(),
        blocks: Vec::new(),
    }
}

impl<'semantic> Lowerer<'semantic> {
    fn new(semantic: &'semantic SemanticProgram) -> Self {
        let mut class_layouts = HashMap::new();
        for name in BUILTIN_RUNTIME_CLASS_NAMES {
            class_layouts.insert((*name).to_string(), builtin_class_layout(name));
        }
        class_layouts.insert(
            "locale".to_string(),
            ClassLayout {
                runtime_name: "locale".to_string(),
                base_class: None,
                interfaces: Vec::new(),
                object_kind: ObjectKind::Struct { computes: false },
                unique: false,
                abstract_class: false,
                epic_internal_class: false,
                final_class: false,
                final_super: false,
                concrete: false,
                castable: false,
                fields: Vec::new(),
            },
        );
        Self {
            callable_names: collect_top_level_function_names(&semantic.program),
            facts: &semantic.facts,
            chunks: Vec::new(),
            functions: Vec::new(),
            classes: BUILTIN_RUNTIME_CLASS_NAMES
                .iter()
                .map(|name| builtin_class_descriptor(name))
                .collect(),
            class_layouts,
            enum_layouts: HashMap::new(),
            interface_layouts: HashMap::new(),
            type_functions: HashMap::new(),
            function_return_classes: HashMap::new(),
            global_bindings: HashMap::new(),
            next_failure_context_id: 0,
        }
    }

    fn finish(self) -> BytecodeProgram {
        BytecodeProgram::new(self.chunks, self.functions, self.classes, 0)
    }

    fn predeclare_static_type_functions(&mut self, statements: &[Stmt], namespace: Option<&str>) {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let runtime_name = namespace
                .map(|namespace| format!("{namespace}.{name}"))
                .unwrap_or_else(|| name.clone());
            match &expr.kind {
                ExprKind::Function {
                    params,
                    return_type,
                    body,
                    ..
                } => {
                    if static_type_function_return_type(return_type.as_ref())
                        && let Some(type_function_params) =
                            params.iter().map(bytecode_type_function_param).collect()
                        && let Some(target) = bytecode_expr_to_type_name(body)
                    {
                        let info = BytecodeTypeFunction {
                            params: type_function_params,
                            target,
                        };
                        self.type_functions
                            .entry(name.clone())
                            .or_default()
                            .push(info.clone());
                        self.type_functions
                            .entry(runtime_name)
                            .or_default()
                            .push(info);
                    }
                }
                ExprKind::ModuleDefinition { statements, .. } => {
                    self.predeclare_static_type_functions(statements, Some(&runtime_name));
                }
                _ => {}
            }
        }
    }

    fn lower_program(&mut self, program: &Program) -> Result<(), UnsupportedBytecode> {
        self.predeclare_static_type_functions(&program.statements, None);
        self.predeclare_runtime_type_layouts(&program.statements, None)?;
        self.predeclare_global_bindings(&program.statements, None);
        let mut state = ChunkState::new("entry");
        self.install_global_bindings(&mut state);
        state.last_value = state.none();
        let program_span = program
            .statements
            .last()
            .map_or_else(|| Span::new(0, 0, 1, 1), |statement| statement.span);
        state.begin_defer_scope(program_span);
        for statement in &program.statements {
            self.lower_statement(statement, &mut state)?;
        }
        state.end_defer_scope(program_span);
        state.chunk.emit(Instruction::Return {
            value: state.last_value,
            span: program_span,
        });
        self.chunks.insert(0, state.finish());
        Ok(())
    }

    fn predeclare_global_bindings(&mut self, statements: &[Stmt], namespace: Option<&str>) {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    annotation,
                    expr,
                    ..
                } => {
                    let runtime_name = namespace
                        .map(|namespace| format!("{namespace}.{name}"))
                        .unwrap_or_else(|| name.clone());
                    if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
                        self.predeclare_global_bindings(statements, Some(&runtime_name));
                    } else if should_predeclare_runtime_global_let(annotation.as_ref(), expr) {
                        self.predeclare_global_binding(
                            runtime_name,
                            false,
                            self.iterable_kind_for_binding_span(statement.span),
                        );
                    }
                }
                StmtKind::Var { name, .. } => {
                    let runtime_name = namespace
                        .map(|namespace| format!("{namespace}.{name}"))
                        .unwrap_or_else(|| name.clone());
                    self.predeclare_global_binding(
                        runtime_name,
                        true,
                        self.iterable_kind_for_binding_span(statement.span),
                    );
                }
                _ => {}
            }
        }
    }

    fn predeclare_global_binding(
        &mut self,
        name: String,
        mutable: bool,
        iterable_kind: Option<IterableKind>,
    ) {
        self.global_bindings.entry(name).or_insert(GlobalBinding {
            mutable,
            iterable_kind,
        });
    }

    fn install_global_bindings(&self, state: &mut ChunkState) {
        for (name, global) in &self.global_bindings {
            let operand = state.constant(Constant::GlobalRef(name.clone()));
            state.define_global_if_absent(
                name.clone(),
                Binding {
                    operand,
                    mutable: global.mutable,
                    ref_backed: true,
                    iterable_kind: global.iterable_kind,
                },
            );
        }
    }

    fn predeclare_runtime_type_layouts(
        &mut self,
        statements: &[Stmt],
        namespace: Option<&str>,
    ) -> Result<(), UnsupportedBytecode> {
        for statement in statements {
            match &statement.kind {
                StmtKind::Let { name, expr, .. } => {
                    let runtime_name = namespace
                        .map(|namespace| format!("{namespace}.{name}"))
                        .unwrap_or_else(|| name.clone());
                    match &expr.kind {
                        ExprKind::Function {
                            params,
                            return_type,
                            body,
                            ..
                        } => {
                            if let Some(class_name) =
                                function_return_class_name(return_type.as_ref(), body)
                            {
                                self.function_return_classes
                                    .insert((runtime_name, params.len()), class_name);
                            }
                        }
                        ExprKind::ModuleDefinition { statements, .. } => {
                            self.predeclare_runtime_type_layouts(statements, Some(&runtime_name))?;
                        }
                        ExprKind::EnumDefinition { variants, .. } => {
                            self.predeclare_enum_layout(name.clone(), runtime_name, variants);
                        }
                        ExprKind::ClassDefinition {
                            base,
                            fields,
                            interfaces,
                            specifiers,
                            ..
                        } => self.predeclare_class_layout(
                            name.clone(),
                            runtime_name,
                            specifiers,
                            ObjectKind::Class,
                            &[],
                            base.as_ref(),
                            interfaces,
                            fields.clone(),
                        ),
                        ExprKind::StructDefinition {
                            computes, fields, ..
                        } => self.predeclare_class_layout(
                            name.clone(),
                            runtime_name,
                            &[],
                            ObjectKind::Struct {
                                computes: *computes,
                            },
                            &[],
                            None,
                            &[],
                            fields.clone(),
                        ),
                        ExprKind::InterfaceDefinition {
                            parents,
                            fields,
                            methods,
                            ..
                        } => self.predeclare_interface_layout(
                            name.clone(),
                            runtime_name,
                            &[],
                            parents,
                            fields.clone(),
                            methods.clone(),
                        ),
                        _ => {}
                    }
                }
                StmtKind::ParametricType {
                    name,
                    specifiers,
                    params,
                    expr,
                } => match &expr.kind {
                    ExprKind::ClassDefinition {
                        base,
                        fields,
                        interfaces,
                        specifiers: class_specifiers,
                        ..
                    } => {
                        let runtime_name = namespace
                            .map(|namespace| format!("{namespace}.{name}"))
                            .unwrap_or_else(|| name.clone());
                        let mut runtime_specifiers = specifiers.clone();
                        runtime_specifiers.extend(class_specifiers.clone());
                        self.predeclare_class_layout(
                            name.clone(),
                            runtime_name,
                            &runtime_specifiers,
                            ObjectKind::Class,
                            params,
                            base.as_ref(),
                            interfaces,
                            fields.clone(),
                        );
                    }
                    ExprKind::EnumDefinition { variants, .. } => {
                        let runtime_name = namespace
                            .map(|namespace| format!("{namespace}.{name}"))
                            .unwrap_or_else(|| name.clone());
                        self.predeclare_enum_layout(name.clone(), runtime_name, variants);
                    }
                    ExprKind::StructDefinition {
                        computes, fields, ..
                    } => {
                        let runtime_name = namespace
                            .map(|namespace| format!("{namespace}.{name}"))
                            .unwrap_or_else(|| name.clone());
                        self.predeclare_class_layout(
                            name.clone(),
                            runtime_name,
                            &[],
                            ObjectKind::Struct {
                                computes: *computes,
                            },
                            &[],
                            None,
                            &[],
                            fields.clone(),
                        );
                    }
                    ExprKind::InterfaceDefinition {
                        parents,
                        fields,
                        methods,
                        ..
                    } => {
                        let runtime_name = namespace
                            .map(|namespace| format!("{namespace}.{name}"))
                            .unwrap_or_else(|| name.clone());
                        self.predeclare_interface_layout(
                            name.clone(),
                            runtime_name,
                            params,
                            parents,
                            fields.clone(),
                            methods.clone(),
                        );
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(())
    }

    fn lower_statement(
        &mut self,
        statement: &Stmt,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        match &statement.kind {
            StmtKind::Using { path } => {
                state.import(path.clone());
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::TypeAlias { .. } | StmtKind::ParametricTypeAlias { .. } => {
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::ScopedAccessLevel { .. } => {
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::ParametricType {
                name,
                specifiers,
                params,
                expr,
            } => {
                if let ExprKind::ClassDefinition {
                    base,
                    blocks,
                    extension_methods,
                    fields,
                    interfaces,
                    methods,
                    specifiers: class_specifiers,
                    ..
                } = &expr.kind
                {
                    let mut runtime_specifiers = specifiers.clone();
                    runtime_specifiers.extend(class_specifiers.clone());
                    self.register_class_layout(
                        name.clone(),
                        name.clone(),
                        &runtime_specifiers,
                        params,
                        base.as_ref(),
                        interfaces,
                        fields.clone(),
                        methods,
                        blocks,
                        extension_methods,
                    )?;
                    let class_type_constant = self
                        .resolve_class_layout(name, state)
                        .ok_or(UnsupportedBytecode)?
                        .class_type_constant();
                    let class_type = state.constant(class_type_constant);
                    state.define(
                        name.clone(),
                        Binding {
                            operand: class_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::InterfaceDefinition {
                    parents,
                    fields,
                    methods,
                    ..
                } = &expr.kind
                {
                    self.register_interface_layout(
                        name.clone(),
                        name.clone(),
                        params,
                        parents,
                        fields.clone(),
                        methods.clone(),
                    );
                    let interface_type =
                        state.constant(Constant::InterfaceType { name: name.clone() });
                    state.define(
                        name.clone(),
                        Binding {
                            operand: interface_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::StructDefinition {
                    computes, fields, ..
                } = &expr.kind
                {
                    self.predeclare_class_layout(
                        name.clone(),
                        name.clone(),
                        &[],
                        ObjectKind::Struct {
                            computes: *computes,
                        },
                        &[],
                        None,
                        &[],
                        fields.clone(),
                    );
                    let struct_type = state.constant(Constant::StructType {
                        name: name.clone(),
                        computes: *computes,
                    });
                    state.define(
                        name.clone(),
                        Binding {
                            operand: struct_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::EnumDefinition { variants, .. } = &expr.kind {
                    self.predeclare_enum_layout(name.clone(), name.clone(), variants);
                    state.last_value = state.none();
                    return Ok(());
                }
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Let {
                name,
                annotation,
                expr,
                ..
            } => {
                if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
                    self.lower_module_definition(name, statements, state, statement.span)?;
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::ClassDefinition {
                    base,
                    blocks,
                    extension_methods,
                    fields,
                    interfaces,
                    methods,
                    specifiers,
                    ..
                } = &expr.kind
                {
                    self.register_class_layout(
                        name.clone(),
                        name.clone(),
                        specifiers,
                        &[],
                        base.as_ref(),
                        interfaces,
                        fields.clone(),
                        methods,
                        blocks,
                        extension_methods,
                    )?;
                    let class_type_constant = self
                        .resolve_class_layout(name, state)
                        .ok_or(UnsupportedBytecode)?
                        .class_type_constant();
                    let class_type = state.constant(class_type_constant);
                    state.define(
                        name.clone(),
                        Binding {
                            operand: class_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::InterfaceDefinition {
                    parents,
                    fields,
                    methods,
                    ..
                } = &expr.kind
                {
                    self.register_interface_layout(
                        name.clone(),
                        name.clone(),
                        &[],
                        parents,
                        fields.clone(),
                        methods.clone(),
                    );
                    let interface_type =
                        state.constant(Constant::InterfaceType { name: name.clone() });
                    state.define(
                        name.clone(),
                        Binding {
                            operand: interface_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::StructDefinition {
                    computes, fields, ..
                } = &expr.kind
                {
                    self.predeclare_class_layout(
                        name.clone(),
                        name.clone(),
                        &[],
                        ObjectKind::Struct {
                            computes: *computes,
                        },
                        &[],
                        None,
                        &[],
                        fields.clone(),
                    );
                    let struct_type = state.constant(Constant::StructType {
                        name: name.clone(),
                        computes: *computes,
                    });
                    state.define(
                        name.clone(),
                        Binding {
                            operand: struct_type,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: None,
                        },
                    );
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::EnumDefinition { variants, .. } = &expr.kind {
                    self.predeclare_enum_layout(name.clone(), name.clone(), variants);
                    state.last_value = state.none();
                    return Ok(());
                }
                if is_compile_time_type_expr(expr) && !is_type_value_annotation(annotation.as_ref())
                {
                    state.last_value = state.none();
                    return Ok(());
                }

                let iterable_kind = self
                    .iterable_kind_for_binding_span(statement.span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let value = if let ExprKind::Function {
                    params,
                    effects,
                    body,
                    ..
                } = &expr.kind
                {
                    self.emit_function_value(
                        Some(name.clone()),
                        params,
                        effects,
                        body,
                        state,
                        expr.span,
                    )?
                } else if matches!(expr.kind, ExprKind::External) {
                    self.emit_external(
                        annotation
                            .as_ref()
                            .map(|annotation| annotation.name.clone())
                            .or_else(|| {
                                self.facts
                                    .expression_type(expr.span)
                                    .and_then(type_to_runtime_type_name)
                            })
                            .unwrap_or(TypeName::Any),
                        state,
                        expr.span,
                    )
                } else {
                    self.lower_expr(expr, state)?
                };
                let value = self.freeze_binding_value_if_needed(expr, value, state);
                if state.is_entry_root() && self.global_bindings.contains_key(name) {
                    self.initialize_global_binding(
                        name.clone(),
                        name.clone(),
                        value,
                        false,
                        iterable_kind,
                        state,
                        statement.span,
                    );
                } else {
                    state.define(
                        name.clone(),
                        Binding {
                            operand: value,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind,
                        },
                    );
                }
                self.mark_binding_callable_if_needed(state, name, statement.span);
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Var {
                name,
                annotation,
                expr,
                ..
            } => {
                let iterable_kind = self
                    .iterable_kind_for_binding_span(statement.span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let value = if matches!(expr.kind, ExprKind::External) {
                    self.emit_external(
                        annotation
                            .as_ref()
                            .map(|annotation| annotation.name.clone())
                            .or_else(|| {
                                self.facts
                                    .expression_type(expr.span)
                                    .and_then(type_to_runtime_type_name)
                            })
                            .unwrap_or(TypeName::Any),
                        state,
                        expr.span,
                    )
                } else {
                    self.lower_expr(expr, state)?
                };
                if state.is_entry_root() && self.global_bindings.contains_key(name) {
                    self.initialize_global_binding(
                        name.clone(),
                        name.clone(),
                        value,
                        true,
                        iterable_kind,
                        state,
                        statement.span,
                    );
                } else {
                    self.define_mutable_binding(
                        name.clone(),
                        value,
                        iterable_kind,
                        state,
                        statement.span,
                    );
                }
                self.mark_binding_callable_if_needed(state, name, statement.span);
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Return(expr) => {
                let value = self.lower_expr(expr, state)?;
                state.emit_defer_scope_exits(state.defer_scope_depth, statement.span);
                state.chunk.emit(Instruction::Return {
                    value,
                    span: statement.span,
                });
                Ok(())
            }
            StmtKind::Defer(body) => {
                let captures = self.collect_function_captures(&[], body, state);
                let function = self.lower_function_with_context(
                    Some("__verse_defer_body".to_string()),
                    &[],
                    &[],
                    body,
                    &state.imports,
                    &state.extensions,
                    &captures,
                )?;
                let dest = state.allocate_register(statement.span);
                let callee = state.constant(Constant::NativeFunction("__verse_defer".to_string()));
                let procedure = state.constant(Constant::Function(function));
                let procedure = if captures.is_empty() {
                    procedure
                } else {
                    let parent_scope = self.emit_capture_scope(&captures, state, statement.span);
                    let function_dest = state.allocate_register(statement.span);
                    let none = state.none();
                    state.chunk.emit(Instruction::NewFunction {
                        dest: function_dest,
                        procedure,
                        self_value: none,
                        parent_scope,
                        span: statement.span,
                    });
                    ValueOperand::Register(function_dest)
                };
                state.chunk.emit(Instruction::Call {
                    dest,
                    callee,
                    arguments: vec![procedure],
                    named_arguments: Vec::new(),
                    named_argument_values: Vec::new(),
                    callee_yields: false,
                    span: statement.span,
                });
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Set { target, op, expr } => {
                self.lower_assignment(target, *op, expr, state, statement.span)?;
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Break => {
                let Some(loop_context) = state.loop_breaks.last() else {
                    return Err(UnsupportedBytecode);
                };
                let defer_scope_count = state
                    .defer_scope_depth
                    .saturating_sub(loop_context.defer_scope_depth);
                state.emit_defer_scope_exits(defer_scope_count, statement.span);
                let jump = state.chunk.emit_jump(Instruction::Jump {
                    jump_offset: usize::MAX,
                });
                let Some(breaks) = state.loop_breaks.last_mut() else {
                    return Err(UnsupportedBytecode);
                };
                breaks.breaks.push(jump);
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Expr(expr) => {
                state.last_value = self.lower_expr(expr, state)?;
                Ok(())
            }
            StmtKind::ExtensionMethod(method) => {
                self.lower_extension_method(None, method, state)?;
                state.last_value = state.none();
                Ok(())
            }
        }
    }

    fn lower_module_definition(
        &mut self,
        namespace: &str,
        statements: &[Stmt],
        state: &mut ChunkState,
        _span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        let import_len = state.imports.len();
        state.enter_scope();
        for statement in statements {
            self.lower_module_member(namespace, statement, state)?;
        }
        state.exit_scope();
        state.imports.truncate(import_len);
        Ok(())
    }

    fn lower_module_member(
        &mut self,
        namespace: &str,
        statement: &Stmt,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        match &statement.kind {
            StmtKind::Using { path } => {
                state.import(path.clone());
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::TypeAlias { .. } | StmtKind::ParametricTypeAlias { .. } => {
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::ScopedAccessLevel { .. } => {
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::ParametricType {
                name,
                specifiers,
                params,
                expr,
            } => {
                let qualified = format!("{namespace}.{name}");
                if let ExprKind::ClassDefinition {
                    base,
                    blocks,
                    extension_methods,
                    fields,
                    interfaces,
                    methods,
                    specifiers: class_specifiers,
                    ..
                } = &expr.kind
                {
                    let mut runtime_specifiers = specifiers.clone();
                    runtime_specifiers.extend(class_specifiers.clone());
                    self.register_class_layout(
                        name.clone(),
                        qualified.clone(),
                        &runtime_specifiers,
                        params,
                        base.as_ref(),
                        interfaces,
                        fields.clone(),
                        methods,
                        blocks,
                        extension_methods,
                    )?;
                    let binding = Binding {
                        operand: state.constant(
                            self.resolve_class_layout(&qualified, state)
                                .ok_or(UnsupportedBytecode)?
                                .class_type_constant(),
                        ),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::InterfaceDefinition {
                    parents,
                    fields,
                    methods,
                    ..
                } = &expr.kind
                {
                    self.register_interface_layout(
                        name.clone(),
                        qualified.clone(),
                        params,
                        parents,
                        fields.clone(),
                        methods.clone(),
                    );
                    let binding = Binding {
                        operand: state.constant(Constant::InterfaceType {
                            name: qualified.clone(),
                        }),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::StructDefinition {
                    computes, fields, ..
                } = &expr.kind
                {
                    self.predeclare_class_layout(
                        name.clone(),
                        qualified.clone(),
                        &[],
                        ObjectKind::Struct {
                            computes: *computes,
                        },
                        &[],
                        None,
                        &[],
                        fields.clone(),
                    );
                    let binding = Binding {
                        operand: state.constant(Constant::StructType {
                            name: qualified.clone(),
                            computes: *computes,
                        }),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::EnumDefinition { variants, .. } = &expr.kind {
                    self.predeclare_enum_layout(name.clone(), qualified, variants);
                    state.last_value = state.none();
                    return Ok(());
                }
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Let {
                name,
                annotation,
                expr,
                ..
            } => {
                let qualified = format!("{namespace}.{name}");
                if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
                    self.lower_module_definition(&qualified, statements, state, statement.span)?;
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::ClassDefinition {
                    base,
                    blocks,
                    extension_methods,
                    fields,
                    interfaces,
                    methods,
                    specifiers,
                    ..
                } = &expr.kind
                {
                    self.register_class_layout(
                        name.clone(),
                        qualified.clone(),
                        specifiers,
                        &[],
                        base.as_ref(),
                        interfaces,
                        fields.clone(),
                        methods,
                        blocks,
                        extension_methods,
                    )?;
                    let binding = Binding {
                        operand: state.constant(
                            self.resolve_class_layout(&qualified, state)
                                .ok_or(UnsupportedBytecode)?
                                .class_type_constant(),
                        ),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::InterfaceDefinition {
                    parents,
                    fields,
                    methods,
                    ..
                } = &expr.kind
                {
                    self.register_interface_layout(
                        name.clone(),
                        qualified.clone(),
                        &[],
                        parents,
                        fields.clone(),
                        methods.clone(),
                    );
                    let binding = Binding {
                        operand: state.constant(Constant::InterfaceType {
                            name: qualified.clone(),
                        }),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::StructDefinition {
                    computes, fields, ..
                } = &expr.kind
                {
                    self.predeclare_class_layout(
                        name.clone(),
                        qualified.clone(),
                        &[],
                        ObjectKind::Struct {
                            computes: *computes,
                        },
                        &[],
                        None,
                        &[],
                        fields.clone(),
                    );
                    let binding = Binding {
                        operand: state.constant(Constant::StructType {
                            name: qualified.clone(),
                            computes: *computes,
                        }),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    };
                    state.define(name.clone(), binding);
                    state.define_global(qualified, binding);
                    state.last_value = state.none();
                    return Ok(());
                }
                if let ExprKind::EnumDefinition { variants, .. } = &expr.kind {
                    self.predeclare_enum_layout(name.clone(), qualified, variants);
                    state.last_value = state.none();
                    return Ok(());
                }
                if is_compile_time_type_expr(expr) && !is_type_value_annotation(annotation.as_ref())
                {
                    state.last_value = state.none();
                    return Ok(());
                }

                let iterable_kind = self
                    .iterable_kind_for_binding_span(statement.span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let value = if let ExprKind::Function {
                    params,
                    effects,
                    body,
                    ..
                } = &expr.kind
                {
                    self.emit_function_value(
                        Some(qualified.clone()),
                        params,
                        effects,
                        body,
                        state,
                        expr.span,
                    )?
                } else if matches!(expr.kind, ExprKind::External) {
                    self.emit_external(
                        annotation
                            .as_ref()
                            .map(|annotation| annotation.name.clone())
                            .or_else(|| {
                                self.facts
                                    .expression_type(expr.span)
                                    .and_then(type_to_runtime_type_name)
                            })
                            .unwrap_or(TypeName::Any),
                        state,
                        expr.span,
                    )
                } else {
                    self.lower_expr(expr, state)?
                };
                let binding = if self.global_bindings.contains_key(&qualified) {
                    self.initialize_global_binding(
                        name.clone(),
                        qualified.clone(),
                        value,
                        false,
                        iterable_kind,
                        state,
                        statement.span,
                    )
                } else {
                    let binding = Binding {
                        operand: value,
                        mutable: false,
                        ref_backed: false,
                        iterable_kind,
                    };
                    state.define(name.clone(), binding);
                    self.mark_binding_callable_if_needed(state, name, statement.span);
                    binding
                };
                state.define_global(qualified, binding);
                self.mark_binding_callable_if_needed(state, name, statement.span);
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::Var {
                name,
                annotation,
                expr,
                ..
            } => {
                let qualified = format!("{namespace}.{name}");
                let iterable_kind = self
                    .iterable_kind_for_binding_span(statement.span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let value = if matches!(expr.kind, ExprKind::External) {
                    self.emit_external(
                        annotation
                            .as_ref()
                            .map(|annotation| annotation.name.clone())
                            .or_else(|| {
                                self.facts
                                    .expression_type(expr.span)
                                    .and_then(type_to_runtime_type_name)
                            })
                            .unwrap_or(TypeName::Any),
                        state,
                        expr.span,
                    )
                } else {
                    self.lower_expr(expr, state)?
                };
                let binding = if self.global_bindings.contains_key(&qualified) {
                    self.initialize_global_binding(
                        name.clone(),
                        qualified.clone(),
                        value,
                        true,
                        iterable_kind,
                        state,
                        statement.span,
                    )
                } else {
                    self.define_mutable_binding(
                        name.clone(),
                        value,
                        iterable_kind,
                        state,
                        statement.span,
                    );
                    self.mark_binding_callable_if_needed(state, name, statement.span);
                    state.lookup(name).ok_or(UnsupportedBytecode)?
                };
                state.define_global(qualified, binding);
                self.mark_binding_callable_if_needed(state, name, statement.span);
                state.last_value = state.none();
                Ok(())
            }
            StmtKind::ExtensionMethod(method) => {
                self.lower_extension_method(Some(namespace), method, state)?;
                state.last_value = state.none();
                Ok(())
            }
            _ => Err(UnsupportedBytecode),
        }
    }

    fn lower_extension_method(
        &mut self,
        module: Option<&str>,
        extension: &ExtensionMethod,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        let Some(body) = extension.method.body.as_ref() else {
            return Err(UnsupportedBytecode);
        };
        let mut params = Vec::with_capacity(extension.method.params.len() + 1);
        params.push(extension.receiver.clone());
        params.extend(extension.method.params.clone());

        let function_name = module.map_or_else(
            || extension.method.name.clone(),
            |module| format!("{module}.{}", extension.method.name),
        );
        let function = self.lower_function(
            Some(function_name),
            &params,
            &extension.method.effects,
            body,
        )?;
        state.define_extension(ExtensionBinding {
            name: extension.method.name.clone(),
            module: module.map(str::to_string),
            function,
            receiver_type: param_binding_type(&extension.receiver, self.facts),
            params: lower_param_names(&extension.method.params)?,
            param_types: lower_param_types(&extension.method.params, self.facts),
            param_defaults: lower_param_defaults(&extension.method.params),
            source_params: extension.method.params.clone(),
            fields: Vec::new(),
            captures_self: false,
        });
        Ok(())
    }

    fn predeclare_class_layout(
        &mut self,
        local_name: String,
        runtime_name: String,
        specifiers: &[String],
        object_kind: ObjectKind,
        type_params: &[TypeParam],
        base: Option<&TypeAnnotation>,
        interfaces: &[TypeAnnotation],
        fields: Vec<StructField>,
    ) {
        let (base_class, layout_fields) =
            self.collect_class_layout_fields(base, interfaces, &fields, type_params);
        let interface_names = self.class_interface_names(base, interfaces, type_params);
        let layout = ClassLayout {
            runtime_name: runtime_name.clone(),
            base_class,
            interfaces: interface_names,
            object_kind,
            unique: specifiers.iter().any(|specifier| specifier == "unique"),
            abstract_class: specifiers.iter().any(|specifier| specifier == "abstract"),
            epic_internal_class: specifiers
                .iter()
                .any(|specifier| specifier == "epic_internal"),
            final_class: specifiers.iter().any(|specifier| specifier == "final"),
            final_super: specifiers
                .iter()
                .any(|specifier| specifier == "final_super"),
            concrete: specifiers.iter().any(|specifier| specifier == "concrete"),
            castable: specifiers.iter().any(|specifier| specifier == "castable"),
            fields: layout_fields,
        };
        self.class_layouts.insert(local_name, layout.clone());
        self.class_layouts.insert(runtime_name, layout);
    }

    fn predeclare_enum_layout(
        &mut self,
        local_name: String,
        runtime_name: String,
        variants: &[EnumVariant],
    ) {
        let layout = EnumLayout {
            runtime_name: runtime_name.clone(),
            variants: variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect(),
        };
        self.enum_layouts.insert(local_name, layout.clone());
        self.enum_layouts.insert(runtime_name, layout);
    }

    fn predeclare_interface_layout(
        &mut self,
        local_name: String,
        runtime_name: String,
        type_params: &[TypeParam],
        parents: &[TypeAnnotation],
        fields: Vec<StructField>,
        methods: Vec<ClassMethod>,
    ) {
        let layout = self.collect_interface_layout(
            runtime_name.clone(),
            parents,
            fields,
            methods,
            type_params,
        );
        self.interface_layouts.insert(local_name, layout.clone());
        self.interface_layouts.insert(runtime_name, layout);
    }

    fn collect_class_layout_fields(
        &self,
        base: Option<&TypeAnnotation>,
        interfaces: &[TypeAnnotation],
        fields: &[StructField],
        type_params: &[TypeParam],
    ) -> (Option<String>, Vec<StructField>) {
        let base_class_name = base
            .and_then(|annotation| self.type_annotation_aggregate_name(annotation, type_params))
            .filter(|base_name| {
                self.resolve_class_layout(base_name, &ChunkState::new("<class-base>"))
                    .is_some()
            });
        let mut layout_fields = if let Some(base_name) = base_class_name.as_ref() {
            self.resolve_class_layout(base_name, &ChunkState::new("<class-base>"))
                .map(|layout| layout.fields.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for interface_name in self.class_interface_names(base, interfaces, type_params) {
            if let Some(interface) = self.resolve_interface_layout(&interface_name) {
                merge_struct_fields(&mut layout_fields, interface.fields.clone());
            }
        }
        for field in fields {
            if let Some(existing) = layout_fields
                .iter_mut()
                .find(|candidate| candidate.name == field.name)
            {
                *existing = field.clone();
            } else {
                layout_fields.push(field.clone());
            }
        }
        (base_class_name, layout_fields)
    }

    fn collect_interface_layout(
        &self,
        runtime_name: String,
        parents: &[TypeAnnotation],
        fields: Vec<StructField>,
        methods: Vec<ClassMethod>,
        type_params: &[TypeParam],
    ) -> InterfaceLayout {
        let mut layout_fields = Vec::new();
        let mut layout_methods = Vec::new();
        for parent in parents {
            if let Some(parent_name) = self.type_annotation_aggregate_name(parent, type_params)
                && let Some(parent_layout) = self.resolve_interface_layout(&parent_name)
            {
                merge_struct_fields(&mut layout_fields, parent_layout.fields.clone());
                layout_methods.extend(parent_layout.methods.clone());
            }
        }
        merge_struct_fields(&mut layout_fields, fields);
        layout_methods.extend(methods);
        InterfaceLayout {
            runtime_name,
            fields: layout_fields,
            methods: layout_methods,
        }
    }

    fn register_class_layout(
        &mut self,
        local_name: String,
        runtime_name: String,
        specifiers: &[String],
        type_params: &[TypeParam],
        base: Option<&TypeAnnotation>,
        interfaces: &[TypeAnnotation],
        fields: Vec<StructField>,
        methods: &[ClassMethod],
        blocks: &[ClassBlock],
        extension_methods: &[ExtensionMethod],
    ) -> Result<(), UnsupportedBytecode> {
        let type_param_names = type_params
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>();
        let (base_class_name, layout_fields) =
            self.collect_class_layout_fields(base, interfaces, &fields, type_params);
        let interface_names = self.class_interface_names(base, interfaces, type_params);

        let class_extensions = extension_methods
            .iter()
            .map(|extension| {
                self.lower_class_extension_method(
                    &runtime_name,
                    &layout_fields,
                    base_class_name.as_deref(),
                    extension,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut method_descriptors = if let Some(base_name) = base_class_name.as_ref() {
            self.resolve_class_descriptor(&base_name)
                .map(|class| class.methods.clone())
                .ok_or(UnsupportedBytecode)?
        } else {
            Vec::new()
        };
        let interface_default_methods = self
            .class_interface_names(base, interfaces, type_params)
            .into_iter()
            .filter_map(|interface_name| self.resolve_interface_layout(&interface_name))
            .flat_map(|interface| interface.methods.clone())
            .filter(|method| method.body.is_some())
            .collect::<Vec<_>>();
        for method in interface_default_methods {
            let descriptor = self.lower_class_method(
                &runtime_name,
                &layout_fields,
                &class_extensions,
                base_class_name.as_deref(),
                &method,
                &type_param_names,
            )?;
            if let Some(existing) = method_descriptors.iter_mut().find(|candidate| {
                candidate.name == descriptor.name
                    && candidate.params.len() == descriptor.params.len()
                    && candidate.qualifier == descriptor.qualifier
                    && candidate.param_types == descriptor.param_types
            }) {
                *existing = descriptor;
            } else {
                method_descriptors.push(descriptor);
            }
        }
        let local_methods = methods
            .iter()
            .filter(|method| method.body.is_some())
            .map(|method| {
                self.lower_class_method(
                    &runtime_name,
                    &layout_fields,
                    &class_extensions,
                    base_class_name.as_deref(),
                    method,
                    &type_param_names,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        for method in local_methods {
            if let Some(existing) = method_descriptors.iter_mut().find(|candidate| {
                candidate.name == method.name
                    && candidate.params.len() == method.params.len()
                    && candidate.qualifier == method.qualifier
                    && candidate.param_types == method.param_types
            }) {
                *existing = method;
            } else {
                method_descriptors.push(method);
            }
        }
        let mut block_descriptors = if let Some(base_name) = base_class_name.as_ref() {
            self.resolve_class_descriptor(&base_name)
                .map(|class| class.blocks.clone())
                .ok_or(UnsupportedBytecode)?
        } else {
            Vec::new()
        };
        for block in blocks {
            block_descriptors.push(self.lower_class_block(
                &runtime_name,
                &layout_fields,
                &class_extensions,
                base_class_name.as_deref(),
                block,
            )?);
        }
        let class_descriptor = ClassDescriptor {
            name: runtime_name.clone(),
            unique: specifiers.iter().any(|specifier| specifier == "unique"),
            base_class: base_class_name.clone(),
            interfaces: interface_names.clone(),
            abstract_class: specifiers.iter().any(|specifier| specifier == "abstract"),
            epic_internal_class: specifiers
                .iter()
                .any(|specifier| specifier == "epic_internal"),
            final_class: specifiers.iter().any(|specifier| specifier == "final"),
            final_super: specifiers
                .iter()
                .any(|specifier| specifier == "final_super"),
            concrete: specifiers.iter().any(|specifier| specifier == "concrete"),
            castable: specifiers.iter().any(|specifier| specifier == "castable"),
            fields: layout_fields
                .iter()
                .map(|field| field.name.clone())
                .collect(),
            methods: method_descriptors,
            blocks: block_descriptors,
        };
        let layout = ClassLayout {
            runtime_name: runtime_name.clone(),
            base_class: base_class_name.clone(),
            interfaces: interface_names,
            object_kind: ObjectKind::Class,
            unique: specifiers.iter().any(|specifier| specifier == "unique"),
            abstract_class: specifiers.iter().any(|specifier| specifier == "abstract"),
            epic_internal_class: specifiers
                .iter()
                .any(|specifier| specifier == "epic_internal"),
            final_class: specifiers.iter().any(|specifier| specifier == "final"),
            final_super: specifiers
                .iter()
                .any(|specifier| specifier == "final_super"),
            concrete: specifiers.iter().any(|specifier| specifier == "concrete"),
            castable: specifiers.iter().any(|specifier| specifier == "castable"),
            fields: layout_fields,
        };
        self.class_layouts.insert(local_name, layout.clone());
        self.class_layouts.insert(runtime_name, layout);
        self.classes.push(class_descriptor);
        Ok(())
    }

    fn lower_class_extension_method(
        &mut self,
        class_name: &str,
        fields: &[StructField],
        super_class: Option<&str>,
        extension: &ExtensionMethod,
    ) -> Result<ExtensionBinding, UnsupportedBytecode> {
        let Some(body) = extension.method.body.as_ref() else {
            return Err(UnsupportedBytecode);
        };
        let mut state = ChunkState::with_class_extension_context(
            format!("{class_name}.{}", extension.method.name),
            fields,
            &extension.receiver,
            &extension.method.params,
            super_class,
            self.facts,
        );
        self.install_global_bindings(&mut state);
        let value = self.lower_expr(body, &mut state)?;
        state.chunk.emit(Instruction::Return {
            value,
            span: body.span,
        });
        let register_params =
            class_extension_register_params(fields, &extension.receiver, &extension.method.params)?;
        let function = self.push_function_descriptor(
            Some(format!("{class_name}.{}", extension.method.name)),
            register_params.clone(),
            extension.method.params.clone(),
            vec![None; register_params.len()],
            vec![None; register_params.len()],
            state,
            extension
                .method
                .effects
                .iter()
                .any(|effect| effect == "decides"),
        )?;
        Ok(ExtensionBinding {
            name: extension.method.name.clone(),
            module: None,
            function,
            receiver_type: param_binding_type(&extension.receiver, self.facts),
            params: lower_param_names(&extension.method.params)?,
            param_types: lower_param_types(&extension.method.params, self.facts),
            param_defaults: lower_param_defaults(&extension.method.params),
            source_params: extension.method.params.clone(),
            fields: fields.iter().map(|field| field.name.clone()).collect(),
            captures_self: true,
        })
    }

    fn register_interface_layout(
        &mut self,
        local_name: String,
        runtime_name: String,
        type_params: &[TypeParam],
        parents: &[TypeAnnotation],
        fields: Vec<StructField>,
        methods: Vec<ClassMethod>,
    ) {
        let layout = self.collect_interface_layout(
            runtime_name.clone(),
            parents,
            fields,
            methods,
            type_params,
        );
        self.interface_layouts.insert(local_name, layout.clone());
        self.interface_layouts.insert(runtime_name, layout);
    }

    fn type_annotation_aggregate_name(
        &self,
        annotation: &TypeAnnotation,
        type_params: &[TypeParam],
    ) -> Option<String> {
        self.resolve_static_type_function_type_name(&annotation.name, type_params, 0)
            .as_ref()
            .and_then(type_name_aggregate_head)
            .or_else(|| type_annotation_class_name(annotation))
    }

    fn resolve_static_type_function_type_name(
        &self,
        type_name: &TypeName,
        type_params: &[TypeParam],
        depth: usize,
    ) -> Option<TypeName> {
        if depth > 16 {
            return None;
        }
        match type_name {
            TypeName::Applied { name, args } => {
                if let Some(info) = self.select_bytecode_type_function(name, args, type_params) {
                    let substitutions = info
                        .params
                        .iter()
                        .map(|param| param.name.clone())
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    let target =
                        substitute_bytecode_type_name_params(&info.target, &substitutions)?;
                    return self
                        .resolve_static_type_function_type_name(&target, type_params, depth + 1)
                        .or(Some(target));
                }
                Some(TypeName::Applied {
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| {
                            self.resolve_static_type_function_type_name(arg, type_params, depth + 1)
                                .unwrap_or_else(|| arg.clone())
                        })
                        .collect(),
                })
            }
            TypeName::Array(item) => Some(TypeName::Array(match item.as_ref() {
                Some(item) => Some(Box::new(
                    self.resolve_static_type_function_type_name(item, type_params, depth + 1)
                        .unwrap_or_else(|| item.as_ref().clone()),
                )),
                None => None,
            })),
            TypeName::Map(key, value) => Some(TypeName::Map(
                Box::new(
                    self.resolve_static_type_function_type_name(key, type_params, depth + 1)
                        .unwrap_or_else(|| key.as_ref().clone()),
                ),
                Box::new(
                    self.resolve_static_type_function_type_name(value, type_params, depth + 1)
                        .unwrap_or_else(|| value.as_ref().clone()),
                ),
            )),
            TypeName::WeakMap(key, value) => Some(TypeName::WeakMap(
                Box::new(
                    self.resolve_static_type_function_type_name(key, type_params, depth + 1)
                        .unwrap_or_else(|| key.as_ref().clone()),
                ),
                Box::new(
                    self.resolve_static_type_function_type_name(value, type_params, depth + 1)
                        .unwrap_or_else(|| value.as_ref().clone()),
                ),
            )),
            TypeName::Tuple(items) => Some(TypeName::Tuple(
                items
                    .iter()
                    .map(|item| {
                        self.resolve_static_type_function_type_name(item, type_params, depth + 1)
                            .unwrap_or_else(|| item.clone())
                    })
                    .collect(),
            )),
            TypeName::Option(item) => Some(TypeName::Option(Box::new(
                self.resolve_static_type_function_type_name(item, type_params, depth + 1)
                    .unwrap_or_else(|| item.as_ref().clone()),
            ))),
            TypeName::TypeBounds { lower, upper } => Some(TypeName::TypeBounds {
                lower: Box::new(
                    self.resolve_static_type_function_type_name(lower, type_params, depth + 1)
                        .unwrap_or_else(|| lower.as_ref().clone()),
                ),
                upper: Box::new(
                    self.resolve_static_type_function_type_name(upper, type_params, depth + 1)
                        .unwrap_or_else(|| upper.as_ref().clone()),
                ),
            }),
            _ => Some(type_name.clone()),
        }
    }

    fn select_bytecode_type_function(
        &self,
        name: &str,
        args: &[TypeName],
        type_params: &[TypeParam],
    ) -> Option<&BytecodeTypeFunction> {
        self.type_functions
            .get(name)?
            .iter()
            .filter(|info| info.params.len() == args.len())
            .filter_map(|info| {
                self.bytecode_type_function_match_score(info, args, type_params)
                    .map(|score| (info, score))
            })
            .min_by_key(|(_, score)| *score)
            .map(|(info, _)| info)
    }

    fn bytecode_type_function_match_score(
        &self,
        info: &BytecodeTypeFunction,
        args: &[TypeName],
        type_params: &[TypeParam],
    ) -> Option<usize> {
        info.params
            .iter()
            .zip(args)
            .try_fold(0usize, |score, (param, arg)| {
                Some(
                    score
                        + self.bytecode_type_function_param_match_score(param, arg, type_params)?,
                )
            })
    }

    fn bytecode_type_function_param_match_score(
        &self,
        param: &BytecodeTypeFunctionParam,
        arg: &TypeName,
        type_params: &[TypeParam],
    ) -> Option<usize> {
        if let Some(score) =
            self.bytecode_type_constraint_match_score(&param.constraint, arg, type_params)
        {
            return Some(score);
        }
        let resolved_constraint =
            self.resolve_static_type_function_type_name(&param.constraint, type_params, 0)?;
        self.bytecode_type_constraint_match_score(&resolved_constraint, arg, type_params)
    }

    fn bytecode_type_constraint_match_score(
        &self,
        constraint: &TypeName,
        arg: &TypeName,
        type_params: &[TypeParam],
    ) -> Option<usize> {
        if constraint == arg {
            return Some(0);
        }
        match constraint {
            TypeName::Type => Some(32),
            TypeName::TypeBounds { upper, .. } => self
                .bytecode_type_constraint_match_score(upper, arg, type_params)
                .map(|score| score + 4),
            TypeName::Applied { name, args }
                if matches!(
                    name.as_str(),
                    "subtype"
                        | "castable_subtype"
                        | "concrete_subtype"
                        | "castable_concrete_subtype"
                ) && args.len() == 1 =>
            {
                let expected = bytecode_type_constraint_payload_head(&args[0])?;
                let actuals = self.bytecode_type_arg_runtime_heads(arg, type_params);
                actuals
                    .into_iter()
                    .filter(|actual| self.runtime_type_head_satisfies(&actual, &expected))
                    .map(|actual| {
                        if runtime_names_match(&actual, &expected) {
                            1
                        } else {
                            2
                        }
                    })
                    .min()
            }
            _ => None,
        }
    }

    fn bytecode_type_arg_runtime_heads(
        &self,
        arg: &TypeName,
        type_params: &[TypeParam],
    ) -> Vec<String> {
        if let TypeName::Named(name) = arg
            && let Some(param) = type_params.iter().find(|param| param.name == *name)
        {
            match &param.constraint {
                TypeParamConstraint::Type => Vec::new(),
                TypeParamConstraint::Subtype(parent) => {
                    bytecode_type_constraint_payload_head(parent)
                        .into_iter()
                        .collect()
                }
                TypeParamConstraint::TypeBounds { upper, .. } => {
                    bytecode_type_constraint_payload_head(upper)
                        .into_iter()
                        .collect()
                }
            }
        } else {
            bytecode_type_constraint_payload_head(arg)
                .into_iter()
                .collect()
        }
    }

    fn runtime_type_head_satisfies(&self, actual: &str, expected: &str) -> bool {
        if expected == "any" || runtime_names_match(actual, expected) {
            return true;
        }
        let mut current = Some(actual.to_string());
        while let Some(name) = current {
            if runtime_names_match(&name, expected) {
                return true;
            }
            if self.class_layouts.get(&name).is_some_and(|layout| {
                layout
                    .interfaces
                    .iter()
                    .any(|interface| self.runtime_type_head_satisfies(interface, expected))
            }) {
                return true;
            }
            current = self
                .class_layouts
                .get(&name)
                .and_then(|layout| layout.base_class.clone());
        }
        self.interface_layouts.contains_key(actual)
            && self
                .interface_layouts
                .get(actual)
                .is_some_and(|_| runtime_names_match(actual, expected))
    }

    fn class_interface_names(
        &self,
        base: Option<&TypeAnnotation>,
        interfaces: &[TypeAnnotation],
        type_params: &[TypeParam],
    ) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(base_name) =
            base.and_then(|annotation| self.type_annotation_aggregate_name(annotation, type_params))
            && self.resolve_interface_layout(&base_name).is_some()
        {
            names.push(base_name);
        }
        names.extend(
            interfaces
                .iter()
                .filter_map(|annotation| {
                    self.type_annotation_aggregate_name(annotation, type_params)
                })
                .filter(|name| self.resolve_interface_layout(name).is_some()),
        );
        names
    }

    fn lower_class_method(
        &mut self,
        class_name: &str,
        fields: &[StructField],
        extensions: &[ExtensionBinding],
        super_class: Option<&str>,
        method: &ClassMethod,
        type_params: &[String],
    ) -> Result<ClassMethodDescriptor, UnsupportedBytecode> {
        let Some(body) = method.body.as_ref() else {
            return Err(UnsupportedBytecode);
        };
        let function = self.lower_class_method_function(
            class_name,
            fields,
            extensions,
            super_class,
            method,
            body,
        )?;
        Ok(ClassMethodDescriptor {
            qualifier: method.qualifier.clone(),
            name: method.name.clone(),
            params: lower_param_names(&method.params)?,
            param_types: lower_runtime_param_types(&method.params, type_params, self.facts),
            function,
            field_count: fields.len(),
            decides: method.effects.iter().any(|effect| effect == "decides"),
        })
    }

    fn lower_class_block(
        &mut self,
        class_name: &str,
        fields: &[StructField],
        extensions: &[ExtensionBinding],
        super_class: Option<&str>,
        block: &ClassBlock,
    ) -> Result<ClassBlockDescriptor, UnsupportedBytecode> {
        let mut state = ChunkState::with_class_method_context(
            format!("{class_name}.<block>"),
            fields,
            &[],
            super_class,
            self.facts,
        );
        self.install_global_bindings(&mut state);
        for extension in extensions {
            state.define_extension(extension.clone());
        }
        let value = self.lower_expr(&block.body, &mut state)?;
        state.chunk.emit(Instruction::Return {
            value,
            span: block.body.span,
        });
        let register_params = class_method_register_params(fields, &[])?;
        let function = self.push_function_descriptor(
            Some(format!("{class_name}.<block>")),
            register_params.clone(),
            Vec::new(),
            vec![None; register_params.len()],
            vec![None; register_params.len()],
            state,
            false,
        )?;
        Ok(ClassBlockDescriptor {
            function,
            field_count: fields.len(),
        })
    }

    fn lower_class_method_function(
        &mut self,
        class_name: &str,
        fields: &[StructField],
        extensions: &[ExtensionBinding],
        super_class: Option<&str>,
        method: &ClassMethod,
        body: &Expr,
    ) -> Result<usize, UnsupportedBytecode> {
        let decides = method.effects.iter().any(|effect| effect == "decides");
        let mut state = ChunkState::with_class_method_context(
            format!("{class_name}.{}", method.name),
            fields,
            &method.params,
            super_class,
            self.facts,
        );
        self.install_global_bindings(&mut state);
        for extension in extensions {
            state.define_extension(extension.clone());
        }
        if decides {
            let (value, mut failure_jumps) = self.lower_failable_expr(body, &mut state)?;
            state.chunk.emit(Instruction::Return {
                value,
                span: body.span,
            });
            let failure_start = state.chunk.next_instruction_index();
            for jump in failure_jumps.drain(..) {
                state.chunk.patch_jump(jump, failure_start);
            }
            let register_params = class_method_register_params(fields, &method.params)?;
            return self.push_function_descriptor(
                Some(format!("{class_name}.{}", method.name)),
                register_params.clone(),
                method.params.clone(),
                vec![None; register_params.len()],
                vec![None; register_params.len()],
                state,
                decides,
            );
        }

        let value = self.lower_expr(body, &mut state)?;
        state.chunk.emit(Instruction::Return {
            value,
            span: body.span,
        });
        let register_params = class_method_register_params(fields, &method.params)?;
        self.push_function_descriptor(
            Some(format!("{class_name}.{}", method.name)),
            register_params.clone(),
            method.params.clone(),
            vec![None; register_params.len()],
            vec![None; register_params.len()],
            state,
            decides,
        )
    }

    fn lower_expr(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        if let Some(value) = self.lower_aggregate_type_value(expr, state) {
            return Ok(value);
        }
        if let Some((name, params)) = self.lower_parametric_type_value(expr) {
            return Ok(state.constant(Constant::ParametricType { name, params }));
        }
        if let Some(type_name) = self.lower_alias_type_value(expr, state) {
            return Ok(state.constant(Constant::Type(type_name)));
        }
        if let Some(constant) = lower_constant_expr(expr)? {
            return Ok(state.constant(constant));
        }

        match &expr.kind {
            ExprKind::Ident(name) => self.lower_identifier(name, state, expr.span),
            ExprKind::External => self.lower_external(expr, state),
            ExprKind::QualifiedName { qualifier, name } => {
                self.lower_identifier(&format!("{qualifier}.{name}"), state, expr.span)
            }
            ExprKind::Unary { op, expr: operand } => match op {
                UnaryOp::Positive => self.lower_expr(operand, state),
                UnaryOp::Negate => {
                    let source = self.lower_expr(operand, state)?;
                    let dest = state.allocate_register(expr.span);
                    state.chunk.emit(Instruction::Neg {
                        dest,
                        source,
                        span: expr.span,
                    });
                    Ok(ValueOperand::Register(dest))
                }
                UnaryOp::Not => self.lower_bool_expression_from_failable(expr, state),
            },
            ExprKind::Binary { op, .. }
                if is_comparison_binary_op(*op) || matches!(op, BinaryOp::And | BinaryOp::Or) =>
            {
                self.lower_bool_expression_from_failable(expr, state)
            }
            ExprKind::Binary { left, op, right } => {
                self.lower_binary(left, *op, right, state, expr.span)
            }
            ExprKind::InterpolatedString(parts) => {
                self.lower_interpolated_string(parts, state, expr.span)
            }
            ExprKind::Array(items) => {
                let values = items
                    .iter()
                    .map(|item| self.lower_expr(item, state))
                    .collect::<Result<Vec<_>, _>>()?;
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::NewArray {
                    dest,
                    values,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            ExprKind::Tuple(items) => {
                let values = items
                    .iter()
                    .map(|item| self.lower_expr(item, state))
                    .collect::<Result<Vec<_>, _>>()?;
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::NewArray {
                    dest,
                    values,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            ExprKind::Map(entries) => {
                let mut keys = Vec::with_capacity(entries.len());
                let mut values = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    keys.push(self.lower_expr(key, state)?);
                    values.push(self.lower_expr(value, state)?);
                }
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::NewMap {
                    dest,
                    keys,
                    values,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            ExprKind::Archetype {
                callee, entries, ..
            } => self.lower_archetype(callee, entries, state, expr.span),
            ExprKind::Loop { body } => self.lower_loop(body, state, expr.span),
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(
                condition,
                then_branch,
                else_branch.as_deref(),
                state,
                expr.span,
            ),
            ExprKind::Case { subject, arms } => self.lower_case(subject, arms, state, expr.span),
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.lower_block_value(statements, state, expr.span)
            }
            ExprKind::Function {
                params,
                effects,
                body,
                ..
            } => self.emit_function_value(None, params, effects, body, state, expr.span),
            ExprKind::Var {
                name, expr: value, ..
            } => self.lower_var_expression(name, value, state, expr.span),
            ExprKind::TypeLiteral { expr: value } => {
                let type_name = self
                    .facts
                    .expression_type(value.span)
                    .and_then(type_to_runtime_type_name)
                    .unwrap_or(TypeName::Any);
                Ok(state.constant(Constant::Type(type_name)))
            }
            ExprKind::TypeAnnotationLiteral { .. } => {
                let type_name = self
                    .facts
                    .expression_type(expr.span)
                    .and_then(|value_type| match value_type {
                        Type::TypeValueOf(item) => type_to_runtime_type_name(item.as_ref()),
                        other => type_to_runtime_type_name(&other),
                    })
                    .unwrap_or(TypeName::Any);
                Ok(state.constant(Constant::Type(type_name)))
            }
            ExprKind::Set { target, op, expr } => {
                self.lower_assignment(target, *op, expr, state, expr.span)?;
                Ok(state.none())
            }
            ExprKind::Spawn { body } => self.lower_spawn(body, state, expr.span),
            ExprKind::Concurrent { op, body } => self.lower_concurrent(*op, body, state, expr.span),
            ExprKind::Call { callee, args } => self.lower_call(callee, args, state, expr.span),
            ExprKind::For { clauses, body } => self.lower_for(clauses, body, state, expr.span),
            ExprKind::Profile { description, body } => {
                self.lower_profile(description, body, state, expr.span)
            }
            ExprKind::Option(value) => self.lower_option(value.as_deref(), state, expr.span),
            ExprKind::UnwrapOption(value) => {
                let source = self.lower_expr(value, state)?;
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::Query {
                    dest,
                    source,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            ExprKind::BracketCall { callee, args } => {
                let call_args = args
                    .iter()
                    .cloned()
                    .map(CallArg::Positional)
                    .collect::<Vec<_>>();
                if self.is_callable(callee, state) {
                    return self.lower_call(callee, &call_args, state, expr.span);
                }
                if let ExprKind::Member { object, name } = &callee.kind
                    && let Some(extension) = self.select_extension_for_call(
                        object,
                        &call_args,
                        state.lookup_extensions(name),
                    )
                {
                    return self
                        .lower_extension_call(object, &call_args, extension, state, expr.span);
                }
                let callee = self.lower_expr(callee, state)?;
                let arguments = args
                    .iter()
                    .map(|arg| self.lower_expr(arg, state))
                    .collect::<Result<Vec<_>, _>>()?;
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::Call {
                    dest,
                    callee,
                    arguments,
                    named_arguments: Vec::new(),
                    named_argument_values: Vec::new(),
                    callee_yields: true,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            ExprKind::Member { object, name } if name == "Length" => {
                self.lower_length(object, state, expr.span)
            }
            ExprKind::Member { object, name } => {
                if let Some(namespace) = compile_time_member_path(object) {
                    let qualified = format!("{namespace}.{name}");
                    if let Some(value) =
                        self.lower_builtin_member_path(&qualified, state, expr.span)?
                    {
                        return Ok(value);
                    }
                    if state.lookup(&qualified).is_some()
                        || self.class_layouts.contains_key(&qualified)
                    {
                        return self.lower_identifier(&qualified, state, expr.span);
                    }
                }
                if self
                    .facts
                    .expression_type(object.span)
                    .is_some_and(type_is_type_value_for_extension_accessor)
                    && let Some(extension) =
                        self.select_extension_for_call(object, &[], state.lookup_extensions(name))
                {
                    return self.lower_extension_call(object, &[], extension, state, expr.span);
                }
                self.lower_field_access(object, name, state, expr.span)
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => {
                if let Some(namespace) = compile_time_member_path(object) {
                    let qualified = format!("{namespace}.{qualifier}.{name}");
                    if state.lookup(&qualified).is_some()
                        || self.class_layouts.contains_key(&qualified)
                    {
                        return self.lower_identifier(&qualified, state, expr.span);
                    }
                }
                if self
                    .facts
                    .expression_type(object.span)
                    .is_some_and(type_is_type_value_for_extension_accessor)
                    && let Some(extension) = self.select_extension_for_call(
                        object,
                        &[],
                        state.lookup_qualified_extensions(qualifier, name),
                    )
                {
                    return self.lower_extension_call(object, &[], extension, state, expr.span);
                }
                self.lower_field_access(object, &format!("({qualifier}:){name}"), state, expr.span)
            }
            ExprKind::Index { collection, index } => {
                let callee = self.lower_expr(collection, state)?;
                let index = self.lower_expr(index, state)?;
                let dest = state.allocate_register(expr.span);
                state.chunk.emit(Instruction::Call {
                    dest,
                    callee,
                    arguments: vec![index],
                    named_arguments: Vec::new(),
                    named_argument_values: Vec::new(),
                    callee_yields: true,
                    span: expr.span,
                });
                Ok(ValueOperand::Register(dest))
            }
            _ => Err(UnsupportedBytecode),
        }
    }

    fn lower_aggregate_type_value(
        &self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Option<ValueOperand> {
        match self.facts.expression_type(expr.span)? {
            Type::StructType(name) => Some(
                state.constant(
                    self.resolve_class_layout(name, state)
                        .map(|layout| match layout.object_kind {
                            ObjectKind::Struct { computes } => Constant::StructType {
                                name: layout.runtime_name.clone(),
                                computes,
                            },
                            ObjectKind::Class => layout.class_type_constant(),
                        })
                        .unwrap_or_else(|| Constant::StructType {
                            name: name.clone(),
                            computes: false,
                        }),
                ),
            ),
            Type::ClassType(name) => Some(
                state.constant(
                    self.resolve_class_layout(name, state)
                        .map(ClassLayout::class_type_constant)
                        .unwrap_or_else(|| Constant::ClassType {
                            name: name.clone(),
                            base: None,
                            interfaces: Vec::new(),
                            unique: false,
                            abstract_class: false,
                            epic_internal_class: false,
                            final_class: false,
                            final_super: false,
                            concrete: false,
                            castable: false,
                        }),
                ),
            ),
            Type::InterfaceType(name) => Some(
                state.constant(Constant::InterfaceType {
                    name: self
                        .resolve_interface_layout(name)
                        .map_or_else(|| name.clone(), |layout| layout.runtime_name.clone()),
                }),
            ),
            _ => None,
        }
    }

    fn lower_alias_type_value(&self, expr: &Expr, state: &ChunkState) -> Option<TypeName> {
        let Type::TypeValueOf(item) = self.facts.expression_type(expr.span)? else {
            return None;
        };
        match &expr.kind {
            ExprKind::Ident(name) => {
                if self.runtime_identifier_exists(name, state) {
                    return None;
                }
            }
            ExprKind::QualifiedName { qualifier, name } => {
                let qualified = format!("{qualifier}.{name}");
                if self.runtime_identifier_exists(&qualified, state) {
                    return None;
                }
            }
            ExprKind::Member { object, name } => {
                let qualified =
                    compile_time_member_path(object).map(|path| format!("{path}.{name}"))?;
                if self.runtime_identifier_exists(&qualified, state) {
                    return None;
                }
            }
            ExprKind::QualifiedMember {
                object,
                qualifier,
                name,
            } => {
                let qualified = compile_time_member_path(object)
                    .map(|path| format!("{path}.{qualifier}.{name}"))?;
                if self.runtime_identifier_exists(&qualified, state) {
                    return None;
                }
            }
            ExprKind::Call { callee, .. } => {
                let callee_name = callable_lookup_name(callee);
                let callee_is_parametric_type = self
                    .facts
                    .expression_type(callee.span)
                    .is_some_and(|value_type| matches!(value_type, Type::ParametricType { .. }))
                    || callee_name.as_ref().is_some_and(|name| {
                        self.class_layouts.contains_key(name)
                            || self.interface_layouts.contains_key(name)
                    });
                if !self.facts.is_static_type_function_call(expr.span) && !callee_is_parametric_type
                {
                    let name = callee_name?;
                    if self.runtime_identifier_exists(&name, state) {
                        return None;
                    }
                }
            }
            ExprKind::Tuple(_) => {}
            _ => return None,
        }
        type_to_runtime_type_name(item)
    }

    fn lower_parametric_type_value(&self, expr: &Expr) -> Option<(String, Vec<String>)> {
        let Type::ParametricType {
            name,
            params,
            kind: ParametricTypeKind::Alias,
        } = self.facts.expression_type(expr.span)?
        else {
            return None;
        };
        Some((name.clone(), params.clone()))
    }

    fn runtime_identifier_exists(&self, name: &str, state: &ChunkState) -> bool {
        state.lookup(name).is_some()
            || self.callable_names.contains(name)
            || bytecode_native_function_name(name)
            || self.class_layouts.contains_key(name)
    }

    fn define_mutable_binding(
        &mut self,
        name: String,
        value: ValueOperand,
        iterable_kind: Option<IterableKind>,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::NewRef {
            dest,
            domain: None,
            span,
        });
        let ref_value = ValueOperand::Register(dest);
        state.chunk.emit(Instruction::RefSet {
            ref_value,
            value,
            span,
        });
        state.define(
            name,
            Binding {
                operand: ref_value,
                mutable: true,
                ref_backed: true,
                iterable_kind,
            },
        );
        ref_value
    }

    fn initialize_global_binding(
        &mut self,
        local_name: String,
        global_name: String,
        value: ValueOperand,
        mutable: bool,
        iterable_kind: Option<IterableKind>,
        state: &mut ChunkState,
        span: Span,
    ) -> Binding {
        let ref_value = state.constant(Constant::GlobalRef(global_name));
        state.chunk.emit(Instruction::RefSet {
            ref_value,
            value,
            span,
        });
        let binding = Binding {
            operand: ref_value,
            mutable,
            ref_backed: true,
            iterable_kind,
        };
        state.define(local_name, binding);
        binding
    }

    fn lower_identifier(
        &mut self,
        name: &str,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        if name == "Inf" {
            return Ok(state.constant(Constant::Float(f64::INFINITY)));
        }
        if name == "NaN" {
            return Ok(state.constant(Constant::Float(f64::NAN)));
        }
        if name == "PiFloat" {
            return Ok(state.constant(Constant::Float(std::f64::consts::PI)));
        }
        if bytecode_native_function_name(name) {
            return Ok(state.constant(Constant::NativeFunction(name.to_string())));
        }
        let Some(binding) = state.lookup(name) else {
            if let Some(function) = self.function_descriptor_index(name) {
                return Ok(state.constant(Constant::Function(function)));
            }
            if let Some(layout) = self.resolve_class_layout(name, state) {
                match layout.object_kind {
                    ObjectKind::Class => {
                        return Ok(state.constant(layout.class_type_constant()));
                    }
                    ObjectKind::Struct { computes } => {
                        return Ok(state.constant(Constant::StructType {
                            name: layout.runtime_name.clone(),
                            computes,
                        }));
                    }
                }
            }
            if let Some(layout) = self.resolve_interface_layout(name) {
                return Ok(state.constant(Constant::InterfaceType {
                    name: layout.runtime_name.clone(),
                }));
            }
            return Err(UnsupportedBytecode);
        };
        if binding.ref_backed {
            Ok(self.emit_ref_get(binding.operand, state, span))
        } else {
            Ok(binding.operand)
        }
    }

    fn lower_external(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let value_type = self
            .facts
            .expression_type(expr.span)
            .and_then(type_to_runtime_type_name)
            .unwrap_or(TypeName::Any);
        Ok(self.emit_external(value_type, state, expr.span))
    }

    fn emit_external(
        &mut self,
        value_type: TypeName,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let source = state.constant(Constant::External(value_type));
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::Move { dest, source, span });
        ValueOperand::Register(dest)
    }

    fn freeze_binding_value_if_needed(
        &self,
        expr: &Expr,
        value: ValueOperand,
        state: &mut ChunkState,
    ) -> ValueOperand {
        let Some(value_type) = self.facts.expression_type(expr.span) else {
            return value;
        };
        if !type_needs_binding_freeze(&value_type) {
            return value;
        }
        let dest = state.allocate_register(expr.span);
        state.chunk.emit(Instruction::Freeze {
            dest,
            value,
            span: expr.span,
        });
        ValueOperand::Register(dest)
    }

    fn lower_profile(
        &mut self,
        description: &Expr,
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let (wall_time_start, user_tag) =
            self.emit_begin_profile_block(description, state, span)?;
        let value = self.lower_expr(body, state)?;
        self.emit_end_profile_block(wall_time_start, user_tag, state, span);
        Ok(value)
    }

    fn lower_failable_profile(
        &mut self,
        description: &Expr,
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let (wall_time_start, user_tag) =
            self.emit_begin_profile_block(description, state, span)?;
        let (value, failure_jumps) = self.lower_failable_expr(body, state)?;
        self.emit_end_profile_block(wall_time_start, user_tag, state, span);
        Ok((value, failure_jumps))
    }

    fn emit_begin_profile_block(
        &mut self,
        description: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(RegisterIndex, ValueOperand), UnsupportedBytecode> {
        let user_tag = self.lower_expr(description, state)?;
        let wall_time_start = state.allocate_register(span);
        state.chunk.emit(Instruction::BeginProfileBlock {
            dest: wall_time_start,
            span,
        });
        Ok((wall_time_start, user_tag))
    }

    fn emit_end_profile_block(
        &mut self,
        wall_time_start: RegisterIndex,
        user_tag: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) {
        let begin_row = state.constant(Constant::Int(span_position(span.line)));
        let begin_column = state.constant(Constant::Int(span_position(span.column)));
        let end_row = state.constant(Constant::Int(span_position(span.line)));
        let end_column = state.constant(Constant::Int(span_position(
            span.column
                .saturating_add(span.end.saturating_sub(span.start)),
        )));
        state.chunk.emit(Instruction::EndProfileBlock {
            wall_time_start: ValueOperand::Register(wall_time_start),
            user_tag,
            snippet_path: String::new(),
            begin_row,
            begin_column,
            end_row,
            end_column,
            span,
        });
    }

    fn lower_var_expression(
        &mut self,
        name: &str,
        value_expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let iterable_kind = self
            .facts
            .expression_type(value_expr.span)
            .and_then(iterable_kind_from_type)
            .or_else(|| self.iterable_kind_for_expr(value_expr, state));
        let value = self.lower_expr(value_expr, state)?;
        self.define_mutable_binding(name.to_string(), value, iterable_kind, state, span);
        self.mark_binding_callable_if_needed(state, name, span);
        Ok(value)
    }

    fn lower_failable_var_expression(
        &mut self,
        name: &str,
        value_expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let iterable_kind = self
            .facts
            .expression_type(value_expr.span)
            .and_then(iterable_kind_from_type)
            .or_else(|| self.iterable_kind_for_expr(value_expr, state));
        let (value, failure_jumps) = self.lower_failable_expr(value_expr, state)?;
        self.define_mutable_binding(name.to_string(), value, iterable_kind, state, span);
        self.mark_binding_callable_if_needed(state, name, span);
        Ok((value, failure_jumps))
    }

    fn emit_ref_get(
        &mut self,
        ref_value: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::RefGet {
            dest,
            ref_value,
            span,
        });
        ValueOperand::Register(dest)
    }

    fn lower_compound_assignment_value(
        &mut self,
        ref_value: ValueOperand,
        op: AssignOp,
        right_source: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::RefGet {
            dest,
            ref_value,
            span,
        });
        let left_source = ValueOperand::Register(dest);
        match op {
            AssignOp::AddAssign => state.chunk.emit(Instruction::Add {
                dest,
                left_source,
                right_source,
                span,
            }),
            AssignOp::SubAssign => state.chunk.emit(Instruction::Sub {
                dest,
                left_source,
                right_source,
                span,
            }),
            AssignOp::MulAssign => state.chunk.emit(Instruction::Mul {
                dest,
                left_source,
                right_source,
                span,
            }),
            AssignOp::DivAssign => state.chunk.emit(Instruction::Div {
                dest,
                left_source,
                right_source,
                span,
            }),
            AssignOp::Assign => unreachable!("plain assignment handled before compound lowering"),
        }
        left_source
    }

    fn iterable_kind_for_expr(&self, expr: &Expr, state: &ChunkState) -> Option<IterableKind> {
        if let Some(kind) = self
            .facts
            .expression_type(expr.span)
            .and_then(iterable_kind_from_type)
        {
            return Some(kind);
        }

        match &expr.kind {
            ExprKind::Array(_) | ExprKind::String(_) | ExprKind::For { .. } => {
                Some(IterableKind::Indexed)
            }
            ExprKind::Map(_) => Some(IterableKind::Map),
            ExprKind::Ident(name) => state.lookup(name).and_then(|binding| binding.iterable_kind),
            _ => None,
        }
    }

    fn iterable_kind_for_binding_span(&self, span: Span) -> Option<IterableKind> {
        self.facts
            .binding_type(span)
            .and_then(iterable_kind_from_type)
    }

    fn mark_binding_callable_if_needed(&self, state: &mut ChunkState, name: &str, span: Span) {
        if self
            .facts
            .binding_type(span)
            .is_some_and(bytecode_type_is_callable)
        {
            state.mark_callable(name);
        }
    }

    fn install_captures(&self, captures: &[CaptureBinding], state: &mut ChunkState, span: Span) {
        for (index, capture) in captures.iter().enumerate() {
            let dest = state.allocate_register(span);
            state.chunk.emit(Instruction::LoadCapture {
                dest,
                scope: ValueOperand::Register(RegisterIndex::SCOPE),
                index,
                span,
            });
            state.define(
                capture.name.clone(),
                Binding {
                    operand: ValueOperand::Register(dest),
                    mutable: capture.binding.mutable,
                    ref_backed: capture.binding.ref_backed,
                    iterable_kind: capture.binding.iterable_kind,
                },
            );
            if capture.callable {
                state.mark_callable(&capture.name);
            }
        }
    }

    fn lower_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let left_source = self.lower_expr(left, state)?;
        let right_source = self.lower_expr(right, state)?;
        let dest = state.allocate_register(span);
        let instruction = match op {
            BinaryOp::Add => Instruction::Add {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Subtract => Instruction::Sub {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Multiply => Instruction::Mul {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Divide => Instruction::Div {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Remainder => Instruction::Mod {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::NotEqual => Instruction::Neq {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Less => Instruction::Lt {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::LessEqual => Instruction::Lte {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Greater => Instruction::Gt {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::GreaterEqual => Instruction::Gte {
                dest,
                left_source,
                right_source,
                span,
            },
            BinaryOp::Equal | BinaryOp::And | BinaryOp::Or | BinaryOp::Range => {
                return Err(UnsupportedBytecode);
            }
        };
        state.chunk.emit(instruction);
        Ok(ValueOperand::Register(dest))
    }

    fn lower_interpolated_string(
        &mut self,
        parts: &[InterpolatedStringPart],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let mut current: Option<ValueOperand> = None;
        for part in parts {
            let value = match part {
                InterpolatedStringPart::Text(text) => {
                    if text.is_empty() {
                        continue;
                    }
                    state.constant(Constant::String(text.clone()))
                }
                InterpolatedStringPart::Expr(expr) => {
                    let value = self.lower_expr(expr, state)?;
                    let dest = state.allocate_register(expr.span);
                    let callee = state.constant(Constant::NativeFunction("ToString".to_string()));
                    state.chunk.emit(Instruction::Call {
                        dest,
                        callee,
                        arguments: vec![value],
                        named_arguments: Vec::new(),
                        named_argument_values: Vec::new(),
                        callee_yields: false,
                        span: expr.span,
                    });
                    ValueOperand::Register(dest)
                }
            };
            current = Some(if let Some(left_source) = current {
                let dest = state.allocate_register(span);
                state.chunk.emit(Instruction::Add {
                    dest,
                    left_source,
                    right_source: value,
                    span,
                });
                ValueOperand::Register(dest)
            } else {
                value
            });
        }
        Ok(current.unwrap_or_else(|| state.constant(Constant::String(String::new()))))
    }

    fn lower_assignment(
        &mut self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        if let Some(slot) = self.slot_target_for_expr(target, state) {
            return self.lower_slot_assignment(&slot, op, expr, state, span);
        }
        if let ExprKind::Member { object, name } = &target.kind
            && !is_self_expr(object)
        {
            return self.lower_field_assignment(object, name, op, expr, state, span);
        }

        let name = match &target.kind {
            ExprKind::Ident(name) => name,
            ExprKind::Member { object, name } if is_self_expr(object) => name,
            _ => return Err(UnsupportedBytecode),
        };
        let Some(binding) = state.lookup(name) else {
            return Err(UnsupportedBytecode);
        };
        if !binding.mutable {
            return Err(UnsupportedBytecode);
        }
        if !binding.ref_backed {
            return Err(UnsupportedBytecode);
        }
        let right_source = self.lower_expr(expr, state)?;
        match op {
            AssignOp::Assign => state.chunk.emit(Instruction::RefSet {
                ref_value: binding.operand,
                value: right_source,
                span,
            }),
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                let value = self.lower_compound_assignment_value(
                    binding.operand,
                    op,
                    right_source,
                    state,
                    span,
                );
                state.chunk.emit(Instruction::RefSet {
                    ref_value: binding.operand,
                    value,
                    span,
                });
            }
        }
        Ok(())
    }

    fn lower_field_assignment(
        &mut self,
        object: &Expr,
        name: &str,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        let (object, writebacks) = self.lower_field_object_for_writeback(object, state)?;
        let right_source = self.lower_expr(expr, state)?;
        let value = match op {
            AssignOp::Assign => right_source,
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                let left_dest = state.allocate_register(span);
                state.chunk.emit(Instruction::LoadField {
                    dest: left_dest,
                    object,
                    name: name.to_string(),
                    span,
                });
                let left_source = ValueOperand::Register(left_dest);
                let dest = state.allocate_register(span);
                let instruction = match op {
                    AssignOp::AddAssign => Instruction::Add {
                        dest,
                        left_source,
                        right_source,
                        span,
                    },
                    AssignOp::SubAssign => Instruction::Sub {
                        dest,
                        left_source,
                        right_source,
                        span,
                    },
                    AssignOp::MulAssign => Instruction::Mul {
                        dest,
                        left_source,
                        right_source,
                        span,
                    },
                    AssignOp::DivAssign => Instruction::Div {
                        dest,
                        left_source,
                        right_source,
                        span,
                    },
                    AssignOp::Assign => unreachable!("plain assignment handled above"),
                };
                state.chunk.emit(instruction);
                ValueOperand::Register(dest)
            }
        };
        state.chunk.emit(Instruction::SetField {
            object,
            name: name.to_string(),
            value,
            span,
        });
        self.emit_field_writebacks(&writebacks, state);
        Ok(())
    }

    fn lower_field_object_for_writeback(
        &mut self,
        object: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<FieldWriteback>), UnsupportedBytecode> {
        match &object.kind {
            ExprKind::Ident(name) => {
                let Some(binding) = state.lookup(name) else {
                    return Err(UnsupportedBytecode);
                };
                if binding.mutable && binding.ref_backed {
                    let value = self.emit_ref_get(binding.operand, state, object.span);
                    Ok((
                        value,
                        vec![FieldWriteback::Ref {
                            ref_value: binding.operand,
                            value,
                            span: object.span,
                        }],
                    ))
                } else {
                    Ok((self.lower_expr(object, state)?, Vec::new()))
                }
            }
            ExprKind::Member {
                object: parent,
                name,
            } if !is_self_expr(parent) => {
                let (parent_value, mut writebacks) =
                    self.lower_field_object_for_writeback(parent, state)?;
                let child_dest = state.allocate_register(object.span);
                state.chunk.emit(Instruction::LoadField {
                    dest: child_dest,
                    object: parent_value,
                    name: name.clone(),
                    span: object.span,
                });
                let child = ValueOperand::Register(child_dest);
                writebacks.push(FieldWriteback::Field {
                    object: parent_value,
                    name: name.clone(),
                    value: child,
                    span: object.span,
                });
                Ok((child, writebacks))
            }
            _ => Ok((self.lower_expr(object, state)?, Vec::new())),
        }
    }

    fn emit_field_writebacks(&mut self, writebacks: &[FieldWriteback], state: &mut ChunkState) {
        for writeback in writebacks.iter().rev() {
            match writeback {
                FieldWriteback::Ref {
                    ref_value,
                    value,
                    span,
                } => state.chunk.emit(Instruction::RefSet {
                    ref_value: *ref_value,
                    value: *value,
                    span: *span,
                }),
                FieldWriteback::Field {
                    object,
                    name,
                    value,
                    span,
                } => state.chunk.emit(Instruction::SetField {
                    object: *object,
                    name: name.clone(),
                    value: *value,
                    span: *span,
                }),
            }
        }
    }

    fn slot_target_for_expr<'expr>(
        &self,
        target: &'expr Expr,
        state: &ChunkState,
    ) -> Option<SlotTarget<'expr>> {
        let mut steps = Vec::new();
        let mut cursor = target;
        loop {
            match &cursor.kind {
                ExprKind::Index { collection, index } => {
                    steps.push(SlotStep {
                        index,
                        span: cursor.span,
                    });
                    cursor = collection;
                }
                ExprKind::BracketCall { callee, args } if !self.is_callable(callee, state) => {
                    let [index] = args.as_slice() else {
                        return None;
                    };
                    steps.push(SlotStep {
                        index,
                        span: cursor.span,
                    });
                    cursor = callee;
                }
                _ => break,
            }
        }
        if steps.is_empty() {
            return None;
        }
        steps.reverse();
        Some(SlotTarget {
            base: cursor,
            steps,
        })
    }

    fn lower_slot_assignment(
        &mut self,
        slot: &SlotTarget<'_>,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        let (access, _) = self.lower_slot_access(slot, false, state)?;
        let right_source = self.lower_expr(expr, state)?;
        let value_to_set = match op {
            AssignOp::Assign => right_source,
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                self.lower_slot_compound_value(&access, op, right_source, state, span)
            }
        };
        self.emit_slot_call_set(&access, value_to_set, state, span);
        self.emit_slot_writebacks(&access.writebacks, state);
        Ok(())
    }

    fn lower_failable_slot_assignment(
        &mut self,
        slot: &SlotTarget<'_>,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let (access, mut failure_jumps) = self.lower_slot_access(slot, true, state)?;
        let (right_source, right_jumps) = self.lower_failable_expr(expr, state)?;
        failure_jumps.extend(right_jumps);
        let value_to_set = match op {
            AssignOp::Assign => right_source,
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => self.lower_failable_slot_compound_value(
                &access,
                op,
                right_source,
                &mut failure_jumps,
                state,
                span,
            ),
        };
        self.emit_slot_call_set(&access, value_to_set, state, span);
        self.emit_slot_writebacks(&access.writebacks, state);
        Ok((value_to_set, failure_jumps))
    }

    fn lower_slot_access(
        &mut self,
        slot: &SlotTarget<'_>,
        failable: bool,
        state: &mut ChunkState,
    ) -> Result<(LoweredSlotAccess, Vec<usize>), UnsupportedBytecode> {
        if slot.steps.is_empty() {
            return Err(UnsupportedBytecode);
        }

        let mut failure_jumps = Vec::new();
        let mut write_container = self.lower_mutable_collection_value(slot.base, state)?;
        let mut read_container =
            self.mutable_collection_read_value(slot.base, write_container, state);
        let mut writebacks = Vec::new();
        let final_step = slot.steps.len() - 1;

        for (step_index, step) in slot.steps.iter().enumerate() {
            let index = self.lower_expr(step.index, state)?;
            if step_index == final_step {
                return Ok((
                    LoweredSlotAccess {
                        container: write_container,
                        read_container,
                        index,
                        writebacks,
                    },
                    failure_jumps,
                ));
            }

            let dest = state.allocate_register(step.span);
            if failable {
                let leniency_indicator = state.allocate_register(step.span);
                let jump = state.chunk.emit_jump(Instruction::ArrayIndexFastFail {
                    dest,
                    leniency_indicator,
                    array: read_container,
                    index,
                    on_failure: usize::MAX,
                    span: step.span,
                });
                failure_jumps.push(jump);
            } else {
                state.chunk.emit(Instruction::Call {
                    dest,
                    callee: read_container,
                    arguments: vec![index],
                    named_arguments: Vec::new(),
                    named_argument_values: Vec::new(),
                    callee_yields: true,
                    span: step.span,
                });
            }

            let child = ValueOperand::Register(dest);
            writebacks.push(SlotWriteback {
                container: write_container,
                index,
                value: child,
                span: step.span,
            });
            write_container = child;
            read_container = child;
        }

        Err(UnsupportedBytecode)
    }

    fn lower_slot_compound_value(
        &mut self,
        access: &LoweredSlotAccess,
        op: AssignOp,
        right_source: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let left_dest = state.allocate_register(span);
        state.chunk.emit(Instruction::Call {
            dest: left_dest,
            callee: access.read_container,
            arguments: vec![access.index],
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span,
        });
        self.lower_slot_binary_assignment(
            ValueOperand::Register(left_dest),
            op,
            right_source,
            state,
            span,
        )
    }

    fn lower_failable_slot_compound_value(
        &mut self,
        access: &LoweredSlotAccess,
        op: AssignOp,
        right_source: ValueOperand,
        failure_jumps: &mut Vec<usize>,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let left_dest = state.allocate_register(span);
        let left_leniency = state.allocate_register(span);
        let lookup_failure = state.chunk.emit_jump(Instruction::ArrayIndexFastFail {
            dest: left_dest,
            leniency_indicator: left_leniency,
            array: access.read_container,
            index: access.index,
            on_failure: usize::MAX,
            span,
        });
        failure_jumps.push(lookup_failure);
        self.lower_slot_binary_assignment(
            ValueOperand::Register(left_dest),
            op,
            right_source,
            state,
            span,
        )
    }

    fn lower_slot_binary_assignment(
        &mut self,
        left_source: ValueOperand,
        op: AssignOp,
        right_source: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        let instruction = match op {
            AssignOp::AddAssign => Instruction::Add {
                dest,
                left_source,
                right_source,
                span,
            },
            AssignOp::SubAssign => Instruction::Sub {
                dest,
                left_source,
                right_source,
                span,
            },
            AssignOp::MulAssign => Instruction::Mul {
                dest,
                left_source,
                right_source,
                span,
            },
            AssignOp::DivAssign => Instruction::Div {
                dest,
                left_source,
                right_source,
                span,
            },
            AssignOp::Assign => unreachable!("plain assignment handled before compound lowering"),
        };
        state.chunk.emit(instruction);
        ValueOperand::Register(dest)
    }

    fn emit_slot_call_set(
        &mut self,
        access: &LoweredSlotAccess,
        value_to_set: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) {
        state.chunk.emit(Instruction::CallSet {
            container: access.container,
            index: access.index,
            value_to_set,
            span,
        });
    }

    fn emit_slot_writebacks(&mut self, writebacks: &[SlotWriteback], state: &mut ChunkState) {
        for writeback in writebacks.iter().rev() {
            state.chunk.emit(Instruction::CallSet {
                container: writeback.container,
                index: writeback.index,
                value_to_set: writeback.value,
                span: writeback.span,
            });
        }
    }

    fn lower_mutable_collection_value(
        &mut self,
        collection: &Expr,
        state: &mut ChunkState,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        if let ExprKind::Ident(name) = &collection.kind {
            let Some(binding) = state.lookup(name) else {
                return Err(UnsupportedBytecode);
            };
            if !binding.mutable {
                return Err(UnsupportedBytecode);
            }
            if !binding.ref_backed {
                return Err(UnsupportedBytecode);
            }
            Ok(binding.operand)
        } else {
            self.lower_expr(collection, state)
        }
    }

    fn mutable_collection_read_value(
        &mut self,
        collection: &Expr,
        container: ValueOperand,
        state: &mut ChunkState,
    ) -> ValueOperand {
        if matches!(collection.kind, ExprKind::Ident(_)) {
            self.emit_ref_get(container, state, collection.span)
        } else {
            container
        }
    }

    fn lower_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let result = state.allocate_register(span);
        let failure_context_id = self.allocate_failure_context_id();
        let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
            on_failure: usize::MAX,
            id: failure_context_id,
            span: condition.span,
        });

        state.enter_scope();
        let mut failure_jumps = self.lower_condition_failable(condition, state)?;
        let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
            done: usize::MAX,
            id: failure_context_id,
            span: condition.span,
        });
        let then_value = self.lower_expr(then_branch, state)?;
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: then_value,
            span: then_branch.span,
        });
        state.exit_scope();

        let end_jump = state.chunk.emit_jump(Instruction::Jump {
            jump_offset: usize::MAX,
        });
        let else_start = state.chunk.next_instruction_index();
        state.chunk.patch_jump(begin_failure, else_start);
        for jump in failure_jumps.drain(..) {
            state.chunk.patch_jump(jump, else_start);
        }

        let else_value = if let Some(else_branch) = else_branch {
            state.enter_scope();
            let value = self.lower_expr(else_branch, state)?;
            state.exit_scope();
            value
        } else {
            state.none()
        };
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: else_value,
            span,
        });

        let end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(end_failure, end);
        state.chunk.patch_jump(end_jump, end);
        Ok(ValueOperand::Register(result))
    }

    fn lower_case(
        &mut self,
        subject: &Expr,
        arms: &[CaseArm],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        if arms.is_empty() {
            return Err(UnsupportedBytecode);
        }

        let subject_value = self.lower_expr(subject, state)?;
        let result = state.allocate_register(span);
        let mut end_jumps = Vec::new();
        let mut end_failure_jumps = Vec::new();
        let mut has_wildcard = false;

        for arm in arms {
            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    has_wildcard = true;
                    let arm_value = self.lower_expr(&arm.expr, state)?;
                    state.chunk.emit(Instruction::Move {
                        dest: result,
                        source: arm_value,
                        span: arm.expr.span,
                    });
                    end_jumps.push(state.chunk.emit_jump(Instruction::Jump {
                        jump_offset: usize::MAX,
                    }));
                    break;
                }
                CasePattern::Expr(pattern) => {
                    let failure_context_id = self.allocate_failure_context_id();
                    let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                        on_failure: usize::MAX,
                        id: failure_context_id,
                        span: pattern.span,
                    });
                    let pattern_value = self.lower_expr(pattern, state)?;
                    let compare_dest = state.allocate_register(pattern.span);
                    let leniency_indicator = state.allocate_register(pattern.span);
                    let compare_failure = state.chunk.emit_jump(Instruction::EqFastFail {
                        dest: compare_dest,
                        leniency_indicator,
                        lhs: subject_value,
                        rhs: pattern_value,
                        on_failure: usize::MAX,
                        span: pattern.span,
                    });
                    let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                        done: usize::MAX,
                        id: failure_context_id,
                        span: pattern.span,
                    });
                    let arm_value = self.lower_expr(&arm.expr, state)?;
                    state.chunk.emit(Instruction::Move {
                        dest: result,
                        source: arm_value,
                        span: arm.expr.span,
                    });
                    end_jumps.push(state.chunk.emit_jump(Instruction::Jump {
                        jump_offset: usize::MAX,
                    }));

                    let next_arm = state.chunk.next_instruction_index();
                    state.chunk.patch_jump(begin_failure, next_arm);
                    state.chunk.patch_jump(compare_failure, next_arm);
                    end_failure_jumps.push(end_failure);
                }
            }
        }

        if !has_wildcard {
            state.chunk.emit(Instruction::Err { span });
        }

        let end = state.chunk.next_instruction_index();
        for jump in end_jumps {
            state.chunk.patch_jump(jump, end);
        }
        for jump in end_failure_jumps {
            state.chunk.patch_jump(jump, end);
        }
        Ok(ValueOperand::Register(result))
    }

    fn lower_failable_case(
        &mut self,
        subject: &Expr,
        arms: &[CaseArm],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        if arms.is_empty() {
            return Err(UnsupportedBytecode);
        }

        let subject_value = self.lower_expr(subject, state)?;
        let result = state.allocate_register(span);
        let mut failure_jumps = Vec::new();
        let mut end_jumps = Vec::new();
        let mut end_failure_jumps = Vec::new();
        let mut has_wildcard = false;

        for arm in arms {
            match &arm.pattern {
                CasePattern::Wildcard { .. } => {
                    has_wildcard = true;
                    let (arm_value, mut arm_failure_jumps) =
                        self.lower_failable_expr(&arm.expr, state)?;
                    state.chunk.emit(Instruction::Move {
                        dest: result,
                        source: arm_value,
                        span: arm.expr.span,
                    });
                    failure_jumps.append(&mut arm_failure_jumps);
                    end_jumps.push(state.chunk.emit_jump(Instruction::Jump {
                        jump_offset: usize::MAX,
                    }));
                    break;
                }
                CasePattern::Expr(pattern) => {
                    let failure_context_id = self.allocate_failure_context_id();
                    let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                        on_failure: usize::MAX,
                        id: failure_context_id,
                        span: pattern.span,
                    });
                    let pattern_value = self.lower_expr(pattern, state)?;
                    let compare_dest = state.allocate_register(pattern.span);
                    let leniency_indicator = state.allocate_register(pattern.span);
                    let compare_failure = state.chunk.emit_jump(Instruction::EqFastFail {
                        dest: compare_dest,
                        leniency_indicator,
                        lhs: subject_value,
                        rhs: pattern_value,
                        on_failure: usize::MAX,
                        span: pattern.span,
                    });
                    let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                        done: usize::MAX,
                        id: failure_context_id,
                        span: pattern.span,
                    });
                    let (arm_value, mut arm_failure_jumps) =
                        self.lower_failable_expr(&arm.expr, state)?;
                    state.chunk.emit(Instruction::Move {
                        dest: result,
                        source: arm_value,
                        span: arm.expr.span,
                    });
                    failure_jumps.append(&mut arm_failure_jumps);
                    end_jumps.push(state.chunk.emit_jump(Instruction::Jump {
                        jump_offset: usize::MAX,
                    }));

                    let next_arm = state.chunk.next_instruction_index();
                    state.chunk.patch_jump(begin_failure, next_arm);
                    state.chunk.patch_jump(compare_failure, next_arm);
                    end_failure_jumps.push(end_failure);
                }
            }
        }

        if !has_wildcard {
            failure_jumps.push(state.chunk.emit_jump(Instruction::Jump {
                jump_offset: usize::MAX,
            }));
        }

        let end = state.chunk.next_instruction_index();
        for jump in end_jumps {
            state.chunk.patch_jump(jump, end);
        }
        for jump in end_failure_jumps {
            state.chunk.patch_jump(jump, end);
        }
        Ok((ValueOperand::Register(result), failure_jumps))
    }

    fn lower_condition_failable(
        &mut self,
        condition: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        self.lower_failure_condition_value(condition, state)
            .map(|(_, jumps)| jumps)
    }

    fn lower_failure_condition_value(
        &mut self,
        condition: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        if let ExprKind::FailureBind { name, expr } = &condition.kind {
            let (value, jumps) = self.lower_failable_expr(expr, state)?;
            state.define(
                name.clone(),
                Binding {
                    operand: value,
                    mutable: false,
                    ref_backed: false,
                    iterable_kind: self
                        .facts
                        .expression_type(expr.span)
                        .and_then(iterable_kind_from_type)
                        .or_else(|| self.iterable_kind_for_expr(expr, state)),
                },
            );
            return Ok((value, jumps));
        }

        self.lower_failable_expr(condition, state)
    }

    fn lower_failable_expr(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        match &expr.kind {
            ExprKind::FailureSequence(clauses) => self.lower_failure_sequence(clauses, state),
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
                self.lower_failure_statements(statements, state)
            }
            ExprKind::Case { subject, arms } => {
                self.lower_failable_case(subject, arms, state, expr.span)
            }
            ExprKind::Profile { description, body } => {
                self.lower_failable_profile(description, body, state, expr.span)
            }
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => self.lower_not_failable(expr, state),
            ExprKind::Binary {
                left,
                op: BinaryOp::And,
                right,
            } => {
                let (_, mut jumps) = self.lower_failable_expr(left, state)?;
                let (value, right_jumps) = self.lower_failable_expr(right, state)?;
                jumps.extend(right_jumps);
                Ok((value, jumps))
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::Or,
                right,
            } => self.lower_or_failable(left, right, state, expr.span),
            ExprKind::Binary { left, op, right } if is_comparison_binary_op(*op) => {
                let (lhs, mut jumps) = self.lower_comparison_operand(left, state)?;
                let (rhs, rhs_jumps) = self.lower_comparison_operand(right, state)?;
                jumps.extend(rhs_jumps);
                let dest = state.allocate_register(expr.span);
                let leniency_indicator = state.allocate_register(expr.span);
                let instruction = match op {
                    BinaryOp::Equal => Instruction::EqFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    BinaryOp::NotEqual => Instruction::NeqFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    BinaryOp::Less => Instruction::LtFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    BinaryOp::LessEqual => Instruction::LteFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    BinaryOp::Greater => Instruction::GtFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    BinaryOp::GreaterEqual => Instruction::GteFastFail {
                        dest,
                        leniency_indicator,
                        lhs,
                        rhs,
                        on_failure: usize::MAX,
                        span: expr.span,
                    },
                    _ => unreachable!("comparison op checked before instruction selection"),
                };
                let jump = state.chunk.emit_jump(instruction);
                jumps.push(jump);
                Ok((ValueOperand::Register(dest), jumps))
            }
            ExprKind::UnwrapOption(value) => {
                let source = self.lower_expr(value, state)?;
                let dest = state.allocate_register(expr.span);
                let jump = self.emit_query_fast_fail(dest, source, state, expr.span);
                Ok((ValueOperand::Register(dest), vec![jump]))
            }
            ExprKind::Index { .. } => self.lower_array_index_fast_fail(expr, state),
            ExprKind::BracketCall { callee, .. } if !self.is_callable(callee, state) => {
                self.lower_array_index_fast_fail(expr, state)
            }
            ExprKind::BracketCall { .. } => {
                let value = self.lower_expr(expr, state)?;
                Ok((value, Vec::new()))
            }
            ExprKind::Var {
                name, expr: value, ..
            } => self.lower_failable_var_expression(name, value, state, expr.span),
            ExprKind::Set {
                target,
                op,
                expr: value,
            } => self.lower_failable_set_expression(target, *op, value, state, expr.span),
            ExprKind::For { clauses, body } => self.lower_failable_for(clauses, body, state),
            _ => {
                let source = self.lower_expr(expr, state)?;
                let dest = state.allocate_register(expr.span);
                let jump = self.emit_query_fast_fail(dest, source, state, expr.span);
                Ok((ValueOperand::Register(dest), vec![jump]))
            }
        }
    }

    fn lower_comparison_operand(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        match &expr.kind {
            ExprKind::Index { .. } | ExprKind::UnwrapOption(_) => {
                self.lower_failable_expr(expr, state)
            }
            ExprKind::BracketCall { callee, .. } if !self.is_callable(callee, state) => {
                self.lower_failable_expr(expr, state)
            }
            _ => Ok((self.lower_expr(expr, state)?, Vec::new())),
        }
    }

    fn lower_failable_set_expression(
        &mut self,
        target: &Expr,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        if let Some(slot) = self.slot_target_for_expr(target, state) {
            return self.lower_failable_slot_assignment(&slot, op, expr, state, span);
        }
        if let ExprKind::Member { object, name } = &target.kind
            && !is_self_expr(object)
        {
            return self.lower_failable_field_assignment(object, name, op, expr, state, span);
        }

        let name = match &target.kind {
            ExprKind::Ident(name) => name,
            ExprKind::Member { object, name } if is_self_expr(object) => name,
            _ => return Err(UnsupportedBytecode),
        };
        let Some(binding) = state.lookup(name) else {
            return Err(UnsupportedBytecode);
        };
        if !binding.mutable || !binding.ref_backed {
            return Err(UnsupportedBytecode);
        }
        let (right_source, failure_jumps) = self.lower_failable_expr(expr, state)?;
        let value = match op {
            AssignOp::Assign => right_source,
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                self.lower_compound_assignment_value(binding.operand, op, right_source, state, span)
            }
        };
        state.chunk.emit(Instruction::RefSet {
            ref_value: binding.operand,
            value,
            span,
        });
        Ok((value, failure_jumps))
    }

    fn lower_failable_field_assignment(
        &mut self,
        object: &Expr,
        name: &str,
        op: AssignOp,
        expr: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let (object, writebacks) = self.lower_field_object_for_writeback(object, state)?;
        let (right_source, failure_jumps) = self.lower_failable_expr(expr, state)?;
        let value = match op {
            AssignOp::Assign => right_source,
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign => {
                let left_dest = state.allocate_register(span);
                state.chunk.emit(Instruction::LoadField {
                    dest: left_dest,
                    object,
                    name: name.to_string(),
                    span,
                });
                self.lower_slot_binary_assignment(
                    ValueOperand::Register(left_dest),
                    op,
                    right_source,
                    state,
                    span,
                )
            }
        };
        state.chunk.emit(Instruction::SetField {
            object,
            name: name.to_string(),
            value,
            span,
        });
        self.emit_field_writebacks(&writebacks, state);
        Ok((value, failure_jumps))
    }

    fn lower_bool_expression_from_failable(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let result = state.allocate_register(expr.span);
        let failure_context_id = self.allocate_failure_context_id();
        let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
            on_failure: usize::MAX,
            id: failure_context_id,
            span: expr.span,
        });
        let (_, mut failure_jumps) = self.lower_failable_expr(expr, state)?;
        let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
            done: usize::MAX,
            id: failure_context_id,
            span: expr.span,
        });
        let true_value = state.constant(Constant::Bool(true));
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: true_value,
            span: expr.span,
        });
        let end_jump = state.chunk.emit_jump(Instruction::Jump {
            jump_offset: usize::MAX,
        });

        let false_start = state.chunk.next_instruction_index();
        state.chunk.patch_jump(begin_failure, false_start);
        for jump in failure_jumps.drain(..) {
            state.chunk.patch_jump(jump, false_start);
        }
        let false_value = state.constant(Constant::Bool(false));
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: false_value,
            span: expr.span,
        });

        let end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(end_failure, end);
        state.chunk.patch_jump(end_jump, end);
        Ok(ValueOperand::Register(result))
    }

    fn lower_failure_sequence(
        &mut self,
        clauses: &[Expr],
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let mut last = state.constant(Constant::Bool(true));
        let mut jumps = Vec::new();
        for clause in clauses {
            let (value, clause_jumps) = self.lower_failure_condition_value(clause, state)?;
            last = value;
            jumps.extend(clause_jumps);
        }
        Ok((last, jumps))
    }

    fn lower_failure_statements(
        &mut self,
        statements: &[Stmt],
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let mut last = state.none();
        let mut jumps = Vec::new();
        let span = statements
            .first()
            .map_or_else(|| Span::new(0, 0, 1, 1), |statement| statement.span);
        state.begin_defer_scope(span);
        for statement in statements {
            match &statement.kind {
                StmtKind::Let { name, expr, .. } => {
                    let iterable_kind = self
                        .iterable_kind_for_binding_span(statement.span)
                        .or_else(|| self.iterable_kind_for_expr(expr, state));
                    let (value, statement_jumps) = self.lower_failable_expr(expr, state)?;
                    state.define(
                        name.clone(),
                        Binding {
                            operand: value,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind,
                        },
                    );
                    self.mark_binding_callable_if_needed(state, name, statement.span);
                    last = value;
                    jumps.extend(statement_jumps);
                }
                StmtKind::Var { name, expr, .. } => {
                    let iterable_kind = self
                        .iterable_kind_for_binding_span(statement.span)
                        .or_else(|| self.iterable_kind_for_expr(expr, state));
                    let (value, statement_jumps) = self.lower_failable_expr(expr, state)?;
                    self.define_mutable_binding(
                        name.clone(),
                        value,
                        iterable_kind,
                        state,
                        statement.span,
                    );
                    self.mark_binding_callable_if_needed(state, name, statement.span);
                    last = value;
                    jumps.extend(statement_jumps);
                }
                StmtKind::Set { target, op, expr } => {
                    self.lower_assignment(target, *op, expr, state, statement.span)?;
                    last = state.none();
                }
                StmtKind::Defer(_) => {
                    self.lower_statement(statement, state)?;
                    last = state.none();
                }
                StmtKind::Expr(expr) => {
                    let (value, statement_jumps) =
                        self.lower_failure_condition_value(expr, state)?;
                    last = value;
                    jumps.extend(statement_jumps);
                }
                _ => return Err(UnsupportedBytecode),
            }
        }
        state.end_defer_scope(span);
        if !jumps.is_empty() {
            let success_jump = state.chunk.emit_jump(Instruction::Jump {
                jump_offset: usize::MAX,
            });
            let failure_cleanup = state.chunk.next_instruction_index();
            for jump in jumps.drain(..) {
                state.chunk.patch_jump(jump, failure_cleanup);
            }
            state.emit_defer_scope_exits(1, span);
            let false_value = state.constant(Constant::Bool(false));
            let failure_dest = state.allocate_register(span);
            let outer_failure_jump = state.chunk.emit_jump(Instruction::QueryFastFail {
                dest: failure_dest,
                leniency_indicator: failure_dest,
                source: false_value,
                on_failure: usize::MAX,
                span,
            });
            let end = state.chunk.next_instruction_index();
            state.chunk.patch_jump(success_jump, end);
            jumps.push(outer_failure_jump);
        }
        Ok((last, jumps))
    }

    fn lower_array_index_fast_fail(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let (array, index, span) = lower_failable_index_parts(expr)?;
        let array = self.lower_expr(array, state)?;
        let index = self.lower_expr(index, state)?;
        let dest = state.allocate_register(expr.span);
        let leniency_indicator = state.allocate_register(expr.span);
        let jump = state.chunk.emit_jump(Instruction::ArrayIndexFastFail {
            dest,
            leniency_indicator,
            array,
            index,
            on_failure: usize::MAX,
            span,
        });
        Ok((ValueOperand::Register(dest), vec![jump]))
    }

    fn lower_not_failable(
        &mut self,
        expr: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let result = state.allocate_register(expr.span);
        let failure_context_id = self.allocate_failure_context_id();
        let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
            on_failure: usize::MAX,
            id: failure_context_id,
            span: expr.span,
        });
        let (_, mut inner_jumps) = self.lower_failable_expr(expr, state)?;
        let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
            done: usize::MAX,
            id: failure_context_id,
            span: expr.span,
        });
        let failure_jump = state.chunk.emit_jump(Instruction::Jump {
            jump_offset: usize::MAX,
        });

        let success_start = state.chunk.next_instruction_index();
        state.chunk.patch_jump(begin_failure, success_start);
        for jump in inner_jumps.drain(..) {
            state.chunk.patch_jump(jump, success_start);
        }
        let true_value = state.constant(Constant::Bool(true));
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: true_value,
            span: expr.span,
        });

        let end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(end_failure, end);
        Ok((ValueOperand::Register(result), vec![failure_jump]))
    }

    fn lower_or_failable(
        &mut self,
        left: &Expr,
        right: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let result = state.allocate_register(span);
        let failure_context_id = self.allocate_failure_context_id();
        let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
            on_failure: usize::MAX,
            id: failure_context_id,
            span: left.span,
        });
        let (left_value, mut left_jumps) = self.lower_failable_expr(left, state)?;
        let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
            done: usize::MAX,
            id: failure_context_id,
            span: left.span,
        });
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: left_value,
            span: left.span,
        });
        let success_jump = state.chunk.emit_jump(Instruction::Jump {
            jump_offset: usize::MAX,
        });

        let right_start = state.chunk.next_instruction_index();
        state.chunk.patch_jump(begin_failure, right_start);
        for jump in left_jumps.drain(..) {
            state.chunk.patch_jump(jump, right_start);
        }
        let (right_value, right_jumps) = self.lower_failable_expr(right, state)?;
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: right_value,
            span: right.span,
        });

        let end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(end_failure, end);
        state.chunk.patch_jump(success_jump, end);
        Ok((ValueOperand::Register(result), right_jumps))
    }

    fn emit_query_fast_fail(
        &mut self,
        dest: RegisterIndex,
        source: ValueOperand,
        state: &mut ChunkState,
        span: Span,
    ) -> usize {
        let leniency_indicator = state.allocate_register(span);
        state.chunk.emit_jump(Instruction::QueryFastFail {
            dest,
            leniency_indicator,
            source,
            on_failure: usize::MAX,
            span,
        })
    }

    fn lower_option(
        &mut self,
        value: Option<&Expr>,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let Some(value) = value else {
            return Ok(state.constant(Constant::Option(None)));
        };

        let result = state.allocate_register(span);
        let failure_context_id = self.allocate_failure_context_id();
        let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
            on_failure: usize::MAX,
            id: failure_context_id,
            span: value.span,
        });
        let (value, mut failure_jumps) = if self.option_body_can_fail(value, state) {
            self.lower_failable_expr(value, state)?
        } else {
            (self.lower_expr(value, state)?, Vec::new())
        };
        let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
            done: usize::MAX,
            id: failure_context_id,
            span,
        });
        state.chunk.emit(Instruction::NewOption {
            dest: result,
            value,
            span,
        });
        let end_jump = state.chunk.emit_jump(Instruction::Jump {
            jump_offset: usize::MAX,
        });

        let failure_start = state.chunk.next_instruction_index();
        state.chunk.patch_jump(begin_failure, failure_start);
        for jump in failure_jumps.drain(..) {
            state.chunk.patch_jump(jump, failure_start);
        }
        let empty_option = state.constant(Constant::Option(None));
        state.chunk.emit(Instruction::Move {
            dest: result,
            source: empty_option,
            span,
        });

        let end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(end_failure, end);
        state.chunk.patch_jump(end_jump, end);
        Ok(ValueOperand::Register(result))
    }

    fn lower_block_value(
        &mut self,
        statements: &[Stmt],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let Some((last, prefix)) = statements.split_last() else {
            return Ok(state.none());
        };

        state.enter_scope();
        state.begin_defer_scope(span);
        for statement in prefix {
            self.lower_statement(statement, state)?;
        }

        let value = match &last.kind {
            StmtKind::Expr(expr) => self.lower_expr(expr, state)?,
            StmtKind::Return(_) => {
                self.lower_statement(last, state)?;
                state.none()
            }
            _ => {
                self.lower_statement(last, state)?;
                state.none()
            }
        };
        state.end_defer_scope(span);
        state.exit_scope();

        if matches!(value, ValueOperand::Uninitialized) {
            Ok(state.constant(Constant::None))
        } else {
            let _ = span;
            Ok(value)
        }
    }

    fn lower_loop(
        &mut self,
        body: &Expr,
        state: &mut ChunkState,
        _span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let loop_start = state.chunk.next_instruction_index();
        state.loop_breaks.push(LoopBreakContext {
            breaks: Vec::new(),
            defer_scope_depth: state.defer_scope_depth,
        });
        let _ = self.lower_expr(body, state)?;
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });
        let loop_end = state.chunk.next_instruction_index();

        let breaks = state
            .loop_breaks
            .pop()
            .expect("loop break stack should have current loop");
        for break_jump in breaks.breaks {
            state.chunk.patch_jump(break_jump, loop_end);
        }

        Ok(state.constant(Constant::None))
    }

    fn lower_for(
        &mut self,
        clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let Some((first_clause, tail_clauses)) = clauses.split_first() else {
            return Err(UnsupportedBytecode);
        };
        let ForClause::Generator {
            binding, iterable, ..
        } = first_clause
        else {
            return Err(UnsupportedBytecode);
        };

        if let ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } = &iterable.kind
        {
            let ForBinding::Value(binding) = binding else {
                return Err(UnsupportedBytecode);
            };
            return self.lower_range_for(
                binding,
                left,
                right,
                iterable.span,
                tail_clauses,
                body,
                state,
                span,
            );
        }

        match self.iterable_kind_for_expr(iterable, state) {
            Some(IterableKind::Indexed) => {
                self.lower_indexed_for(binding, iterable, tail_clauses, body, state, span)
            }
            Some(IterableKind::Map) => {
                self.lower_map_for(binding, iterable, tail_clauses, body, state, span)
            }
            None => Err(UnsupportedBytecode),
        }
    }

    fn lower_failable_for(
        &mut self,
        clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Vec<usize>), UnsupportedBytecode> {
        let Some((first_clause, tail_clauses)) = clauses.split_first() else {
            return Err(UnsupportedBytecode);
        };
        let ForClause::Generator {
            binding, iterable, ..
        } = first_clause
        else {
            return Err(UnsupportedBytecode);
        };

        let jumps = if let ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } = &iterable.kind
        {
            let ForBinding::Value(binding) = binding else {
                return Err(UnsupportedBytecode);
            };
            self.lower_failable_range_for(
                binding,
                left,
                right,
                iterable.span,
                tail_clauses,
                body,
                state,
            )?
        } else {
            match self.iterable_kind_for_expr(iterable, state) {
                Some(IterableKind::Indexed) => {
                    self.lower_failable_indexed_for(binding, iterable, tail_clauses, body, state)?
                }
                Some(IterableKind::Map) => {
                    self.lower_failable_map_for(binding, iterable, tail_clauses, body, state)?
                }
                None => return Err(UnsupportedBytecode),
            }
        };

        Ok((state.none(), jumps))
    }

    fn lower_range_for(
        &mut self,
        binding: &str,
        left: &Expr,
        right: &Expr,
        iterable_span: Span,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let start = self.lower_expr(left, state)?;
        let end = self.lower_expr(right, state)?;
        let one = state.constant(Constant::Int(1));
        let result = state.allocate_register(span);
        state.chunk.emit(Instruction::NewMutableArray {
            dest: result,
            values: Vec::new(),
            span,
        });

        let index = state.allocate_register(iterable_span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: start,
            span: iterable_span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable_span);
        let leniency_indicator = state.allocate_register(iterable_span);
        let exit_jump = state.chunk.emit_jump(Instruction::LteFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: end,
            on_failure: usize::MAX,
            span: iterable_span,
        });

        state.enter_scope();
        state.define(
            binding.to_owned(),
            Binding {
                operand: ValueOperand::Register(index),
                mutable: false,
                ref_backed: false,
                iterable_kind: None,
            },
        );
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable_span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        state.chunk.emit(Instruction::InPlaceMakeImmutable {
            dest: result,
            container: ValueOperand::Register(result),
            span,
        });
        Ok(ValueOperand::Register(result))
    }

    fn lower_indexed_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let container = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container,
            span: iterable.span,
        });

        let result = state.allocate_register(span);
        state.chunk.emit(Instruction::NewMutableArray {
            dest: result,
            values: Vec::new(),
            span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let element = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Call {
            dest: element,
            callee: container,
            arguments: vec![ValueOperand::Register(index)],
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span: iterable.span,
        });

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(index),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        state.chunk.emit(Instruction::InPlaceMakeImmutable {
            dest: result,
            container: ValueOperand::Register(result),
            span,
        });
        Ok(ValueOperand::Register(result))
    }

    fn lower_map_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let map = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container: map,
            span: iterable.span,
        });

        let result = state.allocate_register(span);
        state.chunk.emit(Instruction::NewMutableArray {
            dest: result,
            values: Vec::new(),
            span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let value_register = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::MapValue {
            dest: value_register,
            map,
            index: ValueOperand::Register(index),
            span: iterable.span,
        });

        let key_register = if matches!(binding, ForBinding::Pair { .. }) {
            let key_register = state.allocate_register(iterable.span);
            state.chunk.emit(Instruction::MapKey {
                dest: key_register,
                map,
                index: ValueOperand::Register(index),
                span: iterable.span,
            });
            Some(key_register)
        } else {
            None
        };

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                let key_register = key_register.expect("pair binding should emit map key");
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(key_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        state.chunk.emit(Instruction::InPlaceMakeImmutable {
            dest: result,
            container: ValueOperand::Register(result),
            span,
        });
        Ok(ValueOperand::Register(result))
    }

    fn lower_failable_range_for(
        &mut self,
        binding: &str,
        left: &Expr,
        right: &Expr,
        iterable_span: Span,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        let start = self.lower_expr(left, state)?;
        let end = self.lower_expr(right, state)?;
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable_span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: start,
            span: iterable_span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable_span);
        let leniency_indicator = state.allocate_register(iterable_span);
        let exit_jump = state.chunk.emit_jump(Instruction::LteFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: end,
            on_failure: usize::MAX,
            span: iterable_span,
        });

        state.enter_scope();
        state.define(
            binding.to_owned(),
            Binding {
                operand: ValueOperand::Register(index),
                mutable: false,
                ref_backed: false,
                iterable_kind: None,
            },
        );
        let jumps = self.lower_failable_for_tail_or_body(tail_clauses, body, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable_span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(jumps)
    }

    fn lower_failable_indexed_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        let container = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container,
            span: iterable.span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let element = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Call {
            dest: element,
            callee: container,
            arguments: vec![ValueOperand::Register(index)],
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span: iterable.span,
        });

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(index),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        let jumps = self.lower_failable_for_tail_or_body(tail_clauses, body, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(jumps)
    }

    fn lower_failable_map_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        let map = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container: map,
            span: iterable.span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let value_register = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::MapValue {
            dest: value_register,
            map,
            index: ValueOperand::Register(index),
            span: iterable.span,
        });

        let key_register = if matches!(binding, ForBinding::Pair { .. }) {
            let key_register = state.allocate_register(iterable.span);
            state.chunk.emit(Instruction::MapKey {
                dest: key_register,
                map,
                index: ValueOperand::Register(index),
                span: iterable.span,
            });
            Some(key_register)
        } else {
            None
        };

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                let key_register = key_register.expect("pair binding should emit map key");
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(key_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        let jumps = self.lower_failable_for_tail_or_body(tail_clauses, body, state)?;
        state.exit_scope();
        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(jumps)
    }

    fn lower_failable_for_tail_or_body(
        &mut self,
        clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        let Some((clause, tail)) = clauses.split_first() else {
            let (_, jumps) = self.lower_failable_expr(body, state)?;
            return Ok(jumps);
        };

        match clause {
            ForClause::Generator {
                binding, iterable, ..
            } => self.lower_failable_nested_for_generator(binding, iterable, tail, body, state),
            ForClause::RangeOrLet { name, expr, .. }
                if matches!(
                    expr.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Range,
                        ..
                    }
                ) =>
            {
                let ExprKind::Binary {
                    left,
                    op: BinaryOp::Range,
                    right,
                } = &expr.kind
                else {
                    unreachable!("range clause shape was checked above")
                };
                self.lower_failable_range_for(name, left, right, expr.span, tail, body, state)
            }
            ForClause::Let { name, expr, span } | ForClause::RangeOrLet { name, expr, span } => {
                let failure_context_id = self.allocate_failure_context_id();
                let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                    on_failure: usize::MAX,
                    id: failure_context_id,
                    span: *span,
                });
                let iterable_kind = self
                    .iterable_kind_for_binding_span(*span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let (value, mut failure_jumps) = self.lower_failable_expr(expr, state)?;
                state.define(
                    name.clone(),
                    Binding {
                        operand: value,
                        mutable: false,
                        ref_backed: false,
                        iterable_kind,
                    },
                );
                let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                    done: usize::MAX,
                    id: failure_context_id,
                    span: *span,
                });
                let tail_jumps = self.lower_failable_for_tail_or_body(tail, body, state)?;
                let failure_target = state.chunk.next_instruction_index();
                state.chunk.patch_jump(begin_failure, failure_target);
                state.chunk.patch_jump(end_failure, failure_target);
                for jump in failure_jumps.drain(..) {
                    state.chunk.patch_jump(jump, failure_target);
                }
                Ok(tail_jumps)
            }
            ForClause::Filter(expr) => {
                let failure_context_id = self.allocate_failure_context_id();
                let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                    on_failure: usize::MAX,
                    id: failure_context_id,
                    span: expr.span,
                });
                let (_, mut failure_jumps) = self.lower_failable_expr(expr, state)?;
                let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                    done: usize::MAX,
                    id: failure_context_id,
                    span: expr.span,
                });
                let tail_jumps = self.lower_failable_for_tail_or_body(tail, body, state)?;
                let failure_target = state.chunk.next_instruction_index();
                state.chunk.patch_jump(begin_failure, failure_target);
                state.chunk.patch_jump(end_failure, failure_target);
                for jump in failure_jumps.drain(..) {
                    state.chunk.patch_jump(jump, failure_target);
                }
                Ok(tail_jumps)
            }
        }
    }

    fn lower_failable_nested_for_generator(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        state: &mut ChunkState,
    ) -> Result<Vec<usize>, UnsupportedBytecode> {
        if let ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } = &iterable.kind
        {
            let ForBinding::Value(binding) = binding else {
                return Err(UnsupportedBytecode);
            };
            return self.lower_failable_range_for(
                binding,
                left,
                right,
                iterable.span,
                tail_clauses,
                body,
                state,
            );
        }

        match self.iterable_kind_for_expr(iterable, state) {
            Some(IterableKind::Indexed) => {
                self.lower_failable_indexed_for(binding, iterable, tail_clauses, body, state)
            }
            Some(IterableKind::Map) => {
                self.lower_failable_map_for(binding, iterable, tail_clauses, body, state)
            }
            None => Err(UnsupportedBytecode),
        }
    }

    fn lower_for_tail_or_body(
        &mut self,
        clauses: &[ForClause],
        body: &Expr,
        result: RegisterIndex,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        let Some((clause, tail)) = clauses.split_first() else {
            let body_value = self.lower_expr(body, state)?;
            state.chunk.emit(Instruction::ArrayAdd {
                dest: result,
                container: ValueOperand::Register(result),
                value_to_add: body_value,
                span: body.span,
            });
            return Ok(());
        };

        match clause {
            ForClause::Generator {
                binding, iterable, ..
            } => self.lower_nested_for_generator(binding, iterable, tail, body, result, state),
            ForClause::RangeOrLet { name, expr, .. }
                if matches!(
                    expr.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Range,
                        ..
                    }
                ) =>
            {
                let ExprKind::Binary {
                    left,
                    op: BinaryOp::Range,
                    right,
                } = &expr.kind
                else {
                    unreachable!("range clause shape was checked above")
                };
                self.lower_nested_range_for(name, left, right, expr.span, tail, body, result, state)
            }
            ForClause::Let { name, expr, span } | ForClause::RangeOrLet { name, expr, span } => {
                let failure_context_id = self.allocate_failure_context_id();
                let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                    on_failure: usize::MAX,
                    id: failure_context_id,
                    span: *span,
                });
                let iterable_kind = self
                    .iterable_kind_for_binding_span(*span)
                    .or_else(|| self.iterable_kind_for_expr(expr, state));
                let (value, mut failure_jumps) = self.lower_failable_expr(expr, state)?;
                state.define(
                    name.clone(),
                    Binding {
                        operand: value,
                        mutable: false,
                        ref_backed: false,
                        iterable_kind,
                    },
                );
                let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                    done: usize::MAX,
                    id: failure_context_id,
                    span: *span,
                });
                self.lower_for_tail_or_body(tail, body, result, state)?;
                let failure_target = state.chunk.next_instruction_index();
                state.chunk.patch_jump(begin_failure, failure_target);
                state.chunk.patch_jump(end_failure, failure_target);
                for jump in failure_jumps.drain(..) {
                    state.chunk.patch_jump(jump, failure_target);
                }
                Ok(())
            }
            ForClause::Filter(expr) => {
                let failure_context_id = self.allocate_failure_context_id();
                let begin_failure = state.chunk.emit_jump(Instruction::BeginFailureContext {
                    on_failure: usize::MAX,
                    id: failure_context_id,
                    span: expr.span,
                });
                let (_, mut failure_jumps) = self.lower_failable_expr(expr, state)?;
                let end_failure = state.chunk.emit_jump(Instruction::EndFailureContext {
                    done: usize::MAX,
                    id: failure_context_id,
                    span: expr.span,
                });
                self.lower_for_tail_or_body(tail, body, result, state)?;
                let failure_target = state.chunk.next_instruction_index();
                state.chunk.patch_jump(begin_failure, failure_target);
                state.chunk.patch_jump(end_failure, failure_target);
                for jump in failure_jumps.drain(..) {
                    state.chunk.patch_jump(jump, failure_target);
                }
                Ok(())
            }
        }
    }

    fn lower_nested_for_generator(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        result: RegisterIndex,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        if let ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } = &iterable.kind
        {
            let ForBinding::Value(binding) = binding else {
                return Err(UnsupportedBytecode);
            };
            return self.lower_nested_range_for(
                binding,
                left,
                right,
                iterable.span,
                tail_clauses,
                body,
                result,
                state,
            );
        }

        match self.iterable_kind_for_expr(iterable, state) {
            Some(IterableKind::Indexed) => {
                self.lower_nested_indexed_for(binding, iterable, tail_clauses, body, result, state)
            }
            Some(IterableKind::Map) => {
                self.lower_nested_map_for(binding, iterable, tail_clauses, body, result, state)
            }
            None => Err(UnsupportedBytecode),
        }
    }

    fn lower_nested_range_for(
        &mut self,
        binding: &str,
        left: &Expr,
        right: &Expr,
        iterable_span: Span,
        tail_clauses: &[ForClause],
        body: &Expr,
        result: RegisterIndex,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        let start = self.lower_expr(left, state)?;
        let end = self.lower_expr(right, state)?;
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable_span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: start,
            span: iterable_span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable_span);
        let leniency_indicator = state.allocate_register(iterable_span);
        let exit_jump = state.chunk.emit_jump(Instruction::LteFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: end,
            on_failure: usize::MAX,
            span: iterable_span,
        });

        state.enter_scope();
        state.define(
            binding.to_owned(),
            Binding {
                operand: ValueOperand::Register(index),
                mutable: false,
                ref_backed: false,
                iterable_kind: None,
            },
        );
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();

        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable_span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(())
    }

    fn lower_nested_indexed_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        result: RegisterIndex,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        let container = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container,
            span: iterable.span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let element = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Call {
            dest: element,
            callee: container,
            arguments: vec![ValueOperand::Register(index)],
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span: iterable.span,
        });

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(index),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(element),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();

        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(())
    }

    fn lower_nested_map_for(
        &mut self,
        binding: &ForBinding,
        iterable: &Expr,
        tail_clauses: &[ForClause],
        body: &Expr,
        result: RegisterIndex,
        state: &mut ChunkState,
    ) -> Result<(), UnsupportedBytecode> {
        let map = self.lower_expr(iterable, state)?;
        let length = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Length {
            dest: length,
            container: map,
            span: iterable.span,
        });

        let zero = state.constant(Constant::Int(0));
        let one = state.constant(Constant::Int(1));
        let index = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::Move {
            dest: index,
            source: zero,
            span: iterable.span,
        });

        let loop_start = state.chunk.next_instruction_index();
        let compare_dest = state.allocate_register(iterable.span);
        let leniency_indicator = state.allocate_register(iterable.span);
        let exit_jump = state.chunk.emit_jump(Instruction::LtFastFail {
            dest: compare_dest,
            leniency_indicator,
            lhs: ValueOperand::Register(index),
            rhs: ValueOperand::Register(length),
            on_failure: usize::MAX,
            span: iterable.span,
        });

        let value_register = state.allocate_register(iterable.span);
        state.chunk.emit(Instruction::MapValue {
            dest: value_register,
            map,
            index: ValueOperand::Register(index),
            span: iterable.span,
        });

        let key_register = if matches!(binding, ForBinding::Pair { .. }) {
            let key_register = state.allocate_register(iterable.span);
            state.chunk.emit(Instruction::MapKey {
                dest: key_register,
                map,
                index: ValueOperand::Register(index),
                span: iterable.span,
            });
            Some(key_register)
        } else {
            None
        };

        state.enter_scope();
        match binding {
            ForBinding::Value(binding) => {
                state.define(
                    binding.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
            ForBinding::Pair { key, value } => {
                let key_register = key_register.expect("pair binding should emit map key");
                state.define(
                    key.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(key_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
                state.define(
                    value.to_owned(),
                    Binding {
                        operand: ValueOperand::Register(value_register),
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: None,
                    },
                );
            }
        }
        self.lower_for_tail_or_body(tail_clauses, body, result, state)?;
        state.exit_scope();

        state.chunk.emit(Instruction::Add {
            dest: index,
            left_source: ValueOperand::Register(index),
            right_source: one,
            span: iterable.span,
        });
        state.chunk.emit(Instruction::Jump {
            jump_offset: loop_start,
        });

        let loop_end = state.chunk.next_instruction_index();
        state.chunk.patch_jump(exit_jump, loop_end);
        Ok(())
    }

    fn lower_spawn(
        &mut self,
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let captures = self.collect_function_captures(&[], body, state);
        let function = self.lower_task_function_with_context(
            Some("__verse_spawn_body".to_string()),
            &[],
            &["suspends".to_string()],
            body,
            &state.imports,
            &state.extensions,
            &captures,
        )?;
        let dest = state.allocate_register(span);
        let callee = state.constant(Constant::Function(function));
        let callee = if captures.is_empty() {
            callee
        } else {
            let parent_scope = self.emit_capture_scope(&captures, state, span);
            let function_dest = state.allocate_register(span);
            let none = state.none();
            state.chunk.emit(Instruction::NewFunction {
                dest: function_dest,
                procedure: callee,
                self_value: none,
                parent_scope,
                span,
            });
            ValueOperand::Register(function_dest)
        };
        state.chunk.emit(Instruction::CallTask {
            dest,
            parent: ValueOperand::Uninitialized,
            callee,
            arguments: Vec::new(),
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_concurrent(
        &mut self,
        op: ConcurrentOp,
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let ExprKind::ColonBlock(statements) = &body.kind else {
            return Err(UnsupportedBytecode);
        };
        let mut arguments = Vec::with_capacity(statements.len());
        for (index, statement) in statements.iter().enumerate() {
            let function = self.lower_task_statement_with_context(
                Some(format!(
                    "__verse_{}_branch_{index}",
                    concurrent_native_suffix(op)
                )),
                statement,
                &state.imports,
                &state.extensions,
            )?;
            arguments.push(state.constant(Constant::Function(function)));
        }
        let dest = state.allocate_register(span);
        let callee = state.constant(Constant::NativeFunction(format!(
            "__verse_{}",
            concurrent_native_suffix(op)
        )));
        state.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments,
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        if let ExprKind::Member { object, name } = &callee.kind
            && name == "Length"
        {
            if !args.is_empty() {
                return Err(UnsupportedBytecode);
            }
            return self.lower_length(object, state, span);
        }

        if let ExprKind::Member { object, name } = &callee.kind
            && let Some(extension) =
                self.select_extension_for_call(object, args, state.lookup_extensions(name))
        {
            return self.lower_extension_call(object, args, extension, state, span);
        }

        if let ExprKind::QualifiedMember {
            object,
            qualifier,
            name,
        } = &callee.kind
            && let Some(extension) = self.select_extension_for_call(
                object,
                args,
                state.lookup_qualified_extensions(qualifier, name),
            )
        {
            return self.lower_extension_call(object, args, extension, state, span);
        }

        if let ExprKind::QualifiedName { qualifier, name } = &callee.kind
            && qualifier == "super"
        {
            return self.lower_super_call(name, args, state, span);
        }

        let implicit_self_member = if let ExprKind::Ident(name) = &callee.kind
            && state.lookup(name).is_none()
            && state.lookup("Self").is_some()
            && !self.callable_names.contains(name)
            && !bytecode_native_function_name(name)
        {
            Some(name.as_str())
        } else {
            None
        };

        if let ExprKind::Ident(name) = &callee.kind
            && implicit_self_member.is_none()
            && !self.callable_names.contains(name)
            && !bytecode_native_function_name(name)
            && state.lookup(name).is_none()
        {
            return Err(UnsupportedBytecode);
        }

        let await_sequence = call_uses_await_sequence(callee);
        let has_named_args = args.iter().any(|arg| matches!(arg, CallArg::Named { .. }));
        let direct_callee_name = callable_lookup_name(callee);
        let selected_function = direct_callee_name
            .as_deref()
            .filter(|name| !bytecode_native_function_name(name))
            .and_then(|name| self.select_function_descriptor_for_call(name, args));

        let callee = if let Some(name) = implicit_self_member {
            let self_binding = state.lookup("Self").ok_or(UnsupportedBytecode)?;
            let dest = state.allocate_register(span);
            state.chunk.emit(Instruction::LoadField {
                dest,
                object: self_binding.operand,
                name: name.to_string(),
                span,
            });
            ValueOperand::Register(dest)
        } else if let Some(function) = selected_function {
            state.constant(Constant::Function(function))
        } else {
            self.lower_expr(callee, state)?
        };
        let (arguments, named_arguments, named_argument_values) = if has_named_args {
            if let Some(function) = selected_function {
                (
                    self.lower_function_call_arguments(function, args, state)?,
                    Vec::new(),
                    Vec::new(),
                )
            } else if let Some(name) = direct_callee_name.as_ref() {
                if bytecode_native_function_name(name) {
                    self.lower_native_call_arguments(args, state)?
                } else if self.has_function_descriptor(name) {
                    (
                        self.lower_named_call_arguments(name, args, state)?,
                        Vec::new(),
                        Vec::new(),
                    )
                } else {
                    self.lower_native_call_arguments(args, state)?
                }
            } else {
                self.lower_native_call_arguments(args, state)?
            }
        } else if let Some(function) = selected_function {
            (
                self.lower_function_call_arguments(function, args, state)?,
                Vec::new(),
                Vec::new(),
            )
        } else {
            args.iter()
                .map(|arg| {
                    let CallArg::Positional(arg) = arg else {
                        return Err(UnsupportedBytecode);
                    };
                    self.lower_expr(arg, state)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|arguments| (arguments, Vec::new(), Vec::new()))?
        };
        let dest = state.allocate_register(span);
        if await_sequence {
            state.chunk.emit(Instruction::BeginAwait { span });
        }
        state.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments,
            named_arguments,
            named_argument_values,
            callee_yields: true,
            span,
        });
        if await_sequence {
            state.chunk.emit(Instruction::AwaitSuccess { span });
            state.chunk.emit(Instruction::EndAwait { span });
        }
        Ok(ValueOperand::Register(dest))
    }

    fn has_function_descriptor(&self, name: &str) -> bool {
        self.function_descriptor_index(name).is_some()
    }

    fn function_descriptor_index(&self, name: &str) -> Option<usize> {
        self.functions
            .iter()
            .rev()
            .position(|descriptor| descriptor.name() == Some(name))
            .map(|reverse_index| self.functions.len() - 1 - reverse_index)
    }

    fn select_function_descriptor_for_call(&self, name: &str, args: &[CallArg]) -> Option<usize> {
        self.functions
            .iter()
            .enumerate()
            .filter(|(_, descriptor)| descriptor.name() == Some(name))
            .filter_map(|(index, descriptor)| {
                self.function_descriptor_call_score(descriptor, args)
                    .map(|score| (index, score))
            })
            .min_by_key(|(_, score)| *score)
            .map(|(index, _)| index)
    }

    fn function_descriptor_call_score(
        &self,
        descriptor: &FunctionDescriptor,
        args: &[CallArg],
    ) -> Option<usize> {
        self.call_argument_type_match_score(
            descriptor.params(),
            descriptor.source_params(),
            descriptor.param_types(),
            descriptor.param_defaults(),
            args,
            true,
        )
    }

    fn select_extension_for_call(
        &self,
        object: &Expr,
        args: &[CallArg],
        extensions: Vec<ExtensionBinding>,
    ) -> Option<ExtensionBinding> {
        extensions
            .into_iter()
            .filter_map(|extension| {
                self.extension_call_score(&extension, object, args)
                    .map(|score| (extension, score))
            })
            .min_by_key(|(_, score)| *score)
            .map(|(extension, _)| extension)
    }

    fn extension_call_score(
        &self,
        extension: &ExtensionBinding,
        object: &Expr,
        args: &[CallArg],
    ) -> Option<usize> {
        let mut score = 0usize;
        if let Some(expected) = extension.receiver_type.as_ref()
            && let Some(actual) = self.facts.expression_type(object.span)
        {
            score += self.extension_receiver_type_match_score(expected, actual)?;
        }
        score += self.call_argument_type_match_score(
            &extension.params,
            &extension.source_params,
            &extension.param_types,
            &extension.param_defaults,
            args,
            false,
        )?;
        Some(score)
    }

    fn extension_receiver_type_match_score(&self, expected: &Type, actual: &Type) -> Option<usize> {
        if bytecode_type_matches(expected, actual) {
            return Some(bytecode_argument_match_score(expected, actual));
        }
        let expected_name = aggregate_runtime_type_name(expected)?;
        self.receiver_runtime_type_names(actual)
            .into_iter()
            .any(|actual_name| {
                self.parametric_type_name_pattern_matches(expected_name, &actual_name)
            })
            .then_some(2)
            .or_else(|| {
                self.receiver_type_is_parametric_pattern(expected)
                    .then_some(8)
            })
    }

    fn receiver_runtime_type_names(&self, actual: &Type) -> Vec<String> {
        if let Type::Param(_, constraint) = actual {
            return self.receiver_runtime_type_names_from_constraint(constraint);
        }
        let Some(name) = aggregate_runtime_type_name(actual) else {
            return Vec::new();
        };
        self.receiver_runtime_type_names_from_name(name)
    }

    fn receiver_runtime_type_names_from_constraint(
        &self,
        constraint: &TypeParamConstraint,
    ) -> Vec<String> {
        match constraint {
            TypeParamConstraint::Type => Vec::new(),
            TypeParamConstraint::Subtype(parent) => {
                self.receiver_runtime_type_names_from_type_name(parent)
            }
            TypeParamConstraint::TypeBounds { upper, .. } => {
                self.receiver_runtime_type_names_from_type_name(upper)
            }
        }
    }

    fn receiver_runtime_type_names_from_type_name(&self, name: &TypeName) -> Vec<String> {
        if let Some(inner) = official_subtype_type_name_payload(name) {
            return self.receiver_runtime_type_names_from_type_name(inner);
        }
        if let Some(runtime_name) = render_aggregate_runtime_type_name_from_type_name(name) {
            return self.receiver_runtime_type_names_from_name(&runtime_name);
        }
        let value_type = bytecode_type_from_type_name(name);
        if let Some(runtime_name) = aggregate_runtime_type_name(&value_type) {
            return self.receiver_runtime_type_names_from_name(runtime_name);
        }
        Vec::new()
    }

    fn receiver_runtime_type_names_from_name(&self, name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        self.collect_receiver_runtime_type_names(name, &mut seen, &mut names);
        names
    }

    fn collect_receiver_runtime_type_names(
        &self,
        name: &str,
        seen: &mut HashSet<String>,
        names: &mut Vec<String>,
    ) {
        if !seen.insert(name.to_string()) {
            return;
        }
        names.push(name.to_string());
        if let Some(layout) = self.class_layouts.get(name) {
            for interface in &layout.interfaces {
                self.collect_receiver_runtime_type_names(interface, seen, names);
            }
            if let Some(base) = &layout.base_class {
                self.collect_receiver_runtime_type_names(base, seen, names);
            }
        }
    }

    fn parametric_type_name_pattern_matches(&self, pattern: &str, actual: &str) -> bool {
        let mut inferred = HashMap::new();
        self.parametric_type_name_pattern_matches_inner(pattern, actual, &mut inferred)
    }

    fn parametric_type_name_pattern_matches_inner(
        &self,
        pattern: &str,
        actual: &str,
        inferred: &mut HashMap<String, String>,
    ) -> bool {
        if runtime_names_match(pattern, actual) {
            return true;
        }
        if self.is_type_param_pattern_atom(pattern) {
            return match inferred.get(pattern) {
                Some(existing) => runtime_names_match(existing, actual),
                None => {
                    inferred.insert(pattern.to_string(), actual.to_string());
                    true
                }
            };
        }
        let Some((pattern_head, pattern_args)) = parse_parametric_instance_name(pattern) else {
            return false;
        };
        let Some((actual_head, actual_args)) = parse_parametric_instance_name(actual) else {
            return false;
        };
        runtime_names_match(&pattern_head, &actual_head)
            && pattern_args.len() == actual_args.len()
            && pattern_args
                .iter()
                .zip(actual_args.iter())
                .all(|(pattern, actual)| {
                    self.parametric_type_name_pattern_matches_inner(pattern, actual, inferred)
                })
    }

    fn is_type_param_pattern_atom(&self, name: &str) -> bool {
        !name.contains('.')
            && !name.contains('(')
            && !is_builtin_type_atom(name)
            && !self.class_layouts.contains_key(name)
            && !self.interface_layouts.contains_key(name)
    }

    fn receiver_type_is_parametric_pattern(&self, value_type: &Type) -> bool {
        match value_type {
            Type::Param(_, _) => true,
            Type::Class(name) | Type::Interface(name) | Type::Struct(name) => {
                self.parametric_name_contains_type_param_atom(name)
            }
            Type::Array(item)
            | Type::Option(item)
            | Type::Subtype(item)
            | Type::CastableSubtype(item)
            | Type::ConcreteSubtype(item)
            | Type::ClassifiableSubset(item)
            | Type::ClassifiableSubsetKey(item)
            | Type::ClassifiableSubsetVar(item)
            | Type::Modifier(item)
            | Type::ModifierStack(item)
            | Type::Signalable(item) => self.receiver_type_is_parametric_pattern(item),
            _ => false,
        }
    }

    fn parametric_name_contains_type_param_atom(&self, name: &str) -> bool {
        let Some((_, args)) = parse_parametric_instance_name(name) else {
            return false;
        };
        args.iter().any(|arg| {
            self.is_type_param_pattern_atom(arg)
                || self.parametric_name_contains_type_param_atom(arg)
        })
    }

    fn call_argument_type_match_score(
        &self,
        param_names: &[String],
        source_params: &[Param],
        param_types: &[Option<Type>],
        param_defaults: &[Option<Expr>],
        args: &[CallArg],
        enforce_types: bool,
    ) -> Option<usize> {
        let param_count = param_names.len();
        let mut assigned = vec![false; param_count];
        let mut actuals = vec![None; param_count];
        let mut score = 0usize;
        let mut positional_index = 0usize;

        if let Some(items) = single_tuple_binding_param_items(source_params, param_count)
            && args.len() == items.len()
            && args.iter().all(|arg| matches!(arg, CallArg::Positional(_)))
        {
            assigned[0] = true;
            actuals[0] = Some(Type::Tuple(
                args.iter()
                    .map(|arg| {
                        self.facts
                            .expression_type(positional_call_arg_expr(arg)?.span)
                            .cloned()
                    })
                    .collect::<Option<Vec<_>>>()?,
            ));
        } else if !source_params.is_empty()
            && let [CallArg::Positional(expr)] = args
            && self.expr_is_tuple_with_len(expr, source_params.len())
            && source_params.len() > 1
        {
            let Type::Tuple(items) = self.facts.expression_type(expr.span)? else {
                return None;
            };
            let mut flat_index = 0usize;
            for (param, item_type) in source_params.iter().zip(items) {
                self.assign_param_pattern_type(
                    param,
                    item_type,
                    &mut flat_index,
                    &mut assigned,
                    &mut actuals,
                )?;
            }
        } else {
            for arg in args {
                match arg {
                    CallArg::Positional(expr) => {
                        while positional_index < param_count && assigned[positional_index] {
                            positional_index += 1;
                        }
                        if positional_index >= param_count {
                            return None;
                        }
                        if let Some((param, start, end)) =
                            find_param_for_flat_index(source_params, positional_index)
                            && start == positional_index
                            && self.expr_is_tuple_param_value(expr, param)
                        {
                            let Type::Tuple(items) = self.facts.expression_type(expr.span)? else {
                                return None;
                            };
                            let mut flat_index = start;
                            self.assign_tuple_param_pattern_type(
                                param,
                                items,
                                &mut flat_index,
                                &mut assigned,
                                &mut actuals,
                            )?;
                            positional_index = end;
                        } else {
                            assigned[positional_index] = true;
                            actuals[positional_index] =
                                self.facts.expression_type(expr.span).cloned();
                            positional_index += 1;
                        }
                    }
                    CallArg::Named { name, expr, .. } => {
                        let param_index = param_names.iter().position(|param| param == name)?;
                        if assigned[param_index] {
                            return None;
                        }
                        assigned[param_index] = true;
                        actuals[param_index] = self.facts.expression_type(expr.span).cloned();
                    }
                }
            }
        }

        for (param_index, actual) in actuals.iter().enumerate() {
            if !assigned[param_index] {
                continue;
            }
            let expected = param_types.get(param_index).and_then(Option::as_ref);
            if !bytecode_argument_type_matches(expected, actual.as_ref()) {
                if enforce_types {
                    return None;
                }
                score += 8;
                continue;
            }
            if let (Some(expected), Some(actual)) = (expected, actual.as_ref()) {
                score += bytecode_argument_match_score(expected, actual);
            }
        }

        for (index, assigned) in assigned.iter().enumerate() {
            if !assigned && param_defaults.get(index).is_none_or(Option::is_none) {
                return None;
            }
        }

        Some(score)
    }

    fn assign_param_pattern_type(
        &self,
        param: &Param,
        actual: &Type,
        flat_index: &mut usize,
        assigned: &mut [bool],
        actuals: &mut [Option<Type>],
    ) -> Option<()> {
        match &param.pattern {
            ParamPattern::Binding | ParamPattern::Anonymous => {
                if assigned.get(*flat_index).copied().unwrap_or(true) {
                    return None;
                }
                assigned[*flat_index] = true;
                actuals[*flat_index] = Some(actual.clone());
                *flat_index += 1;
                Some(())
            }
            ParamPattern::Tuple(_) => {
                let Type::Tuple(actual_items) = actual else {
                    return None;
                };
                self.assign_tuple_param_pattern_type(
                    param,
                    actual_items,
                    flat_index,
                    assigned,
                    actuals,
                )
            }
        }
    }

    fn assign_tuple_param_pattern_type(
        &self,
        param: &Param,
        actual_items: &[Type],
        flat_index: &mut usize,
        assigned: &mut [bool],
        actuals: &mut [Option<Type>],
    ) -> Option<()> {
        let ParamPattern::Tuple(items) = &param.pattern else {
            return None;
        };
        if items.len() != actual_items.len() {
            return None;
        }
        for (item, actual) in items.iter().zip(actual_items) {
            self.assign_param_pattern_type(item, actual, flat_index, assigned, actuals)?;
        }
        Some(())
    }

    fn function_return_class_by_arity(
        &self,
        name: &str,
        arity: usize,
        state: &ChunkState,
    ) -> Option<String> {
        self.function_return_classes
            .get(&(name.to_string(), arity))
            .cloned()
            .or_else(|| {
                self.function_return_classes
                    .iter()
                    .find_map(|((candidate, _), class_name)| {
                        (candidate == name).then(|| class_name.clone())
                    })
            })
            .or_else(|| {
                state.imports.iter().rev().find_map(|module| {
                    self.function_return_classes
                        .get(&(format!("{module}.{name}"), arity))
                        .cloned()
                        .or_else(|| {
                            let qualified = format!("{module}.{name}");
                            self.function_return_classes.iter().find_map(
                                |((candidate, _), class_name)| {
                                    (candidate == &qualified).then(|| class_name.clone())
                                },
                            )
                        })
                })
            })
    }

    fn lower_extension_call(
        &mut self,
        object: &Expr,
        args: &[CallArg],
        extension: ExtensionBinding,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let mut arguments = Vec::with_capacity(args.len() + 1 + extension.fields.len() + 1);
        if extension.captures_self {
            let self_binding = state.lookup("Self").ok_or(UnsupportedBytecode)?;
            arguments.push(self_binding.operand);
            for field in &extension.fields {
                let binding = state.lookup(field).ok_or(UnsupportedBytecode)?;
                arguments.push(binding.operand);
            }
        }
        arguments.push(self.lower_expr(object, state)?);
        if args.iter().any(|arg| matches!(arg, CallArg::Named { .. })) {
            arguments.extend(self.lower_call_arguments_for_params(
                &extension.params,
                &[],
                &extension.param_defaults,
                args,
                state,
            )?);
        } else {
            for arg in args {
                let CallArg::Positional(expr) = arg else {
                    return Err(UnsupportedBytecode);
                };
                arguments.push(self.lower_expr(expr, state)?);
            }
        }
        let dest = state.allocate_register(span);
        let callee = state.constant(Constant::Function(extension.function));
        state.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments,
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_super_call(
        &mut self,
        name: &str,
        args: &[CallArg],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let base_class = state.super_class.clone().ok_or(UnsupportedBytecode)?;
        let self_binding = state.lookup("Self").ok_or(UnsupportedBytecode)?;
        let callee_dest = state.allocate_register(span);
        state.chunk.emit(Instruction::LoadFieldFromSuper {
            dest: callee_dest,
            object: self_binding.operand,
            base_class,
            name: name.to_string(),
            span,
        });
        let has_named_args = args.iter().any(|arg| matches!(arg, CallArg::Named { .. }));
        let (arguments, named_arguments, named_argument_values) = if has_named_args {
            self.lower_native_call_arguments(args, state)?
        } else {
            args.iter()
                .map(|arg| {
                    let CallArg::Positional(arg) = arg else {
                        return Err(UnsupportedBytecode);
                    };
                    self.lower_expr(arg, state)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|arguments| (arguments, Vec::new(), Vec::new()))?
        };
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::Call {
            dest,
            callee: ValueOperand::Register(callee_dest),
            arguments,
            named_arguments,
            named_argument_values,
            callee_yields: true,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_length(
        &mut self,
        object: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let container = self.lower_expr(object, state)?;
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::Length {
            dest,
            container,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_field_access(
        &mut self,
        object: &Expr,
        name: &str,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let object = self.lower_expr(object, state)?;
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::LoadField {
            dest,
            object,
            name: name.to_string(),
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn lower_builtin_member_path(
        &mut self,
        path: &str,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<Option<ValueOperand>, UnsupportedBytecode> {
        if let Some(Type::Enum(enum_name)) = self.facts.expression_type(span)
            && let Some((_, variant)) = path.rsplit_once('.')
        {
            return Ok(Some(state.constant(Constant::EnumValue {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
            })));
        }
        if let Some((enum_name, variant)) = path.rsplit_once('.')
            && let Some(layout) = self.resolve_enum_layout(enum_name, state)
            && layout.variants.contains(variant)
        {
            return Ok(Some(state.constant(Constant::EnumValue {
                enum_name: layout.runtime_name.clone(),
                variant: variant.to_string(),
            })));
        }
        let Some(color_name) = path.strip_prefix("NamedColors.") else {
            return Ok(None);
        };
        let Some(color) = NAMED_COLORS
            .iter()
            .find(|candidate| candidate.name == color_name)
        else {
            return Ok(None);
        };
        Ok(Some(self.emit_native_call(
            "MakeColorFromSRGBValues",
            vec![
                state.constant(Constant::Int(i64::from(color.red))),
                state.constant(Constant::Int(i64::from(color.green))),
                state.constant(Constant::Int(i64::from(color.blue))),
            ],
            state,
            span,
        )))
    }

    fn emit_native_call(
        &mut self,
        name: &'static str,
        arguments: Vec<ValueOperand>,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        let callee = state.constant(Constant::NativeFunction(name.to_string()));
        state.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments,
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span,
        });
        ValueOperand::Register(dest)
    }

    fn lower_archetype(
        &mut self,
        callee: &Expr,
        entries: &[ArchetypeEntry],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let class_name = if matches!(&callee.kind, ExprKind::Ident(name) if name == "super") {
            state.super_class.clone().ok_or(UnsupportedBytecode)?
        } else {
            let Some(class_name) = archetype_callee_name(callee) else {
                return Err(UnsupportedBytecode);
            };
            class_name
        };
        let class_name = if self.resolve_class_layout(&class_name, state).is_some()
            || matches!(
                class_name.as_str(),
                "event" | "generator" | "sticky_event" | "color" | "color_alpha"
            ) {
            class_name
        } else {
            self.archetype_type_value_layout_name(callee, state)
                .unwrap_or(class_name)
        };
        if let Some(value_type) = self.archetype_type_value_external_type_name(callee) {
            if !entries.is_empty() {
                return Err(UnsupportedBytecode);
            }
            return Ok(self.emit_external(value_type, state, span));
        }
        if matches!(class_name.as_str(), "event" | "generator" | "sticky_event") {
            if !entries.is_empty() {
                return Err(UnsupportedBytecode);
            }
            let value_type = self
                .facts
                .expression_type(span)
                .and_then(type_to_runtime_type_name)
                .unwrap_or_else(|| TypeName::Applied {
                    name: class_name,
                    args: Vec::new(),
                });
            return Ok(self.emit_external(value_type, state, span));
        }
        if class_name == "color" {
            return self.lower_color_archetype(entries, state, span);
        }
        if class_name == "color_alpha" {
            return self.lower_color_alpha_archetype(entries, state, span);
        }
        let Some(layout) = self.resolve_class_layout(&class_name, state).cloned() else {
            return Err(UnsupportedBytecode);
        };

        state.enter_scope();
        let mut explicit_fields = HashMap::new();
        for entry in entries {
            match entry {
                ArchetypeEntry::Field(field) => {
                    let value = self.lower_expr(&field.expr, state)?;
                    explicit_fields.insert(field.name.clone(), value);
                }
                ArchetypeEntry::Let(binding) => {
                    let value = self.lower_expr(&binding.expr, state)?;
                    state.define(
                        binding.name.clone(),
                        Binding {
                            operand: value,
                            mutable: false,
                            ref_backed: false,
                            iterable_kind: self
                                .facts
                                .expression_type(binding.expr.span)
                                .and_then(iterable_kind_from_type)
                                .or_else(|| self.iterable_kind_for_expr(&binding.expr, state)),
                        },
                    );
                }
                ArchetypeEntry::ConstructorCall(call) => {
                    let (delegated, delegated_class) =
                        self.lower_archetype_constructor_call(call, state)?;
                    if let Some(delegated_class) = delegated_class {
                        if !self.class_is_same_or_superclass(
                            &layout.runtime_name,
                            &delegated_class,
                            state,
                        ) {
                            state.exit_scope();
                            return Err(UnsupportedBytecode);
                        }
                        if runtime_names_match(&delegated_class, &layout.runtime_name) {
                            explicit_fields.clear();
                        }
                        let delegated_layout = self
                            .resolve_class_layout(&delegated_class, state)
                            .cloned()
                            .unwrap_or_else(|| layout.clone());
                        for field in delegated_layout.fields {
                            if layout
                                .fields
                                .iter()
                                .any(|candidate| candidate.name == field.name)
                            {
                                let dest = state.allocate_register(call.span);
                                state.chunk.emit(Instruction::LoadField {
                                    dest,
                                    object: delegated,
                                    name: field.name.clone(),
                                    span: call.span,
                                });
                                explicit_fields
                                    .insert(field.name.clone(), ValueOperand::Register(dest));
                            }
                        }
                    }
                }
                ArchetypeEntry::Block(expr) => {
                    let _ = self.lower_expr(expr, state)?;
                }
            }
        }

        let mut fields = Vec::with_capacity(layout.fields.len());
        for field in &layout.fields {
            let value = if let Some(value) = explicit_fields.remove(&field.name) {
                value
            } else {
                let value_expr = field.default.as_ref().ok_or(UnsupportedBytecode)?;
                self.lower_expr(value_expr, state)?
            };
            fields.push((field.name.clone(), field.mutable, value));
        }
        state.exit_scope();

        if !explicit_fields.is_empty() {
            return Err(UnsupportedBytecode);
        }

        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::NewObject {
            dest,
            class_name: layout.runtime_name,
            object_kind: layout.object_kind,
            fields,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn archetype_type_value_external_type_name(&self, callee: &Expr) -> Option<TypeName> {
        let Type::TypeValueOf(item) = self.facts.expression_type(callee.span)? else {
            return None;
        };
        match item.as_ref() {
            Type::Event(_) | Type::StickyEvent(_) => type_to_runtime_type_name(item.as_ref()),
            _ => None,
        }
    }

    fn archetype_type_value_layout_name(
        &self,
        callee: &Expr,
        state: &ChunkState,
    ) -> Option<String> {
        let target_name = match self.facts.expression_type(callee.span)? {
            Type::StructType(name)
            | Type::ClassType(name)
            | Type::Struct(name)
            | Type::Class(name) => name.clone(),
            Type::TypeValueOf(item) => match item.as_ref() {
                Type::Struct(name) | Type::Class(name) => name.clone(),
                _ => return None,
            },
            _ => return None,
        };

        if self.resolve_class_layout(&target_name, state).is_some() {
            return Some(target_name);
        }
        let erased = erase_parametric_instance_name(&target_name);
        if erased != target_name && self.resolve_class_layout(erased, state).is_some() {
            return Some(erased.to_string());
        }
        Some(target_name)
    }

    fn lower_color_archetype(
        &mut self,
        entries: &[ArchetypeEntry],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let mut red = None;
        let mut green = None;
        let mut blue = None;
        for entry in entries {
            let ArchetypeEntry::Field(field) = entry else {
                return Err(UnsupportedBytecode);
            };
            let value = self.lower_expr(&field.expr, state)?;
            match field.name.as_str() {
                "R" if red.is_none() => red = Some(value),
                "G" if green.is_none() => green = Some(value),
                "B" if blue.is_none() => blue = Some(value),
                _ => return Err(UnsupportedBytecode),
            }
        }
        Ok(self.emit_native_call(
            "MakeColorFromSRGB",
            vec![
                red.ok_or(UnsupportedBytecode)?,
                green.ok_or(UnsupportedBytecode)?,
                blue.ok_or(UnsupportedBytecode)?,
            ],
            state,
            span,
        ))
    }

    fn lower_color_alpha_archetype(
        &mut self,
        entries: &[ArchetypeEntry],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let mut color = None;
        let mut alpha = None;
        for entry in entries {
            let ArchetypeEntry::Field(field) = entry else {
                return Err(UnsupportedBytecode);
            };
            let value = self.lower_expr(&field.expr, state)?;
            match field.name.as_str() {
                "Color" if color.is_none() => color = Some(value),
                "A" if alpha.is_none() => alpha = Some(value),
                _ => return Err(UnsupportedBytecode),
            }
        }
        let color = color.ok_or(UnsupportedBytecode)?;
        let red = self.lower_color_component(color, "R", state, span);
        let green = self.lower_color_component(color, "G", state, span);
        let blue = self.lower_color_component(color, "B", state, span);
        Ok(self.emit_native_call(
            "MakeColorAlpha",
            vec![red, green, blue, alpha.ok_or(UnsupportedBytecode)?],
            state,
            span,
        ))
    }

    fn lower_color_component(
        &mut self,
        color: ValueOperand,
        name: &str,
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::LoadField {
            dest,
            object: color,
            name: name.to_string(),
            span,
        });
        ValueOperand::Register(dest)
    }

    fn lower_archetype_constructor_call(
        &mut self,
        call: &ArchetypeConstructorCall,
        state: &mut ChunkState,
    ) -> Result<(ValueOperand, Option<String>), UnsupportedBytecode> {
        let function = self
            .select_function_descriptor_for_call(&call.name, &call.args)
            .or_else(|| self.function_descriptor_index(&call.name))
            .ok_or(UnsupportedBytecode)?;
        let has_named_args = call
            .args
            .iter()
            .any(|arg| matches!(arg, CallArg::Named { .. }));
        let arguments = if has_named_args {
            self.lower_function_call_arguments(function, &call.args, state)?
        } else {
            self.lower_function_call_arguments(function, &call.args, state)?
        };
        let dest = state.allocate_register(call.span);
        let callee = state.constant(Constant::Function(function));
        state.chunk.emit(Instruction::Call {
            dest,
            callee,
            arguments,
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span: call.span,
        });
        Ok((
            ValueOperand::Register(dest),
            self.function_return_class_by_arity(&call.name, call.args.len(), state),
        ))
    }

    fn class_is_same_or_superclass(
        &self,
        target_class: &str,
        candidate_class: &str,
        state: &ChunkState,
    ) -> bool {
        if runtime_names_match(candidate_class, target_class) {
            return true;
        }
        let mut current = self
            .resolve_class_layout(target_class, state)
            .and_then(|layout| layout.base_class.clone());
        while let Some(class_name) = current {
            if runtime_names_match(candidate_class, &class_name) {
                return true;
            }
            current = self
                .resolve_class_layout(&class_name, state)
                .and_then(|layout| layout.base_class.clone());
        }
        false
    }

    fn resolve_class_layout(&self, name: &str, state: &ChunkState) -> Option<&ClassLayout> {
        self.class_layouts.get(name).or_else(|| {
            state.imports.iter().rev().find_map(|module| {
                let qualified = format!("{module}.{name}");
                self.class_layouts.get(&qualified)
            })
        })
    }

    fn resolve_enum_layout(&self, name: &str, state: &ChunkState) -> Option<&EnumLayout> {
        self.enum_layouts.get(name).or_else(|| {
            state.imports.iter().rev().find_map(|module| {
                let qualified = format!("{module}.{name}");
                self.enum_layouts.get(&qualified)
            })
        })
    }

    fn resolve_class_descriptor(&self, name: &str) -> Option<&ClassDescriptor> {
        self.classes
            .iter()
            .rev()
            .find(|class| class.name == name)
            .or_else(|| {
                self.class_layouts.get(name).and_then(|layout| {
                    self.classes
                        .iter()
                        .rev()
                        .find(|class| class.name == layout.runtime_name)
                })
            })
    }

    fn resolve_interface_layout(&self, name: &str) -> Option<&InterfaceLayout> {
        self.interface_layouts.get(name)
    }

    fn lower_named_call_arguments(
        &mut self,
        callee_name: &str,
        args: &[CallArg],
        state: &mut ChunkState,
    ) -> Result<Vec<ValueOperand>, UnsupportedBytecode> {
        if let Some(param_aliases) = bytecode_native_param_aliases(callee_name) {
            return self.lower_native_named_call_arguments(param_aliases, args, state);
        }

        let function = self
            .select_function_descriptor_for_call(callee_name, args)
            .ok_or(UnsupportedBytecode)?;
        self.lower_function_call_arguments(function, args, state)
    }

    fn lower_function_call_arguments(
        &mut self,
        function: usize,
        args: &[CallArg],
        state: &mut ChunkState,
    ) -> Result<Vec<ValueOperand>, UnsupportedBytecode> {
        let descriptor = self
            .functions
            .get(function)
            .ok_or(UnsupportedBytecode)?
            .clone();
        self.lower_call_arguments_for_params(
            descriptor.params(),
            descriptor.source_params(),
            descriptor.param_defaults(),
            args,
            state,
        )
    }

    fn lower_call_arguments_for_params(
        &mut self,
        params: &[String],
        source_params: &[Param],
        defaults: &[Option<Expr>],
        args: &[CallArg],
        state: &mut ChunkState,
    ) -> Result<Vec<ValueOperand>, UnsupportedBytecode> {
        let mut assigned = vec![false; params.len()];
        let mut values = vec![ValueOperand::Uninitialized; params.len()];
        let mut positional_index = 0usize;

        if let Some(items) = single_tuple_binding_param_items(source_params, params.len())
            && args.len() == items.len()
            && args.iter().all(|arg| matches!(arg, CallArg::Positional(_)))
        {
            let span = args
                .first()
                .and_then(positional_call_arg_expr)
                .map_or(source_params[0].span, |expr| expr.span);
            let tuple_values = args
                .iter()
                .map(|arg| {
                    let Some(expr) = positional_call_arg_expr(arg) else {
                        return Err(UnsupportedBytecode);
                    };
                    self.lower_expr(expr, state)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let dest = state.allocate_register(span);
            state.chunk.emit(Instruction::NewArray {
                dest,
                values: tuple_values,
                span,
            });
            assigned[0] = true;
            values[0] = ValueOperand::Register(dest);
        } else if !source_params.is_empty()
            && let [CallArg::Positional(expr)] = args
            && self.expr_is_tuple_with_len(expr, source_params.len())
            && source_params.len() > 1
        {
            let tuple = self.lower_expr(expr, state)?;
            let mut flat_index = 0usize;
            for (tuple_index, param) in source_params.iter().enumerate() {
                let item = self.lower_tuple_item(tuple, tuple_index, expr.span, state);
                self.assign_param_pattern_operand(
                    param,
                    item,
                    &mut flat_index,
                    &mut assigned,
                    &mut values,
                    state,
                    expr.span,
                )?;
            }
        } else {
            for arg in args {
                match arg {
                    CallArg::Positional(expr) => {
                        while positional_index < params.len() && assigned[positional_index] {
                            positional_index += 1;
                        }
                        if positional_index >= params.len() {
                            return Err(UnsupportedBytecode);
                        }
                        if let Some((param, start, end)) =
                            find_param_for_flat_index(source_params, positional_index)
                            && start == positional_index
                            && self.expr_is_tuple_param_value(expr, param)
                        {
                            let tuple = self.lower_expr(expr, state)?;
                            let mut flat_index = start;
                            self.assign_tuple_param_pattern_operand(
                                param,
                                tuple,
                                &mut flat_index,
                                &mut assigned,
                                &mut values,
                                state,
                                expr.span,
                            )?;
                            positional_index = end;
                        } else {
                            assigned[positional_index] = true;
                            values[positional_index] = self.lower_expr(expr, state)?;
                            positional_index += 1;
                        }
                    }
                    CallArg::Named { name, expr, .. } => {
                        let Some(param_index) = params.iter().position(|param| param == name)
                        else {
                            return Err(UnsupportedBytecode);
                        };
                        if assigned[param_index] {
                            return Err(UnsupportedBytecode);
                        }
                        assigned[param_index] = true;
                        values[param_index] = self.lower_expr(expr, state)?;
                    }
                }
            }
        }

        state.enter_scope();
        for index in 0..params.len() {
            if !assigned[index] {
                let default = defaults
                    .get(index)
                    .and_then(Option::as_ref)
                    .ok_or(UnsupportedBytecode)?;
                values[index] = self.lower_expr(default, state)?;
                assigned[index] = true;
            }
            state.define(
                params[index].clone(),
                Binding {
                    operand: values[index],
                    mutable: false,
                    ref_backed: false,
                    iterable_kind: None,
                },
            );
        }
        state.exit_scope();

        Ok(values)
    }

    fn expr_is_tuple_with_len(&self, expr: &Expr, len: usize) -> bool {
        matches!(self.facts.expression_type(expr.span), Some(Type::Tuple(items)) if items.len() == len)
    }

    fn expr_is_tuple_param_value(&self, expr: &Expr, param: &Param) -> bool {
        match &param.pattern {
            ParamPattern::Tuple(items) => self.expr_is_tuple_with_len(expr, items.len()),
            _ => false,
        }
    }

    fn lower_tuple_item(
        &mut self,
        tuple: ValueOperand,
        index: usize,
        span: Span,
        state: &mut ChunkState,
    ) -> ValueOperand {
        let dest = state.allocate_register(span);
        let index = state.constant(Constant::Int(index as i64));
        state.chunk.emit(Instruction::Call {
            dest,
            callee: tuple,
            arguments: vec![index],
            named_arguments: Vec::new(),
            named_argument_values: Vec::new(),
            callee_yields: true,
            span,
        });
        ValueOperand::Register(dest)
    }

    fn assign_tuple_param_pattern_operand(
        &mut self,
        param: &Param,
        tuple: ValueOperand,
        flat_index: &mut usize,
        assigned: &mut [bool],
        values: &mut [ValueOperand],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        let ParamPattern::Tuple(items) = &param.pattern else {
            return self.assign_param_pattern_operand(
                param, tuple, flat_index, assigned, values, state, span,
            );
        };
        for (tuple_index, item) in items.iter().enumerate() {
            let operand = self.lower_tuple_item(tuple, tuple_index, span, state);
            self.assign_param_pattern_operand(
                item, operand, flat_index, assigned, values, state, span,
            )?;
        }
        Ok(())
    }

    fn assign_param_pattern_operand(
        &mut self,
        param: &Param,
        operand: ValueOperand,
        flat_index: &mut usize,
        assigned: &mut [bool],
        values: &mut [ValueOperand],
        state: &mut ChunkState,
        span: Span,
    ) -> Result<(), UnsupportedBytecode> {
        match &param.pattern {
            ParamPattern::Binding | ParamPattern::Anonymous => {
                if assigned.get(*flat_index).copied().unwrap_or(true) {
                    return Err(UnsupportedBytecode);
                }
                assigned[*flat_index] = true;
                values[*flat_index] = operand;
                *flat_index += 1;
                Ok(())
            }
            ParamPattern::Tuple(_) => self.assign_tuple_param_pattern_operand(
                param, operand, flat_index, assigned, values, state, span,
            ),
        }
    }

    fn lower_native_call_arguments(
        &mut self,
        args: &[CallArg],
        state: &mut ChunkState,
    ) -> Result<(Vec<ValueOperand>, Vec<String>, Vec<ValueOperand>), UnsupportedBytecode> {
        let mut positional = Vec::new();
        let mut named_arguments = Vec::new();
        let mut named_argument_values = Vec::new();
        for arg in args {
            match arg {
                CallArg::Positional(expr) => positional.push(self.lower_expr(expr, state)?),
                CallArg::Named {
                    name,
                    expr,
                    optional: _,
                    ..
                } => {
                    named_arguments.push(name.clone());
                    named_argument_values.push(self.lower_expr(expr, state)?);
                }
            }
        }
        Ok((positional, named_arguments, named_argument_values))
    }

    fn lower_native_named_call_arguments(
        &mut self,
        param_aliases: Vec<Vec<&'static str>>,
        args: &[CallArg],
        state: &mut ChunkState,
    ) -> Result<Vec<ValueOperand>, UnsupportedBytecode> {
        let mut assigned = vec![false; param_aliases.len()];
        let mut values = vec![ValueOperand::Uninitialized; param_aliases.len()];
        let mut positional_index = 0usize;

        for arg in args {
            match arg {
                CallArg::Positional(expr) => {
                    while positional_index < param_aliases.len() && assigned[positional_index] {
                        positional_index += 1;
                    }
                    if positional_index >= param_aliases.len() {
                        return Err(UnsupportedBytecode);
                    }
                    assigned[positional_index] = true;
                    values[positional_index] = self.lower_expr(expr, state)?;
                    positional_index += 1;
                }
                CallArg::Named {
                    name,
                    expr,
                    optional: _,
                    ..
                } => {
                    let Some(param_index) = param_aliases
                        .iter()
                        .position(|aliases| aliases.iter().any(|alias| *alias == name))
                    else {
                        return Err(UnsupportedBytecode);
                    };
                    if assigned[param_index] {
                        return Err(UnsupportedBytecode);
                    }
                    assigned[param_index] = true;
                    values[param_index] = self.lower_expr(expr, state)?;
                }
            }
        }

        let last_assigned = assigned
            .iter()
            .rposition(|assigned| *assigned)
            .ok_or(UnsupportedBytecode)?;
        if assigned
            .iter()
            .take(last_assigned + 1)
            .any(|assigned| !*assigned)
        {
            return Err(UnsupportedBytecode);
        }
        Ok(values.into_iter().take(last_assigned + 1).collect())
    }

    fn emit_function_value(
        &mut self,
        name: Option<String>,
        params: &[Param],
        effects: &[String],
        body: &Expr,
        state: &mut ChunkState,
        span: Span,
    ) -> Result<ValueOperand, UnsupportedBytecode> {
        let captures = self.collect_function_captures(params, body, state);
        let function = self.lower_function_with_context(
            name,
            params,
            effects,
            body,
            &state.imports,
            &state.extensions,
            &captures,
        )?;
        let procedure = state.constant(Constant::Function(function));
        let none = state.none();
        let parent_scope = self.emit_capture_scope(&captures, state, span);
        let dest = state.allocate_register(span);
        state.chunk.emit(Instruction::NewFunction {
            dest,
            procedure,
            self_value: none,
            parent_scope,
            span,
        });
        Ok(ValueOperand::Register(dest))
    }

    fn emit_capture_scope(
        &self,
        captures: &[CaptureBinding],
        state: &mut ChunkState,
        span: Span,
    ) -> ValueOperand {
        if captures.is_empty() {
            return state.none();
        }
        let dest = state.allocate_register(span);
        let values = captures
            .iter()
            .map(|capture| capture.binding.operand)
            .collect();
        state
            .chunk
            .emit(Instruction::NewScope { dest, values, span });
        ValueOperand::Register(dest)
    }

    fn collect_function_captures(
        &self,
        params: &[Param],
        body: &Expr,
        state: &ChunkState,
    ) -> Vec<CaptureBinding> {
        collect_function_captures(params, body, state)
            .into_iter()
            .filter(|capture| !self.capture_is_globally_resolved(&capture.name))
            .collect()
    }

    fn capture_is_globally_resolved(&self, name: &str) -> bool {
        self.global_bindings.contains_key(name)
            || self.callable_names.contains(name)
            || self.class_layouts.contains_key(name)
            || self.enum_layouts.contains_key(name)
            || self.interface_layouts.contains_key(name)
    }

    fn lower_function(
        &mut self,
        name: Option<String>,
        params: &[Param],
        effects: &[String],
        body: &Expr,
    ) -> Result<usize, UnsupportedBytecode> {
        self.lower_function_with_context(name, params, effects, body, &[], &[], &[])
    }

    fn lower_task_function_with_context(
        &mut self,
        name: Option<String>,
        params: &[Param],
        effects: &[String],
        body: &Expr,
        imports: &[String],
        extensions: &[ExtensionBinding],
        captures: &[CaptureBinding],
    ) -> Result<usize, UnsupportedBytecode> {
        let param_names = lower_param_names(params)?;
        let decides = effects.iter().any(|effect| effect == "decides");
        let mut state = ChunkState::with_params(
            name.as_ref()
                .map_or_else(|| "<task>".to_string(), |name| name.clone()),
            params,
            self.facts,
        );
        self.install_global_bindings(&mut state);
        state.imports = imports.to_vec();
        state.extensions = extensions.to_vec();
        self.install_captures(captures, &mut state, body.span);
        let task_handle = state.allocate_register(body.span);
        state.chunk.emit(Instruction::BeginTask {
            dest: task_handle,
            parent: ValueOperand::Uninitialized,
            add_to_task_group: false,
            on_yield: usize::MAX,
            span: body.span,
        });
        state.begin_defer_scope(body.span);
        let value = if decides {
            let (value, mut failure_jumps) = self.lower_failable_expr(body, &mut state)?;
            let failure_start = state.chunk.next_instruction_index();
            for jump in failure_jumps.drain(..) {
                state.chunk.patch_jump(jump, failure_start);
            }
            value
        } else {
            self.lower_expr(body, &mut state)?
        };
        state.end_defer_scope(body.span);
        let task_end = state.chunk.next_instruction_index();
        if let Some(Instruction::BeginTask { on_yield, .. }) = state
            .chunk
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::BeginTask { .. }))
        {
            *on_yield = task_end;
        }
        state.chunk.emit(Instruction::EndTask {
            write: None,
            switch: None,
            value,
            which: ValueOperand::Uninitialized,
            signal: None,
            span: body.span,
        });
        let param_types = lower_param_types(params, self.facts);
        let param_defaults = lower_param_defaults(params);
        self.push_function_descriptor(
            name,
            param_names,
            params.to_vec(),
            param_types,
            param_defaults,
            state,
            decides,
        )
    }

    fn lower_task_statement_with_context(
        &mut self,
        name: Option<String>,
        statement: &Stmt,
        imports: &[String],
        extensions: &[ExtensionBinding],
    ) -> Result<usize, UnsupportedBytecode> {
        let mut state = ChunkState::new(
            name.as_ref()
                .map_or_else(|| "<task-branch>".to_string(), |name| name.clone()),
        );
        self.install_global_bindings(&mut state);
        state.imports = imports.to_vec();
        state.extensions = extensions.to_vec();
        let task_handle = state.allocate_register(statement.span);
        state.chunk.emit(Instruction::BeginTask {
            dest: task_handle,
            parent: ValueOperand::Uninitialized,
            add_to_task_group: false,
            on_yield: usize::MAX,
            span: statement.span,
        });
        state.begin_defer_scope(statement.span);
        let value = match &statement.kind {
            StmtKind::Expr(expr) => self.lower_expr(expr, &mut state)?,
            StmtKind::Let { name, expr, .. } => {
                let value = self.lower_expr(expr, &mut state)?;
                state.define(
                    name.clone(),
                    Binding {
                        operand: value,
                        mutable: false,
                        ref_backed: false,
                        iterable_kind: self
                            .facts
                            .expression_type(expr.span)
                            .and_then(iterable_kind_from_type)
                            .or_else(|| self.iterable_kind_for_expr(expr, &state)),
                    },
                );
                self.mark_binding_callable_if_needed(&mut state, name, statement.span);
                value
            }
            StmtKind::Var { name, expr, .. } => {
                let value = self.lower_expr(expr, &mut state)?;
                let iterable_kind = self
                    .facts
                    .expression_type(expr.span)
                    .and_then(iterable_kind_from_type)
                    .or_else(|| self.iterable_kind_for_expr(expr, &state));
                self.define_mutable_binding(
                    name.clone(),
                    value,
                    iterable_kind,
                    &mut state,
                    statement.span,
                );
                self.mark_binding_callable_if_needed(&mut state, name, statement.span);
                value
            }
            _ => {
                self.lower_statement(statement, &mut state)?;
                state.last_value
            }
        };
        state.end_defer_scope(statement.span);
        let task_end = state.chunk.next_instruction_index();
        if let Some(Instruction::BeginTask { on_yield, .. }) = state
            .chunk
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::BeginTask { .. }))
        {
            *on_yield = task_end;
        }
        state.chunk.emit(Instruction::EndTask {
            write: None,
            switch: None,
            value,
            which: ValueOperand::Uninitialized,
            signal: None,
            span: statement.span,
        });
        self.push_function_descriptor(
            name,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            state,
            false,
        )
    }

    fn lower_function_with_context(
        &mut self,
        name: Option<String>,
        params: &[Param],
        effects: &[String],
        body: &Expr,
        imports: &[String],
        extensions: &[ExtensionBinding],
        captures: &[CaptureBinding],
    ) -> Result<usize, UnsupportedBytecode> {
        let param_names = lower_param_names(params)?;
        let decides = effects.iter().any(|effect| effect == "decides");
        let mut state = ChunkState::with_params(
            name.as_ref()
                .map_or_else(|| "<anonymous>".to_string(), |name| name.clone()),
            params,
            self.facts,
        );
        self.install_global_bindings(&mut state);
        state.imports = imports.to_vec();
        state.extensions = extensions.to_vec();
        self.install_captures(captures, &mut state, body.span);
        state.begin_defer_scope(body.span);
        let value = if decides {
            let (value, mut failure_jumps) = self.lower_failable_expr(body, &mut state)?;
            state.end_defer_scope(body.span);
            state.chunk.emit(Instruction::Return {
                value,
                span: body.span,
            });
            let failure_start = state.chunk.next_instruction_index();
            for jump in failure_jumps.drain(..) {
                state.chunk.patch_jump(jump, failure_start);
            }
            state.emit_defer_scope_exits(state.defer_scope_depth, body.span);
            let param_types = lower_param_types(params, self.facts);
            let param_defaults = lower_param_defaults(params);
            return self.push_function_descriptor(
                name,
                param_names,
                params.to_vec(),
                param_types,
                param_defaults,
                state,
                decides,
            );
        } else {
            self.lower_expr(body, &mut state)?
        };
        state.end_defer_scope(body.span);
        state.chunk.emit(Instruction::Return {
            value,
            span: body.span,
        });

        let param_types = lower_param_types(params, self.facts);
        let param_defaults = lower_param_defaults(params);
        self.push_function_descriptor(
            name,
            param_names,
            params.to_vec(),
            param_types,
            param_defaults,
            state,
            decides,
        )
    }

    fn push_function_descriptor(
        &mut self,
        name: Option<String>,
        params: Vec<String>,
        source_params: Vec<Param>,
        param_types: Vec<Option<Type>>,
        param_defaults: Vec<Option<Expr>>,
        state: ChunkState,
        decides: bool,
    ) -> Result<usize, UnsupportedBytecode> {
        let chunk_index = self.chunks.len() + 1;
        let function_index = self.functions.len();
        self.functions.push(FunctionDescriptor {
            name,
            params,
            source_params,
            param_types,
            param_defaults,
            chunk: chunk_index,
            decides,
        });
        self.chunks.push(state.finish());
        Ok(function_index)
    }

    fn allocate_failure_context_id(&mut self) -> usize {
        let id = self.next_failure_context_id;
        self.next_failure_context_id += 1;
        id
    }

    fn is_callable(&self, callee: &Expr, state: &ChunkState) -> bool {
        if self
            .facts
            .expression_type(callee.span)
            .is_some_and(bytecode_type_is_callable)
        {
            return true;
        }
        match &callee.kind {
            ExprKind::Ident(name) => {
                self.callable_names.contains(name)
                    || state.lookup_callable(name)
                    || bytecode_native_function_name(name)
                    || self.class_layouts.contains_key(name)
            }
            ExprKind::Member { .. } | ExprKind::QualifiedMember { .. } => true,
            ExprKind::Call { callee, .. } => self.is_callable(callee, state),
            _ => false,
        }
    }

    fn option_body_can_fail(&self, expr: &Expr, state: &ChunkState) -> bool {
        match &expr.kind {
            ExprKind::FailureBind { .. }
            | ExprKind::FailureSequence(_)
            | ExprKind::UnwrapOption(_)
            | ExprKind::Index { .. }
            | ExprKind::For { .. } => true,
            ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => statements
                .iter()
                .any(|statement| self.option_statement_can_fail(statement, state)),
            ExprKind::Case { subject, arms } => {
                self.option_body_can_fail(subject, state)
                    || !case_arms_have_wildcard(arms)
                    || arms.iter().any(|arm| {
                        let pattern_can_fail = match &arm.pattern {
                            CasePattern::Wildcard { .. } => false,
                            CasePattern::Expr(pattern) => self.option_body_can_fail(pattern, state),
                        };
                        pattern_can_fail || self.option_body_can_fail(&arm.expr, state)
                    })
            }
            ExprKind::Profile { body, .. } => self.option_body_can_fail(body, state),
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => self.option_body_can_fail(expr, state),
            ExprKind::Binary { left, op, right } => {
                matches!(
                    op,
                    BinaryOp::Divide
                        | BinaryOp::Remainder
                        | BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ) || self.option_body_can_fail(left, state)
                    || self.option_body_can_fail(right, state)
            }
            ExprKind::Member { object, .. } | ExprKind::QualifiedMember { object, .. } => {
                self.option_body_can_fail(object, state)
            }
            ExprKind::Call { callee, .. } => self.option_body_can_fail(callee, state),
            ExprKind::BracketCall { callee, args } => {
                !self.is_callable(callee, state)
                    || self.option_body_can_fail(callee, state)
                    || args.iter().any(|arg| self.option_body_can_fail(arg, state))
            }
            ExprKind::Var { expr, .. } => self.option_body_can_fail(expr, state),
            ExprKind::Set { target, expr, .. } => {
                self.option_body_can_fail(target, state) || self.option_body_can_fail(expr, state)
            }
            ExprKind::Spawn { .. } | ExprKind::Concurrent { .. } => false,
            _ => false,
        }
    }

    fn option_statement_can_fail(&self, statement: &Stmt, state: &ChunkState) -> bool {
        match &statement.kind {
            StmtKind::Let { expr, .. }
            | StmtKind::Var { expr, .. }
            | StmtKind::Return(expr)
            | StmtKind::Expr(expr) => self.option_body_can_fail(expr, state),
            StmtKind::Set { target, expr, .. } => {
                self.option_body_can_fail(target, state) || self.option_body_can_fail(expr, state)
            }
            StmtKind::Defer(_)
            | StmtKind::Using { .. }
            | StmtKind::Break
            | StmtKind::TypeAlias { .. }
            | StmtKind::ScopedAccessLevel { .. }
            | StmtKind::ParametricType { .. }
            | StmtKind::ParametricTypeAlias { .. }
            | StmtKind::ExtensionMethod(_) => false,
        }
    }
}

fn collect_top_level_function_names(program: &Program) -> HashSet<String> {
    program
        .statements
        .iter()
        .filter_map(|statement| {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                return None;
            };
            matches!(expr.kind, ExprKind::Function { .. }).then(|| name.clone())
        })
        .collect()
}

fn collect_function_captures(
    params: &[Param],
    body: &Expr,
    state: &ChunkState,
) -> Vec<CaptureBinding> {
    let mut bound = HashSet::new();
    for param in params {
        collect_param_bound_names(param, &mut bound);
    }

    let mut names = Vec::new();
    collect_capture_names_expr(body, &mut bound, &mut names);

    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .filter_map(|name| {
            state.lookup(&name).map(|binding| CaptureBinding {
                callable: state.lookup_callable(&name),
                name,
                binding,
            })
        })
        .collect()
}

fn collect_param_bound_names(param: &Param, bound: &mut HashSet<String>) {
    match &param.pattern {
        ParamPattern::Binding => {
            bound.insert(param.name.clone());
        }
        ParamPattern::Anonymous => {}
        ParamPattern::Tuple(items) => {
            for item in items {
                collect_param_bound_names(item, bound);
            }
        }
    }
}

fn collect_for_binding_names(binding: &ForBinding, bound: &mut HashSet<String>) {
    match binding {
        ForBinding::Value(name) => {
            bound.insert(name.clone());
        }
        ForBinding::Pair { key, value } => {
            bound.insert(key.clone());
            bound.insert(value.clone());
        }
    }
}

fn collect_capture_names_stmt(stmt: &Stmt, bound: &mut HashSet<String>, names: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Let { name, expr, .. } | StmtKind::Var { name, expr, .. } => {
            collect_capture_names_expr(expr, bound, names);
            bound.insert(name.clone());
        }
        StmtKind::Set { target, expr, .. } => {
            collect_capture_names_expr(target, bound, names);
            collect_capture_names_expr(expr, bound, names);
        }
        StmtKind::Return(expr) | StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
            collect_capture_names_expr(expr, bound, names);
        }
        StmtKind::ExtensionMethod(method) => {
            if let Some(body) = method.method.body.as_ref() {
                collect_capture_names_expr(body, bound, names);
            }
        }
        StmtKind::ParametricType { name, expr, .. } => {
            collect_capture_names_expr(expr, bound, names);
            bound.insert(name.clone());
        }
        StmtKind::Using { .. }
        | StmtKind::TypeAlias { .. }
        | StmtKind::ParametricTypeAlias { .. }
        | StmtKind::ScopedAccessLevel { .. }
        | StmtKind::Break => {}
    }
}

fn collect_capture_names_expr(expr: &Expr, bound: &mut HashSet<String>, names: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident(name) | ExprKind::QualifiedName { name, .. } => {
            if !bound.contains(name) {
                names.push(name.clone());
            }
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::TypeLiteral { expr }
        | ExprKind::Option(Some(expr))
        | ExprKind::UnwrapOption(expr) => {
            collect_capture_names_expr(expr, bound, names);
        }
        ExprKind::TypeAnnotationLiteral { .. } => {}
        ExprKind::Binary { left, right, .. } => {
            collect_capture_names_expr(left, bound, names);
            collect_capture_names_expr(right, bound, names);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_capture_names_expr(condition, bound, names);
            collect_capture_names_expr(then_branch, bound, names);
            if let Some(else_branch) = else_branch {
                collect_capture_names_expr(else_branch, bound, names);
            }
        }
        ExprKind::FailureBind { name, expr } => {
            collect_capture_names_expr(expr, bound, names);
            bound.insert(name.clone());
        }
        ExprKind::FailureSequence(clauses)
        | ExprKind::Array(clauses)
        | ExprKind::Tuple(clauses) => {
            for clause in clauses {
                collect_capture_names_expr(clause, bound, names);
            }
        }
        ExprKind::Set { target, expr, .. } => {
            collect_capture_names_expr(target, bound, names);
            collect_capture_names_expr(expr, bound, names);
        }
        ExprKind::Var {
            name, expr: value, ..
        } => {
            collect_capture_names_expr(value, bound, names);
            bound.insert(name.clone());
        }
        ExprKind::Loop { body }
        | ExprKind::Spawn { body }
        | ExprKind::Concurrent { body, .. }
        | ExprKind::Profile { body, .. } => {
            collect_capture_names_expr(body, bound, names);
        }
        ExprKind::For { clauses, body } => {
            let mut loop_bound = bound.clone();
            for clause in clauses {
                match clause {
                    ForClause::Generator {
                        binding, iterable, ..
                    } => {
                        collect_capture_names_expr(iterable, &mut loop_bound, names);
                        collect_for_binding_names(binding, &mut loop_bound);
                    }
                    ForClause::Let { name, expr, .. }
                    | ForClause::RangeOrLet { name, expr, .. } => {
                        collect_capture_names_expr(expr, &mut loop_bound, names);
                        loop_bound.insert(name.clone());
                    }
                    ForClause::Filter(expr) => {
                        collect_capture_names_expr(expr, &mut loop_bound, names);
                    }
                }
            }
            collect_capture_names_expr(body, &mut loop_bound, names);
        }
        ExprKind::Block(statements) | ExprKind::ColonBlock(statements) => {
            let mut block_bound = bound.clone();
            for statement in statements {
                collect_capture_names_stmt(statement, &mut block_bound, names);
            }
        }
        ExprKind::Call { callee, args } => {
            collect_capture_names_expr(callee, bound, names);
            for arg in args {
                collect_capture_names_call_arg(arg, bound, names);
            }
        }
        ExprKind::BracketCall { callee, args } => {
            collect_capture_names_expr(callee, bound, names);
            for arg in args {
                collect_capture_names_expr(arg, bound, names);
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                collect_capture_names_expr(key, bound, names);
                collect_capture_names_expr(value, bound, names);
            }
        }
        ExprKind::Archetype {
            callee, entries, ..
        } => {
            collect_capture_names_expr(callee, bound, names);
            for entry in entries {
                collect_capture_names_archetype_entry(entry, bound, names);
            }
        }
        ExprKind::Case { subject, arms } => {
            collect_capture_names_expr(subject, bound, names);
            for arm in arms {
                if let CasePattern::Expr(pattern) = &arm.pattern {
                    collect_capture_names_expr(pattern, bound, names);
                }
                collect_capture_names_expr(&arm.expr, bound, names);
            }
        }
        ExprKind::QualifiedMember { object, .. }
        | ExprKind::Member { object, .. }
        | ExprKind::Index {
            collection: object, ..
        } => {
            collect_capture_names_expr(object, bound, names);
            if let ExprKind::Index { index, .. } = &expr.kind {
                collect_capture_names_expr(index, bound, names);
            }
        }
        ExprKind::InterpolatedString(parts) => {
            for part in parts {
                if let InterpolatedStringPart::Expr(part) = part {
                    collect_capture_names_expr(part, bound, names);
                }
            }
        }
        ExprKind::Function { .. }
        | ExprKind::Number { .. }
        | ExprKind::Char { .. }
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::None
        | ExprKind::External
        | ExprKind::Option(None)
        | ExprKind::EnumDefinition { .. }
        | ExprKind::StructDefinition { .. }
        | ExprKind::ClassDefinition { .. }
        | ExprKind::InterfaceDefinition { .. }
        | ExprKind::ModuleDefinition { .. } => {}
    }
}

fn collect_capture_names_call_arg(
    arg: &CallArg,
    bound: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    match arg {
        CallArg::Positional(expr) | CallArg::Named { expr, .. } => {
            collect_capture_names_expr(expr, bound, names);
        }
    }
}

fn collect_capture_names_archetype_entry(
    entry: &ArchetypeEntry,
    bound: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    match entry {
        ArchetypeEntry::Field(field) => collect_capture_names_expr(&field.expr, bound, names),
        ArchetypeEntry::Let(binding) => {
            collect_capture_names_expr(&binding.expr, bound, names);
            bound.insert(binding.name.clone());
        }
        ArchetypeEntry::Block(expr) => collect_capture_names_expr(expr, bound, names),
        ArchetypeEntry::ConstructorCall(call) => {
            for arg in &call.args {
                collect_capture_names_call_arg(arg, bound, names);
            }
        }
    }
}

fn lower_param_names(params: &[Param]) -> Result<Vec<String>, UnsupportedBytecode> {
    let mut names = Vec::new();
    for param in params {
        lower_param_pattern_names(param, &mut names);
    }
    Ok(names)
}

fn lower_param_types(params: &[Param], facts: &SemanticFacts) -> Vec<Option<Type>> {
    let mut types = Vec::new();
    for param in params {
        lower_param_pattern_types(param, facts, &mut types);
    }
    types
}

fn param_binding_type(param: &Param, facts: &SemanticFacts) -> Option<Type> {
    facts
        .binding_type(param.span)
        .cloned()
        .or_else(|| param.annotation.as_ref().map(bytecode_type_from_annotation))
}

fn lower_param_defaults(params: &[Param]) -> Vec<Option<Expr>> {
    let mut defaults = Vec::new();
    for param in params {
        lower_param_pattern_defaults(param, &mut defaults);
    }
    defaults
}

fn lower_param_pattern_names(param: &Param, names: &mut Vec<String>) {
    match &param.pattern {
        ParamPattern::Binding => names.push(param.name.clone()),
        ParamPattern::Anonymous => names.push(format!("__anon_param_{}", names.len())),
        ParamPattern::Tuple(items) => {
            for item in items {
                lower_param_pattern_names(item, names);
            }
        }
    }
}

fn lower_param_pattern_types(param: &Param, facts: &SemanticFacts, types: &mut Vec<Option<Type>>) {
    match &param.pattern {
        ParamPattern::Binding | ParamPattern::Anonymous => {
            types.push(param_binding_type(param, facts));
        }
        ParamPattern::Tuple(items) => {
            for item in items {
                lower_param_pattern_types(item, facts, types);
            }
        }
    }
}

fn lower_param_pattern_defaults(param: &Param, defaults: &mut Vec<Option<Expr>>) {
    match &param.pattern {
        ParamPattern::Binding | ParamPattern::Anonymous => defaults.push(param.default.clone()),
        ParamPattern::Tuple(items) => {
            for item in items {
                lower_param_pattern_defaults(item, defaults);
            }
        }
    }
}

fn define_flattened_param_bindings(
    state: &mut ChunkState,
    params: &[Param],
    facts: &SemanticFacts,
    start_register: usize,
) {
    let mut next_register = start_register;
    for param in params {
        define_flattened_param_binding(state, param, facts, &mut next_register);
    }
}

fn define_flattened_param_binding(
    state: &mut ChunkState,
    param: &Param,
    facts: &SemanticFacts,
    next_register: &mut usize,
) {
    match &param.pattern {
        ParamPattern::Binding | ParamPattern::Anonymous => {
            let register = RegisterIndex(*next_register);
            *next_register += 1;
            state.next_register = state.next_register.max(register.0 + 1);
            if matches!(param.pattern, ParamPattern::Anonymous) {
                return;
            }
            let value_type = facts.binding_type(param.span);
            state.define(
                param.name.clone(),
                Binding {
                    operand: ValueOperand::Register(register),
                    mutable: false,
                    ref_backed: false,
                    iterable_kind: value_type.and_then(iterable_kind_from_type),
                },
            );
            if value_type.is_some_and(bytecode_type_is_callable) {
                state.mark_callable(&param.name);
            }
        }
        ParamPattern::Tuple(items) => {
            for item in items {
                define_flattened_param_binding(state, item, facts, next_register);
            }
        }
    }
}

fn param_flat_len(param: &Param) -> usize {
    match &param.pattern {
        ParamPattern::Binding | ParamPattern::Anonymous => 1,
        ParamPattern::Tuple(items) => items.iter().map(param_flat_len).sum(),
    }
}

fn find_param_for_flat_index(params: &[Param], target: usize) -> Option<(&Param, usize, usize)> {
    let mut start = 0usize;
    for param in params {
        let len = param_flat_len(param);
        let end = start + len;
        if target >= start && target < end {
            return Some((param, start, end));
        }
        start = end;
    }
    None
}

fn single_tuple_binding_param_items(
    source_params: &[Param],
    flattened_param_count: usize,
) -> Option<&[TypeName]> {
    if flattened_param_count != 1 {
        return None;
    }
    let [param] = source_params else {
        return None;
    };
    if !matches!(
        param.pattern,
        ParamPattern::Binding | ParamPattern::Anonymous
    ) {
        return None;
    }
    let TypeName::Tuple(items) = &param.annotation.as_ref()?.name else {
        return None;
    };
    Some(items)
}

fn positional_call_arg_expr(arg: &CallArg) -> Option<&Expr> {
    match arg {
        CallArg::Positional(expr) => Some(expr),
        CallArg::Named { .. } => None,
    }
}

fn bytecode_type_from_annotation(annotation: &TypeAnnotation) -> Type {
    bytecode_type_from_type_name(&annotation.name)
}

fn bytecode_type_from_type_name(name: &TypeName) -> Type {
    match name {
        TypeName::Int => Type::Int,
        TypeName::IntRange { min, max } => Type::IntRange(IntRange::new(*min, *max)),
        TypeName::Float => Type::Float,
        TypeName::FloatRange(range) => Type::FloatRange(*range),
        TypeName::Rational => Type::Rational,
        TypeName::Number => Type::Number,
        TypeName::Bool => Type::Bool,
        TypeName::String => Type::String,
        TypeName::Message => Type::Message,
        TypeName::Char => Type::Char,
        TypeName::Char8 => Type::Char8,
        TypeName::Char32 => Type::Char32,
        TypeName::None => Type::None,
        TypeName::Any => Type::Any,
        TypeName::Comparable => Type::Comparable,
        TypeName::Type => Type::TypeValue,
        TypeName::TypeBounds { lower, upper } => Type::TypeValueBounds {
            lower: Box::new(bytecode_type_from_type_name(lower)),
            upper: Box::new(bytecode_type_from_type_name(upper)),
        },
        TypeName::Array(item) => Type::Array(Box::new(
            item.as_deref()
                .map(bytecode_type_from_type_name)
                .unwrap_or(Type::Unknown),
        )),
        TypeName::Option(item) => Type::Option(Box::new(bytecode_type_from_type_name(item))),
        TypeName::Map(key, value) => Type::Map(
            Box::new(bytecode_type_from_type_name(key)),
            Box::new(bytecode_type_from_type_name(value)),
        ),
        TypeName::WeakMap(key, value) => Type::WeakMap(
            Box::new(bytecode_type_from_type_name(key)),
            Box::new(bytecode_type_from_type_name(value)),
        ),
        TypeName::Tuple(items) => {
            Type::Tuple(items.iter().map(bytecode_type_from_type_name).collect())
        }
        TypeName::Named(name) => Type::Param(name.clone(), TypeParamConstraint::Type),
        TypeName::Applied { name, args } if name == "task" && args.len() == 1 => {
            Type::Task(Box::new(bytecode_type_from_type_name(&args[0])))
        }
        TypeName::Applied { name, args } if name == "event" => Type::Event(
            args.first()
                .map(|arg| Box::new(bytecode_type_from_type_name(arg))),
        ),
        TypeName::Applied { name, args } if name == "subscribable_event_intrnl" => {
            Type::SubscribableEventIntrnl(
                args.first()
                    .map(|arg| Box::new(bytecode_type_from_type_name(arg))),
            )
        }
        TypeName::Applied { name, args } if name == "subscribable_event" && args.len() == 1 => {
            Type::SubscribableEvent(Box::new(bytecode_type_from_type_name(&args[0])))
        }
        TypeName::Applied { name, args } if name == "sticky_event" => Type::StickyEvent(
            args.first()
                .map(|arg| Box::new(bytecode_type_from_type_name(arg))),
        ),
        TypeName::Applied { name, args } if name == "generator" => Type::Generator(
            args.first()
                .map(|arg| Box::new(bytecode_type_from_type_name(arg))),
        ),
        TypeName::Function => Type::Function {
            arity: None,
            arity_range: None,
            effects: Vec::new(),
            param_types: None,
            param_specs: None,
            return_type: Box::new(Type::Unknown),
        },
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => Type::Function {
            arity: Some(params.len()),
            arity_range: None,
            effects: effects.clone(),
            param_types: Some(params.iter().map(bytecode_type_from_type_name).collect()),
            param_specs: None,
            return_type: Box::new(bytecode_type_from_type_name(return_type)),
        },
        TypeName::Applied { name, .. } => Type::Param(name.clone(), TypeParamConstraint::Type),
    }
}

fn bytecode_argument_type_matches(expected: Option<&Type>, actual: Option<&Type>) -> bool {
    match (expected, actual) {
        (None, _) | (_, None) => true,
        (Some(expected), Some(actual)) => bytecode_type_matches(expected, actual),
    }
}

fn bytecode_optional_payload_matches(expected: Option<&Type>, actual: Option<&Type>) -> bool {
    match (expected, actual) {
        (Some(expected), Some(actual)) => bytecode_type_matches(expected, actual),
        (None, None) => true,
        _ => false,
    }
}

fn bytecode_argument_match_score(expected: &Type, actual: &Type) -> usize {
    if expected == actual {
        0
    } else if bytecode_type_matches(expected, actual) {
        1
    } else {
        usize::MAX / 2
    }
}

fn bytecode_type_matches(expected: &Type, actual: &Type) -> bool {
    use Type::*;

    match (expected, actual) {
        (Any | Unknown, _) | (_, Any | Unknown) | (Param(_, _), _) => true,
        (Int, Int | IntRange(_)) => true,
        (IntRange(_), Int | IntRange(_)) => true,
        (Float, Float | FloatRange(_)) => true,
        (FloatRange(expected), FloatRange(actual)) if expected.contains_range(*actual) => true,
        (Rational, Rational) => true,
        (Number, Int | IntRange(_) | Float | FloatRange(_) | Rational | Number) => true,
        (
            Comparable,
            Int | IntRange(_) | Float | FloatRange(_) | Rational | Number | Bool | String | Char
            | Char8 | Char32 | Enum(_),
        ) => true,
        (Message, Message | String) => true,
        (String, String) => true,
        (Bool, Bool) => true,
        (Char | Char8, Char | Char8) | (Char32, Char32) => true,
        (None, None) => true,
        (TypeValue, StructType(_) | ClassType(_) | InterfaceType(_) | ParametricType { .. }) => {
            true
        }
        (TypeValue, Subtype(_) | CastableSubtype(_) | ConcreteSubtype(_)) => true,
        (Range, Range) => true,
        (Enum(left), Enum(right))
        | (EnumType(left), EnumType(right))
        | (Struct(left), Struct(right))
        | (StructType(left), StructType(right))
        | (Class(left), Class(right))
        | (ClassType(left), ClassType(right))
        | (Interface(left), Interface(right))
        | (InterfaceType(left), InterfaceType(right))
        | (Module(left), Module(right)) => left == right,
        (Array(expected), Array(actual))
        | (Option(expected), Option(actual))
        | (Task(expected), Task(actual))
        | (Subtype(expected), Subtype(actual))
        | (CastableSubtype(expected), CastableSubtype(actual))
        | (ConcreteSubtype(expected), ConcreteSubtype(actual))
        | (ClassifiableSubset(expected), ClassifiableSubset(actual))
        | (ClassifiableSubsetKey(expected), ClassifiableSubsetKey(actual))
        | (ClassifiableSubsetVar(expected), ClassifiableSubsetVar(actual))
        | (SuccessResult(expected), SuccessResult(actual))
        | (ErrorResult(expected), ErrorResult(actual))
        | (Modifier(expected), Modifier(actual))
        | (ModifierStack(expected), ModifierStack(actual))
        | (Signalable(expected), Signalable(actual)) => bytecode_type_matches(expected, actual),
        (Map(expected_key, expected_value), Map(actual_key, actual_value))
        | (WeakMap(expected_key, expected_value), WeakMap(actual_key, actual_value))
        | (Result(expected_key, expected_value), Result(actual_key, actual_value)) => {
            bytecode_type_matches(expected_key, actual_key)
                && bytecode_type_matches(expected_value, actual_value)
        }
        (Result(expected_success, _), SuccessResult(actual_success)) => {
            bytecode_type_matches(expected_success, actual_success)
        }
        (Result(_, expected_error), ErrorResult(actual_error)) => {
            bytecode_type_matches(expected_error, actual_error)
        }
        (Tuple(expected), Tuple(actual)) => {
            expected.len() == actual.len()
                && expected
                    .iter()
                    .zip(actual)
                    .all(|(expected, actual)| bytecode_type_matches(expected, actual))
        }
        (Event(expected), Event(actual))
        | (SubscribableEventIntrnl(expected), SubscribableEventIntrnl(actual))
        | (StickyEvent(expected), StickyEvent(actual))
        | (Generator(expected), Generator(actual))
        | (Awaitable(expected), Awaitable(actual))
        | (Subscribable(expected), Subscribable(actual))
        | (Listenable(expected), Listenable(actual)) => match (expected, actual) {
            (Some(expected), Some(actual)) => bytecode_type_matches(expected, actual),
            _ => true,
        },
        (SubscribableEvent(expected), SubscribableEvent(actual)) => {
            bytecode_type_matches(expected, actual)
        }
        (Awaitable(expected), SubscribableEvent(actual))
        | (SubscribableEventIntrnl(expected), SubscribableEvent(actual))
        | (Event(expected), SubscribableEvent(actual))
        | (Listenable(expected), SubscribableEvent(actual))
        | (Subscribable(expected), SubscribableEvent(actual)) => {
            bytecode_optional_payload_matches(expected.as_deref(), Some(actual.as_ref()))
        }
        (Awaitable(expected), SubscribableEventIntrnl(actual))
        | (Awaitable(expected), StickyEvent(actual))
        | (Event(expected), SubscribableEventIntrnl(actual))
        | (Event(expected), StickyEvent(actual))
        | (Listenable(expected), SubscribableEventIntrnl(actual))
        | (Subscribable(expected), SubscribableEventIntrnl(actual)) => {
            bytecode_optional_payload_matches(expected.as_deref(), actual.as_deref())
        }
        (Signalable(expected), SubscribableEvent(actual)) => {
            bytecode_type_matches(expected, actual)
        }
        (Signalable(expected), SubscribableEventIntrnl(actual)) => actual
            .as_deref()
            .is_some_and(|actual| bytecode_type_matches(expected, actual)),
        (Signalable(expected), StickyEvent(actual)) => actual
            .as_deref()
            .is_some_and(|actual| bytecode_type_matches(expected, actual)),
        (
            Function {
                param_types: expected_params,
                return_type: expected_return,
                ..
            },
            Function {
                param_types: actual_params,
                return_type: actual_return,
                ..
            },
        ) => {
            let params_match = match (expected_params, actual_params) {
                (Some(expected), Some(actual)) => {
                    expected.len() == actual.len()
                        && expected
                            .iter()
                            .zip(actual)
                            .all(|(expected, actual)| bytecode_type_matches(expected, actual))
                }
                _ => true,
            };
            params_match && bytecode_type_matches(expected_return, actual_return)
        }
        _ => expected == actual,
    }
}

fn bytecode_type_is_callable(value_type: &Type) -> bool {
    matches!(value_type, Type::Function { .. } | Type::Overload(_))
}

fn class_method_register_params(
    fields: &[StructField],
    params: &[Param],
) -> Result<Vec<String>, UnsupportedBytecode> {
    let mut names = Vec::with_capacity(1 + fields.len() + params.len());
    names.push("Self".to_string());
    names.extend(fields.iter().map(|field| field.name.clone()));
    names.extend(lower_param_names(params)?);
    Ok(names)
}

fn class_extension_register_params(
    fields: &[StructField],
    receiver: &Param,
    params: &[Param],
) -> Result<Vec<String>, UnsupportedBytecode> {
    let mut names = class_method_register_params(fields, &[])?;
    names.push(receiver.name.clone());
    names.extend(lower_param_names(params)?);
    Ok(names)
}

fn lower_failable_index_parts(expr: &Expr) -> Result<(&Expr, &Expr, Span), UnsupportedBytecode> {
    match &expr.kind {
        ExprKind::Index { collection, index } => Ok((collection, index, expr.span)),
        ExprKind::BracketCall { callee, args } => {
            let [index] = args.as_slice() else {
                return Err(UnsupportedBytecode);
            };
            Ok((callee, index, expr.span))
        }
        _ => Err(UnsupportedBytecode),
    }
}

fn callable_lookup_name(callee: &Expr) -> Option<String> {
    match &callee.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::QualifiedName { qualifier, name } => Some(format!("{qualifier}.{name}")),
        ExprKind::Member { object, name } => {
            compile_time_member_path(object).map(|namespace| format!("{namespace}.{name}"))
        }
        ExprKind::QualifiedMember {
            object,
            qualifier,
            name,
        } => compile_time_member_path(object)
            .map(|namespace| format!("{namespace}.{qualifier}.{name}")),
        _ => None,
    }
}

fn archetype_callee_name(callee: &Expr) -> Option<String> {
    match &callee.kind {
        ExprKind::Call { callee, .. } => archetype_callee_name(callee),
        _ => callable_lookup_name(callee),
    }
}

fn case_arms_have_wildcard(arms: &[CaseArm]) -> bool {
    arms.iter()
        .any(|arm| matches!(arm.pattern, CasePattern::Wildcard { .. }))
}

fn compile_time_member_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::QualifiedName { qualifier, name } => Some(format!("{qualifier}.{name}")),
        ExprKind::Member { object, name } => {
            compile_time_member_path(object).map(|namespace| format!("{namespace}.{name}"))
        }
        ExprKind::QualifiedMember {
            object,
            qualifier,
            name,
        } => compile_time_member_path(object)
            .map(|namespace| format!("{namespace}.{qualifier}.{name}")),
        _ => None,
    }
}

fn is_self_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Ident(name) if name == "Self")
}

fn type_annotation_class_name(annotation: &TypeAnnotation) -> Option<String> {
    match &annotation.name {
        TypeName::Named(name) => Some(name.clone()),
        TypeName::Applied { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn type_name_aggregate_head(type_name: &TypeName) -> Option<String> {
    match type_name {
        TypeName::Named(name) | TypeName::Applied { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn bytecode_type_constraint_payload_head(type_name: &TypeName) -> Option<String> {
    match type_name {
        TypeName::Applied { name, args }
            if matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) && args.len() == 1 =>
        {
            bytecode_type_constraint_payload_head(&args[0])
        }
        TypeName::TypeBounds { upper, .. } => bytecode_type_constraint_payload_head(upper),
        TypeName::Named(name) | TypeName::Applied { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn static_type_function_return_type(return_type: Option<&TypeAnnotation>) -> bool {
    return_type.is_some_and(|annotation| {
        matches!(
            annotation.name,
            TypeName::Type | TypeName::TypeBounds { .. } | TypeName::Applied { .. }
        )
    })
}

fn bytecode_type_function_param(param: &Param) -> Option<BytecodeTypeFunctionParam> {
    let annotation = param.annotation.as_ref()?;
    static_type_function_param_type_name(&annotation.name).then(|| BytecodeTypeFunctionParam {
        name: param.name.clone(),
        constraint: annotation.name.clone(),
    })
}

fn static_type_function_param_type_name(type_name: &TypeName) -> bool {
    match type_name {
        TypeName::Type | TypeName::TypeBounds { .. } => true,
        TypeName::Applied { name, .. } => matches!(
            name.as_str(),
            "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
        ),
        _ => false,
    }
}

fn bytecode_expr_to_type_name(expr: &Expr) -> Option<TypeName> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(TypeName::parse(name.clone())),
        ExprKind::QualifiedName { qualifier, name } => {
            Some(TypeName::Named(format!("{qualifier}.{name}")))
        }
        ExprKind::Member { object, name } => compile_time_member_path(object)
            .map(|namespace| TypeName::Named(format!("{namespace}.{name}"))),
        ExprKind::QualifiedMember {
            object,
            qualifier,
            name,
        } => compile_time_member_path(object)
            .map(|namespace| TypeName::Named(format!("{namespace}.{qualifier}.{name}"))),
        ExprKind::Call { callee, args } => {
            let name = callable_lookup_name(callee)?;
            let args = args
                .iter()
                .map(|arg| {
                    let CallArg::Positional(expr) = arg else {
                        return None;
                    };
                    bytecode_expr_to_type_name(expr)
                })
                .collect::<Option<Vec<_>>>()?;
            Some(TypeName::Applied { name, args })
        }
        ExprKind::TypeAnnotationLiteral { annotation } => Some(annotation.name.clone()),
        ExprKind::Tuple(items) => Some(TypeName::Tuple(
            items
                .iter()
                .map(bytecode_expr_to_type_name)
                .collect::<Option<Vec<_>>>()?,
        )),
        ExprKind::Option(Some(item)) => Some(TypeName::Option(Box::new(
            bytecode_expr_to_type_name(item)?,
        ))),
        ExprKind::Array(items) => {
            let mut item_types = items.iter().map(bytecode_expr_to_type_name);
            let first = item_types.next().flatten();
            Some(TypeName::Array(first.map(Box::new)))
        }
        _ => None,
    }
}

fn substitute_bytecode_type_name_params(
    type_name: &TypeName,
    substitutions: &HashMap<String, TypeName>,
) -> Option<TypeName> {
    match type_name {
        TypeName::TypeBounds { lower, upper } => Some(TypeName::TypeBounds {
            lower: Box::new(substitute_bytecode_type_name_params(lower, substitutions)?),
            upper: Box::new(substitute_bytecode_type_name_params(upper, substitutions)?),
        }),
        TypeName::Array(item) => Some(TypeName::Array(match item.as_ref() {
            Some(item) => Some(Box::new(substitute_bytecode_type_name_params(
                item,
                substitutions,
            )?)),
            None => None,
        })),
        TypeName::Map(key, value) => Some(TypeName::Map(
            Box::new(substitute_bytecode_type_name_params(key, substitutions)?),
            Box::new(substitute_bytecode_type_name_params(value, substitutions)?),
        )),
        TypeName::WeakMap(key, value) => Some(TypeName::WeakMap(
            Box::new(substitute_bytecode_type_name_params(key, substitutions)?),
            Box::new(substitute_bytecode_type_name_params(value, substitutions)?),
        )),
        TypeName::Tuple(items) => Some(TypeName::Tuple(
            items
                .iter()
                .map(|item| substitute_bytecode_type_name_params(item, substitutions))
                .collect::<Option<Vec<_>>>()?,
        )),
        TypeName::Option(item) => Some(TypeName::Option(Box::new(
            substitute_bytecode_type_name_params(item, substitutions)?,
        ))),
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => Some(TypeName::FunctionSignature {
            params: params
                .iter()
                .map(|param| substitute_bytecode_type_name_params(param, substitutions))
                .collect::<Option<Vec<_>>>()?,
            effects: effects.clone(),
            return_type: Box::new(substitute_bytecode_type_name_params(
                return_type,
                substitutions,
            )?),
        }),
        TypeName::Applied { name, args } => Some(TypeName::Applied {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_bytecode_type_name_params(arg, substitutions))
                .collect::<Option<Vec<_>>>()?,
        }),
        TypeName::Named(name) => substitutions
            .get(name)
            .cloned()
            .or_else(|| Some(TypeName::Named(name.clone()))),
        other => Some(other.clone()),
    }
}

fn function_return_class_name(return_type: Option<&TypeAnnotation>, body: &Expr) -> Option<String> {
    return_type
        .and_then(type_annotation_class_name)
        .or_else(|| match &body.kind {
            ExprKind::Archetype { callee, .. } => archetype_callee_name(callee),
            _ => None,
        })
}

fn runtime_names_match(left: &str, right: &str) -> bool {
    left == right
        || left.rsplit('.').next().is_some_and(|name| name == right)
        || right.rsplit('.').next().is_some_and(|name| name == left)
}

fn aggregate_runtime_type_name(value_type: &Type) -> Option<&str> {
    match value_type {
        Type::Class(name) | Type::Interface(name) | Type::Struct(name) => Some(name),
        _ => None,
    }
}

fn official_subtype_type_name_payload(name: &TypeName) -> Option<&TypeName> {
    match name {
        TypeName::Applied { name, args }
            if matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) && args.len() == 1 =>
        {
            args.first()
        }
        _ => None,
    }
}

fn render_aggregate_runtime_type_name_from_type_name(name: &TypeName) -> Option<String> {
    match name {
        TypeName::Named(name) => Some(name.clone()),
        TypeName::Applied { name, args } => {
            let args = args
                .iter()
                .map(render_runtime_type_name_from_type_name)
                .collect::<Option<Vec<_>>>()?;
            Some(format!("{name}({})", args.join(", ")))
        }
        _ => None,
    }
}

fn render_runtime_type_name_from_type_name(name: &TypeName) -> Option<String> {
    Some(match name {
        TypeName::Int => "int".to_string(),
        TypeName::Float => "float".to_string(),
        TypeName::Rational => "rational".to_string(),
        TypeName::Number => "number".to_string(),
        TypeName::Bool => "bool".to_string(),
        TypeName::String => "string".to_string(),
        TypeName::Message => "message".to_string(),
        TypeName::Char => "char".to_string(),
        TypeName::Char8 => "char8".to_string(),
        TypeName::Char32 => "char32".to_string(),
        TypeName::None => "none".to_string(),
        TypeName::Any => "any".to_string(),
        TypeName::Comparable => "comparable".to_string(),
        TypeName::Type => "type".to_string(),
        TypeName::TypeBounds { lower, upper } => format!(
            "type({}, {})",
            render_runtime_type_name_from_type_name(lower)?,
            render_runtime_type_name_from_type_name(upper)?
        ),
        TypeName::IntRange { min, max } => format!("type{{_X:int where {min} <= _X, _X <= {max}}}"),
        TypeName::FloatRange(range) => format!(
            "type{{_X:float where {} <= _X, _X <= {}}}",
            range.min.render(),
            range.max.render()
        ),
        TypeName::Array(Some(item)) => {
            format!("array<{}>", render_runtime_type_name_from_type_name(item)?)
        }
        TypeName::Array(None) => "array<unknown>".to_string(),
        TypeName::Map(key, value) => format!(
            "map<{}, {}>",
            render_runtime_type_name_from_type_name(key)?,
            render_runtime_type_name_from_type_name(value)?
        ),
        TypeName::WeakMap(key, value) => format!(
            "weak_map<{}, {}>",
            render_runtime_type_name_from_type_name(key)?,
            render_runtime_type_name_from_type_name(value)?
        ),
        TypeName::Tuple(items) => {
            let items = items
                .iter()
                .map(render_runtime_type_name_from_type_name)
                .collect::<Option<Vec<_>>>()?;
            format!("tuple({})", items.join(", "))
        }
        TypeName::Option(item) => format!("?{}", render_runtime_type_name_from_type_name(item)?),
        TypeName::Function => "function".to_string(),
        TypeName::FunctionSignature { .. } => return None,
        TypeName::Applied { name, args } => {
            if matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) && args.len() == 1
            {
                render_runtime_type_name_from_type_name(&args[0])?
            } else {
                let args = args
                    .iter()
                    .map(render_runtime_type_name_from_type_name)
                    .collect::<Option<Vec<_>>>()?;
                format!("{name}({})", args.join(", "))
            }
        }
        TypeName::Named(name) => name.clone(),
    })
}

fn parse_parametric_instance_name(name: &str) -> Option<(String, Vec<String>)> {
    let (head, rest) = name.split_once('(')?;
    let args = rest.strip_suffix(')')?;
    Some((
        head.to_string(),
        split_parametric_instance_args(args)
            .into_iter()
            .map(str::to_string)
            .collect(),
    ))
}

fn split_parametric_instance_args(args: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(args[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    items
}

fn is_builtin_type_atom(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "nat"
            | "nat8"
            | "nat16"
            | "nat32"
            | "nat64"
            | "float"
            | "rational"
            | "number"
            | "logic"
            | "void"
            | "false"
            | "string"
            | "message"
            | "char"
            | "char8"
            | "char32"
            | "any"
            | "comparable"
            | "type"
            | "function"
    )
}

fn erase_parametric_instance_name(name: &str) -> &str {
    name.split_once('(')
        .map(|(generic, _)| generic)
        .unwrap_or(name)
}

fn runtime_param_type(
    erased_type_params: &[String],
    value_type: Option<&Type>,
    annotation: Option<&TypeAnnotation>,
) -> Option<TypeName> {
    if let Some(value_type) = value_type {
        return runtime_param_type_from_type(value_type, erased_type_params);
    }
    let annotation = annotation?;
    runtime_param_type_from_annotation(annotation, erased_type_params)
}

fn runtime_param_type_from_type(
    value_type: &Type,
    erased_type_params: &[String],
) -> Option<TypeName> {
    let type_name = type_to_runtime_type_name(value_type)?;
    (!runtime_type_name_erases_to_param(&type_name, erased_type_params)).then_some(type_name)
}

fn runtime_param_type_from_annotation(
    annotation: &TypeAnnotation,
    erased_type_params: &[String],
) -> Option<TypeName> {
    (!runtime_type_name_erases_to_param(&annotation.name, erased_type_params))
        .then(|| annotation.name.clone())
}

fn runtime_type_name_erases_to_param(type_name: &TypeName, erased_type_params: &[String]) -> bool {
    matches!(
        type_name,
        TypeName::Named(name) | TypeName::Applied { name, .. }
            if erased_type_params.iter().any(|param| param == name)
    )
}

fn lower_runtime_param_types(
    params: &[Param],
    erased_type_params: &[String],
    facts: &SemanticFacts,
) -> Vec<Option<TypeName>> {
    let mut types = Vec::new();
    for param in params {
        lower_runtime_param_pattern_types(param, erased_type_params, facts, &mut types);
    }
    types
}

fn lower_runtime_param_pattern_types(
    param: &Param,
    erased_type_params: &[String],
    facts: &SemanticFacts,
    types: &mut Vec<Option<TypeName>>,
) {
    match &param.pattern {
        ParamPattern::Binding | ParamPattern::Anonymous => {
            types.push(runtime_param_type(
                erased_type_params,
                facts.binding_type(param.span),
                param.annotation.as_ref(),
            ));
        }
        ParamPattern::Tuple(items) => {
            for item in items {
                lower_runtime_param_pattern_types(item, erased_type_params, facts, types);
            }
        }
    }
}

fn type_to_runtime_type_name(value_type: &Type) -> Option<TypeName> {
    Some(match value_type {
        Type::Int => TypeName::Int,
        Type::IntRange(range) => TypeName::IntRange {
            min: range.min,
            max: range.max,
        },
        Type::Float => TypeName::Float,
        Type::FloatRange(range) => TypeName::FloatRange(*range),
        Type::Rational => TypeName::Rational,
        Type::Number => TypeName::Number,
        Type::Bool => TypeName::Bool,
        Type::String => TypeName::String,
        Type::Message => TypeName::Message,
        Type::Char => TypeName::Char,
        Type::Char8 => TypeName::Char8,
        Type::Char32 => TypeName::Char32,
        Type::None => TypeName::None,
        Type::Any | Type::Unknown => TypeName::Any,
        Type::Comparable => TypeName::Comparable,
        Type::TypeValue => TypeName::Type,
        Type::TypeValueOf(_) => TypeName::Type,
        Type::TypeValueBounds { lower, upper } => TypeName::TypeBounds {
            lower: Box::new(type_to_runtime_type_name(lower)?),
            upper: Box::new(type_to_runtime_type_name(upper)?),
        },
        Type::Enum(name)
        | Type::Struct(name)
        | Type::Class(name)
        | Type::Interface(name)
        | Type::Module(name) => TypeName::Named(name.clone()),
        Type::Param(name, _) => TypeName::Named(name.clone()),
        Type::Array(item) => TypeName::Array(Some(Box::new(type_to_runtime_type_name(item)?))),
        Type::Map(key, value) => TypeName::Map(
            Box::new(type_to_runtime_type_name(key)?),
            Box::new(type_to_runtime_type_name(value)?),
        ),
        Type::WeakMap(key, value) => TypeName::WeakMap(
            Box::new(type_to_runtime_type_name(key)?),
            Box::new(type_to_runtime_type_name(value)?),
        ),
        Type::Tuple(items) => TypeName::Tuple(
            items
                .iter()
                .map(type_to_runtime_type_name)
                .collect::<Option<Vec<_>>>()?,
        ),
        Type::Option(item) => TypeName::Option(Box::new(type_to_runtime_type_name(item)?)),
        Type::Function {
            param_types,
            effects,
            return_type,
            ..
        } => {
            if let Some(param_types) = param_types {
                TypeName::FunctionSignature {
                    params: param_types
                        .iter()
                        .map(type_to_runtime_type_name)
                        .collect::<Option<Vec<_>>>()?,
                    effects: effects.clone(),
                    return_type: Box::new(type_to_runtime_type_name(return_type)?),
                }
            } else {
                TypeName::Function
            }
        }
        Type::Event(payload) => applied_runtime_type("event", payload.as_deref())?,
        Type::SubscribableEventIntrnl(payload) => {
            applied_runtime_type("subscribable_event_intrnl", payload.as_deref())?
        }
        Type::SubscribableEvent(payload) => TypeName::Applied {
            name: "subscribable_event".to_string(),
            args: vec![type_to_runtime_type_name(payload)?],
        },
        Type::StickyEvent(payload) => applied_runtime_type("sticky_event", payload.as_deref())?,
        Type::Task(payload) => TypeName::Applied {
            name: "task".to_string(),
            args: vec![type_to_runtime_type_name(payload)?],
        },
        Type::Generator(item) => applied_runtime_type("generator", item.as_deref())?,
        Type::Subtype(item) => TypeName::Applied {
            name: "subtype".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::CastableSubtype(item) => TypeName::Applied {
            name: "castable_subtype".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ConcreteSubtype(item) => TypeName::Applied {
            name: "concrete_subtype".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ClassifiableSubset(item) => TypeName::Applied {
            name: "classifiable_subset".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ClassifiableSubsetKey(item) => TypeName::Applied {
            name: "classifiable_subset_key".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ClassifiableSubsetVar(item) => TypeName::Applied {
            name: "classifiable_subset_var".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::Modifier(item) => TypeName::Applied {
            name: "modifier".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ModifierStack(item) => TypeName::Applied {
            name: "modifier_stack".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::Awaitable(payload) => applied_runtime_type("awaitable", payload.as_deref())?,
        Type::Signalable(payload) => TypeName::Applied {
            name: "signalable".to_string(),
            args: vec![type_to_runtime_type_name(payload)?],
        },
        Type::Subscribable(payload) => applied_runtime_type("subscribable", payload.as_deref())?,
        Type::Listenable(payload) => applied_runtime_type("listenable", payload.as_deref())?,
        Type::Result(success, error) => TypeName::Applied {
            name: "result".to_string(),
            args: vec![
                type_to_runtime_type_name(success)?,
                type_to_runtime_type_name(error)?,
            ],
        },
        Type::SuccessResult(item) => TypeName::Applied {
            name: "success_result".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::ErrorResult(item) => TypeName::Applied {
            name: "error_result".to_string(),
            args: vec![type_to_runtime_type_name(item)?],
        },
        Type::Never
        | Type::Range
        | Type::EnumType(_)
        | Type::StructType(_)
        | Type::ClassType(_)
        | Type::InterfaceType(_)
        | Type::ParametricType { .. }
        | Type::Overload(_) => return None,
    })
}

fn type_is_type_value_for_extension_accessor(value_type: &Type) -> bool {
    matches!(
        value_type,
        Type::TypeValueOf(_)
            | Type::StructType(_)
            | Type::ClassType(_)
            | Type::InterfaceType(_)
            | Type::ParametricType { .. }
            | Type::Subtype(_)
            | Type::CastableSubtype(_)
            | Type::ConcreteSubtype(_)
    )
}

fn applied_runtime_type(name: &str, payload: Option<&Type>) -> Option<TypeName> {
    Some(TypeName::Applied {
        name: name.to_string(),
        args: match payload {
            Some(payload) => vec![type_to_runtime_type_name(payload)?],
            None => Vec::new(),
        },
    })
}

fn merge_struct_fields(target: &mut Vec<StructField>, fields: Vec<StructField>) {
    for field in fields {
        if let Some(existing) = target
            .iter_mut()
            .find(|candidate| candidate.name == field.name)
        {
            *existing = field;
        } else {
            target.push(field);
        }
    }
}

fn is_compile_time_type_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Ident(name) if is_builtin_type_identifier(name))
}

fn should_predeclare_runtime_global_let(annotation: Option<&TypeAnnotation>, expr: &Expr) -> bool {
    !matches!(
        expr.kind,
        ExprKind::ModuleDefinition { .. }
            | ExprKind::InterfaceDefinition { .. }
            | ExprKind::ClassDefinition { .. }
            | ExprKind::StructDefinition { .. }
            | ExprKind::EnumDefinition { .. }
    ) && (!is_compile_time_type_expr(expr) || is_type_value_annotation(annotation))
}

fn is_type_value_annotation(annotation: Option<&TypeAnnotation>) -> bool {
    matches!(
        annotation.map(|annotation| &annotation.name),
        Some(TypeName::Type)
    )
}

fn call_uses_await_sequence(callee: &Expr) -> bool {
    match &callee.kind {
        ExprKind::Member { name, .. } if name == "Await" => true,
        ExprKind::QualifiedMember { name, .. } if name == "Await" => true,
        ExprKind::Ident(name) if name == "Sleep" => true,
        _ => false,
    }
}

fn concurrent_native_suffix(op: ConcurrentOp) -> &'static str {
    match op {
        ConcurrentOp::Sync => "sync",
        ConcurrentOp::Race => "race",
        ConcurrentOp::Rush => "rush",
        ConcurrentOp::Branch => "branch",
    }
}

fn is_builtin_type_identifier(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "float"
            | "rational"
            | "number"
            | "logic"
            | "string"
            | "message"
            | "char"
            | "char8"
            | "char32"
            | "void"
            | "any"
            | "comparable"
    )
}

fn bytecode_native_function_name(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "Print"
            | "assert_eq"
            | "str"
            | "Err"
            | "ToDiagnostic"
            | "GetSecondsSinceEpoch"
            | "GetSession"
            | "GetSimulationElapsedTime"
            | "FitsInPlayerMap"
            | "Mod"
            | "Quotient"
            | "BitAnd"
            | "BitOr"
            | "BitXor"
            | "BitNot"
            | "Clamp"
            | "Lerp"
            | "Abs"
            | "Min"
            | "Max"
            | "Ceil"
            | "Floor"
            | "Int"
            | "Sqrt"
            | "Sin"
            | "Cos"
            | "Tan"
            | "ArcSin"
            | "ArcCos"
            | "ArcTan"
            | "Sinh"
            | "Cosh"
            | "Tanh"
            | "ArSinh"
            | "ArCosh"
            | "ArTanh"
            | "Pow"
            | "Exp"
            | "Ln"
            | "Log"
            | "Sgn"
            | "IsAlmostEqual"
            | "MakeColorFromSRGB"
            | "MakeColorFromSRGBValues"
            | "MakeSRGBFromColor"
            | "MakeColorFromHex"
            | "MakeColorFromHSV"
            | "MakeHSVFromColor"
            | "MakeColorAlpha"
            | "Over"
            | "ToString"
            | "Localize"
            | "Join"
            | "GetRandomFloat"
            | "GetRandomInt"
            | "Shuffle"
            | "Round"
            | "Concatenate"
            | "ConcatenateMaps"
            | "MakeClassifiableSubset"
            | "MakeClassifiableSubsetVar"
            | "GetCastableFinalSuperClass"
            | "GetCastableFinalSuperClassFromType"
            | "MakeSuccess"
            | "MakeError"
            | "Sleep"
            | "__verse_sync"
            | "__verse_race"
            | "__verse_rush"
            | "__verse_branch"
            | "__verse_begin_defer_scope"
            | "__verse_end_defer_scope"
            | "__verse_defer"
    )
}

fn bytecode_native_param_aliases(name: &str) -> Option<Vec<Vec<&'static str>>> {
    let aliases = match name {
        "Print" => vec![vec!["Message"], vec!["Duration"], vec!["Color"]],
        "ToDiagnostic" => vec![vec!["Value"]],
        "FitsInPlayerMap" => vec![vec!["Value"]],
        "MakeColorFromSRGB" | "MakeColorFromSRGBValues" => {
            vec![vec!["Red"], vec!["Green"], vec!["Blue"]]
        }
        "MakeSRGBFromColor" | "MakeHSVFromColor" => vec![vec!["Color"]],
        "MakeColorFromHex" => vec![vec!["hexString"]],
        "MakeColorFromHSV" => vec![vec!["Hue"], vec!["Saturation"], vec!["Value"]],
        "MakeColorAlpha" => vec![vec!["R"], vec!["G"], vec!["B"], vec!["A"]],
        "Over" => vec![vec!["CA1"], vec!["CA2"]],
        "ToString" => vec![vec!["Val", "String", "Character"]],
        "Localize" => vec![vec!["Message"]],
        "Join" => vec![vec!["Strings", "Messages"], vec!["Separator"]],
        "GetRandomFloat" | "GetRandomInt" => vec![vec!["Low"], vec!["High"]],
        "Shuffle" => vec![vec!["Input"]],
        "Round" => vec![vec!["Val"]],
        "Clamp" => vec![vec!["Value"], vec!["A"], vec!["B"]],
        "Lerp" => vec![vec!["From"], vec!["To"], vec!["Parameter"]],
        "Abs" | "Ceil" | "Floor" => vec![vec!["Value", "Val"]],
        "Min" | "Max" => vec![vec!["X"], vec!["Y"]],
        "BitAnd" | "BitOr" | "BitXor" => vec![vec!["X"], vec!["Y"]],
        "BitNot" => vec![vec!["X"]],
        "Int" | "Sgn" => vec![vec!["Val"]],
        "Sqrt" | "Sin" | "Cos" | "Tan" | "ArcSin" | "ArcCos" | "Sinh" | "Cosh" | "Tanh"
        | "ArSinh" | "ArCosh" | "ArTanh" | "Exp" | "Ln" => vec![vec!["X"]],
        "Pow" => vec![vec!["A"], vec!["B"]],
        "Log" => vec![vec!["B"], vec!["X"]],
        "IsAlmostEqual" => vec![vec!["Val1"], vec!["Val2"], vec!["AbsoluteTolerance"]],
        "Concatenate" => vec![vec!["Arrays"]],
        "GetCastableFinalSuperClass" => vec![vec!["base_type"], vec!["Instance"]],
        "GetCastableFinalSuperClassFromType" => vec![vec!["base_type"], vec!["sub_type"]],
        "Sleep" => vec![vec!["Seconds"]],
        _ => return None,
    };
    Some(aliases)
}

fn span_position(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn is_comparison_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

fn lower_constant_expr(expr: &Expr) -> Result<Option<Constant>, UnsupportedBytecode> {
    match &expr.kind {
        ExprKind::Number { value, kind } => lower_number_literal(*value, *kind).map(Some),
        ExprKind::Char { value, kind } => Ok(Some(Constant::Char {
            value: *value,
            kind: *kind,
        })),
        ExprKind::Bool(value) => Ok(Some(Constant::Bool(*value))),
        ExprKind::String(value) => Ok(Some(Constant::String(value.clone()))),
        ExprKind::None => Ok(Some(Constant::None)),
        ExprKind::Option(None) => Ok(Some(Constant::Option(None))),
        ExprKind::Tuple(items) => {
            let mut constants = Vec::with_capacity(items.len());
            for item in items {
                let Some(constant) = lower_constant_expr(item)? else {
                    return Ok(None);
                };
                constants.push(constant);
            }
            Ok(Some(Constant::Tuple(constants)))
        }
        ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } => {
            let Some(Constant::Int(start)) = lower_constant_expr(left)? else {
                return Ok(None);
            };
            let Some(Constant::Int(end)) = lower_constant_expr(right)? else {
                return Ok(None);
            };
            Ok(Some(Constant::Range { start, end }))
        }
        _ => Ok(None),
    }
}

fn lower_number_literal(
    value: NumberLiteral,
    kind: NumberKind,
) -> Result<Constant, UnsupportedBytecode> {
    match (value, kind) {
        (NumberLiteral::Int(value), NumberKind::Int) => i64::try_from(value)
            .map(Constant::Int)
            .map_err(|_| UnsupportedBytecode),
        (NumberLiteral::Float(value), NumberKind::Float) => Ok(Constant::Float(value)),
        (NumberLiteral::Int(value), NumberKind::Float) => Ok(Constant::Float(value as f64)),
        (NumberLiteral::Float(value), NumberKind::Int) => Ok(Constant::Float(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[test]
    fn official_utility_instruction_opcodes_are_preserved() {
        let source = ValueOperand::Uninitialized;
        let dest = RegisterIndex(2);
        let instructions = [
            Instruction::MoveTrailed {
                dest,
                source,
                span: span(),
            },
            Instruction::MoveNonComparable {
                dest,
                source,
                span: span(),
            },
            Instruction::ResetNonTrailed { dest, span: span() },
            Instruction::JumpIfInitialized {
                source,
                jump_offset: 1,
                span: span(),
            },
            Instruction::Switch {
                which: source,
                jump_offsets: vec![0, 1],
                span: span(),
            },
            Instruction::Err { span: span() },
            Instruction::Tracepoint {
                name: "marker".to_string(),
                span: span(),
            },
            Instruction::CanFastAppendToArrayFastFail {
                leniency_indicator: dest,
                ref_value: source,
                maybe_mutable_array: source,
                on_failure: 0,
                span: span(),
            },
            Instruction::FastAppendToArray {
                left_source: source,
                right_source: source,
                span: span(),
            },
            Instruction::MutableAdd {
                dest,
                left_source: source,
                right_source: source,
                span: span(),
            },
            Instruction::ReturnTrailed {
                value: source,
                span: span(),
            },
            Instruction::RefSetLive {
                ref_value: source,
                value: source,
                task: source,
                span: span(),
            },
            Instruction::Freeze {
                dest,
                value: source,
                span: span(),
            },
            Instruction::FreezeIfAccessor {
                dest,
                value: source,
                span: span(),
            },
            Instruction::Melt {
                dest,
                value: source,
                span: span(),
            },
            Instruction::LengthWithEffects {
                dest,
                container: source,
                span: span(),
            },
            Instruction::CallSetLive {
                container: source,
                index: source,
                value_to_set: source,
                task: source,
                span: span(),
            },
            Instruction::NewObject {
                dest,
                class_name: "counter".to_string(),
                object_kind: ObjectKind::Class,
                fields: vec![("Value".to_string(), false, source)],
                span: span(),
            },
            Instruction::LoadField {
                dest,
                object: source,
                name: "Value".to_string(),
                span: span(),
            },
        ];
        let opcodes = instructions
            .iter()
            .map(Instruction::opcode)
            .collect::<Vec<_>>();
        assert_eq!(
            opcodes,
            vec![
                Opcode::MoveTrailed,
                Opcode::MoveNonComparable,
                Opcode::ResetNonTrailed,
                Opcode::JumpIfInitialized,
                Opcode::Switch,
                Opcode::Err,
                Opcode::Tracepoint,
                Opcode::CanFastAppendToArrayFastFail,
                Opcode::FastAppendToArray,
                Opcode::MutableAdd,
                Opcode::ReturnTrailed,
                Opcode::RefSetLive,
                Opcode::Freeze,
                Opcode::FreezeIfAccessor,
                Opcode::Melt,
                Opcode::LengthWithEffects,
                Opcode::CallSetLive,
                Opcode::NewObject,
                Opcode::LoadField,
            ]
        );
    }
}
