use rowan::GreenNodeBuilder;

use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::{SyntaxKind, SyntaxNode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Whitespace,
    LineComment,
    BlockComment,
    Ident,
    Keyword,
    Int,
    Float,
    String,
    CString,
    ByteString,
    Char,
    Byte,
    Punct,
    Error,
    Eof,
}

impl TokenKind {
    const fn syntax(self) -> SyntaxKind {
        match self {
            Self::Whitespace => SyntaxKind::Whitespace,
            Self::LineComment => SyntaxKind::LineComment,
            Self::BlockComment => SyntaxKind::BlockComment,
            Self::Ident => SyntaxKind::Ident,
            Self::Keyword => SyntaxKind::Keyword,
            Self::Int => SyntaxKind::Int,
            Self::Float => SyntaxKind::Float,
            Self::String => SyntaxKind::String,
            Self::CString => SyntaxKind::CString,
            Self::ByteString => SyntaxKind::ByteString,
            Self::Char => SyntaxKind::Char,
            Self::Byte => SyntaxKind::Byte,
            Self::Punct => SyntaxKind::Punct,
            Self::Error => SyntaxKind::Error,
            Self::Eof => SyntaxKind::Eof,
        }
    }

    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
    pub cst: SyntaxNode,
}

const KEYWORDS: &[&str] = &[
    "as", "break", "catch", "const", "continue", "defer", "else", "enum", "export", "extern",
    "false", "fn", "for", "from", "if", "impl", "import", "in", "let", "macro", "match", "mut",
    "null", "opaque", "pub", "raise", "return", "struct", "test", "trait", "true", "try", "type",
    "var", "while", "with",
];

const PUNCTUATION: &[&str] = &[
    "+%=", "-%=", "*%=", "+|=", "-|=", "*|=", "...", "::", "->", "=>", "<=", ">=", "==", "!=",
    "&&", "||", "<<", ">>", "+=", "-=", "*=", "/=", "+%", "-%", "*%", "+|", "-|", "*|", "..", "{",
    "}", "(", ")", "[", "]", ",", ";", ":", "@", ".", "?", "+", "-", "*", "/", "%", "!", "~", "&",
    "|", "^", "<", ">", "=",
];

#[must_use]
pub fn lex(path: &str, text: &str) -> Lexed {
    let mut lexer = Lexer {
        path,
        text,
        offset: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };
    lexer.run();

    let green = build_lossless_tree(&lexer.tokens);

    Lexed {
        tokens: lexer.tokens,
        diagnostics: lexer.diagnostics,
        cst: SyntaxNode::new_root(green),
    }
}

fn build_lossless_tree(tokens: &[Token]) -> rowan::GreenNode {
    let mut builder = GreenNodeBuilder::new();
    let mut delimiters = Vec::new();
    builder.start_node(SyntaxKind::Root.into());
    for token in tokens {
        if token.kind == TokenKind::Eof {
            continue;
        }
        let node = match token.text.as_str() {
            "{" => Some((SyntaxKind::Block, "}")),
            "(" => Some((SyntaxKind::Parenthesized, ")")),
            "[" => Some((SyntaxKind::Bracketed, "]")),
            _ => None,
        };
        if let Some((kind, close)) = node {
            builder.start_node(kind.into());
            builder.token(token.kind.syntax().into(), &token.text);
            delimiters.push(close);
        } else if delimiters.last().is_some_and(|close| *close == token.text) {
            builder.token(token.kind.syntax().into(), &token.text);
            builder.finish_node();
            delimiters.pop();
        } else {
            builder.token(token.kind.syntax().into(), &token.text);
        }
    }
    for _ in delimiters {
        builder.finish_node();
    }
    builder.finish_node();
    builder.finish()
}

