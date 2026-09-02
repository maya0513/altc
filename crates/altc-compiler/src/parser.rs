use crate::SourceFile;
use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Lexed, Token, TokenKind};

pub(crate) struct Parsed {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

pub(crate) fn parse(source: &SourceFile, lexed: &Lexed) -> Parsed {
    let tokens = lexed
        .tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .cloned()
        .collect();
    let mut parser = Parser {
        source,
        tokens,
        cursor: 0,
        next_expr_id: 0,
        diagnostics: Vec::new(),
    };
    let program = parser.parse_program();
    Parsed {
        program,
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    next_expr_id: ExprId,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn parse_program(&mut self) -> Program {
        let mut items = Vec::new();
        while !self.at_eof() {
            let attributes = self.parse_attributes();

            if self.at("import") {
                let start = self.bump().span.start;
                let (path, _) = self.parse_path();
                let end = self.expect(";", "expected `;` after import").span.end;
                items.push(Item::Import(ImportDecl {
                    path,
                    span: Span::new(start, end),
                }));
                continue;
            }

            if self.at("trait")
                || self.at("impl")
                || self.at("impl")
                || self.at("type")
                || self.at("test")
            {
                let feature = self.current().text.clone();
                let span = self.consume_unsupported_item(&feature);
                items.push(Item::Unsupported { feature, span });
                continue;
            }

            let public = self.eat("pub").is_some();
            let export_c = if self.eat("export").is_some() {
                let convention =
                    self.expect_kind(TokenKind::String, "expected calling convention string");
                if convention
                    .as_ref()
                    .is_some_and(|token| token.text != "\"C\"")
                {
                    self.unsupported("calling conventions other than C", convention.unwrap().span);
                }
                true
            } else {
                false
            };

            let item = if self.at("fn") {
                self.parse_function(attributes, public, export_c, false, None, true)
                    .map(Item::Function)
            } else if self.at("const") {
                if let Some(attribute) = attributes.first() {
                    self.unsupported("attributes on constants", attribute.span);
                }
                if export_c {
                    self.unsupported("exported C constants", self.current().span);
                }
                self.parse_const().map(Item::Const)
            } else if self.at("struct") {
                self.parse_struct(attributes, public, false)
                    .map(Item::Struct)
            } else if self.at("enum") {
                self.parse_enum(attributes, public, false).map(Item::Enum)
            } else if self.at("extern") && !public && !export_c {
                if let Some(attribute) = attributes.first() {
                    self.unsupported("attributes on extern blocks", attribute.span);
                }
                self.parse_extern_block().map(Item::Extern)
            } else {
                let span = self.current().span;
                self.error("ALTC2001", "expected a top-level declaration", span);
                self.synchronize_item();
                None
            };

            if let Some(item) = item {
                items.push(item);
            }
        }
        Program { items }
    }

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attributes = Vec::new();
        while let Some(at) = self.eat("@") {
            let Some(name) = self.expect_ident("expected attribute name") else {
                continue;
            };
            let mut arguments = Vec::new();
            let mut end = name.span.end;
            if self.eat("(").is_some() {
                while !self.at(")") && !self.at_eof() {
                    if matches!(
                        self.current().kind,
                        TokenKind::Ident | TokenKind::String | TokenKind::Int
                    ) {
                        let token = self.bump();
                        arguments.push(token.text);
                    } else {
                        self.error(
                            "ALTC2002",
                            "expected an attribute argument",
                            self.current().span,
                        );
                        self.bump();
                    }
                    if self.eat(",").is_none() {
                        break;
                    }
                }
                end = self.expect(")", "expected `)` after attribute").span.end;
            }
            attributes.push(Attribute {
                name: name.text,
                arguments,
                span: Span::new(at.span.start, end),
            });
        }
        attributes
    }

    fn parse_function(
        &mut self,
        attributes: Vec<Attribute>,
        public: bool,
        export_c: bool,
        is_extern: bool,
        header: Option<String>,
        body_required: bool,
    ) -> Option<Function> {
        let start = self.expect("fn", "expected `fn`").span.start;
        let name = self.expect_ident("expected function name")?;
        if self.at("[") {
            let span = self.consume_balanced("[", "]");
            self.unsupported("generic functions", span);
        }
        self.expect("(", "expected `(` after function name");
        let mut parameters = Vec::new();
        while !self.at(")") && !self.at_eof() {
            if self.at("...") {
                let span = self.bump().span;
                self.unsupported("variadic functions", span);
                break;
            }
            let Some(parameter_name) = self.expect_ident("expected parameter name") else {
                self.synchronize_list(")");
                break;
            };
            self.expect(":", "expected `:` after parameter name");
            let ty = self.parse_type();
            let span = parameter_name.span.join(ty.span());
            parameters.push(Parameter {
                name: parameter_name.text,
                ty,
                span,
            });
            if self.eat(",").is_none() {
                break;
            }
        }
        self.expect(")", "expected `)` after parameters");
        let return_type = if self.eat("->").is_some() {
            self.parse_type()
        } else {
            TypeRef::Named {
                path: vec!["unit".to_owned()],
                span: name.span,
            }
        };
        let effects = self.parse_effects();
        let body = if self.at("{") {
            Some(self.parse_block())
        } else {
            self.expect(";", "expected function body or `;`");
            None
        };
        if body_required && body.is_none() {
            self.error("ALTC2003", "an altc function must have a body", name.span);
        }
        let end = body
            .as_ref()
            .map_or_else(|| self.previous().span.end, |block| block.span.end);
        Some(Function {
            attributes,
            public,
            export_c,
            name: name.text,
            parameters,
            return_type,
            effects,
            body,
            span: Span::new(start, end),
            is_extern,
            header,
        })
    }

    fn parse_effects(&mut self) -> Vec<EffectRef> {
        let mut effects = Vec::new();
        if self.eat("with").is_none() {
            return effects;
        }
        self.expect("{", "expected `{` after `with`");
        while !self.at("}") && !self.at_eof() {
            let token = self.bump();
            match token.text.as_str() {
                "alloc" => effects.push(EffectRef::Alloc(token.span)),
                "free" => effects.push(EffectRef::Free(token.span)),
                "raise" => {
                    self.expect("[", "expected `[` after `raise`");
                    let ty = self.parse_type();
                    let end = self.expect("]", "expected `]` after error type").span.end;
                    effects.push(EffectRef::Raise(ty, Span::new(token.span.start, end)));
                }
                _ => self.error(
                    "ALTC2004",
                    format!("unknown effect `{}`", token.text),
                    token.span,
                ),
            }
            if self.eat(",").is_none() {
                break;
            }
        }
        self.expect("}", "expected `}` after effect contract");
        effects
    }

    fn parse_const(&mut self) -> Option<ConstDecl> {
        let start = self.expect("const", "expected `const`").span.start;
        let name = self.expect_ident("expected constant name")?;
        self.expect(":", "expected `:` after constant name");
        let ty = self.parse_type();
        self.expect("=", "expected `=` in constant declaration");
        let value = self.parse_expression(true);
        let end = self.expect(";", "expected `;` after constant").span.end;
        Some(ConstDecl {
            name: name.text,
            ty,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_struct(
        &mut self,
        attributes: Vec<Attribute>,
        public: bool,
        is_extern: bool,
    ) -> Option<StructDecl> {
        let start = self.expect("struct", "expected `struct`").span.start;
        let name = self.expect_ident("expected struct name")?;
        if self.at("[") {
            let span = self.consume_balanced("[", "]");
            self.unsupported("generic structs", span);
        }
        self.expect("{", "expected `{` after struct name");
        let mut fields = Vec::new();
        while !self.at("}") && !self.at_eof() {
            let field_attributes = self.parse_attributes();
            if !is_extern && !field_attributes.is_empty() {
                self.unsupported("attributes on struct fields", field_attributes[0].span);
            }
            let field_public = self.eat("pub").is_some();
            let Some(field_name) = self.expect_ident("expected field name") else {
                self.synchronize_list("}");
                continue;
            };
            self.expect(":", "expected `:` after field name");
            let ty = self.parse_type();
            let end = self.expect(",", "expected `,` after field").span.end;
            fields.push(FieldDecl {
                attributes: field_attributes,
                public: field_public,
                name: field_name.text,
                ty,
                span: Span::new(field_name.span.start, end),
            });
        }
        let end = self.expect("}", "expected `}` after struct").span.end;
        Some(StructDecl {
            attributes,
            public,
            name: name.text,
            fields,
            span: Span::new(start, end),
            is_extern,
        })
    }

    fn parse_enum(
        &mut self,
        attributes: Vec<Attribute>,
        public: bool,
        is_extern: bool,
    ) -> Option<EnumDecl> {
        let start = self.expect("enum", "expected `enum`").span.start;
        let name = self.expect_ident("expected enum name")?;
        if self.at("[") {
            let span = self.consume_balanced("[", "]");
            self.unsupported("generic enums", span);
        }
        self.expect("{", "expected `{` after enum name");
        let mut variants = Vec::new();
        while !self.at("}") && !self.at_eof() {
            let attributes = self.parse_attributes();
            let variant_public = self.eat("pub").is_some();
            let Some(variant_name) = self.expect_ident("expected variant name") else {
                self.synchronize_list("}");
                continue;
            };
            let mut payload = Vec::new();
            if self.eat("(").is_some() {
                while !self.at(")") && !self.at_eof() {
                    payload.push(self.parse_type());
                    if self.eat(",").is_none() {
                        break;
                    }
                }
                self.expect(")", "expected `)` after variant payload");
            }
            let end = self.expect(",", "expected `,` after variant").span.end;
            variants.push(VariantDecl {
                attributes,
                public: variant_public,
                name: variant_name.text,
                payload,
                span: Span::new(variant_name.span.start, end),
            });
        }
        let end = self.expect("}", "expected `}` after enum").span.end;
        Some(EnumDecl {
            attributes,
            public,
            name: name.text,
            variants,
            span: Span::new(start, end),
            is_extern,
        })
    }

    fn parse_extern_block(&mut self) -> Option<ExternBlock> {
        let start = self.expect("extern", "expected `extern`").span.start;
        let convention = self.expect_kind(TokenKind::String, "expected `\"C\"`")?;
        if convention.text != "\"C\"" {
            self.unsupported("calling conventions other than C", convention.span);
        }
        self.expect("from", "expected `from` in extern block");
        let header = if self.current().kind == TokenKind::String {
            self.bump().text
        } else if self.eat("<").is_some() {
            let mut value = String::from("<");
            while !self.at(">") && !self.at_eof() {
                value.push_str(&self.bump().text);
            }
            value.push_str(&self.expect(">", "expected `>` after header name").text);
            value
        } else {
            self.error("ALTC2005", "expected C header name", self.current().span);
            "<invalid.h>".to_owned()
        };
        self.expect("{", "expected `{` in extern block");
        let mut functions = Vec::new();
        let mut opaque_types = Vec::new();
        let mut globals = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        while !self.at("}") && !self.at_eof() {
            let attributes = self.parse_attributes();
            if self.at("fn") {
                if let Some(function) =
                    self.parse_function(attributes, false, false, true, Some(header.clone()), false)
                {
                    functions.push(function);
                }
            } else if self.eat("opaque").is_some() {
                let opaque_start = self.previous().span.start;
                self.expect("type", "expected `type` after `opaque`");
                if let Some(name) = self.expect_ident("expected opaque type name") {
                    let end = self.expect(";", "expected `;` after opaque type").span.end;
                    opaque_types.push(OpaqueType {
                        attributes,
                        name: name.text,
                        span: Span::new(opaque_start, end),
                        header: header.clone(),
                    });
                }
            } else if self.at("var") || self.at("const") {
                let mutable = self.eat("var").is_some();
                if !mutable {
                    self.expect("const", "expected `const`");
                }
                let global_start = self.previous().span.start;
                let Some(name) = self.expect_ident("expected extern object name") else {
                    self.synchronize_list("}");
                    continue;
                };
                self.expect(":", "expected `:` after extern object name");
                let ty = self.parse_type();
                let end = self
                    .expect(";", "expected `;` after extern object")
                    .span
                    .end;
                globals.push(ExternGlobal {
                    attributes,
                    mutable,
                    name: name.text,
                    ty,
                    span: Span::new(global_start, end),
                    header: header.clone(),
                });
            } else if self.at("struct") {
                if let Some(declaration) = self.parse_struct(attributes, false, true) {
                    structs.push(declaration);
                }
            } else if self.at("enum") {
                if let Some(declaration) = self.parse_enum(attributes, false, true) {
                    enums.push(declaration);
                }
            } else {
                let feature = self.current().text.clone();
                let span = self.consume_unsupported_item(&format!("extern {feature}"));
                self.unsupported(format!("extern `{feature}` declarations"), span);
            }
        }
        let end = self.expect("}", "expected `}` after extern block").span.end;
        Some(ExternBlock {
            header,
            functions,
            opaque_types,
            globals,
            structs,
            enums,
            span: Span::new(start, end),
        })
    }

    fn parse_type(&mut self) -> TypeRef {
        let start = self.current().span.start;
        let nullable = self.eat("?").is_some();
        if self.eat("*").is_some() {
            let mutable = if self.eat("mut").is_some() {
                true
            } else {
                self.expect("const", "expected `const` or `mut` after pointer `*`");
                false
            };
            let pointee = self.parse_type();
            return TypeRef::Pointer {
                nullable,
                mutable,
                span: Span::new(start, pointee.span().end),
                pointee: Box::new(pointee),
            };
        }
        if nullable {
            self.error(
                "ALTC2006",
                "`?` is only valid before a pointer type",
                Span::new(start, self.previous().span.end),
            );
        }
        if self.at("[") {
            let open = self.bump();
            if self.eat("]").is_some() {
                let mutable = if self.eat("mut").is_some() {
                    true
                } else {
                    self.expect("const", "expected `const` or `mut` after `[]`");
                    false
                };
                let element = self.parse_type();
                return TypeRef::Slice {
                    mutable,
                    span: Span::new(open.span.start, element.span().end),
                    element: Box::new(element),
                };
            }
            let element = self.parse_type();
            self.expect(";", "expected `;` between array element type and length");
            let length = self.parse_expression(true);
            let end = self.expect("]", "expected `]` after array length").span.end;
            return TypeRef::Array {
                element: Box::new(element),
                length: Box::new(length),
                span: Span::new(open.span.start, end),
            };
        }
        if self.at("extern") || self.at("fn") {
            let extern_c = if self.eat("extern").is_some() {
                let convention = self.expect_kind(
                    TokenKind::String,
                    "expected calling convention after `extern`",
                );
                if convention
                    .as_ref()
                    .is_some_and(|convention| convention.text != "\"C\"")
                {
                    self.unsupported(
                        "function pointer calling conventions other than C",
                        convention.expect("present convention").span,
                    );
                }
                true
            } else {
                false
            };
            self.expect("fn", "expected `fn` in function pointer type");
            self.expect("(", "expected `(` in function pointer type");
            let mut parameters = Vec::new();
            while !self.at(")") && !self.at_eof() {
                parameters.push(self.parse_type());
                if self.eat(",").is_none() {
                    break;
                }
            }
            self.expect(")", "expected `)` after function pointer parameters");
            let return_type = if self.eat("->").is_some() {
                self.parse_type()
            } else {
                TypeRef::Named {
                    path: vec!["unit".to_owned()],
                    span: self.previous().span,
                }
            };
            let effects = self.parse_effects();
            let end = self.previous().span.end.max(return_type.span().end);
            return TypeRef::FunctionPointer {
                extern_c,
                parameters,
                return_type: Box::new(return_type),
                effects,
                span: Span::new(start, end),
            };
        }
        let (path, span) = self.parse_path();
        if self.at("[") {
            let generic_span = self.consume_balanced("[", "]");
            self.unsupported("generic type arguments", generic_span);
            return TypeRef::Unsupported {
                feature: "generic type".to_owned(),
                span: span.join(generic_span),
            };
        }
        TypeRef::Named { path, span }
    }

    fn parse_block(&mut self) -> Block {
        let start = self.expect("{", "expected `{`").span.start;
        let mut statements = Vec::new();
        let mut tail = None;
        while !self.at("}") && !self.at_eof() {
            if self.at("let") {
                statements.push(self.parse_let_statement());
                continue;
            }
            if self.at("const") {
                if let Some(value) = self.parse_const() {
                    statements.push(Stmt::Const(value));
                }
                continue;
            }
            if let Some(defer) = self.eat("defer") {
                let block = self.parse_block();
                let span = Span::new(defer.span.start, block.span.end);
                statements.push(Stmt::Defer(block, span));
                continue;
            }
            let expression = self.parse_expression(true);
            if self.eat(";").is_some() {
                statements.push(Stmt::Expr(expression));
            } else if self.at("}") {
                tail = Some(Box::new(expression));
                break;
            } else {
                self.error(
                    "ALTC2007",
                    "expected `;` or the end of the block",
                    self.current().span,
                );
                statements.push(Stmt::Expr(expression));
                self.synchronize_statement();
            }
        }
        let end = self.expect("}", "expected `}` after block").span.end;
        Block {
            statements,
            tail,
            span: Span::new(start, end),
        }
    }

    fn parse_let_statement(&mut self) -> Stmt {
        let start = self.expect("let", "expected `let`").span.start;
        let mutable = self.eat("mut").is_some();
        let name = self
            .expect_ident("expected binding name")
            .unwrap_or_else(|| Token {
                kind: TokenKind::Ident,
                text: "error".to_owned(),
                span: self.current().span,
            });
        let ty = if self.eat(":").is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect("=", "expected `=` in binding");
        let value = self.parse_expression(true);
        let end = self.expect(";", "expected `;` after binding").span.end;
        Stmt::Let {
            mutable,
            name: name.text,
            ty,
            value,
            span: Span::new(start, end),
        }
    }

    fn parse_expression(&mut self, allow_struct_literal: bool) -> Expr {
        self.parse_expression_bp(0, allow_struct_literal)
    }

    fn parse_expression_bp(&mut self, minimum_bp: u8, allow_struct_literal: bool) -> Expr {
        let mut left = self.parse_prefix(allow_struct_literal);
        loop {
            if self.at("(") {
                let start = left.span.start;
                self.bump();
                let mut arguments = Vec::new();
                while !self.at(")") && !self.at_eof() {
                    arguments.push(self.parse_expression(true));
                    if self.eat(",").is_none() {
                        break;
                    }
                }
                let end = self.expect(")", "expected `)` after arguments").span.end;
                left = self.expr(
                    ExprKind::Call {
                        callee: Box::new(left),
                        arguments,
                    },
                    Span::new(start, end),
                );
                continue;
            }
            if self.at(".") || self.at("->") {
                let start = left.span.start;
                let pointer = self.bump().text == "->";
                let name = self
                    .expect_ident("expected field name")
                    .unwrap_or_else(|| Token {
                        kind: TokenKind::Ident,
                        text: "error".to_owned(),
                        span: self.current().span,
                    });
                let end = name.span.end;
                left = self.expr(
                    ExprKind::Field {
                        base: Box::new(left),
                        name: name.text,
                        pointer,
                    },
                    Span::new(start, end),
                );
                continue;
            }
            if let Some(question) = self.eat("?") {
                let start = left.span.start;
                left = self.expr(
                    ExprKind::Propagate(Box::new(left)),
                    Span::new(start, question.span.end),
                );
                continue;
            }
            if self.at("[") {
                let start = left.span.start;
                self.bump();
                if self.eat("..").is_some() {
                    let end_value = if self.at("]") {
                        None
                    } else {
                        Some(Box::new(self.parse_expression(true)))
                    };
                    let end = self.expect("]", "expected `]` after slice range").span.end;
                    left = self.expr(
                        ExprKind::Slice {
                            base: Box::new(left),
                            start: None,
                            end: end_value,
                        },
                        Span::new(start, end),
                    );
                    continue;
                }
                let first = self.parse_expression(true);
                if self.eat("..").is_some() {
                    let end_value = if self.at("]") {
                        None
                    } else {
                        Some(Box::new(self.parse_expression(true)))
                    };
                    let end = self.expect("]", "expected `]` after slice range").span.end;
                    left = self.expr(
                        ExprKind::Slice {
                            base: Box::new(left),
                            start: Some(Box::new(first)),
                            end: end_value,
                        },
                        Span::new(start, end),
                    );
                } else {
                    let end = self.expect("]", "expected `]` after index").span.end;
                    left = self.expr(
                        ExprKind::Index {
                            base: Box::new(left),
                            index: Box::new(first),
                        },
                        Span::new(start, end),
                    );
                }
                continue;
            }
            if self.at("as") {
                let left_bp = 25;
                if left_bp < minimum_bp {
                    break;
                }
                self.bump();
                let ty = self.parse_type();
                let span = Span::new(left.span.start, ty.span().end);
                left = self.expr(
                    ExprKind::Cast {
                        value: Box::new(left),
                        ty,
                    },
                    span,
                );
                continue;
            }

            let Some((left_bp, right_bp, infix)) = infix_binding_power(&self.current().text) else {
                break;
            };
            if left_bp < minimum_bp {
                break;
            }
            let operator_token = self.bump();
            let right = self.parse_expression_bp(right_bp, allow_struct_literal);
            let span = Span::new(left.span.start, right.span.end);
            left = match infix {
                Infix::Binary(operator) => self.expr(
                    ExprKind::Binary {
                        operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span,
                ),
                Infix::Assign(operator) => self.expr(
                    ExprKind::Assign {
                        operator,
                        target: Box::new(left),
                        value: Box::new(right),
                    },
                    span,
                ),
            };
            if matches!(
                operator_token.text.as_str(),
                "<" | "<=" | ">" | ">=" | "==" | "!="
            ) && matches!(
                self.current().text.as_str(),
                "<" | "<=" | ">" | ">=" | "==" | "!="
            ) {
                self.error(
                    "ALTC2008",
                    "comparison operators cannot be chained",
                    self.current().span,
                );
                break;
            }
        }
        left
    }

    fn parse_prefix(&mut self, allow_struct_literal: bool) -> Expr {
        let token = self.current().clone();
        if let Some(operator) = unary_operator(&token.text) {
            self.bump();
            let value = self.parse_expression_bp(27, allow_struct_literal);
            return self.expr(
                ExprKind::Unary {
                    operator,
                    value: Box::new(value.clone()),
                },
                Span::new(token.span.start, value.span.end),
            );
        }
        match token.text.as_str() {
            "raise" => {
                self.bump();
                let value = self.parse_expression_bp(0, true);
                self.expr(
                    ExprKind::Raise(Box::new(value.clone())),
                    Span::new(token.span.start, value.span.end),
                )
            }
            "return" => {
                self.bump();
                let value = if self.at(";") || self.at("}") {
                    None
                } else {
                    Some(Box::new(self.parse_expression(true)))
                };
                let end = value
                    .as_ref()
                    .map_or(token.span.end, |value| value.span.end);
                self.expr(ExprKind::Return(value), Span::new(token.span.start, end))
            }
            "break" => {
                self.bump();
                self.expr(ExprKind::Break, token.span)
            }
            "continue" => {
                self.bump();
                self.expr(ExprKind::Continue, token.span)
            }
            "if" => self.parse_if(),
            "match" => self.parse_match(),
            "try" => self.parse_try(),
            "while" => self.parse_while(),
            "for" => self.parse_for(),
            "{" => {
                let block = self.parse_block();
                let span = block.span;
                self.expr(ExprKind::Block(block), span)
            }
            "(" => {
                self.bump();
                let expression = self.parse_expression(true);
                self.expect(")", "expected `)` after expression");
                expression
            }
            "[" => {
                let start = self.bump().span.start;
                let mut values = Vec::new();
                while !self.at("]") && !self.at_eof() {
                    values.push(self.parse_expression(true));
                    if self.eat(",").is_none() {
                        break;
                    }
                }
                let end = self
                    .expect("]", "expected `]` after array literal")
                    .span
                    .end;
                self.expr(ExprKind::ArrayLiteral(values), Span::new(start, end))
            }
            "true" | "false" | "null" => {
                self.bump();
                let literal = match token.text.as_str() {
                    "true" => Literal::Bool(true),
                    "false" => Literal::Bool(false),
                    _ => Literal::Null,
                };
                self.expr(ExprKind::Literal(literal), token.span)
            }
            _ if matches!(
                token.kind,
                TokenKind::Int
                    | TokenKind::Float
                    | TokenKind::String
                    | TokenKind::CString
                    | TokenKind::Char
            ) =>
            {
                self.bump();
                let literal = literal_from_token(&token).expect("guarded literal token");
                self.expr(ExprKind::Literal(literal), token.span)
            }
            _ if matches!(token.kind, TokenKind::Byte | TokenKind::ByteString) => {
                self.bump();
                self.unsupported("byte literals and byte strings", token.span);
                self.expr(ExprKind::Error, token.span)
            }
            _ if token.kind == TokenKind::Ident => {
                let (path, mut span) = self.parse_path();
                let type_applied_builtin = path.len() == 1
                    && matches!(
                        path[0].as_str(),
                        "sizeof"
                            | "alignof"
                            | "ptr_cast"
                            | "ptr_cast_mut"
                            | "ptr_as_const"
                            | "offset"
                            | "ptr_addr"
                            | "ptr_from_addr"
                            | "ptr_from_addr_mut"
                            | "slice_from_parts"
                            | "empty_slice"
                            | "empty_slice_mut"
                            | "slice_as_const"
                    );
                if type_applied_builtin && self.at("[") {
                    let generic_start = self.bump().span.start;
                    let mut arguments = Vec::new();
                    while !self.at("]") && !self.at_eof() {
                        arguments.push(self.parse_type());
                        if self.eat(",").is_none() {
                            break;
                        }
                    }
                    let generic_end = self
                        .expect("]", "expected `]` after type arguments")
                        .span
                        .end;
                    let generic_span = Span::new(generic_start, generic_end);
                    span = span.join(generic_span);
                    return self.expr(ExprKind::TypeApply { path, arguments }, span);
                }
                if allow_struct_literal && self.at("{") {
                    self.bump();
                    let mut fields = Vec::new();
                    while !self.at("}") && !self.at_eof() {
                        let Some(name) = self.expect_ident("expected field name") else {
                            self.synchronize_list("}");
                            break;
                        };
                        self.expect(":", "expected `:` after field name");
                        let value = self.parse_expression(true);
                        let end = self
                            .expect(",", "expected `,` after field initializer")
                            .span
                            .end;
                        fields.push(FieldInitializer {
                            name: name.text,
                            value,
                            span: Span::new(name.span.start, end),
                        });
                    }
                    let end = self
                        .expect("}", "expected `}` after struct literal")
                        .span
                        .end;
                    self.expr(
                        ExprKind::StructLiteral { path, fields },
                        Span::new(span.start, end),
                    )
                } else {
                    self.expr(ExprKind::Path(path), span)
                }
            }
            _ => {
                self.error("ALTC2009", "expected an expression", token.span);
                if !self.at_eof() {
                    self.bump();
                }
                self.expr(ExprKind::Error, token.span)
            }
        }
    }

    fn parse_if(&mut self) -> Expr {
        let start = self.expect("if", "expected `if`").span.start;
        let condition = self.parse_expression(false);
        let then_block = self.parse_block();
        let else_expr = if self.eat("else").is_some() {
            if self.at("if") {
                Some(Box::new(self.parse_if()))
            } else {
                let block = self.parse_block();
                let span = block.span;
                Some(Box::new(self.expr(ExprKind::Block(block), span)))
            }
        } else {
            None
        };
        let end = else_expr
            .as_ref()
            .map_or(then_block.span.end, |expression| expression.span.end);
        self.expr(
            ExprKind::If {
                condition: Box::new(condition),
                then_block,
                else_expr,
            },
            Span::new(start, end),
        )
    }

    fn parse_match(&mut self) -> Expr {
        let start = self.expect("match", "expected `match`").span.start;
        let value = self.parse_expression(false);
        let (arms, end) = self.parse_arms();
        self.expr(
            ExprKind::Match {
                value: Box::new(value),
                arms,
            },
            Span::new(start, end),
        )
    }

    fn parse_try(&mut self) -> Expr {
        let start = self.expect("try", "expected `try`").span.start;
        let body = self.parse_block();
        self.expect("catch", "expected `catch` after try block");
        let (arms, end) = self.parse_arms();
        self.expr(ExprKind::Try { body, arms }, Span::new(start, end))
    }

    fn parse_arms(&mut self) -> (Vec<MatchArm>, usize) {
        self.expect("{", "expected `{` before match arms");
        let mut arms = Vec::new();
        while !self.at("}") && !self.at_eof() {
            let pattern = self.parse_pattern();
            self.expect("=>", "expected `=>` after pattern");
            let value = self.parse_expression(true);
            let end = self.expect(",", "expected `,` after match arm").span.end;
            arms.push(MatchArm {
                span: Span::new(pattern.span().start, end),
                pattern,
                value,
            });
        }
        let end = self.expect("}", "expected `}` after match arms").span.end;
        (arms, end)
    }

    fn parse_pattern(&mut self) -> Pattern {
        let token = self.current().clone();
        if token.text == "_" {
            self.bump();
            return Pattern::Wildcard(token.span);
        }
        if token.text == "null" {
            self.bump();
            return Pattern::Null(token.span);
        }
        if token.text == "nonnull" && self.peek_text(1) == Some("(") {
            self.bump();
            self.bump();
            let name = self.expect_ident("expected binding in nonnull pattern");
            let end = self
                .expect(")", "expected `)` after nonnull pattern")
                .span
                .end;
            return Pattern::NonNull(
                name.map_or_else(|| "error".to_owned(), |token| token.text),
                Span::new(token.span.start, end),
            );
        }
        if matches!(token.text.as_str(), "true" | "false") || token.kind == TokenKind::Int {
            self.bump();
            let literal = if token.text == "true" {
                Literal::Bool(true)
            } else if token.text == "false" {
                Literal::Bool(false)
            } else {
                Literal::Int(token.text)
            };
            return Pattern::Literal(literal, token.span);
        }
        if token.kind != TokenKind::Ident {
            self.error("ALTC2010", "expected a pattern", token.span);
            self.bump();
            return Pattern::Error(token.span);
        }
        let (path, path_span) = self.parse_path();
        if self.eat("(").is_some() {
            let mut patterns = Vec::new();
            while !self.at(")") && !self.at_eof() {
                patterns.push(self.parse_pattern());
                if self.eat(",").is_none() {
                    break;
                }
            }
            let end = self
                .expect(")", "expected `)` after variant pattern")
                .span
                .end;
            Pattern::Variant(path, patterns, Span::new(path_span.start, end))
        } else if self.eat("{").is_some() {
            let mut fields = Vec::new();
            while !self.at("}") && !self.at_eof() {
                let Some(name) = self.expect_ident("expected field name") else {
                    self.synchronize_list("}");
                    break;
                };
                let pattern = if self.eat(":").is_some() {
                    Some(self.parse_pattern())
                } else {
                    None
                };
                let end = self
                    .expect(",", "expected `,` after field pattern")
                    .span
                    .end;
                fields.push(FieldPattern {
                    name: name.text,
                    pattern,
                    span: Span::new(name.span.start, end),
                });
            }
            let end = self
                .expect("}", "expected `}` after struct pattern")
                .span
                .end;
            Pattern::Struct(path, fields, Span::new(path_span.start, end))
        } else if path.len() == 1 {
            Pattern::Bind(path[0].clone(), path_span)
        } else {
            Pattern::Path(path, path_span)
        }
    }

    fn parse_while(&mut self) -> Expr {
        let start = self.expect("while", "expected `while`").span.start;
        let condition = self.parse_expression(false);
        let body = self.parse_block();
        let end = body.span.end;
        self.expr(
            ExprKind::While {
                condition: Box::new(condition),
                body,
            },
            Span::new(start, end),
        )
    }

    fn parse_for(&mut self) -> Expr {
        let start = self.expect("for", "expected `for`").span.start;
        if !self.at("(") {
            let binding = self
                .expect_ident("expected range-for binding")
                .unwrap_or_else(|| Token {
                    kind: TokenKind::Ident,
                    text: "error".to_owned(),
                    span: self.current().span,
                });
            self.expect("in", "expected `in` after range-for binding");
            let sequence = self.parse_expression(false);
            let body = self.parse_block();
            return self.expr(
                ExprKind::RangeFor {
                    binding: binding.text,
                    binding_span: binding.span,
                    sequence: Box::new(sequence),
                    body: body.clone(),
                },
                Span::new(start, body.span.end),
            );
        }
        self.bump();
        let initializer = if self.at(";") {
            None
        } else if self.eat("let").is_some() {
            let init_start = self.previous().span.start;
            let mutable = self.eat("mut").is_some();
            let name = self
                .expect_ident("expected for-loop binding")
                .unwrap_or_else(|| Token {
                    kind: TokenKind::Ident,
                    text: "error".to_owned(),
                    span: self.current().span,
                });
            let ty = if self.eat(":").is_some() {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect("=", "expected `=` in for initializer");
            let value = self.parse_expression(true);
            let span = Span::new(init_start, value.span.end);
            Some(Box::new(ForInitializer::Let {
                mutable,
                name: name.text,
                ty,
                value,
                span,
            }))
        } else {
            Some(Box::new(ForInitializer::Expr(self.parse_expression(true))))
        };
        self.expect(";", "expected `;` after for initializer");
        let condition = if self.at(";") {
            None
        } else {
            Some(Box::new(self.parse_expression(true)))
        };
        self.expect(";", "expected `;` after for condition");
        let update = if self.at(")") {
            None
        } else {
            Some(Box::new(self.parse_expression(true)))
        };
        self.expect(")", "expected `)` after for clauses");
        let body = self.parse_block();
        let end = body.span.end;
        self.expr(
            ExprKind::For {
                initializer,
                condition,
                update,
                body,
            },
            Span::new(start, end),
        )
    }

    fn parse_path(&mut self) -> (Vec<String>, Span) {
        let first = self
            .expect_ident("expected identifier")
            .unwrap_or_else(|| Token {
                kind: TokenKind::Ident,
                text: "error".to_owned(),
                span: self.current().span,
            });
        let mut path = vec![first.text];
        let mut end = first.span.end;
        while self.eat("::").is_some() {
            if let Some(segment) = self.expect_ident("expected path segment") {
                end = segment.span.end;
                path.push(segment.text);
            } else {
                break;
            }
        }
        (path, Span::new(first.span.start, end))
    }

    fn expr(&mut self, kind: ExprKind, span: Span) -> Expr {
        let id = self.next_expr_id;
        self.next_expr_id += 1;
        Expr { id, kind, span }
    }

    fn consume_unsupported_item(&mut self, feature: &str) -> Span {
        let start = self.current().span.start;
        let mut braces = 0usize;
        let mut end = self.current().span.end;
        while !self.at_eof() {
            let token = self.bump();
            end = token.span.end;
            match token.text.as_str() {
                "{" => braces += 1,
                "}" if braces > 0 => {
                    braces -= 1;
                    if braces == 0 {
                        break;
                    }
                }
                ";" if braces == 0 => break,
                _ => {}
            }
        }
        let span = Span::new(start, end);
        self.unsupported(feature, span);
        span
    }

    fn consume_balanced(&mut self, open: &str, close: &str) -> Span {
        let start = self.expect(open, format!("expected `{open}`")).span.start;
        let mut depth = 1usize;
        let mut end = start;
        while !self.at_eof() && depth > 0 {
            let token = self.bump();
            end = token.span.end;
            if token.text == open {
                depth += 1;
            } else if token.text == close {
                depth -= 1;
            }
        }
        Span::new(start, end)
    }

    fn synchronize_item(&mut self) {
        while !self.at_eof() {
            if self.at(";") {
                self.bump();
                break;
            }
            if matches!(
                self.current().text.as_str(),
                "fn" | "const" | "struct" | "enum" | "extern" | "pub" | "@"
            ) {
                break;
            }
            self.bump();
        }
    }

    fn synchronize_statement(&mut self) {
        while !self.at_eof() && !self.at(";") && !self.at("}") {
            self.bump();
        }
        self.eat(";");
    }

    fn synchronize_list(&mut self, close: &str) {
        while !self.at_eof() && !self.at(",") && !self.at(close) {
            self.bump();
        }
        self.eat(",");
    }

    fn expect_ident(&mut self, message: &str) -> Option<Token> {
        if self.current().kind == TokenKind::Ident {
            Some(self.bump())
        } else {
            self.error("ALTC2011", message, self.current().span);
            None
        }
    }

    fn expect_kind(&mut self, kind: TokenKind, message: &str) -> Option<Token> {
        if self.current().kind == kind {
            Some(self.bump())
        } else {
            self.error("ALTC2012", message, self.current().span);
            None
        }
    }

    fn expect(&mut self, text: &str, message: impl Into<String>) -> Token {
        if self.at(text) {
            self.bump()
        } else {
            let span = self.current().span;
            self.error("ALTC2013", message, span);
            Token {
                kind: TokenKind::Error,
                text: text.to_owned(),
                span: Span::new(span.start, span.start),
            }
        }
    }

    fn unsupported(&mut self, feature: impl AsRef<str>, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                "ALTC4001",
                format!("{} are not implemented in executable v0", feature.as_ref()),
                &self.source.path,
                &self.source.text,
                span,
            )
            .with_help(
                "use the implemented language subset documented in docs/language.md#現在対象外",
            ),
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

    fn at(&self, text: &str) -> bool {
        self.current().text == text
    }

    fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn eat(&mut self, text: &str) -> Option<Token> {
        self.at(text).then(|| self.bump())
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if !self.at_eof() {
            self.cursor += 1;
        }
        token
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.cursor.saturating_sub(1)]
    }

    fn peek_text(&self, offset: usize) -> Option<&str> {
        self.tokens
            .get(self.cursor + offset)
            .map(|token| token.text.as_str())
    }
}

#[derive(Clone, Copy)]
enum Infix {
    Binary(BinaryOp),
    Assign(AssignOp),
}

fn infix_binding_power(operator: &str) -> Option<(u8, u8, Infix)> {
    let value = match operator {
        "=" => (1, 1, Infix::Assign(AssignOp::Assign)),
        "+=" => (1, 1, Infix::Assign(AssignOp::Add)),
        "-=" => (1, 1, Infix::Assign(AssignOp::Subtract)),
        "*=" => (1, 1, Infix::Assign(AssignOp::Multiply)),
        "/=" => (1, 1, Infix::Assign(AssignOp::Divide)),
        "+%=" => (1, 1, Infix::Assign(AssignOp::WrapAdd)),
        "-%=" => (1, 1, Infix::Assign(AssignOp::WrapSubtract)),
        "*%=" => (1, 1, Infix::Assign(AssignOp::WrapMultiply)),
        "+|=" => (1, 1, Infix::Assign(AssignOp::SaturatingAdd)),
        "-|=" => (1, 1, Infix::Assign(AssignOp::SaturatingSubtract)),
        "*|=" => (1, 1, Infix::Assign(AssignOp::SaturatingMultiply)),
        "||" => (2, 3, Infix::Binary(BinaryOp::LogicalOr)),
        "&&" => (4, 5, Infix::Binary(BinaryOp::LogicalAnd)),
        "|" => (6, 7, Infix::Binary(BinaryOp::BitOr)),
        "^" => (8, 9, Infix::Binary(BinaryOp::BitXor)),
        "&" => (10, 11, Infix::Binary(BinaryOp::BitAnd)),
        "==" => (12, 13, Infix::Binary(BinaryOp::Equal)),
        "!=" => (12, 13, Infix::Binary(BinaryOp::NotEqual)),
        "<" => (14, 15, Infix::Binary(BinaryOp::Less)),
        "<=" => (14, 15, Infix::Binary(BinaryOp::LessEqual)),
        ">" => (14, 15, Infix::Binary(BinaryOp::Greater)),
        ">=" => (14, 15, Infix::Binary(BinaryOp::GreaterEqual)),
        "<<" => (16, 17, Infix::Binary(BinaryOp::ShiftLeft)),
        ">>" => (16, 17, Infix::Binary(BinaryOp::ShiftRight)),
        "+" => (18, 19, Infix::Binary(BinaryOp::Add)),
        "-" => (18, 19, Infix::Binary(BinaryOp::Subtract)),
        "+%" => (18, 19, Infix::Binary(BinaryOp::WrapAdd)),
        "-%" => (18, 19, Infix::Binary(BinaryOp::WrapSubtract)),
        "+|" => (18, 19, Infix::Binary(BinaryOp::SaturatingAdd)),
        "-|" => (18, 19, Infix::Binary(BinaryOp::SaturatingSubtract)),
        "*" => (20, 21, Infix::Binary(BinaryOp::Multiply)),
        "/" => (20, 21, Infix::Binary(BinaryOp::Divide)),
        "%" => (20, 21, Infix::Binary(BinaryOp::Remainder)),
        "*%" => (20, 21, Infix::Binary(BinaryOp::WrapMultiply)),
        "*|" => (20, 21, Infix::Binary(BinaryOp::SaturatingMultiply)),
        _ => return None,
    };
    Some(value)
}

fn unary_operator(operator: &str) -> Option<UnaryOp> {
    match operator {
        "!" => Some(UnaryOp::Not),
        "~" => Some(UnaryOp::BitNot),
        "+" => Some(UnaryOp::Plus),
        "-" => Some(UnaryOp::Negate),
        "&" => Some(UnaryOp::Address),
        "*" => Some(UnaryOp::Dereference),
        _ => None,
    }
}

fn literal_from_token(token: &Token) -> Option<Literal> {
    match token.kind {
        TokenKind::Int => Some(Literal::Int(token.text.clone())),
        TokenKind::Float => Some(Literal::Float(token.text.clone())),
        TokenKind::String => Some(Literal::String(token.text.clone())),
        TokenKind::CString => Some(Literal::CString(token.text.clone())),
        TokenKind::Char => Some(Literal::Char(token.text.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{SourceFile, lex};

    use super::*;

    #[test]
    fn parses_precedence_and_block_tail() {
        let source = SourceFile::new(
            "test.altc",
            "const BEFORE: i32 = 0; fn f() -> i32 { 1 + 2 * 3 }",
        );
        let lexed = lex(&source.path, &source.text);
        let parsed = parse(&source, &lexed);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let function = parsed
            .program
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) => Some(function),
                _ => None,
            })
            .unwrap();
        let expression = function.body.as_ref().unwrap().tail.as_ref().unwrap();
        let other = Expr {
            id: 999,
            kind: ExprKind::Literal(Literal::Bool(false)),
            span: Span::default(),
        };
        let expressions = [&other, expression];
        let right = expressions
            .iter()
            .find_map(|expression| match &expression.kind {
                ExprKind::Binary { right, .. } => Some(right),
                _ => None,
            })
            .unwrap();
        assert!(matches!(right.kind, ExprKind::Binary { .. }));
        assert!(literal_from_token(&lexed.tokens[0]).is_none());
    }

    #[test]
    fn reports_unsupported_generics_distinctly() {
        let source = SourceFile::new("test.altc", "fn identity[T](x: T) -> T { x }");
        let lexed = lex(&source.path, &source.text);
        let parsed = parse(&source, &lexed);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ALTC4001")
        );
    }

    #[test]
    fn recovers_across_statements_and_top_level_items() {
        let source = SourceFile::new(
            "test.altc",
            "fn first() -> i32 { let value: i32 = ; 1 }\nfn second() -> i32 { 2 + ; 3 }\nfn valid() -> i32 { 4 }",
        );
        let lexed = lex(&source.path, &source.text);
        let parsed = parse(&source, &lexed);
        assert!(parsed.diagnostics.len() >= 2, "{:?}", parsed.diagnostics);
        assert!(
            parsed
                .program
                .items
                .iter()
                .any(|item| matches!(item, Item::Function(function) if function.name == "valid"))
        );
    }

    #[test]
    fn recognizes_the_complete_v0_surface() {
        let source = SourceFile::new(
            "surface.altc",
            r#"
@repr(C) pub struct Pair { pub left: i32, right: i32, }
enum Value { None, One(i32), Pair(i32, i32), }
@repr(C) enum Flag { Off, On, }
const ANSWER: i32 = 42;
extern "C" from <stddef.h> {
  @c_typedef("size_t") opaque type CSize;
  @c_name("puts") fn output(value: *const c_char) -> c_int;
}
fn all(mut_pointer: *mut Pair, nullable: ?*const i32) -> i32 with { alloc, free, raise[Value] } {
  const LOCAL: i32 = 1;
  let mut total: i32 = +LOCAL;
  let pair: Pair = Pair { left: 1, right: 2, };
  total += pair.left;
  total = if !false { total } else if true { 0 } else { -1 };
  while total < 4 { total += 1; continue; };
  for (let mut index: i32 = 0; index < 2; index += 1) {
    if index == 1 { break; };
  };
  for (total = 0; ; total += 1) { break; };
  for (;;) { break; };
  match nullable { nonnull(value) => *value, null => total, };
  match Value::Pair(1, 2) {
    Value::Pair(first, second) => first + second,
    Value::One(value) => value,
    Value::None => 0,
  };
  match pair { Pair { left, right: renamed, } => left + renamed, };
  defer { total = 0; }
  let address: usize = ptr_addr[Pair](mut_pointer);
  let again: *mut Pair = ptr_from_addr_mut[Pair](address);
  (*again).left = total;
  again->right = total;
  output(c"ok")?;
  raise Value::None;
  total as i32
}
"#,
        );
        let lexed = lex(&source.path, &source.text);
        let parsed = parse(&source, &lexed);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.items.len(), 6);
    }

    #[test]
    fn reports_each_unsupported_and_recovery_family_without_panicking() {
        let cases = [
            "import thing; trait T {} impl T {} type Alias = i32; test sample {}",
            "@ fn broken() {} @attr(!) const X: i32 = 1;",
            "export \"Rust\" fn f(); export \"C\" const X: i32 = 1;",
            "fn generic[T](x: [i32], callback: extern fn(i32)) -> Box[i32];",
            "fn variadic(first: i32, ...) {} fn missing_body();",
            "struct Generic[T] { @field pub value i32 } enum E[T] { @x pub V(i32 i32) }",
            "extern \"Rust\" from bad { variable x; opaque nope; }",
            "fn malformed(123, ok i32) with nope { let = ; 1 + ; }",
            "fn bytes() { b'x'; b\"bytes\"; unknown[i32](1); }",
            "fn patterns(value: i32) { match value { @ => 0 missing } }",
            "fn nullable(value: ?i32) { value; }",
            "nonsense here; fn recovered() {}",
        ];
        for text in cases {
            let source = SourceFile::new("errors.altc", text);
            let lexed = lex(&source.path, &source.text);
            let parsed = parse(&source, &lexed);
            assert!(
                !parsed.diagnostics.is_empty(),
                "case unexpectedly valid: {text}"
            );
        }
    }

    #[test]
    fn maps_every_operator_and_rejects_unknown_operators() {
        let infix = [
            "=", "+=", "-=", "*=", "/=", "+%=", "-%=", "*%=", "+|=", "-|=", "*|=", "||", "&&", "|",
            "^", "&", "==", "!=", "<", "<=", ">", ">=", "<<", ">>", "+", "-", "+%", "-%", "+|",
            "-|", "*", "/", "%", "*%", "*|",
        ];
        assert!(
            infix
                .iter()
                .all(|operator| infix_binding_power(operator).is_some())
        );
        assert!(infix_binding_power("???").is_none());

        for operator in ["!", "~", "+", "-", "&", "*"] {
            assert!(unary_operator(operator).is_some());
        }
        assert!(unary_operator("?").is_none());
    }

    #[test]
    fn covers_targeted_recovery_edges_and_nested_delimiters() {
        let cases = [
            "@attribute extern \"C\" from <x.h> {}",
            "@attribute(one, two,) extern \"C\" from <x.h> {}",
            "extern C from <x.h> {}",
            "fn effect() with { mystery } {}",
            "fn generic_type(value: Vector[i32]) {}",
            "struct Fields { 123 bad, valid: i32, }",
            "enum Variants { Value(i32 i64), }",
            "fn statements() { let 123 = 1; value.; value[0]; 1 < 2 < 3; }",
            "fn generic_arguments() { builtin[i32, u32]; }",
            "struct S { value: i32, } fn literal() { S { 123 bad, }; }",
            "fn patterns(x: i32) { match x { true => 1, false => 0, 1 => 1, E(1 2) => 0, S { 123, } => 0, } }",
            "fn loop() { for (let 123 = 0; true; value) {} }",
            "fn paths(value: *) { value::; }",
            "fn nested[T[U]]() {}",
            "fn unfinished() { for value in sequence",
            "nonsense fn recovered() {}",
            "extern \"C\" from <x.h> { var 123; const 456; }",
            "fn callback(value: extern \"Rust\" fn(i32, u32)) {}",
            "fn bad_range() { for 123 in values {} }",
            "extern \"C\" from <x.h> { macro value { nested { item; } } }",
        ];
        for text in cases {
            let source = SourceFile::new("edges.altc", text);
            let lexed = lex(&source.path, &source.text);
            let parsed = parse(&source, &lexed);
            assert!(
                !parsed.diagnostics.is_empty(),
                "case unexpectedly valid: {text}"
            );
        }
        let source = SourceFile::new(
            "edges.altc",
            "fn slices(values: [i32; 2usize], end: usize) { values[..end]; sizeof[i32, u32](); }",
        );
        let lexed = lex(&source.path, &source.text);
        let parsed = parse(&source, &lexed);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }
}
