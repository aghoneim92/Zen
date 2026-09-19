use super::*;
fn lower(source: &str) -> Program {
    let (syntax, diagnostics) = zen_syntax::parse(FileId(0), source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let checked = zen_semantics::check(vec![zen_semantics::ModuleInput {
        name: "main".into(),
        file: FileId(0),
        syntax,
    }]);
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    lower_program(&checked).unwrap()
}
fn function(p: &Program, name: &str) -> SymbolId {
    p.functions.iter().find(|f| f.name == name).unwrap().id
}
fn task<H: Host>(i: &mut Interpreter<'_, H>, name: &str, args: Vec<Value>) -> TaskId {
    let Value::Task(id) = i.run_function(function(i.program, name), args).unwrap() else {
        panic!("expected task")
    };
    id
}
#[derive(Default)]
struct Recorder(Vec<String>);
impl Host for Recorder {
    fn call_native(
        &mut self,
        _: SymbolId,
        _: &BTreeMap<ParamId, Type>,
        args: &[Value],
    ) -> Option<Result<Value, String>> {
        self.0.extend(args.iter().map(ToString::to_string));
        Some(Ok(Value::Unit))
    }
}
#[test]
fn fifo_identity_completed_values_and_stable_dump() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn work(n: Int) -> Int { record(n); return n; }",
    );
    let run = || {
        let mut i = Interpreter::new(&p, Recorder::default());
        let a = task(&mut i, "work", vec![Value::Int(1.into())]);
        let b = task(&mut i, "work", vec![Value::Int(2.into())]);
        assert!(a.index < b.index);
        assert_eq!(i.task_state(a), Some(TaskState::Created));
        assert!(i.host.0.is_empty());
        i.shared.borrow_mut().ready(b);
        i.shared.borrow_mut().ready(a);
        i.shared.borrow_mut().ready(b); // duplicate wake is harmless
        assert_eq!(i.shared.borrow().ready.len(), 2);
        assert_eq!(i.drive_task(a).unwrap().to_string(), "Int(1)");
        assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
        assert_eq!(i.drive_task(b).unwrap().to_string(), "Int(2)");
        i.shared.borrow_mut().ready(a);
        assert!(!i.step());
        assert_eq!(i.task_state(a), Some(TaskState::Completed));
        i.task_dump()
    };
    assert_eq!(run(), run());
}
#[test]
fn multiple_waiters_resume_fifo_without_reexecuting() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn work() -> Int { return 40; } async fn waiter(t: Task<Int>, n: Int) -> Int { let v = await t; record(n); return v + n; }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let child = task(&mut i, "work", vec![]);
    let a = task(
        &mut i,
        "waiter",
        vec![Value::Task(child), Value::Int(1.into())],
    );
    let b = task(
        &mut i,
        "waiter",
        vec![Value::Task(child), Value::Int(2.into())],
    );
    i.shared.borrow_mut().ready(a);
    i.shared.borrow_mut().ready(b);
    assert!(i.step());
    assert_eq!(i.task_state(a), Some(TaskState::Waiting(child)));
    assert!(i.step());
    assert_eq!(i.task_state(b), Some(TaskState::Waiting(child)));
    assert_eq!(i.shared.borrow().tasks[child.index].waiters, [a, b]);
    assert!(i.step());
    assert_eq!(i.task_state(child), Some(TaskState::Completed));
    assert_eq!(
        i.shared.borrow().ready.iter().copied().collect::<Vec<_>>(),
        [a, b]
    );
    assert_eq!(i.drive_task(b).unwrap().to_string(), "Int(42)");
    assert_eq!(i.drive_task(a).unwrap().to_string(), "Int(41)");
    assert_eq!(i.host.0, ["Int(1)", "Int(2)"]);
}
#[test]
fn cancel_suspended_scopes_runs_registered_defers_once() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn leaf() -> Unit {} async fn work() -> Unit { defer record(1); { defer record(2); await leaf(); defer record(99); } record(100); }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let root = task(&mut i, "work", vec![]);
    i.shared.borrow_mut().ready(root);
    assert!(i.step());
    assert!(matches!(i.task_state(root), Some(TaskState::Waiting(_))));
    assert!(i.host.0.is_empty());
    i.cancel(root, true);
    while i.step() {}
    assert_eq!(i.task_state(root), Some(TaskState::Cancelled));
    assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
    assert_eq!(
        i.drive_task(root).unwrap_err().kind,
        ErrorKind::UnspecifiedCancellation
    );
    i.cancel(root, true);
    i.shutdown().unwrap();
    assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
    assert!(i.shared.borrow().tasks.iter().all(|t| t.state.terminal()));
}
#[test]
fn cancellation_cleanup_can_suspend_and_shutdown_runs_it() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn leaf() -> Unit {} async fn cleanup() -> Unit { await leaf(); record(2); } async fn work() -> Unit { defer record(1); defer await cleanup(); await leaf(); record(99); }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let root = task(&mut i, "work", vec![]);
    i.shared.borrow_mut().ready(root);
    i.step();
    i.shutdown().unwrap();
    assert_eq!(i.task_state(root), Some(TaskState::Cancelled));
    assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
}
#[test]
fn cancel_created_task_never_enters_body() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn work() -> Unit { defer record(1); record(2); }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let root = task(&mut i, "work", vec![]);
    i.cancel(root, false);
    assert!(i.step());
    assert_eq!(i.task_state(root), Some(TaskState::Cancelled));
    assert!(i.host.0.is_empty());
}
// Create valid task-taking HIR, then supply cyclic handles through the
// embedding boundary. Source cannot manufacture these identities itself.
fn set_argument<H: Host>(i: &mut Interpreter<'_, H>, id: TaskId, target: TaskId) {
    let mut r = i.shared.borrow_mut();
    let Some(Entry::Body { frame, .. }) = &mut r.tasks[id.index].entry else {
        panic!()
    };
    frame.locals.values_mut().next().unwrap().value = Value::Task(target);
}
#[test]
fn self_await_cycles_and_no_progress_are_errors() {
    let p = lower("async fn wait(t: Task<Int>) -> Int { return await t; }");
    let mut i = Interpreter::new(&p, NoHost);
    let a = task(&mut i, "wait", vec![Value::Unit]);
    set_argument(&mut i, a, a);
    let e = i.drive_task(a).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Deadlock);
    assert!(e.message.contains("itself"));
    let a = task(&mut i, "wait", vec![Value::Unit]);
    let b = task(&mut i, "wait", vec![Value::Task(a)]);
    set_argument(&mut i, a, b);
    let e = i.drive_task(a).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Deadlock);
    assert!(e.message.contains("no progress") && e.message.contains("Waiting"));
    i.shutdown().unwrap();
}
#[test]
fn result_err_is_completed_and_native_failure_retains_await_stack() {
    let p = lower(
        "async fn bad() -> Result<Int, String> { return .err(\"ordinary\"); } native async fn missing() -> Int; async fn outer() -> Int { return await missing(); }",
    );
    let mut i = Interpreter::new(&p, NoHost);
    let a = task(&mut i, "bad", vec![]);
    assert!(matches!(i.drive_task(a).unwrap(), Value::Enum { .. }));
    assert_eq!(i.task_state(a), Some(TaskState::Completed));
    let b = task(&mut i, "outer", vec![]);
    let e = i.drive_task(b).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnboundNative(function(&p, "missing")));
    assert_eq!(
        e.stack.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["outer", "missing"]
    );
    assert!(e.message.contains("awaiting Task#"));
    assert_eq!(i.task_state(b), Some(TaskState::Faulted));
}
#[test]
fn completed_await_reuses_value_and_foreign_handles_are_rejected() {
    let p = lower(
        "async fn work() -> Int { return 21; } async fn twice(t: Task<Int>) -> Int { return await t + await t; }",
    );
    let mut i = Interpreter::new(&p, NoHost);
    let a = task(&mut i, "work", vec![]);
    let b = task(&mut i, "twice", vec![Value::Task(a)]);
    assert_eq!(i.drive_task(b).unwrap().to_string(), "Int(42)");
    assert!(Value::Task(a).equals(&Value::Task(a)).is_err());
    let mut other = Interpreter::new(&p, NoHost);
    assert_eq!(other.drive_task(a).unwrap_err().kind, ErrorKind::Invariant);
    let foreign = task(&mut other, "twice", vec![Value::Task(a)]);
    assert_eq!(
        other.drive_task(foreign).unwrap_err().kind,
        ErrorKind::Invariant
    );
}

