#[derive(Debug, Default)]
pub(super) struct Editor {
    input: String,
    cursor: usize,
    pub(super) selected: usize,
    pub(super) palette_hidden: bool,
}

impl Editor {
    pub(super) fn text(&self) -> &str {
        &self.input
    }

    pub(super) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(super) fn into_text(self) -> String {
        self.input
    }

    pub(super) fn insert(&mut self, value: &str) {
        self.input.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.palette_hidden = false;
        self.selected = 0;
    }

    pub(super) fn replace_with_command(&mut self, command: &str) {
        let token = command.split_whitespace().next().unwrap_or(command);
        self.input.clear();
        self.input.push_str(token);
        if command.contains('<') {
            self.input.push(' ');
        }
        self.cursor = self.input.len();
        self.palette_hidden = false;
        self.selected = 0;
    }

    pub(super) fn left(&mut self) {
        self.cursor = previous_boundary(&self.input, self.cursor);
    }

    pub(super) fn right(&mut self) {
        self.cursor = next_boundary(&self.input, self.cursor);
    }

    pub(super) fn home(&mut self) {
        self.cursor = 0;
    }

    pub(super) fn end(&mut self) {
        self.cursor = self.input.len();
    }

    pub(super) fn word_left(&mut self) {
        while self.cursor > 0 && !is_word(char_before(&self.input, self.cursor)) {
            self.left();
        }
        while self.cursor > 0 && is_word(char_before(&self.input, self.cursor)) {
            self.left();
        }
    }

    pub(super) fn word_right(&mut self) {
        while self.cursor < self.input.len() && !is_word(char_at(&self.input, self.cursor)) {
            self.right();
        }
        while self.cursor < self.input.len() && is_word(char_at(&self.input, self.cursor)) {
            self.right();
        }
    }

    pub(super) fn backspace(&mut self) {
        let start = previous_boundary(&self.input, self.cursor);
        if start < self.cursor {
            self.input.drain(start..self.cursor);
            self.cursor = start;
            self.palette_hidden = false;
            self.selected = 0;
        }
    }

    pub(super) fn delete(&mut self) {
        let end = next_boundary(&self.input, self.cursor);
        if self.cursor < end {
            self.input.drain(self.cursor..end);
            self.palette_hidden = false;
            self.selected = 0;
        }
    }

    pub(super) fn delete_word_back(&mut self) {
        let end = self.cursor;
        self.word_left();
        if self.cursor < end {
            self.input.drain(self.cursor..end);
            self.palette_hidden = false;
            self.selected = 0;
        }
    }

    pub(super) fn delete_to_start(&mut self) {
        self.input.drain(..self.cursor);
        self.cursor = 0;
        self.palette_hidden = false;
        self.selected = 0;
    }

    pub(super) fn delete_to_end(&mut self) {
        self.input.truncate(self.cursor);
        self.palette_hidden = false;
        self.selected = 0;
    }

    pub(super) fn previous_suggestion(&mut self, count: usize) {
        if count > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
            self.palette_hidden = false;
        }
    }

    pub(super) fn next_suggestion(&mut self, count: usize) {
        if count > 0 {
            self.selected = (self.selected + 1) % count;
            self.palette_hidden = false;
        }
    }

    pub(super) fn escape(&mut self) {
        self.palette_hidden = true;
        self.selected = 0;
    }
}

fn previous_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .chars()
        .next()
        .map(|ch| cursor + ch.len_utf8())
        .unwrap_or(value.len())
}

fn char_before(value: &str, cursor: usize) -> char {
    value[..cursor].chars().next_back().unwrap_or(' ')
}

fn char_at(value: &str, cursor: usize) -> char {
    value[cursor..].chars().next().unwrap_or(' ')
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
fn pop_last_utf8_char(input: &mut Vec<u8>) {
    let Some(last) = input.pop() else { return };
    if last & 0b1100_0000 == 0b1000_0000 {
        while matches!(input.last(), Some(byte) if byte & 0b1100_0000 == 0b1000_0000) {
            input.pop();
        }
        input.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_utf8_at_the_cursor_and_supports_line_shortcuts() {
        let mut editor = Editor::default();
        editor.insert("가나다");
        editor.left();
        editor.insert("X");
        assert_eq!(editor.text(), "가나X다");
        editor.home();
        editor.delete();
        assert_eq!(editor.text(), "나X다");
        editor.end();
        editor.delete_to_start();
        assert!(editor.text().is_empty());
    }

    #[test]
    fn moves_and_deletes_by_words() {
        let mut editor = Editor::default();
        editor.insert("hello 한국어 world");
        editor.word_left();
        assert_eq!(&editor.text()[..editor.cursor()], "hello 한국어 ");
        editor.delete_word_back();
        assert_eq!(editor.text(), "hello world");
        editor.word_right();
        editor.delete_to_end();
        assert_eq!(editor.text(), "hello world");
    }

    #[test]
    fn command_completion_keeps_required_argument_input_open() {
        let mut editor = Editor::default();
        editor.replace_with_command("/search <질문>");
        assert_eq!(editor.text(), "/search ");
        editor.replace_with_command("/model [id]");
        assert_eq!(editor.text(), "/model");
    }

    #[test]
    fn legacy_byte_helper_removes_a_complete_utf8_character() {
        let mut input = "a한".as_bytes().to_vec();
        pop_last_utf8_char(&mut input);
        assert_eq!(String::from_utf8(input).unwrap(), "a");
    }
}
