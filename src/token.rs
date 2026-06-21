#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    pub fn through(self, end: Span) -> Self {
        Self {
            start: self.start,
            end: end.end,
            line: self.line,
            column: self.column,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringLiteralPart {
    Text(String),
    Interpolation { source: String, span: Span },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    Int,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberLiteral {
    Int(i128),
    Float(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterKind {
    Char,
    Char32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number {
        value: NumberLiteral,
        kind: NumberKind,
    },
    Char {
        value: char,
        kind: CharacterKind,
    },
    String(Vec<StringLiteralPart>),
    If,
    Else,
    True,
    False,
    None,
    Var,
    Set,
    Loop,
    For,
    Do,
    Break,
    Return,
    Defer,
    And,
    Or,
    Not,
    Dot,
    DotDot,
    At,
    Arrow,
    FatArrow,
    Question,
    Colon,
    ColonEqual,
    Equal,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    PlusEqual,
    Minus,
    MinusEqual,
    Star,
    StarEqual,
    Slash,
    SlashEqual,
    Percent,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Newline,
    Eof,
}
