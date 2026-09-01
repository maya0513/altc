use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub span: Span,
    pub start: SourcePosition,
    pub end: SourcePosition,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        file: &str,
        text: &str,
        span: Span,
    ) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            file: file.to_owned(),
            span,
            start: position(text, span.start),
            end: position(text, span.end),
            labels: vec![Label {
                span,
                message: String::new(),
                primary: true,
            }],
            notes: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
            primary: false,
        });
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[must_use]
pub fn position(text: &str, byte: usize) -> SourcePosition {
    let byte = byte.min(text.len());
    let before = &text[..byte];
    let line = before.bytes().filter(|value| *value == b'\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = text[line_start..byte].chars().count() + 1;
    SourcePosition { line, column }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enriches_diagnostics_and_clamps_positions() {
        let diagnostic = Diagnostic::error("E", "message", "test", "a\nλ", Span::new(2, 99))
            .with_label(Span::new(0, 1), "secondary")
            .with_note("note")
            .with_help("help");
        assert_eq!(diagnostic.start, SourcePosition { line: 2, column: 1 });
        assert_eq!(diagnostic.end, SourcePosition { line: 2, column: 2 });
        assert_eq!(diagnostic.labels.len(), 2);
        assert_eq!(diagnostic.notes, ["note"]);
        assert_eq!(diagnostic.help.as_deref(), Some("help"));
    }
}
