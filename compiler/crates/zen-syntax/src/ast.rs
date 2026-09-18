use zen_diagnostics::Span;
#[derive(Clone, Debug)]
pub struct Name {
    pub text: String,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct TypeRef {
    pub kind: TypeKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum TypeKind {
    Named(String, Vec<TypeRef>),
    Function(Vec<TypeRef>, Box<TypeRef>),
}
#[derive(Clone, Debug)]
pub struct Generic {
    pub name: Name,
    pub bounds: Vec<TypeRef>,
}
#[derive(Clone, Debug)]
pub struct Param {
    pub name: Name,
    pub ty: Option<TypeRef>,
    pub default: Option<Expr>,
}
#[derive(Clone, Debug)]
pub struct Function {
    pub name: Name,
    pub generics: Vec<Generic>,
    pub params: Vec<Param>,
    pub result: TypeRef,
    pub body: Option<Expr>,
    pub asynchronous: bool,
    pub native: bool,
    pub public: bool,
}
#[derive(Clone, Debug)]
pub struct Field {
    pub name: Name,
    pub ty: TypeRef,
    pub public: bool,
}
#[derive(Clone, Debug)]
pub struct Variant {
    pub name: Name,
    pub payload: Vec<TypeRef>,
}
#[derive(Clone, Debug)]
pub struct Decl {
    pub kind: DeclKind,
    pub span: Span,
    pub public: bool,
}
#[derive(Clone, Debug)]
pub enum DeclKind {
    Struct(Name, Vec<Generic>, Vec<Field>),
    Enum(Name, Vec<Generic>, Vec<Variant>),
    Interface(Name, Vec<Generic>, Vec<Function>),
    Impl {
        interface: Option<TypeRef>,
        target: TypeRef,
        methods: Vec<Function>,
    },
    Function(Function),
    Const(Name, TypeRef, Expr),
}
#[derive(Clone, Debug)]
pub struct Import {
    pub path: Vec<Name>,
    pub alias: Option<Name>,
    pub span: Span,
}
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub imports: Vec<Import>,
    pub declarations: Vec<Decl>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub usize);
#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub span: Span,
    pub kind: ExprKind,
}
#[derive(Clone, Debug)]
pub struct Arg {
    pub label: Option<Name>,
    pub value: Expr,
}
#[derive(Clone, Debug)]
pub enum Literal {
    Integer(String),
    Float(String),
    String(String),
    Char(char),
    Bool(bool),
    Unit,
}
#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    Name(Name, Vec<TypeRef>),
    Contextual(Name),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Arg>),
    Member(Box<Expr>, Name, Vec<TypeRef>),
    Struct(TypeRef, Vec<(Name, Expr)>),
    Update(Box<Expr>, Vec<(Name, Expr)>),
    List(Vec<Expr>),
    Block(Vec<Stmt>, Option<Box<Expr>>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    Match(Box<Expr>, Vec<(Pattern, Expr)>),
    Lambda {
        params: Vec<Param>,
        result: Option<TypeRef>,
        body: Box<Expr>,
        asynchronous: bool,
    },
}
#[derive(Clone, Debug)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}
#[derive(Clone, Debug)]
pub enum StmtKind {
    Bind {
        name: Name,
        mutable: bool,
        ty: Option<TypeRef>,
        value: Expr,
    },
    Assign(Expr, Expr),
    Return(Option<Expr>),
    Defer(Expr),
    Break,
    Continue,
    While(Expr, Expr),
    For(Name, Expr, Expr),
    Expr(Expr),
}
#[derive(Clone, Debug)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatternKind,
}
#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Bind(Name),
    Literal(Literal),
    Variant(Option<String>, Name, Vec<Pattern>),
}
