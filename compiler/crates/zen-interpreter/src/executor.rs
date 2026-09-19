//! The FIFO queue and task table define scheduling. Rust futures only retain
//! evaluator continuations; their wakers and drop behavior have no Zen semantics.
use crate::*;
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};

pub(crate) type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
type Completion = Result<Value, InterpreterError>;

/// Identity belongs to one interpreter. Only the stable allocation index is displayed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId {
    owner: usize,
    index: usize,
}
impl std::fmt::Debug for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Task#{}", self.index)
    }
}
impl TaskId {
    pub fn index(self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Created,
    Ready,
    Running,
    Waiting(TaskId),
    Completed,
    Cancelled,
    Faulted,
}
impl TaskState {
    fn terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Faulted)
    }
}

pub(crate) enum Entry {
    Invoke {
        function: SymbolId,
        types: BTreeMap<ParamId, Type>,
        arguments: Vec<Value>,
        span: Span,
    },
    Body {
        frame: Frame,
        body: ExprId,
        target: ReturnTarget,
        span: Span,
    },
    Native {
        frame: Frame,
        function: SymbolId,
        values: Vec<Value>,
        span: Span,
    },
}
struct NativeRequest {
    function: SymbolId,
    types: BTreeMap<ParamId, Type>,
    arguments: Vec<Value>,
    span: Span,
}
struct Task {
    state: TaskState,
    entry: Option<Entry>,
    result: Option<Completion>,
    parent: Option<TaskId>,
    owner_scope: Option<BlockId>,
    children: Vec<TaskId>,
    waiters: Vec<TaskId>,
    cancelled: bool,
    name: String,
    request: Option<NativeRequest>,
    response: Option<Completion>,
    abort_wait: Option<InterpreterError>,
}
pub(crate) struct Shared {
    owner: usize,
    tasks: Vec<Task>,
    ready: VecDeque<TaskId>,
    pub constants: BTreeMap<SymbolId, Value>,
}
impl Shared {
    fn valid(&self, id: TaskId) -> bool {
        id.owner == self.owner && id.index < self.tasks.len()
    }
    fn create(&mut self, entry: Entry, parent: Option<TaskId>, name: String) -> TaskId {
        let id = TaskId {
            owner: self.owner,
            index: self.tasks.len(),
        };
        self.tasks.push(Task {
            state: TaskState::Created,
            entry: Some(entry),
            result: None,
            parent,
            owner_scope: None,
            children: vec![],
            waiters: vec![],
            cancelled: false,
            name,
            request: None,
            response: None,
            abort_wait: None,
        });
        if let Some(parent) = parent {
            self.tasks[parent.index].children.push(id);
        }
        id
    }
    fn ready(&mut self, id: TaskId) {
        let task = &mut self.tasks[id.index];
        if !task.state.terminal()
            && task.state != TaskState::Ready
            && task.state != TaskState::Running
        {
            task.state = TaskState::Ready;
            self.ready.push_back(id);
        }
    }
    fn complete(&mut self, id: TaskId, result: Completion) {
        let task = &mut self.tasks[id.index];
        task.state = match &result {
            Ok(_) => TaskState::Completed,
            Err(e) if e.kind == ErrorKind::Cancelled => TaskState::Cancelled,
            Err(_) => TaskState::Faulted,
        };
        task.result = Some(result);
        let waiters = std::mem::take(&mut task.waiters);
        for waiter in waiters {
            if self.tasks[waiter.index].state == TaskState::Waiting(id) {
                self.ready(waiter);
            }
        }
    }
}

