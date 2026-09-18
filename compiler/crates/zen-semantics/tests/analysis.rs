use zen_diagnostics::FileId;
use zen_semantics::{analysis::*, types::Type};
fn db(text: &str) -> Database {
    let mut db = Database::default();
    db.set_file("test.zen".into(), "test".into(), text.into());
    db
}
#[test]
fn navigation_and_inference() {
    let text = "struct User { name: String; } fn f(user: User) -> Unit { let name = user.name; } fn g() -> Unit { let name = 1; }";
    let p = db(text).analyze();
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    let e = p
        .symbol_at(FileId(0), text.find("user.name").unwrap() + 5)
        .unwrap();
    assert_eq!(p.declaration(&e).unwrap().kind, Kind::Field);
    assert_eq!(
        p.declaration(&e).unwrap().selection.start,
        text.find("name:").unwrap()
    );
    let e = p
        .symbol_at(FileId(0), text.find("let name").unwrap() + 4)
        .unwrap();
    assert_eq!(p.references(&e, true).len(), 1);
    assert_eq!(p.declaration(&e).unwrap().ty, Type::String);
}
#[test]
fn exact_rename_and_shadowing() {
    let text = "fn f(x: Int) -> Int { let y = x; return y; } fn g(x: Int) -> Int { return x; }";
    let db = db(text);
    let p = db.analyze();
    let e = p
        .symbol_at(FileId(0), text.find("let y").unwrap() + 4)
        .unwrap();
    assert_eq!(db.rename(&p, &e, "value").unwrap().len(), 2);
    assert!(db.rename(&p, &e, "x").is_err());
    assert!(db.rename(&p, &e, "return").is_err());
}
#[test]
fn member_completion() {
    let text = "struct User { name: String; } fn f(user: User) -> Unit { user.; }";
    let db = db(text);
    let cs = db.complete(FileId(0), text.find("user.;").unwrap() + 5);
    assert!(cs.iter().any(|c| c.name == "name"), "{cs:?}");
}
#[test]
fn unwrapped_member_completion_before_another_statement() {
    let models = include_str!("../../../../examples/editor/src/models.zen");
    let source = include_str!("../../../../examples/editor/src/main.zen");
    for suffix in ["", "na"] {
        let text = source.replace(
            "    let status: State",
            &format!("    user.{suffix}\n    let status: State"),
        );
        let mut database = Database::default();
        database.set_file("models.zen".into(), "models".into(), models.into());
        let file = database.set_file("main.zen".into(), "main".into(), text.clone());
        let offset = text.find(&format!("user.{suffix}\n")).unwrap() + 5 + suffix.len();
        let cs = database.complete(file, offset);
        assert!(cs.iter().any(|c| c.name == "name"), "{cs:?}");
        if suffix.is_empty() {
            assert!(cs.iter().any(|c| c.name == "describe"), "{cs:?}");
        }
        assert!(
            !cs.iter().any(|c| c.name == "ok" || c.name == "err"),
            "{cs:?}"
        );
    }
}
#[test]
fn missing_receiver_does_not_use_function_return_type() {
    let text = "fn f() -> Result<Unit, String> { missing.\n return .ok(unit); }";
    let cs = db(text).complete(FileId(0), text.find("missing.").unwrap() + 8);
    assert!(cs.is_empty(), "{cs:?}");
}
#[test]
fn enum_completion() {
    let text = "enum State { idle; ready; } fn f() -> State { return .; }";
    let cs = db(text).complete(FileId(0), text.find(".;").unwrap() + 1);
    assert!(cs.iter().any(|c| c.name == "ready"), "{cs:?}");
}
#[test]
fn match_completion() {
    let text =
        "enum State { idle; ready; } fn f(s: State) -> Unit { match s { .idle => unit; . } }";
    let cs = db(text).complete(FileId(0), text.find(". }").unwrap() + 1);
    assert_eq!(cs.first().map(|c| c.name.as_str()), Some("ready"), "{cs:?}");
}
#[test]
fn await_and_propagation() {
    let text = "struct User { name: String; } native async fn load() -> User; native fn get() -> Option<User>; async fn f() -> Option<Unit> { let a = await load(); let b = get()?; return .some(unit); }";
    let p = db(text).analyze();
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    for n in ["a", "b"] {
        let e = p
            .symbol_at(FileId(0), text.find(&format!("let {n}")).unwrap() + 4)
            .unwrap();
        assert_eq!(p.display_type(&p.declaration(&e).unwrap().ty), "User");
    }
}
#[test]
fn scope_does_not_leak() {
    let text = "fn f(x: Int) -> Unit { if true { let inside = x; } let outside = x; }";
    let p = db(text).analyze();
    let names = p
        .visible_symbols(FileId(0), text.find("let outside").unwrap())
        .into_iter()
        .map(|c| c.name)
        .collect::<Vec<_>>();
    assert!(names.contains(&"x".into()));
    assert!(!names.contains(&"inside".into()), "{names:?}");
}

