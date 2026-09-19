use super::*;

impl Checker {
    pub(super) fn bodies(&mut self) {
        for n in self.nominals.clone().into_iter().skip(6) {
            self.ctx = Context {
                module: n.module,
                generics: n.generics.clone(),
                ..Context::default()
            };
            for generic in &n.generics {
                for bound in &generic.bounds {
                    self.check_constraints(bound, n.name.span);
                    if n.public {
                        self.public_type(bound, n.name.span);
                    }
                }
            }
            match n.kind {
                Definition::Struct(fields) => {
                    for f in fields {
                        self.check_constraints(&f.ty, f.name.span);
                        if n.public && f.public {
                            self.public_type(&f.ty, f.name.span);
                        }
                    }
                }
                Definition::Enum(v) => {
                    for (name, ts) in v {
                        for t in ts {
                            self.check_constraints(&t, name.span);
                            if n.public {
                                self.public_type(&t, name.span);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for (_, (m, t, e, public)) in self.constants.clone() {
            self.ctx = Context {
                module: m,
                ..Context::default()
            };
            if !self.constant_expr(&e, &mut BTreeSet::new(), m) {
                self.error(
                    "ZEN-TYPE-0031",
                    e.span,
                    "constant initializer is not compile-time evaluable (or contains a cycle)",
                );
            }
            self.expr(&e, Some(&t));
            self.check_constraints(&t, e.span);
            if public {
                self.public_type(&t, e.span);
            }
        }
        let functions = self
            .functions
            .values()
            .cloned()
            .chain(
                self.implementations
                    .iter()
                    .flat_map(|i| i.methods.iter().cloned()),
            )
            .chain(
                self.nominals
                    .iter()
                    .filter_map(|n| {
                        if let Definition::Interface(m) = &n.kind {
                            Some(m.clone())
                        } else {
                            None
                        }
                    })
                    .flatten(),
            )
            .collect::<Vec<_>>();
        for f in functions {
            self.check_function(&f);
        }
    }
    pub(super) fn constant_expr(
        &self,
        e: &Expr,
        seen: &mut BTreeSet<SymbolId>,
        module: usize,
    ) -> bool {
        if seen.len() > 128 {
            return false;
        }
        match &e.kind {
            ExprKind::Literal(_) => true,
            ExprKind::Unary(o, e) if o == "!" || o == "-" => self.constant_expr(e, seen, module),
            ExprKind::Binary(_, a, b) => {
                self.constant_expr(a, seen, module) && self.constant_expr(b, seen, module)
            }
            ExprKind::List(a) => a.iter().all(|e| self.constant_expr(e, seen, module)),
            ExprKind::Struct(_, a) => a.iter().all(|(_, e)| self.constant_expr(e, seen, module)),
            ExprKind::Name(n, _) => {
                let Some(id) = self.names[module]
                    .values
                    .get(&n.text)
                    .or_else(|| self.names[module].import_values.get(&n.text))
                    .copied()
                else {
                    return false;
                };
                if !seen.insert(id) {
                    return false;
                }
                let result = self
                    .constants
                    .get(&id)
                    .is_some_and(|(m, _, e, _)| self.constant_expr(e, seen, *m));
                seen.remove(&id);
                result
            }
            _ => false,
        }
    }
    pub(super) fn deterministic_default(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Name(n, _) if self.local(&n.text).is_some() => true,
            ExprKind::Unary(o, e) if o == "!" || o == "-" => self.deterministic_default(e),
            ExprKind::Binary(_, a, b) => {
                self.deterministic_default(a) && self.deterministic_default(b)
            }
            ExprKind::List(a) => a.iter().all(|e| self.deterministic_default(e)),
            ExprKind::Struct(_, a) => a.iter().all(|(_, e)| self.deterministic_default(e)),
            _ => self.constant_expr(e, &mut BTreeSet::new(), self.ctx.module),
        }
    }
    pub(super) fn check_function(&mut self, f: &Signature) {
        self.ctx = Context {
            module: f.module,
            generics: f.generics.clone(),
            locals: vec![BTreeMap::new()],
            result: Some(f.result.clone()),
            asynchronous: f.asynchronous,
            ..Context::default()
        };
        for generic in &f.generics {
            for bound in &generic.bounds {
                self.check_constraints(bound, f.name.span);
                if f.public {
                    self.public_type(bound, f.name.span);
                }
            }
        }
        for (n, t, default) in &f.params {
            self.check_constraints(t, n.span);
            if let Some(e) = default {
                if !self.deterministic_default(e) {
                    self.error(
                        "ZEN-TYPE-0032",
                        e.span,
                        "default expression is not provably deterministic in this frontend",
                    );
                }
                self.expr(e, Some(t));
            }
            self.bind(n, t.clone(), false);
            if f.public {
                self.public_type(t, n.span);
            }
        }
        self.check_constraints(&f.result, f.name.span);
        if f.public {
            self.public_type(&f.result, f.name.span);
        }
        if let Some(body) = &f.body {
            let t = self.expr(body, Some(&f.result));
            if t != Type::Never && f.result != Type::Unit {
                self.error(
                    "ZEN-TYPE-0010",
                    body.span,
                    "not every reachable path explicitly returns a value",
                );
            }
        }
    }
    pub(super) fn bind(&mut self, n: &Name, ty: Type, mutable: bool) {
        if let Some(old) = self.local(&n.text) {
            self.diagnostics.push(
                Diagnostic::error(
                    "ZEN-NAME-0007",
                    n.span,
                    format!("local shadowing of `{}` is forbidden", n.text),
                )
                .label(old.span, "previous binding"),
            );
            return;
        }
        let symbol = self.new_symbol(n, ty.clone());
        self.resolved.bindings.insert(key(n.span), symbol);
        self.resolved.locals.insert(
            symbol,
            resolved::Local {
                id: symbol,
                name: n.text.clone(),
                span: n.span,
                ty: ty.clone(),
                mutable,
            },
        );
        let local = Local {
            ty,
            mutable,
            span: n.span,
            symbol,
            closure: self.ctx.closure,
        };
        if self.ctx.locals.is_empty() {
            self.ctx.locals.push(BTreeMap::new());
        }
        self.ctx
            .locals
            .last_mut()
            .unwrap()
            .insert(n.text.clone(), local);
    }
    pub(super) fn local(&self, name: &str) -> Option<Local> {
        self.ctx
            .locals
            .iter()
            .rev()
            .find_map(|s| s.get(name).cloned())
    }
    pub(super) fn value_id(&self, name: &str) -> Option<SymbolId> {
        let n = &self.names[self.ctx.module];
        n.values
            .get(name)
            .or_else(|| n.import_values.get(name))
            .copied()
    }
}
