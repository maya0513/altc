//! Name resolution, type/effect checking, and memory-summary inference.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;

use crate::SourceFile;
use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::lex;
use crate::literal::{decode_quoted, parse_float, parse_integer};
use crate::parser::parse;
use crate::syntax::SyntaxNode;

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
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryVia {
    pub category: MemoryCategory,
    pub callee: String,
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

#[derive(Clone, Debug)]
pub struct Analysis {
    pub source: SourceFile,
    pub cst: SyntaxNode,
    pub diagnostics: Vec<Diagnostic>,
    pub memory_summaries: Vec<MemorySummary>,
    pub(crate) program: Program,
    pub(crate) model: SemanticModel,
}

impl Analysis {
    #[must_use]
    pub fn succeeded(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::Severity::Error)
    }

    #[must_use]
    pub fn entrypoint(&self) -> Option<&str> {
        self.program.items.iter().find_map(|item| match item {
            Item::Function(function) if function.attribute("entry").is_some() => {
                Some(function.name.as_str())
            }
            _ => None,
        })
    }

    #[must_use]
    pub fn has_extern_declarations(&self) -> bool {
        self.program.items.iter().any(|item| match item {
            Item::Extern(block) => !block.functions.is_empty() || !block.opaque_types.is_empty(),
            _ => false,
        })
    }
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
    pub repr_c: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumInfo {
    pub variants: BTreeMap<String, Vec<Type>>,
    pub repr_c: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SemanticModel {
    pub expression_types: HashMap<ExprId, Type>,
    pub try_error_types: HashMap<ExprId, Type>,
    pub functions: BTreeMap<String, FunctionInfo>,
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub opaque_types: BTreeMap<String, String>,
    pub const_types: BTreeMap<String, Type>,
}

#[must_use]
pub fn analyze(source: SourceFile, options: &AnalyzeOptions) -> Analysis {
    let lexed = lex(&source.path, &source.text);
    let parsed = parse(&source, &lexed);
    let mut diagnostics = lexed.diagnostics.clone();
    diagnostics.extend(parsed.diagnostics);
    if let Some(target) = &options.target {
        diagnostics.push(Diagnostic::error(
            "ALTC4001",
            format!("cross-compilation target `{target}` is not implemented in executable v0"),
            &source.path,
            &source.text,
            Span::new(0, 0),
        ));
    }

    let mut analyzer = Analyzer::new(&source, parsed.program.clone(), diagnostics);
    analyzer.validate_attributes();
    analyzer.collect_declarations();
    analyzer.validate_c_name_collisions();
    analyzer.check_program();
    let memory_summaries = analyzer.finish_summaries();

    Analysis {
        source: source.clone(),
        cst: lexed.cst,
        diagnostics: analyzer.diagnostics,
        memory_summaries,
        program: parsed.program,
        model: analyzer.model,
    }
}

struct Analyzer<'a> {
    source: &'a SourceFile,
    program: Program,
    diagnostics: Vec<Diagnostic>,
    model: SemanticModel,
    direct_memory: BTreeMap<String, Vec<MemoryEvent>>,
    calls: BTreeMap<String, Vec<(String, Span)>>,
    actual_effects: BTreeMap<String, BTreeSet<Effect>>,
}

impl<'a> Analyzer<'a> {
    fn new(source: &'a SourceFile, program: Program, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source,
            program,
            diagnostics,
            model: SemanticModel::default(),
            direct_memory: BTreeMap::new(),
            calls: BTreeMap::new(),
            actual_effects: BTreeMap::new(),
        }
    }

    fn validate_attributes(&mut self) {
        let items = self.program.items.clone();
        for item in items {
            match item {
                Item::Struct(declaration) => {
                    self.validate_attribute_list(&declaration.attributes, &["repr"], "struct");
                }
                Item::Enum(declaration) => {
                    self.validate_attribute_list(&declaration.attributes, &["repr"], "enum");
                    for variant in declaration.variants {
                        if let Some(attribute) = variant.attributes.first() {
                            self.error(
                                "ALTC4001",
                                "attributes on enum variants are not implemented in executable v0",
                                attribute.span,
                            );
                        }
                    }
                }
                Item::Function(function) => {
                    let allowed = if function.export_c {
                        &["entry", "c_name"][..]
                    } else {
                        &["entry"][..]
                    };
                    self.validate_attribute_list(&function.attributes, allowed, "function");
                }
                Item::Extern(block) => {
                    for function in block.functions {
                        self.validate_attribute_list(
                            &function.attributes,
                            &["c_name"],
                            "extern function",
                        );
                    }
                    for opaque in block.opaque_types {
                        self.validate_attribute_list(
                            &opaque.attributes,
                            &["c_typedef"],
                            "extern opaque type",
                        );
                        if opaque.attribute("c_typedef").is_none() && !is_c_identifier(&opaque.name)
                        {
                            self.error(
                                "ALTC3210",
                                "extern opaque type name is not a non-reserved C identifier; use `@c_typedef`",
                                opaque.span,
                            );
                        }
                    }
                }
                Item::Const(_) | Item::Unsupported { .. } => {}
            }
        }
    }

    fn validate_attribute_list(
        &mut self,
        attributes: &[Attribute],
        allowed: &[&str],
        declaration: &str,
    ) {
        let mut names = HashSet::new();
        for attribute in attributes {
            if !names.insert(attribute.name.as_str()) {
                self.error(
                    "ALTC3210",
                    format!("duplicate `@{}` attribute", attribute.name),
                    attribute.span,
                );
            }
            if !allowed.contains(&attribute.name.as_str()) {
                let code = if matches!(
                    attribute.name.as_str(),
                    "entry" | "repr" | "c_name" | "c_typedef"
                ) {
                    "ALTC3210"
                } else {
                    "ALTC4001"
                };
                self.error(
                    code,
                    format!(
                        "attribute `@{}` is not supported on {declaration}",
                        attribute.name
                    ),
                    attribute.span,
                );
                continue;
            }
            match attribute.name.as_str() {
                "entry" if !attribute.arguments.is_empty() => self.error(
                    "ALTC3210",
                    "`@entry` does not accept arguments",
                    attribute.span,
                ),
                "repr" if attribute.arguments.as_slice() != ["C"] => {
                    self.error("ALTC3210", "only `@repr(C)` is supported", attribute.span)
                }
                "c_name" | "c_typedef" => {
                    let value = attribute
                        .arguments
                        .first()
                        .filter(|_| attribute.arguments.len() == 1)
                        .and_then(|value| decode_quoted(value, "").ok());
                    if value.as_deref().is_none_or(|value| !is_c_identifier(value)) {
                        self.error(
                            "ALTC3210",
                            format!(
                                "`@{}` requires one non-reserved C identifier string",
                                attribute.name
                            ),
                            attribute.span,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_declarations(&mut self) {
        let items = self.program.items.clone();
        for item in &items {
            match item {
                Item::Struct(declaration) => {
                    self.insert_type_name(&declaration.name, declaration.span);
                    self.model.structs.insert(
                        declaration.name.clone(),
                        StructInfo {
                            fields: BTreeMap::new(),
                            repr_c: has_repr_c(&declaration.attributes),
                        },
                    );
                }
                Item::Enum(declaration) => {
                    self.insert_type_name(&declaration.name, declaration.span);
                    self.model.enums.insert(
                        declaration.name.clone(),
                        EnumInfo {
                            variants: BTreeMap::new(),
                            repr_c: has_repr_c(&declaration.attributes),
                        },
                    );
                }
                Item::Extern(block) => {
                    for opaque in &block.opaque_types {
                        self.insert_type_name(&opaque.name, opaque.span);
                        let spelling = opaque
                            .attribute("c_typedef")
                            .and_then(|attribute| attribute.arguments.first())
                            .map_or_else(
                                || opaque.name.clone(),
                                |value| {
                                    decode_quoted(value, "").unwrap_or_else(|_| opaque.name.clone())
                                },
                            );
                        self.model
                            .opaque_types
                            .insert(opaque.name.clone(), spelling);
                    }
                }
                _ => {}
            }
        }

        for item in &items {
            match item {
                Item::Struct(declaration) => self.collect_struct(declaration),
                Item::Enum(declaration) => self.collect_enum(declaration),
                Item::Const(declaration) => {
                    let ty = self.resolve_type(&declaration.ty);
                    if self
                        .model
                        .const_types
                        .insert(declaration.name.clone(), ty)
                        .is_some()
                    {
                        self.duplicate("constant", &declaration.name, declaration.span);
                    }
                }
                Item::Function(function) => self.collect_function(function),
                Item::Extern(block) => {
                    for function in &block.functions {
                        self.collect_function(function);
                    }
                }
                Item::Unsupported { .. } => {}
            }
        }
    }

    fn validate_c_name_collisions(&mut self) {
        let mut names = BTreeMap::<String, Span>::new();
        let functions: Vec<_> = self.model.functions.values().cloned().collect();
        for function in functions {
            if !function.is_extern && !function.export_c {
                continue;
            }
            if !is_c_identifier(&function.c_name) {
                self.error(
                    "ALTC3210",
                    format!(
                        "C symbol `{}` is not a non-reserved C identifier; use a valid `@c_name`",
                        function.c_name
                    ),
                    function.span,
                );
            }
            if function.c_name == "main" && function.export_c {
                self.error(
                    "ALTC3211",
                    "exported C symbol `main` is reserved for the `@entry` wrapper",
                    function.span,
                );
            }
            if let Some(previous) = names.insert(function.c_name.clone(), function.span) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "ALTC3211",
                        format!("duplicate C symbol `{}`", function.c_name),
                        &self.source.path,
                        &self.source.text,
                        function.span,
                    )
                    .with_label(previous, "first declaration uses this C symbol"),
                );
            }
        }
    }

    fn insert_type_name(&mut self, name: &str, span: Span) {
        if self.model.structs.contains_key(name)
            || self.model.enums.contains_key(name)
            || self.model.opaque_types.contains_key(name)
        {
            self.duplicate("type", name, span);
        }
    }

    fn collect_struct(&mut self, declaration: &StructDecl) {
        let mut fields = BTreeMap::new();
        for field in &declaration.fields {
            let ty = self.resolve_type(&field.ty);
            if !ty.is_storable() {
                self.error(
                    "ALTC3001",
                    format!("field `{}` must have a storable type", field.name),
                    field.span,
                );
            }
            if fields.insert(field.name.clone(), ty).is_some() {
                self.duplicate("field", &field.name, field.span);
            }
        }
        if let Some(info) = self.model.structs.get_mut(&declaration.name) {
            info.fields = fields;
        }
    }

    fn collect_enum(&mut self, declaration: &EnumDecl) {
        let mut variants = BTreeMap::new();
        for variant in &declaration.variants {
            let payload: Vec<_> = variant
                .payload
                .iter()
                .map(|ty| self.resolve_type(ty))
                .collect();
            if payload.iter().any(|ty| !ty.is_storable()) {
                self.error(
                    "ALTC3002",
                    format!("payload of `{}` must be storable", variant.name),
                    variant.span,
                );
            }
            if variants.insert(variant.name.clone(), payload).is_some() {
                self.duplicate("variant", &variant.name, variant.span);
            }
        }
        let repr_c = has_repr_c(&declaration.attributes);
        if repr_c && variants.values().any(|payload| !payload.is_empty()) {
            self.error(
                "ALTC3003",
                "`@repr(C)` cannot be used on a payload enum",
                declaration.span,
            );
        }
        if let Some(info) = self.model.enums.get_mut(&declaration.name) {
            info.variants = variants;
        }
    }

    fn collect_function(&mut self, function: &Function) {
        let parameters: Vec<_> = function
            .parameters
            .iter()
            .map(|parameter| self.resolve_type(&parameter.ty))
            .collect();
        let return_type = self.resolve_type(&function.return_type);
        let mut effects = BTreeSet::new();
        let mut error_type = None;
        for effect in &function.effects {
            let value = match effect {
                EffectRef::Alloc(_) => Effect::Alloc,
                EffectRef::Free(_) => Effect::Free,
                EffectRef::Raise(ty, span) => {
                    let ty = self.resolve_type(ty);
                    if error_type.is_some() {
                        self.error(
                            "ALTC3101",
                            "a function may declare at most one `raise[E]` effect",
                            *span,
                        );
                    }
                    error_type = Some(ty.clone());
                    Effect::Raise(ty.display())
                }
            };
            if !effects.insert(value) {
                self.error(
                    "ALTC3102",
                    "duplicate effect in function contract",
                    effect_span(effect),
                );
            }
        }
        let c_name = function
            .attribute("c_name")
            .and_then(|attribute| attribute.arguments.first())
            .map_or_else(
                || function.name.clone(),
                |value| decode_quoted(value, "").unwrap_or_else(|_| function.name.clone()),
            );
        let info = FunctionInfo {
            name: function.name.clone(),
            parameters,
            return_type,
            effects,
            error_type,
            is_extern: function.is_extern,
            export_c: function.export_c,
            c_name,
            span: function.span,
        };
        if self
            .model
            .functions
            .insert(function.name.clone(), info)
            .is_some()
        {
            self.duplicate("function", &function.name, function.span);
        }
    }

    fn check_program(&mut self) {
        self.validate_layouts();
        self.validate_entrypoints();
        self.validate_c_abi();
        let items = self.program.items.clone();
        for item in &items {
            match item {
                Item::Const(declaration) => {
                    let expected = self.resolve_type(&declaration.ty);
                    let mut checker = BodyChecker::new(self, None, Type::Unit);
                    checker.in_const = true;
                    let actual = checker.check_expr(&declaration.value, Some(&expected));
                    checker.expect_same(&expected, &actual, declaration.value.span);
                }
                Item::Function(function) if function.body.is_some() => {
                    self.check_function(function)
                }
                _ => {}
            }
        }
    }

    fn validate_layouts(&mut self) {
        let items = self.program.items.clone();
        for item in items {
            let (name, span, repr_c) = match item {
                Item::Struct(declaration) => (
                    declaration.name.clone(),
                    declaration.span,
                    self.model.structs[&declaration.name].repr_c,
                ),
                Item::Enum(declaration) => (
                    declaration.name.clone(),
                    declaration.span,
                    self.model.enums[&declaration.name].repr_c,
                ),
                _ => continue,
            };
            if self.layout_depends_on(&name, &name, &mut BTreeSet::new()) {
                self.error(
                    "ALTC3212",
                    format!("type `{name}` has an infinitely recursive by-value layout"),
                    span,
                );
            }
            if repr_c {
                let fields: Vec<Type> = if let Some(info) = self.model.structs.get(&name) {
                    info.fields.values().cloned().collect()
                } else {
                    self.model.enums[&name]
                        .variants
                        .values()
                        .flatten()
                        .cloned()
                        .collect()
                };
                for field in fields {
                    if !self.is_c_abi_safe(&field, false) {
                        self.error(
                            "ALTC3207",
                            format!(
                                "`@repr(C)` type `{name}` contains non-C-ABI-safe field type `{}`",
                                field.display()
                            ),
                            span,
                        );
                    }
                }
            }
        }
    }

    fn layout_depends_on(
        &self,
        current: &str,
        target: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(current.to_owned()) {
            return false;
        }
        let dependencies: Vec<String> = if let Some(info) = self.model.structs.get(current) {
            info.fields
                .values()
                .filter_map(by_value_named_type)
                .map(str::to_owned)
                .collect()
        } else if let Some(info) = self.model.enums.get(current) {
            info.variants
                .values()
                .flatten()
                .filter_map(by_value_named_type)
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        let cyclic = dependencies.iter().any(|dependency| {
            dependency == target || self.layout_depends_on(dependency, target, visiting)
        });
        visiting.remove(current);
        cyclic
    }

    fn validate_entrypoints(&mut self) {
        let functions: Vec<_> = self
            .program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) if function.attribute("entry").is_some() => {
                    Some(function.clone())
                }
                _ => None,
            })
            .collect();
        if functions.len() > 1 {
            for function in functions.iter().skip(1) {
                self.error(
                    "ALTC3201",
                    "an executable may have only one `@entry` function",
                    function.span,
                );
            }
        }
        for function in functions {
            let info = self.model.functions[&function.name].clone();
            if !function.parameters.is_empty()
                || info.return_type != Type::Int(IntType::CInt)
                || function.public
                || function.export_c
                || info.error_type.is_some()
            {
                self.error(
                    "ALTC3202",
                    "`@entry` must be private, non-exported, parameterless, return `c_int`, and not raise",
                    function.span,
                );
            }
        }
    }

    fn validate_c_abi(&mut self) {
        let functions: Vec<_> = self.model.functions.values().cloned().collect();
        for function in functions {
            if !function.export_c && !function.is_extern {
                continue;
            }
            for ty in &function.parameters {
                if !self.is_c_abi_safe(ty, false) {
                    self.error(
                        "ALTC3203",
                        format!(
                            "parameter type `{}` of C function `{}` is not C ABI-safe",
                            ty.display(),
                            function.name
                        ),
                        function.span,
                    );
                }
            }
            if !self.is_c_abi_safe(&function.return_type, true) {
                self.error(
                    "ALTC3204",
                    format!(
                        "return type `{}` of C function `{}` is not C ABI-safe",
                        function.return_type.display(),
                        function.name
                    ),
                    function.span,
                );
            }
            if let Some(error) = &function.error_type {
                if function.is_extern {
                    self.error(
                        "ALTC3205",
                        "raising extern functions are not implemented in executable v0",
                        function.span,
                    );
                } else if !self.is_c_abi_safe(error, false) {
                    self.error(
                        "ALTC3206",
                        format!(
                            "error type `{}` of exported raising function is not C ABI-safe",
                            error.display()
                        ),
                        function.span,
                    );
                }
            }
        }
    }

    fn is_c_abi_safe(&self, ty: &Type, allow_unit: bool) -> bool {
        match ty {
            Type::Unit => allow_unit,
            Type::Bool | Type::Int(_) | Type::Float(_) | Type::Char => true,
            Type::Pointer { pointee, .. } => match pointee.as_ref() {
                Type::Unit
                | Type::Bool
                | Type::Int(_)
                | Type::Float(_)
                | Type::Char
                | Type::Opaque(_) => true,
                Type::Struct(name) => self.model.structs.get(name).is_some_and(|info| info.repr_c),
                Type::Enum(name) => self.model.enums.get(name).is_some_and(|info| info.repr_c),
                _ => false,
            },
            Type::Struct(name) => self.model.structs.get(name).is_some_and(|info| info.repr_c),
            Type::Enum(name) => self.model.enums.get(name).is_some_and(|info| info.repr_c),
            Type::Str | Type::Opaque(_) | Type::Never | Type::Unknown => false,
        }
    }

    fn check_function(&mut self, function: &Function) {
        let info = self.model.functions[&function.name].clone();
        let mut checker =
            BodyChecker::new(self, Some(function.name.clone()), info.return_type.clone());
        checker.push_scope();
        let mut parameter_names = HashSet::new();
        for (parameter, ty) in function.parameters.iter().zip(info.parameters.iter()) {
            if !parameter_names.insert(parameter.name.clone()) {
                checker
                    .analyzer
                    .duplicate("parameter", &parameter.name, parameter.span);
            }
            checker.bind(parameter.name.clone(), ty.clone(), false, parameter.span);
        }
        let actual = checker.check_block(
            function.body.as_ref().expect("checked body"),
            Some(&info.return_type),
        );
        checker.expect_same(
            &info.return_type,
            &actual,
            function.body.as_ref().unwrap().span,
        );
        checker.pop_scope();
        let actual_effects = checker.actual_effects.clone();
        drop(checker);

        for effect in &actual_effects {
            if !info.effects.contains(effect) {
                self.error(
                    "ALTC3103",
                    format!(
                        "function `{}` performs undeclared effect `{}`",
                        function.name,
                        effect_name(effect)
                    ),
                    function.span,
                );
            }
        }
        self.actual_effects
            .insert(function.name.clone(), actual_effects);
    }

    fn resolve_type(&mut self, reference: &TypeRef) -> Type {
        match reference {
            TypeRef::Named { path, span } => {
                if path.len() != 1 {
                    self.error(
                        "ALTC3004",
                        "module-qualified types require multi-module support",
                        *span,
                    );
                    return Type::Unknown;
                }
                let name = &path[0];
                primitive_type(name)
                    .or_else(|| {
                        self.model
                            .structs
                            .contains_key(name)
                            .then(|| Type::Struct(name.clone()))
                    })
                    .or_else(|| {
                        self.model
                            .enums
                            .contains_key(name)
                            .then(|| Type::Enum(name.clone()))
                    })
                    .or_else(|| self.model.opaque_types.get(name).cloned().map(Type::Opaque))
                    .unwrap_or_else(|| {
                        self.error("ALTC3005", format!("unknown type `{name}`"), *span);
                        Type::Unknown
                    })
            }
            TypeRef::Pointer {
                nullable,
                mutable,
                pointee,
                ..
            } => Type::Pointer {
                nullable: *nullable,
                mutable: *mutable,
                pointee: Box::new(self.resolve_type(pointee)),
            },
            TypeRef::Unsupported { .. } => Type::Unknown,
        }
    }

    fn finish_summaries(&self) -> Vec<MemorySummary> {
        let mut categories: BTreeMap<String, BTreeSet<MemoryCategory>> = BTreeMap::new();
        for name in self.model.functions.keys() {
            let direct = self.direct_memory.get(name).cloned().unwrap_or_default();
            categories.insert(
                name.clone(),
                direct.iter().map(|event| event.category).collect(),
            );
        }
        loop {
            let old = categories.clone();
            for (caller, calls) in &self.calls {
                let entry = categories.entry(caller.clone()).or_default();
                for (callee, _) in calls {
                    if let Some(callee_categories) = old.get(callee) {
                        entry.extend(callee_categories.iter().copied());
                    }
                }
            }
            if categories == old {
                break;
            }
        }

        self.model
            .functions
            .iter()
            .filter(|(_, info)| !info.is_extern)
            .map(|(name, info)| {
                let direct = self.direct_memory.get(name).cloned().unwrap_or_default();
                let direct_categories: BTreeSet<_> =
                    direct.iter().map(|event| event.category).collect();
                let mut transitive = Vec::new();
                for (callee, call_span) in self.calls.get(name).into_iter().flatten() {
                    for category in categories.get(callee).into_iter().flatten() {
                        if !direct_categories.contains(category) {
                            transitive.push(MemoryVia {
                                category: *category,
                                callee: callee.clone(),
                                call_span: *call_span,
                            });
                        }
                    }
                }
                transitive
                    .sort_by_key(|via| (via.category, via.callee.clone(), via.call_span.start));
                transitive.dedup_by(|left, right| {
                    left.category == right.category && left.callee == right.callee
                });
                MemorySummary {
                    function: name.clone(),
                    effects: info.effects.clone(),
                    categories: categories.get(name).cloned().unwrap_or_default(),
                    direct,
                    transitive,
                }
            })
            .collect()
    }

    fn duplicate(&mut self, kind: &str, name: &str, span: Span) {
        self.error(
            "ALTC3006",
            format!("duplicate {kind} declaration `{name}`"),
            span,
        );
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::error(
            code,
            message,
            &self.source.path,
            &self.source.text,
            span,
        ));
    }
}

