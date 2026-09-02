use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::lex;
use crate::parser::parse;
use crate::semantics::{Analysis, AnalyzeOptions, ModuleSpanSource, analyze_combined};
use crate::{SourceFile, SyntaxNode};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModulePath(Vec<String>);

impl ModulePath {
    #[must_use]
    pub fn new(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(segments.into_iter().map(Into::into).collect())
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    fn root() -> Self {
        Self(Vec::new())
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("::"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleLoadError {
    pub message: String,
}

impl ModuleLoadError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait ModuleLoader {
    fn load(&mut self, module: &ModulePath) -> Result<SourceFile, ModuleLoadError>;
}

#[derive(Clone, Debug)]
pub struct AnalysisModule {
    pub path: ModulePath,
    pub source: SourceFile,
    pub cst: SyntaxNode,
}

struct ParsedModule {
    source: SourceFile,
    cst: SyntaxNode,
    program: Program,
    imports: Vec<ImportDecl>,
}

#[derive(Clone, Copy)]
struct DeclarationVisibility {
    public: bool,
}

pub fn analyze_project(
    root: SourceFile,
    loader: &mut dyn ModuleLoader,
    options: &AnalyzeOptions,
) -> Analysis {
    let mut modules = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let root_path = ModulePath::root();
    let root_module = parse_module(root.clone(), &mut diagnostics);
    modules.insert(root_path.clone(), root_module);
    let mut visiting = vec![root_path.clone()];
    load_imports(
        &root_path,
        loader,
        &mut modules,
        &mut visiting,
        &mut diagnostics,
    );

    let visibility = collect_visibility(&modules);
    let mut combined_text = String::new();
    let mut offsets = BTreeMap::new();
    let mut ordered_paths = vec![root_path.clone()];
    ordered_paths.extend(modules.keys().filter(|path| !path.0.is_empty()).cloned());
    for path in &ordered_paths {
        offsets.insert(path.clone(), combined_text.len());
        combined_text.push_str(&modules[path].source.text);
        combined_text.push('\n');
    }

    let mut combined = Program::default();
    let mut next_expression = 0;
    let mut span_sources = Vec::new();
    let mut public_modules = Vec::new();
    for path in &ordered_paths {
        let module = &modules[path];
        let offset = offsets[path];
        span_sources.push(ModuleSpanSource {
            start: offset,
            end: offset + module.source.text.len(),
            source: module.source.clone(),
        });
        public_modules.push(AnalysisModule {
            path: path.clone(),
            source: module.source.clone(),
            cst: module.cst.clone(),
        });
        let imported: BTreeSet<_> = module
            .imports
            .iter()
            .map(|import| ModulePath(import.path.clone()))
            .collect();
        let local_names = declaration_names(&module.program);
        let mut program = module.program.clone();
        rewrite_program(
            &mut program,
            path,
            &imported,
            &local_names,
            &visibility,
            &module.source,
            &mut diagnostics,
            offset,
            &mut next_expression,
        );
        combined.items.extend(
            program
                .items
                .into_iter()
                .filter(|item| !matches!(item, Item::Import(_))),
        );
    }

    let root_cst = modules[&root_path].cst.clone();
    let synthetic = SourceFile::new(root.path.clone(), combined_text);
    analyze_combined(
        root,
        root_cst,
        synthetic,
        combined,
        diagnostics,
        public_modules,
        span_sources,
        options,
    )
}

fn parse_module(source: SourceFile, diagnostics: &mut Vec<Diagnostic>) -> ParsedModule {
    let lexed = lex(&source.path, &source.text);
    let parsed = parse(&source, &lexed);
    diagnostics.extend(lexed.diagnostics);
    diagnostics.extend(parsed.diagnostics);
    let imports = parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Import(import) => Some(import.clone()),
            _ => None,
        })
        .collect();
    ParsedModule {
        source,
        cst: lexed.cst,
        program: parsed.program,
        imports,
    }
}

fn load_imports(
    current: &ModulePath,
    loader: &mut dyn ModuleLoader,
    modules: &mut BTreeMap<ModulePath, ParsedModule>,
    visiting: &mut Vec<ModulePath>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let imports = modules[current].imports.clone();
    let source = modules[current].source.clone();
    let mut seen = BTreeSet::new();
    for import in imports {
        let target = ModulePath(import.path.clone());
        if !seen.insert(target.clone()) {
            diagnostics.push(
                Diagnostic::error(
                    "ALTC3403",
                    format!("module `{target}` is imported more than once"),
                    &source.path,
                    &source.text,
                    import.span,
                )
                .with_help("remove the duplicate import"),
            );
            continue;
        }
        if let Some(cycle_start) = visiting.iter().position(|path| path == &target) {
            let mut cycle = visiting[cycle_start..]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            cycle.push(target.to_string());
            diagnostics.push(
                Diagnostic::error(
                    "ALTC3402",
                    format!("module import cycle: {}", cycle.join(" -> ")),
                    &source.path,
                    &source.text,
                    import.span,
                )
                .with_help("remove one import edge from the cycle"),
            );
            continue;
        }
        if modules.contains_key(&target) {
            continue;
        }
        match loader.load(&target) {
            Ok(loaded) => {
                let module = parse_module(loaded, diagnostics);
                if module.program.items.iter().any(
                    |item| matches!(item, Item::Function(function) if function.attribute("entry").is_some()),
                ) {
                    diagnostics.push(
                        Diagnostic::error(
                            "ALTC3405",
                            "`@entry` is only allowed in the root module",
                            &module.source.path,
                            &module.source.text,
                            module
                                .program
                                .items
                                .iter()
                                .find_map(|item| match item {
                                    Item::Function(function)
                                        if function.attribute("entry").is_some() =>
                                    {
                                        Some(function.span)
                                    }
                                    _ => None,
                                })
                                .unwrap_or_default(),
                        )
                        .with_help("move the entry point to the root file"),
                    );
                }
                modules.insert(target.clone(), module);
                visiting.push(target.clone());
                load_imports(&target, loader, modules, visiting, diagnostics);
                visiting.pop();
            }
            Err(error) => diagnostics.push(
                Diagnostic::error(
                    "ALTC3401",
                    format!("failed to load module `{target}`: {}", error.message),
                    &source.path,
                    &source.text,
                    import.span,
                )
                .with_help(format!(
                    "provide `{}` below the root source directory",
                    target.0.join("/") + ".altc"
                )),
            ),
        }
    }
}

fn collect_visibility(
    modules: &BTreeMap<ModulePath, ParsedModule>,
) -> BTreeMap<(ModulePath, String), DeclarationVisibility> {
    let mut visibility = BTreeMap::new();
    for (path, module) in modules {
        for item in &module.program.items {
            let declaration = match item {
                Item::Function(value) => Some((&value.name, value.public)),
                Item::Struct(value) => Some((&value.name, value.public)),
                Item::Enum(value) => Some((&value.name, value.public)),
                Item::Const(value) => Some((&value.name, false)),
                _ => None,
            };
            if let Some((name, public)) = declaration {
                visibility.insert(
                    (path.clone(), name.clone()),
                    DeclarationVisibility { public },
                );
            }
        }
    }
    visibility
}

fn declaration_names(program: &Program) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for item in &program.items {
        match item {
            Item::Const(value) => {
                names.insert(value.name.clone());
            }
            Item::Function(value) => {
                names.insert(value.name.clone());
            }
            Item::Struct(value) => {
                names.insert(value.name.clone());
            }
            Item::Enum(value) => {
                names.insert(value.name.clone());
            }
            Item::Extern(block) => {
                names.extend(block.functions.iter().map(|value| value.name.clone()));
                names.extend(block.opaque_types.iter().map(|value| value.name.clone()));
                names.extend(block.globals.iter().map(|value| value.name.clone()));
                names.extend(block.structs.iter().map(|value| value.name.clone()));
                names.extend(block.enums.iter().map(|value| value.name.clone()));
            }
            Item::Import(_) | Item::Unsupported { .. } => {}
        }
    }
    names
}

fn mangle(path: &ModulePath, name: &str) -> String {
    if path.0.is_empty() {
        name.to_owned()
    } else {
        format!("altc_mod_{}___{}", path.0.join("__"), name)
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite_path(
    path: &mut Vec<String>,
    span: Span,
    module: &ModulePath,
    imported: &BTreeSet<ModulePath>,
    local_names: &BTreeSet<String>,
    visibility: &BTreeMap<(ModulePath, String), DeclarationVisibility>,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(target) = imported
        .iter()
        .filter(|target| path.len() > target.0.len() && path.starts_with(target.0.as_slice()))
        .max_by_key(|target| target.0.len())
    {
        let symbol = path[target.0.len()].clone();
        if !visibility
            .get(&(target.clone(), symbol.clone()))
            .is_some_and(|declaration| declaration.public)
        {
            diagnostics.push(
                Diagnostic::error(
                    "ALTC3404",
                    format!("`{target}::{symbol}` is private"),
                    &source.path,
                    &source.text,
                    span,
                )
                .with_help(format!("add `pub` to `{symbol}` in module `{target}`")),
            );
        }
        let mut rewritten = vec![mangle(target, &symbol)];
        rewritten.extend(path.iter().skip(target.0.len() + 1).cloned());
        *path = rewritten;
    } else if let Some(first) = path.first_mut() {
        if local_names.contains(first) {
            *first = mangle(module, first);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite_program(
    program: &mut Program,
    module: &ModulePath,
    imported: &BTreeSet<ModulePath>,
    local_names: &BTreeSet<String>,
    visibility: &BTreeMap<(ModulePath, String), DeclarationVisibility>,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
    offset: usize,
    next_expression: &mut ExprId,
) {
    let mut context = RewriteContext {
        module,
        imported,
        local_names,
        visibility,
        source,
        diagnostics,
        offset,
        next_expression,
    };
    for item in &mut program.items {
        context.item(item);
    }
}

struct RewriteContext<'a> {
    module: &'a ModulePath,
    imported: &'a BTreeSet<ModulePath>,
    local_names: &'a BTreeSet<String>,
    visibility: &'a BTreeMap<(ModulePath, String), DeclarationVisibility>,
    source: &'a SourceFile,
    diagnostics: &'a mut Vec<Diagnostic>,
    offset: usize,
    next_expression: &'a mut ExprId,
}

impl RewriteContext<'_> {
    fn span(&self, span: &mut Span) {
        span.start += self.offset;
        span.end += self.offset;
    }

    fn path(&mut self, path: &mut Vec<String>, span: Span) {
        rewrite_path(
            path,
            span,
            self.module,
            self.imported,
            self.local_names,
            self.visibility,
            self.source,
            self.diagnostics,
        );
    }

    fn item(&mut self, item: &mut Item) {
        match item {
            Item::Import(import) => self.span(&mut import.span),
            Item::Const(declaration) => {
                declaration.name = mangle(self.module, &declaration.name);
                self.ty(&mut declaration.ty);
                self.expression(&mut declaration.value);
                self.span(&mut declaration.span);
            }
            Item::Function(function) => self.function(function),
            Item::Struct(declaration) => self.struct_decl(declaration),
            Item::Enum(declaration) => self.enum_decl(declaration),
            Item::Extern(block) => {
                for function in &mut block.functions {
                    self.function(function);
                }
                for opaque in &mut block.opaque_types {
                    opaque.name = mangle(self.module, &opaque.name);
                    self.attributes(&mut opaque.attributes);
                    self.span(&mut opaque.span);
                }
                for global in &mut block.globals {
                    preserve_extern_c_name(
                        &mut global.attributes,
                        &global.name,
                        global.span,
                        self.module,
                    );
                    global.name = mangle(self.module, &global.name);
                    self.attributes(&mut global.attributes);
                    self.ty(&mut global.ty);
                    self.span(&mut global.span);
                }
                for declaration in &mut block.structs {
                    self.struct_decl(declaration);
                }
                for declaration in &mut block.enums {
                    self.enum_decl(declaration);
                }
                self.span(&mut block.span);
            }
            Item::Unsupported { span, .. } => self.span(span),
        }
    }

    fn attributes(&self, attributes: &mut [Attribute]) {
        for attribute in attributes {
            self.span(&mut attribute.span);
        }
    }

    fn function(&mut self, function: &mut Function) {
        if function.is_extern {
            preserve_extern_c_name(
                &mut function.attributes,
                &function.name,
                function.span,
                self.module,
            );
        }
        function.name = mangle(self.module, &function.name);
        if !self.module.0.is_empty() {
            function
                .attributes
                .retain(|attribute| attribute.name != "entry");
        }
        self.attributes(&mut function.attributes);
        for parameter in &mut function.parameters {
            self.ty(&mut parameter.ty);
            self.span(&mut parameter.span);
        }
        self.ty(&mut function.return_type);
        for effect in &mut function.effects {
            self.effect(effect);
        }
        if let Some(body) = &mut function.body {
            self.block(body);
        }
        self.span(&mut function.span);
    }

    fn struct_decl(&mut self, declaration: &mut StructDecl) {
        declaration.name = mangle(self.module, &declaration.name);
        self.attributes(&mut declaration.attributes);
        for field in &mut declaration.fields {
            self.attributes(&mut field.attributes);
            self.ty(&mut field.ty);
            self.span(&mut field.span);
        }
        self.span(&mut declaration.span);
    }

    fn enum_decl(&mut self, declaration: &mut EnumDecl) {
        declaration.name = mangle(self.module, &declaration.name);
        self.attributes(&mut declaration.attributes);
        for variant in &mut declaration.variants {
            self.attributes(&mut variant.attributes);
            for ty in &mut variant.payload {
                self.ty(ty);
            }
            self.span(&mut variant.span);
        }
        self.span(&mut declaration.span);
    }

    fn effect(&mut self, effect: &mut EffectRef) {
        match effect {
            EffectRef::Alloc(span) | EffectRef::Free(span) => self.span(span),
            EffectRef::Raise(ty, span) => {
                self.ty(ty);
                self.span(span);
            }
        }
    }

    fn ty(&mut self, ty: &mut TypeRef) {
        match ty {
            TypeRef::Named { path, span } => {
                self.path(path, *span);
                self.span(span);
            }
            TypeRef::Pointer { pointee, span, .. } => {
                self.ty(pointee);
                self.span(span);
            }
            TypeRef::Array {
                element,
                length,
                span,
            } => {
                self.ty(element);
                self.expression(length);
                self.span(span);
            }
            TypeRef::Slice { element, span, .. } => {
                self.ty(element);
                self.span(span);
            }
            TypeRef::FunctionPointer {
                parameters,
                return_type,
                effects,
                span,
                ..
            } => {
                for parameter in parameters {
                    self.ty(parameter);
                }
                self.ty(return_type);
                for effect in effects {
                    self.effect(effect);
                }
                self.span(span);
            }
            TypeRef::Unsupported { span, .. } => self.span(span),
        }
    }

    fn block(&mut self, block: &mut Block) {
        for statement in &mut block.statements {
            match statement {
                Stmt::Let {
                    ty, value, span, ..
                } => {
                    if let Some(ty) = ty {
                        self.ty(ty);
                    }
                    self.expression(value);
                    self.span(span);
                }
                Stmt::Const(declaration) => {
                    self.ty(&mut declaration.ty);
                    self.expression(&mut declaration.value);
                    self.span(&mut declaration.span);
                }
                Stmt::Expr(expression) => self.expression(expression),
                Stmt::Defer(block, span) => {
                    self.block(block);
                    self.span(span);
                }
            }
        }
        if let Some(tail) = &mut block.tail {
            self.expression(tail);
        }
        self.span(&mut block.span);
    }

    fn expression(&mut self, expression: &mut Expr) {
        expression.id = *self.next_expression;
        *self.next_expression += 1;
        match &mut expression.kind {
            ExprKind::Literal(_) | ExprKind::Break | ExprKind::Continue | ExprKind::Error => {}
            ExprKind::Path(path) => self.path(path, expression.span),
            ExprKind::Block(block) => self.block(block),
            ExprKind::If {
                condition,
                then_block,
                else_expr,
            } => {
                self.expression(condition);
                self.block(then_block);
                if let Some(else_expr) = else_expr {
                    self.expression(else_expr);
                }
            }
            ExprKind::Match { value, arms } => {
                self.expression(value);
                for arm in arms {
                    self.pattern(&mut arm.pattern);
                    self.expression(&mut arm.value);
                    self.span(&mut arm.span);
                }
            }
            ExprKind::Try { body, arms } => {
                self.block(body);
                for arm in arms {
                    self.pattern(&mut arm.pattern);
                    self.expression(&mut arm.value);
                    self.span(&mut arm.span);
                }
            }
            ExprKind::While { condition, body } => {
                self.expression(condition);
                self.block(body);
            }
            ExprKind::For {
                initializer,
                condition,
                update,
                body,
            } => {
                if let Some(initializer) = initializer {
                    match initializer.as_mut() {
                        ForInitializer::Let {
                            ty, value, span, ..
                        } => {
                            if let Some(ty) = ty {
                                self.ty(ty);
                            }
                            self.expression(value);
                            self.span(span);
                        }
                        ForInitializer::Expr(value) => self.expression(value),
                    }
                }
                if let Some(condition) = condition {
                    self.expression(condition);
                }
                if let Some(update) = update {
                    self.expression(update);
                }
                self.block(body);
            }
            ExprKind::RangeFor {
                binding_span,
                sequence,
                body,
                ..
            } => {
                self.span(binding_span);
                self.expression(sequence);
                self.block(body);
            }
            ExprKind::Call { callee, arguments } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            ExprKind::TypeApply { path, arguments } => {
                self.path(path, expression.span);
                for argument in arguments {
                    self.ty(argument);
                }
            }
            ExprKind::Field { base, .. } => self.expression(base),
            ExprKind::Index { base, index } => {
                self.expression(base);
                self.expression(index);
            }
            ExprKind::Slice { base, start, end } => {
                self.expression(base);
                if let Some(start) = start {
                    self.expression(start);
                }
                if let Some(end) = end {
                    self.expression(end);
                }
            }
            ExprKind::Propagate(value)
            | ExprKind::Unary { value, .. }
            | ExprKind::Cast { value, .. }
            | ExprKind::Raise(value) => self.expression(value),
            ExprKind::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            ExprKind::Assign { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            ExprKind::StructLiteral { path, fields } => {
                self.path(path, expression.span);
                for field in fields {
                    self.expression(&mut field.value);
                    self.span(&mut field.span);
                }
            }
            ExprKind::ArrayLiteral(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            ExprKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
        }
        if let ExprKind::Cast { ty, .. } = &mut expression.kind {
            self.ty(ty);
        }
        self.span(&mut expression.span);
    }

    fn pattern(&mut self, pattern: &mut Pattern) {
        match pattern {
            Pattern::Wildcard(span)
            | Pattern::Bind(_, span)
            | Pattern::Literal(_, span)
            | Pattern::Null(span)
            | Pattern::NonNull(_, span)
            | Pattern::Error(span) => self.span(span),
            Pattern::Path(path, span) => {
                self.path(path, *span);
                self.span(span);
            }
            Pattern::Variant(path, patterns, span) => {
                self.path(path, *span);
                for pattern in patterns {
                    self.pattern(pattern);
                }
                self.span(span);
            }
            Pattern::Struct(path, fields, span) => {
                self.path(path, *span);
                for field in fields {
                    if let Some(pattern) = &mut field.pattern {
                        self.pattern(pattern);
                    }
                    self.span(&mut field.span);
                }
                self.span(span);
            }
        }
    }
}

fn preserve_extern_c_name(
    attributes: &mut Vec<Attribute>,
    original_name: &str,
    span: Span,
    module: &ModulePath,
) {
    if !module.0.is_empty()
        && !attributes
            .iter()
            .any(|attribute| attribute.name == "c_name")
    {
        attributes.push(Attribute {
            name: "c_name".to_owned(),
            arguments: vec![format!("\"{original_name}\"")],
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CodegenOptions, emit_c};

    #[derive(Default)]
    struct MemoryLoader {
        modules: BTreeMap<ModulePath, SourceFile>,
    }

    impl MemoryLoader {
        fn with(mut self, path: &[&str], text: &str) -> Self {
            let module = ModulePath::new(path.iter().copied());
            self.modules.insert(
                module.clone(),
                SourceFile::new(format!("{}.altc", path.join("/")), text),
            );
            self
        }
    }

    impl ModuleLoader for MemoryLoader {
        fn load(&mut self, module: &ModulePath) -> Result<SourceFile, ModuleLoadError> {
            self.modules
                .get(module)
                .cloned()
                .ok_or_else(|| ModuleLoadError::new("not found"))
        }
    }

    #[test]
    fn analyzes_diamond_graph_and_generates_deterministically() {
        let root = SourceFile::new(
            "root.altc",
            "import left; import right; @entry fn main_value() -> c_int { left::value() + right::value() }",
        );
        let mut loader = MemoryLoader::default()
            .with(
                &["left"],
                "import common; pub fn value() -> c_int { common::base() }",
            )
            .with(
                &["right"],
                "import common; pub fn value() -> c_int { common::base() + 1 }",
            )
            .with(&["common"], "pub fn base() -> c_int { 4 }");
        let first = analyze_project(root.clone(), &mut loader, &AnalyzeOptions::default());
        assert!(first.succeeded(), "{:?}", first.diagnostics);
        assert_eq!(first.modules().len(), 4);
        assert_eq!(first.modules()[0].path.to_string(), "");
        let first_c = emit_c(&first, &CodegenOptions::default()).unwrap().source;

        let mut loader = MemoryLoader::default()
            .with(
                &["left"],
                "import common; pub fn value() -> c_int { common::base() }",
            )
            .with(
                &["right"],
                "import common; pub fn value() -> c_int { common::base() + 1 }",
            )
            .with(&["common"], "pub fn base() -> c_int { 4 }");
        let second = analyze_project(root, &mut loader, &AnalyzeOptions::default());
        let second_c = emit_c(&second, &CodegenOptions::default()).unwrap().source;
        assert_eq!(first_c, second_c);
    }

    #[test]
    fn reports_missing_duplicate_cycle_privacy_entry_and_imported_syntax() {
        let root = SourceFile::new(
            "root.altc",
            "import missing; import private; import private; import cycle; import broken; fn use() { private::hidden(); let value = private::make(); value.secret; private::Choice::Hidden(); private::Value { secret: 1, visible: 2, }; }",
        );
        let mut loader = MemoryLoader::default()
            .with(
                &["private"],
                "fn hidden() {} pub struct Value { secret: i32, pub visible: i32, } pub enum Choice { Hidden, pub Visible, } pub fn make() -> Value { Value { secret: 1, visible: 2, } }",
            )
            .with(&["cycle"], "import cycle;")
            .with(
                &["broken"],
                "const BEFORE: i32 = 0; @entry fn imported() -> c_int { 0 } fn {",
            );
        let analysis = analyze_project(root, &mut loader, &AnalyzeOptions::default());
        let codes: BTreeSet<_> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for expected in [
            "ALTC2001", "ALTC3401", "ALTC3402", "ALTC3403", "ALTC3404", "ALTC3405",
        ] {
            assert!(codes.contains(expected), "missing {expected}: {codes:?}");
        }
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ALTC2001" && diagnostic.file == "broken.altc"));
    }

    #[test]
    fn single_file_analysis_rejects_import_without_loader() {
        let analysis = crate::analyze(
            SourceFile::new("root.altc", "import math;"),
            &AnalyzeOptions::default(),
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC3401")
        );
    }

    #[test]
    fn rewrites_the_complete_imported_ast_surface() {
        let root = SourceFile::new("root.altc", "import surface; fn root() { surface::all(); }");
        let mut loader = MemoryLoader::default().with(
            &["surface"],
            r#"
pub struct Pair { pub left: i32, pub right: i32, }
pub enum Choice { pub One(i32), pub None, }
const LIMIT: usize = 2usize;
extern "C" from <stddef.h> { @c_name("external") fn external(); }
pub fn all() {
  let values: [i32; LIMIT] = [1, 2];
  let end: usize = 2usize;
  values[0usize];
  values[0usize..end];
  for (external(); ; external()) { break; };
  match Pair { left: 1, right: 2, } {
    Pair { left: renamed, right, } => renamed + right,
  };
  match Choice::One(1) {
    Choice::One(value) => value,
    Choice::None => 0,
  };
}
fn unsupported_type(value: Generic[i32]) { value; }
trait Unsupported {}
"#,
        );
        let analysis = analyze_project(root, &mut loader, &AnalyzeOptions::default());
        assert_eq!(analysis.modules().len(), 2);
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC4001")
        );
    }

    #[test]
    fn preserves_imported_extern_spelling_and_propagates_effects() {
        let root = SourceFile::new(
            "root.altc",
            "import memory; fn use() -> c_int with { alloc } { memory::allocate() as c_int }",
        );
        let mut loader = MemoryLoader::default().with(
            &["memory"],
            r#"extern "C" from <stdlib.h> {
                 fn malloc(size: usize) -> ?*mut unit with { alloc };
                 @c_name("free") fn release(pointer: *mut unit) with { free };
               }
               pub fn allocate() -> usize with { alloc } {
                 match malloc(1usize) {
                   nonnull(value) => ptr_addr(value),
                   null => 0usize,
                 }
               }"#,
        );
        let analysis = analyze_project(root, &mut loader, &AnalyzeOptions::default());
        assert!(analysis.succeeded(), "{:?}", analysis.diagnostics);
        let function = analysis
            .memory_summaries
            .iter()
            .find(|summary| summary.function == "use")
            .unwrap();
        assert!(function.effects.contains(&crate::Effect::Alloc));
        assert!(
            function
                .transitive
                .iter()
                .any(|via| via.file == "root.altc" && via.call_span.end <= 84)
        );
        let unit = emit_c(&analysis, &CodegenOptions::default()).unwrap();
        let ffi = unit.ffi_check.unwrap();
        assert!(ffi.contains("&malloc"));
        assert!(ffi.contains("&free"));
        assert!(!ffi.contains("&altc_mod_memory___malloc"));
    }

    #[test]
    fn rewrite_context_visits_every_pattern_shape() {
        let module = ModulePath::new(["patterns"]);
        let imported = BTreeSet::new();
        let local_names = BTreeSet::new();
        let visibility = BTreeMap::new();
        let source = SourceFile::new("patterns.altc", "");
        let mut diagnostics = Vec::new();
        let mut next_expression = 0;
        let mut context = RewriteContext {
            module: &module,
            imported: &imported,
            local_names: &local_names,
            visibility: &visibility,
            source: &source,
            diagnostics: &mut diagnostics,
            offset: 4,
            next_expression: &mut next_expression,
        };
        let span = Span::new(0, 0);
        let mut patterns = vec![
            Pattern::Wildcard(span),
            Pattern::Bind("value".to_owned(), span),
            Pattern::Literal(Literal::Bool(true), span),
            Pattern::Null(span),
            Pattern::NonNull("pointer".to_owned(), span),
            Pattern::Error(span),
            Pattern::Path(vec!["Value".to_owned()], span),
            Pattern::Variant(
                vec!["Choice".to_owned(), "Some".to_owned()],
                vec![Pattern::Bind("inside".to_owned(), span)],
                span,
            ),
            Pattern::Struct(
                vec!["Record".to_owned()],
                vec![FieldPattern {
                    name: "field".to_owned(),
                    pattern: Some(Pattern::Bind("renamed".to_owned(), span)),
                    span,
                }],
                span,
            ),
        ];
        for pattern in &mut patterns {
            context.pattern(pattern);
            assert_eq!(pattern.span().start, 4);
        }
    }
}
