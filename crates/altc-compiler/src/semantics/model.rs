use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::ast::ExprId;
use crate::diagnostic::Span;

#[derive(Clone, Debug, Default)]
pub struct AnalyzeOptions {
    /// Reserved for future target-specific analysis. v0 always uses the host ABI.
    pub target: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "error_type", rename_all = "kebab-case")]
pub enum Effect {
    Alloc,
    Free,
    Raise(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryCategory {
    IndirectRead,
    IndirectWrite,
    GlobalRead,
    GlobalWrite,
    VolatileRead,
    VolatileWrite,
    ExternCall,
    IndirectCall,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryEvent {
    pub category: MemoryCategory,
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryVia {
    pub category: MemoryCategory,
    pub callee: String,
    pub file: String,
    pub call_span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemorySummary {
    pub function: String,
    pub effects: BTreeSet<Effect>,
    pub categories: BTreeSet<MemoryCategory>,
    pub direct: Vec<MemoryEvent>,
    pub transitive: Vec<MemoryVia>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Type {
    Unit,
    Bool,
    Int(IntType),
    Float(FloatType),
    Char,
    Str,
    Pointer {
        nullable: bool,
        mutable: bool,
        pointee: Box<Type>,
    },
    Array {
        element: Box<Type>,
        length: usize,
    },
    Slice {
        mutable: bool,
        element: Box<Type>,
    },
    FunctionPointer {
        extern_c: bool,
        parameters: Vec<Type>,
        return_type: Box<Type>,
        effects: BTreeSet<Effect>,
        error_type: Option<Box<Type>>,
    },
    Struct(String),
    Enum(String),
    Opaque(String),
    Never,
    Unknown,
}

impl Type {
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Unit => "unit".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Int(kind) => kind.name().to_owned(),
            Self::Float(FloatType::F32) => "f32".to_owned(),
            Self::Float(FloatType::F64) => "f64".to_owned(),
            Self::Float(FloatType::CFloat) => "c_float".to_owned(),
            Self::Float(FloatType::CDouble) => "c_double".to_owned(),
            Self::Float(FloatType::CLongDouble) => "c_longdouble".to_owned(),
            Self::Char => "char".to_owned(),
            Self::Str => "str".to_owned(),
            Self::Pointer {
                nullable,
                mutable,
                pointee,
            } => format!(
                "{}*{} {}",
                if *nullable { "?" } else { "" },
                if *mutable { "mut" } else { "const" },
                pointee.display()
            ),
            Self::Array { element, length } => format!("[{}; {length}]", element.display()),
            Self::Slice { mutable, element } => format!(
                "[]{} {}",
                if *mutable { "mut" } else { "const" },
                element.display()
            ),
            Self::FunctionPointer {
                extern_c,
                parameters,
                return_type,
                effects,
                ..
            } => {
                let parameters = parameters
                    .iter()
                    .map(Type::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                let effects = effects
                    .iter()
                    .map(|effect| match effect {
                        Effect::Alloc => "alloc".to_owned(),
                        Effect::Free => "free".to_owned(),
                        Effect::Raise(error) => format!("raise[{error}]"),
                    })
                    .collect::<Vec<_>>();
                format!(
                    "{}fn({parameters}) -> {}{}",
                    if *extern_c { "extern \"C\" " } else { "" },
                    return_type.display(),
                    if effects.is_empty() {
                        String::new()
                    } else {
                        format!(" with {{ {} }}", effects.join(", "))
                    }
                )
            }
            Self::Struct(name) | Self::Enum(name) | Self::Opaque(name) => name.clone(),
            Self::Never => "<never>".to_owned(),
            Self::Unknown => "<unknown>".to_owned(),
        }
    }

    pub(crate) const fn is_numeric(&self) -> bool {
        matches!(self, Self::Int(_) | Self::Float(_))
    }

    pub(crate) const fn is_integer(&self) -> bool {
        matches!(self, Self::Int(_))
    }

    pub(crate) const fn is_storable(&self) -> bool {
        !matches!(self, Self::Unit | Self::Never | Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum IntType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Isize,
    Usize,
    CChar,
    CSChar,
    CUChar,
    CShort,
    CUShort,
    CInt,
    CUInt,
    CLong,
    CULong,
    CLongLong,
    CULongLong,
}

impl IntType {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::Isize => "isize",
            Self::Usize => "usize",
            Self::CChar => "c_char",
            Self::CSChar => "c_schar",
            Self::CUChar => "c_uchar",
            Self::CShort => "c_short",
            Self::CUShort => "c_ushort",
            Self::CInt => "c_int",
            Self::CUInt => "c_uint",
            Self::CLong => "c_long",
            Self::CULong => "c_ulong",
            Self::CLongLong => "c_longlong",
            Self::CULongLong => "c_ulonglong",
        }
    }

    pub(crate) fn is_unsigned(self) -> bool {
        matches!(
            self,
            Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::Usize
                | Self::CUChar
                | Self::CUShort
                | Self::CUInt
                | Self::CULong
                | Self::CULongLong
        ) || (self == Self::CChar && std::ffi::c_char::MIN == 0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FloatType {
    F32,
    F64,
    CFloat,
    CDouble,
    CLongDouble,
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionInfo {
    pub name: String,
    pub parameters: Vec<Type>,
    pub return_type: Type,
    pub effects: BTreeSet<Effect>,
    pub error_type: Option<Type>,
    pub is_extern: bool,
    pub export_c: bool,
    pub c_name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct StructInfo {
    pub fields: BTreeMap<String, Type>,
    pub field_public: BTreeMap<String, bool>,
    pub field_c_names: BTreeMap<String, String>,
    pub repr_c: bool,
    pub is_extern: bool,
    pub c_name: String,
    pub c_typedef: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumInfo {
    pub variants: BTreeMap<String, Vec<Type>>,
    pub variant_public: BTreeMap<String, bool>,
    pub variant_c_names: BTreeMap<String, String>,
    pub repr_c: bool,
    pub is_extern: bool,
    pub c_name: String,
    pub c_typedef: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct GlobalInfo {
    pub ty: Type,
    pub mutable: bool,
    pub c_name: String,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SemanticModel {
    pub expression_types: HashMap<ExprId, Type>,
    pub try_error_types: HashMap<ExprId, Type>,
    pub functions: BTreeMap<String, FunctionInfo>,
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub opaque_types: BTreeMap<String, String>,
    pub globals: BTreeMap<String, GlobalInfo>,
    pub const_types: BTreeMap<String, Type>,
    pub type_references: HashMap<Span, Type>,
}