#[test]
fn editing_every_boundary_does_not_panic() {
    let source = "struct User<T> { name: T; } enum State { idle; ready(Int); } interface Show { fn show(self) -> String; } impl Show for User<String> { fn show(self) -> String { return self.name; } } fn f(user: User<String>) -> Unit { let list = [1,2]; let v = list.get(0); match v { .some(x) => unit; .none => unit; } }";
    for i in 0..source.len() {
        let database = db(&source[..i]);
        let _ = database.analyze();
        let _ = database.complete(FileId(0), i);
    }
}
#[test]
fn type_and_named_argument_completion() {
    let text = "fn f() -> Unit { let n: In = 1; }";
    assert!(
        db(text)
            .complete(FileId(0), text.find("In =").unwrap() + 2)
            .iter()
            .any(|c| c.name == "Int")
    );
    let text = "native fn request(url: String, timeout: Int) -> Unit; fn f() -> Unit { request(url: \"x\", ti); }";
    let c = db(text).complete(FileId(0), text.find("ti);").unwrap() + 2);
    assert!(c.iter().any(|c| c.name == "timeout: "), "{c:?}");
}
#[test]
fn capture_is_rejected_even_when_types_match() {
    let text = "const outside: Int = 1; fn f() -> Int { let local = 2; return outside + local; }";
    let database = db(text);
    let program = database.analyze();
    let e = program
        .symbol_at(FileId(0), text.find("let local").unwrap() + 4)
        .unwrap();
    assert!(database.rename(&program, &e, "outside").is_err());
}
#[test]
fn match_fix_is_compiler_owned() {
    let text =
        "enum State { idle; ready(Int); } fn f(s: State) -> Unit { match s { .idle => unit; } }";
    let database = db(text);
    let program = database.analyze();
    let d = program
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "ZEN-MATCH-0001")
        .unwrap();
    let (span, replacement) = d.fixes[0].replacement.as_ref().unwrap();
    assert!(replacement.contains(".ready(_)"));
    let mut updated = text.to_owned();
    updated.replace_range(span.start..span.end, replacement);
    assert!(db(&updated).analyze().diagnostics.is_empty());
}
#[test]
fn cross_module_definition_and_visibility() {
    let mut database = Database::default();
    database.set_file(
        "models.zen".into(),
        "models".into(),
        "public struct User { public name: String; secret: Int; }".into(),
    );
    let text = "import models.User; fn f(user: User) -> Unit { user.; }";
    let file = database.set_file("main.zen".into(), "main".into(), text.into());
    let p = database.analyze();
    let e = p
        .symbol_at(file, text.find("user: User").unwrap() + 6)
        .unwrap();
    assert_eq!(p.declaration(&e).unwrap().selection.file, FileId(0));
    let names = database
        .complete(file, text.find("user.;").unwrap() + 5)
        .into_iter()
        .map(|c| c.name)
        .collect::<Vec<_>>();
    assert!(names.contains(&"name".into()));
    assert!(!names.contains(&"secret".into()));
}
#[test]
fn constrained_and_builtin_members_use_the_resolver() {
    let text =
        "interface Show { fn show(self) -> String; } fn f<T: Show>(value: T) -> Unit { value.; }";
    let cs = db(text).complete(FileId(0), text.find("value.;").unwrap() + 6);
    assert!(cs.iter().any(|c| c.name == "show"), "{cs:?}");
    let text = "fn f(items: List<Int>) -> Unit { items.; }";
    let cs = db(text).complete(FileId(0), text.find("items.;").unwrap() + 6);
    assert!(cs.iter().any(|c| c.name == "get"), "{cs:?}");
}
#[test]
fn named_argument_labels_rename_with_parameters() {
    let text = "fn f(input: Int) -> Int { return input; } fn g() -> Int { return f(input: 1); }";
    let database = db(text);
    let p = database.analyze();
    let e = p
        .symbol_at(FileId(0), text.find("input:").unwrap())
        .unwrap();
    assert_eq!(database.rename(&p, &e, "value").unwrap().len(), 3);
}
#[test]
fn interface_method_rename_includes_implementations() {
    let text = "struct S {} interface I { fn name(self) -> Int; } impl I for S { fn name(self) -> Int { return 1; } } fn f(s: S) -> Int { return s.name(); }";
    let database = db(text);
    let p = database.analyze();
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    let e = p
        .symbol_at(FileId(0), text.find("fn name").unwrap() + 3)
        .unwrap();
    assert_eq!(database.rename(&p, &e, "value").unwrap().len(), 3);
}
#[test]
fn argument_colon_is_an_expression_context() {
    let text = "native fn f(value: Int) -> Unit; fn g() -> Unit { let number = 1; f(value: num); }";
    let c = db(text).complete(FileId(0), text.find("num);").unwrap() + 3);
    assert!(c.iter().any(|c| c.name == "number"), "{c:?}");
}
#[test]
fn generic_parameters_are_visible_in_type_positions() {
    let text = "fn f<T>(value: Tt) -> Unit {}";
    let cs = db(text).complete(FileId(0), text.find("Tt").unwrap() + 1);
    assert!(cs.iter().any(|c| c.name == "T"), "{cs:?}");
}
#[test]
fn arbitrary_broken_programs_are_safe_to_analyze() {
    let words = [
        "fn",
        "f",
        "(",
        ")",
        "{",
        "}",
        "->",
        "Int",
        "Unit",
        ";",
        "let",
        "=",
        "1",
        ".",
        "?",
        "<",
        ">",
        "return",
        "impl",
        "match",
        "_",
        ",",
        "async",
        "&",
        "struct",
        "enum",
        "interface",
        "self",
    ];
    let mut seed = 342u64;
    for _ in 0..600 {
        let mut text = String::new();
        for _ in 0..40 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(words[(seed >> 32) as usize % words.len()]);
            text.push(' ');
        }
        let _ = db(&text).analyze();
    }
}
