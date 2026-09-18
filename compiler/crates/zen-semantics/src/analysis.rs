//! Compiler-owned, immutable IDE queries. Entity IDs belong to one analysis snapshot;
//! source file IDs belong to the caller's database and survive text changes.
use crate::{CheckedProgram, types::*};
use std::collections::BTreeMap;
use zen_diagnostics::{FileId, Span};
use zen_syntax::ast::Name;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Entity {
    Value(SymbolId),
    Builtin(String),
    Nominal(TypeId),
    Member(TypeId, String),
    Generic(ParamId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Variable,
    Parameter,
    Function,
    Method,
    Constant,
    Struct,
    Enum,
    Interface,
    Field,
    Variant,
    TypeParameter,
}
#[derive(Clone, Debug)]
pub struct Declaration {
    pub entity: Entity,
    pub name: String,
    pub selection: Span,
    pub range: Span,
    pub kind: Kind,
    pub ty: Type,
    pub parent: Option<Entity>,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub name: String,
    pub entity: Entity,
    pub ty: Type,
}
#[derive(Clone, Debug)]
pub struct CallSite {
    pub span: Span,
    pub callee: Span,
    pub symbol: SymbolId,
    pub parameters: Vec<(String, Type)>,
    pub arguments: Vec<(Span, Option<String>)>,
}
#[derive(Clone, Debug, Default)]
pub struct SemanticIndex {
    pub declarations: Vec<Declaration>,
    pub namespaces: Vec<Span>,
    pub builtin_types: Vec<Span>,
    pub type_contexts: Vec<Span>,
    pub writes: Vec<Span>,
    pub references: Vec<(Span, Entity)>,
    pub scopes: Vec<(Span, Vec<Candidate>)>,
    pub expected: Vec<(Span, Type)>,
    pub inferred: Vec<(Span, Type)>,
    pub calls: Vec<CallSite>,
    pub matches: Vec<(Span, Type, Vec<String>)>,
    pub implementations: Vec<(Entity, Span)>,
    pub pending_references: Vec<(Span, Span)>,
    pub generics: BTreeMap<ParamId, Name>,
    pub member_contexts: Vec<(Type, FileId)>,
    pub members: Vec<(Type, FileId, bool, Candidate)>,
}
impl SemanticIndex {
    pub(crate) fn reference(&mut self, span: Span, entity: Entity) {
        if !self.references.iter().any(|x| x.0 == span && x.1 == entity) {
            self.references.push((span, entity));
        }
    }
}
pub fn contains(span: Span, file: FileId, offset: usize) -> bool {
    span.file == file && span.start <= offset && offset <= span.end
}
impl CheckedProgram {
    pub fn symbol_at(&self, file: FileId, offset: usize) -> Option<Entity> {
        self.ide
            .references
            .iter()
            .filter(|(s, _)| contains(*s, file, offset))
            .min_by_key(|(s, _)| s.end - s.start)
            .map(|(_, e)| e.clone())
            .or_else(|| {
                self.ide
                    .declarations
                    .iter()
                    .find(|d| contains(d.selection, file, offset))
                    .map(|d| d.entity.clone())
            })
    }
    pub fn declaration(&self, e: &Entity) -> Option<&Declaration> {
        self.ide.declarations.iter().find(|d| &d.entity == e)
    }
    pub fn references(&self, e: &Entity, include_declaration: bool) -> Vec<Span> {
        let mut spans: Vec<_> = self
            .ide
            .references
            .iter()
            .filter(|(_, x)| x == e)
            .map(|(s, _)| *s)
            .collect();
        if include_declaration && let Some(d) = self.declaration(e) {
            spans.push(d.selection);
        }
        spans.sort_by_key(|s| (s.file, s.start, s.end));
        spans.dedup();
        spans
    }
    pub fn type_at(&self, file: FileId, offset: usize) -> Option<&Type> {
        self.expressions
            .iter()
            .filter(|e| contains(e.span, file, offset))
            .min_by_key(|e| e.span.end - e.span.start)
            .map(|e| &e.ty)
            .or_else(|| {
                self.ide
                    .declarations
                    .iter()
                    .find(|d| contains(d.selection, file, offset))
                    .map(|d| &d.ty)
            })
    }
    pub fn visible_symbols(&self, file: FileId, offset: usize) -> Vec<Candidate> {
        self.ide
            .scopes
            .iter()
            .filter(|(s, _)| contains(*s, file, offset))
            .min_by_key(|(s, _)| s.end - s.start)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }
    pub fn members(&self, ty: &Type, file: FileId) -> Vec<Candidate> {
        let mut result = vec![];
        if let Type::Nominal(id, args) = ty {
            let n = &self.nominal_types[id.0];
            let sub = n
                .parameters
                .iter()
                .copied()
                .zip(args.iter().cloned())
                .collect();
            match &n.kind {
                NominalKind::Struct(fields) => {
                    for f in fields.iter().filter(|f| f.public || n.module == Some(file)) {
                        result.push(Candidate {
                            name: f.name.clone(),
                            entity: Entity::Member(*id, f.name.clone()),
                            ty: f.ty.substitute(&sub),
                        });
                    }
                }
                NominalKind::Enum(variants) => {
                    for v in variants {
                        result.push(Candidate {
                            name: v.name.clone(),
                            entity: Entity::Member(*id, v.name.clone()),
                            ty: Type::Function(
                                v.payload.iter().map(|t| t.substitute(&sub)).collect(),
                                Box::new(ty.clone()),
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
        result.extend(
            self.ide
                .members
                .iter()
                .filter(|(t, m, p, _)| t == ty && (*p || *m == file))
                .map(|(_, _, _, c)| c.clone()),
        );
        result
    }
    pub fn display_type(&self, t: &Type) -> String {
        match t {
            Type::Nominal(id, args) => {
                let Some(nominal) = self.nominal_types.get(id.0) else {
                    return "<unknown>".into();
                };
                let name = &nominal.name;
                if args.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{}<{}>",
                        name,
                        args.iter()
                            .map(|a| self.display_type(a))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Type::Param(id) => self
                .ide
                .generics
                .get(id)
                .map(|n| n.text.clone())
                .unwrap_or_else(|| format!("T{}", id.0)),
            Type::Function(args, r) => format!(
                "({}) -> {}",
                args.iter()
                    .map(|a| self.display_type(a))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.display_type(r)
            ),
            Type::Error => "<unknown>".into(),
            _ => format!("{t:?}"),
        }
    }
}

/// An in-memory source database. Removed slots are not recycled.
#[derive(Clone, Debug, Default)]
pub struct Database {
    pub files: Vec<Option<InputFile>>,
}
#[derive(Clone, Debug)]
pub struct InputFile {
    pub path: String,
    pub module: String,
    pub text: String,
}
impl Database {
    pub fn set_file(&mut self, path: String, module: String, text: String) -> FileId {
        if let Some(i) = self
            .files
            .iter()
            .position(|f| f.as_ref().is_some_and(|f| f.path == path))
        {
            self.files[i] = Some(InputFile { path, module, text });
            return FileId(i);
        }
        let id = FileId(self.files.len());
        self.files.push(Some(InputFile { path, module, text }));
        id
    }
    pub fn analyze(&self) -> CheckedProgram {
        let mut errors = vec![];
        let modules = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                f.as_ref().map(|f| {
                    let (syntax, diagnostics) = zen_syntax::parse(FileId(i), &f.text);
                    errors.extend(diagnostics);
                    crate::ModuleInput {
                        name: f.module.clone(),
                        file: FileId(i),
                        syntax,
                    }
                })
            })
            .collect();
        let mut program = crate::check(modules);
        program.diagnostics.splice(0..0, errors);
        for module in &program.modules {
            let Some(source) = self.files[module.file.0].as_ref() else {
                continue;
            };
            for decl in &module.syntax.declarations {
                use zen_syntax::ast::DeclKind;
                let functions = match &decl.kind {
                    DeclKind::Function(f) => std::slice::from_ref(f),
                    DeclKind::Interface(_, _, fs) | DeclKind::Impl { methods: fs, .. } => {
                        fs.as_slice()
                    }
                    _ => &[],
                };
                for f in functions {
                    if let Some(d) = program
                        .ide
                        .declarations
                        .iter_mut()
                        .find(|d| d.selection == f.name.span)
                        && let Some(header) = source.text.get(f.name.span.start..f.result.span.end)
                    {
                        d.detail = format!(
                            "{}{}fn {}",
                            if f.native { "native " } else { "" },
                            if f.asynchronous { "async " } else { "" },
                            header
                        );
                    }
                }
            }
        }

        program
    }
    /// Validate edits by rechecking a complete speculative compiler snapshot.
    pub fn rename(
        &self,
        program: &CheckedProgram,
        entity: &Entity,
        name: &str,
    ) -> Result<Vec<Span>, String> {
        let (tokens, errors) = zen_syntax::lexer::lex(FileId(0), name);
        if !errors.is_empty()
            || tokens.len() != 2
            || tokens[0].kind != zen_syntax::lexer::TokenKind::Ident
            || tokens[0].text != name
        {
            return Err("The new name must be a non-reserved Zen identifier".into());
        }
        let decl = program
            .declaration(entity)
            .ok_or("This symbol has no editable declaration")?;
        if decl.name == "self" {
            return Err("The self receiver cannot be renamed".into());
        }
        if !program.diagnostics.is_empty() {
            return Err(
                "Resolve compiler errors before renaming so the result can be verified".into(),
            );
        }
        let mut family = vec![entity.clone()];
        // Interface declarations and their explicit implementations share a rename.
        for (interface, implementation) in &program.ide.implementations {
            let Some(method) = program
                .ide
                .declarations
                .iter()
                .find(|d| d.selection == *implementation && d.kind == Kind::Method)
            else {
                continue;
            };
            if interface == entity || method.entity == *entity {
                family.push(interface.clone());
            }
        }
        for (interface, implementation) in &program.ide.implementations {
            if family.contains(interface)
                && let Some(method) = program
                    .ide
                    .declarations
                    .iter()
                    .find(|d| d.selection == *implementation && d.kind == Kind::Method)
            {
                family.push(method.entity.clone());
            }
        }
        let mut spans: Vec<_> = family
            .iter()
            .flat_map(|e| program.references(e, true))
            .collect();
        spans.sort_by_key(|s| (s.file, s.start, s.end));
        spans.dedup();
        let mut next = self.clone();
        for span in spans.iter().rev() {
            let f = next.files[span.file.0]
                .as_mut()
                .ok_or("Source disappeared")?;
            if f.text.get(span.start..span.end) != Some(decl.name.as_str()) {
                return Err("Renaming aliased imports is not yet supported".into());
            }
            f.text.replace_range(span.start..span.end, name);
        }
        let checked = next.analyze();
        if !checked.diagnostics.is_empty() {
            return Err("Rename would introduce a compiler error (possibly a duplicate or shadowed declaration)".into());
        }
        // A type-correct edit can still capture a name. Verify that every old
        // resolved reference keeps the same declaration after offset translation.
        let translated = |span: Span| {
            let delta: isize = spans
                .iter()
                .filter(|s| s.file == span.file && s.end <= span.start)
                .map(|s| name.len() as isize - (s.end - s.start) as isize)
                .sum();
            Span::new(
                span.file,
                span.start.saturating_add_signed(delta),
                span.end.saturating_add_signed(delta),
            )
        };
        for (usage, target) in &program.ide.references {
            let Some(def) = program.declaration(target) else {
                continue;
            };
            let at = translated(*usage);
            let expected = translated(def.selection);
            let actual = checked
                .symbol_at(at.file, at.start)
                .and_then(|e| checked.declaration(&e));
            if actual.is_none_or(|d| {
                d.selection.file != expected.file || d.selection.start != expected.start
            }) {
                return Err("Rename would change name resolution or capture another symbol".into());
            }
        }
        Ok(spans)
    }
}

impl Database {
    /// One speculative parse/check per completion request, never per candidate.
    /// The placeholder supplies syntax only; all types and lookup come from the checker.
    pub fn complete(&self, file: FileId, offset: usize) -> Vec<Candidate> {
        let Some(input) = self.files.get(file.0).and_then(Option::as_ref) else {
            return vec![];
        };
        if !input.text.is_char_boundary(offset) {
            return vec![];
        }
        let start = input.text[..offset]
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map_or(0, |i| i + 1);
        let prefix = &input.text[start..offset];
        let before = input.text[..start].trim_end();
        let dot = before.ends_with('.');
        let mut probe = self.clone();
        let text = &mut probe.files[file.0].as_mut().unwrap().text;
        text.replace_range(start..offset, "__zen_cursor");
        let program = probe.analyze();
        let mut candidates = if dot {
            let dot_offset = before.len() - 1;
            let receiver = program
                .expressions
                .iter()
                .filter(|e| {
                    e.span.file == file
                        && e.span.end <= dot_offset
                        && probe.files[file.0].as_ref().unwrap().text[e.span.end..dot_offset]
                            .trim()
                            .is_empty()
                })
                .max_by_key(|e| (e.span.end, usize::MAX - e.span.start));
            if let Some(e) = receiver {
                program.members(&e.ty, file)
            } else if let Some((_, ty)) = program
                .ide
                .expected
                .iter()
                // Only a contextual constructor beginning at this dot may use
                // its expected type. An enclosing block's return type says
                // nothing about an unresolved or incomplete member receiver.
                .filter(|(s, _)| s.start == dot_offset && contains(*s, file, start))
                .min_by_key(|(s, _)| s.end - s.start)
            {
                program.members(ty, file)
            } else {
                vec![]
            }
        } else {
            program.visible_symbols(file, start)
        };
        // A partially entered match arm may have been dropped by parser recovery.
        // Complete its syntax, then ask the checker for the scrutinee type.
        if dot && candidates.is_empty() {
            let text = &mut probe.files[file.0].as_mut().unwrap().text;
            let after = start + "__zen_cursor".len();
            text.insert_str(after, " => unit;");
            let matched = probe.analyze();
            if let Some((_, ty, handled)) = matched
                .ide
                .matches
                .iter()
                .filter(|(s, _, _)| contains(*s, file, start))
                .min_by_key(|(s, _, _)| s.end - s.start)
            {
                candidates = matched.members(ty, file);
                candidates.sort_by_key(|c| handled.contains(&c.name));
            }
        }
        if !dot
            && let Some(call) = program
                .ide
                .calls
                .iter()
                .filter(|c| contains(c.span, file, start) && start > c.callee.end)
                .min_by_key(|c| c.span.end - c.span.start)
        {
            for (name, ty) in &call.parameters {
                if !call
                    .arguments
                    .iter()
                    .any(|(_, label)| label.as_ref() == Some(name))
                {
                    candidates.insert(
                        0,
                        Candidate {
                            name: format!("{name}: "),
                            entity: Entity::Builtin(name.clone()),
                            ty: ty.clone(),
                        },
                    );
                }
            }
        }
        if program
            .ide
            .type_contexts
            .iter()
            .any(|sp| contains(*sp, file, start))
        {
            candidates.retain(|c| matches!(c.entity, Entity::Nominal(_) | Entity::Generic(_)));
            for (name, ty) in [
                ("Int", Type::Int),
                ("Float", Type::Float),
                ("String", Type::String),
                ("Bool", Type::Bool),
                ("Char", Type::Char),
                ("Unit", Type::Unit),
                ("Never", Type::Never),
                ("I8", Type::I8),
                ("I16", Type::I16),
                ("I32", Type::I32),
                ("I64", Type::I64),
                ("U8", Type::U8),
                ("U16", Type::U16),
                ("U32", Type::U32),
                ("U64", Type::U64),
                ("F32", Type::F32),
                ("F64", Type::Float),
            ] {
                candidates.push(Candidate {
                    name: name.into(),
                    entity: Entity::Builtin(name.into()),
                    ty,
                });
            }
        }
        candidates.retain(|c| c.name.starts_with(prefix) && c.name != "__zen_cursor");
        candidates
    }
}

/// Until a package manifest is specified, all tools share the CLI's source-root rule.
pub fn source_root(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Path;
    path.ancestors()
        .skip(1)
        .find(|p| p.file_name().is_some_and(|n| n == "src"))
        .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")))
        .to_path_buf()
}
