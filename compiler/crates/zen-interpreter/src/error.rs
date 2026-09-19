use std::fmt;
use zen_diagnostics::{SourceMap, Span};
use zen_hir::{LambdaId, SymbolId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    MissingEntryPoint,
    InvalidEntryPoint,
    UnboundNative(SymbolId),
    Cancelled,
    UnspecifiedCancellation,
    Deadlock,
    Invariant,
    ConstantCycle(SymbolId),
    ResourceLimit,
    Host,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameIdentity {
    Function(SymbolId),
    Lambda(LambdaId),
    Constant(SymbolId),
}
#[derive(Clone, Debug)]
pub struct StackFrame {
    pub identity: FrameIdentity,
    pub name: String,
    pub call_site: Span,
}
#[derive(Clone, Debug)]
pub struct InterpreterError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<Span>,
    /// Outer caller first. Retained before frames are unwound.
    pub stack: Vec<StackFrame>,
}
impl fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "interpreter error: {}", self.message)
    }
}
impl std::error::Error for InterpreterError {}
impl InterpreterError {
    pub fn render(&self, sources: &SourceMap) -> String {
        use std::fmt::Write;
        let mut text = self.to_string();
        let location = |span: Span| {
            sources
                .files
                .get(span.file.0)
                .map(|file| {
                    let (line, column) = file.location(span.start);
                    format!("{}:{line}:{column}", file.path)
                })
                .unwrap_or_else(|| format!("{span:?}"))
        };
        if let Some(span) = self.span {
            write!(text, "\n  at {}", location(span)).unwrap();
        }
        for frame in self.stack.iter().rev() {
            write!(
                text,
                "\n  in {} (called at {})",
                frame.name,
                location(frame.call_site)
            )
            .unwrap();
        }
        text
    }
}
