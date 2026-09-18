use super::*;

impl Checker {
    pub(super) fn error(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }
    pub(super) fn new_symbol(&mut self, name: &Name, ty: Type) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            id,
            name: name.text.clone(),
            span: name.span,
            ty,
        });
        id
    }
    pub(super) fn generic_shell(&mut self, g: &[Generic]) -> Vec<GenericParam> {
        let mut seen = BTreeSet::new();
        g.iter()
            .map(|p| {
                if !seen.insert(p.name.text.clone()) {
                    self.error("ZEN-NAME-0003", p.name.span, "duplicate generic parameter");
                }
                let id = ParamId(self.next_param);
                self.next_param += 1;
                self.ide.generics.insert(id, p.name.clone());
                GenericParam {
                    id,
                    name: p.name.text.clone(),
                    bounds: vec![],
                }
            })
            .collect()
    }
    pub(super) fn prelude(&mut self) {
        for (name, count) in [
            ("Option", 1),
            ("Result", 2),
            ("Task", 1),
            ("List", 1),
            ("Map", 2),
            ("Set", 1),
        ] {
            let mut generics = vec![];
            for i in 0..count {
                generics.push(GenericParam {
                    id: ParamId(self.next_param),
                    name: format!("T{i}"),
                    bounds: vec![],
                });
                self.next_param += 1;
            }
            let n = Name {
                text: name.into(),
                span: Span::default(),
            };
            let kind = match name {
                "Option" => Definition::Enum(vec![
                    (
                        Name {
                            text: "some".into(),
                            span: n.span,
                        },
                        vec![Type::Param(generics[0].id)],
                    ),
                    (
                        Name {
                            text: "none".into(),
                            span: n.span,
                        },
                        vec![],
                    ),
                ]),
                "Result" => Definition::Enum(vec![
                    (
                        Name {
                            text: "ok".into(),
                            span: n.span,
                        },
                        vec![Type::Param(generics[0].id)],
                    ),
                    (
                        Name {
                            text: "err".into(),
                            span: n.span,
                        },
                        vec![Type::Param(generics[1].id)],
                    ),
                ]),
                _ => Definition::Builtin,
            };
            self.nominals.push(Nominal {
                name: n,
                module: usize::MAX,
                public: true,
                generics,
                kind,
            });
        }
    }
    pub(super) fn collect(&mut self) {
        for m in 0..self.modules.len() {
            let decls = self.modules[m].syntax.declarations.clone();
            for (i, d) in decls.iter().enumerate() {
                match &d.kind {
                    DeclKind::Struct(n, g, _)
                    | DeclKind::Enum(n, g, _)
                    | DeclKind::Interface(n, g, _) => {
                        let id = TypeId(self.nominals.len());
                        if let Some(old) = self.names[m].types.get(&n.text) {
                            self.duplicate(n, self.nominals[old.0].name.span);
                        } else {
                            self.names[m].types.insert(n.text.clone(), id);
                        }
                        let generics = self.generic_shell(g);
                        let kind = match d.kind {
                            DeclKind::Struct(..) => Definition::Struct(vec![]),
                            DeclKind::Enum(..) => Definition::Enum(vec![]),
                            _ => Definition::Interface(vec![]),
                        };
                        self.nominals.push(Nominal {
                            name: n.clone(),
                            module: m,
                            public: d.public,
                            generics,
                            kind,
                        });
                        self.decl_ids.insert((m, i), id.0);
                    }
                    DeclKind::Function(Function { name: n, .. }) | DeclKind::Const(n, ..) => {
                        let id = self.new_symbol(n, Type::Error);
                        if let Some(old) = self.names[m].values.get(&n.text) {
                            self.duplicate(n, self.symbols[old.0].span);
                        } else {
                            self.names[m].values.insert(n.text.clone(), id);
                        }
                        self.decl_ids.insert((m, i), id.0);
                    }
                    _ => {}
                }
            }
        }
    }
    pub(super) fn duplicate(&mut self, n: &Name, old: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                "ZEN-NAME-0003",
                n.span,
                format!("duplicate declaration `{}`", n.text),
            )
            .label(old, "previous declaration"),
        );
    }
    pub(super) fn imports(&mut self) {
        for m in 0..self.modules.len() {
            let mut seen = BTreeMap::new();
            for import in self.modules[m].syntax.imports.clone() {
                let Some(last) = import.path.last() else {
                    continue;
                };
                let local = import.alias.as_ref().unwrap_or(last);
                if let Some(old) = seen.insert(local.text.clone(), local.span) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "ZEN-NAME-0002",
                            local.span,
                            "imports introduce the same local name",
                        )
                        .label(old, "other import"),
                    );
                    continue;
                }
                let path = import.path[..import.path.len() - 1]
                    .iter()
                    .map(|n| n.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let Some(other) = self.modules.iter().position(|x| x.name == path) else {
                    self.error(
                        "ZEN-NAME-0001",
                        import.span,
                        format!("unresolved module `{path}`"),
                    );
                    continue;
                };
                if import.path.len() > 1 {
                    self.ide.namespaces.push(
                        import.path[0]
                            .span
                            .join(import.path[import.path.len() - 2].span),
                    );
                }
                let t = self.names[other].types.get(&last.text).copied();
                let v = self.names[other].values.get(&last.text).copied();
                let public = self.modules[other].syntax.declarations.iter().any(|d| {
                    d.public
                        && match &d.kind {
                            DeclKind::Struct(n, ..)
                            | DeclKind::Enum(n, ..)
                            | DeclKind::Interface(n, ..)
                            | DeclKind::Const(n, ..) => n.text == last.text,
                            DeclKind::Function(f) => f.name.text == last.text,
                            _ => false,
                        }
                });
                if !public {
                    self.error(
                        "ZEN-NAME-0005",
                        import.span,
                        "imported declaration is missing or private",
                    );
                    continue;
                }
                if let Some(t) = t {
                    self.ide.reference(last.span, Entity::Nominal(t));
                    self.names[m].import_types.insert(local.text.clone(), t);
                }
                if let Some(v) = v {
                    self.ide.reference(last.span, Entity::Value(v));
                    self.names[m].import_values.insert(local.text.clone(), v);
                }
            }
        }
    }
    pub(super) fn type_id(&self, name: &str) -> Option<TypeId> {
        let n = &self.names[self.ctx.module];
        n.types
            .get(name)
            .or_else(|| n.import_types.get(name))
            .copied()
            .or_else(|| {
                self.nominals
                    .iter()
                    .take(6)
                    .position(|t| t.name.text == name)
                    .map(TypeId)
            })
    }
    pub(super) fn resolve(&mut self, t: &TypeRef) -> Type {
        self.ide.type_contexts.push(t.span);
        self.capture_scope(t.span);
        match &t.kind {
            TypeKind::Function(a, r) => Type::Function(
                a.iter().map(|x| self.resolve(x)).collect(),
                Box::new(self.resolve(r)),
            ),
            TypeKind::Named(n, a) => {
                let primitive = match n.as_str() {
                    "Bool" => Some(Type::Bool),
                    "Int" => Some(Type::Int),
                    "Float" | "F64" => Some(Type::Float),
                    "String" => Some(Type::String),
                    "Char" => Some(Type::Char),
                    "Unit" => Some(Type::Unit),
                    "Never" => Some(Type::Never),
                    "I8" => Some(Type::I8),
                    "I16" => Some(Type::I16),
                    "I32" => Some(Type::I32),
                    "I64" => Some(Type::I64),
                    "U8" => Some(Type::U8),
                    "U16" => Some(Type::U16),
                    "U32" => Some(Type::U32),
                    "U64" => Some(Type::U64),
                    "F32" => Some(Type::F32),
                    _ => None,
                };
                if let Some(p) = primitive {
                    self.ide.builtin_types.push(Span::new(
                        t.span.file,
                        t.span.start,
                        t.span.start + n.len(),
                    ));
                    if !a.is_empty() {
                        self.error(
                            "ZEN-TYPE-0002",
                            t.span,
                            "primitive type takes no generic arguments",
                        );
                    }
                    return p;
                }
                if let Some(g) = self.ctx.generics.iter().find(|g| g.name == *n) {
                    let id = g.id;
                    if !a.is_empty() {
                        self.error(
                            "ZEN-TYPE-0002",
                            t.span,
                            "generic parameter takes no type arguments",
                        );
                    }
                    self.ide.reference(
                        Span::new(t.span.file, t.span.start, t.span.start + n.len()),
                        Entity::Generic(id),
                    );
                    return Type::Param(id);
                }
                let Some(id) = self.type_id(n) else {
                    self.error("ZEN-NAME-0004", t.span, format!("unknown type `{n}`"));
                    return Type::Error;
                };
                self.ide.reference(
                    Span::new(t.span.file, t.span.start, t.span.start + n.len()),
                    Entity::Nominal(id),
                );
                let args = a.iter().map(|x| self.resolve(x)).collect::<Vec<_>>();
                let g = self.nominals[id.0].generics.clone();
                if args.len() != g.len() {
                    self.error(
                        "ZEN-TYPE-0002",
                        t.span,
                        format!("`{n}` requires {} type arguments", g.len()),
                    );
                    return Type::Error;
                }
                Type::Nominal(id, args)
            }
        }
    }
    pub(super) fn value_type(&mut self, t: &TypeRef) -> Type {
        let ty = self.resolve(t);
        self.reject_interface_value(&ty, t.span);
        ty
    }
    pub(super) fn reject_interface_value(&mut self, t: &Type, s: Span) {
        match t {
            Type::Nominal(id, a) => {
                if matches!(self.nominals[id.0].kind, Definition::Interface(_)) {
                    self.error(
                        "ZEN-TYPE-0003",
                        s,
                        "interfaces are constraints, not first-class value types",
                    );
                }
                for t in a {
                    self.reject_interface_value(t, s);
                }
            }
            Type::Function(a, r) => {
                for t in a {
                    self.reject_interface_value(t, s);
                }
                self.reject_interface_value(r, s);
            }
            _ => {}
        }
    }
    pub(super) fn bounds(
        &mut self,
        syntax: &[Generic],
        mut generics: Vec<GenericParam>,
    ) -> Vec<GenericParam> {
        self.ctx.generics = generics.clone();
        for (s, g) in syntax.iter().zip(&mut generics) {
            for b in &s.bounds {
                let t = self.resolve(b);
                if !matches!(&t,Type::Nominal(id,_) if matches!(self.nominals[id.0].kind,Definition::Interface(_)))
                {
                    self.error(
                        "ZEN-IMPL-0001",
                        b.span,
                        "generic bound must name an interface",
                    );
                }
                g.bounds.push(t);
            }
        }
        self.ctx.generics = generics.clone();
        generics
    }
    pub(super) fn signature(
        &mut self,
        f: &Function,
        receiver: Option<Type>,
        inherited: Vec<GenericParam>,
        symbol: Option<SymbolId>,
    ) -> Signature {
        let mut generics = inherited;
        let shell = self.generic_shell(&f.generics);
        generics.extend(shell);
        self.ctx.generics = generics.clone();
        let offset = generics.len() - f.generics.len();
        for (i, g) in f.generics.iter().enumerate() {
            for b in &g.bounds {
                let t = self.resolve(b);
                if !matches!(&t,Type::Nominal(id,_) if matches!(self.nominals[id.0].kind,Definition::Interface(_)))
                {
                    self.error("ZEN-IMPL-0001", b.span, "bound must be an interface");
                }
                generics[offset + i].bounds.push(t);
            }
        }
        self.ctx.generics = generics.clone();
        let mut params = vec![];
        let mut seen = BTreeMap::new();
        for (i, p) in f.params.iter().enumerate() {
            if let Some(old) = seen.insert(p.name.text.clone(), p.name.span) {
                self.duplicate(&p.name, old);
            }
            let ty = if p.name.text == "self" {
                if i != 0 || p.ty.is_some() || p.default.is_some() {
                    self.error(
                        "ZEN-IMPL-0002",
                        p.name.span,
                        "self must be the first, unannotated receiver",
                    );
                }
                receiver.clone().unwrap_or_else(|| {
                    self.error(
                        "ZEN-IMPL-0002",
                        p.name.span,
                        "self is only valid in a method",
                    );
                    Type::Error
                })
            } else {
                p.ty.as_ref()
                    .map(|t| self.value_type(t))
                    .unwrap_or(Type::Error)
            };
            params.push((p.name.clone(), ty, p.default.clone()));
        }
        let result = self.value_type(&f.result);
        let symbol = symbol.unwrap_or_else(|| self.new_symbol(&f.name, Type::Error));
        let sig = Signature {
            symbol,
            name: f.name.clone(),
            module: self.ctx.module,
            public: f.public,
            generics,
            params,
            result,
            asynchronous: f.asynchronous,
            body: f.body.clone(),
        };
        self.symbols[symbol.0].ty = sig.ty();
        sig
    }
    pub(super) fn signatures(&mut self) {
        for m in 0..self.modules.len() {
            self.ctx = Context {
                module: m,
                ..Context::default()
            };
            for (i, d) in self.modules[m]
                .syntax
                .declarations
                .clone()
                .iter()
                .enumerate()
            {
                match &d.kind {
                    DeclKind::Struct(_, g, _)
                    | DeclKind::Enum(_, g, _)
                    | DeclKind::Interface(_, g, _) => {
                        let id = self.decl_ids[&(m, i)];
                        let generics = self.bounds(g, self.nominals[id].generics.clone());
                        self.nominals[id].generics = generics.clone();
                        let mut seen = BTreeMap::new();
                        let kind = match &d.kind {
                            DeclKind::Struct(_, _, _) => {
                                let mut out = vec![];
                                for f in fields_for(d) {
                                    if let Some(old) = seen.insert(f.name.text.clone(), f.name.span)
                                    {
                                        self.duplicate(&f.name, old);
                                    }
                                    let ty = self.value_type(&f.ty);
                                    out.push(FieldDef {
                                        name: f.name.clone(),
                                        ty,
                                        public: f.public,
                                    });
                                }
                                Definition::Struct(out)
                            }
                            DeclKind::Enum(_, _, variants) => {
                                let mut out = vec![];
                                for v in variants {
                                    if let Some(old) = seen.insert(v.name.text.clone(), v.name.span)
                                    {
                                        self.duplicate(&v.name, old);
                                    }
                                    out.push((
                                        v.name.clone(),
                                        v.payload.iter().map(|t| self.value_type(t)).collect(),
                                    ));
                                }
                                Definition::Enum(out)
                            }
                            DeclKind::Interface(_, _, methods) => {
                                let receiver = Type::Nominal(
                                    TypeId(id),
                                    generics.iter().map(|g| Type::Param(g.id)).collect(),
                                );
                                let mut out = vec![];
                                for f in methods {
                                    if let Some(old) = seen.insert(f.name.text.clone(), f.name.span)
                                    {
                                        self.duplicate(&f.name, old);
                                    }
                                    let mut signature = self.signature(
                                        f,
                                        Some(receiver.clone()),
                                        generics.clone(),
                                        None,
                                    );
                                    signature.public = self.nominals[id].public;
                                    out.push(signature);
                                }
                                Definition::Interface(out)
                            }
                            _ => unreachable!(),
                        };
                        self.nominals[id].kind = kind;
                        self.ctx.generics.clear();
                    }
                    DeclKind::Function(f) => {
                        let id = SymbolId(self.decl_ids[&(m, i)]);
                        let sig = self.signature(f, None, vec![], Some(id));
                        self.functions.insert(id, sig);
                        self.ctx.generics.clear();
                    }
                    DeclKind::Const(_, t, e) => {
                        let id = SymbolId(self.decl_ids[&(m, i)]);
                        let ty = self.value_type(t);
                        self.symbols[id.0].ty = ty.clone();
                        self.constants.insert(id, (m, ty, e.clone(), d.public));
                    }
                    _ => {}
                }
            }
        }
        // Implementations require every interface signature to have been collected first.
        for m in 0..self.modules.len() {
            self.ctx = Context {
                module: m,
                ..Context::default()
            };
            for d in self.modules[m].syntax.declarations.clone() {
                if let DeclKind::Impl {
                    interface,
                    target,
                    methods,
                } = d.kind
                {
                    let target = self.value_type(&target);
                    let interface = interface.as_ref().map(|t| self.resolve(t));
                    let mut out = vec![];
                    let mut seen = BTreeMap::new();
                    for f in methods {
                        if let Some(old) = seen.insert(f.name.text.clone(), f.name.span) {
                            self.duplicate(&f.name, old);
                        }
                        out.push(self.signature(&f, Some(target.clone()), vec![], None));
                    }
                    if interface.is_none()
                        && !matches!(&target,Type::Nominal(id,_) if self.nominals[id.0].module==m)
                    {
                        self.error("ZEN-IMPL-0003",d.span,"inherent implementation must be in the module defining its nominal type");
                    }
                    self.implementations.push(Implementation {
                        span: d.span,
                        module: m,
                        interface,
                        target,
                        methods: out,
                    });
                }
            }
        }
    }
    pub(super) fn show(&self, t: &Type) -> String {
        match t {
            Type::Nominal(id, a) => {
                let n = &self.nominals[id.0].name.text;
                if a.is_empty() {
                    n.clone()
                } else {
                    format!(
                        "{}<{}>",
                        n,
                        a.iter()
                            .map(|t| self.show(t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Type::Param(id) => self
                .ctx
                .generics
                .iter()
                .find(|g| g.id == *id)
                .map(|g| g.name.clone())
                .or_else(|| self.ide.generics.get(id).map(|n| n.text.clone()))
                .unwrap_or_else(|| format!("T{}", id.0)),
            Type::Function(a, r) => format!(
                "({}) -> {}",
                a.iter()
                    .map(|t| self.show(t))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.show(r)
            ),
            _ => format!("{t:?}"),
        }
    }
    pub(super) fn expect_type(&mut self, actual: &Type, expected: &Type, span: Span) {
        if *actual != Type::Error && *expected != Type::Error && !actual.assignable_to(expected) {
            let mut d = Diagnostic::error(
                "ZEN-TYPE-0001",
                span,
                "incompatible types; no implicit conversion exists",
            );
            d.expected = Some(self.show(expected));
            d.actual = Some(self.show(actual));
            self.diagnostics.push(d);
        }
    }
    pub(super) fn substitution(&self, id: TypeId, args: &[Type]) -> BTreeMap<ParamId, Type> {
        self.nominals[id.0]
            .generics
            .iter()
            .zip(args)
            .map(|(g, a)| (g.id, a.clone()))
            .collect()
    }
    pub(super) fn validate_impls(&mut self) {
        let implementations = self.implementations.clone();
        let mut pairs = BTreeSet::new();
        let mut inherent = BTreeMap::new();
        for imp in implementations {
            let location = imp.span;
            self.ctx = Context {
                module: imp.module,
                ..Context::default()
            };
            self.check_constraints(&imp.target, location);
            if let Some(interface) = &imp.interface {
                self.check_constraints(interface, location);
            }
            if let Some(interface) = &imp.interface {
                let key = format!("{:?}:{:?}", imp.target, interface);
                if !pairs.insert(key) {
                    self.error(
                        "ZEN-IMPL-0004",
                        location,
                        "duplicate interface implementation",
                    );
                }
                let Type::Nominal(id, args) = interface else {
                    self.error(
                        "ZEN-IMPL-0001",
                        location,
                        "implementation requires an interface",
                    );
                    continue;
                };
                let Definition::Interface(required) = self.nominals[id.0].kind.clone() else {
                    self.error(
                        "ZEN-IMPL-0001",
                        location,
                        "implementation requires an interface",
                    );
                    continue;
                };
                let sub = self.substitution(*id, args);
                for req in &required {
                    if let Some(actual) = imp.methods.iter().find(|m| m.name.text == req.name.text)
                    {
                        let params = req
                            .params
                            .iter()
                            .map(|(n, t, _)| {
                                if n.text == "self" {
                                    imp.target.clone()
                                } else {
                                    t.substitute(&sub)
                                }
                            })
                            .collect::<Vec<_>>();
                        let actual_params = actual
                            .params
                            .iter()
                            .map(|(_, t, _)| t.clone())
                            .collect::<Vec<_>>();
                        if !actual.generics.is_empty()
                            || params != actual_params
                            || req.result.substitute(&sub) != actual.result
                            || req.asynchronous != actual.asynchronous
                            || req
                                .params
                                .iter()
                                .map(|(n, _, _)| &n.text)
                                .ne(actual.params.iter().map(|(n, _, _)| &n.text))
                        {
                            self.error(
                                "ZEN-IMPL-0005",
                                actual.name.span,
                                "interface method signature does not match exactly",
                            );
                        }
                    } else {
                        self.error(
                            "ZEN-IMPL-0006",
                            location,
                            format!("missing interface method `{}`", req.name.text),
                        );
                    }
                }
                for f in &imp.methods {
                    if !required.iter().any(|r| r.name.text == f.name.text) {
                        self.error(
                            "ZEN-IMPL-0007",
                            f.name.span,
                            "unexpected method in interface implementation",
                        );
                    }
                }
            } else {
                for f in imp.methods {
                    let key = format!("{:?}:{}", imp.target, f.name.text);
                    if let Some(old) = inherent.insert(key, f.name.span) {
                        self.duplicate(&f.name, old);
                    }
                }
            }
        }
    }
    pub(super) fn public_type(&mut self, t: &Type, span: Span) {
        match t {
            Type::Nominal(id, a) => {
                if !self.nominals[id.0].public {
                    self.error("ZEN-NAME-0006", span, "public API exposes a private type");
                }
                for t in a {
                    self.public_type(t, span);
                }
            }
            Type::Function(a, r) => {
                for t in a {
                    self.public_type(t, span);
                }
                self.public_type(r, span);
            }
            _ => {}
        }
    }
    pub(super) fn check_constraints(&mut self, t: &Type, span: Span) {
        match t {
            Type::Nominal(id, args) => {
                let generics = self.nominals[id.0].generics.clone();
                let sub = self.substitution(*id, args);
                for (g, a) in generics.iter().zip(args) {
                    for bound in &g.bounds {
                        let b = bound.substitute(&sub);
                        if !self.conforms(a, &b) {
                            self.error(
                                "ZEN-TYPE-0030",
                                span,
                                format!(
                                    "{} does not explicitly implement {}",
                                    self.show(a),
                                    self.show(&b)
                                ),
                            );
                        }
                    }
                    self.check_constraints(a, span);
                }
            }
            Type::Function(a, r) => {
                for t in a {
                    self.check_constraints(t, span);
                }
                self.check_constraints(r, span);
            }
            _ => {}
        }
    }
    pub(super) fn conforms(&self, t: &Type, b: &Type) -> bool {
        if *t == Type::Error {
            return true;
        }
        if let Type::Param(p) = t {
            return self
                .ctx
                .generics
                .iter()
                .find(|g| g.id == *p)
                .is_some_and(|g| g.bounds.contains(b));
        }
        self.implementations
            .iter()
            .any(|i| i.target == *t && i.interface.as_ref() == Some(b))
    }
}
