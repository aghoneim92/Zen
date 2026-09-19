use std::{fs, path::PathBuf};
use zen_diagnostics::FileId;
use zen_hir::*;
use zen_semantics::{ModuleInput, check};

fn checked(source: &str) -> zen_semantics::CheckedProgram {
    let (syntax, diagnostics) = zen_syntax::parse(FileId(0), source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let checked = check(vec![ModuleInput {
        name: "test".into(),
        file: FileId(0),
        syntax,
    }]);
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    checked
}
fn lower(source: &str) -> Program {
    lower_program(&checked(source)).unwrap()
}

#[test]
fn compiler_corpus() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/valid");
    let mut paths = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "zen"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let checked = checked(&source);
        let hir = lower_program(&checked).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            hir.dump(),
            lower_program(&checked).unwrap().dump(),
            "{}",
            path.display()
        );
        for e in &hir.expressions {
            assert_eq!(
                e.id.0,
                hir.expressions.iter().position(|x| x.id == e.id).unwrap()
            );
            assert_eq!(e.span.file, FileId(0));
            assert!(e.span.end <= source.len());
        }
    }
}
#[test]
fn invalid_programs_stop() {
    let mut db = zen_semantics::analysis::Database::default();
    for source in ["fn f() -> Int { return true; }", "fn f( {"] {
        db.set_file("test.zen".into(), "test".into(), source.into());
        assert_eq!(
            lower_program(&db.analyze()).unwrap_err(),
            LoweringError::InvalidProgram
        );
    }
    let mut c = checked("fn f() -> Int { return 1; }");
    c.expressions.clear();
    assert!(matches!(
        lower_program(&c),
        Err(LoweringError::Internal { .. })
    ));
}

