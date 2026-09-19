use std::collections::BTreeMap;
use std::{
    fs,
    path::{Path, PathBuf},
};
use zen_diagnostics::FileId;
use zen_hir::{Body, ParamId, SymbolId, Type};
use zen_interpreter::{Host, Interpreter, Value};
#[derive(Default)]
struct TestHost {
    bindings: Vec<SymbolId>,
    events: Vec<String>,
}
impl Host for TestHost {
    fn call_native(
        &mut self,
        id: SymbolId,
        _: &BTreeMap<ParamId, Type>,
        args: &[Value],
    ) -> Option<Result<Value, String>> {
        if !self.bindings.contains(&id) {
            return None;
        }
        Some(match args {
            [Value::Int(value)] => {
                self.events.push(value.to_string());
                Ok(Value::Unit)
            }
            _ => Err("trace-int requires one Int".into()),
        })
    }
}

fn fixtures(path: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            fixtures(&path, out);
        } else if path.extension().is_some_and(|x| x == "zen") {
            out.push(path);
        }
    }
}
#[test]
fn execution_conformance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/conformance");
    let mut paths = vec![];
    fixtures(&root, &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let (syntax, diagnostics) = zen_syntax::parse(FileId(0), &source);
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:#?}",
            path.display()
        );
        let checked = zen_semantics::check(vec![zen_semantics::ModuleInput {
            file: FileId(0),
            name: "main".into(),
            syntax,
        }]);
        assert!(
            checked.diagnostics.is_empty(),
            "{}: {:#?}",
            path.display(),
            checked.diagnostics
        );
        let hir = zen_hir::lower_program(&checked).unwrap();
        drop(checked);
        let expected = fs::read_to_string(path.with_extension("expected")).unwrap();
        let mut bindings = vec![];
        if path.with_extension("host").exists() {
            for line in fs::read_to_string(path.with_extension("host"))
                .unwrap()
                .lines()
            {
                let words = line.split_whitespace().collect::<Vec<_>>();
                assert_eq!(words.len(), 2);
                assert_eq!(words[1], "trace-int");
                // Explicit test manifest linkage happens once, outside execution.
                let function = hir.functions.iter().find(|f| f.name == words[0]).unwrap();
                assert!(matches!(function.body, Body::Native));
                assert_eq!(function.parameters.len(), 1);
                assert_eq!(function.parameters[0].ty, Type::Int);
                assert_eq!(function.completion, Type::Unit);
                bindings.push(function.id);
            }
        }
        let events = if path.with_extension("events").exists() {
            fs::read_to_string(path.with_extension("events")).unwrap()
        } else {
            String::new()
        };
        for _ in 0..2 {
            let mut interpreter = Interpreter::new(
                &hir,
                TestHost {
                    bindings: bindings.clone(),
                    events: vec![],
                },
            );
            let value = interpreter
                .run_main(FileId(0))
                .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
            assert_eq!(value.to_string(), expected.trim(), "{}", path.display());
            assert_eq!(
                interpreter.host.events.join("\n"),
                events.trim(),
                "{}",
                path.display()
            );
        }
    }
}