#[test]
fn cleanup_failure_is_a_fault_and_remaining_cleanup_is_attempted() {
    struct Failing(Vec<String>);
    impl Host for Failing {
        fn call_native(
            &mut self,
            _: SymbolId,
            _: &BTreeMap<ParamId, Type>,
            args: &[Value],
        ) -> Option<Result<Value, String>> {
            self.0.extend(args.iter().map(ToString::to_string));
            Some(Err("cleanup host failure".into()))
        }
    }
    let p = lower(
        "native fn fail(n: Int) -> Unit; async fn leaf() -> Unit {} async fn work() -> Unit { defer fail(1); defer fail(2); await leaf(); }",
    );
    let mut i = Interpreter::new(&p, Failing(vec![]));
    let root = task(&mut i, "work", vec![]);
    i.shared.borrow_mut().ready(root);
    i.step();
    let e = i.shutdown().unwrap_err();
    assert_eq!(e.kind, ErrorKind::Host);
    assert_eq!(i.task_state(root), Some(TaskState::Faulted));
    assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
}

#[test]
fn cleanup_deadlock_still_unwinds_remaining_defers() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn work(t: Task<Int>) -> Int { defer record(1); defer { let n = await t; }; return 0; }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let a = task(&mut i, "work", vec![Value::Unit]);
    let b = task(&mut i, "work", vec![Value::Task(a)]);
    set_argument(&mut i, a, b);
    assert_eq!(i.drive_task(a).unwrap_err().kind, ErrorKind::Deadlock);
    assert_eq!(i.shutdown().unwrap_err().kind, ErrorKind::Deadlock);
    assert_eq!(i.host.0, ["Int(1)", "Int(1)"]);
    assert!(i.shared.borrow().tasks.iter().all(|t| t.state.terminal()));
    assert!(i.continuations.iter().all(Option::is_none));
}

