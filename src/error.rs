use std::error::Error;
use std::fmt;

use crate::token::Span;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            message: message.into(),
            span,
        }
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
        Self::Lex(Diagnostic::new(message, Some(span)))
    }

    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self::Parse(Diagnostic::new(message, Some(span)))
    }

    pub fn check(message: impl Into<String>) -> Self {
        Self::Check(Diagnostic::new(message, None))
    }

    pub fn check_at(message: impl Into<String>, span: Span) -> Self {
        Self::Check(Diagnostic::new(message, Some(span)))
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(Diagnostic::new(message, None))
    }

    pub fn runtime_at(message: impl Into<String>, span: Span) -> Self {
        Self::Runtime(Diagnostic::new(message, Some(span)))
    }

    pub fn pretty(&self, source: &str) -> String {
        let diagnostic = match self {
            Self::Lex(diagnostic)
            | Self::Parse(diagnostic)
            | Self::Check(diagnostic)
            | Self::Runtime(diagnostic) => diagnostic,
        };

        let Some(span) = diagnostic.span else {
            return diagnostic.message.clone();
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
            "{} at line {}, column {}\n{}\n{}",
            diagnostic.message, span.line, span.column, line_text, marker
        )
    }
}

impl fmt::Display for VerseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(diagnostic) => write!(formatter, "lexer error: {}", diagnostic.message),
            Self::Parse(diagnostic) => write!(formatter, "parse error: {}", diagnostic.message),
            Self::Check(diagnostic) => write!(formatter, "check error: {}", diagnostic.message),
            Self::Runtime(diagnostic) => write!(formatter, "runtime error: {}", diagnostic.message),
        }
    }
}

impl Error for VerseError {}
