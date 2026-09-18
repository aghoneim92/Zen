mod formatting;
use std::{collections::BTreeSet, env, path::PathBuf, process::ExitCode};
use zen_diagnostics::{Diagnostic, SourceMap, Span};
use zen_semantics::ModuleInput;
fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "fmt") {
        return formatting::run(&args[1..]);
    }
    if args == ["lsp"] {
        zen_lsp::run();
        return ExitCode::SUCCESS;
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] || args == ["check", "--help"] {
        println!(
            "Zen compiler frontend\n\nUsage: zen check <file>\n       zen fmt [--check] <path>...\n       zen fmt -\n       zen lsp\n       zen --help\n       zen --version"
        );
        return ExitCode::SUCCESS;
    }
    if args == ["--version"] || args == ["-V"] {
        println!("zen {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() != 2 || args[0] != "check" {
        eprintln!("error: expected `zen check <file>`; see --help");
        return ExitCode::from(2);
    }
    let path = match std::fs::canonicalize(&args[1]) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error[ZEN-IO-0001]: cannot open {}: {e}", args[1]);
            return ExitCode::from(1);
        }
    };
    let root = zen_semantics::analysis::source_root(&path);
    let mut loader = Loader {
        root,
        sources: SourceMap::default(),
        modules: vec![],
        diagnostics: vec![],
        visited: BTreeSet::new(),
    };
    loader.load(path, None);
    if loader.diagnostics.is_empty() {
        let result = zen_semantics::check(loader.modules);
        loader.diagnostics.extend(result.diagnostics);
    }
    for d in &loader.diagnostics {
        eprintln!("{}", d.render(&loader.sources));
    }
    if loader.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

struct Loader {
    root: PathBuf,
    sources: SourceMap,
    modules: Vec<ModuleInput>,
    diagnostics: Vec<Diagnostic>,
    visited: BTreeSet<PathBuf>,
}
impl Loader {
    fn load(&mut self, path: PathBuf, import: Option<Span>) {
        if !self.visited.insert(path.clone()) {
            return;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                let span = import.unwrap_or_else(|| {
                    let id = self.sources.add(path.display().to_string(), "");
                    Span::new(id, 0, 0)
                });
                self.diagnostics.push(Diagnostic::error(
                    "ZEN-IO-0001",
                    span,
                    format!("cannot read UTF-8 source {}: {e}", path.display()),
                ));
                return;
            }
        };
        let file = self.sources.add(path.display().to_string(), source);
        let (syntax, errors) = zen_syntax::parse(file, &self.sources.get(file).text);
        self.diagnostics.extend(errors);
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .with_extension("");
        let name = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join(".");
        let imports = syntax.imports.clone();
        self.modules.push(ModuleInput { name, file, syntax });
        for i in imports {
            if i.path.len() < 2 {
                self.diagnostics.push(Diagnostic::error(
                    "ZEN-NAME-0001",
                    i.span,
                    "import requires an absolute module path and declaration",
                ));
                continue;
            }
            let mut path = self.root.clone();
            for part in &i.path[..i.path.len() - 1] {
                path.push(&part.text);
            }
            path.set_extension("zen");
            self.load(path, Some(i.span));
        }
    }
}