pub(crate) struct Machine<'p> {
    pub program: &'p Program,
    pub shared: Rc<RefCell<Shared>>,
    task: TaskId,
    pub frames: Vec<Frame>,
    pub evaluating_constants: BTreeSet<SymbolId>,
    pub depth: usize,
    pub cleanup_depth: usize,
}
impl<'p> Machine<'p> {
    fn new(program: &'p Program, shared: Rc<RefCell<Shared>>, task: TaskId) -> Self {
        Self {
            program,
            shared,
            task,
            frames: vec![],
            evaluating_constants: BTreeSet::new(),
            depth: 0,
            cleanup_depth: 0,
        }
    }
    pub(crate) fn cancel_requested(&self) -> bool {
        self.shared.borrow().tasks[self.task.index].cancelled
    }
    pub(crate) fn spawn(&mut self, entry: Entry) -> Value {
        let name = match &entry {
            Entry::Body { frame, .. } | Entry::Native { frame, .. } => frame.trace.name.clone(),
            Entry::Invoke { .. } => "invocation".into(),
        };
        let mut runtime = self.shared.borrow_mut();
        let id = runtime.create(entry, Some(self.task), name);
        runtime.tasks[id.index].owner_scope = self
            .frames
            .last()
            .and_then(|f| f.scopes.last())
            .map(|s| s.id);
        Value::Task(id)
    }
    pub(crate) async fn await_task(&mut self, target: TaskId, span: Span) -> Completion {
        let shared = self.shared.clone();
        std::future::poll_fn(|_| {
            let mut runtime = shared.borrow_mut();
            if let Some(mut error) = runtime.tasks[self.task.index].abort_wait.take() {
                error.span = Some(span);
                error.stack = self.frames.iter().map(|f| f.trace.clone()).collect();
                return Poll::Ready(Err(error));
            }
            if runtime.tasks[self.task.index].cancelled && self.cleanup_depth == 0 {
                return Poll::Ready(Err(self.error(
                    ErrorKind::Cancelled,
                    "task cancelled by executor",
                    Some(span),
                )));
            }
            if !runtime.valid(target) {
                return Poll::Ready(Err(
                    self.invariant("task handle belongs to another interpreter", span)
                ));
            }
            if target == self.task {
                return Poll::Ready(Err(self.error(
                    ErrorKind::Deadlock,
                    "task attempted to await itself",
                    Some(span),
                )));
            }
            if let Some(result) = runtime.tasks[target.index].result.clone() {
                return Poll::Ready(match result {
                    Ok(value) => Ok(value),
                    Err(e) if e.kind == ErrorKind::Cancelled => Err(self.error(
                        ErrorKind::UnspecifiedCancellation,
                        "awaiting a cancelled task has unspecified language semantics",
                        Some(span),
                    )),
                    Err(mut e) => {
                        let mut stack = self
                            .frames
                            .iter()
                            .map(|f| f.trace.clone())
                            .collect::<Vec<_>>();
                        stack.extend(e.stack);
                        e.stack = stack;
                        e.message = format!(
                            "Task#{} awaiting Task#{}: {}",
                            self.task.index, target.index, e.message
                        );
                        Err(e)
                    }
                });
            }
            if runtime.tasks[target.index].state == TaskState::Created {
                runtime.ready(target);
            }
            runtime.tasks[self.task.index].state = TaskState::Waiting(target);
            let waiters = &mut runtime.tasks[target.index].waiters;
            if !waiters.contains(&self.task) {
                waiters.push(self.task);
            }
            Poll::Pending
        })
        .await
    }
    // Native requests are serviced inline by the executor and immediately
    // repolled, so a synchronous native call never yields to another Zen task.
    pub(crate) async fn native(
        &mut self,
        function: SymbolId,
        arguments: Vec<Value>,
        span: Span,
    ) -> Completion {
        self.shared.borrow_mut().tasks[self.task.index].request = Some(NativeRequest {
            function,
            types: self.types(),
            arguments,
            span,
        });
        std::future::poll_fn(|_| {
            match self.shared.borrow_mut().tasks[self.task.index]
                .response
                .take()
            {
                Some(Err(mut error)) => {
                    error.stack = self.frames.iter().map(|f| f.trace.clone()).collect();
                    Poll::Ready(Err(error))
                }
                Some(result) => Poll::Ready(result),
                None => Poll::Pending,
            }
        })
        .await
    }
    async fn execute(mut self, entry: Entry) -> Completion {
        match entry {
            Entry::Invoke {
                function,
                types,
                arguments,
                span,
            } => {
                self.invoke_function(
                    function,
                    function,
                    types,
                    arguments.into_iter().enumerate().collect(),
                    &[],
                    span,
                )
                .await
            }
            Entry::Body {
                frame,
                body,
                target,
                span,
            } => {
                self.frames.push(frame);
                let flow = self.eval(body).await?;
                self.finish(flow, target, span)
            }
            Entry::Native {
                frame,
                function,
                values,
                span,
            } => {
                self.frames.push(frame);
                self.native(function, values, span).await
            }
        }
    }
}

