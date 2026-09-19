use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use zen_semantics::types::{LIST, OPTION, RESULT, SET};
use zen_semantics::{CheckedProgram, TypedExpr, resolved::key};
use zen_syntax::ast;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoweringError {
    InvalidProgram,
    Internal { span: Span, message: String },
}
impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProgram => write!(f, "HIR requires an error-free checked program"),
            Self::Internal { span, message } => {
                write!(f, "ICE: HIR lowering at {span:?}: {message}")
            }
        }
    }
}
impl std::error::Error for LoweringError {}
type Result<T> = std::result::Result<T, LoweringError>;
fn ice(span: Span, message: impl Into<String>) -> LoweringError {
    LoweringError::Internal {
        span,
        message: message.into(),
    }
}
fn need<T>(value: Option<T>, span: Span, message: &str) -> Result<T> {
    value.ok_or_else(|| ice(span, message))
}
fn valid_type(t: &Type) -> bool {
    match t {
        Type::Error => false,
        Type::Nominal(_, a) => a.iter().all(valid_type),
        Type::Function(a, r) => a.iter().all(valid_type) && valid_type(r),
        _ => true,
    }
}
#[derive(Default)]
struct CaptureScope {
    defined: BTreeSet<SymbolId>,
    captures: BTreeSet<SymbolId>,
}
struct Lower<'a> {
    checked: &'a CheckedProgram,
    facts: BTreeMap<(FileId, usize), &'a TypedExpr>,
    out: Program,
    captures: Vec<CaptureScope>,
    loops: Vec<LoopId>,
    scopes: Vec<BlockId>,
    target: Option<(ReturnTarget, Type)>,
    next_lambda: usize,
    next_loop: usize,
    next_block: usize,
}