#[derive(Clone)]
struct Binding {
    ty: Type,
    mutable: bool,
}

struct BodyChecker<'a, 'b> {
    analyzer: &'a mut Analyzer<'b>,
    function: Option<String>,
    return_type: Type,
    scopes: Vec<HashMap<String, Binding>>,
    loop_depth: usize,
    in_defer: bool,
    in_const: bool,
    try_errors: Vec<Option<Type>>,
    actual_effects: BTreeSet<Effect>,
}

impl<'a, 'b> BodyChecker<'a, 'b> {
    fn new(analyzer: &'a mut Analyzer<'b>, function: Option<String>, return_type: Type) -> Self {
        Self {
            analyzer,
            function,
            return_type,
            scopes: Vec::new(),
            loop_depth: 0,
            in_defer: false,
            in_const: false,
            try_errors: Vec::new(),
            actual_effects: BTreeSet::new(),
        }
    }

    fn check_block(&mut self, block: &Block, expected: Option<&Type>) -> Type {
        self.push_scope();
        let mut last_statement_type = Type::Unit;
        for statement in &block.statements {
            last_statement_type = self.check_statement(statement);
        }
        let ty = block
            .tail
            .as_ref()
            .map_or(last_statement_type, |expression| {
                self.check_expr(expression, expected)
            });
        self.pop_scope();
        ty
    }