#[test]
fn cancellation_observation_faults_waiter_without_fabricating_value() {
    let p = lower(
        "async fn work() -> Int { return 1; } async fn waiter(t: Task<Int>) -> Int { return await t; }",
    );
    let mut i = Interpreter::new(&p, NoHost);
    let child = task(&mut i, "work", vec![]);
    i.cancel(child, false);
    let parent = task(&mut i, "waiter", vec![Value::Task(child)]);
    assert_eq!(
        i.drive_task(parent).unwrap_err().kind,
        ErrorKind::UnspecifiedCancellation
    );
    assert_eq!(i.task_state(child), Some(TaskState::Cancelled));
    assert_eq!(i.task_state(parent), Some(TaskState::Faulted));
}

#[test]
fn cancellation_during_normal_cleanup_does_not_resume_following_statements() {
    let p = lower(
        "native fn record(n: Int) -> Unit; async fn leaf() -> Unit {} async fn work() -> Unit { defer record(1); { defer { await leaf(); record(2); }; } record(99); }",
    );
    let mut i = Interpreter::new(&p, Recorder::default());
    let root = task(&mut i, "work", vec![]);
    i.shared.borrow_mut().ready(root);
    i.step(); // suspended in nested normal-scope cleanup
    assert!(matches!(i.task_state(root), Some(TaskState::Waiting(_))));
    i.cancel(root, false);
    while i.step() {}
    assert_eq!(i.task_state(root), Some(TaskState::Cancelled));
    assert_eq!(i.host.0, ["Int(2)", "Int(1)"]);
}
