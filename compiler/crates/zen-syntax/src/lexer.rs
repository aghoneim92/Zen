use zen_diagnostics::{Diagnostic, FileId, Span};
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident,
    Keyword,
    Integer,
    Float,
    String,
    Char,
    Punct,
    Eof,
}
#[derive(Clone, Debug)]
pub struct Token {
    /// Trivia preceding this token; EOF owns trailing file trivia.
    pub leading: Vec<Trivia>,
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    LineComment,
    BlockComment,
}
#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}
const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "component",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "import",
    "in",
    "interface",
    "let",
    "match",
    "native",
    "public",
    "return",
    "self",
    "state",
    "struct",
    "true",
    "unit",
    "var",
    "view",
    "while",
    "with",
];
pub fn lex(file: FileId, source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = vec![];
    let mut errors = vec![];
    let mut leading = vec![];
    let mut i = 0;
    let b = source.as_bytes();
    while i < b.len() {
        let start = i;
        let c = source[i..].chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            while i < b.len() && source[i..].chars().next().unwrap().is_whitespace() {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
            leading.push(Trivia {
                kind: TriviaKind::Whitespace,
                span: Span::new(file, start, i),
            });
            continue;
        }
        if source[i..].starts_with("//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            leading.push(Trivia {
                kind: TriviaKind::LineComment,
                span: Span::new(file, start, i),
            });
            continue;
        }
        if source[i..].starts_with("/*") {
            i += 2;
            if let Some(end) = source[i..].find("*/") {
                i += end + 2;
            } else {
                i = b.len();
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0002",
                    Span::new(file, start, i),
                    "unterminated block comment",
                ));
            }
            leading.push(Trivia {
                kind: TriviaKind::BlockComment,
                span: Span::new(file, start, i),
            });
            continue;
        }
        let kind;
        let mut decoded = None;
        if c.is_ascii_alphabetic() || c == '_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            kind = if KEYWORDS.contains(&&source[start..i]) {
                TokenKind::Keyword
            } else {
                TokenKind::Ident
            };
        } else if c.is_ascii_digit() {
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                i += 1;
            }
            let mut float = false;
            if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                float = true;
                i += 1;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                    i += 1;
                }
            }
            if i < b.len() && matches!(b[i], b'e' | b'E') {
                float = true;
                i += 1;
                if i < b.len() && matches!(b[i], b'+' | b'-') {
                    i += 1;
                }
                let exp = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if exp == i {
                    errors.push(Diagnostic::error(
                        "ZEN-LEX-0003",
                        Span::new(file, start, i),
                        "missing exponent digits",
                    ));
                }
            }
            let suffix = i;
            while i < b.len() && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let s = &source[suffix..i];
            if !s.is_empty()
                && !(if float {
                    &["f32", "f64"][..]
                } else {
                    &["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"][..]
                })
                .contains(&s)
            {
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0003",
                    Span::new(file, suffix, i),
                    "invalid numeric suffix",
                ));
            }
            if source[start..suffix].ends_with('_') || source[start..suffix].contains("__") {
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0003",
                    Span::new(file, start, i),
                    "numeric separators must separate digits",
                ));
            }
            kind = if float {
                TokenKind::Float
            } else {
                TokenKind::Integer
            };
        } else if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < b.len() {
                let ch = source[i..].chars().next().unwrap();
                i += ch.len_utf8();
                if ch == quote {
                    closed = true;
                    break;
                }
                if ch == '\n' || ch == '\r' {
                    break;
                }
                if ch != '\\' {
                    value.push(ch);
                    continue;
                }
                let esc_start = i - 1;
                let Some(esc) = source[i..].chars().next() else {
                    break;
                };
                i += esc.len_utf8();
                match esc {
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' if quote == '\'' => value.push('\''),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'u' if i < b.len() && b[i] == b'{' => {
                        i += 1;
                        let digits = i;
                        while i < b.len() && b[i].is_ascii_hexdigit() {
                            i += 1;
                        }
                        let scalar = u32::from_str_radix(&source[digits..i], 16)
                            .ok()
                            .and_then(char::from_u32);
                        if i < b.len() && b[i] == b'}' {
                            i += 1;
                            if let Some(v) = scalar {
                                value.push(v);
                            } else {
                                errors.push(Diagnostic::error(
                                    "ZEN-LEX-0004",
                                    Span::new(file, esc_start, i),
                                    "invalid Unicode scalar escape",
                                ));
                            }
                        } else {
                            errors.push(Diagnostic::error(
                                "ZEN-LEX-0004",
                                Span::new(file, esc_start, i),
                                "expected closing brace in Unicode escape",
                            ));
                        }
                    }
                    _ => errors.push(Diagnostic::error(
                        "ZEN-LEX-0004",
                        Span::new(file, esc_start, i),
                        "unknown escape sequence",
                    )),
                }
            }
            if !closed {
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0005",
                    Span::new(file, start, i),
                    "unterminated literal",
                ));
            }
            if quote == '\'' && value.chars().count() != 1 {
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0006",
                    Span::new(file, start, i),
                    "character literal requires exactly one Unicode scalar",
                ));
            }
            kind = if quote == '"' {
                TokenKind::String
            } else {
                TokenKind::Char
            };
            decoded = Some(value);
        } else {
            let pair = if i + 2 <= b.len() {
                source.get(i..i + 2).unwrap_or("")
            } else {
                ""
            };
            if ["->", "=>", "==", "!=", "<=", ">=", "&&", "||"].contains(&pair) {
                i += 2;
                kind = TokenKind::Punct;
            } else if "{}()[]<>,;:.=+-*!?&/%@".contains(c) {
                i += 1;
                kind = TokenKind::Punct;
            } else {
                i += c.len_utf8();
                errors.push(Diagnostic::error(
                    "ZEN-LEX-0001",
                    Span::new(file, start, i),
                    format!("unexpected character `{c}`; identifiers are ASCII"),
                ));
                continue;
            }
        }
        tokens.push(Token {
            leading: std::mem::take(&mut leading),
            kind,
            text: decoded.unwrap_or_else(|| source[start..i].into()),
            span: Span::new(file, start, i),
        });
    }
    tokens.push(Token {
        leading,
        kind: TokenKind::Eof,
        text: String::new(),
        span: Span::new(file, i, i),
    });
    (tokens, errors)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spans_and_unicode() {
        let (t, d) = lex(FileId(0), "let x = \"λ\"; // hi");
        assert!(d.is_empty());
        assert_eq!(t[3].text, "λ");
        assert_eq!((t[3].span.start, t[3].span.end), (8, 12));
    }
    #[test]
    fn lexical_errors() {
        for s in ["/*", "\"\\q\"", "'ab'", "é", "1.2e", "12bad", "'\\u{D800}'"] {
            assert!(!lex(FileId(0), s).1.is_empty(), "{s}");
        }
    }
    #[test]
    fn reserves_with() {
        assert_eq!(lex(FileId(0), "with").0[0].kind, TokenKind::Keyword);
    }
}

#[cfg(test)]
mod trivia_tests {
    use super::*;
    #[test]
    fn trivia_and_tokens_partition_valid_source() {
        let source = " // Arabic مرحبا\r\nfn f() -> Unit { /* block\n text */ let s = \"\\u{1f600}\"; } // eof";
        let (tokens, errors) = lex(FileId(3), source);
        assert!(errors.is_empty());
        let mut end = 0;
        let mut rebuilt = String::new();
        for token in tokens {
            for span in token
                .leading
                .iter()
                .map(|t| t.span)
                .chain(std::iter::once(token.span))
            {
                assert_eq!(span.file, FileId(3));
                assert_eq!(span.start, end);
                rebuilt.push_str(&source[span.start..span.end]);
                end = span.end;
            }
        }
        assert_eq!(rebuilt, source);
    }
}