/// The caller must stop on parse errors, as for `zen_semantics::check` itself.
/// Database analysis includes parse diagnostics and is also accepted here.
pub fn lower_program(checked: &CheckedProgram) -> Result<Program> {
    if !checked.diagnostics.is_empty() {
        return Err(LoweringError::InvalidProgram);
    }
    for expression in &checked.expressions {
        if !valid_type(&expression.ty) {
            return Err(ice(expression.span, "recovery type in checked expression"));
        }
    }
    for local in checked.resolved.locals.values() {
        if !valid_type(&local.ty) {
            return Err(ice(local.span, "recovery type in local"));
        }
    }
    let mut l = Lower {
        checked,
        facts: checked
            .expressions
            .iter()
            .map(|e| ((e.span.file, e.id.0), e))
            .collect(),
        out: Program {
            modules: vec![],
            nominal_types: checked.nominal_types.clone(),
            nominal_generics: checked.resolved.nominal_generics.clone(),
            functions: vec![],
            constants: vec![],
            implementations: checked.resolved.implementations.clone(),
            locals: checked.resolved.locals.clone(),
            expressions: vec![],
        },
        captures: vec![],
        loops: vec![],
        scopes: vec![],
        target: None,
        next_lambda: 0,
        next_loop: 0,
        next_block: 0,
    };
    for f in &checked.resolved.functions {
        if !valid_type(&f.completion) || f.parameters.iter().any(|p| !valid_type(&p.ty)) {
            return Err(ice(f.span, "recovery type in signature"));
        }
        l.target = Some((ReturnTarget::Function(f.id), f.completion.clone()));
        let parameters = f
            .parameters
            .iter()
            .map(|p| {
                Ok(Parameter {
                    local: p.local,
                    ty: p.ty.clone(),
                    default: p.default.as_ref().map(|e| l.expr(e)).transpose()?,
                })
            })
            .collect::<Result<_>>()?;
        let body = if let Some(body) = &f.body {
            Body::Zen(l.expr(body)?)
        } else if f.native {
            Body::Native
        } else {
            Body::Requirement
        };
        l.out.functions.push(Function {
            id: f.id,
            module: f.module,
            name: f.name.clone(),
            span: f.span,
            public: f.public,
            generics: f.generics.clone(),
            parameters,
            completion: f.completion.clone(),
            asynchronous: f.asynchronous,
            body,
        });
        l.target = None;
    }
    for m in &checked.modules {
        let mut constants = vec![];
        for d in &m.syntax.declarations {
            if let ast::DeclKind::Const(n, _, value) = &d.kind {
                let symbol = need(
                    checked.symbols.iter().find(|s| s.span == n.span),
                    n.span,
                    "constant missing declaration identity",
                )?;
                let value = l.expr(value)?;
                constants.push(symbol.id);
                l.out.constants.push(Constant {
                    id: symbol.id,
                    module: m.file,
                    name: n.text.clone(),
                    span: d.span,
                    public: d.public,
                    ty: symbol.ty.clone(),
                    value,
                });
            }
        }
        l.out.modules.push(Module {
            id: m.file,
            name: m.name.clone(),
            functions: l
                .out
                .functions
                .iter()
                .filter(|f| f.module == m.file)
                .map(|f| f.id)
                .collect(),
            constants,
            nominals: checked
                .nominal_types
                .iter()
                .filter(|n| n.module == Some(m.file))
                .map(|n| n.id)
                .collect(),
        });
    }
    Ok(l.out)
}
impl Lower<'_> {
    fn fact(&self, e: &ast::Expr) -> Result<&TypedExpr> {
        need(
            self.facts.get(&(e.span.file, e.id.0)).copied(),
            e.span,
            "expression missing checked type",
        )
    }
    fn binding(&mut self, span: Span) -> Result<SymbolId> {
        let id = need(
            self.checked.resolved.bindings.get(&key(span)).copied(),
            span,
            "binding missing local identity",
        )?;
        if let Some(scope) = self.captures.last_mut() {
            scope.defined.insert(id);
        }
        Ok(id)
    }
    fn reference(&mut self, id: SymbolId) {
        let start = self
            .captures
            .iter()
            .rposition(|scope| scope.defined.contains(&id))
            .map_or(0, |i| i + 1);
        for scope in self.captures.iter_mut().skip(start) {
            scope.captures.insert(id);
        }
    }
    fn field(&self, span: Span) -> Result<FieldId> {
        need(
            self.checked.resolved.fields.get(&key(span)).copied(),
            span,
            "field missing FieldId",
        )
    }
    fn variant(&self, span: Span) -> Result<VariantId> {
        need(
            self.checked.resolved.variants.get(&key(span)).copied(),
            span,
            "variant missing VariantId",
        )
    }
    fn nominal(&self, ty: &Type, span: Span) -> Result<(TypeId, Vec<Type>)> {
        if let Type::Nominal(id, args) = ty {
            Ok((*id, args.clone()))
        } else {
            Err(ice(span, "expected checked nominal type"))
        }
    }
    fn fields(&mut self, fields: &[(ast::Name, ast::Expr)]) -> Result<Vec<(FieldId, ExprId)>> {
        fields
            .iter()
            .map(|(n, e)| Ok((self.field(n.span)?, self.expr(e)?)))
            .collect()
    }
    fn return_target(&self, span: Span) -> Result<(ReturnTarget, Type)> {
        need(self.target.clone(), span, "missing enclosing callable")
    }
    fn expr(&mut self, e: &ast::Expr) -> Result<ExprId> {
        let ty = self.fact(e)?.ty.clone();
        let kind = match &e.kind {
            ast::ExprKind::Literal(l) => ExprKind::Literal(literal(l, &ty, false, e.span)?),
            ast::ExprKind::Name(_, _) => {
                let id = need(self.fact(e)?.symbol, e.span, "value missing SymbolId")?;
                if self.out.locals.contains_key(&id) {
                    self.reference(id);
                    ExprKind::Local(id)
                } else if let Some(f) = self.checked.resolved.functions.iter().find(|f| f.id == id)
                {
                    let callable = self.checked.resolved.callables.get(&key(e.span));
                    ExprKind::Callable(Callable {
                        target: callable.map(|c| c.target.clone()).unwrap_or(if f.native {
                            CallTarget::Native(id)
                        } else {
                            CallTarget::Function(id)
                        }),
                        type_arguments: if let Some(c) = callable {
                            c.type_arguments.clone()
                        } else if f.generics.is_empty() {
                            vec![]
                        } else {
                            return Err(ice(e.span, "generic function value missing substitution"));
                        },
                        receiver: None,
                    })
                } else {
                    ExprKind::Constant(id)
                }
            }
            ast::ExprKind::Contextual(n) => {
                let (_, type_arguments) = self.nominal(&ty, e.span)?;
                ExprKind::Variant {
                    variant: self.variant(n.span)?,
                    type_arguments,
                    payload: vec![],
                }
            }
            ast::ExprKind::Member(base, n, _) => {
                if self.checked.resolved.variants.contains_key(&key(n.span)) {
                    let (_, type_arguments) = self.nominal(&ty, e.span)?;
                    ExprKind::Variant {
                        variant: self.variant(n.span)?,
                        type_arguments,
                        payload: vec![],
                    }
                } else {
                    let receiver = self.expr(base)?;
                    if let Some(c) = self.checked.resolved.callables.get(&key(e.span)) {
                        ExprKind::Callable(Callable {
                            target: c.target.clone(),
                            type_arguments: c.type_arguments.clone(),
                            receiver: Some(receiver),
                        })
                    } else {
                        ExprKind::Field {
                            receiver,
                            field: self.field(n.span)?,
                        }
                    }
                }
            }
            ast::ExprKind::Call(c, args) => {
                let variant_name = match &c.kind {
                    ast::ExprKind::Contextual(n) | ast::ExprKind::Member(_, n, _) => Some(n),
                    _ => None,
                };
                if let Some(n) = variant_name
                    .filter(|n| self.checked.resolved.variants.contains_key(&key(n.span)))
                {
                    let (_, type_arguments) = self.nominal(&ty, e.span)?;
                    ExprKind::Variant {
                        variant: self.variant(n.span)?,
                        type_arguments,
                        payload: args
                            .iter()
                            .map(|a| self.expr(&a.value))
                            .collect::<Result<_>>()?,
                    }
                } else if let Some(call) = self.checked.resolved.calls.get(&key(e.span)) {
                    let receiver = if call.bound {
                        if let ast::ExprKind::Member(base, _, _) = &c.kind {
                            Some(self.expr(base)?)
                        } else {
                            return Err(ice(c.span, "bound call missing receiver"));
                        }
                    } else {
                        None
                    };
                    if args.len() != call.arguments.len() {
                        return Err(ice(e.span, "call argument mapping is incomplete"));
                    }
                    let arguments = args
                        .iter()
                        .zip(&call.arguments)
                        .map(|(a, p)| {
                            Ok(Argument {
                                parameter: *p,
                                value: self.expr(&a.value)?,
                            })
                        })
                        .collect::<Result<_>>()?;
                    ExprKind::Call {
                        callee: Callee::Resolved(Callable {
                            target: call.callable.target.clone(),
                            type_arguments: call.callable.type_arguments.clone(),
                            receiver,
                        }),
                        arguments,
                        defaults: call.defaults.clone(),
                    }
                } else {
                    let callee = self.expr(c)?;
                    // The checker accepts a Never callee without checking unreachable
                    // arguments. No invocation (or argument evaluation) can occur.
                    if self.out.expression(callee).ty == Type::Never {
                        ExprKind::Diverge { operand: callee }
                    } else {
                        let arguments = args
                            .iter()
                            .enumerate()
                            .map(|(i, a)| {
                                Ok(Argument {
                                    parameter: i,
                                    value: self.expr(&a.value)?,
                                })
                            })
                            .collect::<Result<_>>()?;
                        ExprKind::Call {
                            callee: Callee::Value(callee),
                            arguments,
                            defaults: vec![],
                        }
                    }
                }
            }
            ast::ExprKind::Struct(_, fields) => ExprKind::Struct {
                nominal: self.nominal(&ty, e.span)?.0,
                fields: self.fields(fields)?,
            },
            ast::ExprKind::Update(base, fields) => ExprKind::Update {
                base: self.expr(base)?,
                nominal: self.nominal(&ty, e.span)?.0,
                fields: self.fields(fields)?,
            },
            ast::ExprKind::List(items) => ExprKind::List {
                element: need(
                    self.nominal(&ty, e.span)?.1.first().cloned(),
                    e.span,
                    "list missing element type",
                )?,
                items: items.iter().map(|e| self.expr(e)).collect::<Result<_>>()?,
            },
            ast::ExprKind::Unary(op, operand) => {
                if op == "-"
                    && let ast::ExprKind::Literal(ast::Literal::Integer(_)) = &operand.kind
                {
                    let ast::ExprKind::Literal(l) = &operand.kind else {
                        return Err(ice(e.span, "integer literal invariant"));
                    };
                    ExprKind::Literal(literal(l, &ty, true, e.span)?)
                } else {
                    let operand_type = self.fact(operand)?.ty.clone();
                    let operand = self.expr(operand)?;
                    match op.as_str() {
                        "await" | "?" if operand_type == Type::Never => {
                            ExprKind::Diverge { operand }
                        }
                        "await" => ExprKind::Await { task: operand },
                        "?" => {
                            let (id, args) = self.nominal(&operand_type, e.span)?;
                            let kind = if id == OPTION {
                                Propagation::Option
                            } else if id == RESULT {
                                Propagation::Result {
                                    error: need(
                                        args.get(1).cloned(),
                                        e.span,
                                        "Result missing error type",
                                    )?,
                                }
                            } else {
                                return Err(ice(e.span, "unclassified propagation"));
                            };
                            let (target, completion) = self.return_target(e.span)?;
                            ExprKind::Propagate {
                                kind,
                                operand,
                                target,
                                completion,
                            }
                        }
                        "!" => ExprKind::Unary {
                            op: UnaryOp::Not,
                            operand_type,
                            operand,
                        },
                        "-" => ExprKind::Unary {
                            op: UnaryOp::Negate,
                            operand_type,
                            operand,
                        },
                        _ => return Err(ice(e.span, "unknown checked unary operator")),
                    }
                }
            }
            ast::ExprKind::Binary(op, a, b) => {
                let operand_type = self.fact(a)?.ty.clone();
                let left = self.expr(a)?;
                let right = self.expr(b)?;
                match op.as_str() {
                    "&&" => ExprKind::Logical {
                        op: LogicalOp::And,
                        left,
                        right,
                    },
                    "||" => ExprKind::Logical {
                        op: LogicalOp::Or,
                        left,
                        right,
                    },
                    _ => ExprKind::Binary {
                        op: match op.as_str() {
                            "+" => BinaryOp::Add,
                            "-" => BinaryOp::Subtract,
                            "*" => BinaryOp::Multiply,
                            "==" => BinaryOp::Equal,
                            "!=" => BinaryOp::NotEqual,
                            "<" => BinaryOp::Less,
                            "<=" => BinaryOp::LessEqual,
                            ">" => BinaryOp::Greater,
                            ">=" => BinaryOp::GreaterEqual,
                            _ => return Err(ice(e.span, "unknown checked binary operator")),
                        },
                        operand_type,
                        left,
                        right,
                    },
                }
            }
            ast::ExprKind::Block(stmts, tail) => {
                let scope = BlockId(self.next_block);
                self.next_block += 1;
                self.scopes.push(scope);
                let statements = stmts
                    .iter()
                    .map(|s| self.statement(s))
                    .collect::<Result<_>>()?;
                let tail = tail.as_ref().map(|e| self.expr(e)).transpose()?;
                self.scopes.pop();
                ExprKind::Block {
                    scope,
                    statements,
                    tail,
                }
            }
            ast::ExprKind::If(c, a, b) => ExprKind::If {
                condition: self.expr(c)?,
                then_branch: self.expr(a)?,
                else_branch: b.as_ref().map(|e| self.expr(e)).transpose()?,
            },
            ast::ExprKind::Match(value, arms) => {
                let value = self.expr(value)?;
                let arms = arms
                    .iter()
                    .map(|(p, e)| Ok((self.pattern(p)?, self.expr(e)?)))
                    .collect::<Result<_>>()?;
                ExprKind::Match { value, arms }
            }
            ast::ExprKind::Lambda {
                params,
                body,
                asynchronous,
                ..
            } => {
                let id = LambdaId(self.next_lambda);
                self.next_lambda += 1;
                let Type::Function(_, result) = &ty else {
                    return Err(ice(e.span, "lambda missing function type"));
                };
                let completion = if *asynchronous {
                    need(
                        self.nominal(result, e.span)?.1.first().cloned(),
                        e.span,
                        "async lambda missing completion type",
                    )?
                } else {
                    *result.clone()
                };
                let target = self
                    .target
                    .replace((ReturnTarget::Lambda(id), completion.clone()));
                let loops = std::mem::take(&mut self.loops);
                let scopes = std::mem::take(&mut self.scopes);
                self.captures.push(CaptureScope::default());
                let parameters = params
                    .iter()
                    .map(|p| self.binding(p.name.span))
                    .collect::<Result<_>>()?;
                let body = self.expr(body)?;
                let scope = self
                    .captures
                    .pop()
                    .ok_or_else(|| ice(e.span, "missing capture scope"))?;
                let captures = scope
                    .captures
                    .into_iter()
                    .map(|local| {
                        Ok(Capture {
                            local,
                            ty: need(self.out.locals.get(&local), e.span, "capture missing local")?
                                .ty
                                .clone(),
                        })
                    })
                    .collect::<Result<_>>()?;
                self.target = target;
                self.loops = loops;
                self.scopes = scopes;
                ExprKind::Lambda {
                    id,
                    parameters,
                    completion,
                    asynchronous: *asynchronous,
                    body,
                    captures,
                }
            }
        };
        let id = ExprId(self.out.expressions.len());
        self.out.expressions.push(Expr {
            id,
            span: e.span,
            ty,
            kind,
        });
        Ok(id)
    }
    fn pattern(&mut self, p: &ast::Pattern) -> Result<Pattern> {
        let ty = need(
            self.checked.resolved.patterns.get(&key(p.span)).cloned(),
            p.span,
            "pattern missing type",
        )?;
        let kind = match &p.kind {
            ast::PatternKind::Wildcard => PatternKind::Wildcard,
            ast::PatternKind::Bind(n) => PatternKind::Bind(self.binding(n.span)?),
            ast::PatternKind::Literal(l) => PatternKind::Literal(literal(l, &ty, false, p.span)?),
            ast::PatternKind::Variant(_, n, children) => PatternKind::Variant(
                self.variant(n.span)?,
                children
                    .iter()
                    .map(|p| self.pattern(p))
                    .collect::<Result<_>>()?,
            ),
        };
        Ok(Pattern {
            span: p.span,
            ty,
            kind,
        })
    }
    fn statement(&mut self, s: &ast::Stmt) -> Result<Stmt> {
        let kind = match &s.kind {
            ast::StmtKind::Bind { name, value, .. } => {
                let value = self.expr(value)?;
                StmtKind::Bind {
                    local: self.binding(name.span)?,
                    value,
                }
            }
            ast::StmtKind::Assign(target, value) => {
                let local = need(
                    self.fact(target)?.symbol,
                    target.span,
                    "assignment missing local",
                )?;
                self.reference(local);
                StmtKind::Assign {
                    local,
                    value: self.expr(value)?,
                }
            }
            ast::StmtKind::Return(e) => StmtKind::Return {
                target: self.return_target(s.span)?.0,
                value: e.as_ref().map(|e| self.expr(e)).transpose()?,
            },
            ast::StmtKind::Defer(e) => StmtKind::Defer {
                scope: need(
                    self.scopes.last().copied(),
                    s.span,
                    "defer missing lexical scope",
                )?,
                value: self.expr(e)?,
            },
            ast::StmtKind::Break => StmtKind::Break(need(
                self.loops.last().copied(),
                s.span,
                "break missing loop",
            )?),
            ast::StmtKind::Continue => StmtKind::Continue(need(
                self.loops.last().copied(),
                s.span,
                "continue missing loop",
            )?),
            ast::StmtKind::Expr(e) => StmtKind::Expr(self.expr(e)?),
            ast::StmtKind::While(c, b) => {
                let condition = self.expr(c)?;
                let id = LoopId(self.next_loop);
                self.next_loop += 1;
                self.loops.push(id);
                let body = self.expr(b)?;
                self.loops.pop();
                StmtKind::While {
                    id,
                    condition,
                    body,
                }
            }
            ast::StmtKind::For(n, items, b) => {
                let (nominal, args) = self.nominal(&self.fact(items)?.ty, items.span)?;
                let iteration = if nominal == LIST {
                    Iteration::List
                } else if nominal == SET {
                    Iteration::Set
                } else {
                    return Err(ice(items.span, "unknown iteration semantics"));
                };
                let element = need(
                    args.first().cloned(),
                    items.span,
                    "iteration missing element type",
                )?;
                let iterable = self.expr(items)?;
                let binding = self.binding(n.span)?;
                let id = LoopId(self.next_loop);
                self.next_loop += 1;
                self.loops.push(id);
                let body = self.expr(b)?;
                self.loops.pop();
                StmtKind::For {
                    id,
                    binding,
                    iterable,
                    iteration,
                    element,
                    body,
                }
            }
        };
        Ok(Stmt { span: s.span, kind })
    }
}
fn literal(l: &ast::Literal, ty: &Type, negative: bool, span: Span) -> Result<Literal> {
    Ok(match l {
        ast::Literal::Integer(s) => {
            let digits: String = s
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '_')
                .filter(|c| *c != '_')
                .collect();
            let magnitude = digits.trim_start_matches('0');
            Literal::Integer {
                negative: negative && !magnitude.is_empty(),
                magnitude: if magnitude.is_empty() {
                    "0".into()
                } else {
                    magnitude.into()
                },
            }
        }
        ast::Literal::Float(s) => {
            let s = s.replace('_', "");
            let s = s
                .strip_suffix("f32")
                .or_else(|| s.strip_suffix("f64"))
                .unwrap_or(&s);
            if *ty == Type::F32 {
                Literal::Float32(s.parse().map_err(|_| ice(span, "invalid checked float"))?)
            } else {
                Literal::Float(s.parse().map_err(|_| ice(span, "invalid checked float"))?)
            }
        }
        ast::Literal::String(s) => Literal::String(s.clone()),
        ast::Literal::Char(c) => Literal::Char(*c),
        ast::Literal::Bool(b) => Literal::Bool(*b),
        ast::Literal::Unit => Literal::Unit,
    })
}
