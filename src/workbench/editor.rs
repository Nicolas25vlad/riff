use std::{fs, path::Path};

use riff::Playlist;

#[derive(Debug, Clone)]
pub struct EditorState {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub scroll: usize,
    pub dirty: bool,
    pub clipboard: Option<String>,
    pub message: String,
}

impl EditorState {
    pub fn load(path: &Path) -> Result<Self, String> {
        let source = fs::read_to_string(path)
            .map_err(|err| format!("could not open {}: {err}", path.display()))?;
        let mut lines = source.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Ok(Self {
            lines,
            row: 0,
            col: 0,
            scroll: 0,
            dirty: false,
            clipboard: None,
            message: "Ctrl+S save · Ctrl+K cut · Ctrl+U paste · Ctrl+G help · Ctrl+X leave".into(),
        })
    }

    pub fn content(&self) -> String {
        let mut source = self.lines.join("\n");
        source.push('\n');
        source
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
        }
    }

    pub fn move_right(&mut self) {
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.clamp_col();
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let byte = char_to_byte_index(&self.lines[self.row], self.col);
        self.lines[self.row].insert(byte, ch);
        self.col += 1;
        self.dirty = true;
    }

    pub fn newline(&mut self) {
        let byte = char_to_byte_index(&self.lines[self.row], self.col);
        let tail = self.lines[self.row].split_off(byte);
        self.row += 1;
        self.lines.insert(self.row, tail);
        self.col = 0;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            let end = char_to_byte_index(&self.lines[self.row], self.col);
            let start = char_to_byte_index(&self.lines[self.row], self.col - 1);
            self.lines[self.row].replace_range(start..end, "");
            self.col -= 1;
            self.dirty = true;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.lines[self.row].push_str(&current);
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            let start = char_to_byte_index(&self.lines[self.row], self.col);
            let end = char_to_byte_index(&self.lines[self.row], self.col + 1);
            self.lines[self.row].replace_range(start..end, "");
            self.dirty = true;
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.dirty = true;
        }
    }

    pub fn cut_line(&mut self) {
        self.clipboard = Some(self.lines[self.row].clone());
        if self.lines.len() == 1 {
            self.lines[0].clear();
            self.col = 0;
        } else {
            self.lines.remove(self.row);
            self.row = self.row.min(self.lines.len() - 1);
            self.clamp_col();
        }
        self.dirty = true;
    }

    pub fn paste_line(&mut self) {
        if let Some(line) = self.clipboard.clone() {
            self.lines.insert(self.row + 1, line);
            self.row += 1;
            self.col = self.lines[self.row].chars().count();
            self.dirty = true;
        }
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let source = self.content();
        Playlist::parse(&source).map_err(|err| format!("not saved · {err}"))?;
        fs::write(path, source)
            .map_err(|err| format!("could not save {}: {err}", path.display()))?;
        self.dirty = false;
        self.message = "saved · playlist syntax valid".into();
        Ok(())
    }

    pub fn ensure_cursor_visible(&mut self, height: usize) {
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if self.row >= self.scroll.saturating_add(height.max(1)) {
            self.scroll = self.row.saturating_sub(height.saturating_sub(1));
        }
    }

    fn clamp_col(&mut self) {
        self.col = self.col.min(self.lines[self.row].chars().count());
    }
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> EditorState {
        EditorState {
            lines: vec!["abc".into()],
            row: 0,
            col: 3,
            scroll: 0,
            dirty: false,
            clipboard: None,
            message: String::new(),
        }
    }

    #[test]
    fn edits_unicode_by_character_position() {
        let mut state = editor();
        state.lines[0] = "Motör".into();
        state.col = 5;
        state.backspace();
        assert_eq!(state.lines[0], "Motö");
    }

    #[test]
    fn splits_and_joins_lines() {
        let mut state = editor();
        state.col = 1;
        state.newline();
        assert_eq!(state.lines, vec!["a", "bc"]);
        state.backspace();
        assert_eq!(state.lines, vec!["abc"]);
    }
}
