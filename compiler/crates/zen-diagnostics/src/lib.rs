//! Source ownership and structured, stable-code diagnostics shared by all phases.
use std::fmt::Write;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub usize);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn new(file: FileId, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }
    pub fn join(self, other: Self) -> Self {
        Self {
            end: other.end,
            ..self
        }
    }
}
pub type TextRange = Span;
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: FileId,
    pub path: String,
    pub text: String,
    lines: Vec<usize>,
}
impl SourceFile {
    pub fn location(&self, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self
            .lines
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        (
            line + 1,
            self.text[self.lines[line]..offset].chars().count() + 1,
        )
    }
    pub fn line(&self, number: usize) -> &str {
        self.text
            .lines()
            .nth(number.saturating_sub(1))
            .unwrap_or("")
    }
}
#[derive(Default, Debug)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}
impl SourceMap {
    pub fn add(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let text = text.into();
        let id = FileId(self.files.len());
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        self.files.push(SourceFile {
            id,
            path: path.into(),
            text,
            lines,
        });
        id
    }
    pub fn get(&self, id: FileId) -> &SourceFile {
        &self.files[id.0]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticCode(pub &'static str);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct SuggestedFix {
    pub message: String,
    pub replacement: Option<(Span, String)>,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub primary: Span,
    pub message: String,
    pub labels: Vec<Label>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub related_symbols: Vec<String>,
    pub fixes: Vec<SuggestedFix>,
}
impl Diagnostic {
    pub fn error(code: &'static str, primary: Span, message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode(code),
            severity: Severity::Error,
            primary,
            message: message.into(),
            labels: vec![],
            expected: None,
            actual: None,
            related_symbols: vec![],
            fixes: vec![],
        }
    }
    pub fn label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }
    pub fn render(&self, sources: &SourceMap) -> String {
        let mut out = format!(
            "{}[{}]: {}\n",
            if self.severity == Severity::Error {
                "error"
            } else {
                "warning"
            },
            self.code.0,
            self.message
        );
        for (span, message) in std::iter::once((self.primary, ""))
            .chain(self.labels.iter().map(|l| (l.span, l.message.as_str())))
        {
            let file = sources.get(span.file);
            let (line, col) = file.location(span.start);
            let (_, end) = file.location(span.end);
            let width = if file.location(span.end).0 == line {
                end.saturating_sub(col).max(1)
            } else {
                1
            };
            let _ = writeln!(
                out,
                "  --> {}:{}:{}\n   |\n{line:>3} | {}\n   | {}{}{}",
                file.path,
                line,
                col,
                file.line(line),
                " ".repeat(col - 1),
                "^".repeat(width),
                if message.is_empty() {
                    String::new()
                } else {
                    format!(" {message}")
                }
            );
        }
        if let (Some(e), Some(a)) = (&self.expected, &self.actual) {
            let _ = writeln!(out, "   = expected {e}, received {a}");
        }
        for f in &self.fixes {
            let _ = writeln!(out, "   = help: {}", f.message);
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_locations() {
        let mut s = SourceMap::default();
        let id = s.add("x", "λx\nnext");
        assert_eq!(s.get(id).location(2), (1, 2));
        assert_eq!(s.get(id).location(4), (2, 1));
    }
}
