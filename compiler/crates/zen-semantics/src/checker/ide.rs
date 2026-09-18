use super::*;
impl Checker {
    pub(super) fn capture_scope(&mut self, span: Span) {
        let mut candidates = BTreeMap::new();
        for (index, n) in self.nominals.iter().take(6).enumerate() {
            candidates.insert(
                n.name.text.clone(),
                Candidate {
                    name: n.name.text.clone(),
                    entity: Entity::Nominal(TypeId(index)),
                    ty: Type::Nominal(TypeId(index), vec![]),
                },
            );
        }
        let names = &self.names[self.ctx.module];
        for (name, id) in names.import_types.iter().chain(names.types.iter()) {
            candidates.insert(
                name.clone(),
                Candidate {
                    name: name.clone(),
                    entity: Entity::Nominal(*id),
                    ty: Type::Nominal(*id, vec![]),
                },
            );
        }
        for (name, id) in names.import_values.iter().chain(names.values.iter()) {
            candidates.insert(
                name.clone(),
                Candidate {
                    name: name.clone(),
                    entity: Entity::Value(*id),
                    ty: self.symbols[id.0].ty.clone(),
                },
            );
        }
        for g in &self.ctx.generics {
            candidates.insert(
                g.name.clone(),
                Candidate {
                    name: g.name.clone(),
                    entity: Entity::Generic(g.id),
                    ty: Type::Param(g.id),
                },
            );
        }
        for scope in &self.ctx.locals {
            for (name, l) in scope {
                candidates.insert(
                    name.clone(),
                    Candidate {
                        name: name.clone(),
                        entity: Entity::Value(l.symbol),
                        ty: l.ty.clone(),
                    },
                );
            }
        }
        let mut candidates: Vec<_> = candidates.into_values().collect();
        candidates.sort_by_key(|c| {
            if self.local(&c.name).is_some() {
                0
            } else if names.values.contains_key(&c.name) || names.types.contains_key(&c.name) {
                1
            } else if names.import_values.contains_key(&c.name)
                || names.import_types.contains_key(&c.name)
            {
                2
            } else {
                3
            }
        });
        self.ide.scopes.push((span, candidates));
    }
    pub(super) fn finish_index(&mut self) {
        for module in 0..self.modules.len() {
            self.ctx = Context {
                module,
                ..Context::default()
            };
            self.capture_scope(Span::new(self.modules[module].file, 0, usize::MAX));
        }

        for s in &self.symbols {
            if s.span.end == 0 {
                continue;
            }
            self.ide.declarations.push(Declaration {
                entity: Entity::Value(s.id),
                name: s.name.clone(),
                selection: s.span,
                range: s.span,
                kind: Kind::Variable,
                ty: s.ty.clone(),
                parent: None,
                detail: String::new(),
            });
        }
        for (i, n) in self.nominals.iter().enumerate() {
            let id = TypeId(i);
            if n.module == usize::MAX {
                continue;
            }
            let entity = Entity::Nominal(id);
            let ty = Type::Nominal(id, n.generics.iter().map(|g| Type::Param(g.id)).collect());
            let kind = match n.kind {
                Definition::Struct(_) => Kind::Struct,
                Definition::Enum(_) => Kind::Enum,
                _ => Kind::Interface,
            };
            self.ide.declarations.push(Declaration {
                entity: entity.clone(),
                name: n.name.text.clone(),
                selection: n.name.span,
                range: n.name.span,
                kind,
                ty: ty.clone(),
                parent: None,
                detail: String::new(),
            });
            if let Definition::Interface(methods) = &n.kind {
                for m in methods {
                    if let Some(d) = self
                        .ide
                        .declarations
                        .iter_mut()
                        .find(|d| d.entity == Entity::Value(m.symbol))
                    {
                        d.parent = Some(entity.clone());
                    }
                }
            }
            let children: Vec<_> = match &n.kind {
                Definition::Struct(fs) => fs
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone(), Kind::Field))
                    .collect(),
                Definition::Enum(vs) => vs
                    .iter()
                    .map(|(n, p)| {
                        (
                            n.clone(),
                            Type::Function(p.clone(), Box::new(ty.clone())),
                            Kind::Variant,
                        )
                    })
                    .collect(),
                _ => vec![],
            };
            for (name, t, kind) in children {
                self.ide.declarations.push(Declaration {
                    entity: Entity::Member(id, name.text.clone()),
                    name: name.text,
                    selection: name.span,
                    range: name.span,
                    kind,
                    ty: t,
                    parent: Some(entity.clone()),
                    detail: String::new(),
                });
            }
        }
        for (id, n) in &self.ide.generics {
            self.ide.declarations.push(Declaration {
                entity: Entity::Generic(*id),
                name: n.text.clone(),
                selection: n.span,
                range: n.span,
                kind: Kind::TypeParameter,
                ty: Type::Param(*id),
                parent: None,
                detail: String::new(),
            });
        }
        let signatures: Vec<_> = self
            .functions
            .values()
            .cloned()
            .chain(self.implementations.iter().flat_map(|i| i.methods.clone()))
            .chain(self.nominals.iter().flat_map(|n| {
                if let Definition::Interface(m) = &n.kind {
                    m.clone()
                } else {
                    vec![]
                }
            }))
            .collect();
        for f in signatures {
            let detail = format!(
                "{}fn {}({}) -> {}",
                if f.asynchronous { "async " } else { "" },
                f.name.text,
                f.params
                    .iter()
                    .map(|(n, t, d)| format!(
                        "{}: {}{}",
                        n.text,
                        self.show(t),
                        if d.is_some() { " = …" } else { "" }
                    ))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.show(&f.result)
            );
            if let Some(d) = self
                .ide
                .declarations
                .iter_mut()
                .find(|d| d.entity == Entity::Value(f.symbol))
            {
                d.kind = if self.functions.contains_key(&f.symbol) {
                    Kind::Function
                } else {
                    Kind::Method
                };
                d.detail = detail;
                d.range.end = f.body.as_ref().map_or(f.name.span.end, |b| b.span.end);
            }
            for (n, _, _) in f.params {
                if let Some(d) = self
                    .ide
                    .declarations
                    .iter_mut()
                    .find(|d| d.selection == n.span)
                {
                    d.kind = Kind::Parameter;
                }
            }
        }
        for i in &self.implementations {
            if let Some(Type::Nominal(id, _)) = &i.interface {
                self.ide
                    .implementations
                    .push((Entity::Nominal(*id), i.span));
                if let Definition::Interface(methods) = &self.nominals[id.0].kind {
                    for m in methods {
                        if let Some(f) = i.methods.iter().find(|f| f.name.text == m.name.text) {
                            self.ide
                                .implementations
                                .push((Entity::Value(m.symbol), f.name.span));
                        }
                    }
                }
            }
        }
        for (usage, definition) in self.ide.pending_references.clone() {
            if let Some(d) = self
                .ide
                .declarations
                .iter()
                .find(|d| d.selection == definition)
            {
                self.ide.reference(usage, d.entity.clone());
            }
        }
        for module in &self.modules {
            for decl in &module.syntax.declarations {
                let name = match &decl.kind {
                    DeclKind::Struct(n, ..)
                    | DeclKind::Enum(n, ..)
                    | DeclKind::Interface(n, ..)
                    | DeclKind::Const(n, ..) => Some(n),
                    DeclKind::Function(f) => Some(&f.name),
                    _ => None,
                };
                if let Some(n) = name
                    && let Some(d) = self
                        .ide
                        .declarations
                        .iter_mut()
                        .find(|d| d.selection == n.span)
                {
                    d.range = decl.span;
                    if matches!(decl.kind, DeclKind::Const(..)) {
                        d.kind = Kind::Constant;
                    }
                }
            }
        }
        self.ide
            .declarations
            .sort_by_key(|d| (d.selection.file, d.selection.start));
    }
}