fn fixture(name: &str) -> Program {
    lower(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(format!("{name}.zen")),
        )
        .unwrap(),
    )
}
fn call_name(p: &Program, id: ExprId) -> &str {
    let ExprKind::Call {
        callee: Callee::Resolved(c),
        ..
    } = &p.expression(id).kind
    else {
        panic!("expected direct call")
    };
    let symbol = match c.target {
        CallTarget::Function(s) | CallTarget::Native(s) => s,
        _ => panic!("expected function"),
    };
    &p.functions.iter().find(|f| f.id == symbol).unwrap().name
}
#[test]
fn source_order_and_defaults() {
    let p = fixture("ordering");
    let fields = p
        .expressions
        .iter()
        .find_map(|e| {
            if let ExprKind::Struct { fields, .. } = &e.kind {
                Some(fields)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        fields.iter().map(|(f, _)| f.index).collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(call_name(&p, fields[0].1), "two");
    assert_eq!(call_name(&p, fields[1].1), "one");
    let fields = p
        .expressions
        .iter()
        .find_map(|e| {
            if let ExprKind::Update { fields, .. } = &e.kind {
                Some(fields)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        fields.iter().map(|(f, _)| f.index).collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(call_name(&p, fields[0].1), "one");
    assert_eq!(call_name(&p, fields[1].1), "two");
    let combine = p.functions.iter().find(|f| f.name == "combine").unwrap();
    assert!(combine.parameters[2].default.is_some());
    let (args, defaults) = p
        .expressions
        .iter()
        .find_map(|e| match &e.kind {
            ExprKind::Call {
                callee: Callee::Resolved(c),
                arguments,
                defaults,
            } if c.target == CallTarget::Function(combine.id) => Some((arguments, defaults)),
            _ => None,
        })
        .unwrap();
    assert_eq!(args.iter().map(|a| a.parameter).collect::<Vec<_>>(), [1, 0]);
    assert_eq!(defaults, &[2]);
    assert_eq!(call_name(&p, args[0].value), "two");
    assert!(matches!(
        p.expression(args[1].value).kind,
        ExprKind::Field {
            field: FieldId { index: 0, .. },
            ..
        }
    ));
    let list = p
        .expressions
        .iter()
        .find_map(|e| {
            if let ExprKind::List { items, .. } = &e.kind {
                Some(items)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        list.iter().map(|i| call_name(&p, *i)).collect::<Vec<_>>(),
        ["two", "one"]
    );
}
#[test]
fn captures_are_transitive_and_use_identity() {
    let p = fixture("closures");
    let outer_value = p
        .functions
        .iter()
        .find(|f| f.name == "example")
        .unwrap()
        .parameters[0]
        .local;
    let other_value = p
        .functions
        .iter()
        .find(|f| f.name == "another")
        .unwrap()
        .parameters[0]
        .local;
    assert_ne!(outer_value, other_value);
    let inside = p.locals.values().find(|l| l.name == "inside").unwrap().id;
    let lambdas = p
        .expressions
        .iter()
        .filter_map(|e| {
            if let ExprKind::Lambda { id, captures, .. } = &e.kind {
                Some((id.0, captures))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 2);
    let outer = lambdas.iter().find(|(id, _)| *id == 0).unwrap().1;
    let inner = lambdas.iter().find(|(id, _)| *id == 1).unwrap().1;
    assert_eq!(
        outer.iter().map(|c| c.local).collect::<Vec<_>>(),
        [outer_value]
    );
    assert!(inner.iter().any(|c| c.local == outer_value));
    assert!(inner.iter().any(|c| c.local == inside));
    assert!(p.expressions.iter().any(|e| matches!(
        e.kind,
        ExprKind::Call {
            callee: Callee::Value(_),
            ..
        }
    )));
}
#[test]
fn generic_and_method_resolution() {
    let p = fixture("methods");
    let calls = p
        .expressions
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::Call {
                callee: Callee::Resolved(c),
                ..
            }
            | ExprKind::Callable(c) => Some(c),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(calls.iter().any(|c| matches!(c.target,CallTarget::Requirement { receiver:Type::Param(_), interface:Type::Nominal(_,ref args),.. } if args == &[Type::Int])));
    assert!(
        calls
            .iter()
            .any(|c| matches!(c.target, CallTarget::Implementation { .. }))
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c.target, CallTarget::Inherent(_))
                && c.receiver.is_some()
                && c.type_arguments.iter().any(|(_, t)| *t == Type::Int))
    );
    let identity = p
        .functions
        .iter()
        .find(|f| f.name == "identity" && f.parameters.len() == 1)
        .unwrap();
    assert!(
        calls
            .iter()
            .any(|c| c.target == CallTarget::Function(identity.id)
                && c.type_arguments == vec![(identity.generics[0].id, Type::Int)])
    );
    let generic = p.functions.iter().find(|f| f.name == "generic").unwrap();
    assert_eq!(generic.generics[0].bounds.len(), 1);
    assert!(
        p.implementations
            .iter()
            .any(|i| i.interface.is_some() && i.methods.iter().all(|(_, r)| r.is_some()))
    );
}
#[test]
fn propagation_await_and_control_flow() {
    let p = fixture("control");
    let load = p.functions.iter().find(|f| f.name == "load").unwrap();
    assert!(load.asynchronous);
    assert!(matches!(load.body, Body::Native));
    assert!(matches!(
        load.completion,
        Type::Nominal(zen_semantics::types::RESULT, _)
    ));
    let propagation = p
        .expressions
        .iter()
        .find(|e| {
            matches!(
                e.kind,
                ExprKind::Propagate {
                    kind: Propagation::Result { .. },
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(propagation.ty, Type::Int);
    let ExprKind::Propagate {
        operand,
        kind: Propagation::Result { error },
        ..
    } = &propagation.kind
    else {
        unreachable!()
    };
    assert_eq!(*error, Type::String);
    let ExprKind::Await { task } = p.expression(*operand).kind else {
        panic!("propagation must wrap await")
    };
    assert_eq!(
        p.expression(task).ty,
        zen_semantics::types::task(load.completion.clone())
    );
    assert!(p.expressions.iter().any(|e| matches!(
        e.kind,
        ExprKind::Propagate {
            kind: Propagation::Option,
            ..
        }
    )));
    let mut loop_ids = vec![];
    let mut exits = vec![];
    for e in &p.expressions {
        if let ExprKind::Block {
            scope, statements, ..
        } = &e.kind
        {
            for s in statements {
                match &s.kind {
                    StmtKind::For {
                        id,
                        iteration: Iteration::List,
                        element: Type::Int,
                        ..
                    }
                    | StmtKind::While { id, .. } => loop_ids.push(*id),
                    StmtKind::Break(id) | StmtKind::Continue(id) => exits.push(*id),
                    StmtKind::Defer {
                        scope: owner,
                        value,
                    } => {
                        assert_eq!(scope, owner);
                        assert_eq!(p.expression(*value).ty, Type::Unit);
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(loop_ids.len(), 2);
    assert!(exits.iter().all(|id| loop_ids.contains(id)));
    assert!(loop_ids.iter().all(|id| exits.contains(id)));
    let arms = p
        .expressions
        .iter()
        .find_map(|e| {
            if let ExprKind::Match { arms, .. } = &e.kind {
                Some(arms)
            } else {
                None
            }
        })
        .unwrap();
    let PatternKind::Variant(variant, children) = &arms[0].0.kind else {
        panic!()
    };
    assert_eq!(
        *variant,
        VariantId {
            owner: zen_semantics::types::RESULT,
            index: 0
        }
    );
    let PatternKind::Bind(local) = children[0].kind else {
        panic!()
    };
    assert_eq!(children[0].ty, Type::Int);
    assert!(matches!(p.expression(arms[0].1).kind,ExprKind::Local(id) if id == local));
}
#[test]
fn decoded_literals_and_constants() {
    let p = lower(
        r#"const huge: Int = 123_456_789_012_345_678_901_234_567_890; fn f() -> Unit { let a = -128i8; let b = 1.25f32; let c = 2.5f64; let s = "a\n\u{1F600}"; let ch = '\u{1F600}'; let yes = true; let nothing = unit; }"#,
    );
    assert_eq!(p.constants.len(), 1);
    assert!(p.expressions.iter().any(|e| matches!(&e.kind,ExprKind::Literal(Literal::Integer { magnitude,negative:false }) if magnitude == "123456789012345678901234567890")));
    assert!(p.expressions.iter().any(|e| e.ty == Type::I8 && matches!(&e.kind,ExprKind::Literal(Literal::Integer { magnitude,negative:true }) if magnitude == "128")));
    assert!(
        p.expressions
            .iter()
            .any(|e| matches!(&e.kind,ExprKind::Literal(Literal::String(s)) if s == "a\n😀"))
    );
    assert!(
        p.expressions
            .iter()
            .any(|e| matches!(e.kind, ExprKind::Literal(Literal::Char('😀'))))
    );
    assert!(
        p.expressions
            .iter()
            .any(|e| matches!(e.kind,ExprKind::Literal(Literal::Float32(v)) if v == 1.25))
    );
}
#[test]
fn imports_and_snapshot_ownership() {
    let mut db = zen_semantics::analysis::Database::default();
    db.set_file(
        "lib.zen".into(),
        "lib".into(),
        "public fn identity<T>(x: T) -> T { return x; }".into(),
    );
    db.set_file(
        "main.zen".into(),
        "main".into(),
        "import lib.identity as renamed; fn f() -> Int { return renamed(1); }".into(),
    );
    let p = {
        let c = db.analyze();
        lower_program(&c).unwrap()
    };
    let function = p.functions.iter().find(|f| f.name == "identity").unwrap();
    assert!(p.expressions.iter().any(|e| matches!(&e.kind,ExprKind::Call { callee:Callee::Resolved(c),.. } if c.target == CallTarget::Function(function.id) && c.type_arguments[0].1 == Type::Int)));
    assert_eq!(p.modules.len(), 2);
}
#[test]
fn golden_dumps() {
    for name in ["ordering", "control", "closures", "methods"] {
        let p = fixture(name);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("{name}.hir"));
        if std::env::var_os("ZEN_UPDATE_HIR").is_some() {
            fs::write(&path, p.dump()).unwrap();
        }
        assert_eq!(p.dump(), fs::read_to_string(path).unwrap());
        assert_eq!(p.dump(), fixture(name).dump());
    }
}

#[test]
fn all_supported_operator_families() {
    for (name, ty) in [
        ("Int", Type::Int),
        ("Float", Type::Float),
        ("F32", Type::F32),
        ("I8", Type::I8),
        ("I16", Type::I16),
        ("I32", Type::I32),
        ("I64", Type::I64),
        ("U8", Type::U8),
        ("U16", Type::U16),
        ("U32", Type::U32),
        ("U64", Type::U64),
        ("Bool", Type::Bool),
        ("String", Type::String),
        ("Char", Type::Char),
    ] {
        let mut operations = vec![
            ("==", BinaryOp::Equal, "Bool"),
            ("!=", BinaryOp::NotEqual, "Bool"),
        ];
        if ty.is_numeric() {
            operations.extend([
                ("<", BinaryOp::Less, "Bool"),
                ("<=", BinaryOp::LessEqual, "Bool"),
                (">", BinaryOp::Greater, "Bool"),
                (">=", BinaryOp::GreaterEqual, "Bool"),
            ]);
        }
        if matches!(ty, Type::Int | Type::Float | Type::F32) {
            operations.extend([
                ("+", BinaryOp::Add, name),
                ("-", BinaryOp::Subtract, name),
                ("*", BinaryOp::Multiply, name),
            ]);
        }
        if ty == Type::String {
            operations.push(("+", BinaryOp::Add, name));
        }
        for (text, expected, result) in operations {
            let p = lower(&format!(
                "fn f(a: {name}, b: {name}) -> {result} {{ return a {text} b; }}"
            ));
            assert!(p.expressions.iter().any(|e| matches!(&e.kind,ExprKind::Binary { op,operand_type,.. } if *op == expected && *operand_type == ty)));
        }
        if matches!(ty, Type::Int | Type::Float | Type::F32 | Type::Bool) {
            let (text, expected) = if ty == Type::Bool {
                ("!", UnaryOp::Not)
            } else {
                ("-", UnaryOp::Negate)
            };
            let p = lower(&format!("fn f(a: {name}) -> {name} {{ return {text}a; }}"));
            assert!(p.expressions.iter().any(|e| matches!(&e.kind,ExprKind::Unary { op,operand_type,.. } if *op == expected && *operand_type == ty)));
        }
    }
    for (op, expected) in [("&&", LogicalOp::And), ("||", LogicalOp::Or)] {
        let p = lower(&format!(
            "native fn a() -> Bool; native fn b() -> Bool; fn f() -> Bool {{ return a() {op} b(); }}"
        ));
        let (left, right) = p
            .expressions
            .iter()
            .find_map(|e| match e.kind {
                ExprKind::Logical { op, left, right } if op == expected => Some((left, right)),
                _ => None,
            })
            .unwrap();
        assert_eq!(call_name(&p, left), "a");
        assert_eq!(call_name(&p, right), "b");
    }
}
#[test]
fn intrinsics_iteration_and_bound_values() {
    let p = lower(
        "fn f(items: List<Int>, map: Map<String, Int>, set: Set<Int>) -> Unit { let get: (Int) -> Option<Int> = items.get; let a = get(0); let b = items.first(); let c = items.append(1); let d = map.get(\"key\"); for item in set { let n = item; } }",
    );
    for intrinsic in [
        Intrinsic::ListGet,
        Intrinsic::ListFirst,
        Intrinsic::ListAppend,
        Intrinsic::MapGet,
    ] {
        assert!(p.expressions.iter().any(|e| match &e.kind {
            ExprKind::Callable(c)
            | ExprKind::Call {
                callee: Callee::Resolved(c),
                ..
            } => c.target == CallTarget::Intrinsic(intrinsic),
            _ => false,
        }));
    }
    assert!(p.expressions.iter().any(|e| matches!(&e.kind,ExprKind::Block { statements,.. } if statements.iter().any(|s| matches!(s.kind,StmtKind::For { iteration:Iteration::Set,.. })))));
}
#[test]
fn constructor_identity_and_origins() {
    let source = "fn f() -> Unit { let a: Option<Int> = .some(1); let b = Option<Int>.some(2); }";
    let p = lower(source);
    let constructors = p
        .expressions
        .iter()
        .filter(|e| matches!(e.kind, ExprKind::Variant { .. }))
        .collect::<Vec<_>>();
    assert_eq!(constructors.len(), 2);
    for (e, spelling) in constructors.iter().zip([".some(1)", "Option<Int>.some(2)"]) {
        assert_eq!(&source[e.span.start..e.span.end], spelling);
        assert!(
            matches!(&e.kind,ExprKind::Variant { variant:VariantId { owner:zen_semantics::types::OPTION,index:0 },type_arguments,.. } if type_arguments == &[Type::Int])
        );
    }
    let mut c = checked("struct S { n: Int; } fn f(s: S) -> Int { return s.n; }");
    c.resolved.fields.clear();
    assert!(
        matches!(lower_program(&c),Err(LoweringError::Internal { message,.. }) if message.contains("FieldId"))
    );
}
#[test]
fn nested_lambda_parameters_are_not_outer_captures() {
    let p = lower(
        "fn f(x: Int) -> () -> (Int) -> Int { return () -> (Int) -> Int { return (y: Int) -> Int { let z = y + x; return z; }; }; }",
    );
    let x = p.functions[0].parameters[0].local;
    for e in &p.expressions {
        if let ExprKind::Lambda { captures, .. } = &e.kind {
            assert_eq!(captures.iter().map(|c| c.local).collect::<Vec<_>>(), [x]);
        }
    }
}
#[test]
fn never_operands_keep_divergence() {
    let p = lower(
        "native fn stop() -> Never; async fn f() -> Unit { let a = await stop(); let b = stop()?; stop()(unknown); }",
    );
    assert_eq!(
        p.expressions
            .iter()
            .filter(|e| matches!(e.kind, ExprKind::Diverge { .. }))
            .count(),
        3
    );
}
