use tower_lsp_server::ls_types::{Position, Range};
/// The single UTF-8/UTF-16 conversion boundary. Invalid split-surrogate edits
/// are rejected instead of corrupting the document.
pub fn offset(text: &str, p: Position) -> Option<usize> {
    let start = if p.line == 0 {
        0
    } else {
        text.match_indices('\n').nth(p.line as usize - 1)?.0 + 1
    };
    let line = text[start..].split('\n').next()?.trim_end_matches('\r');
    let mut units = 0;
    for (byte, c) in line.char_indices() {
        if units == p.character {
            return Some(start + byte);
        }
        units += c.len_utf16() as u32;
        if units > p.character {
            return None;
        }
    }
    (units == p.character).then_some(start + line.len())
}
pub fn position(text: &str, byte: usize) -> Position {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let prefix = &text[..byte];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() as u32;
    let start = prefix.rfind('\n').map_or(0, |n| n + 1);
    Position::new(line, text[start..byte].encode_utf16().count() as u32)
}
pub fn range(text: &str, start: usize, end: usize) -> Range {
    Range::new(position(text, start), position(text, end))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_roundtrip() {
        let t = "a😀é\r\nλx\n";
        for (i, _) in t.char_indices() {
            if t.as_bytes()[i] != b'\n' && t.as_bytes()[i] != b'\r' {
                assert_eq!(offset(t, position(t, i)), Some(i));
            }
        }
        assert_eq!(offset(t, Position::new(0, 2)), None);
        assert_eq!(offset(t, Position::new(1, 1)), Some(11));
        assert_eq!(offset(t, Position::new(2, 0)), Some(t.len()));
    }
}