impl Checker {
    /// Enumerate names, then use the *same* resolver as calls to test applicability.
    /// Speculative lookup diagnostics must not change program diagnostics.
    pub(super) fn capture_members(&mut self, ty: &Type) {
        let file = self.modules[self.ctx.module].file;
        if self.ide.member_contexts.contains(&(ty.clone(), file)) {
            return;
        }
        self.ide.member_contexts.push((ty.clone(), file));
        let mut names: BTreeSet<String> = self
            .implementations
            .iter()
            .flat_map(|i| i.methods.iter().map(|m| m.name.text.clone()))
            .collect();
        for n in &self.nominals {
            if let Definition::Interface(methods) = &n.kind {
                names.extend(methods.iter().map(|m| m.name.text.clone()));
            }
        }
        names.extend(["get", "first", "append"].map(str::to_owned));
        for name in names {
            let before = self.diagnostics.len();
            let resolved = self.method(
                ty,
                &Name {
                    text: name.clone(),
                    span: Span::default(),
                },
            );
            let valid = self.diagnostics.len() == before;
            self.diagnostics.truncate(before);
            if valid
                && let Some(mut method) = resolved
                && method
                    .params
                    .first()
                    .is_some_and(|(n, _, _)| n.text == "self")
            {
                method.params.remove(0);
                self.ide.members.push((
                    ty.clone(),
                    file,
                    false,
                    Candidate {
                        name,
                        entity: Entity::Value(method.symbol),
                        ty: method.ty(),
                    },
                ));
            }
        }
    }
}
