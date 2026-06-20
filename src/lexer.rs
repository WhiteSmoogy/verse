use crate::error::VerseError;
use crate::token::{
    CharacterKind, NumberKind, NumberLiteral, Span, StringLiteralPart, Token, TokenKind,
};

pub fn lex(source: &str) -> Result<Vec<Token>, VerseError> {
    Lexer::new(source).lex()
}

struct Lexer {
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, VerseError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            let start = self.snapshot();
            let ch = self.advance();

            match ch {
                ' ' | '\t' => {}
                '\r' => {
                    if self.peek() == Some('\n') {
                        self.advance();
                    }
                    tokens.push(self.token(TokenKind::Newline, start));
                }
                '\n' => tokens.push(self.token(TokenKind::Newline, start)),
                '#' => self.skip_line_comment(),
                ':' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::ColonEqual, start));
                }
                '.' if self.peek() == Some('.') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::DotDot, start));
                }
                '.' => tokens.push(self.token(TokenKind::Dot, start)),
                ':' => tokens.push(self.token(TokenKind::Colon, start)),
                '=' if self.peek() == Some('>') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::FatArrow, start));
                }
                '=' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::EqualEqual, start));
                }
                '<' if self.peek() == Some('#') => {
                    self.skip_angle_comment_after_less(start)?;
                }
                '!' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::NotEqual, start));
                }
                '<' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::LessEqual, start));
                }
                '<' if self.peek() == Some('>') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::NotEqual, start));
                }
                '>' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::GreaterEqual, start));
                }
                '=' => tokens.push(self.token(TokenKind::Equal, start)),
                '<' => tokens.push(self.token(TokenKind::Less, start)),
                '>' => tokens.push(self.token(TokenKind::Greater, start)),
                '@' => tokens.push(self.token(TokenKind::At, start)),
                '?' => tokens.push(self.token(TokenKind::Question, start)),
                '+' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::PlusEqual, start));
                }
                '+' => tokens.push(self.token(TokenKind::Plus, start)),
                '-' if self.peek() == Some('>') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Arrow, start));
                }
                '-' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::MinusEqual, start));
                }
                '-' => tokens.push(self.token(TokenKind::Minus, start)),
                '*' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::StarEqual, start));
                }
                '*' => tokens.push(self.token(TokenKind::Star, start)),
                '/' if self.peek() == Some('=') => {
                    self.advance();
                    tokens.push(self.token(TokenKind::SlashEqual, start));
                }
                '/' => tokens.push(self.token(TokenKind::Slash, start)),
                '%' => tokens.push(self.token(TokenKind::Percent, start)),
                '(' => tokens.push(self.token(TokenKind::LParen, start)),
                ')' => tokens.push(self.token(TokenKind::RParen, start)),
                '[' => tokens.push(self.token(TokenKind::LBracket, start)),
                ']' => tokens.push(self.token(TokenKind::RBracket, start)),
                '{' => tokens.push(self.token(TokenKind::LBrace, start)),
                '}' => tokens.push(self.token(TokenKind::RBrace, start)),
                ',' => tokens.push(self.token(TokenKind::Comma, start)),
                ';' => tokens.push(self.token(TokenKind::Semicolon, start)),
                '\'' => tokens.push(self.character(start)?),
                '"' => tokens.push(self.string(start)?),
                ch if ch.is_ascii_digit() => tokens.push(self.number(start, ch)?),
                ch if is_ident_start(ch) => tokens.push(self.identifier(start, ch)),
                _ => {
                    return Err(VerseError::lex(
                        format!("unexpected character `{ch}`"),
                        self.span_from(start),
                    ));
                }
            }
        }

        let eof = Span::new(self.index, self.index, self.line, self.column);
        tokens.push(Token::new(TokenKind::Eof, eof));
        Ok(tokens)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            index: self.index,
            line: self.line,
            column: self.column,
        }
    }

    fn token(&self, kind: TokenKind, start: Snapshot) -> Token {
        Token::new(kind, self.span_from(start))
    }

    fn span_from(&self, start: Snapshot) -> Span {
        Span::new(start.index, self.index, start.line, start.column)
    }

    fn is_at_end(&self) -> bool {
        self.index >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn peek_offset(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    fn advance(&mut self) -> char {
        let ch = self.chars[self.index];
        self.index += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self, start: Snapshot) -> Result<(), VerseError> {
        let mut depth = 1usize;

        while let Some(ch) = self.peek() {
            if ch == '<' && self.peek_next() == Some('#') {
                self.advance();
                self.advance();
                depth += 1;
                continue;
            }

            if ch == '#' && self.peek_next() == Some('>') {
                self.advance();
                self.advance();
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
                continue;
            }

            self.advance();
        }

        Err(VerseError::lex(
            "unterminated block comment",
            self.span_from(start),
        ))
    }

    fn skip_angle_comment_after_less(&mut self, start: Snapshot) -> Result<(), VerseError> {
        debug_assert_eq!(self.peek(), Some('#'));
        self.advance();
        if self.peek() == Some('>') {
            self.advance();
            self.skip_indented_comment();
            Ok(())
        } else {
            self.skip_block_comment(start)
        }
    }

    fn skip_indented_comment(&mut self) {
        if self.peek() == Some('\r') {
            self.advance();
            if self.peek() == Some('\n') {
                self.advance();
            }
        } else if self.peek() == Some('\n') {
            self.advance();
        } else {
            return;
        }

        loop {
            let line_start = self.snapshot();
            let mut spaces = 0usize;
            while spaces < 4 && self.peek() == Some(' ') {
                self.advance();
                spaces += 1;
            }

            if spaces < 4 {
                self.index = line_start.index;
                self.line = line_start.line;
                self.column = line_start.column;
                return;
            }

            while let Some(ch) = self.peek() {
                self.advance();
                if ch == '\n' {
                    break;
                }
                if ch == '\r' {
                    if self.peek() == Some('\n') {
                        self.advance();
                    }
                    break;
                }
            }

            if self.is_at_end() {
                return;
            }
        }
    }

    fn character(&mut self, start: Snapshot) -> Result<Token, VerseError> {
        let Some(ch) = self.peek() else {
            return Err(VerseError::lex(
                "unterminated character literal",
                self.span_from(start),
            ));
        };

        let value = match ch {
            '\'' => {
                return Err(VerseError::lex(
                    "empty character literal",
                    self.span_from(start),
                ));
            }
            '\n' | '\r' => {
                return Err(VerseError::lex(
                    "unterminated character literal",
                    self.span_from(start),
                ));
            }
            '\\' => {
                self.advance();
                let Some(escaped) = self.peek() else {
                    return Err(VerseError::lex(
                        "unterminated character escape",
                        self.span_from(start),
                    ));
                };
                self.advance();
                decode_escape(escaped).ok_or_else(|| {
                    VerseError::lex(
                        format!("unsupported escape sequence `\\{escaped}`"),
                        self.span_from(start),
                    )
                })?
            }
            _ => self.advance(),
        };

        match self.peek() {
            Some('\'') => self.advance(),
            Some('\n' | '\r') | None => {
                return Err(VerseError::lex(
                    "unterminated character literal",
                    self.span_from(start),
                ));
            }
            Some(_) => {
                return Err(VerseError::lex(
                    "character literal cannot contain multiple characters",
                    self.span_from(start),
                ));
            }
        };

        let kind = if value.is_ascii() {
            CharacterKind::Char
        } else {
            CharacterKind::Char32
        };
        Ok(self.token(TokenKind::Char { value, kind }, start))
    }

    fn string(&mut self, start: Snapshot) -> Result<Token, VerseError> {
        let mut text = String::new();
        let mut parts = Vec::new();

        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    self.advance();
                    if !text.is_empty() || parts.is_empty() {
                        parts.push(StringLiteralPart::Text(text));
                    }
                    return Ok(self.token(TokenKind::String(parts), start));
                }
                '\\' => {
                    self.advance();
                    let Some(escaped) = self.peek() else {
                        return Err(VerseError::lex(
                            "unterminated string escape",
                            self.span_from(start),
                        ));
                    };
                    self.advance();
                    if let Some(value) = decode_escape(escaped) {
                        text.push(value);
                    } else {
                        return Err(VerseError::lex(
                            format!("unsupported escape sequence `\\{escaped}`"),
                            self.span_from(start),
                        ));
                    }
                }
                '{' => {
                    self.advance();
                    if !text.is_empty() {
                        parts.push(StringLiteralPart::Text(std::mem::take(&mut text)));
                    }
                    let interpolation_start = self.snapshot();
                    let source = self.string_interpolation(start)?;
                    let span = self.span_from(interpolation_start);
                    if source.trim().is_empty() {
                        continue;
                    }
                    parts.push(StringLiteralPart::Interpolation { source, span });
                }
                '}' => {
                    return Err(VerseError::lex(
                        "unescaped `}` in string literal",
                        self.span_from(start),
                    ));
                }
                '<' if self.peek_next() == Some('#') => {
                    let comment_start = self.snapshot();
                    self.advance();
                    self.skip_angle_comment_after_less(comment_start)?;
                }
                '\n' | '\r' => {
                    return Err(VerseError::lex(
                        "unterminated string",
                        self.span_from(start),
                    ));
                }
                _ => {
                    text.push(ch);
                    self.advance();
                }
            }
        }

        Err(VerseError::lex(
            "unterminated string",
            self.span_from(start),
        ))
    }

    fn string_interpolation(&mut self, string_start: Snapshot) -> Result<String, VerseError> {
        let mut source = String::new();
        let mut depth = 1usize;

        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    depth += 1;
                    source.push(self.advance());
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        return Ok(source);
                    }
                    source.push(self.advance());
                }
                '"' => self.copy_nested_string(&mut source, string_start)?,
                '#' => self.skip_line_comment(),
                '<' if self.peek_next() == Some('#') => {
                    let comment_start = self.snapshot();
                    self.advance();
                    self.skip_angle_comment_after_less(comment_start)?;
                }
                '\n' | '\r' => source.push(self.advance()),
                _ => source.push(self.advance()),
            }
        }

        Err(VerseError::lex(
            "unterminated string interpolation",
            self.span_from(string_start),
        ))
    }

    fn copy_nested_string(
        &mut self,
        source: &mut String,
        string_start: Snapshot,
    ) -> Result<(), VerseError> {
        source.push(self.advance());

        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    source.push(self.advance());
                    return Ok(());
                }
                '\\' => {
                    source.push(self.advance());
                    let Some(_) = self.peek() else {
                        return Err(VerseError::lex(
                            "unterminated string escape",
                            self.span_from(string_start),
                        ));
                    };
                    source.push(self.advance());
                }
                '<' if self.peek_next() == Some('#') => {
                    let comment_start = self.snapshot();
                    self.advance();
                    self.skip_angle_comment_after_less(comment_start)?;
                }
                '\n' | '\r' => {
                    return Err(VerseError::lex(
                        "unterminated string interpolation",
                        self.span_from(string_start),
                    ));
                }
                _ => source.push(self.advance()),
            }
        }

        Err(VerseError::lex(
            "unterminated string interpolation",
            self.span_from(string_start),
        ))
    }

    fn number(&mut self, start: Snapshot, first: char) -> Result<Token, VerseError> {
        if first == '0' && self.peek() == Some('x') {
            self.advance();
            return self.hex_number(start);
        }
        if first == '0' && self.peek() == Some('o') {
            self.advance();
            return self.byte_character(start);
        }
        if first == '0' && self.peek() == Some('u') {
            self.advance();
            return self.unicode_character(start);
        }

        let mut literal = String::from(first);

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                literal.push(self.advance());
            } else {
                break;
            }
        }

        let mut kind = NumberKind::Int;
        if self.peek() == Some('.') && self.peek_next().is_some_and(|ch| ch.is_ascii_digit()) {
            kind = NumberKind::Float;
            literal.push(self.advance());
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    literal.push(self.advance());
                } else {
                    break;
                }
            }
        }

        if kind == NumberKind::Float && matches!(self.peek(), Some('e' | 'E')) {
            literal.push(self.advance());
            if matches!(self.peek(), Some('+' | '-')) {
                literal.push(self.advance());
            }
            if !self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                return Err(VerseError::lex(
                    "expected exponent digits in float literal",
                    self.span_from(start),
                ));
            }
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    literal.push(self.advance());
                } else {
                    break;
                }
            }
        }

        if kind == NumberKind::Float
            && self.peek() == Some('f')
            && self.peek_offset(1) == Some('6')
            && self.peek_offset(2) == Some('4')
        {
            self.advance();
            self.advance();
            self.advance();
        }

        let value = match kind {
            NumberKind::Int => {
                let value = literal.parse::<i128>().map_err(|_| {
                    VerseError::lex(
                        format!("invalid integer literal `{literal}`"),
                        self.span_from(start),
                    )
                })?;
                self.check_integer_literal_range(value, &literal, start)?;
                NumberLiteral::Int(value)
            }
            NumberKind::Float => {
                let value = literal.parse::<f64>().map_err(|_| {
                    VerseError::lex(
                        format!("invalid float literal `{literal}`"),
                        self.span_from(start),
                    )
                })?;
                if !value.is_finite() {
                    return Err(VerseError::lex(
                        format!("float literal `{literal}` is outside the finite f64 range"),
                        self.span_from(start),
                    ));
                }
                NumberLiteral::Float(value)
            }
        };

        Ok(self.token(TokenKind::Number { value, kind }, start))
    }

    fn check_integer_literal_range(
        &self,
        value: i128,
        literal: &str,
        start: Snapshot,
    ) -> Result<(), VerseError> {
        if value <= i128::from(i64::MAX) + 1 {
            return Ok(());
        }

        Err(VerseError::lex(
            format!("integer literal `{literal}` is outside the 64-bit signed range"),
            self.span_from(start),
        ))
    }

    fn hex_number(&mut self, start: Snapshot) -> Result<Token, VerseError> {
        let mut literal = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_ascii_hexdigit() {
                literal.push(self.advance());
            } else {
                break;
            }
        }

        if literal.is_empty() {
            return Err(VerseError::lex(
                "expected hexadecimal digits after `0x`",
                self.span_from(start),
            ));
        }

        let value = u128::from_str_radix(&literal, 16).map_err(|_| {
            VerseError::lex(
                format!("invalid hexadecimal literal `0x{literal}`"),
                self.span_from(start),
            )
        })?;
        if value > i64::MAX as u128 + 1 {
            return Err(VerseError::lex(
                format!("integer literal `0x{literal}` is outside the 64-bit signed range"),
                self.span_from(start),
            ));
        }

        Ok(self.token(
            TokenKind::Number {
                value: NumberLiteral::Int(value as i128),
                kind: NumberKind::Int,
            },
            start,
        ))
    }

    fn byte_character(&mut self, start: Snapshot) -> Result<Token, VerseError> {
        let literal =
            self.fixed_hex_digits(2, "expected two hexadecimal digits after `0o`", start)?;
        if self.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
            return Err(VerseError::lex(
                "`0o` character literal expects exactly two hexadecimal digits",
                self.span_from(start),
            ));
        }

        let value = u8::from_str_radix(&literal, 16).map_err(|_| {
            VerseError::lex(
                format!("invalid `char` hexadecimal literal `0o{literal}`"),
                self.span_from(start),
            )
        })?;
        let value = char::from(value);

        Ok(self.token(
            TokenKind::Char {
                value,
                kind: CharacterKind::Char,
            },
            start,
        ))
    }

    fn unicode_character(&mut self, start: Snapshot) -> Result<Token, VerseError> {
        let mut literal = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_ascii_hexdigit() && literal.len() < 6 {
                literal.push(self.advance());
            } else {
                break;
            }
        }

        if literal.is_empty() {
            return Err(VerseError::lex(
                "expected hexadecimal digits after `0u`",
                self.span_from(start),
            ));
        }

        if self.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
            return Err(VerseError::lex(
                "`0u` character literal expects at most six hexadecimal digits",
                self.span_from(start),
            ));
        }

        let codepoint = u32::from_str_radix(&literal, 16).map_err(|_| {
            VerseError::lex(
                format!("invalid `char32` hexadecimal literal `0u{literal}`"),
                self.span_from(start),
            )
        })?;
        let Some(value) = char::from_u32(codepoint) else {
            return Err(VerseError::lex(
                format!("invalid Unicode code point `0u{literal}`"),
                self.span_from(start),
            ));
        };

        Ok(self.token(
            TokenKind::Char {
                value,
                kind: CharacterKind::Char32,
            },
            start,
        ))
    }

    fn fixed_hex_digits(
        &mut self,
        count: usize,
        message: &str,
        start: Snapshot,
    ) -> Result<String, VerseError> {
        let mut literal = String::new();
        while literal.len() < count {
            let Some(ch) = self.peek() else {
                break;
            };
            if !ch.is_ascii_hexdigit() {
                break;
            }
            literal.push(self.advance());
        }

        if literal.len() == count {
            Ok(literal)
        } else {
            Err(VerseError::lex(message, self.span_from(start)))
        }
    }

    fn identifier(&mut self, start: Snapshot, first: char) -> Token {
        let mut ident = String::from(first);

        while let Some(ch) = self.peek() {
            if is_ident_continue(ch) {
                ident.push(self.advance());
            } else {
                break;
            }
        }

        let kind = match ident.as_str() {
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "none" => TokenKind::None,
            "var" => TokenKind::Var,
            "set" => TokenKind::Set,
            "loop" => TokenKind::Loop,
            "for" => TokenKind::For,
            "do" => TokenKind::Do,
            "break" => TokenKind::Break,
            "return" => TokenKind::Return,
            "defer" => TokenKind::Defer,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            _ => TokenKind::Ident(ident),
        };

        self.token(kind, start)
    }
}

#[derive(Clone, Copy)]
struct Snapshot {
    index: usize,
    line: usize,
    column: usize,
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn decode_escape(ch: char) -> Option<char> {
    match ch {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '\\' => Some('\\'),
        '{' => Some('{'),
        '}' => Some('}'),
        '<' => Some('<'),
        '>' => Some('>'),
        '&' => Some('&'),
        '#' => Some('#'),
        '~' => Some('~'),
        _ => None,
    }
}
