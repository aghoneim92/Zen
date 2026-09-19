use std::collections::BTreeMap;
use zen_diagnostics::{FileId, SourceMap};
use zen_hir::*;
use zen_interpreter::*;

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
fn function(program: &Program, name: &str) -> SymbolId {
    program
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .id
}
fn error(source: &str) -> InterpreterError {
    Interpreter::new(&lower(source), NoHost)
        .run_main(FileId(0))
        .unwrap_err()
}
#[test]
fn entry_rules_and_unused_native_declarations() {
    assert_eq!(
        error("fn other() -> Unit {}").kind,
        ErrorKind::MissingEntryPoint
    );
    assert_eq!(
        error("struct S {} impl S { fn main() -> Unit {} }").kind,
        ErrorKind::MissingEntryPoint
    );
    assert_eq!(
        error("fn main(x: Int = 1) -> Unit {}").kind,
        ErrorKind::InvalidEntryPoint
    );
    assert_eq!(
        error("fn main<T>() -> Unit {}").kind,
        ErrorKind::InvalidEntryPoint
    );
    assert_eq!(
        error("fn main() -> Float { return 1.0; }").kind,
        ErrorKind::InvalidEntryPoint
    );
    assert_eq!(
        error("async fn work() -> Unit {} fn main() -> Task<Unit> { return work(); }").kind,
        ErrorKind::InvalidEntryPoint
    );
    for source in [
        "async fn main() -> Unit {}",
        "async fn task() -> Unit {} fn main() -> Unit { task(); }",
        "fn main() -> Unit { let f = async () -> Unit {}; f(); }",
    ] {
        let p = lower(source);
        assert!(Interpreter::new(&p, NoHost).run_main(FileId(0)).is_ok());
    }
    let p = lower(
        "async fn task() -> Unit {} native fn missing() -> Unit; fn main() -> Int { if false { missing(); task(); } return 123; }",
    );
    assert_eq!(
        Interpreter::new(&p, NoHost)
            .run_main(FileId(0))
            .unwrap()
            .to_string(),
        "Int(123)"
    );
}
#[test]
fn native_stack_retains_source_and_error_identity() {
    let source = "native fn missing() -> Unit; fn middle() -> Unit { missing(); } fn main() -> Unit { middle(); }";
    let p = lower(source);
    let e = Interpreter::new(&p, NoHost)
        .run_main(FileId(0))
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnboundNative(function(&p, "missing")));
    assert_eq!(
        e.stack.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["main", "middle", "missing"]
    );
    assert_eq!(
        &source[e.span.unwrap().start..e.span.unwrap().end],
        "missing()"
    );
    let mut sources = SourceMap::default();
    sources.add("main.zen", source);
    let rendered = e.render(&sources);
    assert!(
        rendered.contains("main.zen:1:")
            && rendered.contains("in missing")
            && rendered.contains("in main")
    );
}
#[derive(Default)]
struct RecordingHost {
    bindings: BTreeMap<SymbolId, bool>,
    events: Vec<String>,
}
impl Host for RecordingHost {
    fn call_native(
        &mut self,
        id: SymbolId,
        _: &BTreeMap<ParamId, Type>,
        args: &[Value],
    ) -> Option<Result<Value, String>> {
        let fail = *self.bindings.get(&id)?;
        self.events.extend(args.iter().map(ToString::to_string));
        Some(if fail {
            Err("test host failure".into())
        } else {
            Ok(Value::Unit)
        })
    }
}
#[test]
fn errors_attempt_all_cleanup_keep_first_error_and_allow_reuse() {
    let p = lower(
        "native fn record(x: Int) -> Unit; native fn fail(x: Int) -> Unit; fn main() -> Unit { defer record(1); defer fail(2); fail(3); } fn okay() -> Int { return 4; }",
    );
    let host = RecordingHost {
        bindings: [
            (function(&p, "record"), false),
            (function(&p, "fail"), true),
        ]
        .into(),
        events: vec![],
    };
    let mut interpreter = Interpreter::new(&p, host);
    let e = interpreter.run_main(FileId(0)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Host);
    assert_eq!(interpreter.host.events, ["Int(3)", "Int(2)", "Int(1)"]);
    assert_eq!(e.span.unwrap().start, p.expressions.iter().find(|e| matches!(&e.kind, ExprKind::Call { arguments, .. } if arguments.first().is_some_and(|a| matches!(&p.expression(a.value).kind, ExprKind::Literal(Literal::Integer { magnitude, .. }) if magnitude == "3")))).unwrap().span.start);
    assert_eq!(
        interpreter
            .run_function(function(&p, "okay"), vec![])
            .unwrap()
            .to_string(),
        "Int(4)"
    );
}
#[test]
fn interface_requirement_defaults_and_generic_closure_witnesses() {
    let p = lower(
        "interface Add<T> { fn add(self, a: T, b: T = a) -> T; } struct S {} impl Add<Int> for S { fn add(self, a: Int, b: Int) -> Int { return a + b; } } fn make<T: Add<Int>>(x: T) -> () -> Int { return () -> Int { return x.add(3); }; } fn main() -> Int { let f = make(S {}); return f(); }",
    );
    assert_eq!(
        Interpreter::new(&p, NoHost)
            .run_main(FileId(0))
            .unwrap()
            .to_string(),
        "Int(6)"
    );
}
#[test]
fn direct_embedding_generics_map_and_set_intrinsics() {
    let p = lower(
        "fn identity<T>(x: T) -> T { return x; } fn lookup(m: Map<String, Int>, key: String) -> Option<Int> { return m.get(key); } fn total(s: Set<Int>) -> Int { var n = 0; for x in s { n = n * 10 + x; } return n; }",
    );
    let mut interpreter = Interpreter::new(&p, NoHost);
    let id = function(&p, "identity");
    let param = p.functions.iter().find(|f| f.id == id).unwrap().generics[0].id;
    let value = interpreter
        .run_function_with_types(
            id,
            [(param, Type::String)].into(),
            vec![Value::String("zen".into())],
        )
        .unwrap();
    assert_eq!(value.to_string(), "String(\"zen\")");
    let map = Value::Map(vec![(Value::String("key".into()), Value::Int(42.into()))].into());
    let found = interpreter
        .run_function(
            function(&p, "lookup"),
            vec![map.clone(), Value::String("key".into())],
        )
        .unwrap();
    assert!(matches!(
        found,
        Value::Enum {
            variant: VariantId {
                owner: OPTION,
                index: 0
            },
            ..
        }
    ));
    let absent = interpreter
        .run_function(
            function(&p, "lookup"),
            vec![map, Value::String("absent".into())],
        )
        .unwrap();
    assert!(matches!(
        absent,
        Value::Enum {
            variant: VariantId {
                owner: OPTION,
                index: 1
            },
            ..
        }
    ));
    let set = Value::Set(
        vec![
            Value::Int(3.into()),
            Value::Int(1.into()),
            Value::Int(2.into()),
        ]
        .into(),
    );
    assert_eq!(
        interpreter
            .run_function(function(&p, "total"), vec![set])
            .unwrap()
            .to_string(),
        "Int(312)"
    );
}
#[test]
fn malformed_hir_reports_invariants_and_constant_cycles() {
    let mut p = lower("fn main() -> Int { var n = 1; n = 2; return n; }");
    p.locals
        .values_mut()
        .find(|l| l.name == "n")
        .unwrap()
        .mutable = false;
    assert_eq!(
        Interpreter::new(&p, NoHost)
            .run_main(FileId(0))
            .unwrap_err()
            .kind,
        ErrorKind::Invariant
    );
    let mut p = lower("fn main() -> Unit { if true {} }");
    let e = p
        .expressions
        .iter_mut()
        .find(|e| matches!(e.kind, ExprKind::Literal(Literal::Bool(_))))
        .unwrap();
    e.kind = ExprKind::Literal(Literal::Unit);
    e.ty = Type::Unit;
    assert_eq!(
        Interpreter::new(&p, NoHost)
            .run_main(FileId(0))
            .unwrap_err()
            .kind,
        ErrorKind::Invariant
    );
    let mut p = lower("const n: Int = 1; fn main() -> Int { return n; }");
    let c = p.constants[0].clone();
    p.expressions[c.value.0].kind = ExprKind::Constant(c.id);
    let mut interpreter = Interpreter::new(&p, NoHost);
    for _ in 0..2 {
        assert_eq!(
            interpreter.run_main(FileId(0)).unwrap_err().kind,
            ErrorKind::ConstantCycle(c.id)
        );
    }
}
#[test]
fn ieee_and_forbidden_equality() {
    assert!(
        !Value::Float(f64::NAN)
            .equals(&Value::Float(f64::NAN))
            .unwrap()
    );
    assert!(Value::Float(-0.0).equals(&Value::Float(0.0)).unwrap());
    assert!(
        Value::List(vec![].into())
            .equals(&Value::List(vec![].into()))
            .is_err()
    );
}
#[test]
fn recursion_guard_is_an_error_not_a_host_panic() {
    let e = error(
        "fn recurse(n: Int) -> Int { return recurse(n + 1); } fn main() -> Int { return recurse(0); }",
    );
    assert_eq!(e.kind, ErrorKind::ResourceLimit);
    assert!(e.stack.len() > 10);
}

