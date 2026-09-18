use crate::{
    ast::*,
    lexer::{Token, TokenKind, lex},
};
use zen_diagnostics::{Diagnostic, FileId, Span};
type PResult<T> = Result<T, Box<Diagnostic>>;
/// Concrete layout facts recorded by the compiler parser, never guessed from names.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub generics: Vec<Span>,
    pub fields: Vec<Span>,
    pub parentheses: Vec<Span>,
    pub binary: Vec<Span>,
    pub members: Vec<Span>,
    pub lambdas: Vec<Span>,
    pub unary: Vec<Span>,
    pub boundaries: Vec<(usize, BoundaryKind)>,
}
#[derive(Clone, Copy, Debug)]
pub enum BoundaryKind {
    Declaration,
    Definition,
    Statement,
    BlockArm,
}
pub struct ParsedSource {
    pub module: Module,
    pub tokens: Vec<Token>,
    pub layout: Layout,
    pub diagnostics: Vec<Diagnostic>,
}
pub fn parse(file: FileId, source: &str) -> (Module, Vec<Diagnostic>) {
    let parsed = parse_source(file, source);
    (parsed.module, parsed.diagnostics)
}
pub fn parse_source(file: FileId, source: &str) -> ParsedSource {
    let (tokens, diagnostics) = lex(file, source);
    let mut p = Parser {
        tokens,
        pos: 0,
        next_id: 0,
        diagnostics,
        depth: 0,
        layout: Layout::default(),
    };
    let mut module = Module::default();
    while !p.eof() {
        let start = p.pos;
        let result = if p.at("import") {
            if !module.declarations.is_empty() {
                p.diagnostics.push(Diagnostic::error(
                    "ZEN-PARSE-0001",
                    p.token().span,
                    "imports must precede declarations",
                ));
            }
            p.import().map(|i| module.imports.push(i))
        } else {
            p.decl().map(|d| module.declarations.push(d))
        };
        if let Err(e) = result {
            p.diagnostics.push(*e);
            p.recover(true);
        }
        if p.pos == start {
            p.bump();
        }
    }
    ParsedSource {
        module,
        tokens: p.tokens,
        layout: p.layout,
        diagnostics: p.diagnostics,
    }
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_id: usize,
    diagnostics: Vec<Diagnostic>,
    depth: usize,
    layout: Layout,
}
impl Parser {
    fn token(&self) -> &Token {
        &self.tokens[self.pos]
    }
    fn at(&self, s: &str) -> bool {
        self.token().text == s
    }
    fn eof(&self) -> bool {
        self.token().kind == TokenKind::Eof
    }
    fn bump(&mut self) -> Token {
        let t = self.token().clone();
        if !self.eof() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.at(s) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn err(&self, msg: impl Into<String>) -> Box<Diagnostic> {
        Box::new(Diagnostic::error("ZEN-PARSE-0001", self.token().span, msg))
    }
    fn expect(&mut self, s: &str) -> PResult<Token> {
        if self.at(s) {
            Ok(self.bump())
        } else {
            Err(self.err(format!("expected `{s}`, found `{}`", self.token().text)))
        }
    }
    fn name(&mut self) -> PResult<Name> {
        if self.token().kind == TokenKind::Ident {
            let t = self.bump();
            Ok(Name {
                text: t.text,
                span: t.span,
            })
        } else {
            Err(self.err("expected an identifier"))
        }
    }
    fn previous(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }
    fn node(&mut self, start: Span, kind: ExprKind) -> Expr {
        let id = ExprId(self.next_id);
        self.next_id += 1;
        Expr {
            id,
            span: start.join(self.previous()),
            kind,
        }
    }
    fn recover(&mut self, top: bool) {
        while !self.eof() {
            if self.eat(";") {
                return;
            }
            if self.at("}") {
                if top {
                    self.bump();
                }
                return;
            }
            if top
                && [
                    "public",
                    "fn",
                    "async",
                    "native",
                    "struct",
                    "enum",
                    "interface",
                    "impl",
                    "const",
                    "import",
                ]
                .contains(&self.token().text.as_str())
            {
                return;
            }
            self.bump();
        }
    }
    fn import(&mut self) -> PResult<Import> {
        let start = self.expect("import")?.span;
        let mut path = vec![self.name()?];
        while self.eat(".") {
            path.push(self.name()?);
        }
        let alias = if self.eat("as") {
            Some(self.name()?)
        } else {
            None
        };
        let end = self.expect(";")?.span;
        Ok(Import {
            path,
            alias,
            span: start.join(end),
        })
    }
    fn generics(&mut self) -> PResult<Vec<Generic>> {
        if !self.at("<") {
            return Ok(vec![]);
        }
        let start = self.token().span;
        let result = self.generics_inner();
        if result.is_ok() && self.previous().end > start.start {
            self.layout.generics.push(start.join(self.previous()));
        }
        result
    }
    fn generics_inner(&mut self) -> PResult<Vec<Generic>> {
        let mut out = vec![];
        if !self.eat("<") {
            return Ok(out);
        }
        loop {
            let name = self.name()?;
            let mut bounds = vec![];
            if self.eat(":") {
                bounds.push(self.ty()?);
                while self.eat("&") {
                    bounds.push(self.ty()?);
                }
            }
            out.push(Generic { name, bounds });
            if !self.eat(",") || self.at(">") {
                break;
            }
        }
        self.expect(">")?;
        Ok(out)
    }
    fn type_args(&mut self) -> PResult<Vec<TypeRef>> {
        let start = self.token().span;
        let result = self.type_args_inner();
        if result.is_ok() && self.previous().end > start.start {
            self.layout.generics.push(start.join(self.previous()));
        }
        result
    }
    fn type_args_inner(&mut self) -> PResult<Vec<TypeRef>> {
        self.expect("<")?;
        let mut out = vec![self.ty()?];
        while self.eat(",") && !self.at(">") {
            out.push(self.ty()?);
        }
        self.expect(">")?;
        Ok(out)
    }
    fn ty(&mut self) -> PResult<TypeRef> {
        if self.depth > 32 {
            return Err(self.err("syntax nesting limit exceeded"));
        }
        self.depth += 1;
        let r = self.ty_inner();
        self.depth -= 1;
        r
    }
    fn ty_inner(&mut self) -> PResult<TypeRef> {
        let start = self.token().span;
        let kind = if self.eat("(") {
            let mut args = vec![];
            while !self.at(")") {
                args.push(self.ty()?);
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
            self.expect("->")?;
            TypeKind::Function(args, Box::new(self.ty()?))
        } else {
            let mut name = self.name()?.text;
            while self.eat(".") {
                name.push('.');
                name.push_str(&self.name()?.text);
            }
            let args = if self.at("<") {
                self.type_args()?
            } else {
                vec![]
            };
            TypeKind::Named(name, args)
        };
        Ok(TypeRef {
            kind,
            span: start.join(self.previous()),
        })
    }
    fn params(&mut self, lambda: bool) -> PResult<Vec<Param>> {
        self.expect("(")?;
        let mut out = vec![];
        while !self.at(")") {
            let name = if self.at("self") {
                let t = self.bump();
                Name {
                    text: t.text,
                    span: t.span,
                }
            } else {
                self.name()?
            };
            let ty = if self.eat(":") {
                Some(self.ty()?)
            } else if lambda || name.text == "self" {
                None
            } else {
                return Err(self.err("named function parameters require explicit types"));
            };
            let default = if self.eat("=") {
                Some(self.expr(0, true)?)
            } else {
                None
            };
            out.push(Param { name, ty, default });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(out)
    }
    fn function(&mut self, public: bool, signature: bool) -> PResult<Function> {
        let native = self.eat("native");
        let asynchronous = self.eat("async");
        self.expect("fn")?;
        let name = self.name()?;
        let generics = self.generics()?;
        if signature && (native || !generics.is_empty()) {
            return Err(
                self.err("interface methods cannot be native or introduce method type parameters")
            );
        }
        let params = self.params(false)?;
        self.expect("->")?;
        let result = self.ty()?;
        let body = if native || signature {
            self.expect(";")?;
            None
        } else {
            Some(self.block()?)
        };
        self.layout.boundaries.push((
            self.previous().end,
            if body.is_some() {
                BoundaryKind::Definition
            } else {
                BoundaryKind::Statement
            },
        ));
        Ok(Function {
            name,
            generics,
            params,
            result,
            body,
            asynchronous,
            native,
            public,
        })
    }
    fn decl(&mut self) -> PResult<Decl> {
        let start = self.token().span;
        let public = self.eat("public");
        if public && self.at("impl") {
            return Err(self.err("impl blocks do not have a visibility modifier"));
        }
        let kind = if self.eat("struct") {
            let n = self.name()?;
            let g = self.generics()?;
            self.expect("{")?;
            let mut fields = vec![];
            while !self.at("}") && !self.eof() {
                let public = self.eat("public");
                let name = self.name()?;
                self.expect(":")?;
                let ty = self.ty()?;
                self.expect(";")?;
                fields.push(Field { name, ty, public });
            }
            self.expect("}")?;
            DeclKind::Struct(n, g, fields)
        } else if self.eat("enum") {
            let n = self.name()?;
            let g = self.generics()?;
            self.expect("{")?;
            let mut v = vec![];
            while !self.at("}") && !self.eof() {
                let name = self.name()?;
                let mut payload = vec![];
                if self.eat("(") {
                    payload.push(self.ty()?);
                    while self.eat(",") && !self.at(")") {
                        payload.push(self.ty()?);
                    }
                    self.expect(")")?;
                }
                self.expect(";")?;
                v.push(Variant { name, payload });
            }
            self.expect("}")?;
            DeclKind::Enum(n, g, v)
        } else if self.eat("interface") {
            let n = self.name()?;
            let g = self.generics()?;
            self.expect("{")?;
            let mut methods = vec![];
            while !self.at("}") && !self.eof() {
                methods.push(self.function(false, true)?);
            }
            self.expect("}")?;
            DeclKind::Interface(n, g, methods)
        } else if self.eat("impl") {
            let first = self.ty()?;
            let (interface, target) = if self.eat("for") {
                (Some(first), self.ty()?)
            } else {
                (None, first)
            };
            self.expect("{")?;
            let mut methods = vec![];
            while !self.at("}") && !self.eof() {
                let public = self.eat("public");
                methods.push(self.function(public, false)?);
            }
            self.expect("}")?;
            DeclKind::Impl {
                interface,
                target,
                methods,
            }
        } else if self.eat("const") {
            let n = self.name()?;
            self.expect(":")?;
            let t = self.ty()?;
            self.expect("=")?;
            let e = self.expr(0, true)?;
            self.expect(";")?;
            DeclKind::Const(n, t, e)
        } else {
            DeclKind::Function(self.function(public, false)?)
        };
        self.layout
            .boundaries
            .push((self.previous().end, BoundaryKind::Declaration));
        Ok(Decl {
            kind,
            span: start.join(self.previous()),
            public,
        })
    }
    fn block(&mut self) -> PResult<Expr> {
        if self.depth > 32 {
            return Err(self.err("syntax nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.block_inner();
        self.depth -= 1;
        result
    }
    fn block_inner(&mut self) -> PResult<Expr> {
        let start = self.expect("{")?.span;
        let mut stmts = vec![];
        let mut tail = None;
        while !self.at("}") && !self.eof() {
            let before = self.pos;
            match self.statement() {
                Ok((stmt, end)) => {
                    if let Some(e) = end {
                        tail = Some(Box::new(e));
                        break;
                    }
                    if let Some(s) = stmt {
                        stmts.push(s);
                    }
                }
                Err(e) => {
                    self.diagnostics.push(*e);
                    self.recover(false);
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
        if self.eof() {
            let error = self.err("expected `}`");
            self.diagnostics.push(*error);
        } else {
            self.expect("}")?;
        }
        Ok(self.node(start, ExprKind::Block(stmts, tail)))
    }
    fn statement(&mut self) -> PResult<(Option<Stmt>, Option<Expr>)> {
        let start = self.token().span;
        let kind = if self.at("let") || self.at("var") {
            let mutable = self.bump().text == "var";
            let name = self.name()?;
            let ty = if self.eat(":") {
                Some(self.ty()?)
            } else {
                None
            };
            self.expect("=")?;
            let value = self.expr(0, true)?;
            self.expect(";")?;
            StmtKind::Bind {
                name,
                mutable,
                ty,
                value,
            }
        } else if self.eat("return") {
            let e = if self.at(";") {
                None
            } else {
                Some(self.expr(0, true)?)
            };
            self.expect(";")?;
            StmtKind::Return(e)
        } else if self.eat("defer") {
            let e = self.expr(0, true)?;
            self.expect(";")?;
            StmtKind::Defer(e)
        } else if self.eat("break") {
            self.expect(";")?;
            StmtKind::Break
        } else if self.eat("continue") {
            self.expect(";")?;
            StmtKind::Continue
        } else if self.eat("while") {
            let c = self.expr(0, false)?;
            let b = self.block()?;
            StmtKind::While(c, b)
        } else if self.eat("for") {
            let n = self.name()?;
            self.expect("in")?;
            let e = self.expr(0, false)?;
            let b = self.block()?;
            StmtKind::For(n, e, b)
        } else {
            let e = self.expr(0, true)?;
            if self.eat("=") {
                let v = self.expr(0, true)?;
                self.expect(";")?;
                StmtKind::Assign(e, v)
            } else if self.eat(";") || matches!(e.kind, ExprKind::If(_, _, None)) {
                StmtKind::Expr(e)
            } else if self.at("}") {
                return Ok((None, Some(e)));
            } else if matches!(
                e.kind,
                ExprKind::If(..) | ExprKind::Match(..) | ExprKind::Block(..)
            ) {
                StmtKind::Expr(e)
            } else {
                // Retain a parsed expression while the user is still typing its
                // terminator. The next statement remains available for recovery,
                // and IDE queries can still resolve this expression's receiver.
                let error = self.err("expected `;` after expression");
                self.diagnostics.push(*error);
                StmtKind::Expr(e)
            }
        };
        self.layout
            .boundaries
            .push((self.previous().end, BoundaryKind::Statement));
        Ok((
            Some(Stmt {
                span: start.join(self.previous()),
                kind,
            }),
            None,
        ))
    }
    fn expr(&mut self, min: u8, allow_struct: bool) -> PResult<Expr> {
        if self.depth > 32 {
            return Err(self.err("syntax nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.expr_inner(min, allow_struct);
        self.depth -= 1;
        result
    }
    fn expr_inner(&mut self, min: u8, allow_struct: bool) -> PResult<Expr> {
        let start = self.token().span;
        let mut lhs = if self.at("!") || self.at("-") || self.at("await") {
            self.layout.unary.push(self.token().span);
            let op = self.bump().text;
            let rhs = self.expr(if op == "await" { 10 } else { 8 }, allow_struct)?;
            self.node(start, ExprKind::Unary(op, Box::new(rhs)))
        } else {
            self.primary(allow_struct)?
        };
        let mut chain = 0;
        loop {
            chain += 1;
            if chain > 32 {
                return Err(self.err("expression nesting limit exceeded"));
            }
            if self.at("(") && 11 >= min {
                let args = self.args()?;
                lhs = self.node(start, ExprKind::Call(Box::new(lhs), args));
                continue;
            }
            if self.eat_if(".", 11, min) {
                self.layout.members.push(self.previous());
                let name = self.name()?;
                let types = self.expression_types();
                lhs = self.node(start, ExprKind::Member(Box::new(lhs), name, types));
                continue;
            }
            if self.eat_if("?", 9, min) {
                lhs = self.node(start, ExprKind::Unary("?".into(), Box::new(lhs)));
                continue;
            }
            if self.at("with") && min == 0 {
                self.bump();
                let fields = self.initializers()?;
                if fields.is_empty() {
                    return Err(self.err("struct update requires at least one field"));
                }
                lhs = self.node(start, ExprKind::Update(Box::new(lhs), fields));
                continue;
            }
            let power = match self.token().text.as_str() {
                "||" => 1,
                "&&" => 2,
                "==" | "!=" => 3,
                "<" | "<=" | ">" | ">=" => 4,
                "+" | "-" => 5,
                "*" => 6,
                _ => 0,
            };
            if power == 0 || power < min {
                break;
            }
            self.layout.binary.push(self.token().span);
            let op = self.bump().text;
            let rhs = self.expr(power + 1, allow_struct)?;
            if power == 4
                && matches!(&lhs.kind,ExprKind::Binary(o,..) if ["<","<=",">",">="].contains(&o.as_str()))
            {
                return Err(Box::new(Diagnostic::error(
                    "ZEN-PARSE-0002",
                    lhs.span,
                    "comparison operators cannot be chained",
                )));
            }
            lhs = self.node(start, ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }
    fn eat_if(&mut self, s: &str, p: u8, min: u8) -> bool {
        p >= min && self.eat(s)
    }
    // A generic argument list in expression position must have an unambiguous continuation.
    fn expression_types(&mut self) -> Vec<TypeRef> {
        if !self.at("<") {
            return vec![];
        }
        let saved = self.pos;
        let layout_saved = self.layout.generics.len();
        match self.type_args() {
            Ok(args)
                if ["(", ".", "{", ";", ",", ")", "]", "}"]
                    .iter()
                    .any(|s| self.at(s)) =>
            {
                args
            }
            _ => {
                self.pos = saved;
                self.layout.generics.truncate(layout_saved);
                vec![]
            }
        }
    }
    fn args(&mut self) -> PResult<Vec<Arg>> {
        self.expect("(")?;
        let mut out = vec![];
        while !self.at(")") {
            let label = if self.token().kind == TokenKind::Ident
                && self.tokens.get(self.pos + 1).is_some_and(|t| t.text == ":")
            {
                let n = self.name()?;
                self.bump();
                Some(n)
            } else {
                None
            };
            let value = self.expr(0, true)?;
            out.push(Arg { label, value });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(out)
    }
    fn initializers(&mut self) -> PResult<Vec<(Name, Expr)>> {
        let start = self.token().span;
        let result = self.initializers_inner();
        if result.is_ok() && self.previous().end > start.start {
            self.layout.fields.push(start.join(self.previous()));
        }
        result
    }
    fn initializers_inner(&mut self) -> PResult<Vec<(Name, Expr)>> {
        self.expect("{")?;
        let mut out = vec![];
        while !self.at("}") {
            let n = self.name()?;
            self.expect(":")?;
            let e = self.expr(0, true)?;
            out.push((n, e));
            if !self.eat(",") {
                break;
            }
        }
        self.expect("}")?;
        Ok(out)
    }
    fn is_lambda(&self) -> bool {
        if !self.at("(") {
            return false;
        }
        let mut depth = 0;
        for i in self.pos..self.tokens.len() {
            match self.tokens[i].text.as_str() {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        return self
                            .tokens
                            .get(i + 1)
                            .is_some_and(|t| t.text == "->" || t.text == "{");
                    }
                }
                _ => {}
            }
        }
        false
    }
    fn primary(&mut self, allow_struct: bool) -> PResult<Expr> {
        let start = self.token().span;
        if self.at("{") {
            return self.block();
        }
        if self.eat("if") {
            let c = self.expr(0, false)?;
            let a = self.block()?;
            let b = if self.eat("else") {
                Some(Box::new(if self.at("if") {
                    self.primary(true)?
                } else {
                    self.block()?
                }))
            } else {
                None
            };
            return Ok(self.node(start, ExprKind::If(Box::new(c), Box::new(a), b)));
        }
        if self.eat("match") {
            let e = self.expr(0, false)?;
            self.expect("{")?;
            let mut arms = vec![];
            while !self.at("}") && !self.eof() {
                let p = self.pattern()?;
                self.expect("=>")?;
                let a = self.expr(0, true)?;
                self.expect(";")?;
                self.layout.boundaries.push((
                    self.previous().end,
                    if matches!(a.kind, ExprKind::Block(..)) {
                        BoundaryKind::BlockArm
                    } else {
                        BoundaryKind::Statement
                    },
                ));
                arms.push((p, a));
            }
            self.expect("}")?;
            return Ok(self.node(start, ExprKind::Match(Box::new(e), arms)));
        }
        if self.at("async") || self.is_lambda() {
            let asynchronous = self.eat("async");
            let params = self.params(true)?;
            let result = if self.eat("->") {
                Some(self.ty()?)
            } else {
                None
            };
            let body = Box::new(self.block()?);
            self.layout.lambdas.push(start.join(self.previous()));
            return Ok(self.node(
                start,
                ExprKind::Lambda {
                    params,
                    result,
                    body,
                    asynchronous,
                },
            ));
        }
        if self.eat("(") {
            let e = self.expr(0, true)?;
            self.expect(")")?;
            self.layout.parentheses.push(start.join(self.previous()));
            return Ok(self.node(start, e.kind));
        }
        if self.eat("[") {
            let mut values = vec![];
            while !self.at("]") {
                values.push(self.expr(0, true)?);
                if !self.eat(",") {
                    break;
                }
            }
            self.expect("]")?;
            return Ok(self.node(start, ExprKind::List(values)));
        }
        if self.eat(".") {
            let n = self.name()?;
            return Ok(self.node(start, ExprKind::Contextual(n)));
        }
        if let Some(l) = self.literal() {
            return Ok(self.node(start, ExprKind::Literal(l)));
        }
        if self.token().kind == TokenKind::Ident || self.at("self") {
            let t = self.bump();
            let name = Name {
                text: t.text,
                span: t.span,
            };
            let args = self.expression_types();
            if allow_struct && self.at("{") {
                let ty = TypeRef {
                    kind: TypeKind::Named(name.text, args),
                    span: start.join(self.previous()),
                };
                let fields = self.initializers()?;
                return Ok(self.node(start, ExprKind::Struct(ty, fields)));
            }
            return Ok(self.node(start, ExprKind::Name(name, args)));
        }
        Err(self.err("expected an expression"))
    }
    fn literal(&mut self) -> Option<Literal> {
        let t = self.token();
        let l = match t.kind {
            TokenKind::Integer => Literal::Integer(t.text.clone()),
            TokenKind::Float => Literal::Float(t.text.clone()),
            TokenKind::String => Literal::String(t.text.clone()),
            TokenKind::Char => Literal::Char(t.text.chars().next().unwrap_or('\0')),
            _ => match t.text.as_str() {
                "true" => Literal::Bool(true),
                "false" => Literal::Bool(false),
                "unit" => Literal::Unit,
                _ => return None,
            },
        };
        self.bump();
        Some(l)
    }
    fn pattern(&mut self) -> PResult<Pattern> {
        if self.depth > 32 {
            return Err(self.err("pattern nesting limit exceeded"));
        }
        self.depth += 1;
        let r = self.pattern_inner();
        self.depth -= 1;
        r
    }
    fn pattern_inner(&mut self) -> PResult<Pattern> {
        let start = self.token().span;
        let kind = if self.eat("_") {
            PatternKind::Wildcard
        } else if let Some(l) = self.literal() {
            PatternKind::Literal(l)
        } else if self.eat(".") {
            let n = self.name()?;
            let p = self.pattern_payload()?;
            PatternKind::Variant(None, n, p)
        } else {
            let n = self.name()?;
            if self.eat(".") {
                let mut path = n.text;
                let mut variant = self.name()?;
                while self.eat(".") {
                    path.push('.');
                    path.push_str(&variant.text);
                    variant = self.name()?;
                }
                let p = self.pattern_payload()?;
                PatternKind::Variant(Some(path), variant, p)
            } else {
                PatternKind::Bind(n)
            }
        };
        Ok(Pattern {
            span: start.join(self.previous()),
            kind,
        })
    }
    fn pattern_payload(&mut self) -> PResult<Vec<Pattern>> {
        let mut out = vec![];
        if self.eat("(") {
            out.push(self.pattern()?);
            while self.eat(",") && !self.at(")") {
                out.push(self.pattern()?);
            }
            self.expect(")")?;
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn body(s: &str) -> Expr {
        let (m, d) = parse(FileId(0), s);
        assert!(d.is_empty(), "{d:?}");
        let DeclKind::Function(f) = m.declarations.into_iter().next().unwrap().kind else {
            panic!()
        };
        f.body.unwrap()
    }
    #[test]
    fn precedence() {
        let b = body("fn f() -> Unit { 1 + 2 * 3; }");
        let ExprKind::Block(s, _) = b.kind else {
            panic!()
        };
        let StmtKind::Expr(e) = &s[0].kind else {
            panic!()
        };
        assert!(
            matches!(&e.kind,ExprKind::Binary(o,_,r) if o=="+" && matches!(&r.kind,ExprKind::Binary(o,..) if o=="*"))
        );
    }
    #[test]
    fn await_then_propagate() {
        let b = body("async fn f() -> Unit { await load()?; }");
        let ExprKind::Block(s, _) = b.kind else {
            panic!()
        };
        let StmtKind::Expr(e) = &s[0].kind else {
            panic!()
        };
        assert!(
            matches!(&e.kind,ExprKind::Unary(o,e) if o=="?" && matches!(&e.kind,ExprKind::Unary(o,_) if o=="await"))
        );
    }
    #[test]
    fn recovers() {
        let (_, d) = parse(FileId(0), "fn f() -> Unit { let a = ; let b = ; }");
        assert_eq!(d.len(), 2);
    }
}
