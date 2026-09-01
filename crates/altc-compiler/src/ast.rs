#![allow(dead_code)]

use crate::diagnostic::Span;

pub(crate) type ExprId = u32;

#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub(crate) enum Item {
    Const(ConstDecl),
    Function(Function),
    Struct(StructDecl),
    Enum(EnumDecl),
    Extern(ExternBlock),
    Unsupported { feature: String, span: Span },
}

#[derive(Clone, Debug)]
pub(crate) struct Attribute {
    pub name: String,
    pub arguments: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct ConstDecl {
    pub name: String,
    pub ty: TypeRef,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub attributes: Vec<Attribute>,
    pub public: bool,
    pub export_c: bool,
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeRef,
    pub effects: Vec<EffectRef>,
    pub body: Option<Block>,
    pub span: Span,
    pub is_extern: bool,
    pub header: Option<String>,
}

impl Function {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) enum EffectRef {
    Alloc(Span),
    Free(Span),
    Raise(TypeRef, Span),
}

#[derive(Clone, Debug)]
pub(crate) struct StructDecl {
    pub attributes: Vec<Attribute>,
    pub public: bool,
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
    pub is_extern: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldDecl {
    pub public: bool,
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumDecl {
    pub attributes: Vec<Attribute>,
    pub public: bool,
    pub name: String,
    pub variants: Vec<VariantDecl>,
    pub span: Span,
    pub is_extern: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VariantDecl {
    pub attributes: Vec<Attribute>,
    pub public: bool,
    pub name: String,
    pub payload: Vec<TypeRef>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternBlock {
    pub header: String,
    pub functions: Vec<Function>,
    pub opaque_types: Vec<OpaqueType>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct OpaqueType {
    pub attributes: Vec<Attribute>,
    pub name: String,
    pub span: Span,
    pub header: String,
}

impl OpaqueType {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TypeRef {
    Named {
        path: Vec<String>,
        span: Span,
    },
    Pointer {
        nullable: bool,
        mutable: bool,
        pointee: Box<TypeRef>,
        span: Span,
    },
    Unsupported {
        feature: String,
        span: Span,
    },
}

impl TypeRef {
    pub const fn span(&self) -> Span {
        match self {
            Self::Named { span, .. }
            | Self::Pointer { span, .. }
            | Self::Unsupported { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub statements: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) enum Stmt {
    Let {
        mutable: bool,
        name: String,
        ty: Option<TypeRef>,
        value: Expr,
        span: Span,
    },
    Const(ConstDecl),
    Expr(Expr),
    Defer(Block, Span),
}

#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub id: ExprId,
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Literal(Literal),
    Path(Vec<String>),
    Block(Block),
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_expr: Option<Box<Expr>>,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Try {
        body: Block,
        arms: Vec<MatchArm>,
    },
    While {
        condition: Box<Expr>,
        body: Block,
    },
    For {
        initializer: Option<Box<ForInitializer>>,
        condition: Option<Box<Expr>>,
        update: Option<Box<Expr>>,
        body: Block,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    TypeApply {
        path: Vec<String>,
        arguments: Vec<TypeRef>,
    },
    Field {
        base: Box<Expr>,
        name: String,
        pointer: bool,
    },
    Propagate(Box<Expr>),
    Unary {
        operator: UnaryOp,
        value: Box<Expr>,
    },
    Binary {
        operator: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Cast {
        value: Box<Expr>,
        ty: TypeRef,
    },
    Assign {
        operator: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    StructLiteral {
        path: Vec<String>,
        fields: Vec<FieldInitializer>,
    },
    Raise(Box<Expr>),
    Return(Option<Box<Expr>>),
    Break,
    Continue,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) enum ForInitializer {
    Let {
        mutable: bool,
        name: String,
        ty: Option<TypeRef>,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub(crate) struct FieldInitializer {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct MatchArm {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) enum Pattern {
    Wildcard(Span),
    Bind(String, Span),
    Literal(Literal, Span),
    Path(Vec<String>, Span),
    Variant(Vec<String>, Vec<Pattern>, Span),
    Struct(Vec<String>, Vec<FieldPattern>, Span),
    Null(Span),
    NonNull(String, Span),
    Error(Span),
}

impl Pattern {
    pub const fn span(&self) -> Span {
        match self {
            Self::Wildcard(span)
            | Self::Bind(_, span)
            | Self::Literal(_, span)
            | Self::Path(_, span)
            | Self::Variant(_, _, span)
            | Self::Struct(_, _, span)
            | Self::Null(span)
            | Self::NonNull(_, span)
            | Self::Error(span) => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FieldPattern {
    pub name: String,
    pub pattern: Option<Pattern>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Int(String),
    Float(String),
    Bool(bool),
    Null,
    String(String),
    CString(String),
    Char(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnaryOp {
    Not,
    BitNot,
    Plus,
    Negate,
    Address,
    Dereference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BinaryOp {
    LogicalOr,
    LogicalAnd,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    Add,
    Subtract,
    WrapAdd,
    WrapSubtract,
    SaturatingAdd,
    SaturatingSubtract,
    Multiply,
    Divide,
    Remainder,
    WrapMultiply,
    SaturatingMultiply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssignOp {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    WrapAdd,
    WrapSubtract,
    WrapMultiply,
    SaturatingAdd,
    SaturatingSubtract,
    SaturatingMultiply,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_helpers_cover_every_variant_and_attribute_lookup() {
        let span = Span::new(3, 8);
        let patterns = [
            Pattern::Wildcard(span),
            Pattern::Bind("value".to_owned(), span),
            Pattern::Literal(Literal::Bool(true), span),
            Pattern::Path(vec!["E".to_owned(), "V".to_owned()], span),
            Pattern::Variant(Vec::new(), Vec::new(), span),
            Pattern::Struct(Vec::new(), Vec::new(), span),
            Pattern::Null(span),
            Pattern::NonNull("pointer".to_owned(), span),
            Pattern::Error(span),
        ];
        assert!(patterns.iter().all(|pattern| pattern.span() == span));

        let named = TypeRef::Named {
            path: vec!["i32".to_owned()],
            span,
        };
        let pointer = TypeRef::Pointer {
            nullable: false,
            mutable: false,
            pointee: Box::new(named.clone()),
            span,
        };
        let unsupported = TypeRef::Unsupported {
            feature: "test".to_owned(),
            span,
        };
        assert_eq!(named.span(), span);
        assert_eq!(pointer.span(), span);
        assert_eq!(unsupported.span(), span);

        let opaque = OpaqueType {
            attributes: vec![Attribute {
                name: "c_typedef".to_owned(),
                arguments: vec!["\"opaque_t\"".to_owned()],
                span,
            }],
            name: "Opaque".to_owned(),
            span,
            header: "<opaque.h>".to_owned(),
        };
        assert!(opaque.attribute("c_typedef").is_some());
        assert!(opaque.attribute("missing").is_none());
    }
}
