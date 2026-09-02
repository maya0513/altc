use crate::semantics::{FloatType, IntType, Type};

pub(super) fn c_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Int(IntType::I8) => "int8_t".to_owned(),
        Type::Int(IntType::I16) => "int16_t".to_owned(),
        Type::Int(IntType::I32) => "int32_t".to_owned(),
        Type::Int(IntType::I64) => "int64_t".to_owned(),
        Type::Int(IntType::U8) => "uint8_t".to_owned(),
        Type::Int(IntType::U16) => "uint16_t".to_owned(),
        Type::Int(IntType::U32) => "uint32_t".to_owned(),
        Type::Int(IntType::U64) => "uint64_t".to_owned(),
        Type::Int(IntType::Isize) => "ptrdiff_t".to_owned(),
        Type::Int(IntType::Usize) => "size_t".to_owned(),
        Type::Int(IntType::CChar) => "char".to_owned(),
        Type::Int(IntType::CSChar) => "signed char".to_owned(),
        Type::Int(IntType::CUChar) => "unsigned char".to_owned(),
        Type::Int(IntType::CShort) => "short".to_owned(),
        Type::Int(IntType::CUShort) => "unsigned short".to_owned(),
        Type::Int(IntType::CInt) => "int".to_owned(),
        Type::Int(IntType::CUInt) => "unsigned int".to_owned(),
        Type::Int(IntType::CLong) => "long".to_owned(),
        Type::Int(IntType::CULong) => "unsigned long".to_owned(),
        Type::Int(IntType::CLongLong) => "long long".to_owned(),
        Type::Int(IntType::CULongLong) => "unsigned long long".to_owned(),
        Type::Float(FloatType::F32 | FloatType::CFloat) => "float".to_owned(),
        Type::Float(FloatType::F64 | FloatType::CDouble) => "double".to_owned(),
        Type::Float(FloatType::CLongDouble) => "long double".to_owned(),
        Type::Char => "uint32_t".to_owned(),
        Type::Str => "altc_str".to_owned(),
        Type::Pointer {
            mutable, pointee, ..
        } => {
            if **pointee == Type::Unit {
                if *mutable {
                    "void *".to_owned()
                } else {
                    "const void *".to_owned()
                }
            } else if *mutable {
                format!("{} *", c_type(pointee))
            } else {
                format!("const {} *", c_type(pointee))
            }
        }
        Type::Array { element, length } => array_type_name(element, *length),
        Type::Slice { mutable, element } => slice_type_name(*mutable, element),
        Type::FunctionPointer { .. } => function_pointer_type_name(ty),
        Type::Struct(name) | Type::Enum(name) => type_name(name),
        Type::Opaque(name) => name.clone(),
        Type::Never | Type::Unknown => "int32_t".to_owned(),
    }
}

pub(super) fn function_pointer_type_name(ty: &Type) -> String {
    format!("altc_fnptr_{}", type_key(ty))
}

pub(super) fn slice_data_pointer_type(mutable: bool, element: &Type) -> String {
    if mutable {
        format!("{} *", c_type(element))
    } else if matches!(element, Type::Pointer { .. } | Type::FunctionPointer { .. }) {
        format!("{} const *", c_type(element))
    } else {
        format!("const {} *", c_type(element))
    }
}

pub(super) fn array_type_name(element: &Type, length: usize) -> String {
    format!("altc_array_{}_{}", type_key(element), length)
}

pub(super) fn slice_type_name(mutable: bool, element: &Type) -> String {
    format!(
        "altc_slice_{}_{}",
        if mutable { "mut" } else { "const" },
        type_key(element)
    )
}

fn type_key(ty: &Type) -> String {
    match ty {
        Type::Unit => "unit".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Int(kind) => kind.name().to_owned(),
        Type::Float(FloatType::F32) => "f32".to_owned(),
        Type::Float(FloatType::F64) => "f64".to_owned(),
        Type::Float(FloatType::CFloat) => "c_float".to_owned(),
        Type::Float(FloatType::CDouble) => "c_double".to_owned(),
        Type::Float(FloatType::CLongDouble) => "c_longdouble".to_owned(),
        Type::Char => "char".to_owned(),
        Type::Str => "str".to_owned(),
        Type::Pointer {
            nullable,
            mutable,
            pointee,
        } => format!(
            "ptr_{}_{}_{}",
            if *nullable { "nullable" } else { "nonnull" },
            if *mutable { "mut" } else { "const" },
            type_key(pointee)
        ),
        Type::Array { element, length } => format!("array_{}_{}", type_key(element), length),
        Type::Slice { mutable, element } => format!(
            "slice_{}_{}",
            if *mutable { "mut" } else { "const" },
            type_key(element)
        ),
        Type::FunctionPointer {
            extern_c,
            parameters,
            return_type,
            effects,
            error_type,
        } => {
            let parameters = parameters
                .iter()
                .map(type_key)
                .collect::<Vec<_>>()
                .join("_");
            let effects = effects
                .iter()
                .map(|effect| match effect {
                    crate::Effect::Alloc => "alloc".to_owned(),
                    crate::Effect::Free => "free".to_owned(),
                    crate::Effect::Raise(error) => format!("raise_{}", sanitize(error)),
                })
                .collect::<Vec<_>>()
                .join("_");
            format!(
                "{}_{}_to_{}_{}_{}",
                if *extern_c { "c" } else { "altc" },
                if parameters.is_empty() {
                    "void".to_owned()
                } else {
                    parameters
                },
                type_key(return_type),
                if effects.is_empty() { "pure" } else { &effects },
                error_type.as_deref().map_or_else(
                    || "ok".to_owned(),
                    |error| format!("err_{}", type_key(error))
                )
            )
        }
        Type::Struct(name) => format!("struct_{}", sanitize(name)),
        Type::Enum(name) => format!("enum_{}", sanitize(name)),
        Type::Opaque(name) => format!("opaque_{}", sanitize(name)),
        Type::Never => "never".to_owned(),
        Type::Unknown => "unknown".to_owned(),
    }
}

pub(super) fn type_name(name: &str) -> String {
    format!("altc_{}", sanitize(name))
}

pub(super) fn tag_name(name: &str) -> String {
    format!("altc_tag_{}", sanitize(name))
}

pub(super) fn local_name(name: &str) -> String {
    format!("v_{}", sanitize(name))
}

pub(super) fn const_name(name: &str) -> String {
    format!("altc_const_{}", sanitize(name))
}

pub(super) fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}
