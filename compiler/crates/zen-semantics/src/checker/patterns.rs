use super::*;

impl Checker {
    pub(super) fn equality(&self, t: &Type, seen: &mut BTreeSet<String>) -> bool {
        if seen.len() > 64 {
            return false;
        }
        match t {
            Type::Bool
            | Type::Int
            | Type::Float
            | Type::String
            | Type::Char
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::F32
            | Type::Never
            | Type::Error => true,
            Type::Nominal(id, args) => {
                if !seen.insert(format!("{t:?}")) {
                    return true;
                }
                let sub = self.substitution(*id, args);
                let result = match &self.nominals[id.0].kind {
                    Definition::Struct(fields) => fields
                        .iter()
                        .all(|f| self.equality(&f.ty.substitute(&sub), seen)),
                    Definition::Enum(v) => v
                        .iter()
                        .all(|(_, a)| a.iter().all(|t| self.equality(&t.substitute(&sub), seen))),
                    _ => false,
                };
                seen.remove(&format!("{t:?}"));
                result
            }
            _ => false,
        }
    }
    pub(super) fn pattern(&mut self, p: &Pattern, ty: &Type) {
        match &p.kind {
            PatternKind::Wildcard => {}
            PatternKind::Bind(n) => self.bind(n, ty.clone(), false),
            PatternKind::Literal(l) => {
                let t = self.literal_type(l, p.span, false);
                self.expect_type(&t, ty, p.span);
            }
            PatternKind::Variant(qualifier, n, children) => {
                let Type::Nominal(id, args) = ty else {
                    self.error(
                        "ZEN-MATCH-0002",
                        p.span,
                        "enum pattern requires an enum scrutinee",
                    );
                    return;
                };
                if let Some(q) = qualifier
                    && self.type_id(q) != Some(*id)
                {
                    self.error("ZEN-MATCH-0002", p.span, "pattern names a different enum");
                }
                if let Some(q) = qualifier
                    && self.type_id(q) == Some(*id)
                {
                    self.ide.reference(
                        Span::new(p.span.file, p.span.start, p.span.start + q.len()),
                        Entity::Nominal(*id),
                    );
                }
                let Definition::Enum(variants) = self.nominals[id.0].kind.clone() else {
                    self.error("ZEN-MATCH-0002", p.span, "pattern requires an enum");
                    return;
                };
                let Some((_, payload)) = variants.iter().find(|(v, _)| v.text == n.text) else {
                    self.error("ZEN-MATCH-0002", n.span, "unknown variant in pattern");
                    return;
                };
                self.ide
                    .reference(n.span, Entity::Member(*id, n.text.clone()));
                if payload.len() != children.len() {
                    self.error("ZEN-MATCH-0002", p.span, "incorrect pattern payload count");
                }
                let sub = self.substitution(*id, args);
                for (p, t) in children.iter().zip(payload) {
                    self.pattern(p, &t.substitute(&sub));
                }
            }
        }
    }
    pub(super) fn match_expr(
        &mut self,
        value: &Expr,
        arms: &[(Pattern, Expr)],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let ty = self.expr(value, None);
        self.ide.matches.push((
            span,
            ty.clone(),
            arms.iter()
                .filter_map(|(p, _)| {
                    if let PatternKind::Variant(_, n, children) = &p.kind {
                        if children
                            .iter()
                            .all(|p| matches!(p.kind, PatternKind::Wildcard | PatternKind::Bind(_)))
                        {
                            Some(n.text.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect(),
        ));
        let mut result = Type::Never;
        let mut patterns = vec![];
        for (p, e) in arms {
            self.ctx.locals.push(BTreeMap::new());
            self.pattern(p, &ty);
            let context = expected.or(if result == Type::Never {
                None
            } else {
                Some(&result)
            });
            let t = self.expr(e, context);
            result = self.join(&result, &t, e.span);
            patterns.push(vec![p.clone()]);
            self.ctx.locals.pop();
        }
        if ty != Type::Error && !self.exhaustive(std::slice::from_ref(&ty), &patterns, 0) {
            self.error(
                "ZEN-MATCH-0001",
                span,
                "non-exhaustive match; add missing cases or a wildcard `_`",
            );
            if let Type::Nominal(id, _) = &ty
                && let Definition::Enum(variants) = &self.nominals[id.0].kind
            {
                let missing: Vec<_> = variants.iter().filter(|(n,_)| !arms.iter().any(|(p,_)| matches!(&p.kind, PatternKind::Variant(_,name,children) if name.text==n.text && children.iter().all(|p|matches!(p.kind,PatternKind::Wildcard|PatternKind::Bind(_)))))).map(|(n,payload)|format!("\n    .{}{} => {{ /* TODO: implement this arm */ }};",n.text,if payload.is_empty(){String::new()}else{format!("({})",vec!["_";payload.len()].join(", "))})).collect();
                if !missing.is_empty() {
                    self.diagnostics
                        .last_mut()
                        .unwrap()
                        .fixes
                        .push(SuggestedFix {
                            message: "Add missing match arms (TODO stubs)".into(),
                            replacement: Some((
                                Span::new(
                                    span.file,
                                    span.end.saturating_sub(1),
                                    span.end.saturating_sub(1),
                                ),
                                format!("{}\n", missing.join("")),
                            )),
                        });
                }
            }
        }
        result
    }
    // Pattern-matrix specialization handles nested payload patterns and correlated columns.
    // Open domains require a default row. A depth bound conservatively rejects recursive
    // coverage proofs rather than accepting a match that could fail at runtime.
    pub(super) fn exhaustive(&self, types: &[Type], rows: &[Vec<Pattern>], depth: usize) -> bool {
        if types.is_empty() {
            return !rows.is_empty();
        }
        if depth > 64 {
            return false;
        }
        if rows.is_empty() {
            return false;
        }
        let wild = |p: &Pattern| matches!(p.kind, PatternKind::Wildcard | PatternKind::Bind(_));
        let defaults = rows
            .iter()
            .filter(|r| r.first().is_some_and(wild))
            .map(|r| r[1..].to_vec())
            .collect::<Vec<_>>();
        if self.exhaustive(&types[1..], &defaults, depth + 1) {
            return true;
        }
        let constructors: Vec<(String, Vec<Type>)> = match &types[0] {
            Type::Bool => vec![("true".into(), vec![]), ("false".into(), vec![])],
            Type::Nominal(id, args) => {
                let Definition::Enum(v) = &self.nominals[id.0].kind else {
                    return false;
                };
                let sub = self.substitution(*id, args);
                v.iter()
                    .map(|(n, a)| {
                        (
                            n.text.clone(),
                            a.iter().map(|t| t.substitute(&sub)).collect(),
                        )
                    })
                    .collect()
            }
            _ => return false,
        };
        constructors.iter().all(|(name, payload)| {
            let mut specialized = vec![];
            for row in rows {
                let Some(head) = row.first() else {
                    continue;
                };
                let children = if wild(head) {
                    Some(
                        payload
                            .iter()
                            .map(|_| Pattern {
                                span: head.span,
                                kind: PatternKind::Wildcard,
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    match &head.kind {
                        PatternKind::Literal(Literal::Bool(b))
                            if name == if *b { "true" } else { "false" } =>
                        {
                            Some(vec![])
                        }
                        PatternKind::Variant(_, n, c)
                            if n.text == *name && c.len() == payload.len() =>
                        {
                            Some(c.clone())
                        }
                        _ => None,
                    }
                };
                if let Some(mut children) = children {
                    children.extend_from_slice(&row[1..]);
                    specialized.push(children);
                }
            }
            let mut next = payload.clone();
            next.extend_from_slice(&types[1..]);
            self.exhaustive(&next, &specialized, depth + 1)
        })
    }
}
