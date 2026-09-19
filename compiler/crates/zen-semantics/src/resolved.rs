//! Checker decisions retained for downstream lowering. Source keys identify facts,
//! never names to resolve again. AST bodies are discarded by HIR lowering.
use crate::types::*;
use std::collections::BTreeMap;
use zen_diagnostics::{FileId, Span};
use zen_syntax::ast::Expr;

pub type SourceKey = (FileId, usize, usize);
pub fn key(span: Span) -> SourceKey {
    (span.file, span.start, span.end)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldId {
    pub owner: TypeId,
    pub index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariantId {
    pub owner: TypeId,
    pub index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    ListGet,
    ListFirst,
    ListAppend,
    MapGet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    Function(SymbolId),
    Native(SymbolId),
    Inherent(SymbolId),
    Implementation {
        method: SymbolId,
        interface: Type,
        requirement: SymbolId,
    },
    Requirement {
        method: SymbolId,
        interface: Type,
        receiver: Type,
    },
    Intrinsic(Intrinsic),
}
#[derive(Clone, Debug)]
pub struct GenericParameter {
    pub id: ParamId,
    pub name: String,
    pub bounds: Vec<Type>,
}
#[derive(Clone, Debug)]
pub struct Local {
    pub id: SymbolId,
    pub name: String,
    pub span: Span,
    pub ty: Type,
    pub mutable: bool,
}
#[derive(Clone, Debug)]
pub struct Parameter {
    pub local: SymbolId,
    pub ty: Type,
    pub default: Option<Expr>,
}
#[derive(Clone, Debug)]
pub struct Function {
    pub id: SymbolId,
    pub module: FileId,
    pub name: String,
    pub span: Span,
    pub public: bool,
    pub generics: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub completion: Type,
    pub asynchronous: bool,
    pub native: bool,
    pub body: Option<Expr>,
}
#[derive(Clone, Debug)]
pub struct Implementation {
    pub span: Span,
    pub module: FileId,
    pub target: Type,
    pub interface: Option<Type>,
    /// (implementation method, corresponding requirement if any).
    pub methods: Vec<(SymbolId, Option<SymbolId>)>,
}
#[derive(Clone, Debug)]
pub struct Callable {
    pub target: CallTarget,
    pub type_arguments: Vec<(ParamId, Type)>,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub callable: Callable,
    /// Parameter indices include self, and are in source argument order.
    pub arguments: Vec<usize>,
    pub defaults: Vec<usize>,
    pub bound: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ResolvedProgram {
    pub functions: Vec<Function>,
    pub implementations: Vec<Implementation>,
    pub nominal_generics: BTreeMap<TypeId, Vec<GenericParameter>>,
    pub locals: BTreeMap<SymbolId, Local>,
    pub bindings: BTreeMap<SourceKey, SymbolId>,
    pub fields: BTreeMap<SourceKey, FieldId>,
    pub variants: BTreeMap<SourceKey, VariantId>,
    pub patterns: BTreeMap<SourceKey, Type>,
    pub callables: BTreeMap<SourceKey, Callable>,
    pub calls: BTreeMap<SourceKey, Call>,
}
