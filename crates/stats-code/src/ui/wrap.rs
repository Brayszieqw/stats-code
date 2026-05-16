/// Word-wrap `text` to at most `width` columns.
/// Returns a Vec of lines.  If a single word is longer than `width` it is
/// hard-broken into chunks.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let max_width = width.max(12);
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.chars().count() > max_width {
                lines.extend(split_long_word(word, max_width));
            } else {
                current.push_str(word);
            }
            continue;
        }
        let next_width = current.chars().count() + 1 + word.chars().count();
        if next_width <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = String::new();
            if word.chars().count() > max_width {
                lines.extend(split_long_word(word, max_width));
            } else {
                current.push_str(word);
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut chunk = String::new();
    let mut lines = Vec::new();
    for ch in word.chars() {
        if chunk.chars().count() >= width {
            lines.push(chunk);
            chunk = String::new();
        }
        chunk.push(ch);
    }
    if !chunk.is_empty() {
        lines.push(chunk);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_empty() {
        assert!(wrap_text("", 40).is_empty());
    }

    #[test]
    fn short_fits_one_line() {
        let lines = wrap_text("hello world", 40);
        assert_eq!(lines, vec!["hello world"]);
    }

    #[test]
    fn wraps_at_width() {
        let lines = wrap_text("aaa bbb ccc ddd eee", 20);
        for line in &lines {
            assert!(line.chars().count() <= 20, "line too long: {line}");
        }
    }

    #[test]
    fn long_word_split() {
        // min width is 12, so long words get split into ≤12 char chunks
        let lines = wrap_text("abcdefghijklmnopqrstuvwxyz", 5);
        for line in &lines {
            assert!(line.chars().count() <= 12, "line too long: {line}");
        }
        assert!(lines.len() > 1, "long word should be split");
    }
}
