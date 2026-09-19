use super::*;

impl Checker {
    pub(super) fn record(&mut self, e: &Expr, ty: Type, symbol: Option<SymbolId>) -> Type {
        if let Some(id) = symbol {
            let span = match &e.kind {
                ExprKind::Name(n, _) | ExprKind::Member(_, n, _) => n.span,
                _ => e.span,
            };
            self.ide.reference(span, Entity::Value(id));
        }
        let key = (e.span.file, e.id.0);
        if let Some(&index) = self.expression_index.get(&key) {
            let existing = &mut self.expressions[index];
            existing.ty = ty.clone();
            if symbol.is_some() {
                existing.symbol = symbol;
            }
        } else {
            self.expression_index.insert(key, self.expressions.len());
            self.expressions.push(TypedExpr {
                id: e.id,
                span: e.span,
                ty: ty.clone(),
                symbol,
            });
        }
        ty
    }
    pub(super) fn expr(&mut self, e: &Expr, expected: Option<&Type>) -> Type {
        self.expr_usage(e, expected, true)
    }
    pub(super) fn expr_usage(
        &mut self,
        e: &Expr,
        expected: Option<&Type>,
        value_required: bool,
    ) -> Type {
        if self.expression_depth >= 32 {
            self.error(
                "ZEN-TYPE-0099",
                e.span,
                "expression exceeds the frontend complexity limit",
            );
            return self.record(e, Type::Error, None);
        }
        self.capture_scope(e.span);
        if let Some(t) = expected {
            self.ide.expected.push((e.span, t.clone()));
        }
        self.expression_depth += 1;
        let t = self.expr_inner(e, expected, value_required);
        self.expression_depth -= 1;
        if let Some(want) = expected {
            self.expect_type(&t, want, e.span);
        }
        self.record(e, t, None)
    }
    pub(super) fn literal_type(&mut self, l: &Literal, span: Span, negative: bool) -> Type {
        match l {
            Literal::Bool(_) => Type::Bool,
            Literal::Unit => Type::Unit,
            Literal::String(_) => Type::String,
            Literal::Char(_) => Type::Char,
            Literal::Float(s) => {
                if s.ends_with("f32") {
                    Type::F32
                } else {
                    Type::Float
                }
            }
            Literal::Integer(s) => {
                let text = s.replace('_', "");
                let split = text
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(text.len());
                let (digits, suffix) = text.split_at(split);
                let (ty, bits, signed) = match suffix {
                    "i8" => (Type::I8, 8, true),
                    "i16" => (Type::I16, 16, true),
                    "i32" => (Type::I32, 32, true),
                    "i64" => (Type::I64, 64, true),
                    "u8" => (Type::U8, 8, false),
                    "u16" => (Type::U16, 16, false),
                    "u32" => (Type::U32, 32, false),
                    "u64" => (Type::U64, 64, false),
                    _ => return Type::Int,
                };
                let max = if signed {
                    (1u128 << (bits - 1)) - u128::from(!negative)
                } else {
                    (1u128 << bits) - 1
                };
                if (negative && !signed) || digits.parse::<u128>().map_or(true, |v| v > max) {
                    self.error(
                        "ZEN-TYPE-0020",
                        span,
                        "fixed-width integer literal is out of range",
                    );
                }
                ty
            }
        }
    }
    pub(super) fn expr_inner(
        &mut self,
        e: &Expr,
        expected: Option<&Type>,
        value_required: bool,
    ) -> Type {
        match &e.kind {
            ExprKind::Literal(l) => self.literal_type(l, e.span, false),
            ExprKind::Name(n, args) => {
                if let Some(local) = self.local(&n.text) {
                    if !args.is_empty() {
                        self.error(
                            "ZEN-TYPE-0002",
                            e.span,
                            "local value cannot take type arguments",
                        );
                    }
                    self.record(e, local.ty.clone(), Some(local.symbol));
                    return local.ty;
                }
                if let Some(id) = self.value_id(&n.text) {
                    let t = if let Some(f) = self.functions.get(&id).cloned() {
                        if !f.generics.is_empty() {
                            self.instantiate_function_value(&f, args, expected, e.span, None)
                        } else {
                            if !args.is_empty() {
                                self.error(
                                    "ZEN-TYPE-0002",
                                    e.span,
                                    "non-generic function takes no type arguments",
                                );
                            }
                            f.ty()
                        }
                    } else {
                        if !args.is_empty() {
                            self.error("ZEN-TYPE-0002", e.span, "constant takes no type arguments");
                        }
                        self.symbols[id.0].ty.clone()
                    };
                    self.record(e, t.clone(), Some(id));
                    t
                } else {
                    self.error(
                        "ZEN-NAME-0001",
                        n.span,
                        format!("unknown value `{}`", n.text),
                    );
                    Type::Error
                }
            }
            ExprKind::Contextual(n) => {
                if let Some(t) = expected {
                    self.construct_variant(t, n, &[], e.span)
                } else {
                    self.error(
                        "ZEN-TYPE-0011",
                        e.span,
                        "contextual enum constructor requires an expected enum type",
                    );
                    Type::Error
                }
            }
            ExprKind::Call(c, args) => self.call(c, args, expected, e.span),
            ExprKind::Member(base, n, types) => {
                if let Some(t) = self.explicit_enum(base) {
                    if !types.is_empty() {
                        self.error(
                            "ZEN-TYPE-0002",
                            e.span,
                            "supply enum arguments on the enum type, not its variant",
                        );
                    }
                    self.record(base, t.clone(), None);
                    return self.construct_variant(&t, n, &[], e.span);
                }
                let t = self.expr(base, None);
                self.capture_members(&t);
                if let Some(field) = self.field(&t, n) {
                    if !types.is_empty() {
                        self.error("ZEN-TYPE-0002", e.span, "field takes no type arguments");
                    }
                    field
                } else if let Some(mut f) = self.method(&t, n) {
                    if f.params.first().is_some_and(|(n, _, _)| n.text == "self") {
                        f.params.remove(0);
                    } else {
                        self.error(
                            "ZEN-IMPL-0002",
                            e.span,
                            "instance method must explicitly declare self",
                        );
                    }
                    let ty = self.instantiate_function_value(&f, types, expected, e.span, Some(&t));
                    self.record(e, ty.clone(), Some(f.symbol));
                    ty
                } else {
                    Type::Error
                }
            }
            ExprKind::Struct(t, fields) => {
                let ty = self.infer_struct(t, fields, expected);
                self.check_constraints(&ty, t.span);
                self.struct_fields(&ty, fields, false, e.span);
                ty
            }
            ExprKind::Update(base, fields) => {
                let ty = self.expr(base, None);
                self.struct_fields(&ty, fields, true, e.span);
                ty
            }
            ExprKind::List(items) => {
                let context = match expected {
                    Some(Type::Nominal(id, a)) if *id == LIST => a.first().cloned(),
                    _ => None,
                };
                let mut element = context.clone();
                for item in items {
                    let t = self.expr(item, element.as_ref());
                    if element.is_none() || element == Some(Type::Never) {
                        element = Some(t);
                    }
                }
                if let Some(t) = element {
                    Type::Nominal(LIST, vec![t])
                } else {
                    self.error(
                        "ZEN-TYPE-0012",
                        e.span,
                        "empty list requires a contextual List<T> type",
                    );
                    Type::Error
                }
            }
            ExprKind::Unary(op, operand) => {
                if op == "-"
                    && let ExprKind::Literal(Literal::Integer(_)) = &operand.kind
                {
                    let ExprKind::Literal(l) = &operand.kind else {
                        unreachable!()
                    };
                    let t = self.literal_type(l, e.span, true);
                    self.record(operand, t.clone(), None);
                    return t;
                }
                let t = self.expr(operand, None);
                match op.as_str() {
                    "!" => {
                        self.expect_type(&t, &Type::Bool, operand.span);
                        Type::Bool
                    }
                    "-" => {
                        if !matches!(
                            t,
                            Type::Int | Type::Float | Type::F32 | Type::Error | Type::Never
                        ) {
                            self.error("ZEN-TYPE-0021",e.span,"unary negation requires Int or floating-point operands; fixed-width arithmetic must be checked");
                        }
                        t
                    }
                    "await" => {
                        if !self.ctx.asynchronous {
                            self.error(
                                "ZEN-ASYNC-0001",
                                e.span,
                                "await requires an async function or lambda",
                            );
                        }
                        if let Type::Nominal(id, a) = &t
                            && *id == TASK
                        {
                            return a[0].clone();
                        }
                        if t != Type::Error && t != Type::Never {
                            self.error("ZEN-ASYNC-0002", operand.span, "await requires Task<T>");
                        }
                        if t == Type::Never { t } else { Type::Error }
                    }
                    "?" => self.propagate(&t, e.span),
                    _ => Type::Error,
                }
            }
            ExprKind::Binary(op, a, b) => {
                let left = self.expr(a, None);
                let right = self.expr(b, Some(&left));
                if left == Type::Error || right == Type::Error {
                    return Type::Error;
                }
                match op.as_str() {
                    "&&" | "||" => {
                        self.expect_type(&left, &Type::Bool, a.span);
                        self.expect_type(&right, &Type::Bool, b.span);
                        Type::Bool
                    }
                    "==" | "!=" => {
                        if !self.equality(&left, &mut BTreeSet::new()) {
                            self.error(
                                "ZEN-TYPE-0022",
                                e.span,
                                "operands are not equality-capable",
                            );
                        }
                        Type::Bool
                    }
                    "<" | "<=" | ">" | ">=" => {
                        if !left.is_numeric() && left != Type::Never {
                            self.error(
                                "ZEN-TYPE-0023",
                                e.span,
                                "ordering is only defined for numeric operands in this prelude",
                            );
                        }
                        Type::Bool
                    }
                    _ => {
                        if !(matches!(left, Type::Int | Type::Float | Type::F32 | Type::Never)
                            || op == "+" && left == Type::String)
                        {
                            self.error("ZEN-TYPE-0021",e.span,"arithmetic is not defined for this type; fixed-width operations require checked APIs");
                        }
                        left
                    }
                }
            }
            ExprKind::Block(stmts, tail) => {
                self.ctx.locals.push(BTreeMap::new());
                let mut diverges = false;
                let mut start = e.span.start;
                for s in stmts {
                    self.capture_scope(Span::new(e.span.file, start, s.span.end));
                    diverges |= self.statement(s);
                    start = s.span.end;
                }
                self.capture_scope(Span::new(e.span.file, start, e.span.end));
                let t = if let Some(tail) = tail {
                    self.expr(tail, expected)
                } else {
                    Type::Unit
                };
                self.ctx.locals.pop();
                if diverges { Type::Never } else { t }
            }
            ExprKind::If(c, a, b) => {
                self.expr(c, Some(&Type::Bool));
                let at = self.expr_usage(a, expected, value_required);
                if let Some(b) = b {
                    let context = expected.or(if at == Type::Never { None } else { Some(&at) });
                    let bt = self.expr_usage(b, context, value_required);
                    self.join(&at, &bt, e.span)
                } else {
                    if value_required {
                        self.error("ZEN-TYPE-0013", e.span, "if used as a value requires else");
                    }
                    self.expect_type(&at, &Type::Unit, a.span);
                    Type::Unit
                }
            }
            ExprKind::Match(value, arms) => self.match_expr(value, arms, expected, e.span),
            ExprKind::Lambda {
                params,
                result,
                body,
                asynchronous,
            } => {
                let context = if let Some(Type::Function(a, r)) = expected {
                    Some((a.clone(), r.as_ref().clone()))
                } else {
                    None
                };
                if let Some((a, _)) = &context
                    && a.len() != params.len()
                {
                    self.error(
                        "ZEN-TYPE-0008",
                        e.span,
                        "lambda parameter count differs from context",
                    );
                }
                let saved = self.ctx.clone();
                self.ctx.closure += 1;
                self.ctx.loops = 0;
                self.ctx.defer_boundary = None;
                self.ctx.asynchronous = *asynchronous;
                self.ctx.locals.push(BTreeMap::new());
                let ret = if let Some(r) = result {
                    self.value_type(r)
                } else if let Some((_, r)) = &context {
                    if *asynchronous {
                        if let Type::Nominal(id, a) = r
                            && *id == TASK
                        {
                            a[0].clone()
                        } else {
                            self.error(
                                "ZEN-TYPE-0014",
                                e.span,
                                "async lambda context must return Task<T>",
                            );
                            Type::Error
                        }
                    } else {
                        r.clone()
                    }
                } else {
                    self.error(
                        "ZEN-TYPE-0014",
                        e.span,
                        "lambda requires a return annotation or function context",
                    );
                    Type::Error
                };
                self.ctx.result = Some(ret.clone());
                let mut types = vec![];
                for (i, p) in params.iter().enumerate() {
                    if p.name.text == "self" {
                        self.error(
                            "ZEN-IMPL-0002",
                            p.name.span,
                            "self can only be declared by methods",
                        );
                    }
                    let ty = if let Some(t) = &p.ty {
                        self.value_type(t)
                    } else if let Some(t) = context.as_ref().and_then(|(a, _)| a.get(i)) {
                        t.clone()
                    } else {
                        self.error(
                            "ZEN-TYPE-0014",
                            p.name.span,
                            "lambda parameter type cannot be inferred",
                        );
                        Type::Error
                    };
                    if p.default.is_some() {
                        self.error(
                            "ZEN-TYPE-0014",
                            p.name.span,
                            "lambda defaults are not part of function types",
                        );
                    }
                    self.check_constraints(&ty, p.name.span);
                    self.bind(&p.name, ty.clone(), false);
                    types.push(ty);
                }
                let bt = self.expr(body, Some(&ret));
                if ret != Type::Unit && bt != Type::Never && ret != Type::Error {
                    self.error(
                        "ZEN-TYPE-0010",
                        body.span,
                        "lambda has a reachable path without return",
                    );
                }
                self.ctx = saved;
                Type::Function(types, Box::new(if *asynchronous { task(ret) } else { ret }))
            }
        }
    }
    pub(super) fn join(&mut self, a: &Type, b: &Type, s: Span) -> Type {
        if *a == Type::Never {
            return b.clone();
        }
        if *b == Type::Never {
            return a.clone();
        }
        self.expect_type(b, a, s);
        a.clone()
    }
    pub(super) fn statement(&mut self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Bind {
                name,
                mutable,
                ty,
                value,
            } => {
                let annotation = ty.as_ref().map(|t| self.value_type(t));
                let t = self.expr(value, annotation.as_ref());
                let binding = annotation.unwrap_or_else(|| t.clone());
                self.check_constraints(&binding, s.span);
                self.bind(name, binding.clone(), *mutable);
                if ty.is_none() {
                    self.ide.inferred.push((name.span, binding));
                }
                t == Type::Never
            }
            StmtKind::Assign(target, value) => {
                if let ExprKind::Name(n, args) = &target.kind
                    && args.is_empty()
                {
                    if let Some(l) = self.local(&n.text) {
                        if !l.mutable {
                            self.error(
                                "ZEN-TYPE-0004",
                                target.span,
                                "cannot assign to an immutable binding",
                            );
                        }
                        if l.closure < self.ctx.closure {
                            self.error(
                                "ZEN-TYPE-0005",
                                target.span,
                                "closure cannot assign to a captured binding",
                            );
                        }
                        self.ide.writes.push(target.span);
                        self.record(target, l.ty.clone(), Some(l.symbol));
                        return self.expr(value, Some(&l.ty)) == Type::Never;
                    }
                    self.error(
                        "ZEN-NAME-0001",
                        target.span,
                        "assignment requires a local mutable binding",
                    );
                } else {
                    self.error(
                        "ZEN-TYPE-0006",
                        target.span,
                        "assignments may only target local mutable bindings; use a struct update",
                    );
                }
                self.expr(value, None);
                false
            }
            StmtKind::Return(value) => {
                if self.ctx.defer_boundary.is_some() {
                    self.error(
                        "ZEN-TYPE-0042",
                        s.span,
                        "return cannot leave a deferred expression",
                    );
                }
                if let Some(r) = self.ctx.result.clone() {
                    if let Some(v) = value {
                        self.expr(v, Some(&r));
                    } else {
                        self.expect_type(&Type::Unit, &r, s.span);
                    }
                } else {
                    self.error("ZEN-TYPE-0010", s.span, "return outside function");
                }
                true
            }
            StmtKind::Expr(e) => self.expr_usage(e, None, false) == Type::Never,
            StmtKind::Defer(e) => {
                let saved = self.ctx.defer_boundary;
                self.ctx.defer_boundary = Some(self.ctx.loops);
                self.expr(e, Some(&Type::Unit));
                self.ctx.defer_boundary = saved;
                false
            }
            StmtKind::Break | StmtKind::Continue => {
                if self
                    .ctx
                    .defer_boundary
                    .is_some_and(|depth| self.ctx.loops <= depth)
                {
                    self.error(
                        "ZEN-TYPE-0042",
                        s.span,
                        "loop exit cannot leave a deferred expression",
                    );
                }
                if self.ctx.loops == 0 {
                    self.error(
                        "ZEN-TYPE-0015",
                        s.span,
                        "break/continue requires an enclosing loop",
                    );
                }
                true
            }
            StmtKind::While(c, b) => {
                self.expr(c, Some(&Type::Bool));
                self.ctx.loops += 1;
                self.expr(b, Some(&Type::Unit));
                self.ctx.loops -= 1;
                false
            }
            StmtKind::For(n, items, b) => {
                let t = self.expr(items, None);
                let element = if let Type::Nominal(id, a) = t
                    && (id == LIST || id == SET)
                {
                    a[0].clone()
                } else {
                    self.error(
                        "ZEN-TYPE-0016",
                        items.span,
                        "for requires a known iterable collection",
                    );
                    Type::Error
                };
                self.ctx.locals.push(BTreeMap::new());
                self.bind(n, element, false);
                self.ctx.loops += 1;
                self.expr(b, Some(&Type::Unit));
                self.ctx.loops -= 1;
                self.ctx.locals.pop();
                false
            }
        }
    }
    pub(super) fn propagate(&mut self, t: &Type, s: Span) -> Type {
        if *t == Type::Never || *t == Type::Error {
            return t.clone();
        }
        if let Type::Nominal(id, a) = t {
            let valid = match (&self.ctx.result, *id) {
                (Some(Type::Nominal(r, b)), RESULT) => *r == RESULT && a.get(1) == b.get(1),
                (Some(Type::Nominal(r, _)), OPTION) => *r == OPTION,
                _ => false,
            };
            if (*id == RESULT || *id == OPTION) && valid {
                if self.ctx.defer_boundary.is_some() {
                    self.error(
                        "ZEN-TYPE-0042",
                        s,
                        "propagation cannot leave a deferred expression",
                    );
                }
                return a[0].clone();
            }
        }
        let mut d = Diagnostic::error(
            "ZEN-TYPE-0041",
            s,
            "cannot propagate this value from the enclosing return type",
        );
        d.actual = Some(self.show(t));
        d.expected = self.ctx.result.as_ref().map(|t| self.show(t));
        d.fixes.push(SuggestedFix{message:"convert the error explicitly before applying `?`; Option and Result cannot interconvert implicitly".into(),replacement:None});
        self.diagnostics.push(d);
        Type::Error
    }
}
