//! The altc v0 compiler frontend and C11 backend.

mod ast;
mod codegen;
mod diagnostic;
mod lexer;
mod literal;
mod parser;
mod sema;
mod syntax;

pub use codegen::{CodegenOptions, GeneratedUnit, emit_c};
pub use diagnostic::{Diagnostic, Label, Severity, SourcePosition, Span};
pub use lexer::{Lexed, Token, TokenKind, lex};
pub use sema::{Analysis, AnalyzeOptions, Effect, MemoryCategory, MemorySummary, analyze};
pub use syntax::{AltcLanguage, SyntaxKind, SyntaxNode};

/// A UTF-8 source file supplied to the compiler.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

impl SourceFile {
    #[must_use]
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }
}
