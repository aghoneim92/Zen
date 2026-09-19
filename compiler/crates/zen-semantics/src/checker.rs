use crate::analysis::*;
use crate::resolved::{self, key};
use crate::types::*;
use std::collections::{BTreeMap, BTreeSet};
use zen_diagnostics::{Diagnostic, FileId, Span, SuggestedFix};
use zen_syntax::ast::*;
#[derive(Clone, Debug)]
pub struct ModuleInput {
    pub name: String,
    pub file: FileId,
    pub syntax: Module,
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub span: Span,
    pub ty: Type,
}
#[derive(Clone, Debug)]
pub struct TypedExpr {
    pub id: ExprId,
    pub span: Span,
    pub ty: Type,
    pub symbol: Option<SymbolId>,
}
#[derive(Debug)]
pub struct CheckedProgram {
    pub resolved: resolved::ResolvedProgram,
    pub modules: Vec<ModuleInput>,
    pub nominal_types: Vec<NominalDefinition>,
    pub expressions: Vec<TypedExpr>,
    pub symbols: Vec<Symbol>,
    pub diagnostics: Vec<Diagnostic>,
    pub ide: SemanticIndex,
}
#[derive(Clone)]
struct GenericParam {
    id: ParamId,
    name: String,
    bounds: Vec<Type>,
}
#[derive(Clone)]
struct FieldDef {
    name: Name,
    ty: Type,
    public: bool,
}
#[derive(Clone)]
struct Signature {
    symbol: SymbolId,
    name: Name,
    module: usize,
    public: bool,
    generics: Vec<GenericParam>,
    params: Vec<(Name, Type, Option<Expr>)>,
    result: Type,
    asynchronous: bool,
    body: Option<Expr>,
    native: bool,
    intrinsic: Option<resolved::Intrinsic>,
}
impl Signature {
    fn ty(&self) -> Type {
        Type::Function(
            self.params.iter().map(|(_, t, _)| t.clone()).collect(),
            Box::new(if self.asynchronous {
                task(self.result.clone())
            } else {
                self.result.clone()
            }),
        )
    }
}
#[derive(Clone)]
enum Definition {
    Struct(Vec<FieldDef>),
    Enum(Vec<(Name, Vec<Type>)>),
    Interface(Vec<Signature>),
    Builtin,
}
#[derive(Clone)]
struct Nominal {
    name: Name,
    module: usize,
    public: bool,
    generics: Vec<GenericParam>,
    kind: Definition,
}
#[derive(Clone)]
struct Implementation {
    span: Span,
    module: usize,
    interface: Option<Type>,
    target: Type,
    methods: Vec<Signature>,
}
#[derive(Default)]
struct Names {
    types: BTreeMap<String, TypeId>,
    values: BTreeMap<String, SymbolId>,
    import_types: BTreeMap<String, TypeId>,
    import_values: BTreeMap<String, SymbolId>,
}
#[derive(Clone)]
struct Local {
    ty: Type,
    mutable: bool,
    span: Span,
    symbol: SymbolId,
    closure: usize,
}
#[derive(Clone, Default)]
struct Context {
    module: usize,
    generics: Vec<GenericParam>,
    locals: Vec<BTreeMap<String, Local>>,
    result: Option<Type>,
    asynchronous: bool,
    loops: usize,
    closure: usize,
}
struct Checker {
    resolved: resolved::ResolvedProgram,
    ide: SemanticIndex,
    modules: Vec<ModuleInput>,
    names: Vec<Names>,
    nominals: Vec<Nominal>,
    functions: BTreeMap<SymbolId, Signature>,
    constants: BTreeMap<SymbolId, (usize, Type, Expr, bool)>,
    implementations: Vec<Implementation>,
    symbols: Vec<Symbol>,
    expressions: Vec<TypedExpr>,
    expression_index: BTreeMap<(FileId, usize), usize>,
    diagnostics: Vec<Diagnostic>,
    next_param: usize,
    expression_depth: usize,
    ctx: Context,
    decl_ids: BTreeMap<(usize, usize), usize>,
}
pub fn check(modules: Vec<ModuleInput>) -> CheckedProgram {
    let mut c = Checker {
        resolved: resolved::ResolvedProgram::default(),
        ide: SemanticIndex::default(),
        names: (0..modules.len()).map(|_| Names::default()).collect(),
        modules,
        nominals: vec![],
        functions: BTreeMap::new(),
        constants: BTreeMap::new(),
        implementations: vec![],
        symbols: vec![],
        expressions: vec![],
        expression_index: BTreeMap::new(),
        diagnostics: vec![],
        next_param: 0,
        expression_depth: 0,
        ctx: Context::default(),
        decl_ids: BTreeMap::new(),
    };
    c.prelude();
    c.collect();
    c.imports();
    c.signatures();
    c.validate_impls();
    c.bodies();
    let nominal_types = c
        .nominals
        .iter()
        .enumerate()
        .map(|(index, n)| NominalDefinition {
            id: TypeId(index),
            name: n.name.text.clone(),
            span: n.name.span,
            module: c.modules.get(n.module).map(|m| m.file),
            public: n.public,
            parameters: n.generics.iter().map(|g| g.id).collect(),
            kind: match &n.kind {
                Definition::Struct(fields) => NominalKind::Struct(
                    fields
                        .iter()
                        .map(|f| SemanticField {
                            name: f.name.text.clone(),
                            span: f.name.span,
                            ty: f.ty.clone(),
                            public: f.public,
                        })
                        .collect(),
                ),
                Definition::Enum(variants) => NominalKind::Enum(
                    variants
                        .iter()
                        .map(|(n, p)| SemanticVariant {
                            name: n.text.clone(),
                            span: n.span,
                            payload: p.clone(),
                        })
                        .collect(),
                ),
                Definition::Interface(methods) => {
                    NominalKind::Interface(methods.iter().map(|m| m.symbol).collect())
                }
                Definition::Builtin => NominalKind::Builtin,
            },
        })
        .collect();
    c.finish_resolved();
    c.finish_index();
    CheckedProgram {
        resolved: c.resolved,
        ide: c.ide,
        modules: c.modules,
        nominal_types,
        expressions: c.expressions,
        symbols: c.symbols,
        diagnostics: c.diagnostics,
    }
}

mod bodies;
mod calls;
mod declarations;
mod expressions;
mod patterns;

fn fields_for(d: &Decl) -> &[Field] {
    if let DeclKind::Struct(_, _, f) = &d.kind {
        f
    } else {
        &[]
    }
}

mod ide;

mod retention;