    fn check_statement(&mut self, statement: &Stmt) -> Type {
        match statement {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
                span,
            } => {
                let expected = ty.as_ref().map(|ty| self.analyzer.resolve_type(ty));
                let actual = self.check_expr(value, expected.as_ref());
                let binding_type = expected.unwrap_or_else(|| actual.clone());
                self.expect_same(&binding_type, &actual, value.span);
                self.bind(name.clone(), binding_type, *mutable, *span);
                Type::Unit
            }
            Stmt::Const(declaration) => {
                let expected = self.analyzer.resolve_type(&declaration.ty);
                let old = self.in_const;
                self.in_const = true;
                let actual = self.check_expr(&declaration.value, Some(&expected));
                self.in_const = old;
                self.expect_same(&expected, &actual, declaration.value.span);
                self.bind(declaration.name.clone(), expected, false, declaration.span);
                Type::Unit
            }
            Stmt::Expr(expression) => {
                let ty = self.check_expr(expression, None);
                if ty == Type::Never {
                    Type::Never
                } else {
                    Type::Unit
                }
            }
            Stmt::Defer(block, span) => {
                let old = self.in_defer;
                self.in_defer = true;
                let ty = self.check_block(block, Some(&Type::Unit));
                self.expect_same(&Type::Unit, &ty, block.span);
                self.in_defer = old;
                let _ = span;
                Type::Unit
            }
        }
    }

    fn check_expr(&mut self, expression: &Expr, expected: Option<&Type>) -> Type {
        let ty = match &expression.kind {
            ExprKind::Literal(literal) => self.check_literal(literal, expected, expression.span),
            ExprKind::Path(path) => self.check_path(path, expression.span),
            ExprKind::Block(block) => self.check_block(block, expected),
            ExprKind::If {
                condition,
                then_block,
                else_expr,
            } => {
                let condition_type = self.check_expr(condition, Some(&Type::Bool));
                self.expect_same(&Type::Bool, &condition_type, condition.span);
                let then_type = self.check_block(then_block, expected);
                if let Some(else_expr) = else_expr {
                    let else_type = self.check_expr(else_expr, expected.or(Some(&then_type)));
                    self.unify(&then_type, &else_type, expression.span)
                } else {
                    self.expect_same(&Type::Unit, &then_type, then_block.span);
                    Type::Unit
                }
            }
            ExprKind::Match { value, arms } => {
                let value_type = self.check_expr(value, None);
                self.check_arms(&value_type, arms, expected, expression.span)
            }
            ExprKind::Try { body, arms } => {
                self.try_errors.push(None);
                let body_type = self.check_block(body, expected);
                let error_type = self.try_errors.pop().flatten().unwrap_or_else(|| {
                    self.analyzer.error(
                        "ALTC3104",
                        "a `try` block must contain a raising path",
                        body.span,
                    );
                    Type::Unknown
                });
                self.analyzer
                    .model
                    .try_error_types
                    .insert(expression.id, error_type.clone());
                let arm_type = self.check_arms(
                    &error_type,
                    arms,
                    expected.or(Some(&body_type)),
                    expression.span,
                );
                self.unify(&body_type, &arm_type, expression.span)
            }
            ExprKind::While { condition, body } => {
                let condition_type = self.check_expr(condition, Some(&Type::Bool));
                self.expect_same(&Type::Bool, &condition_type, condition.span);
                self.loop_depth += 1;
                let body_type = self.check_block(body, Some(&Type::Unit));
                self.loop_depth -= 1;
                self.expect_same(&Type::Unit, &body_type, body.span);
                Type::Unit
            }
            ExprKind::For {
                initializer,
                condition,
                update,
                body,
            } => {
                self.push_scope();
                if let Some(initializer) = initializer {
                    match initializer.as_ref() {
                        ForInitializer::Let {
                            mutable,
                            name,
                            ty,
                            value,
                            span,
                        } => {
                            let expected = ty.as_ref().map(|ty| self.analyzer.resolve_type(ty));
                            let actual = self.check_expr(value, expected.as_ref());
                            let ty = expected.unwrap_or_else(|| actual.clone());
                            self.expect_same(&ty, &actual, value.span);
                            self.bind(name.clone(), ty, *mutable, *span);
                        }
                        ForInitializer::Expr(expression) => {
                            self.check_expr(expression, None);
                        }
                    }
                }
                if let Some(condition) = condition {
                    let ty = self.check_expr(condition, Some(&Type::Bool));
                    self.expect_same(&Type::Bool, &ty, condition.span);
                }
                self.loop_depth += 1;
                let body_type = self.check_block(body, Some(&Type::Unit));
                if let Some(update) = update {
                    self.check_expr(update, None);
                }
                self.loop_depth -= 1;
                self.pop_scope();
                self.expect_same(&Type::Unit, &body_type, body.span);
                Type::Unit
            }
            ExprKind::Call { callee, arguments } => {
                self.check_call(callee, arguments, false, expression.span)
            }
            ExprKind::TypeApply { path, arguments } => {
                self.check_type_apply(path, arguments, expression.span)
            }
            ExprKind::Field {
                base,
                name,
                pointer,
            } => {
                self.check_field(base, name, *pointer, false, expression.span)
                    .0
            }
            ExprKind::Propagate(value) => {
                if self.in_defer {
                    self.analyzer.error(
                        "ALTC3302",
                        "`?` cannot be used inside defer",
                        expression.span,
                    );
                }
                if let ExprKind::Call { callee, arguments } = &value.kind {
                    self.check_call(callee, arguments, true, expression.span)
                } else {
                    self.analyzer.error(
                        "ALTC3105",
                        "postfix `?` can only be applied to a raising call",
                        expression.span,
                    );
                    Type::Unknown
                }
            }
            ExprKind::Unary { operator, value } => {
                self.check_unary(*operator, value, expected, expression.span)
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => self.check_binary(*operator, left, right, expected, expression.span),
            ExprKind::Cast { value, ty } => {
                let source = self.check_expr(value, None);
                let target = self.analyzer.resolve_type(ty);
                if !(source.is_numeric() && target.is_numeric()
                    || source == Type::Char && target == Type::Int(IntType::U32))
                {
                    self.analyzer.error(
                        "ALTC3007",
                        format!(
                            "cannot cast `{}` to `{}`",
                            source.display(),
                            target.display()
                        ),
                        expression.span,
                    );
                }
                target
            }
            ExprKind::Assign {
                operator,
                target,
                value,
            } => {
                let (target_type, mutable) = self.check_place(target, true);
                self.analyzer
                    .model
                    .expression_types
                    .insert(target.id, target_type.clone());
                if !mutable {
                    self.analyzer.error(
                        "ALTC3008",
                        "assignment target is not mutable",
                        target.span,
                    );
                }
                let value_type = self.check_expr(value, Some(&target_type));
                self.expect_same(&target_type, &value_type, value.span);
                let integer_only = matches!(
                    operator,
                    AssignOp::WrapAdd
                        | AssignOp::WrapSubtract
                        | AssignOp::WrapMultiply
                        | AssignOp::SaturatingAdd
                        | AssignOp::SaturatingSubtract
                        | AssignOp::SaturatingMultiply
                );
                if (*operator != AssignOp::Assign && !target_type.is_numeric())
                    || (integer_only && !target_type.is_integer())
                {
                    self.analyzer.error(
                        "ALTC3009",
                        "compound assignment operator is not defined for this type",
                        expression.span,
                    );
                }
                Type::Unit
            }
            ExprKind::StructLiteral { path, fields } => {
                self.check_struct_literal(path, fields, expression.span)
            }
            ExprKind::Raise(value) => {
                if self.in_defer {
                    self.analyzer.error(
                        "ALTC3303",
                        "`raise` cannot be used inside defer",
                        expression.span,
                    );
                }
                let error_type = self.check_expr(value, None);
                self.route_error(error_type, expression.span);
                Type::Never
            }
            ExprKind::Return(value) => {
                if self.in_defer {
                    self.analyzer.error(
                        "ALTC3304",
                        "`return` cannot be used inside defer",
                        expression.span,
                    );
                }
                let actual = value.as_ref().map_or(Type::Unit, |value| {
                    self.check_expr(value, Some(&self.return_type.clone()))
                });
                self.expect_same(&self.return_type.clone(), &actual, expression.span);
                Type::Never
            }
            ExprKind::Break | ExprKind::Continue => {
                if self.in_defer {
                    self.analyzer.error(
                        "ALTC3305",
                        "loop control cannot be used inside defer",
                        expression.span,
                    );
                }
                if self.loop_depth == 0 {
                    self.analyzer.error(
                        "ALTC3306",
                        "loop control expression used outside a loop",
                        expression.span,
                    );
                }
                Type::Never
            }
            ExprKind::Error => Type::Unknown,
        };
        self.analyzer
            .model
            .expression_types
            .insert(expression.id, ty.clone());
        ty
    }

    fn check_literal(&mut self, literal: &Literal, expected: Option<&Type>, span: Span) -> Type {
        match literal {
            Literal::Int(raw) => self.check_integer_literal(raw, expected, span, false),
            Literal::Float(raw) => {
                let ty = if raw.ends_with("f32") {
                    Type::Float(FloatType::F32)
                } else {
                    expected
                        .filter(|ty| matches!(ty, Type::Float(_)))
                        .cloned()
                        .unwrap_or(Type::Float(FloatType::F64))
                };
                match parse_float(raw) {
                    Ok(value)
                        if matches!(ty, Type::Float(FloatType::F32))
                            && !(value as f32).is_finite() =>
                    {
                        self.analyzer.error(
                            "ALTC1006",
                            "floating-point literal is out of range for `f32`",
                            span,
                        );
                    }
                    Err(message) => self.analyzer.error("ALTC1006", message, span),
                    _ => {}
                }
                ty
            }
            Literal::Bool(_) => Type::Bool,
            Literal::Null => {
                if let Some(Type::Pointer { nullable: true, .. }) = expected {
                    expected.cloned().unwrap_or(Type::Unknown)
                } else {
                    self.analyzer.error(
                        "ALTC3010",
                        "`null` requires a nullable pointer context",
                        span,
                    );
                    Type::Unknown
                }
            }
            Literal::String(raw) => {
                if let Err(message) = decode_quoted(raw, "") {
                    self.analyzer.error("ALTC1007", message, span);
                }
                Type::Str
            }
            Literal::CString(raw) => {
                match decode_quoted(raw, "c") {
                    Ok(value) if value.contains('\0') => self.analyzer.error(
                        "ALTC1008",
                        "C string literal cannot contain an embedded NUL",
                        span,
                    ),
                    Err(message) => self.analyzer.error("ALTC1007", message, span),
                    _ => {}
                }
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Int(IntType::CChar)),
                }
            }
            Literal::Char(raw) => {
                match decode_quoted(raw, "") {
                    Ok(value) if value.chars().count() != 1 => self.analyzer.error(
                        "ALTC1009",
                        "character literal must contain exactly one Unicode scalar value",
                        span,
                    ),
                    Err(message) => self.analyzer.error("ALTC1007", message, span),
                    _ => {}
                }
                Type::Char
            }
        }
    }

    fn check_integer_literal(
        &mut self,
        raw: &str,
        expected: Option<&Type>,
        span: Span,
        allow_signed_minimum: bool,
    ) -> Type {
        let ty = integer_literal_type(raw)
            .or_else(|| expected.filter(|ty| ty.is_integer()).cloned())
            .unwrap_or(Type::Int(IntType::I32));
        match parse_integer(raw) {
            Ok((value, _)) => {
                if let Type::Int(kind) = ty {
                    let (maximum, signed) = integer_maximum(kind);
                    let limit = maximum + u128::from(allow_signed_minimum && signed);
                    if value > limit {
                        self.analyzer.error(
                            "ALTC1005",
                            format!("integer literal is out of range for `{}`", kind.name()),
                            span,
                        );
                    }
                }
            }
            Err(message) => self.analyzer.error("ALTC1005", message, span),
        }
        ty
    }

    fn check_path(&mut self, path: &[String], span: Span) -> Type {
        if path.len() == 1 {
            if let Some(binding) = self.lookup(&path[0]) {
                return binding.ty.clone();
            }
            if let Some(ty) = self.analyzer.model.const_types.get(&path[0]) {
                return ty.clone();
            }
            if self.analyzer.model.functions.contains_key(&path[0]) {
                self.analyzer.error(
                    "ALTC3011",
                    "function values require function-pointer support; call the function directly",
                    span,
                );
                return Type::Unknown;
            }
        } else if let Some((enum_name, variant_name)) = split_variant(path) {
            if let Some(info) = self.analyzer.model.enums.get(enum_name) {
                if let Some(payload) = info.variants.get(variant_name) {
                    if payload.is_empty() {
                        return Type::Enum(enum_name.to_owned());
                    }
                    self.analyzer.error(
                        "ALTC3012",
                        format!(
                            "variant `{variant_name}` requires {} payload value(s)",
                            payload.len()
                        ),
                        span,
                    );
                    return Type::Enum(enum_name.to_owned());
                }
            }
        }
        self.analyzer.error(
            "ALTC3013",
            format!("unknown value `{}`", path.join("::")),
            span,
        );
        Type::Unknown
    }

    fn check_call(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
        propagate: bool,
        span: Span,
    ) -> Type {
        if self.in_const {
            self.analyzer.error(
                "ALTC3014",
                "function calls are not allowed in const expressions",
                span,
            );
        }
        let (path, type_arguments): (&[String], &[TypeRef]) = match &callee.kind {
            ExprKind::Path(path) => (path, &[]),
            ExprKind::TypeApply { path, arguments } => (path, arguments),
            _ => {
                self.analyzer.error(
                    "ALTC3015",
                    "indirect calls require function-pointer support",
                    callee.span,
                );
                for argument in arguments {
                    self.check_expr(argument, None);
                }
                return Type::Unknown;
            }
        };
        if path.len() == 1 {
            if let Some(result) = self.check_builtin(&path[0], type_arguments, arguments, span) {
                if propagate {
                    self.analyzer
                        .error("ALTC3105", "postfix `?` requires a raising call", span);
                }
                return result;
            }
            if let Some(info) = self.analyzer.model.functions.get(&path[0]).cloned() {
                if info.parameters.len() != arguments.len() {
                    self.analyzer.error(
                        "ALTC3016",
                        format!(
                            "function `{}` expects {} argument(s), found {}",
                            info.name,
                            info.parameters.len(),
                            arguments.len()
                        ),
                        span,
                    );
                }
                for (argument, expected) in arguments.iter().zip(&info.parameters) {
                    let actual = self.check_expr(argument, Some(expected));
                    self.expect_same(expected, &actual, argument.span);
                }
                for effect in &info.effects {
                    match effect {
                        Effect::Alloc | Effect::Free => {
                            self.actual_effects.insert(effect.clone());
                        }
                        Effect::Raise(_) => {}
                    }
                }
                if let Some(function) = self.function.clone() {
                    if info.is_extern {
                        self.memory(&function, MemoryCategory::ExternCall, span);
                    } else {
                        self.analyzer
                            .calls
                            .entry(function)
                            .or_default()
                            .push((info.name.clone(), span));
                    }
                }
                match (&info.error_type, propagate) {
                    (Some(error), true) => self.route_error(error.clone(), span),
                    (Some(_), false) => self.analyzer.error(
                        "ALTC3106",
                        format!("raising call to `{}` requires postfix `?`", info.name),
                        span,
                    ),
                    (None, true) => {
                        self.analyzer
                            .error("ALTC3105", "postfix `?` requires a raising call", span)
                    }
                    (None, false) => {}
                }
                return info.return_type;
            }
        }
        if let Some((enum_name, variant_name)) = split_variant(path) {
            if let Some(info) = self.analyzer.model.enums.get(enum_name).cloned() {
                if let Some(payload) = info.variants.get(variant_name) {
                    let payload = payload.clone();
                    if payload.len() != arguments.len() {
                        self.analyzer.error(
                            "ALTC3017",
                            format!(
                                "variant `{variant_name}` expects {} payload value(s)",
                                payload.len()
                            ),
                            span,
                        );
                    }
                    for (argument, expected) in arguments.iter().zip(&payload) {
                        let actual = self.check_expr(argument, Some(expected));
                        self.expect_same(expected, &actual, argument.span);
                    }
                    return Type::Enum(enum_name.to_owned());
                }
            }
        }
        self.analyzer.error(
            "ALTC3018",
            format!("unknown callable `{}`", path.join("::")),
            span,
        );
        Type::Unknown
    }

    fn check_type_apply(&mut self, path: &[String], arguments: &[TypeRef], span: Span) -> Type {
        if path.len() == 1 && matches!(path[0].as_str(), "sizeof" | "alignof") {
            if arguments.len() != 1 {
                self.analyzer.error(
                    "ALTC3048",
                    format!("`{}` expects exactly one type argument", path[0]),
                    span,
                );
                return Type::Unknown;
            }
            let ty = self.analyzer.resolve_type(&arguments[0]);
            if !ty.is_storable() {
                self.analyzer.error(
                    "ALTC3049",
                    format!("`{}` requires a concrete storable type", path[0]),
                    span,
                );
            }
            Type::Int(IntType::Usize)
        } else {
            self.analyzer
                .error("ALTC3050", "a type-applied builtin must be called", span);
            Type::Unknown
        }
    }

    fn check_builtin(
        &mut self,
        name: &str,
        type_arguments: &[TypeRef],
        arguments: &[Expr],
        span: Span,
    ) -> Option<Type> {
        match name {
            "ptr_as_const" => {
                if !type_arguments.is_empty() || arguments.len() != 1 {
                    self.analyzer.error(
                        "ALTC3019",
                        "`ptr_as_const` expects no type arguments and one value argument",
                        span,
                    );
                }
                let Some(argument) = arguments.first() else {
                    return Some(Type::Unknown);
                };
                let ty = self.check_expr(argument, None);
                match ty {
                    Type::Pointer {
                        nullable, pointee, ..
                    } => Some(Type::Pointer {
                        nullable,
                        mutable: false,
                        pointee,
                    }),
                    other => {
                        self.analyzer.error(
                            "ALTC3020",
                            format!(
                                "`ptr_as_const` requires a pointer, found `{}`",
                                other.display()
                            ),
                            span,
                        );
                        Some(Type::Unknown)
                    }
                }
            }
            "ptr_addr" => {
                if !type_arguments.is_empty() || arguments.len() != 1 {
                    self.analyzer.error(
                        "ALTC3019",
                        "`ptr_addr` expects no type arguments and one value argument",
                        span,
                    );
                }
                let argument_type = arguments
                    .first()
                    .map(|argument| self.check_expr(argument, None))
                    .unwrap_or(Type::Unknown);
                if !matches!(argument_type, Type::Pointer { .. }) {
                    self.analyzer
                        .error("ALTC3020", "`ptr_addr` requires a pointer", span);
                }
                Some(Type::Int(IntType::Usize))
            }
            "ptr_cast" | "ptr_cast_mut" => {
                if type_arguments.len() != 1 || arguments.len() != 1 {
                    self.analyzer.error(
                        "ALTC3019",
                        format!("`{name}` expects one type and one value argument"),
                        span,
                    );
                    return Some(Type::Unknown);
                }
                let pointee = self.analyzer.resolve_type(&type_arguments[0]);
                let source = self.check_expr(&arguments[0], None);
                match source {
                    Type::Pointer {
                        nullable, mutable, ..
                    } => Some(Type::Pointer {
                        nullable,
                        mutable: if name == "ptr_cast_mut" {
                            true
                        } else {
                            mutable
                        },
                        pointee: Box::new(pointee),
                    }),
                    _ => {
                        self.analyzer.error(
                            "ALTC3020",
                            format!("`{name}` requires a pointer"),
                            span,
                        );
                        Some(Type::Unknown)
                    }
                }
            }
            "offset" => {
                if type_arguments.len() != 1 || arguments.len() != 2 {
                    self.analyzer.error(
                        "ALTC3019",
                        "`offset` expects one type and two value arguments",
                        span,
                    );
                    return Some(Type::Unknown);
                }
                let pointee = self.analyzer.resolve_type(&type_arguments[0]);
                let pointer = self.check_expr(&arguments[0], None);
                let offset = self.check_expr(&arguments[1], Some(&Type::Int(IntType::Isize)));
                self.expect_same(&Type::Int(IntType::Isize), &offset, arguments[1].span);
                match pointer {
                    Type::Pointer {
                        nullable: false,
                        mutable,
                        pointee: source_pointee,
                    } => {
                        self.expect_same(&pointee, &source_pointee, arguments[0].span);
                        Some(Type::Pointer {
                            nullable: false,
                            mutable,
                            pointee: Box::new(pointee),
                        })
                    }
                    _ => {
                        self.analyzer.error(
                            "ALTC3020",
                            "`offset` requires a non-null pointer",
                            span,
                        );
                        Some(Type::Unknown)
                    }
                }
            }
            "ptr_from_addr" | "ptr_from_addr_mut" => {
                if type_arguments.len() != 1 || arguments.len() != 1 {
                    self.analyzer.error(
                        "ALTC3019",
                        format!("`{name}` expects one type and one value argument"),
                        span,
                    );
                    return Some(Type::Unknown);
                }
                let pointee = self.analyzer.resolve_type(&type_arguments[0]);
                let address = self.check_expr(&arguments[0], Some(&Type::Int(IntType::Usize)));
                self.expect_same(&Type::Int(IntType::Usize), &address, arguments[0].span);
                Some(Type::Pointer {
                    nullable: true,
                    mutable: name == "ptr_from_addr_mut",
                    pointee: Box::new(pointee),
                })
            }
            _ => None,
        }
    }

    fn check_unary(
        &mut self,
        operator: UnaryOp,
        value: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        match operator {
            UnaryOp::Address => {
                let (ty, mutable) = self.check_place(value, false);
                Type::Pointer {
                    nullable: false,
                    mutable,
                    pointee: Box::new(ty),
                }
            }
            UnaryOp::Dereference => {
                let ty = self.check_expr(value, None);
                match ty {
                    Type::Pointer {
                        nullable: false,
                        pointee,
                        ..
                    } => {
                        self.memory_current(MemoryCategory::IndirectRead, span);
                        *pointee
                    }
                    Type::Pointer { nullable: true, .. } => {
                        self.analyzer.error(
                            "ALTC3021",
                            "nullable pointers must be refined with `nonnull(...)` before dereference",
                            span,
                        );
                        Type::Unknown
                    }
                    other => {
                        self.analyzer.error(
                            "ALTC3022",
                            format!("cannot dereference `{}`", other.display()),
                            span,
                        );
                        Type::Unknown
                    }
                }
            }
            UnaryOp::Not => {
                let ty = self.check_expr(value, Some(&Type::Bool));
                self.expect_same(&Type::Bool, &ty, value.span);
                Type::Bool
            }
            UnaryOp::BitNot => {
                let ty = self.check_expr(value, expected);
                if !ty.is_integer() {
                    self.analyzer
                        .error("ALTC3023", "bitwise not requires an integer", span);
                }
                ty
            }
            UnaryOp::Plus | UnaryOp::Negate => {
                let ty = if operator == UnaryOp::Negate {
                    if let ExprKind::Literal(Literal::Int(raw)) = &value.kind {
                        let ty = self.check_integer_literal(raw, expected, value.span, true);
                        self.analyzer
                            .model
                            .expression_types
                            .insert(value.id, ty.clone());
                        ty
                    } else {
                        self.check_expr(value, expected)
                    }
                } else {
                    self.check_expr(value, expected)
                };
                if !ty.is_numeric() {
                    self.analyzer.error(
                        "ALTC3024",
                        "unary numeric operator requires a number",
                        span,
                    );
                }
                ty
            }
        }
    }

    fn check_binary(
        &mut self,
        operator: BinaryOp,
        left: &Expr,
        right: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        if matches!(operator, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
            let left_type = self.check_expr(left, Some(&Type::Bool));
            let right_type = self.check_expr(right, Some(&Type::Bool));
            self.expect_same(&Type::Bool, &left_type, left.span);
            self.expect_same(&Type::Bool, &right_type, right.span);
            return Type::Bool;
        }
        if matches!(operator, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
            let left_type = self.check_expr(left, expected);
            let right_type = self.check_expr(right, Some(&Type::Int(IntType::U32)));
            if !left_type.is_integer()
                || !matches!(right_type, Type::Int(kind) if kind.is_unsigned())
            {
                self.analyzer.error(
                    "ALTC3026",
                    "shift requires an integer left operand and an unsigned integer right operand",
                    span,
                );
            }
            return left_type;
        }
        let operand_expected = if matches!(
            operator,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            None
        } else {
            expected
        };
        let (left_type, right_type) =
            if operand_expected.is_none() && expression_needs_context(left) {
                let right_type = self.check_expr(right, None);
                let left_type = self.check_expr(left, Some(&right_type));
                (left_type, right_type)
            } else {
                let left_type = self.check_expr(left, operand_expected);
                let right_type = self.check_expr(right, Some(&left_type));
                (left_type, right_type)
            };
        self.expect_same(&left_type, &right_type, span);
        if matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual) {
            let supported = left_type.is_numeric()
                || matches!(left_type, Type::Bool | Type::Char | Type::Pointer { .. })
                || matches!(&left_type, Type::Enum(name) if self.analyzer.model.enums.get(name).is_some_and(|info| info.variants.values().all(Vec::is_empty)));
            if !supported {
                self.analyzer
                    .error("ALTC3025", "type does not support equality", span);
            }
            Type::Bool
        } else if matches!(
            operator,
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual
        ) {
            if !left_type.is_numeric() {
                self.analyzer.error(
                    "ALTC3025",
                    "ordering comparison requires numeric operands",
                    span,
                );
            }
            Type::Bool
        } else {
            let integer_only = matches!(
                operator,
                BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::Remainder
                    | BinaryOp::WrapAdd
                    | BinaryOp::WrapSubtract
                    | BinaryOp::WrapMultiply
                    | BinaryOp::SaturatingAdd
                    | BinaryOp::SaturatingSubtract
                    | BinaryOp::SaturatingMultiply
            );
            if (integer_only && !left_type.is_integer())
                || (!integer_only && !left_type.is_numeric())
            {
                self.analyzer
                    .error("ALTC3026", "operator is not defined for this type", span);
            }
            left_type
        }
    }

    fn check_struct_literal(
        &mut self,
        path: &[String],
        fields: &[FieldInitializer],
        span: Span,
    ) -> Type {
        if path.len() != 1 {
            self.analyzer
                .error("ALTC3027", "module paths are not supported in v0", span);
            return Type::Unknown;
        }
        let name = &path[0];
        let Some(info) = self.analyzer.model.structs.get(name).cloned() else {
            self.analyzer
                .error("ALTC3028", format!("unknown struct `{name}`"), span);
            return Type::Unknown;
        };
        let mut supplied = HashSet::new();
        for field in fields {
            if !supplied.insert(field.name.clone()) {
                self.analyzer
                    .duplicate("field initializer", &field.name, field.span);
            }
            if let Some(expected) = info.fields.get(&field.name) {
                let actual = self.check_expr(&field.value, Some(expected));
                self.expect_same(expected, &actual, field.value.span);
            } else {
                self.analyzer.error(
                    "ALTC3029",
                    format!("unknown field `{}` on `{name}`", field.name),
                    field.span,
                );
            }
        }
        for field in info.fields.keys() {
            if !supplied.contains(field) {
                self.analyzer.error(
                    "ALTC3030",
                    format!("missing field `{field}` in `{name}` literal"),
                    span,
                );
            }
        }
        Type::Struct(name.clone())
    }

    fn check_field(
        &mut self,
        base: &Expr,
        name: &str,
        pointer: bool,
        write: bool,
        span: Span,
    ) -> (Type, bool) {
        let base_type = self.check_expr(base, None);
        let (aggregate, mutable) = if pointer {
            match base_type {
                Type::Pointer {
                    nullable: false,
                    mutable,
                    pointee,
                } => {
                    self.memory_current(
                        if write {
                            MemoryCategory::IndirectWrite
                        } else {
                            MemoryCategory::IndirectRead
                        },
                        span,
                    );
                    (*pointee, mutable)
                }
                Type::Pointer { nullable: true, .. } => {
                    self.analyzer.error(
                        "ALTC3021",
                        "nullable pointer field access requires refinement",
                        span,
                    );
                    return (Type::Unknown, false);
                }
                other => {
                    self.analyzer.error(
                        "ALTC3031",
                        format!("`->` requires a pointer, found `{}`", other.display()),
                        span,
                    );
                    return (Type::Unknown, false);
                }
            }
        } else {
            let (_, mutable) = self.check_place(base, false);
            (base_type, mutable)
        };
        match aggregate {
            Type::Struct(struct_name) => {
                let ty = self
                    .analyzer
                    .model
                    .structs
                    .get(&struct_name)
                    .and_then(|info| info.fields.get(name))
                    .cloned();
                if let Some(ty) = ty {
                    (ty, mutable)
                } else {
                    self.analyzer.error(
                        "ALTC3032",
                        format!("unknown field `{name}` on `{struct_name}`"),
                        span,
                    );
                    (Type::Unknown, false)
                }
            }
            Type::Str if name == "byte_len" => (Type::Int(IntType::Usize), false),
            other => {
                self.analyzer.error(
                    "ALTC3033",
                    format!("type `{}` has no field `{name}`", other.display()),
                    span,
                );
                (Type::Unknown, false)
            }
        }
    }

    fn check_place(&mut self, expression: &Expr, write: bool) -> (Type, bool) {
        match &expression.kind {
            ExprKind::Path(path) if path.len() == 1 => {
                if let Some(binding) = self.lookup(&path[0]).cloned() {
                    (binding.ty, binding.mutable)
                } else if let Some(ty) = self.analyzer.model.const_types.get(&path[0]).cloned() {
                    (ty, false)
                } else {
                    self.analyzer
                        .error("ALTC3013", "unknown assignment target", expression.span);
                    (Type::Unknown, false)
                }
            }
            ExprKind::Unary {
                operator: UnaryOp::Dereference,
                value,
            } => {
                let pointer = self.check_expr(value, None);
                match pointer {
                    Type::Pointer {
                        nullable: false,
                        mutable,
                        pointee,
                    } => {
                        if write {
                            self.memory_current(MemoryCategory::IndirectWrite, expression.span);
                        }
                        (*pointee, mutable)
                    }
                    _ => {
                        self.analyzer.error(
                            "ALTC3034",
                            "invalid pointer assignment target",
                            expression.span,
                        );
                        (Type::Unknown, false)
                    }
                }
            }
            ExprKind::Field {
                base,
                name,
                pointer,
            } => self.check_field(base, name, *pointer, write, expression.span),
            _ => {
                let ty = self.check_expr(expression, None);
                self.analyzer.error(
                    "ALTC3035",
                    "expression is not an assignable place",
                    expression.span,
                );
                (ty, false)
            }
        }
    }

    fn check_arms(
        &mut self,
        value_type: &Type,
        arms: &[MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let mut result: Option<Type> = None;
        let mut covered = BTreeSet::new();
        let mut covers_all = false;
        for arm in arms {
            self.push_scope();
            let coverage = self.check_pattern(&arm.pattern, value_type);
            if coverage == "*" {
                covers_all = true;
            } else if !covered.insert(coverage.clone()) {
                self.analyzer.error(
                    "ALTC3036",
                    format!("unreachable duplicate pattern `{coverage}`"),
                    arm.pattern.span(),
                );
            }
            let arm_type = self.check_expr(&arm.value, expected.or(result.as_ref()));
            result = Some(match result {
                Some(previous) => self.unify(&previous, &arm_type, arm.value.span),
                None => arm_type,
            });
            self.pop_scope();
        }
        if !covers_all && !self.is_exhaustive(value_type, &covered) {
            self.analyzer
                .error("ALTC3037", "non-exhaustive match", span);
        }
        result.unwrap_or(Type::Unit)
    }

    fn check_pattern(&mut self, pattern: &Pattern, value_type: &Type) -> String {
        match pattern {
            Pattern::Wildcard(_) => "*".to_owned(),
            Pattern::Bind(name, span) => {
                self.bind(name.clone(), value_type.clone(), false, *span);
                "*".to_owned()
            }
            Pattern::Literal(literal, span) => {
                let ty = self.check_literal(literal, Some(value_type), *span);
                self.expect_same(value_type, &ty, *span);
                match literal {
                    Literal::Bool(value) => value.to_string(),
                    Literal::Int(value) => value.clone(),
                    _ => "literal".to_owned(),
                }
            }
            Pattern::Null(span) => {
                if !matches!(value_type, Type::Pointer { nullable: true, .. }) {
                    self.analyzer.error(
                        "ALTC3038",
                        "`null` pattern requires a nullable pointer",
                        *span,
                    );
                }
                "null".to_owned()
            }
            Pattern::NonNull(name, span) => {
                if let Type::Pointer {
                    nullable: true,
                    mutable,
                    pointee,
                } = value_type
                {
                    self.bind(
                        name.clone(),
                        Type::Pointer {
                            nullable: false,
                            mutable: *mutable,
                            pointee: pointee.clone(),
                        },
                        false,
                        *span,
                    );
                } else {
                    self.analyzer
                        .error("ALTC3039", "`nonnull` requires a nullable pointer", *span);
                }
                "nonnull".to_owned()
            }
            Pattern::Path(path, span) | Pattern::Variant(path, _, span) => {
                let Type::Enum(enum_name) = value_type else {
                    self.analyzer
                        .error("ALTC3040", "variant pattern requires an enum", *span);
                    return path.join("::");
                };
                let Some((path_enum, variant)) = split_variant(path) else {
                    self.analyzer
                        .error("ALTC3041", "variant pattern must be qualified", *span);
                    return path.join("::");
                };
                if path_enum != enum_name {
                    self.analyzer
                        .error("ALTC3042", "variant belongs to a different enum", *span);
                }
                let payload = self
                    .analyzer
                    .model
                    .enums
                    .get(enum_name)
                    .and_then(|info| info.variants.get(variant))
                    .cloned();
                if let Some(payload) = payload {
                    let patterns = match pattern {
                        Pattern::Variant(_, patterns, _) => patterns.as_slice(),
                        _ => &[],
                    };
                    if payload.len() != patterns.len() {
                        self.analyzer.error(
                            "ALTC3043",
                            "variant pattern payload count differs",
                            *span,
                        );
                    }
                    for (pattern, ty) in patterns.iter().zip(payload.iter()) {
                        self.check_pattern(pattern, ty);
                    }
                } else {
                    self.analyzer
                        .error("ALTC3044", format!("unknown variant `{variant}`"), *span);
                }
                variant.to_owned()
            }
            Pattern::Struct(path, fields, span) => {
                if path.len() != 1 || *value_type != Type::Struct(path[0].clone()) {
                    self.analyzer
                        .error("ALTC3045", "struct pattern type does not match", *span);
                    return "struct".to_owned();
                }
                let info = self.analyzer.model.structs.get(&path[0]).cloned();
                if let Some(info) = info {
                    for field in fields {
                        if let Some(ty) = info.fields.get(&field.name) {
                            if let Some(pattern) = &field.pattern {
                                self.check_pattern(pattern, ty);
                            } else {
                                self.bind(field.name.clone(), ty.clone(), false, field.span);
                            }
                        } else {
                            self.analyzer.error(
                                "ALTC3046",
                                "unknown struct pattern field",
                                field.span,
                            );
                        }
                    }
                }
                "struct".to_owned()
            }
            Pattern::Error(_) => "error".to_owned(),
        }
    }

    fn is_exhaustive(&self, value_type: &Type, covered: &BTreeSet<String>) -> bool {
        match value_type {
            Type::Bool => covered.contains("true") && covered.contains("false"),
            Type::Pointer { nullable: true, .. } => {
                covered.contains("null") && covered.contains("nonnull")
            }
            Type::Enum(name) => self.analyzer.model.enums.get(name).is_some_and(|info| {
                info.variants
                    .keys()
                    .all(|variant| covered.contains(variant))
            }),
            _ => false,
        }
    }

    fn route_error(&mut self, error_type: Type, span: Span) {
        if let Some(target) = self.try_errors.last_mut() {
            if let Some(previous) = target {
                if !types_compatible(previous, &error_type) {
                    self.analyzer.error(
                        "ALTC3107",
                        "all errors handled by one try block must have the same type",
                        span,
                    );
                }
            } else {
                *target = Some(error_type);
            }
        } else {
            self.actual_effects
                .insert(Effect::Raise(error_type.display()));
        }
    }

    fn memory_current(&mut self, category: MemoryCategory, span: Span) {
        if let Some(function) = self.function.clone() {
            self.memory(&function, category, span);
        }
    }

    fn memory(&mut self, function: &str, category: MemoryCategory, span: Span) {
        self.analyzer
            .direct_memory
            .entry(function.to_owned())
            .or_default()
            .push(MemoryEvent { category, span });
    }

    fn unify(&mut self, left: &Type, right: &Type, span: Span) -> Type {
        if *left == Type::Never {
            return right.clone();
        }
        if *right == Type::Never {
            return left.clone();
        }
        self.expect_same(left, right, span);
        if *left == Type::Unknown {
            right.clone()
        } else {
            left.clone()
        }
    }

    fn expect_same(&mut self, expected: &Type, actual: &Type, span: Span) {
        if !types_compatible(expected, actual) {
            self.analyzer.error(
                "ALTC3047",
                format!(
                    "type mismatch: expected `{}`, found `{}`",
                    expected.display(),
                    actual.display()
                ),
                span,
            );
        }
    }

    fn bind(&mut self, name: String, ty: Type, mutable: bool, span: Span) {
        let scope = self.scopes.last_mut().expect("binding scope");
        if scope
            .insert(name.clone(), Binding { ty, mutable })
            .is_some()
        {
            self.analyzer.duplicate("binding", &name, span);
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

pub(crate) fn primitive_type(name: &str) -> Option<Type> {
    let ty = match name {
        "unit" => Type::Unit,
        "bool" => Type::Bool,
        "char" => Type::Char,
        "str" => Type::Str,
        "i8" => Type::Int(IntType::I8),
        "i16" => Type::Int(IntType::I16),
        "i32" => Type::Int(IntType::I32),
        "i64" => Type::Int(IntType::I64),
        "u8" => Type::Int(IntType::U8),
        "u16" => Type::Int(IntType::U16),
        "u32" => Type::Int(IntType::U32),
        "u64" => Type::Int(IntType::U64),
        "isize" => Type::Int(IntType::Isize),
        "usize" => Type::Int(IntType::Usize),
        "c_char" => Type::Int(IntType::CChar),
        "c_schar" => Type::Int(IntType::CSChar),
        "c_uchar" => Type::Int(IntType::CUChar),
        "c_short" => Type::Int(IntType::CShort),
        "c_ushort" => Type::Int(IntType::CUShort),
        "c_int" => Type::Int(IntType::CInt),
        "c_uint" => Type::Int(IntType::CUInt),
        "c_long" => Type::Int(IntType::CLong),
        "c_ulong" => Type::Int(IntType::CULong),
        "c_longlong" => Type::Int(IntType::CLongLong),
        "c_ulonglong" => Type::Int(IntType::CULongLong),
        "f32" => Type::Float(FloatType::F32),
        "f64" => Type::Float(FloatType::F64),
        "c_float" => Type::Float(FloatType::CFloat),
        "c_double" => Type::Float(FloatType::CDouble),
        "c_longdouble" => Type::Float(FloatType::CLongDouble),
        _ => return None,
    };
    Some(ty)
}

fn integer_literal_type(raw: &str) -> Option<Type> {
    [
        IntType::I8,
        IntType::I16,
        IntType::I32,
        IntType::I64,
        IntType::U8,
        IntType::U16,
        IntType::U32,
        IntType::U64,
        IntType::Isize,
        IntType::Usize,
        IntType::CChar,
        IntType::CSChar,
        IntType::CUChar,
        IntType::CShort,
        IntType::CUShort,
        IntType::CInt,
        IntType::CUInt,
        IntType::CLong,
        IntType::CULong,
        IntType::CLongLong,
        IntType::CULongLong,
    ]
    .into_iter()
    .find(|kind| raw.ends_with(kind.name()))
    .map(Type::Int)
}

fn integer_maximum(kind: IntType) -> (u128, bool) {
    match kind {
        IntType::I8 => (i8::MAX as u128, true),
        IntType::I16 => (i16::MAX as u128, true),
        IntType::I32 => (i32::MAX as u128, true),
        IntType::I64 => (i64::MAX as u128, true),
        IntType::U8 => (u8::MAX as u128, false),
        IntType::U16 => (u16::MAX as u128, false),
        IntType::U32 => (u32::MAX as u128, false),
        IntType::U64 => (u64::MAX as u128, false),
        IntType::Isize => (isize::MAX as u128, true),
        IntType::Usize => (usize::MAX as u128, false),
        IntType::CChar => (std::ffi::c_char::MAX as u128, std::ffi::c_char::MIN < 0),
        IntType::CSChar => (std::ffi::c_schar::MAX as u128, true),
        IntType::CUChar => (std::ffi::c_uchar::MAX as u128, false),
        IntType::CShort => (std::ffi::c_short::MAX as u128, true),
        IntType::CUShort => (std::ffi::c_ushort::MAX as u128, false),
        IntType::CInt => (std::ffi::c_int::MAX as u128, true),
        IntType::CUInt => (std::ffi::c_uint::MAX as u128, false),
        IntType::CLong => (std::ffi::c_long::MAX as u128, true),
        IntType::CULong => (std::ffi::c_ulong::MAX as u128, false),
        IntType::CLongLong => (std::ffi::c_longlong::MAX as u128, true),
        IntType::CULongLong => (std::ffi::c_ulonglong::MAX as u128, false),
    }
}

fn has_repr_c(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.name == "repr" && attribute.arguments.as_slice() == ["C"])
}

fn by_value_named_type(ty: &Type) -> Option<&str> {
    match ty {
        Type::Struct(name) | Type::Enum(name) => Some(name),
        _ => None,
    }
}

fn expression_needs_context(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Literal(Literal::Int(raw)) => integer_literal_type(raw).is_none(),
        ExprKind::Literal(Literal::Float(raw)) => !raw.ends_with("f32") && !raw.ends_with("f64"),
        ExprKind::Literal(Literal::Null) => true,
        ExprKind::Unary {
            operator: UnaryOp::Plus | UnaryOp::Negate,
            value,
        } => expression_needs_context(value),
        _ => false,
    }
}

fn is_c_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if first == b'_' || !(first.is_ascii_alphabetic()) {
        return false;
    }
    if !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        return false;
    }
    !matches!(
        value,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "_Alignas"
            | "_Alignof"
            | "_Atomic"
            | "_Bool"
            | "_Complex"
            | "_Generic"
            | "_Imaginary"
            | "_Noreturn"
            | "_Static_assert"
            | "_Thread_local"
    )
}

