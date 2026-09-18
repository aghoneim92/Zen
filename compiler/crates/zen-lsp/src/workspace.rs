use crate::position;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tower_lsp_server::ls_types::*;
use zen_diagnostics::{FileId, Span};
use zen_semantics::{CheckedProgram, analysis::Database};
#[derive(Clone)]
pub struct Document {
    pub text: String,
    pub version: i32,
}
pub struct Snapshot {
    pub database: Database,
    pub program: CheckedProgram,
    pub versions: BTreeMap<String, i32>,
}
pub fn path(uri: &Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}
pub fn uri(path: &Path) -> Option<Uri> {
    url::Url::from_file_path(path).ok()?.as_str().parse().ok()
}
pub use zen_semantics::analysis::source_root as root;

impl Snapshot {
    pub fn file(&self, u: &Uri) -> Option<FileId> {
        self.database
            .files
            .iter()
            .position(|f| f.as_ref().is_some_and(|f| f.path == u.as_str()))
            .map(FileId)
    }
    pub fn text(&self, f: FileId) -> &str {
        self.database.files[f.0]
            .as_ref()
            .map_or("", |f| f.text.as_str())
    }
    pub fn at(&self, p: &TextDocumentPositionParams) -> Option<(FileId, usize)> {
        let f = self.file(&p.text_document.uri)?;
        Some((f, position::offset(self.text(f), p.position)?))
    }
    pub fn location(&self, s: Span) -> Option<Location> {
        let f = self.database.files.get(s.file.0)?.as_ref()?;
        Some(Location {
            uri: f.path.parse().ok()?,
            range: position::range(&f.text, s.start, s.end),
        })
    }
    pub fn diagnostics(&self, file: FileId) -> Vec<Diagnostic> {
        self.program
            .diagnostics
            .iter()
            .filter(|d| d.primary.file == file)
            .map(|d| Diagnostic {
                range: position::range(self.text(file), d.primary.start, d.primary.end),
                severity: Some(match d.severity {
                    zen_diagnostics::Severity::Error => DiagnosticSeverity::ERROR,
                    zen_diagnostics::Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                code: Some(NumberOrString::String(d.code.0.into())),
                source: Some("zen".into()),
                message: match (&d.expected, &d.actual) {
                    (Some(e), Some(a)) => format!("{}\nExpected: {e}\nReceived: {a}", d.message),
                    _ => d.message.clone(),
                },
                related_information: Some(
                    d.labels
                        .iter()
                        .filter_map(|l| {
                            Some(DiagnosticRelatedInformation {
                                location: self.location(l.span)?,
                                message: l.message.clone(),
                            })
                        })
                        .collect(),
                ),
                ..Default::default()
            })
            .collect()
    }
}
pub fn load(
    root: PathBuf,
    open: BTreeMap<String, Document>,
    previous: Option<Arc<Snapshot>>,
) -> Snapshot {
    let mut database = previous
        .as_ref()
        .map(|s| s.database.clone())
        .unwrap_or_default();
    let mut paths = vec![];
    scan(&root, &mut paths, 0);
    paths.extend(
        open.keys()
            .filter_map(|u| u.parse().ok().and_then(|u| path(&u)))
            .filter(|p| crate::workspace::root(p) == root),
    );
    paths.sort();
    paths.dedup();
    let mut live = std::collections::BTreeSet::new();
    let mut versions = BTreeMap::new();
    for p in paths {
        let Some(u) = uri(&p) else { continue };
        let key = u.as_str().to_owned();
        let text = if let Some(d) = open.get(&key) {
            versions.insert(key.clone(), d.version);
            Some(d.text.clone())
        } else {
            std::fs::read_to_string(&p).ok()
        };
        let Some(text) = text else { continue };
        let name = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(".");
        let f = database.set_file(key, name, text);
        live.insert(f);
    }
    for (i, f) in database.files.iter_mut().enumerate() {
        if !live.contains(&FileId(i)) {
            *f = None;
        }
    }
    let program = database.analyze();
    Snapshot {
        database,
        program,
        versions,
    }
}
fn scan(root: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        let p = e.path();
        if t.is_dir() {
            if !matches!(
                e.file_name().to_str(),
                Some("target" | "node_modules" | ".git" | ".zed")
            ) {
                scan(&p, files, depth + 1);
            }
        } else if p.extension().is_some_and(|e| e == "zen") {
            files.push(p);
        }
    }
}