#[test]
fn post_resumption_failure_runs_cleanup_and_retains_frames() {
    let p = lower(
        "native fn record(x: Int) -> Unit; native fn fail(x: Int) -> Unit; async fn pause() -> Unit {} async fn work() -> Unit { defer record(1); await pause(); fail(2); } async fn main() -> Unit { defer record(3); await work(); }",
    );
    let host = RecordingHost {
        bindings: [
            (function(&p, "record"), false),
            (function(&p, "fail"), true),
        ]
        .into(),
        events: vec![],
    };
    let mut interpreter = Interpreter::new(&p, host);
    let e = interpreter.run_main(FileId(0)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Host);
    assert_eq!(
        e.stack.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["main", "work", "fail"]
    );
    assert_eq!(interpreter.host.events, ["Int(2)", "Int(1)", "Int(3)"]);
}

#[test]
fn async_native_immediate_completion_uses_resolved_host_bridge() {
    let p = lower(
        "native async fn pause(x: Int) -> Unit; async fn main() -> Unit { await pause(42); }",
    );
    let host = RecordingHost {
        bindings: [(function(&p, "pause"), false)].into(),
        events: vec![],
    };
    let mut interpreter = Interpreter::new(&p, host);
    assert!(matches!(
        interpreter.run_main(FileId(0)).unwrap(),
        Value::Unit
    ));
    assert_eq!(interpreter.host.events, ["Int(42)"]);
}
