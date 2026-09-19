//! Owned, typed, resolved high-level IR. No runtime or target layout decisions.
mod lower;
pub use lower::{LoweringError, lower_program};
use std::collections::BTreeMap;
use zen_diagnostics::{FileId, Span};
pub use zen_semantics::resolved::{
    CallTarget, FieldId, GenericParameter, Implementation, Intrinsic, Local, VariantId,
};
pub use zen_semantics::types::{
    LIST, MAP, NominalDefinition, NominalKind, OPTION, ParamId, RESULT, SET, SymbolId, TASK, Type,
    TypeId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExprId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LambdaId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnTarget {
    Function(SymbolId),
    Lambda(LambdaId),
}
#[derive(Clone, Debug)]
pub struct Program {
    pub modules: Vec<Module>,
    pub nominal_types: Vec<NominalDefinition>,
    pub nominal_generics: BTreeMap<TypeId, Vec<GenericParameter>>,
    pub functions: Vec<Function>,
    pub constants: Vec<Constant>,
    pub implementations: Vec<Implementation>,
    pub locals: BTreeMap<SymbolId, Local>,
    /// Arena in deterministic lowering order; all references index this arena.
    pub expressions: Vec<Expr>,
}
impl Program {
    /// Deterministic development format, not a serialization/versioning contract.
    pub fn dump(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        for m in &self.modules {
            writeln!(out, "module {m:?}").unwrap();
        }
        for n in &self.nominal_types {
            writeln!(out, "nominal {n:?}").unwrap();
        }
        for (id, g) in &self.nominal_generics {
            if !g.is_empty() {
                writeln!(out, "generics {id:?} {g:?}").unwrap();
            }
        }
        for i in &self.implementations {
            writeln!(out, "impl {i:?}").unwrap();
        }
        for f in &self.functions {
            writeln!(out, "fn {f:?}").unwrap();
        }
        for c in &self.constants {
            writeln!(out, "const {c:?}").unwrap();
        }
        for l in self.locals.values() {
            writeln!(out, "local {l:?}").unwrap();
        }
        for e in &self.expressions {
            writeln!(
                out,
                "expr #{} : {:?} = {:?} @ {:?}",
                e.id.0, e.ty, e.kind, e.span
            )
            .unwrap();
        }
        out
    }
    pub fn expression(&self, id: ExprId) -> &Expr {
        &self.expressions[id.0]
    }
}
#[derive(Clone, Debug)]
pub struct Module {
    pub id: FileId,
    pub name: String,
    pub functions: Vec<SymbolId>,
    pub constants: Vec<SymbolId>,
    pub nominals: Vec<TypeId>,
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
    pub body: Body,
}
#[derive(Clone, Debug)]
pub enum Body {
    Zen(ExprId),
    Native,
    Requirement,
}
#[derive(Clone, Debug)]
pub struct Parameter {
    pub local: SymbolId,
    pub ty: Type,
    pub default: Option<ExprId>,
}
#[derive(Clone, Debug)]
pub struct Constant {
    pub id: SymbolId,
    pub module: FileId,
    pub name: String,
    pub span: Span,
    pub public: bool,
    pub ty: Type,
    pub value: ExprId,
}
#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub span: Span,
    pub ty: Type,
    pub kind: ExprKind,
}
#[derive(Clone, Debug)]
pub enum Literal {
    /// Canonical decimal magnitude; no suffix/separators or machine integer limit.
    Integer {
        negative: bool,
        magnitude: String,
    },
    Float(f64),
    Float32(f32),
    String(String),
    Char(char),
    Bool(bool),
    Unit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
#[derive(Clone, Debug)]
pub struct Callable {
    pub target: CallTarget,
    pub type_arguments: Vec<(ParamId, Type)>,
    pub receiver: Option<ExprId>,
}
#[derive(Clone, Debug)]
pub enum Callee {
    Resolved(Callable),
    Value(ExprId),
}
#[derive(Clone, Debug)]
pub struct Argument {
    pub parameter: usize,
    pub value: ExprId,
}
#[derive(Clone, Debug)]
pub enum Propagation {
    Option,
    Result { error: Type },
}
#[derive(Clone, Debug)]
pub struct Capture {
    pub local: SymbolId,
    pub ty: Type,
}
#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    Local(SymbolId),
    Constant(SymbolId),
    Callable(Callable),
    Call {
        callee: Callee,
        arguments: Vec<Argument>,
        defaults: Vec<usize>,
    },
    Field {
        receiver: ExprId,
        field: FieldId,
    },
    Struct {
        nominal: TypeId,
        fields: Vec<(FieldId, ExprId)>,
    },
    Update {
        base: ExprId,
        nominal: TypeId,
        fields: Vec<(FieldId, ExprId)>,
    },
    Variant {
        variant: VariantId,
        type_arguments: Vec<Type>,
        payload: Vec<ExprId>,
    },
    List {
        element: Type,
        items: Vec<ExprId>,
    },
    Unary {
        op: UnaryOp,
        operand_type: Type,
        operand: ExprId,
    },
    Binary {
        op: BinaryOp,
        operand_type: Type,
        left: ExprId,
        right: ExprId,
    },
    Logical {
        op: LogicalOp,
        left: ExprId,
        right: ExprId,
    },
    Block {
        scope: BlockId,
        statements: Vec<Stmt>,
        tail: Option<ExprId>,
    },
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Match {
        value: ExprId,
        arms: Vec<(Pattern, ExprId)>,
    },
    Lambda {
        id: LambdaId,
        parameters: Vec<SymbolId>,
        completion: Type,
        asynchronous: bool,
        body: ExprId,
        captures: Vec<Capture>,
    },
    Await {
        task: ExprId,
    },
    Propagate {
        kind: Propagation,
        operand: ExprId,
        target: ReturnTarget,
        completion: Type,
    },
    /// A Never operand cannot reach the operation (e.g. await a diverging call).
    Diverge {
        operand: ExprId,
    },
}
#[derive(Clone, Debug)]
pub struct Pattern {
    pub span: Span,
    pub ty: Type,
    pub kind: PatternKind,
}
#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Bind(SymbolId),
    Literal(Literal),
    Variant(VariantId, Vec<Pattern>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iteration {
    List,
    Set,
}
#[derive(Clone, Debug)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}
#[derive(Clone, Debug)]
pub enum StmtKind {
    Bind {
        local: SymbolId,
        value: ExprId,
    },
    Assign {
        local: SymbolId,
        value: ExprId,
    },
    Return {
        target: ReturnTarget,
        value: Option<ExprId>,
    },
    Defer {
        scope: BlockId,
        value: ExprId,
    },
    Break(LoopId),
    Continue(LoopId),
    While {
        id: LoopId,
        condition: ExprId,
        body: ExprId,
    },
    For {
        id: LoopId,
        binding: SymbolId,
        iterable: ExprId,
        iteration: Iteration,
        element: Type,
        body: ExprId,
    },
    Expr(ExprId),
}