struct Growing<'a, T>(LocalFuture<'a, T>);
impl<T> Future for Growing<'_, T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || self.0.as_mut().poll(cx))
    }
}
pub(crate) fn growing<'a, T: 'a>(future: LocalFuture<'a, T>) -> LocalFuture<'a, T> {
    Box::pin(Growing(future))
}

/// Single-threaded reference interpreter. Task handles are scoped to this instance.
pub struct Interpreter<'p, H: Host = NoHost> {
    program: &'p Program,
    pub host: H,
    shared: Rc<RefCell<Shared>>,
    continuations: Vec<Option<LocalFuture<'p, Completion>>>,
}
impl<'p, H: Host> Interpreter<'p, H> {
    pub fn new(program: &'p Program, host: H) -> Self {
        static NEXT_OWNER: AtomicUsize = AtomicUsize::new(0);
        Self {
            program,
            host,
            shared: Rc::new(RefCell::new(Shared {
                owner: NEXT_OWNER.fetch_add(1, Ordering::Relaxed),
                tasks: vec![],
                ready: VecDeque::new(),
                constants: BTreeMap::new(),
            })),
            continuations: vec![],
        }
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
            stack: vec![],
        }
    }
    /// Execute a conventional main; only an async main is driven to completion.
    /// Remaining tasks are cancelled and cleaned before returning.
    pub fn run_main(&mut self, module: FileId) -> Completion {
        let result = self.run_main_inner(module);
        let cleanup = self.shutdown();
        result.and_then(|v| cleanup.map(|()| v))
    }
    fn run_main_inner(&mut self, module: FileId) -> Completion {
        let main = self
            .program
            .modules
            .iter()
            .find(|m| m.id == module)
            .and_then(|m| {
                self.program.functions.iter().find(|f| {
                    m.functions.contains(&f.id)
                        && f.name == "main"
                        && !matches!(f.body, Body::Requirement)
                        && !self
                            .program
                            .implementations
                            .iter()
                            .any(|i| i.methods.iter().any(|(id, _)| *id == f.id))
                })
            })
            .ok_or_else(|| {
                self.error(
                    ErrorKind::MissingEntryPoint,
                    "entry module has no main function",
                    None,
                )
            })?;
        if !main.parameters.is_empty()
            || !main.generics.is_empty()
            || !matches!(
                main.completion,
                Type::Unit | Type::Int | Type::Bool | Type::String
            )
        {
            return Err(self.error(ErrorKind::InvalidEntryPoint,
                "reference main must have no parameters or generics and return Unit, Int, Bool, or String", Some(main.span)));
        }
        let value = self.run_function(main.id, vec![])?;
        if main.asynchronous {
            let Value::Task(id) = value else {
                return Err(self.error(
                    ErrorKind::Invariant,
                    "async entry did not produce task",
                    Some(main.span),
                ));
            };
            self.drive_task(id)
        } else {
            Ok(value)
        }
    }
    /// Invoke a resolved function. Async functions return a Task value.
    pub fn run_function(&mut self, function: SymbolId, arguments: Vec<Value>) -> Completion {
        self.run_function_with_types(function, BTreeMap::new(), arguments)
    }
    pub fn run_function_with_types(
        &mut self,
        function: SymbolId,
        types: BTreeMap<ParamId, Type>,
        arguments: Vec<Value>,
    ) -> Completion {
        let f = self
            .program
            .functions
            .iter()
            .find(|f| f.id == function)
            .ok_or_else(|| self.error(ErrorKind::Invariant, "unknown HIR function ID", None))?;
        if f.generics.iter().any(|g| !types.contains_key(&g.id))
            || arguments.len() != f.parameters.len()
        {
            return Err(self.error(
                ErrorKind::Invariant,
                "embedding call requires all parameters and type arguments",
                Some(f.span),
            ));
        }
        let id = self.shared.borrow_mut().create(
            Entry::Invoke {
                function,
                types,
                arguments,
                span: f.span,
            },
            None,
            format!("invoke {}", f.name),
        );
        self.drive_task(id)
    }
    /// Drive a task until completion. Completed values can be observed repeatedly.
    pub fn drive_task(&mut self, id: TaskId) -> Completion {
        if !self.shared.borrow().valid(id) {
            return Err(self.error(ErrorKind::Invariant, "invalid task handle", None));
        }
        if self.shared.borrow().tasks[id.index].state == TaskState::Created {
            self.shared.borrow_mut().ready(id);
        }
        loop {
            if let Some(result) = self.shared.borrow().tasks[id.index].result.clone() {
                return result.map_err(|e| {
                    if e.kind == ErrorKind::Cancelled {
                        self.error(
                            ErrorKind::UnspecifiedCancellation,
                            "awaiting a cancelled task has unspecified language semantics",
                            None,
                        )
                    } else {
                        e
                    }
                });
            }
            if !self.step() {
                return Err(self.error(ErrorKind::Deadlock,
                    format!("async executor made no progress while root Task#{} remained incomplete\n{}", id.index, self.task_dump()), None));
            }
        }
    }
    fn step(&mut self) -> bool {
        let Some(id) = self.shared.borrow_mut().ready.pop_front() else {
            return false;
        };
        {
            let mut runtime = self.shared.borrow_mut();
            if runtime.tasks[id.index].state != TaskState::Ready {
                return true;
            }
            runtime.tasks[id.index].state = TaskState::Running;
        }
        while self.continuations.len() <= id.index {
            self.continuations.push(None);
        }
        let entry = self.shared.borrow_mut().tasks[id.index].entry.take();
        if let Some(entry) = entry {
            if self.shared.borrow().tasks[id.index].cancelled {
                let error = self.error(
                    ErrorKind::Cancelled,
                    "task cancelled before execution",
                    None,
                );
                self.shared.borrow_mut().complete(id, Err(error));
                return true;
            }
            let machine = Machine::new(self.program, self.shared.clone(), id);
            self.continuations[id.index] = Some(Box::pin(machine.execute(entry)));
        }
        let Some(mut future) = self.continuations[id.index].take() else {
            return true;
        };
        loop {
            let poll = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            match poll {
                Poll::Ready(result) => {
                    self.shared.borrow_mut().complete(id, result);
                    break;
                }
                Poll::Pending => {
                    let request = self.shared.borrow_mut().tasks[id.index].request.take();
                    if let Some(request) = request {
                        let result = match self.host.call_native(
                            request.function,
                            &request.types,
                            &request.arguments,
                        ) {
                            Some(Ok(value)) => Ok(value),
                            Some(Err(message)) => {
                                Err(self.error(ErrorKind::Host, message, Some(request.span)))
                            }
                            None => Err(self.error(
                                ErrorKind::UnboundNative(request.function),
                                "native function has no reference-interpreter implementation",
                                Some(request.span),
                            )),
                        };
                        self.shared.borrow_mut().tasks[id.index].response = Some(result);
                    } else {
                        self.continuations[id.index] = Some(future);
                        break;
                    }
                }
            }
        }
        true
    }
    /// Deterministic developer diagnostics; never emitted on stdout automatically.
    pub fn task_dump(&self) -> String {
        self.shared
            .borrow()
            .tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let state = match task.state {
                    TaskState::Waiting(id) => format!("Waiting(Task#{})", id.index),
                    ref s => format!("{s:?}"),
                };
                format!(
                    "Task#{i} [{state}] {} parent={} scope={:?}\n",
                    task.name,
                    task.parent
                        .map(|p| p.index.to_string())
                        .unwrap_or_else(|| "none".into()),
                    task.owner_scope
                )
            })
            .collect()
    }
    pub fn task_state(&self, id: TaskId) -> Option<TaskState> {
        let runtime = self.shared.borrow();
        runtime
            .valid(id)
            .then(|| runtime.tasks[id.index].state.clone())
    }
    // Internal policy mechanism. Source APIs/ownership transfer are intentionally absent.
    fn cancel(&mut self, id: TaskId, children: bool) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let mut runtime = self.shared.borrow_mut();
            if children {
                pending.extend(runtime.tasks[id.index].children.iter().rev().copied());
            }
            if !runtime.tasks[id.index].state.terminal() {
                runtime.tasks[id.index].cancelled = true;
                runtime.ready(id);
            }
        }
    }

    /// End an embedding session, running registered cleanup on unfinished tasks.
    pub fn shutdown(&mut self) -> Result<(), InterpreterError> {
        let mut first_error = None;
        loop {
            let ids: Vec<_> = {
                let runtime = self.shared.borrow();
                runtime
                    .tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| !t.state.terminal())
                    .map(|(index, _)| TaskId {
                        owner: runtime.owner,
                        index,
                    })
                    .collect()
            };
            if ids.is_empty() {
                break;
            }
            for id in &ids {
                self.cancel(*id, false);
            }
            while self.step() {}
            let mut runtime = self.shared.borrow_mut();
            for id in ids {
                if let Some(Err(e)) = &runtime.tasks[id.index].result {
                    if e.kind != ErrorKind::Cancelled && first_error.is_none() {
                        first_error = Some(e.clone());
                    }
                } else if !runtime.tasks[id.index].state.terminal() {
                    // Cleanup may itself await a cycle. Fail that await and poll
                    // the continuation again so remaining defers still run.
                    let error = self.error(ErrorKind::Deadlock, "cleanup made no progress", None);
                    if first_error.is_none() {
                        first_error = Some(error.clone());
                    }
                    runtime.tasks[id.index].abort_wait = Some(error);
                    runtime.ready(id);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}
impl<H: Host> Drop for Interpreter<'_, H> {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
