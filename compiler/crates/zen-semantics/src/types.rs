//! Semantic identity and exact assignment compatibility, independent of parser types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamId(pub usize);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Bool,
    Int,
    Float,
    String,
    Char,
    Unit,
    Never,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    Nominal(TypeId, Vec<Type>),
    Param(ParamId),
    Function(Vec<Type>, Box<Type>),
    /// Error recovery only. Never emitted in a successful typed program.
    Error,
}
impl Type {
    pub fn assignable_to(&self, expected: &Self) -> bool {
        self == expected || *self == Self::Never
    }
    pub fn substitute(&self, map: &std::collections::BTreeMap<ParamId, Type>) -> Self {
        match self {
            Self::Param(p) => map.get(p).cloned().unwrap_or_else(|| self.clone()),
            Self::Nominal(id, args) => {
                Self::Nominal(*id, args.iter().map(|a| a.substitute(map)).collect())
            }
            Self::Function(p, r) => Self::Function(
                p.iter().map(|a| a.substitute(map)).collect(),
                Box::new(r.substitute(map)),
            ),
            _ => self.clone(),
        }
    }
    pub fn contains_param(&self, ids: &[ParamId]) -> bool {
        match self {
            Self::Param(p) => ids.contains(p),
            Self::Nominal(_, a) => a.iter().any(|t| t.contains_param(ids)),
            Self::Function(a, r) => {
                a.iter().any(|t| t.contains_param(ids)) || r.contains_param(ids)
            }
            _ => false,
        }
    }
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Int
                | Self::Float
                | Self::F32
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }
}
pub const OPTION: TypeId = TypeId(0);
pub const RESULT: TypeId = TypeId(1);
pub const TASK: TypeId = TypeId(2);
pub const LIST: TypeId = TypeId(3);
pub const MAP: TypeId = TypeId(4);
pub const SET: TypeId = TypeId(5);
pub fn option(t: Type) -> Type {
    Type::Nominal(OPTION, vec![t])
}
pub fn task(t: Type) -> Type {
    Type::Nominal(TASK, vec![t])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_types() {
        assert!(!Type::Int.assignable_to(&Type::Float));
        assert!(Type::Never.assignable_to(&Type::Int));
        assert!(!option(Type::Never).assignable_to(&option(Type::Int)));
    }
}

/// Nominal metadata retained with a checked program. IDs index this table.
#[derive(Clone, Debug)]
pub struct NominalDefinition {
    pub id: TypeId,
    pub name: String,
    pub span: zen_diagnostics::Span,
    /// None denotes a compiler-owned prelude declaration.
    pub module: Option<zen_diagnostics::FileId>,
    pub public: bool,
    pub parameters: Vec<ParamId>,
    pub kind: NominalKind,
}
#[derive(Clone, Debug)]
pub enum NominalKind {
    Struct(Vec<SemanticField>),
    Enum(Vec<SemanticVariant>),
    Interface(Vec<SymbolId>),
    Builtin,
}
#[derive(Clone, Debug)]
pub struct SemanticField {
    pub name: String,
    pub span: zen_diagnostics::Span,
    pub ty: Type,
    pub public: bool,
}
#[derive(Clone, Debug)]
pub struct SemanticVariant {
    pub name: String,
    pub span: zen_diagnostics::Span,
    pub payload: Vec<Type>,
}
