use crate::token::{CharacterKind, NumberKind, NumberLiteral, Span};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Using {
        path: String,
    },
    Let {
        name: String,
        specifiers: Vec<String>,
        annotation: Option<TypeAnnotation>,
        expr: Expr,
    },
    ParametricType {
        name: String,
        specifiers: Vec<String>,
        params: Vec<TypeParam>,
        expr: Expr,
    },
    TypeAlias {
        name: String,
        target: TypeAnnotation,
    },
    ExtensionMethod(Box<ExtensionMethod>),
    Var {
        name: String,
        annotation: Option<TypeAnnotation>,
        expr: Expr,
    },
    Set {
        target: Expr,
        op: AssignOp,
        expr: Expr,
    },
    Return(Expr),
    Break,
    Defer(Expr),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Number {
        value: NumberLiteral,
        kind: NumberKind,
    },
    Char {
        value: char,
        kind: CharacterKind,
    },
    Bool(bool),
    String(String),
    InterpolatedString(Vec<InterpolatedStringPart>),
    None,
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    FailureBind {
        name: String,
        expr: Box<Expr>,
    },
    FailureSequence(Vec<Expr>),
    Set {
        target: Box<Expr>,
        op: AssignOp,
        expr: Box<Expr>,
    },
    Var {
        name: String,
        annotation: TypeAnnotation,
        expr: Box<Expr>,
    },
    External,
    Loop {
        body: Box<Expr>,
    },
    For {
        clauses: Vec<ForClause>,
        body: Box<Expr>,
    },
    Profile {
        description: Box<Expr>,
        body: Box<Expr>,
    },
    Spawn {
        body: Box<Expr>,
    },
    Concurrent {
        op: ConcurrentOp,
        body: Box<Expr>,
    },
    Block(Vec<Stmt>),
    ColonBlock(Vec<Stmt>),
    Function {
        params: Vec<Param>,
        effects: Vec<String>,
        return_type: Option<TypeAnnotation>,
        body: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    BracketCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Array(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    EnumDefinition {
        open: bool,
        persistable: bool,
        block: bool,
        variants: Vec<EnumVariant>,
    },
    StructDefinition {
        persistable: bool,
        computes: bool,
        block: bool,
        fields: Vec<StructField>,
    },
    ClassDefinition {
        block: bool,
        specifiers: Vec<String>,
        base: Option<TypeAnnotation>,
        interfaces: Vec<TypeAnnotation>,
        fields: Vec<StructField>,
        methods: Vec<ClassMethod>,
        extension_methods: Vec<ExtensionMethod>,
        blocks: Vec<ClassBlock>,
    },
    InterfaceDefinition {
        block: bool,
        parents: Vec<TypeAnnotation>,
        fields: Vec<StructField>,
        methods: Vec<ClassMethod>,
    },
    ModuleDefinition {
        block: bool,
        statements: Vec<Stmt>,
    },
    Archetype {
        block: bool,
        callee: Box<Expr>,
        entries: Vec<ArchetypeEntry>,
    },
    Case {
        subject: Box<Expr>,
        arms: Vec<CaseArm>,
    },
    Tuple(Vec<Expr>),
    Option(Option<Box<Expr>>),
    UnwrapOption(Box<Expr>),
    QualifiedName {
        qualifier: String,
        name: String,
    },
    QualifiedMember {
        object: Box<Expr>,
        qualifier: String,
        name: String,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    Index {
        collection: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrentOp {
    Sync,
    Race,
    Rush,
    Branch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedStringPart {
    Text(String),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub annotation: Option<TypeAnnotation>,
    pub type_params: Vec<TypeParam>,
    pub named: bool,
    pub default: Option<Expr>,
    pub pattern: ParamPattern,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: String,
    pub constraint: TypeParamConstraint,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeParamConstraint {
    Type,
    Subtype(TypeName),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamPattern {
    Binding,
    Anonymous,
    Tuple(Vec<Param>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub attributes: Vec<FieldAttribute>,
    pub var_specifiers: Vec<String>,
    pub specifiers: Vec<String>,
    pub annotation: Option<TypeAnnotation>,
    pub default: Option<Expr>,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldAttribute {
    pub name: String,
    pub arguments: Vec<AttributeArgument>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeArgument {
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassMethod {
    pub qualifier: Option<String>,
    pub name: String,
    pub params: Vec<Param>,
    pub effects: Vec<String>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionMethod {
    pub receiver: Param,
    pub method: ClassMethod,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassBlock {
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchetypeField {
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchetypeLet {
    pub name: String,
    pub annotation: Option<TypeAnnotation>,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchetypeConstructorCall {
    pub name: String,
    pub args: Vec<CallArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchetypeEntry {
    Field(ArchetypeField),
    Let(ArchetypeLet),
    Block(Expr),
    ConstructorCall(ArchetypeConstructorCall),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub qualifier: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub ignore_unreachable: bool,
    pub pattern: CasePattern,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CasePattern {
    Wildcard { span: Span },
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    Positional(Expr),
    Named {
        name: String,
        expr: Expr,
        optional: bool,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAnnotation {
    pub name: TypeName,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeName {
    Int,
    Float,
    Rational,
    Number,
    Bool,
    String,
    Message,
    Char,
    Char8,
    Char32,
    None,
    Any,
    Comparable,
    IntRange {
        min: i64,
        max: i64,
    },
    Array(Option<Box<TypeName>>),
    Map(Box<TypeName>, Box<TypeName>),
    WeakMap(Box<TypeName>, Box<TypeName>),
    Tuple(Vec<TypeName>),
    Option(Box<TypeName>),
    Function,
    FunctionSignature {
        params: Vec<TypeName>,
        effects: Vec<String>,
        return_type: Box<TypeName>,
    },
    Applied {
        name: String,
        args: Vec<TypeName>,
    },
    Named(String),
}

impl TypeName {
    pub fn parse(name: String) -> Self {
        match name.as_str() {
            "int" => Self::Int,
            "float" => Self::Float,
            "rational" => Self::Rational,
            "number" => Self::Number,
            "bool" => Self::Bool,
            "string" => Self::String,
            "message" => Self::Message,
            "char" => Self::Char,
            "char8" => Self::Char8,
            "char32" => Self::Char32,
            "none" => Self::None,
            "void" => Self::None,
            "any" => Self::Any,
            "comparable" => Self::Comparable,
            "logic" => Self::Bool,
            "array" => Self::Array(None),
            "function" => Self::Function,
            _ => Self::Named(name),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Positive,
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForBinding {
    Value(String),
    Pair { key: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForClause {
    Generator {
        binding: ForBinding,
        iterable: Expr,
        span: Span,
    },
    Let {
        name: String,
        expr: Expr,
        span: Span,
    },
    RangeOrLet {
        name: String,
        expr: Expr,
        span: Span,
    },
    Filter(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Range,
}
