use super::*;

impl Checker {
    pub(super) fn explicit_enum(&mut self, e: &Expr) -> Option<Type> {
        if let ExprKind::Name(n, a) = &e.kind
            && self.local(&n.text).is_none()
            && self.value_id(&n.text).is_none()
            && let Some(id) = self.type_id(&n.text)
            && matches!(self.nominals[id.0].kind, Definition::Enum(_))
        {
            let t = TypeRef {
                kind: TypeKind::Named(n.text.clone(), a.clone()),
                span: e.span,
            };
            return Some(self.resolve(&t));
        }
        None
    }
    pub(super) fn construct_variant(
        &mut self,
        t: &Type,
        n: &Name,
        args: &[Arg],
        span: Span,
    ) -> Type {
        let Type::Nominal(id, types) = t else {
            self.error("ZEN-TYPE-0011", span, "constructor requires an enum type");
            return Type::Error;
        };
        let Definition::Enum(variants) = self.nominals[id.0].kind.clone() else {
            self.error("ZEN-TYPE-0011", span, "constructor requires an enum type");
            return Type::Error;
        };
        let Some((definition, payload)) = variants.iter().find(|(v, _)| v.text == n.text) else {
            self.error("ZEN-NAME-0008", n.span, "unknown enum variant");
            return Type::Error;
        };
        self.ide
            .reference(n.span, Entity::Member(*id, definition.text.clone()));
        if args.len() != payload.len() {
            self.error(
                "ZEN-TYPE-0008",
                span,
                format!("variant expects {} payload values", payload.len()),
            );
        }
        let sub = self.substitution(*id, types);
        for (i, a) in args.iter().enumerate() {
            if a.label.is_some() {
                self.error(
                    "ZEN-TYPE-0009",
                    a.value.span,
                    "enum payloads have no named labels",
                );
            }
            let ty = payload.get(i).map(|t| t.substitute(&sub));
            self.expr(&a.value, ty.as_ref());
        }
        self.check_constraints(t, span);
        t.clone()
    }
    pub(super) fn infer_struct(
        &mut self,
        syntax: &TypeRef,
        fields: &[(Name, Expr)],
        expected: Option<&Type>,
    ) -> Type {
        if let TypeKind::Named(name, arguments) = &syntax.kind
            && arguments.is_empty()
            && let Some(id) = self.type_id(name)
            && !self.nominals[id.0].generics.is_empty()
        {
            self.ide.reference(
                Span::new(
                    syntax.span.file,
                    syntax.span.start,
                    syntax.span.start + name.len(),
                ),
                Entity::Nominal(id),
            );
            if let Some(Type::Nominal(other, _)) = expected
                && *other == id
            {
                return expected.unwrap().clone();
            }
            let nominal = self.nominals[id.0].clone();
            let Definition::Struct(definitions) = nominal.kind else {
                return self.value_type(syntax);
            };
            let ids = nominal.generics.iter().map(|g| g.id).collect::<Vec<_>>();
            let mut sub = BTreeMap::new();
            for (name, value) in fields {
                if let Some(field) = definitions.iter().find(|f| f.name.text == name.text) {
                    let context = field.ty.substitute(&sub);
                    let actual = self.expr(
                        value,
                        if context.contains_param(&ids) {
                            None
                        } else {
                            Some(&context)
                        },
                    );
                    self.infer(&field.ty, &actual, &ids, &mut sub, value.span);
                }
            }
            if ids.iter().any(|id| !sub.contains_key(id)) {
                self.error(
                    "ZEN-TYPE-0026",
                    syntax.span,
                    "struct generic arguments cannot be uniquely inferred",
                );
                return Type::Error;
            }
            return Type::Nominal(id, ids.iter().map(|id| sub[id].clone()).collect());
        }
        self.value_type(syntax)
    }
    pub(super) fn struct_fields(
        &mut self,
        t: &Type,
        initializers: &[(Name, Expr)],
        update: bool,
        span: Span,
    ) {
        let Type::Nominal(id, args) = t else {
            if *t != Type::Error {
                self.error(
                    "ZEN-TYPE-0017",
                    span,
                    "struct construction/update requires a struct type",
                );
            }
            return;
        };
        let Definition::Struct(fields) = self.nominals[id.0].kind.clone() else {
            self.error(
                "ZEN-TYPE-0017",
                span,
                "construction/update requires a struct",
            );
            return;
        };
        let module = self.nominals[id.0].module;
        let sub = self.substitution(*id, args);
        let mut seen = BTreeMap::new();
        for (n, e) in initializers {
            if let Some(old) = seen.insert(n.text.clone(), n.span) {
                self.diagnostics.push(
                    Diagnostic::error("ZEN-TYPE-0018", n.span, "duplicate struct initializer")
                        .label(old, "first initializer"),
                );
            }
            if let Some(f) = fields.iter().find(|f| f.name.text == n.text) {
                if !f.public && module != self.ctx.module {
                    self.error("ZEN-NAME-0005", n.span, "struct field is private");
                }
                self.ide
                    .reference(n.span, Entity::Member(*id, f.name.text.clone()));
                self.expr(e, Some(&f.ty.substitute(&sub)));
            } else {
                self.error(
                    "ZEN-TYPE-0019",
                    n.span,
                    format!("unknown field `{}`", n.text),
                );
                self.expr(e, None);
            }
        }
        if !update {
            for f in fields {
                if !seen.contains_key(&f.name.text) {
                    self.error(
                        "ZEN-TYPE-0024",
                        span,
                        format!("missing field `{}`", f.name.text),
                    );
                }
            }
        }
    }
    pub(super) fn field(&mut self, t: &Type, n: &Name) -> Option<Type> {
        if let Type::Nominal(id, args) = t
            && let Definition::Struct(fields) = &self.nominals[id.0].kind
            && let Some(f) = fields.iter().find(|f| f.name.text == n.text).cloned()
        {
            if !f.public && self.nominals[id.0].module != self.ctx.module {
                self.error("ZEN-NAME-0005", n.span, "field is private");
            }
            self.ide
                .reference(n.span, Entity::Member(*id, f.name.text.clone()));
            return Some(f.ty.substitute(&self.substitution(*id, args)));
        }
        None
    }
    pub(super) fn collection_method(&mut self, receiver: &Type, name: &Name) -> Option<Signature> {
        let Type::Nominal(id, args) = receiver else {
            return None;
        };
        let (parameters, result) = match (*id, name.text.as_str()) {
            (LIST, "get") => (vec![("index", Type::Int)], option(args[0].clone())),
            (LIST, "first") => (vec![], option(args[0].clone())),
            (LIST, "append") => (vec![("value", args[0].clone())], receiver.clone()),
            (MAP, "get") => (vec![("key", args[0].clone())], option(args[1].clone())),
            _ => return None,
        };
        let mut params = vec![(
            Name {
                text: "self".into(),
                span: name.span,
            },
            receiver.clone(),
            None,
        )];
        params.extend(parameters.into_iter().map(|(n, t)| {
            (
                Name {
                    text: n.into(),
                    span: name.span,
                },
                t,
                None,
            )
        }));
        let symbol = self.new_symbol(
            &Name {
                text: name.text.clone(),
                span: Span::default(),
            },
            Type::Function(
                params.iter().map(|(_, t, _)| t.clone()).collect(),
                Box::new(result.clone()),
            ),
        );
        Some(Signature {
            symbol,
            name: name.clone(),
            module: usize::MAX,
            public: true,
            generics: vec![],
            params,
            result,
            asynchronous: false,
            body: None,
        })
    }
    pub(super) fn method(&mut self, t: &Type, n: &Name) -> Option<Signature> {
        if let Some(signature) = self.collection_method(t, n) {
            return Some(signature);
        }
        let inherent = self
            .implementations
            .iter()
            .filter(|i| i.target == *t && i.interface.is_none())
            .flat_map(|i| &i.methods)
            .find(|m| m.name.text == n.text)
            .cloned();
        if let Some(f) = inherent {
            if !f.public && f.module != self.ctx.module {
                self.error("ZEN-NAME-0005", n.span, "inherent method is private");
            }
            return Some(f);
        }
        let mut candidates = self
            .implementations
            .iter()
            .filter(|i| i.target == *t && i.interface.is_some())
            .flat_map(|i| &i.methods)
            .filter(|m| m.name.text == n.text)
            .cloned()
            .collect::<Vec<_>>();
        if let Type::Param(p) = t
            && let Some(g) = self.ctx.generics.iter().find(|g| g.id == *p)
        {
            for bound in &g.bounds {
                if let Type::Nominal(id, args) = bound
                    && let Definition::Interface(methods) = &self.nominals[id.0].kind
                {
                    let sub = self.substitution(*id, args);
                    for f in methods.iter().filter(|f| f.name.text == n.text) {
                        let mut f = f.clone();
                        f.generics.clear();
                        f.params = f
                            .params
                            .iter()
                            .map(|(n, ty, d)| {
                                (
                                    n.clone(),
                                    if n.text == "self" {
                                        t.clone()
                                    } else {
                                        ty.substitute(&sub)
                                    },
                                    d.clone(),
                                )
                            })
                            .collect();
                        f.result = f.result.substitute(&sub);
                        candidates.push(f);
                    }
                }
            }
        }
        if candidates.len() == 1 {
            return candidates.pop();
        }
        if candidates.len() > 1 {
            self.error(
                "ZEN-NAME-0009",
                n.span,
                "ambiguous interface method; multiple implementations provide this name",
            );
        } else if *t != Type::Error {
            self.error(
                "ZEN-NAME-0008",
                n.span,
                format!("no field or method `{}` on {}", n.text, self.show(t)),
            );
        }
        None
    }
    pub(super) fn call(
        &mut self,
        callee: &Expr,
        args: &[Arg],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        if let ExprKind::Contextual(n) = &callee.kind {
            if let Some(t) = expected {
                let ty = self.construct_variant(t, n, args, span);
                self.record(
                    callee,
                    Type::Function(
                        args.iter()
                            .map(|a| self.expression_type(&a.value))
                            .collect(),
                        Box::new(ty.clone()),
                    ),
                    None,
                );
                return ty;
            }
            self.error(
                "ZEN-TYPE-0011",
                callee.span,
                "contextual constructor requires an expected enum type",
            );
            return Type::Error;
        }
        if let ExprKind::Name(n, types) = &callee.kind
            && self.local(&n.text).is_none()
            && let Some(id) = self.value_id(&n.text)
            && let Some(f) = self.functions.get(&id).cloned()
        {
            return self.call_signature(&f, types, args, expected, span, callee, false);
        }
        if let ExprKind::Member(base, n, types) = &callee.kind {
            if let Some(t) = self.explicit_enum(base) {
                if !types.is_empty() {
                    self.error(
                        "ZEN-TYPE-0002",
                        callee.span,
                        "supply enum arguments on the enum type, not its variant",
                    );
                }
                self.record(base, t.clone(), None);
                let result = self.construct_variant(&t, n, args, span);
                self.record(
                    callee,
                    Type::Function(
                        args.iter()
                            .map(|a| self.expression_type(&a.value))
                            .collect(),
                        Box::new(result.clone()),
                    ),
                    None,
                );
                return result;
            }
            let receiver = self.expr(base, None);
            self.capture_members(&receiver);
            if let Some(t) = self.field(&receiver, n) {
                if !types.is_empty() {
                    self.error(
                        "ZEN-TYPE-0002",
                        callee.span,
                        "function field takes no type arguments",
                    );
                }
                self.record(callee, t.clone(), None);
                return self.call_value(&t, args, span);
            }
            if let Some(f) = self.method(&receiver, n) {
                return self.call_signature(&f, types, args, expected, span, callee, true);
            }
            return Type::Error;
        }
        let ty = self.expr(callee, None);
        self.call_value(&ty, args, span)
    }
    pub(super) fn expression_type(&self, e: &Expr) -> Type {
        self.expression_index
            .get(&(e.span.file, e.id.0))
            .map(|&i| self.expressions[i].ty.clone())
            .unwrap_or(Type::Error)
    }
    pub(super) fn call_value(&mut self, t: &Type, args: &[Arg], span: Span) -> Type {
        if let Type::Function(params, result) = t {
            if params.len() != args.len() {
                self.error(
                    "ZEN-TYPE-0008",
                    span,
                    format!(
                        "expected {} arguments, received {}",
                        params.len(),
                        args.len()
                    ),
                );
            }
            for (i, a) in args.iter().enumerate() {
                if let Some(n) = &a.label {
                    self.error(
                        "ZEN-TYPE-0009",
                        n.span,
                        "function values have no parameter labels",
                    );
                }
                self.expr(&a.value, params.get(i));
            }
            *result.clone()
        } else {
            if *t != Type::Error && *t != Type::Never {
                self.error("ZEN-TYPE-0025", span, "value is not callable");
            }
            if *t == Type::Never {
                Type::Never
            } else {
                Type::Error
            }
        }
    }
    pub(super) fn infer(
        &mut self,
        template: &Type,
        actual: &Type,
        ids: &[ParamId],
        sub: &mut BTreeMap<ParamId, Type>,
        span: Span,
    ) {
        if *actual == Type::Never || *actual == Type::Error {
            return;
        }
        match template {
            Type::Param(p) if ids.contains(p) => {
                if let Some(old) = sub.get(p) {
                    let old = old.clone();
                    self.expect_type(actual, &old, span);
                } else {
                    sub.insert(*p, actual.clone());
                }
            }
            Type::Nominal(id, a) => {
                if let Type::Nominal(other, b) = actual
                    && id == other
                {
                    for (a, b) in a.iter().zip(b) {
                        self.infer(a, b, ids, sub, span);
                    }
                }
            }
            Type::Function(a, r) => {
                if let Type::Function(b, s) = actual {
                    for (a, b) in a.iter().zip(b) {
                        self.infer(a, b, ids, sub, span);
                    }
                    self.infer(r, s, ids, sub, span);
                }
            }
            _ => {}
        }
    }
    pub(super) fn instantiate_function_value(
        &mut self,
        f: &Signature,
        args: &[TypeRef],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let ids = f.generics.iter().map(|g| g.id).collect::<Vec<_>>();
        let mut sub = BTreeMap::new();
        if !args.is_empty() {
            if args.len() != ids.len() {
                self.error(
                    "ZEN-TYPE-0002",
                    span,
                    "incorrect number of generic arguments",
                );
            }
            for (p, a) in ids.iter().zip(args) {
                sub.insert(*p, self.value_type(a));
            }
        }
        if let Some(t) = expected {
            self.infer(&f.ty(), t, &ids, &mut sub, span);
        }
        self.finish_inference(f, &ids, &sub, span);
        f.ty().substitute(&sub)
    }
    pub(super) fn finish_inference(
        &mut self,
        f: &Signature,
        ids: &[ParamId],
        sub: &BTreeMap<ParamId, Type>,
        span: Span,
    ) {
        if ids.iter().any(|id| !sub.contains_key(id)) {
            self.error(
                "ZEN-TYPE-0026",
                span,
                "generic arguments cannot be uniquely inferred; supply explicit types",
            );
        }
        for g in &f.generics {
            if let Some(t) = sub.get(&g.id) {
                self.check_constraints(t, span);
                for bound in &g.bounds {
                    let b = bound.substitute(sub);
                    if !self.conforms(t, &b) {
                        self.error(
                            "ZEN-TYPE-0030",
                            span,
                            format!(
                                "{} does not explicitly implement {}",
                                self.show(t),
                                self.show(&b)
                            ),
                        );
                    }
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_signature(
        &mut self,
        f: &Signature,
        explicit: &[TypeRef],
        args: &[Arg],
        expected: Option<&Type>,
        span: Span,
        callee: &Expr,
        bound: bool,
    ) -> Type {
        let skip = usize::from(bound && f.params.first().is_some_and(|(n, _, _)| n.text == "self"));
        if bound && skip == 0 {
            self.error(
                "ZEN-IMPL-0002",
                callee.span,
                "instance method must explicitly declare self",
            );
        }
        let params = &f.params[skip..];
        self.ide.calls.push(CallSite {
            span,
            callee: callee.span,
            symbol: f.symbol,
            parameters: params
                .iter()
                .map(|(n, t, _)| (n.text.clone(), t.clone()))
                .collect(),
            arguments: args
                .iter()
                .map(|a| (a.value.span, a.label.as_ref().map(|n| n.text.clone())))
                .collect(),
        });
        let named = args.iter().any(|a| a.label.is_some());
        if named && args.iter().any(|a| a.label.is_none()) {
            self.error(
                "ZEN-TYPE-0007",
                span,
                "cannot mix named and positional arguments",
            );
        }
        let ids = f.generics.iter().map(|g| g.id).collect::<Vec<_>>();
        let mut sub = BTreeMap::new();
        if !explicit.is_empty() {
            if explicit.len() != ids.len() {
                self.error(
                    "ZEN-TYPE-0002",
                    callee.span,
                    "incorrect number of generic arguments",
                );
            }
            for (p, a) in ids.iter().zip(explicit) {
                sub.insert(*p, self.value_type(a));
            }
        }
        let output = if f.asynchronous {
            task(f.result.clone())
        } else {
            f.result.clone()
        };
        if let Some(expected) = expected {
            self.infer(&output, expected, &ids, &mut sub, span);
        }
        let mut used = BTreeSet::new();
        let mut checked = vec![];
        for (i, a) in args.iter().enumerate() {
            let index = if let Some(label) = &a.label {
                let index = params.iter().position(|(n, _, _)| n.text == label.text);
                if index.is_none() {
                    self.error(
                        "ZEN-TYPE-0009",
                        label.span,
                        format!("unknown argument label `{}`", label.text),
                    );
                }
                index
            } else if i < params.len() {
                Some(i)
            } else {
                None
            };
            if let Some(index) = index {
                if let Some(label) = &a.label {
                    self.ide
                        .pending_references
                        .push((label.span, params[index].0.span));
                }
                if !used.insert(index) {
                    self.error(
                        "ZEN-TYPE-0009",
                        a.value.span,
                        "argument provided more than once",
                    );
                }
                let template = &params[index].1;
                let context = template.substitute(&sub);
                let ty = self.expr(
                    &a.value,
                    if context.contains_param(&ids) {
                        None
                    } else {
                        Some(&context)
                    },
                );
                self.infer(template, &ty, &ids, &mut sub, a.value.span);
                checked.push((a.value.span, template.clone(), ty));
            } else {
                self.expr(&a.value, None);
                if a.label.is_none() {
                    self.error("ZEN-TYPE-0008", a.value.span, "too many arguments");
                }
            }
        }
        for (i, (n, _, default)) in params.iter().enumerate() {
            if !used.contains(&i) && default.is_none() {
                self.error(
                    "ZEN-TYPE-0008",
                    span,
                    format!("missing argument `{}`", n.text),
                );
            }
        }
        self.finish_inference(f, &ids, &sub, span);
        for (s, template, ty) in checked {
            self.expect_type(&ty, &template.substitute(&sub), s);
        }
        let result = output.substitute(&sub);
        let callable = Type::Function(
            params.iter().map(|(_, t, _)| t.substitute(&sub)).collect(),
            Box::new(result.clone()),
        );
        self.record(callee, callable, Some(f.symbol));
        result
    }
}
