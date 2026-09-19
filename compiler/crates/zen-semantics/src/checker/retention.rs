use super::*;

impl Checker {
    fn resolved_generics(g: &[GenericParam]) -> Vec<resolved::GenericParameter> {
        g.iter()
            .map(|g| resolved::GenericParameter {
                id: g.id,
                name: g.name.clone(),
                bounds: g.bounds.clone(),
            })
            .collect()
    }
    fn requirement(&self, interface: &Type, name: &str) -> Option<SymbolId> {
        if let Type::Nominal(id, _) = interface
            && let Definition::Interface(methods) = &self.nominals[id.0].kind
        {
            return methods
                .iter()
                .find(|m| m.name.text == name)
                .map(|m| m.symbol);
        }
        None
    }
    pub(super) fn call_target(
        &self,
        f: &Signature,
        receiver: Option<&Type>,
    ) -> resolved::CallTarget {
        use resolved::CallTarget as T;
        if let Some(intrinsic) = f.intrinsic {
            return T::Intrinsic(intrinsic);
        }
        for implementation in &self.implementations {
            if implementation.methods.iter().any(|m| m.symbol == f.symbol) {
                return if let Some(interface) = &implementation.interface {
                    T::Implementation {
                        method: f.symbol,
                        interface: interface.clone(),
                        requirement: self
                            .requirement(interface, &f.name.text)
                            .unwrap_or(f.symbol),
                    }
                } else {
                    T::Inherent(f.symbol)
                };
            }
        }
        if let Some(receiver @ Type::Param(p)) = receiver
            && let Some(g) = self.ctx.generics.iter().find(|g| g.id == *p)
        {
            for interface in &g.bounds {
                if self.requirement(interface, &f.name.text) == Some(f.symbol) {
                    return T::Requirement {
                        method: f.symbol,
                        interface: interface.clone(),
                        receiver: receiver.clone(),
                    };
                }
            }
        }
        if f.native {
            T::Native(f.symbol)
        } else {
            T::Function(f.symbol)
        }
    }
    pub(super) fn finish_resolved(&mut self) {
        for (i, n) in self.nominals.iter().enumerate() {
            self.resolved
                .nominal_generics
                .insert(TypeId(i), Self::resolved_generics(&n.generics));
        }
        let functions = self
            .functions
            .values()
            .chain(self.implementations.iter().flat_map(|i| &i.methods))
            .chain(self.nominals.iter().flat_map(|n| match &n.kind {
                Definition::Interface(m) => m.as_slice(),
                _ => &[],
            }));
        self.resolved.functions = functions
            .map(|f| resolved::Function {
                id: f.symbol,
                module: self.modules[f.module].file,
                name: f.name.text.clone(),
                span: f
                    .body
                    .as_ref()
                    .map_or(f.name.span, |b| f.name.span.join(b.span)),
                public: f.public,
                generics: Self::resolved_generics(&f.generics),
                parameters: f
                    .params
                    .iter()
                    .filter_map(|(n, t, d)| {
                        self.resolved
                            .bindings
                            .get(&key(n.span))
                            .map(|id| resolved::Parameter {
                                local: *id,
                                ty: t.clone(),
                                default: d.clone(),
                            })
                    })
                    .collect(),
                completion: f.result.clone(),
                asynchronous: f.asynchronous,
                native: f.native,
                body: f.body.clone(),
            })
            .collect();
        self.resolved.functions.sort_by_key(|f| f.id);
        self.resolved.implementations = self
            .implementations
            .iter()
            .map(|i| resolved::Implementation {
                span: i.span,
                module: self.modules[i.module].file,
                target: i.target.clone(),
                interface: i.interface.clone(),
                methods: i
                    .methods
                    .iter()
                    .map(|m| {
                        (
                            m.symbol,
                            i.interface
                                .as_ref()
                                .and_then(|t| self.requirement(t, &m.name.text)),
                        )
                    })
                    .collect(),
            })
            .collect();
    }
}
