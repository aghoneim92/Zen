use crate::operations::{binary, literal, unary};
use crate::*;
use std::sync::Arc;

// Every expression consumer propagates non-local Zen flow without host unwinding.
macro_rules! value {
    ($e:expr) => {
        match $e? {
            Flow::Value(v) => v,
            flow => return Ok(flow),
        }
    };
}
impl Machine<'_> {
    pub(crate) fn eval(&mut self, id: ExprId) -> LocalFuture<'_, Evaluation> {
        executor::growing(Box::pin(async move {
            let expr = self
                .program
                .expressions
                .get(id.0)
                .ok_or_else(|| self.invariant("unknown HIR expression ID", Span::default()))?;
            if self.depth >= 512 {
                return Err(self.error(
                    ErrorKind::ResourceLimit,
                    "reference interpreter evaluation stack limit exceeded",
                    Some(expr.span),
                ));
            }
            self.depth += 1;
            let result = self.eval_inner(expr).await;
            self.depth -= 1;
            // A cancellation requested while deferred code was suspended must
            // stop normal execution as soon as that cleanup finishes.
            if result.is_ok() && self.cleanup_depth == 0 && self.cancel_requested() {
                Err(self.error(
                    ErrorKind::Cancelled,
                    "task cancelled by executor",
                    Some(expr.span),
                ))
            } else {
                result
            }
        }))
    }
    async fn eval_inner(&mut self, expr: &Expr) -> Evaluation {
        let span = expr.span;
        let types = self.types();
        let ty = expr.ty.substitute(&types);
        let result = match &expr.kind {
            ExprKind::Literal(v) => literal(v, &ty).map_err(|m| self.invariant(m, span))?,
            ExprKind::Local(id) => self.local(*id, span)?,
            ExprKind::Constant(id) => self.constant(*id, span).await?,
            ExprKind::Callable(c) => return self.callable(c, span).await,
            ExprKind::Call {
                callee,
                arguments,
                defaults,
            } => {
                let (callable, indirect) = match callee {
                    Callee::Resolved(c) => (value!(self.callable(c, span).await), false),
                    Callee::Value(id) => (value!(self.eval(*id).await), true),
                };
                let Value::Callable(callable) = callable else {
                    return Err(self.invariant("non-callable HIR callee", span));
                };
                let offset = usize::from(
                    indirect
                        && matches!(
                            &*callable,
                            CallableValue::Resolved {
                                receiver: Some(_),
                                ..
                            }
                        ),
                );
                let mut supplied = vec![];
                for argument in arguments {
                    supplied.push((
                        argument.parameter + offset,
                        value!(self.eval(argument.value).await),
                    ));
                }
                self.invoke(&callable, supplied, defaults, span).await?
            }
            ExprKind::Field { receiver, field } => {
                let Value::Struct { nominal, fields } = value!(self.eval(*receiver).await) else {
                    return Err(self.invariant("field receiver is not a struct", span));
                };
                if nominal != field.owner {
                    return Err(self.invariant("field owner mismatch", span));
                }
                fields
                    .get(field.index)
                    .cloned()
                    .ok_or_else(|| self.invariant("missing resolved field", span))?
            }
            ExprKind::Struct { nominal, fields } => {
                let Some(NominalDefinition {
                    kind: NominalKind::Struct(metadata),
                    ..
                }) = self.program.nominal_types.get(nominal.0)
                else {
                    return Err(self.invariant("invalid struct nominal", span));
                };
                let mut slots = vec![None; metadata.len()];
                for (field, expression) in fields {
                    let v = value!(self.eval(*expression).await);
                    if field.owner != *nominal {
                        return Err(self.invariant("initializer field owner mismatch", span));
                    }
                    let slot = slots
                        .get_mut(field.index)
                        .ok_or_else(|| self.invariant("invalid initializer field", span))?;
                    if slot.replace(v).is_some() {
                        return Err(self.invariant("duplicate initializer field", span));
                    }
                }
                let fields = slots
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| self.invariant("missing initializer field", span))?;
                Value::Struct {
                    nominal: *nominal,
                    fields,
                }
            }
            ExprKind::Update {
                base,
                nominal,
                fields,
            } => {
                let Value::Struct {
                    nominal: owner,
                    fields: mut slots,
                } = value!(self.eval(*base).await)
                else {
                    return Err(self.invariant("update of non-struct", span));
                };
                if owner != *nominal {
                    return Err(self.invariant("update nominal mismatch", span));
                }
                for (field, expression) in fields {
                    let v = value!(self.eval(*expression).await);
                    if field.owner != owner {
                        return Err(self.invariant("update field owner mismatch", span));
                    }
                    *slots
                        .get_mut(field.index)
                        .ok_or_else(|| self.invariant("missing update field", span))? = v;
                }
                Value::Struct {
                    nominal: owner,
                    fields: slots,
                }
            }
            ExprKind::Variant {
                variant, payload, ..
            } => {
                let mut values = vec![];
                for e in payload {
                    values.push(value!(self.eval(*e).await));
                }
                Value::Enum {
                    variant: *variant,
                    payload: values,
                }
            }
            ExprKind::List { items, .. } => {
                let mut values = vec![];
                for e in items {
                    values.push(value!(self.eval(*e).await));
                }
                Value::List(values.into())
            }
            ExprKind::Unary {
                op,
                operand_type,
                operand,
            } => {
                let v = value!(self.eval(*operand).await);
                unary(*op, &operand_type.substitute(&types), v)
                    .map_err(|m| self.invariant(m, span))?
            }
            ExprKind::Binary {
                op,
                operand_type,
                left,
                right,
            } => {
                let a = value!(self.eval(*left).await);
                let b = value!(self.eval(*right).await);
                binary(*op, &operand_type.substitute(&types), a, b)
                    .map_err(|m| self.invariant(m, span))?
            }
            ExprKind::Logical { op, left, right } => {
                let Value::Bool(a) = value!(self.eval(*left).await) else {
                    return Err(self.invariant("non-Bool logical operand", span));
                };
                if (*op == LogicalOp::And && !a) || (*op == LogicalOp::Or && a) {
                    Value::Bool(a)
                } else {
                    let Value::Bool(b) = value!(self.eval(*right).await) else {
                        return Err(self.invariant("non-Bool logical operand", span));
                    };
                    Value::Bool(b)
                }
            }
            ExprKind::Block {
                scope,
                statements,
                tail,
            } => return self.block(*scope, statements, *tail, span).await,
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let Value::Bool(condition) = value!(self.eval(*condition).await) else {
                    return Err(self.invariant("non-Bool condition", span));
                };
                if condition {
                    return self.eval(*then_branch).await;
                }
                if let Some(branch) = else_branch {
                    return self.eval(*branch).await;
                }
                Value::Unit
            }
            ExprKind::Match {
                value: scrutinee,
                arms,
            } => {
                let v = value!(self.eval(*scrutinee).await);
                for (pattern, arm) in arms {
                    let mut bindings = vec![];
                    if self.pattern(pattern, &v, &mut bindings)? {
                        let ids = bindings.iter().map(|(id, _)| *id).collect::<Vec<_>>();
                        for (id, v) in bindings {
                            // Arm bindings have their own lifetime even when the
                            // arm is not a block. Do not register them in an
                            // enclosing scope on every loop-condition match.
                            self.frames.last_mut().unwrap().locals.insert(
                                id,
                                Slot {
                                    value: v,
                                    mutable: false,
                                },
                            );
                        }
                        let result = self.eval(*arm).await;
                        for id in ids {
                            self.frames.last_mut().unwrap().locals.remove(&id);
                        }
                        return result;
                    }
                }
                return Err(self.invariant("exhaustive HIR match had no matching arm", span));
            }
            ExprKind::Lambda { captures, .. } => {
                let mut values = BTreeMap::new();
                for capture in captures {
                    values.insert(capture.local, self.local(capture.local, span)?);
                }
                Value::Callable(Arc::new(CallableValue::Closure {
                    expression: expr.id,
                    captures: values,
                    types,
                }))
            }
            ExprKind::Await { task: operand, .. } => {
                let Value::Task(task) = value!(self.eval(*operand).await) else {
                    return Err(self.invariant("await operand is not a task", span));
                };
                self.await_task(task, span).await?
            }
            ExprKind::Propagate {
                kind,
                operand,
                target,
                ..
            } => {
                let v = value!(self.eval(*operand).await);
                let Value::Enum { variant, payload } = &v else {
                    return Err(self.invariant("propagation operand is not an enum", span));
                };
                let owner = match kind {
                    Propagation::Option => OPTION,
                    Propagation::Result { .. } => RESULT,
                };
                if variant.owner != owner {
                    return Err(self.invariant("propagation enum mismatch", span));
                }
                match (variant.index, payload.as_slice()) {
                    (0, [value]) => value.clone(),
                    (1, []) if owner == OPTION => return Ok(Flow::Return(*target, v)),
                    (1, [_]) if owner == RESULT => return Ok(Flow::Return(*target, v)),
                    _ => return Err(self.invariant("invalid propagation variant", span)),
                }
            }
            ExprKind::Diverge { operand } => {
                let _ = value!(self.eval(*operand).await);
                return Err(self.invariant("Never operand returned normally", span));
            }
        };
        Ok(Flow::Value(result))
    }

    async fn block(
        &mut self,
        id: BlockId,
        statements: &[Stmt],
        tail: Option<ExprId>,
        span: Span,
    ) -> Evaluation {
        self.frames.last_mut().unwrap().scopes.push(Scope {
            id,
            locals: vec![],
            defers: vec![],
        });
        let mut result = self.block_contents(statements, tail).await;
        // One cleanup path handles normal flow, all non-local flow and execution
        // errors. Keep the first execution error while still attempting cleanup.
        loop {
            let next = self
                .frames
                .last_mut()
                .unwrap()
                .scopes
                .last_mut()
                .unwrap()
                .defers
                .pop();
            let Some(next) = next else {
                break;
            };
            self.cleanup_depth += 1;
            let cleanup = self.eval(next).await;
            self.cleanup_depth -= 1;
            match cleanup {
                Ok(Flow::Value(Value::Unit)) => {}
                Ok(_) => {
                    if result.is_ok() {
                        result = Err(self.invariant(
                            "deferred expression escaped or did not produce Unit",
                            span,
                        ));
                    }
                }
                Err(error) => {
                    if result.is_ok()
                        || result
                            .as_ref()
                            .is_err_and(|e| e.kind == ErrorKind::Cancelled)
                    {
                        result = Err(error);
                    }
                }
            }
        }
        let frame = self.frames.last_mut().unwrap();
        let scope = frame.scopes.pop().unwrap();
        for local in scope.locals {
            frame.locals.remove(&local);
        }
        result
    }
    async fn block_contents(&mut self, statements: &[Stmt], tail: Option<ExprId>) -> Evaluation {
        for statement in statements {
            value!(self.statement(statement).await);
        }
        if let Some(tail) = tail {
            self.eval(tail).await
        } else {
            Ok(Flow::Value(Value::Unit))
        }
    }
    async fn statement(&mut self, statement: &Stmt) -> Evaluation {
        let span = statement.span;
        match &statement.kind {
            StmtKind::Bind {
                local,
                value: expression,
            } => {
                let v = value!(self.eval(*expression).await);
                let mutable = self
                    .program
                    .locals
                    .get(local)
                    .ok_or_else(|| self.invariant("missing local metadata", span))?
                    .mutable;
                self.bind(*local, v, mutable);
            }
            StmtKind::Assign {
                local,
                value: expression,
            } => {
                let v = value!(self.eval(*expression).await);
                let Some(slot) = self.frames.last_mut().and_then(|f| f.locals.get_mut(local))
                else {
                    return Err(self.invariant("assignment to unbound local", span));
                };
                if !slot.mutable {
                    return Err(self.invariant("assignment to immutable local", span));
                }
                slot.value = v;
            }
            StmtKind::Return {
                target,
                value: expression,
            } => {
                let v = if let Some(e) = expression {
                    value!(self.eval(*e).await)
                } else {
                    Value::Unit
                };
                return Ok(Flow::Return(*target, v));
            }
            StmtKind::Defer { scope, value } => {
                let active = self.frames.last_mut().unwrap().scopes.last_mut().unwrap();
                if active.id != *scope {
                    return Err(self.invariant("defer scope mismatch", span));
                }
                active.defers.push(*value);
            }
            StmtKind::Break(id) => return Ok(Flow::Break(*id)),
            StmtKind::Continue(id) => return Ok(Flow::Continue(*id)),
            StmtKind::Expr(e) => {
                value!(self.eval(*e).await);
            }
            StmtKind::While {
                id,
                condition,
                body,
            } => loop {
                let Value::Bool(condition) = value!(self.eval(*condition).await) else {
                    return Err(self.invariant("non-Bool while condition", span));
                };
                if !condition {
                    break;
                }
                match self.eval(*body).await? {
                    Flow::Value(_) => {}
                    Flow::Continue(target) if target == *id => {}
                    Flow::Break(target) if target == *id => break,
                    other => return Ok(other),
                }
            },
            StmtKind::For {
                id,
                binding,
                iterable,
                iteration,
                body,
                ..
            } => {
                let v = value!(self.eval(*iterable).await);
                let items = match (iteration, v) {
                    (Iteration::List, Value::List(items)) | (Iteration::Set, Value::Set(items)) => {
                        items
                    }
                    _ => {
                        return Err(self
                            .invariant("iteration value disagrees with HIR iteration kind", span));
                    }
                };
                for item in items.iter() {
                    // The loop binding is scoped to this iteration, not the enclosing block.
                    self.frames.last_mut().unwrap().locals.insert(
                        *binding,
                        Slot {
                            value: item.clone(),
                            mutable: false,
                        },
                    );
                    let flow = self.eval(*body).await;
                    self.frames.last_mut().unwrap().locals.remove(binding);
                    match flow? {
                        Flow::Value(_) => {}
                        Flow::Continue(target) if target == *id => {}
                        Flow::Break(target) if target == *id => break,
                        other => return Ok(other),
                    }
                }
            }
        }
        Ok(Flow::Value(Value::Unit))
    }
    fn pattern(
        &self,
        pattern: &Pattern,
        value: &Value,
        bindings: &mut Vec<(SymbolId, Value)>,
    ) -> Result<bool, InterpreterError> {
        Ok(match &pattern.kind {
            PatternKind::Wildcard => true,
            PatternKind::Bind(id) => {
                bindings.push((*id, value.clone()));
                true
            }
            PatternKind::Literal(v) => literal(v, &pattern.ty.substitute(&self.types()))
                .and_then(|v| v.equals(value))
                .map_err(|m| self.invariant(m, pattern.span))?,
            PatternKind::Variant(id, patterns) => {
                let Value::Enum { variant, payload } = value else {
                    return Err(self.invariant("enum pattern on non-enum", pattern.span));
                };
                if id != variant {
                    return Ok(false);
                }
                if patterns.len() != payload.len() {
                    return Err(self.invariant("pattern payload length mismatch", pattern.span));
                }
                for (p, v) in patterns.iter().zip(payload) {
                    if !self.pattern(p, v, bindings)? {
                        return Ok(false);
                    }
                }
                true
            }
        })
    }

    async fn callable(&mut self, callable: &Callable, _span: Span) -> Evaluation {
        let receiver = if let Some(e) = callable.receiver {
            Some(value!(self.eval(e).await))
        } else {
            None
        };
        let outer = self.types();
        let mut types = outer.clone();
        for (id, ty) in &callable.type_arguments {
            types.insert(*id, ty.substitute(&outer));
        }
        // Requirement dispatch remains by semantic identities and checked types.
        let target = match &callable.target {
            CallTarget::Requirement {
                method,
                interface,
                receiver,
            } => CallTarget::Requirement {
                method: *method,
                interface: interface.substitute(&types),
                receiver: receiver.substitute(&types),
            },
            other => other.clone(),
        };
        Ok(Flow::Value(Value::Callable(Arc::new(
            CallableValue::Resolved {
                target,
                receiver,
                types,
            },
        ))))
    }
    async fn invoke(
        &mut self,
        callable: &CallableValue,
        mut supplied: Vec<(usize, Value)>,
        defaults: &[usize],
        span: Span,
    ) -> Result<Value, InterpreterError> {
        match callable {
            CallableValue::Resolved {
                target,
                receiver,
                types,
            } => {
                if let Some(receiver) = receiver {
                    supplied.insert(0, (0, receiver.clone()));
                }
                if let CallTarget::Intrinsic(intrinsic) = target {
                    supplied.sort_by_key(|(index, _)| *index);
                    return self.intrinsic(
                        *intrinsic,
                        supplied.into_iter().map(|(_, v)| v).collect(),
                        span,
                    );
                }
                let mut types = types.clone();
                let (function, signature) = match target {
                    CallTarget::Function(id)
                    | CallTarget::Native(id)
                    | CallTarget::Inherent(id) => (*id, *id),
                    CallTarget::Implementation { method, .. } => (*method, *method),
                    CallTarget::Requirement {
                        method,
                        interface,
                        receiver,
                    } => {
                        let implementation = self
                            .program
                            .implementations
                            .iter()
                            .find(|implementation| {
                                implementation.target == *receiver
                                    && implementation.interface.as_ref() == Some(interface)
                            })
                            .ok_or_else(|| {
                                self.invariant("missing checked interface witness", span)
                            })?;
                        let function = implementation
                            .methods
                            .iter()
                            .find(|(_, requirement)| *requirement == Some(*method))
                            .map(|(method, _)| *method)
                            .ok_or_else(|| {
                                self.invariant("missing interface requirement implementation", span)
                            })?;
                        if let Type::Nominal(id, arguments) = interface {
                            let nominal =
                                self.program.nominal_types.get(id.0).ok_or_else(|| {
                                    self.invariant("missing interface nominal", span)
                                })?;
                            types.extend(
                                nominal
                                    .parameters
                                    .iter()
                                    .copied()
                                    .zip(arguments.iter().cloned()),
                            );
                        }
                        (function, *method)
                    }
                    CallTarget::Intrinsic(_) => unreachable!(),
                };
                self.invoke_function(function, signature, types, supplied, defaults, span)
                    .await
            }
            CallableValue::Closure {
                expression,
                captures,
                types,
            } => {
                let Some(Expr {
                    kind:
                        ExprKind::Lambda {
                            id,
                            parameters,
                            asynchronous,
                            body,
                            ..
                        },
                    ..
                }) = self.program.expressions.get(expression.0)
                else {
                    return Err(self.invariant("closure does not refer to a lambda", span));
                };
                let (id, body, asynchronous, parameters) =
                    (*id, *body, *asynchronous, parameters.clone());
                let frame_count = self.frames.len();
                self.frames.push(Frame {
                    trace: StackFrame {
                        identity: FrameIdentity::Lambda(id),
                        name: format!("lambda#{}", id.0),
                        call_site: span,
                    },
                    locals: captures
                        .iter()
                        .map(|(id, v)| {
                            (
                                *id,
                                Slot {
                                    value: v.clone(),
                                    mutable: false,
                                },
                            )
                        })
                        .collect(),
                    types: types.clone(),
                    scopes: vec![],
                });
                let result = async {
                    if supplied.len() != parameters.len() || !defaults.is_empty() {
                        return Err(self.invariant("invalid closure arguments", span));
                    }
                    for (index, value) in supplied {
                        let local = parameters.get(index).ok_or_else(|| {
                            self.invariant("invalid closure parameter index", span)
                        })?;
                        self.bind(*local, value, false);
                    }
                    if asynchronous {
                        let frame = self.frames.pop().unwrap();
                        return Ok(self.spawn(executor::Entry::Body {
                            frame,
                            body,
                            target: ReturnTarget::Lambda(id),
                            span,
                        }));
                    }
                    let flow = self.eval(body).await?;
                    self.finish(flow, ReturnTarget::Lambda(id), span)
                }
                .await;
                self.frames.truncate(frame_count);
                result
            }
        }
    }
    pub(crate) async fn invoke_function(
        &mut self,
        function: SymbolId,
        signature: SymbolId,
        types: BTreeMap<ParamId, Type>,
        supplied: Vec<(usize, Value)>,
        defaults: &[usize],
        span: Span,
    ) -> Result<Value, InterpreterError> {
        let function = self.function(function, span)?;
        let signature = self.function(signature, span)?;
        let frame_count = self.frames.len();
        self.frames.push(Frame {
            trace: StackFrame {
                identity: FrameIdentity::Function(function.id),
                name: function.name.clone(),
                call_site: span,
            },
            locals: BTreeMap::new(),
            types,
            scopes: vec![],
        });
        let result = async {
            if function.parameters.len() != signature.parameters.len() {
                return Err(self.invariant("interface signature arity mismatch", span));
            }
            let mut values = vec![None; signature.parameters.len()];
            for (index, value) in supplied {
                let slot = values
                    .get_mut(index)
                    .ok_or_else(|| self.invariant("invalid argument parameter index", span))?;
                if slot.replace(value.clone()).is_some() {
                    return Err(self.invariant("duplicate supplied parameter", span));
                }
                self.bind(signature.parameters[index].local, value, false);
            }
            // HIR defaults identify omissions; declaration order is the execution
            // order so each initializer can read all earlier parameters.
            for (index, parameter) in signature.parameters.iter().enumerate() {
                if values[index].is_none() {
                    if !defaults.contains(&index) {
                        return Err(self.invariant("missing required call argument", span));
                    }
                    let expression = parameter
                        .default
                        .ok_or_else(|| self.invariant("missing default initializer", span))?;
                    let flow = self.eval(expression).await?;
                    let Flow::Value(value) = flow else {
                        return Err(self.invariant("control flow in parameter default", span));
                    };
                    self.bind(parameter.local, value.clone(), false);
                    values[index] = Some(value);
                }
            }
            let values = values
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| self.invariant("unbound call parameter", span))?;
            // Defaults on requirements use requirement locals; bodies use the
            // corresponding implementation parameter identities.
            self.frames.last_mut().unwrap().locals.clear();
            for (parameter, value) in function.parameters.iter().zip(&values) {
                self.bind(parameter.local, value.clone(), false);
            }
            if function.asynchronous {
                let frame = self.frames.pop().unwrap();
                let entry = match function.body {
                    Body::Zen(body) => executor::Entry::Body {
                        frame,
                        body,
                        target: ReturnTarget::Function(function.id),
                        span,
                    },
                    Body::Native => executor::Entry::Native {
                        frame,
                        function: function.id,
                        values,
                        span,
                    },
                    Body::Requirement => {
                        return Err(self.invariant("unresolved async requirement", span));
                    }
                };
                return Ok(self.spawn(entry));
            }
            match function.body {
                Body::Zen(body) => {
                    let flow = self.eval(body).await?;
                    self.finish(flow, ReturnTarget::Function(function.id), span)
                }
                Body::Native => self.native(function.id, values, span).await,

                Body::Requirement => {
                    Err(self.invariant("unresolved interface requirement invoked", span))
                }
            }
        }
        .await;
        self.frames.truncate(frame_count);
        result
    }
    async fn constant(&mut self, id: SymbolId, span: Span) -> Result<Value, InterpreterError> {
        if let Some(value) = self.shared.borrow().constants.get(&id) {
            return Ok(value.clone());
        }
        if !self.evaluating_constants.insert(id) {
            return Err(self.error(
                ErrorKind::ConstantCycle(id),
                "recursive constant dependency",
                Some(span),
            ));
        }
        let Some(constant) = self.program.constants.iter().find(|c| c.id == id) else {
            self.evaluating_constants.remove(&id);
            return Err(self.invariant("unknown HIR constant ID", span));
        };
        self.frames.push(Frame {
            trace: StackFrame {
                identity: FrameIdentity::Constant(id),
                name: constant.name.clone(),
                call_site: span,
            },
            locals: BTreeMap::new(),
            types: BTreeMap::new(),
            scopes: vec![],
        });
        let result = self.eval(constant.value).await.and_then(|flow| match flow {
            Flow::Value(value) => Ok(value),
            _ => Err(self.invariant("control flow in constant initializer", span)),
        });
        self.frames.pop();
        self.evaluating_constants.remove(&id);
        if let Ok(value) = &result {
            self.shared.borrow_mut().constants.insert(id, value.clone());
        }
        result
    }
}
