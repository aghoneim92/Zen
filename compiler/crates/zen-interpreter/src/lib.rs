//! Deterministic execution of checked, owned HIR.
//! See `docs/interpreter.md` for entry points, evaluation order and host contracts.
mod error;
mod eval;
mod executor;
pub use executor::{Interpreter, TaskId, TaskState};
use executor::{LocalFuture, Machine};
mod operations;
mod value;
pub use error::*;
use std::collections::{BTreeMap, BTreeSet};
pub use value::{BigInt, CallableValue, Value};
use zen_diagnostics::{FileId, Span};
use zen_hir::*;

/// Optional reference-native bridge, explicitly keyed by this program's IDs.
/// Returning None leaves a declaration unbound. Implementations must honor its
/// checked signature. No native functions or console APIs are bound by default.
pub trait Host {
    fn call_native(
        &mut self,
        _function: SymbolId,
        _types: &BTreeMap<ParamId, Type>,
        _arguments: &[Value],
    ) -> Option<Result<Value, String>> {
        None
    }
}
#[derive(Default)]
pub struct NoHost;
impl Host for NoHost {}

#[derive(Clone)]
struct Slot {
    value: Value,
    mutable: bool,
}
struct Scope {
    id: BlockId,
    locals: Vec<SymbolId>,
    defers: Vec<ExprId>,
}
struct Frame {
    trace: StackFrame,
    locals: BTreeMap<SymbolId, Slot>,
    types: BTreeMap<ParamId, Type>,
    scopes: Vec<Scope>,
}
#[derive(Debug)]
enum Flow {
    Value(Value),
    Return(ReturnTarget, Value),
    Break(LoopId),
    Continue(LoopId),
}
type Evaluation = Result<Flow, InterpreterError>;

impl<'p> Machine<'p> {
    fn function(&self, id: SymbolId, span: Span) -> Result<&'p Function, InterpreterError> {
        self.program
            .functions
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| self.invariant("unknown HIR function ID", span))
    }
    fn error(
        &self,
        kind: ErrorKind,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> InterpreterError {
        InterpreterError {
            kind,
            message: message.into(),
            span,
            stack: self.frames.iter().map(|f| f.trace.clone()).collect(),
        }
    }
    fn invariant(&self, message: impl Into<String>, span: Span) -> InterpreterError {
        self.error(ErrorKind::Invariant, message, Some(span))
    }
    fn types(&self) -> BTreeMap<ParamId, Type> {
        self.frames
            .last()
            .map(|f| f.types.clone())
            .unwrap_or_default()
    }
    fn bind(&mut self, id: SymbolId, value: Value, mutable: bool) {
        let frame = self
            .frames
            .last_mut()
            .expect("execution always has a frame");
        frame.locals.insert(id, Slot { value, mutable });
        if let Some(scope) = frame.scopes.last_mut() {
            scope.locals.push(id);
        }
    }
    fn local(&self, id: SymbolId, span: Span) -> Result<Value, InterpreterError> {
        self.frames
            .last()
            .and_then(|f| f.locals.get(&id))
            .map(|s| s.value.clone())
            .ok_or_else(|| self.invariant("unbound HIR local ID", span))
    }
    fn finish(
        &self,
        flow: Flow,
        target: ReturnTarget,
        span: Span,
    ) -> Result<Value, InterpreterError> {
        match flow {
            Flow::Value(v) => Ok(v),
            Flow::Return(t, v) if t == target => Ok(v),
            _ => Err(self.invariant("control flow escaped its resolved target", span)),
        }
    }
}
