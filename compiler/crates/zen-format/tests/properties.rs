use std::{fs, path::Path};
use zen_diagnostics::FileId;
use zen_syntax::{
    ast::*,
    lexer::{TriviaKind, lex},
};
fn name(n: &mut Name) {
    n.span = Default::default();
}
fn ty(t: &mut TypeRef) {
    t.span = Default::default();
    match &mut t.kind {
        TypeKind::Named(_, ts) => ts.iter_mut().for_each(ty),
        TypeKind::Function(ts, r) => {
            ts.iter_mut().for_each(ty);
            ty(r);
        }
    }
}
fn generics(gs: &mut [Generic]) {
    for g in gs {
        name(&mut g.name);
        g.bounds.iter_mut().for_each(ty);
    }
}
fn params(ps: &mut [Param]) {
    for p in ps {
        name(&mut p.name);
        if let Some(t) = &mut p.ty {
            ty(t);
        }
        if let Some(e) = &mut p.default {
            expr(e);
        }
    }
}
fn function(f: &mut Function) {
    name(&mut f.name);
    generics(&mut f.generics);
    params(&mut f.params);
    ty(&mut f.result);
    if let Some(e) = &mut f.body {
        expr(e);
    }
}
fn pattern(p: &mut Pattern) {
    p.span = Default::default();
    match &mut p.kind {
        PatternKind::Bind(n) => name(n),
        PatternKind::Variant(_, n, ps) => {
            name(n);
            ps.iter_mut().for_each(pattern);
        }
        _ => {}
    }
}
fn fields(fs: &mut [(Name, Expr)]) {
    for (n, e) in fs {
        name(n);
        expr(e);
    }
}
fn expr(e: &mut Expr) {
    e.span = Default::default();
    e.id = ExprId(0);
    match &mut e.kind {
        ExprKind::Literal(_) => {}
        ExprKind::Name(n, ts) => {
            name(n);
            ts.iter_mut().for_each(ty);
        }
        ExprKind::Contextual(n) => name(n),
        ExprKind::Unary(_, e) => expr(e),
        ExprKind::Binary(_, a, b) => {
            expr(a);
            expr(b);
        }
        ExprKind::Call(e, args) => {
            expr(e);
            for a in args {
                if let Some(n) = &mut a.label {
                    name(n);
                }
                expr(&mut a.value);
            }
        }
        ExprKind::Member(e, n, ts) => {
            expr(e);
            name(n);
            ts.iter_mut().for_each(ty);
        }
        ExprKind::Struct(t, fs) => {
            ty(t);
            fields(fs);
        }
        ExprKind::Update(e, fs) => {
            expr(e);
            fields(fs);
        }
        ExprKind::List(es) => es.iter_mut().for_each(expr),
        ExprKind::Block(ss, tail) => {
            ss.iter_mut().for_each(stmt);
            if let Some(e) = tail {
                expr(e);
            }
        }
        ExprKind::If(a, b, c) => {
            expr(a);
            expr(b);
            if let Some(e) = c {
                expr(e);
            }
        }
        ExprKind::Match(e, arms) => {
            expr(e);
            for (p, e) in arms {
                pattern(p);
                expr(e);
            }
        }
        ExprKind::Lambda {
            params: ps,
            result,
            body,
            ..
        } => {
            params(ps);
            if let Some(t) = result {
                ty(t);
            }
            expr(body);
        }
    }
}
fn stmt(s: &mut Stmt) {
    s.span = Default::default();
    match &mut s.kind {
        StmtKind::Bind {
            name: n,
            ty: t,
            value,
            ..
        } => {
            name(n);
            if let Some(t) = t {
                ty(t);
            }
            expr(value);
        }
        StmtKind::Assign(a, b) | StmtKind::While(a, b) => {
            expr(a);
            expr(b);
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                expr(e);
            }
        }
        StmtKind::Defer(e) | StmtKind::Expr(e) => expr(e),
        StmtKind::For(n, a, b) => {
            name(n);
            expr(a);
            expr(b);
        }
        StmtKind::Break | StmtKind::Continue => {}
    }
}
fn structure(source: &str) -> String {
    let (mut m, d) = zen_syntax::parse(FileId(0), source);
    assert!(d.is_empty(), "{d:?}\n{source}");
    for i in &mut m.imports {
        i.span = Default::default();
        i.path.iter_mut().for_each(name);
        if let Some(n) = &mut i.alias {
            name(n);
        }
    }
    for d in &mut m.declarations {
        d.span = Default::default();
        match &mut d.kind {
            DeclKind::Struct(n, gs, fs) => {
                name(n);
                generics(gs);
                for f in fs {
                    name(&mut f.name);
                    ty(&mut f.ty);
                }
            }
            DeclKind::Enum(n, gs, vs) => {
                name(n);
                generics(gs);
                for v in vs {
                    name(&mut v.name);
                    v.payload.iter_mut().for_each(ty);
                }
            }
            DeclKind::Interface(n, gs, ms) => {
                name(n);
                generics(gs);
                ms.iter_mut().for_each(function);
            }
            DeclKind::Impl {
                interface,
                target,
                methods,
            } => {
                if let Some(t) = interface {
                    ty(t);
                }
                ty(target);
                methods.iter_mut().for_each(function);
            }
            DeclKind::Function(f) => function(f),
            DeclKind::Const(n, t, e) => {
                name(n);
                ty(t);
                expr(e);
            }
        }
    }
    format!("{m:?}")
}
fn comments(source: &str) -> Vec<String> {
    lex(FileId(0), source)
        .0
        .iter()
        .flat_map(|t| &t.leading)
        .filter(|t| t.kind != TriviaKind::Whitespace)
        .map(|t| {
            source[t.span.start..t.span.end]
                .replace("\r\n", "\n")
                .trim_end()
                .to_owned()
        })
        .collect()
}
fn verify(source: &str) -> String {
    let output = zen_format::format_source(source).unwrap();
    assert_eq!(
        structure(source),
        structure(&output),
        "AST changed:\n{source}\n{output}"
    );
    assert_eq!(
        zen_format::format_source(&output).unwrap(),
        output,
        "not idempotent:\n{source}"
    );
    assert_eq!(comments(source), comments(&output), "comments changed");
    assert!(output.ends_with('\n'));
    assert!(!output.ends_with("\n\n"));
    output
}
#[test]
fn golden_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if !path.to_string_lossy().ends_with(".input.zen") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let output = verify(&source);
        let expected = path.with_file_name(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .replace(".input.", ".expected."),
        );
        assert_eq!(
            output,
            fs::read_to_string(expected).unwrap(),
            "{}",
            path.display()
        );
    }
}
#[test]
fn compiler_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures");
    fn walk(p: &Path) {
        for entry in fs::read_dir(p).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(&p);
            } else if p.extension().is_some_and(|x| x == "zen") {
                let s = fs::read_to_string(&p).unwrap();
                if zen_syntax::parse(FileId(0), &s).1.is_empty() {
                    verify(&s);
                }
            }
        }
    }
    walk(&root);
}
#[test]
fn comments_at_every_token_boundary() {
    for source in [
        include_str!("fixtures/core.input.zen"),
        "fn f<T>(x: T) -> T { return x; }",
        "fn f() -> Unit { let x = S { a: [1, 2], b: 3 }; call(x, 4); }",
    ] {
        let tokens = lex(FileId(0), source).0;
        for token in tokens {
            for comment in [
                " /* Arabic مرحبا 😀 */ ",
                " // line 😀\n",
                "\n\n// first\n// second\n",
            ] {
                let mut s = source.to_owned();
                s.insert_str(token.span.start, comment);
                verify(&s);
            }
        }
    }
}
#[test]
fn width_boundaries() {
    for length in [85, 86, 87, 150] {
        let s = format!("fn f() -> Unit {{ call({}, y); }}", "x".repeat(length));
        let out = verify(&s);
        // 4 indent + call( + identifier + comma/space + y + ); = length + 14
        assert_eq!(out.contains("call(\n"), length + 14 > 100, "{out}");
    }
}
#[test]
fn invalid_sources_and_identity() {
    for s in [
        "fn",
        "fn f() -> Unit {",
        "/*",
        "fn f() -> Unit { let x = ; }",
    ] {
        assert!(
            matches!(zen_format::format_file(FileId(7),s),Err(zen_format::FormatError::Syntax(ds)) if ds.iter().all(|d|d.primary.file==FileId(7)))
        );
    }
    assert_eq!(verify(""), "\n");
    assert_eq!(verify("// only\r\n"), "// only\n");
}
#[test]
fn whitespace_variants_converge() {
    let s = "fn f(a: Int, b: String) -> Unit { let x = [1, 2]; call(a, b); }";
    let canonical = verify(s);
    let tokens = lex(FileId(0), s).0;
    for gap in [" ", "\n\n", "\t", "\r\n"] {
        let variant = tokens
            .iter()
            .map(|t| &s[t.span.start..t.span.end])
            .collect::<Vec<_>>()
            .join(gap);
        assert_eq!(verify(&variant), canonical);
    }
}

#[test]
fn generated_operator_combinations() {
    for left in ["a", "-a", "!a", "(await op())?", "op().member?", "(a + b)"] {
        for op in ["+", "-", "*", "==", "!=", "<", "<=", ">", ">=", "&&", "||"] {
            for right in ["b", "-b", "(b * c)", "await op()?", "thing.member()"] {
                verify(&format!(
                    "async fn f() -> Unit {{ let x = {left} {op} {right}; }}"
                ));
            }
        }
    }
}
