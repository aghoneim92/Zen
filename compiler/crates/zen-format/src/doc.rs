//! A small document algebra. Width counts Unicode scalar values, not terminal cells.
#[derive(Clone, Debug)]
pub(crate) enum Doc {
    Nil,
    Text(String),
    Line(&'static str),
    HardLine,
    Concat(Vec<Doc>),
    Indent(Box<Doc>),
    IndentIfBreak(Box<Doc>),
    Group(Box<Doc>),
    Broken(Box<Doc>),
    IfBreak(&'static str),
}
impl Doc {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
    pub fn concat(d: impl IntoIterator<Item = Doc>) -> Self {
        Self::Concat(d.into_iter().collect())
    }
    pub fn indent(self) -> Self {
        Self::Indent(Box::new(self))
    }
    pub fn group(self) -> Self {
        Self::Group(Box::new(self))
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Flat,
    Break,
}
type Command<'a> = (usize, Mode, &'a Doc);
fn fits(mut remaining: isize, mut stack: Vec<Command<'_>>) -> bool {
    while remaining >= 0 {
        let Some((indent, mode, doc)) = stack.pop() else {
            return true;
        };
        match doc {
            Doc::Nil => {}
            Doc::Text(s) => {
                if s.contains('\n') {
                    return false;
                }
                remaining -= s.chars().count() as isize;
            }
            Doc::Line(flat) => match mode {
                Mode::Flat => remaining -= flat.len() as isize,
                Mode::Break => return true,
            },
            Doc::HardLine => return true,
            Doc::Concat(ds) => stack.extend(ds.iter().rev().map(|d| (indent, mode, d))),
            Doc::Indent(d) => stack.push((indent + 4, mode, d)),
            Doc::IndentIfBreak(d) => stack.push((
                indent + if matches!(mode, Mode::Break) { 4 } else { 0 },
                mode,
                d,
            )),
            Doc::Group(d) => stack.push((indent, Mode::Flat, d)),
            Doc::Broken(d) => stack.push((indent, Mode::Break, d)),
            Doc::IfBreak(s) => {
                if matches!(mode, Mode::Break) {
                    remaining -= s.len() as isize;
                }
            }
        }
    }
    false
}
pub(crate) fn render(doc: &Doc, width: usize) -> String {
    let mut output = String::new();
    let mut column = 0;
    let mut stack = vec![(0, Mode::Break, doc)];
    while let Some((indent, mode, doc)) = stack.pop() {
        match doc {
            Doc::Nil => {}
            Doc::Text(s) => {
                if !s.is_empty() {
                    if column == 0 {
                        output.push_str(&" ".repeat(indent));
                        column = indent;
                    }
                    output.push_str(s);
                    column = s
                        .rfind('\n')
                        .map_or(column + s.chars().count(), |i| s[i + 1..].chars().count());
                }
            }
            Doc::Line(flat) if matches!(mode, Mode::Flat) => {
                output.push_str(flat);
                column += flat.len();
            }
            Doc::HardLine | Doc::Line(_) => {
                while output.ends_with(' ') {
                    output.pop();
                }
                output.push('\n');
                column = 0;
            }
            Doc::Concat(ds) => stack.extend(ds.iter().rev().map(|d| (indent, mode, d))),
            Doc::Indent(d) => stack.push((indent + 4, mode, d)),
            Doc::IndentIfBreak(d) => stack.push((
                indent + if matches!(mode, Mode::Break) { 4 } else { 0 },
                mode,
                d,
            )),
            Doc::Broken(d) => stack.push((indent, Mode::Break, d)),
            Doc::Group(d) => {
                let mut trial = stack.clone();
                trial.push((indent, Mode::Flat, d));
                let mode = if fits(
                    width as isize - (if column == 0 { indent } else { column }) as isize,
                    trial,
                ) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                stack.push((indent, mode, d));
            }
            Doc::IfBreak(s) => {
                if matches!(mode, Mode::Break) {
                    output.push_str(s);
                    column += s.len();
                }
            }
        }
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_and_width() {
        let d = Doc::concat([Doc::text("ab"), Doc::Line(" "), Doc::text("cd")]).group();
        assert_eq!(render(&d, 5), "ab cd");
        assert_eq!(render(&d, 4), "ab\ncd");
        assert_eq!(render(&d, 6), "ab cd");
        assert_eq!(render(&Doc::Nil, 1), "");
        assert_eq!(render(&Doc::text("indivisible"), 1), "indivisible");
    }
    #[test]
    fn indentation_and_nested_groups() {
        let d = Doc::concat([
            Doc::text("{"),
            Doc::concat([
                Doc::HardLine,
                Doc::concat([Doc::text("a"), Doc::Line(" "), Doc::text("b")]).group(),
            ])
            .indent(),
            Doc::HardLine,
            Doc::text("}"),
        ])
        .group();
        assert_eq!(render(&d, 100), "{\n    a b\n}");
        assert_eq!(render(&d, 6), "{\n    a\n    b\n}");
    }
    #[test]
    fn suffix_participates_in_fit() {
        let d = Doc::concat([
            Doc::concat([Doc::text("a"), Doc::Line(" "), Doc::text("b")]).group(),
            Doc::text(";"),
        ]);
        assert_eq!(render(&d, 3), "a\nb;");
    }
}
