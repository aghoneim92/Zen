use std::{fs, path::PathBuf};
use zen_diagnostics::SourceMap;
use zen_semantics::{ModuleInput, check};
#[test]
fn fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures");
    for kind in ["valid", "invalid"] {
        let mut paths = fs::read_dir(root.join(kind))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "zen"))
            .collect::<Vec<_>>();
        paths.sort();
        assert!(!paths.is_empty());
        for path in paths {
            let source = fs::read_to_string(&path).unwrap();
            let mut sources = SourceMap::default();
            let file = sources.add(path.file_name().unwrap().to_string_lossy(), source.clone());
            let (syntax, mut diagnostics) = zen_syntax::parse(file, &source);
            if diagnostics.is_empty() {
                let checked = check(vec![ModuleInput {
                    name: "fixture".into(),
                    file,
                    syntax,
                }]);
                diagnostics.extend(checked.diagnostics);
                if diagnostics.is_empty() {
                    assert!(
                        checked
                            .expressions
                            .iter()
                            .all(|e| e.ty != zen_semantics::types::Type::Error)
                    );
                }
            }
            let rendered = diagnostics
                .iter()
                .map(|d| d.render(&sources))
                .collect::<String>();
            if kind == "valid" {
                assert!(diagnostics.is_empty(), "{}\n{rendered}", path.display());
            } else {
                let expected = source
                    .lines()
                    .filter_map(|l| l.strip_prefix("// expect: "))
                    .flat_map(|s| s.split_whitespace())
                    .collect::<Vec<_>>();
                assert!(
                    !expected.is_empty(),
                    "missing expectations: {}",
                    path.display()
                );
                assert!(
                    !diagnostics.is_empty(),
                    "invalid fixture accepted: {}",
                    path.display()
                );
                for code in &expected {
                    assert!(
                        diagnostics.iter().any(|d| d.code.0 == *code),
                        "{} missing {code}\n{rendered}",
                        path.display()
                    );
                }
                for d in &diagnostics {
                    assert!(
                        expected.contains(&d.code.0),
                        "{} unexpected {}\n{rendered}",
                        path.display(),
                        d.code.0
                    );
                }
                let snapshot = path.with_extension("stderr");
                if snapshot.exists() {
                    if std::env::var_os("ZEN_UPDATE_SNAPSHOTS").is_some() {
                        fs::write(&snapshot, &rendered).unwrap();
                    }
                    assert_eq!(
                        rendered,
                        fs::read_to_string(snapshot).unwrap(),
                        "{}",
                        path.display()
                    );
                }
            }
        }
    }
}
#[test]
fn cross_module_nominal_identity_and_visibility() {
    let mut sources = SourceMap::default();
    let mut modules = vec![];
    for (name, source) in [
        (
            "models",
            "public struct User { public name: String; secret: Int; }",
        ),
        (
            "main",
            "import models.User as Person; fn f(p: Person) -> String { return p.secret; }",
        ),
    ] {
        let file = sources.add(name, source);
        let (syntax, d) = zen_syntax::parse(file, source);
        assert!(d.is_empty());
        modules.push(ModuleInput {
            name: name.into(),
            file,
            syntax,
        });
    }
    let checked = check(modules);
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code.0 == "ZEN-NAME-0005")
    );
}

#[test]
fn malformed_sources_never_panic() {
    // Deterministic mutation coverage of the complete frontend, without a fuzzing dependency.
    let alphabet = [
        "fn", "f", "(", ")", "{", "}", "->", "Int", "Unit", ";", "let", "=", "1", ".", "?", "<",
        ">", "return", "\"λ\"", "impl", "match", "_", ",", "async", "&",
    ];
    let mut state = 0x6a656e_u64;
    for _ in 0..1200 {
        let mut source = String::new();
        for _ in 0..48 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push_str(alphabet[(state >> 32) as usize % alphabet.len()]);
            source.push(' ');
        }
        let (syntax, diagnostics) = zen_syntax::parse(zen_diagnostics::FileId(0), &source);
        if diagnostics.is_empty() {
            let _ = check(vec![ModuleInput {
                name: "fuzz".into(),
                file: zen_diagnostics::FileId(0),
                syntax,
            }]);
        }
    }
}

#[test]
fn nesting_limits_are_diagnostics() {
    for source in [
        format!(
            "fn f() -> Unit {{ let x = {}1{}; }}",
            "(".repeat(2000),
            ")".repeat(2000)
        ),
        format!(
            "fn f() -> Unit {{ {} {} }}",
            "while true {".repeat(2000),
            "}".repeat(2000)
        ),
        format!("fn f() -> Unit {{ let x = {}1; }}", "1 + ".repeat(2000)),
    ] {
        let (_, diagnostics) = zen_syntax::parse(zen_diagnostics::FileId(0), &source);
        assert!(!diagnostics.is_empty());
    }
}

#[test]
fn typed_expressions_retain_symbols() {
    let source =
        "fn id<T>(x: T) -> T { return x; } fn main() -> Unit { let n = id(1); let m = n; }";
    let file = zen_diagnostics::FileId(0);
    let (syntax, d) = zen_syntax::parse(file, source);
    assert!(d.is_empty());
    let checked = check(vec![ModuleInput {
        name: "main".into(),
        file,
        syntax,
    }]);
    assert!(checked.diagnostics.is_empty());
    for name in ["x;", "id(1)", "n;"] {
        let start = source.rfind(name).unwrap();
        assert!(
            checked
                .expressions
                .iter()
                .any(|e| e.span.start == start && e.symbol.is_some()),
            "{name}"
        );
    }
    let ids = checked
        .expressions
        .iter()
        .map(|e| (e.span.file, e.id.0))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), checked.expressions.len());
}
