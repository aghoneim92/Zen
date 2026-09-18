//! Canonical, syntactic Zen formatting. The parser supplies delimiter/operator roles;
//! original tokens and trivia supply spelling and comment ownership.
mod doc;
use doc::Doc;
use std::collections::{BTreeMap, BTreeSet};
use zen_diagnostics::{Diagnostic, FileId};
use zen_syntax::{
    ParsedSource,
    lexer::{TokenKind, TriviaKind},
};

#[derive(Debug)]
pub enum FormatError {
    Syntax(Vec<Diagnostic>),
    Internal(String),
}
pub fn format_source(source: &str) -> Result<String, FormatError> {
    format_file(FileId(0), source)
}
/// Retain the caller's source identity for normal compiler diagnostics.
pub fn format_file(file: FileId, source: &str) -> Result<String, FormatError> {
    let parsed = zen_syntax::parse_source(file, source);
    format_parsed(source, &parsed)
}
/// `source` must be the exact text used to produce `parsed`.
pub fn format_parsed(source: &str, parsed: &ParsedSource) -> Result<String, FormatError> {
    if !parsed.diagnostics.is_empty() {
        return Err(FormatError::Syntax(parsed.diagnostics.clone()));
    }
    let mut f = Formatter {
        source,
        parsed,
        pairs: BTreeMap::new(),
        generics: parsed.layout.generics.iter().map(|s| s.start).collect(),
        fields: parsed.layout.fields.iter().map(|s| s.start).collect(),
        parens: parsed.layout.parentheses.iter().map(|s| s.start).collect(),
        binary: parsed.layout.binary.iter().map(|s| s.start).collect(),
        unary: parsed.layout.unary.iter().map(|s| s.start).collect(),
        members: parsed.layout.members.iter().map(|s| s.start).collect(),
        boundaries: BTreeMap::new(),
    };
    for &(end, kind) in &parsed.layout.boundaries {
        let lines = match kind {
            zen_syntax::BoundaryKind::Statement => 1,
            zen_syntax::BoundaryKind::Declaration
            | zen_syntax::BoundaryKind::Definition
            | zen_syntax::BoundaryKind::BlockArm => 2,
        };
        f.boundaries
            .entry(end)
            .and_modify(|n: &mut usize| *n = (*n).max(lines))
            .or_insert(lines);
    }
    let mut stack = vec![];
    for (i, t) in parsed.tokens.iter().enumerate() {
        if source.get(t.span.start..t.span.end).is_none() {
            return Err(FormatError::Internal("source/token span mismatch".into()));
        }
        if t.kind != TokenKind::Punct {
            continue;
        }
        if ["(", "[", "{"].contains(&t.text.as_str()) || f.generics.contains(&t.span.start) {
            stack.push(i);
        } else if [")", "]", "}"].contains(&t.text.as_str())
            || (t.text == ">" && stack.last().is_some_and(|&j| f.raw(j) == "<"))
        {
            let Some(j) = stack.pop() else {
                return Err(FormatError::Internal("unbalanced parsed delimiters".into()));
            };
            f.pairs.insert(j, i);
        }
    }
    if !stack.is_empty() {
        return Err(FormatError::Internal("unclosed parsed delimiter".into()));
    }
    let end = parsed.tokens.len() - 1;
    let doc = Doc::concat([
        f.gap(0, Doc::Nil),
        f.sequence(0, end, false),
        if end == 0 {
            Doc::Nil
        } else {
            f.gap(end, Doc::Nil)
        },
    ]);
    let rendered = doc::render(&doc, 100);
    Ok(format!("{}\n", rendered.trim_end()))
}
struct Formatter<'a> {
    source: &'a str,
    parsed: &'a ParsedSource,
    pairs: BTreeMap<usize, usize>,
    generics: BTreeSet<usize>,
    fields: BTreeSet<usize>,
    parens: BTreeSet<usize>,
    binary: BTreeSet<usize>,
    unary: BTreeSet<usize>,
    members: BTreeSet<usize>,
    boundaries: BTreeMap<usize, usize>,
}
impl Formatter<'_> {
    fn raw(&self, i: usize) -> &str {
        let s = self.parsed.tokens[i].span;
        &self.source[s.start..s.end]
    }
    fn punct(&self, i: usize, text: &str) -> bool {
        self.parsed.tokens[i].kind == TokenKind::Punct && self.raw(i) == text
    }
    /// Trivia belongs to its token gap. Same-line comments trail the preceding
    /// token; other comments lead the next token (or dangle at a closing delimiter).
    fn gap(&self, i: usize, normal: Doc) -> Doc {
        let comments: Vec<_> = self.parsed.tokens[i]
            .leading
            .iter()
            .filter(|t| t.kind != TriviaKind::Whitespace)
            .collect();
        if comments.is_empty() {
            return normal;
        }
        fn hard_lines(doc: &Doc) -> usize {
            match doc {
                Doc::HardLine => 1,
                Doc::Concat(ds) => ds.iter().map(hard_lines).sum(),
                _ => 0,
            }
        }
        let structural_lines = hard_lines(&normal);
        let mut trailing = true;
        let mut docs = vec![];
        let mut previous = if i == 0 {
            0
        } else {
            self.parsed.tokens[i - 1].span.end
        };
        let mut ended_line = false;
        for (n, c) in comments.iter().enumerate() {
            let between = &self.source[previous..c.span.start];
            let newlines = between.matches('\n').count();
            if newlines > 0 {
                if i > 0 || n > 0 {
                    docs.push(Doc::HardLine);
                    if newlines > 1 || trailing && structural_lines > 1 {
                        docs.push(Doc::HardLine);
                    }
                }
                trailing = false;
            } else if i > 0 || n > 0 {
                docs.push(Doc::text(" "));
            }
            docs.push(Doc::text(
                self.source[c.span.start..c.span.end]
                    .replace("\r\n", "\n")
                    .trim_end()
                    .to_owned(),
            ));
            ended_line = c.kind == TriviaKind::LineComment;
            previous = c.span.end;
        }
        if ended_line || self.source[previous..self.parsed.tokens[i].span.start].contains('\n') {
            docs.push(Doc::HardLine);
            if trailing && structural_lines > 1 {
                docs.push(Doc::HardLine);
            }
        } else {
            docs.push(match normal {
                _ if !trailing && structural_lines > 0 => Doc::HardLine,
                Doc::Nil => Doc::text(" "),
                other => other,
            });
        }
        Doc::concat(docs)
    }
    fn spacing(&self, i: usize, list: bool) -> Doc {
        let a = self.raw(i - 1);
        let b = self.raw(i);
        let prev = &self.parsed.tokens[i - 1];
        let next = &self.parsed.tokens[i];
        if let Some(&lines) = self.boundaries.get(&prev.span.end) {
            return Doc::concat((0..lines).map(|_| Doc::HardLine));
        }
        if self.punct(i - 1, ";") {
            return if !list
                && self
                    .parsed
                    .module
                    .declarations
                    .iter()
                    .any(|d| d.span.start == next.span.start)
            {
                Doc::concat([Doc::HardLine, Doc::HardLine])
            } else {
                Doc::HardLine
            };
        }
        if self.punct(i - 1, ",") {
            return if list { Doc::Line(" ") } else { Doc::HardLine };
        }
        if self.binary.contains(&next.span.start) {
            return Doc::Line(" ");
        }
        if self.binary.contains(&prev.span.start) {
            return Doc::text(" ");
        }
        if self.unary.contains(&next.span.start) {
            return if ["(", "["].contains(&a)
                || self.unary.contains(&prev.span.start) && a != "await"
            {
                Doc::Nil
            } else {
                Doc::text(" ")
            };
        }
        if self.unary.contains(&prev.span.start) {
            return if a == "await" {
                Doc::text(" ")
            } else {
                Doc::Nil
            };
        }
        if self.punct(i, ".")
            && (prev.kind == TokenKind::Keyword && a != "self" || ["=", "=>", ":"].contains(&a))
        {
            return Doc::text(" ");
        }
        if next.kind == TokenKind::Punct && [";", ",", ":", ".", "?"].contains(&b) {
            return Doc::Nil;
        }
        if self.punct(i - 1, ".") || self.generics.contains(&next.span.start) {
            return Doc::Nil;
        }
        if self.punct(i, "(") {
            return if [
                "if", "while", "match", "return", "await", "async", "=", "=>", "->", "with",
            ]
            .contains(&a)
                || self.punct(i - 1, ":")
            {
                Doc::text(" ")
            } else {
                Doc::Nil
            };
        }
        if self.punct(i - 1, "}") && !["else", "with"].contains(&b) && b != "{" {
            return Doc::HardLine;
        }
        Doc::text(" ")
    }
    fn sequence(&self, start: usize, end: usize, list: bool) -> Doc {
        fn finish(head: &mut Vec<Doc>, tail: &mut Vec<Doc>) -> Doc {
            if !tail.is_empty() {
                head.push(Doc::IndentIfBreak(Box::new(Doc::concat(std::mem::take(
                    tail,
                )))));
            }
            Doc::concat(std::mem::take(head)).group()
        }
        let mut docs = vec![];
        let mut head = vec![];
        let mut tail = vec![];
        let mut continuation = false;
        let mut i = start;
        while i < end {
            let pos = self.parsed.tokens[i].span.start;
            let member = self.members.contains(&pos);
            if i > start {
                let separator = self.punct(i - 1, ";")
                    || self.punct(i - 1, ",")
                    || self
                        .boundaries
                        .contains_key(&self.parsed.tokens[i - 1].span.end);
                if separator {
                    docs.push(finish(&mut head, &mut tail));
                    continuation = false;
                    docs.push(self.gap(i, self.spacing(i, list)));
                } else {
                    continuation |= self.binary.contains(&pos) || member;
                    let gap = self.gap(
                        i,
                        if member {
                            Doc::Line("")
                        } else {
                            self.spacing(i, list)
                        },
                    );
                    if continuation {
                        tail.push(gap);
                    } else {
                        head.push(gap);
                    }
                }
            }
            let doc = if let Some(&close) = self.pairs.get(&i) {
                let doc = self.delimited(i, close);
                i = close + 1;
                doc
            } else {
                let doc = Doc::text(self.raw(i));
                i += 1;
                doc
            };
            if continuation {
                tail.push(doc);
            } else {
                head.push(doc);
            }
        }
        docs.push(finish(&mut head, &mut tail));
        Doc::concat(docs)
    }
    fn delimited(&self, open: usize, close: usize) -> Doc {
        let brace = self.punct(open, "{");
        let fields = self.fields.contains(&self.parsed.tokens[open].span.start);
        let paren = self.parens.contains(&self.parsed.tokens[open].span.start);
        let list = !brace && !paren || fields;
        let trailing = list && close > open + 1 && self.punct(close - 1, ",");
        let end = close - usize::from(trailing);
        let empty = end == open + 1;
        let line = if brace { Doc::HardLine } else { Doc::Line("") };
        let mut inner = vec![];
        if !empty {
            inner.push(self.gap(open + 1, line.clone()));
            inner.push(self.sequence(open + 1, end, list && !fields));
            if list {
                // Preserve comments preceding an existing trailing comma.
                if trailing {
                    inner.push(self.gap(close - 1, Doc::Nil));
                }
                inner.push(if fields {
                    Doc::text(",")
                } else {
                    Doc::IfBreak(",")
                });
            }
        }
        // Closing trivia stays inside indentation; the final delimiter dedents.
        let has_comments = self.parsed.tokens[close]
            .leading
            .iter()
            .any(|t| t.kind != TriviaKind::Whitespace);
        if has_comments {
            let gap = self.gap(close, if empty { Doc::Nil } else { line.clone() });
            inner.push(gap);
        }
        let ending = if has_comments || empty && !brace {
            Doc::Nil
        } else {
            line
        };
        // When a closing comment ends in a newline it must dedent the delimiter.
        // A zero-width line already emitted by gap is handled by the renderer.
        let doc = Doc::concat([
            Doc::text(self.raw(open)),
            if brace {
                Doc::concat(inner).indent()
            } else {
                Doc::IndentIfBreak(Box::new(Doc::concat(inner)))
            },
            ending,
            Doc::text(self.raw(close)),
        ]);
        let sole_lambda = self.punct(open, "(")
            && !empty
            && self.parsed.layout.lambdas.iter().any(|s| {
                s.start == self.parsed.tokens[open + 1].span.start
                    && s.end == self.parsed.tokens[end - 1].span.end
            });
        let contains_block = (open + 1..close).any(|i| self.punct(i, "{"));
        if list
            && ((!sole_lambda && contains_block)
                || (open + 1..=close).any(|i| {
                    self.parsed.tokens[i]
                        .leading
                        .iter()
                        .any(|t| t.kind != TriviaKind::Whitespace)
                }))
        {
            Doc::Broken(Box::new(doc))
        } else {
            doc.group()
        }
    }
}
