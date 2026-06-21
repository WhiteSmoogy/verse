use std::error::Error;
use std::fmt;

use crate::token::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(formatter, "error"),
            Self::Warning => write!(formatter, "warning"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    LexError,
    ParseError,
    CheckError,
    RuntimeError,
    UnreachableCode,
    EmptyBlock,
}

impl DiagnosticCode {
    pub const fn number(self) -> u16 {
        match self {
            Self::LexError => 1000,
            Self::ParseError => 1001,
            Self::CheckError => 1002,
            Self::RuntimeError => 1003,
            Self::UnreachableCode => 2000,
            Self::EmptyBlock => 2001,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LexError => "V1000",
            Self::ParseError => "V1001",
            Self::CheckError => "V1002",
            Self::RuntimeError => "V1003",
            Self::UnreachableCode => "V2000",
            Self::EmptyBlock => "V2001",
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::LexError => "lex-error",
            Self::ParseError => "parse-error",
            Self::CheckError => "check-error",
            Self::RuntimeError => "runtime-error",
            Self::UnreachableCode => "unreachable-code",
            Self::EmptyBlock => "empty-block",
        }
    }

    pub const fn default_severity(self) -> DiagnosticSeverity {
        match self {
            Self::LexError | Self::ParseError | Self::CheckError | Self::RuntimeError => {
                DiagnosticSeverity::Error
            }
            Self::UnreachableCode | Self::EmptyBlock => DiagnosticSeverity::Warning,
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiagnosticDescriptor {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub symbol: &'static str,
    pub message_template: &'static str,
}

pub const DIAGNOSTIC_DESCRIPTORS: &[DiagnosticDescriptor] = &[
    DiagnosticDescriptor {
        code: DiagnosticCode::LexError,
        severity: DiagnosticSeverity::Error,
        symbol: "lex-error",
        message_template: "{message}",
    },
    DiagnosticDescriptor {
        code: DiagnosticCode::ParseError,
        severity: DiagnosticSeverity::Error,
        symbol: "parse-error",
        message_template: "{message}",
    },
    DiagnosticDescriptor {
        code: DiagnosticCode::CheckError,
        severity: DiagnosticSeverity::Error,
        symbol: "check-error",
        message_template: "{message}",
    },
    DiagnosticDescriptor {
        code: DiagnosticCode::RuntimeError,
        severity: DiagnosticSeverity::Error,
        symbol: "runtime-error",
        message_template: "{message}",
    },
    DiagnosticDescriptor {
        code: DiagnosticCode::UnreachableCode,
        severity: DiagnosticSeverity::Warning,
        symbol: "unreachable-code",
        message_template: "{message}",
    },
    DiagnosticDescriptor {
        code: DiagnosticCode::EmptyBlock,
        severity: DiagnosticSeverity::Warning,
        symbol: "empty-block",
        message_template: "empty block",
    },
];

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn new(
        code: DiagnosticCode,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            span,
        }
    }

    pub fn error(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(code, DiagnosticSeverity::Error, message, span)
    }

    pub fn warning(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(code, DiagnosticSeverity::Warning, message, span)
    }
}

#[derive(Debug, Clone)]
pub enum VerseError {
    Lex(Diagnostic),
    Parse(Diagnostic),
    Check(Diagnostic),
    Runtime(Diagnostic),
}

impl VerseError {
    pub fn lex(message: impl Into<String>, span: Span) -> Self {
        Self::Lex(Diagnostic::error(
            DiagnosticCode::LexError,
            message,
            Some(span),
        ))
    }

    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self::Parse(Diagnostic::error(
            DiagnosticCode::ParseError,
            message,
            Some(span),
        ))
    }

    pub fn check(message: impl Into<String>) -> Self {
        Self::Check(Diagnostic::error(DiagnosticCode::CheckError, message, None))
    }

    pub fn check_at(message: impl Into<String>, span: Span) -> Self {
        Self::Check(Diagnostic::error(
            DiagnosticCode::CheckError,
            message,
            Some(span),
        ))
    }

    pub fn check_with_code(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::Check(Diagnostic::error(code, message, None))
    }

    pub fn check_at_with_code(
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::Check(Diagnostic::error(code, message, Some(span)))
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(Diagnostic::error(
            DiagnosticCode::RuntimeError,
            message,
            None,
        ))
    }

    pub fn runtime_at(message: impl Into<String>, span: Span) -> Self {
        Self::Runtime(Diagnostic::error(
            DiagnosticCode::RuntimeError,
            message,
            Some(span),
        ))
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        match self {
            Self::Lex(diagnostic)
            | Self::Parse(diagnostic)
            | Self::Check(diagnostic)
            | Self::Runtime(diagnostic) => diagnostic,
        }
    }

    pub fn code(&self) -> DiagnosticCode {
        self.diagnostic().code
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.diagnostic().severity
    }

    pub fn pretty(&self, source: &str) -> String {
        let diagnostic = self.diagnostic();

        let Some(span) = diagnostic.span else {
            return format!(
                "{}[{}]: {}",
                diagnostic.severity, diagnostic.code, diagnostic.message
            );
        };

        let line_text = source
            .lines()
            .nth(span.line.saturating_sub(1))
            .unwrap_or_default();
        let line_width = line_text.chars().count();
        let available_width = line_width
            .saturating_sub(span.column.saturating_sub(1))
            .max(1);
        let caret_width = (span.end.saturating_sub(span.start))
            .max(1)
            .min(available_width);
        let mut marker = String::new();
        marker.push_str(&" ".repeat(span.column.saturating_sub(1)));
        marker.push_str(&"^".repeat(caret_width));

        format!(
            "{}[{}]: {} at line {}, column {}\n{}\n{}",
            diagnostic.severity,
            diagnostic.code,
            diagnostic.message,
            span.line,
            span.column,
            line_text,
            marker
        )
    }
}

impl fmt::Display for VerseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(diagnostic) => {
                write!(
                    formatter,
                    "lexer error[{}]: {}",
                    diagnostic.code, diagnostic.message
                )
            }
            Self::Parse(diagnostic) => {
                write!(
                    formatter,
                    "parse error[{}]: {}",
                    diagnostic.code, diagnostic.message
                )
            }
            Self::Check(diagnostic) => {
                write!(
                    formatter,
                    "check error[{}]: {}",
                    diagnostic.code, diagnostic.message
                )
            }
            Self::Runtime(diagnostic) => {
                write!(
                    formatter,
                    "runtime error[{}]: {}",
                    diagnostic.code, diagnostic.message
                )
            }
        }
    }
}

impl Error for VerseError {}
