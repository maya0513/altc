use rowan::Language;

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SyntaxKind {
    Root,
    Whitespace,
    LineComment,
    BlockComment,
    Ident,
    Int,
    Float,
    String,
    CString,
    ByteString,
    Char,
    Byte,
    Keyword,
    Punct,
    Error,
    Eof,
    Block,
    Parenthesized,
    Bracketed,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AltcLanguage {}

impl Language for AltcLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        match raw.0 {
            0 => SyntaxKind::Root,
            1 => SyntaxKind::Whitespace,
            2 => SyntaxKind::LineComment,
            3 => SyntaxKind::BlockComment,
            4 => SyntaxKind::Ident,
            5 => SyntaxKind::Int,
            6 => SyntaxKind::Float,
            7 => SyntaxKind::String,
            8 => SyntaxKind::CString,
            9 => SyntaxKind::ByteString,
            10 => SyntaxKind::Char,
            11 => SyntaxKind::Byte,
            12 => SyntaxKind::Keyword,
            13 => SyntaxKind::Punct,
            14 => SyntaxKind::Error,
            15 => SyntaxKind::Eof,
            16 => SyntaxKind::Block,
            17 => SyntaxKind::Parenthesized,
            _ => SyntaxKind::Bracketed,
        }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<AltcLanguage>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_syntax_kind_in_both_directions() {
        let kinds = [
            SyntaxKind::Root,
            SyntaxKind::Whitespace,
            SyntaxKind::LineComment,
            SyntaxKind::BlockComment,
            SyntaxKind::Ident,
            SyntaxKind::Int,
            SyntaxKind::Float,
            SyntaxKind::String,
            SyntaxKind::CString,
            SyntaxKind::ByteString,
            SyntaxKind::Char,
            SyntaxKind::Byte,
            SyntaxKind::Keyword,
            SyntaxKind::Punct,
            SyntaxKind::Error,
            SyntaxKind::Eof,
            SyntaxKind::Block,
            SyntaxKind::Parenthesized,
            SyntaxKind::Bracketed,
        ];
        for kind in kinds {
            let raw = AltcLanguage::kind_to_raw(kind);
            assert_eq!(AltcLanguage::kind_from_raw(raw), kind);
        }
        assert_eq!(
            AltcLanguage::kind_from_raw(rowan::SyntaxKind(u16::MAX)),
            SyntaxKind::Bracketed
        );
    }
}