fn split_variant(path: &[String]) -> Option<(&str, &str)> {
    (path.len() == 2).then(|| (path[0].as_str(), path[1].as_str()))
}

fn effect_span(effect: &EffectRef) -> Span {
    match effect {
        EffectRef::Alloc(span) | EffectRef::Free(span) | EffectRef::Raise(_, span) => *span,
    }
}

fn effect_name(effect: &Effect) -> String {
    match effect {
        Effect::Alloc => "alloc".to_owned(),
        Effect::Free => "free".to_owned(),
        Effect::Raise(ty) => format!("raise[{ty}]"),
    }
}

fn types_compatible(expected: &Type, actual: &Type) -> bool {
    expected == actual
        || matches!(expected, Type::Unknown)
        || matches!(actual, Type::Unknown | Type::Never)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_text(text: &str) -> Analysis {
        analyze(
            SourceFile::new("test.altc", text),
            &AnalyzeOptions::default(),
        )
    }

    #[test]
    fn infers_transitive_memory_summary() {
        let analysis = analyze_text(
            "fn read(p: *const i32) -> i32 { *p }\nfn caller(p: *const i32) -> i32 { read(p) }",
        );
        assert!(analysis.succeeded(), "{:?}", analysis.diagnostics);
        let caller = analysis
            .memory_summaries
            .iter()
            .find(|summary| summary.function == "caller")
            .unwrap();
        assert!(caller.categories.contains(&MemoryCategory::IndirectRead));
        assert_eq!(caller.transitive[0].callee, "read");
    }

    #[test]
    fn rejects_missing_effect_and_question_mark() {
        let analysis = analyze_text(
            "enum E { Bad, } fn fail() -> i32 with { raise[E] } { raise E::Bad; } fn caller() -> i32 { fail() }",
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3106")
        );
    }

    #[test]
    fn accepts_nullable_exhaustive_match() {
        let analysis = analyze_text(
            "fn value(p: ?*const i32) -> i32 { match p { nonnull(x) => *x, null => 0, } }",
        );
        assert!(analysis.succeeded(), "{:?}", analysis.diagnostics);
    }

    #[test]
    fn rejects_invalid_and_out_of_range_literals() {
        let analysis = analyze_text(
            r#"
fn bad() {
  let invalid_digits: u8 = 0b102;
  let too_large: u8 = 256u8;
  let invalid_escape: str = "\q";
  let invalid_char: char = 'ab';
  let embedded_nul: *const c_char = c"a\0b";
}
"#,
        );
        let codes: Vec<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains(&"ALTC1005"));
        assert!(codes.contains(&"ALTC1007"));
        assert!(codes.contains(&"ALTC1008"));
        assert!(codes.contains(&"ALTC1009"));
    }

    #[test]
    fn validates_attribute_arguments_placement_and_c_names() {
        let analysis = analyze_text(
            r#"
@repr(Rust)
enum WrongRepr { Value, }

@c_name("not-exported")
fn internal() {}

@c_name("same")
export "C" fn first() {}

@c_name("same")
export "C" fn second() {}
"#,
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3210")
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3211")
        );
    }

    #[test]
    fn applies_contextual_literals_and_exact_operator_rules() {
        let valid = analyze_text(
            "fn same(value: i64, pointer: ?*const i32) -> bool { 1 + value == value + 1 && null == pointer }",
        );
        assert!(valid.succeeded(), "{:?}", valid.diagnostics);

        let invalid = analyze_text(
            r#"
enum Payload { Some(i32), }
fn payload_equal(left: Payload, right: Payload) -> bool { left == right }
fn signed_shift(value: i32) -> i32 { value << 1i32 }
fn bool_order(left: bool, right: bool) -> bool { left < right }
fn float_remainder(left: f32, right: f32) -> f32 { left % right }
"#,
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3025")
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3026")
        );
    }

    #[test]
    fn diagnoses_mutability_type_entry_and_exhaustiveness_errors() {
        let analysis = analyze_text(
            r#"
enum Choice { First, Second, }
@entry fn bad_entry(value: i32) -> i32 { value }
fn invalid(choice: Choice) -> i32 {
  let value: i32 = 1;
  value = 2;
  let wrong: bool = value;
  match choice { Choice::First => 1, }
}
"#,
        );
        let codes: BTreeSet<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains("ALTC3008"));
        assert!(codes.contains("ALTC3047"));
        assert!(codes.contains("ALTC3202"));
        assert!(codes.contains("ALTC3037"));
    }

    #[test]
    fn reaches_recursive_memory_fixed_point() {
        let analysis = analyze_text(
            r#"
fn first(pointer: *const i32) -> i32 { second(pointer) }
fn second(pointer: *const i32) -> i32 {
  if *pointer == 0 { 0 } else { first(pointer) }
}
"#,
        );
        assert!(analysis.succeeded(), "{:?}", analysis.diagnostics);
        for function in ["first", "second"] {
            let summary = analysis
                .memory_summaries
                .iter()
                .find(|summary| summary.function == function)
                .unwrap();
            assert!(summary.categories.contains(&MemoryCategory::IndirectRead));
        }
    }

    #[test]
    fn propagates_declared_allocation_effects_through_calls() {
        let analysis = analyze_text(
            r#"
extern "C" from <stdlib.h> {
  fn malloc(size: usize) -> ?*mut unit with { alloc };
}
fn allocator() -> ?*mut unit with { alloc } { malloc(1usize) }
fn missing_contract() -> ?*mut unit { allocator() }
"#,
        );
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ALTC3103"
                && diagnostic.message.contains("missing_contract")
                && diagnostic.message.contains("alloc")
        }));
    }

    #[test]
    fn covers_type_effect_literal_and_identifier_helpers() {
        let integers = [
            IntType::I8,
            IntType::I16,
            IntType::I32,
            IntType::I64,
            IntType::U8,
            IntType::U16,
            IntType::U32,
            IntType::U64,
            IntType::Isize,
            IntType::Usize,
            IntType::CChar,
            IntType::CSChar,
            IntType::CUChar,
            IntType::CShort,
            IntType::CUShort,
            IntType::CInt,
            IntType::CUInt,
            IntType::CLong,
            IntType::CULong,
            IntType::CLongLong,
            IntType::CULongLong,
        ];
        for kind in integers {
            assert_eq!(primitive_type(kind.name()), Some(Type::Int(kind)));
            let (maximum, _) = integer_maximum(kind);
            assert!(maximum > 0);
            assert_eq!(
                integer_literal_type(&format!("1{}", kind.name())),
                Some(Type::Int(kind))
            );
            let _ = kind.is_unsigned();
        }
        for (name, kind) in [
            ("f32", FloatType::F32),
            ("f64", FloatType::F64),
            ("c_float", FloatType::CFloat),
            ("c_double", FloatType::CDouble),
            ("c_longdouble", FloatType::CLongDouble),
        ] {
            assert_eq!(primitive_type(name), Some(Type::Float(kind)));
            assert_eq!(Type::Float(kind).display(), name);
        }
        assert_eq!(primitive_type("unit"), Some(Type::Unit));
        assert_eq!(primitive_type("bool"), Some(Type::Bool));
        assert_eq!(primitive_type("char"), Some(Type::Char));
        assert_eq!(primitive_type("str"), Some(Type::Str));
        assert_eq!(primitive_type("missing"), None);
        assert_eq!(integer_literal_type("12"), None);

        let displayed = [
            Type::Unit,
            Type::Bool,
            Type::Char,
            Type::Str,
            Type::Pointer {
                nullable: true,
                mutable: true,
                pointee: Box::new(Type::Int(IntType::I32)),
            },
            Type::Pointer {
                nullable: false,
                mutable: false,
                pointee: Box::new(Type::Int(IntType::I32)),
            },
            Type::Struct("S".to_owned()),
            Type::Enum("E".to_owned()),
            Type::Opaque("O".to_owned()),
            Type::Never,
            Type::Unknown,
        ];
        for ty in &displayed {
            assert!(!ty.display().is_empty());
        }
        assert!(Type::Int(IntType::I32).is_numeric());
        assert!(Type::Int(IntType::I32).is_integer());
        assert!(!Type::Unit.is_storable());
        assert!(Type::Bool.is_storable());

        assert!(is_c_identifier("valid_name9"));
        for invalid in ["", "_hidden", "9bad", "bad-name", "while", "_Thread_local"] {
            assert!(!is_c_identifier(invalid), "{invalid}");
        }
        assert!(split_variant(&["E".to_owned(), "V".to_owned()]).is_some());
        assert!(split_variant(&["V".to_owned()]).is_none());

        let span = Span::new(1, 2);
        let repr = Attribute {
            name: "repr".to_owned(),
            arguments: vec!["C".to_owned()],
            span,
        };
        assert!(has_repr_c(std::slice::from_ref(&repr)));
        assert!(!has_repr_c(&[]));
        assert_eq!(
            by_value_named_type(&Type::Struct("S".to_owned())),
            Some("S")
        );
        assert_eq!(by_value_named_type(&Type::Enum("E".to_owned())), Some("E"));
        assert_eq!(by_value_named_type(&Type::Bool), None);

        for effect in [
            EffectRef::Alloc(span),
            EffectRef::Free(span),
            EffectRef::Raise(
                TypeRef::Named {
                    path: vec!["E".to_owned()],
                    span,
                },
                span,
            ),
        ] {
            assert_eq!(effect_span(&effect), span);
        }
        assert_eq!(effect_name(&Effect::Alloc), "alloc");
        assert_eq!(effect_name(&Effect::Free), "free");
        assert_eq!(effect_name(&Effect::Raise("E".to_owned())), "raise[E]");
        assert!(types_compatible(&Type::Bool, &Type::Bool));
        assert!(types_compatible(&Type::Unknown, &Type::Bool));
        assert!(types_compatible(&Type::Bool, &Type::Unknown));
        assert!(types_compatible(&Type::Bool, &Type::Never));
        assert!(!types_compatible(&Type::Bool, &Type::Int(IntType::I32)));

        let expression = |id, kind| Expr { id, kind, span };
        assert!(expression_needs_context(&expression(
            0,
            ExprKind::Literal(Literal::Int("1".to_owned()))
        )));
        assert!(!expression_needs_context(&expression(
            1,
            ExprKind::Literal(Literal::Int("1i32".to_owned()))
        )));
        assert!(expression_needs_context(&expression(
            2,
            ExprKind::Literal(Literal::Float("1.0".to_owned()))
        )));
        assert!(!expression_needs_context(&expression(
            3,
            ExprKind::Literal(Literal::Float("1.0f32".to_owned()))
        )));
        assert!(expression_needs_context(&expression(
            4,
            ExprKind::Literal(Literal::Null)
        )));
        let nested = expression(5, ExprKind::Literal(Literal::Int("1".to_owned())));
        assert!(expression_needs_context(&expression(
            6,
            ExprKind::Unary {
                operator: UnaryOp::Negate,
                value: Box::new(nested)
            }
        )));
        assert!(!expression_needs_context(&expression(
            7,
            ExprKind::Literal(Literal::Bool(true))
        )));
    }

    #[test]
    fn diagnoses_declaration_layout_abi_and_effect_contract_errors() {
        let analysis = analyze_text(
            r#"
struct Duplicate { value: i32, value: i32, }
struct Duplicate { unit: unit, }
struct Recursive { next: Recursive, }
@repr(C) struct BadAbi { text: str, }
enum Payload { Some(unit), Some(i32), }
@repr(C) enum ReprPayload { Some(i32), }
enum Error { Bad, }
const SAME: i32 = 1;
const SAME: i32 = 2;
fn effects() with { alloc, alloc, raise[Error], raise[Error] } {}
fn effects() {}
fn duplicate_parameter(value: i32, value: i32) {}
@entry fn first() -> c_int { 0 }
@entry fn second() -> c_int { 0 }
export "C" fn bad_parameter(value: str) {}
export "C" fn bad_return() -> str { "bad" }
export "C" fn bad_error() with { raise[str] } { raise "bad"; }
extern "C" from <stddef.h> {
  fn external_raise() with { raise[Error] };
}
"#,
        );
        let codes: BTreeSet<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for expected in [
            "ALTC3001", "ALTC3002", "ALTC3003", "ALTC3006", "ALTC3101", "ALTC3102", "ALTC3201",
            "ALTC3203", "ALTC3204", "ALTC3205", "ALTC3206", "ALTC3207", "ALTC3212",
        ] {
            assert!(codes.contains(expected), "missing {expected}: {codes:?}");
        }
    }

    #[test]
    fn diagnoses_expression_control_builtin_place_and_pattern_errors() {
        let analysis = analyze_text(
            r#"
struct Point { x: i32, y: i32, }
enum E { Empty, One(i32), }
enum F { Other, }
fn plain(value: i32) -> i32 { value }
fn raising() -> i32 with { raise[E] } { raise E::Empty; }
fn raising_f() -> i32 with { raise[F] } { raise F::Other; }
const CALL: i32 = plain(1);
fn errors(pointer: ?*mut Point, point: Point, number: i32) {
  let duplicate: i32 = 1;
  let duplicate: i32 = 2;
  plain;
  E::One;
  missing;
  (plain)(1);
  (if true { plain } else { plain })(1);
  plain();
  E::One();
  Missing::Value();
  null;
  true as Point;
  true += false;
  *pointer;
  *number;
  ~true;
  +true;
  false + false;
  1u32 << 1i32;
  Point::Nested { x: 1, y: 2, };
  Missing { value: 1, };
  Point { x: 1, x: 2, unknown: 3, };
  pointer->x;
  number->x;
  point.unknown;
  number.field;
  missing = 1;
  *number = 1;
  1 = 2;
  match true { true => 1, true => 2, };
  match number { null => 0, nonnull(value) => 1, };
  match number { E::Empty => 0, };
  match E::Empty { Empty() => 0, };
  match E::Empty { F::Other => 0, E::Empty(inside) => 1, E::Missing => 2, };
  match point { E { x, } => 0, };
  match point { Point { missing, } => 0, };
  try { 1 } catch { _ => 1, };
  try { raising()?; raising_f()?; 0 } catch { _ => 0, };
  raising()?;
  plain()?;
  (1)?;
  break;
  continue;
  defer { raising()?; raise E::Empty; return; break; }
  sizeof[];
  sizeof[unit];
  plain[i32];
  ptr_as_const(number, number);
  ptr_addr();
  ptr_cast[i32]();
  ptr_cast[i32](number);
  offset[i32](number);
  ptr_from_addr[i32]();
}
"#,
        );
        let codes: BTreeSet<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for expected in [
            "ALTC3006", "ALTC3007", "ALTC3008", "ALTC3009", "ALTC3010", "ALTC3011", "ALTC3012",
            "ALTC3013", "ALTC3015", "ALTC3016", "ALTC3017", "ALTC3018", "ALTC3019", "ALTC3020",
            "ALTC3021", "ALTC3022", "ALTC3023", "ALTC3024", "ALTC3027", "ALTC3028", "ALTC3029",
            "ALTC3030", "ALTC3031", "ALTC3032", "ALTC3033", "ALTC3034", "ALTC3035", "ALTC3036",
            "ALTC3038", "ALTC3039", "ALTC3040", "ALTC3041", "ALTC3042", "ALTC3043", "ALTC3044",
            "ALTC3045", "ALTC3046", "ALTC3048", "ALTC3049", "ALTC3050", "ALTC3104", "ALTC3105",
            "ALTC3107", "ALTC3302", "ALTC3303", "ALTC3304", "ALTC3305", "ALTC3306",
        ] {
            assert!(codes.contains(expected), "missing {expected}: {codes:?}");
        }
    }

    #[test]
    fn reports_requested_cross_compilation_target() {
        let analysis = analyze(
            SourceFile::new("target.altc", "fn value() {}"),
            &AnalyzeOptions {
                target: Some("wasm32-unknown-unknown".to_owned()),
            },
        );
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ALTC4001" && diagnostic.message.contains("wasm32")
        }));
    }

    #[test]
    fn rejects_type_arguments_that_pointer_builtins_would_otherwise_ignore() {
        let analysis = analyze_text(
            r#"
fn invalid(pointer: *mut i32) {
  ptr_addr[i32](pointer);
  ptr_as_const[i32](pointer);
}
"#,
        );
        let builtin_arity_diagnostics = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "ALTC3019")
            .count();
        assert_eq!(builtin_arity_diagnostics, 2, "{:?}", analysis.diagnostics);
    }

    #[test]
    fn validates_remaining_attribute_symbol_and_opaque_type_cases() {
        let analysis = analyze_text(
            r#"
@unknown
struct UnknownAttribute { value: i32, }
@repr(C) @repr(C)
struct DuplicateAttribute { value: i32, }
enum VariantAttribute { @entry Value, }
@entry(1)
fn entry_arguments() {}
@c_name("main")
export "C" fn reserved_main() {}
@c_name("while")
export "C" fn reserved_keyword() {}
extern "C" from <stddef.h> {
  @entry fn wrong_extern_attribute();
  opaque type _private;
  @c_typedef() opaque type MissingArgument;
  @c_typedef("bad-name") opaque type InvalidSpelling;
  @c_typedef("\q") opaque type InvalidEscape;
}
"#,
        );
        assert!(!analysis.succeeded());
        assert!(analysis.has_extern_declarations());
        let messages = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(messages.iter().any(|message| message.contains("duplicate")));
        assert!(messages.iter().any(|message| message.contains("reserved")));
        assert!(
            messages
                .iter()
                .any(|message| message.contains("enum variants"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("opaque type name"))
        );
    }

    #[test]
    fn checks_valid_remaining_expression_and_builtin_paths() {
        let analysis = analyze_text(
            r#"
struct Point { x: i32, y: i32, }
enum Choice { None, Some(i32), }
const GLOBAL: i32 = 1;
fn exercise(pointer: *mut Point, address: usize, flag: bool) -> i32 {
  const LOCAL: i32 = 2;
  let mut point: Point = Point { x: GLOBAL, y: LOCAL, };
  let mut number: i32 = 0;
  for (number = 0; number < 1; number += 1) {};
  let immutable: *const Point = ptr_as_const(pointer);
  let raw: usize = ptr_addr(pointer);
  let changed: *mut i32 = ptr_cast_mut[i32](pointer);
  let advanced: *mut Point = offset[Point](pointer, 1isize);
  let restored: ?*const Point = ptr_from_addr[Point](address);
  let restored_mut: ?*mut Point = ptr_from_addr_mut[Point](raw);
  let point_address: *mut Point = &point;
  let inverted: bool = !flag;
  let negative: i32 = -(number + 1);
  pointer->x;
  pointer->y = *changed;
  point.x = advanced->x;
  let text: str = "text";
  text.byte_len;
  match point { Point { x, y: renamed, } => x + renamed, _ => 0, };
  match restored {
    nonnull(value) => value->x,
    null => if inverted { negative } else { point_address->y },
  }
}
"#,
        );
        assert!(analysis.succeeded(), "{:?}", analysis.diagnostics);
        let summary = analysis
            .memory_summaries
            .iter()
            .find(|summary| summary.function == "exercise")
            .unwrap();
        assert!(summary.categories.contains(&MemoryCategory::IndirectRead));
        assert!(summary.categories.contains(&MemoryCategory::IndirectWrite));
    }

    #[test]
    fn diagnoses_float_escape_value_and_builtin_edge_cases() {
        let analysis = analyze_text(
            r#"
enum E { Value(i32), Empty, }
const GLOBAL: i32 = 1;
fn edge(pointer: *mut i32, address: usize) {
  let too_large: f32 = 1.0e999f32;
  let malformed: f64 = 1.0e+f64;
  let contextual: c_float = 1.5;
  let bad_c_string: *const c_char = c"\q";
  let bad_char: char = '\q';
  let minimum: i8 = -128i8;
  E::Missing;
  unknown();
  Unknown::Value();
  ptr_addr(pointer)?;
  ptr_as_const(pointer);
  ptr_cast_mut[i32](pointer);
  offset[i32](pointer, 1isize);
  offset[i32](address, 1isize);
  ptr_from_addr[i32](address);
  ptr_from_addr_mut[i32](address);
  GLOBAL = 2;
  if true { return; } else {};
}
"#,
        );
        let codes: BTreeSet<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for expected in [
            "ALTC1006", "ALTC1007", "ALTC3013", "ALTC3018", "ALTC3020", "ALTC3105",
        ] {
            assert!(codes.contains(expected), "missing {expected}: {codes:?}");
        }
    }

    #[test]
    fn directly_checks_internal_invariants_and_defensive_pattern_paths() {
        let source = SourceFile::new("internal.altc", "");
        let span = Span::new(0, 0);
        let mut analyzer = Analyzer::new(&source, Program::default(), Vec::new());
        analyzer.model.structs.insert(
            "CStruct".to_owned(),
            StructInfo {
                fields: BTreeMap::new(),
                repr_c: true,
            },
        );
        analyzer.model.structs.insert(
            "RustStruct".to_owned(),
            StructInfo {
                fields: BTreeMap::new(),
                repr_c: false,
            },
        );
        analyzer.model.enums.insert(
            "CEnum".to_owned(),
            EnumInfo {
                variants: BTreeMap::new(),
                repr_c: true,
            },
        );
        analyzer.model.enums.insert(
            "RustEnum".to_owned(),
            EnumInfo {
                variants: BTreeMap::new(),
                repr_c: false,
            },
        );
        for (ty, safe) in [
            (
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Struct("CStruct".to_owned())),
                },
                true,
            ),
            (
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Struct("RustStruct".to_owned())),
                },
                false,
            ),
            (
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Enum("CEnum".to_owned())),
                },
                true,
            ),
            (
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Enum("RustEnum".to_owned())),
                },
                false,
            ),
            (
                Type::Pointer {
                    nullable: false,
                    mutable: false,
                    pointee: Box::new(Type::Str),
                },
                false,
            ),
        ] {
            assert_eq!(analyzer.is_c_abi_safe(&ty, false), safe);
        }
        assert!(!analyzer.layout_depends_on(
            "missing",
            "target",
            &mut BTreeSet::from(["missing".to_owned()])
        ));
        assert!(!analyzer.layout_depends_on("missing", "target", &mut BTreeSet::new()));
        assert_eq!(
            analyzer.resolve_type(&TypeRef::Unsupported {
                feature: "test".to_owned(),
                span,
            }),
            Type::Unknown
        );
        assert_eq!(
            analyzer.resolve_type(&TypeRef::Named {
                path: vec!["module".to_owned(), "Type".to_owned()],
                span,
            }),
            Type::Unknown
        );

        let mut checker = BodyChecker::new(&mut analyzer, None, Type::Unit);
        checker.push_scope();
        let error_expression = Expr {
            id: 900,
            kind: ExprKind::Error,
            span,
        };
        assert_eq!(checker.check_expr(&error_expression, None), Type::Unknown);
        assert_eq!(
            checker.check_pattern(
                &Pattern::Literal(Literal::String("\"x\"".to_owned()), span),
                &Type::Str,
            ),
            "literal"
        );
        assert_eq!(
            checker.check_pattern(
                &Pattern::Literal(Literal::Int("1".to_owned()), span),
                &Type::Int(IntType::I32),
            ),
            "1"
        );
        assert_eq!(
            checker.check_literal(&Literal::Float("1.0e39f32".to_owned()), None, span,),
            Type::Float(FloatType::F32)
        );
        assert_eq!(
            checker.check_pattern(&Pattern::Error(span), &Type::Bool),
            "error"
        );
        assert_eq!(
            checker.check_pattern(
                &Pattern::Struct(vec!["Missing".to_owned()], Vec::new(), span),
                &Type::Struct("Missing".to_owned()),
            ),
            "struct"
        );
        assert_eq!(checker.unify(&Type::Unknown, &Type::Bool, span), Type::Bool);
        checker.pop_scope();

        let unsupported = analyze_text("import feature;");
        assert!(!unsupported.succeeded());
        let qualified = analyze_text("fn value(input: module::Type) {}");
        assert!(
            qualified
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3004")
        );
        let repeated = analyze_text(
            "fn read(p: *const i32) -> i32 { *p } fn twice(p: *const i32) -> i32 { read(p) + read(p) }",
        );
        assert!(repeated.succeeded(), "{:?}", repeated.diagnostics);
        assert_eq!(
            repeated
                .memory_summaries
                .iter()
                .find(|summary| summary.function == "twice")
                .unwrap()
                .transitive
                .len(),
            1
        );
    }
}