struct Lexer<'a> {
    path: &'a str,
    text: &'a str,
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn run(&mut self) {
        if self.text.starts_with('\u{feff}') {
            let end = '\u{feff}'.len_utf8();
            self.error("ALTC1001", "UTF-8 BOM is not allowed", 0, end);
            self.push(TokenKind::Error, 0, end);
            self.offset = end;
        }

        while self.offset < self.text.len() {
            let start = self.offset;
            let rest = &self.text[start..];

            if rest.starts_with("//") {
                self.offset += rest.find('\n').unwrap_or(rest.len());
                self.push(TokenKind::LineComment, start, self.offset);
            } else if let Some(comment_body) = rest.strip_prefix("/*") {
                if let Some(end) = comment_body.find("*/") {
                    self.offset += end + 4;
                    self.push(TokenKind::BlockComment, start, self.offset);
                } else {
                    self.offset = self.text.len();
                    self.push(TokenKind::BlockComment, start, self.offset);
                    self.error("ALTC1002", "unterminated block comment", start, self.offset);
                }
            } else if rest.as_bytes()[0].is_ascii_whitespace() {
                self.offset += 1;
                while self.offset < self.text.len()
                    && self.text.as_bytes()[self.offset].is_ascii_whitespace()
                {
                    self.offset += 1;
                }
                self.push(TokenKind::Whitespace, start, self.offset);
            } else if rest.starts_with("c\"") {
                self.scan_quoted(start, 1, b'"', TokenKind::CString);
            } else if rest.starts_with("b\"") {
                self.scan_quoted(start, 1, b'"', TokenKind::ByteString);
            } else if rest.starts_with("b'") {
                self.scan_quoted(start, 1, b'\'', TokenKind::Byte);
            } else if rest.starts_with('"') {
                self.scan_quoted(start, 0, b'"', TokenKind::String);
            } else if rest.starts_with('\'') {
                self.scan_quoted(start, 0, b'\'', TokenKind::Char);
            } else if rest.as_bytes()[0].is_ascii_digit() {
                self.scan_number(start);
            } else if is_ident_start(rest.as_bytes()[0]) {
                self.offset += 1;
                while self.offset < self.text.len()
                    && is_ident_continue(self.text.as_bytes()[self.offset])
                {
                    self.offset += 1;
                }
                let value = &self.text[start..self.offset];
                let kind = if KEYWORDS.contains(&value) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Ident
                };
                self.push(kind, start, self.offset);
            } else if let Some(punct) = PUNCTUATION.iter().find(|punct| rest.starts_with(**punct)) {
                self.offset += punct.len();
                self.push(TokenKind::Punct, start, self.offset);
            } else {
                let ch = rest.chars().next().expect("non-empty source remainder");
                self.offset += ch.len_utf8();
                self.push(TokenKind::Error, start, self.offset);
                self.error(
                    "ALTC1003",
                    format!("unexpected character `{ch}`"),
                    start,
                    self.offset,
                );
            }
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            text: String::new(),
            span: Span::new(self.text.len(), self.text.len()),
        });
    }

    fn scan_quoted(&mut self, start: usize, prefix: usize, quote: u8, kind: TokenKind) {
        self.offset += prefix + 1;
        let mut escaped = false;
        let mut terminated = false;
        while self.offset < self.text.len() {
            let byte = self.text.as_bytes()[self.offset];
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            self.offset += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                terminated = true;
                break;
            }
        }
        self.push(kind, start, self.offset);
        if !terminated {
            self.error(
                "ALTC1004",
                "unterminated string or character literal",
                start,
                self.offset,
            );
        }
    }

    fn scan_number(&mut self, start: usize) {
        let bytes = self.text.as_bytes();
        self.offset += 1;
        while self.offset < bytes.len()
            && (bytes[self.offset].is_ascii_alphanumeric() || bytes[self.offset] == b'_')
        {
            self.offset += 1;
        }
        let mut kind = TokenKind::Int;
        if self.offset < bytes.len()
            && bytes[self.offset] == b'.'
            && bytes.get(self.offset + 1) != Some(&b'.')
        {
            kind = TokenKind::Float;
            self.offset += 1;
            while self.offset < bytes.len()
                && (bytes[self.offset].is_ascii_digit() || bytes[self.offset] == b'_')
            {
                self.offset += 1;
            }
        }
        if self.offset < bytes.len() && matches!(bytes[self.offset], b'e' | b'E') {
            kind = TokenKind::Float;
            self.offset += 1;
            if self.offset < bytes.len() && matches!(bytes[self.offset], b'+' | b'-') {
                self.offset += 1;
            }
            while self.offset < bytes.len()
                && (bytes[self.offset].is_ascii_digit() || bytes[self.offset] == b'_')
            {
                self.offset += 1;
            }
        }
        while self.offset < bytes.len()
            && (bytes[self.offset].is_ascii_alphanumeric() || bytes[self.offset] == b'_')
        {
            self.offset += 1;
        }
        self.push(kind, start, self.offset);
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            text: self.text[start..end].to_owned(),
            span: Span::new(start, end),
        });
    }

    fn error(&mut self, code: &str, message: impl Into<String>, start: usize, end: usize) {
        self.diagnostics.push(Diagnostic::error(
            code,
            message,
            self.path,
            self.text,
            Span::new(start, end),
        ));
    }
}

const fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_trivia_and_longest_operators() {
        let source = "fn f() { // x\n a +%= 1u8; }";
        let lexed = lex("test.altc", source);
        assert!(lexed.diagnostics.is_empty());
        assert_eq!(lexed.cst.text().to_string(), source);
        assert!(lexed.tokens.iter().any(|token| token.text == "+%="));
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::LineComment)
        );
        assert!(
            lexed
                .cst
                .descendants()
                .any(|node| node.kind() == SyntaxKind::Block)
        );
    }

    #[test]
    fn diagnoses_bom_and_unterminated_literal() {
        let lexed = lex("test.altc", "\u{feff}\"oops");
        assert_eq!(lexed.diagnostics.len(), 2);
        assert_eq!(lexed.diagnostics[0].code, "ALTC1001");
        assert_eq!(lexed.diagnostics[1].code, "ALTC1004");
    }

    #[test]
    fn recognizes_every_punctuation_token_and_utf8_literal() {
        let source = format!("{} \"λ\"", PUNCTUATION.join(" "));
        let lexed = lex("test.altc", &source);
        assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
        for punctuation in PUNCTUATION {
            assert!(
                lexed.tokens.iter().any(|token| token.text == *punctuation),
                "missing token {punctuation}"
            );
        }
        assert!(lexed.tokens.iter().any(|token| token.text == "\"λ\""));
        assert_eq!(lexed.cst.text().to_string(), source);
    }

    #[test]
    fn covers_token_kinds_comments_literals_numbers_and_errors() {
        let source = "/* ok */ c\"c\" b\"bytes\" b'x' 'x' \"s\" 1.5 2e+3 4E-2 5..6 λ";
        let lexed = lex("test.altc", source);
        assert_eq!(lexed.diagnostics.len(), 1);
        for kind in [
            TokenKind::BlockComment,
            TokenKind::CString,
            TokenKind::ByteString,
            TokenKind::Byte,
            TokenKind::Char,
            TokenKind::String,
            TokenKind::Float,
            TokenKind::Int,
            TokenKind::Error,
            TokenKind::Eof,
        ] {
            assert!(lexed.tokens.iter().any(|token| token.kind == kind));
            let _ = kind.syntax();
        }

        let unterminated_comment = lex("test.altc", "/* open");
        assert_eq!(unterminated_comment.diagnostics[0].code, "ALTC1002");
        let newline_literal = lex("test.altc", "\"open\nnext");
        assert_eq!(newline_literal.diagnostics[0].code, "ALTC1004");
        let unbalanced = lex("test.altc", "{([ value");
        assert_eq!(unbalanced.cst.text().to_string(), "{([ value");

        let exponent = lex("numbers.altc", "1.0e+2suffix 2.0E-3_tail");
        assert_eq!(exponent.tokens[0].kind, TokenKind::Float);
        assert_eq!(exponent.tokens[0].text, "1.0e+2suffix");
        assert_eq!(exponent.tokens[2].kind, TokenKind::Float);
        assert_eq!(exponent.tokens[2].text, "2.0E-3_tail");
    }
}
