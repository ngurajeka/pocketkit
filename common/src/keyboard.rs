//! On-screen keyboard for typing URLs and search terms with the D-pad.

use crate::input::Button;

pub const ROWS: usize = 4;
pub const COLS: usize = 10;
pub const MAX_LEN: usize = 2000;

/// Three layouts, cycled with X. Every row has exactly `COLS` keys.
pub const PAGES: [[&str; ROWS]; 3] = [
    ["1234567890", "qwertyuiop", "asdfghjkl:", "zxcvbnm./-"],
    ["1234567890", "QWERTYUIOP", "ASDFGHJKL:", "ZXCVBNM./-"],
    ["!@#$%^&*()", "~`|\\{}[]<>", "?=+'\";,:_-", "./%&#@!*~_"],
];
pub const PAGE_NAMES: [&str; 3] = ["abc", "ABC", "#+="];

#[derive(Debug, PartialEq, Eq)]
pub enum KbResult {
    Pending,
    Submit(String),
    Cancel,
}

#[derive(Debug, Clone, Default)]
pub struct Keyboard {
    pub text: String,
    pub page: usize,
    pub row: usize,
    pub col: usize,
}

impl Keyboard {
    pub fn new(text: &str) -> Self {
        Self { text: text.to_string(), page: 0, row: 1, col: 0 }
    }

    pub fn key_at(&self, row: usize, col: usize) -> char {
        PAGES[self.page][row].chars().nth(col).unwrap_or(' ')
    }

    fn push(&mut self, s: &str) {
        if self.text.chars().count() + s.chars().count() <= MAX_LEN {
            self.text.push_str(s);
        }
    }

    pub fn handle(&mut self, button: Button) -> KbResult {
        match button {
            Button::Up => self.row = (self.row + ROWS - 1) % ROWS,
            Button::Down => self.row = (self.row + 1) % ROWS,
            Button::Left => self.col = (self.col + COLS - 1) % COLS,
            Button::Right => self.col = (self.col + 1) % COLS,
            Button::A => self.push(&self.key_at(self.row, self.col).to_string()),
            Button::B => {
                self.text.pop();
            }
            Button::Y => self.push(" "),
            Button::X => self.page = (self.page + 1) % PAGES.len(),
            Button::L1 => self.text.clear(),
            Button::R1 => self.push(".com"),
            Button::Start => return KbResult::Submit(self.text.clone()),
            Button::Select => return KbResult::Cancel,
            Button::L2 | Button::R2 | Button::Menu => {}
        }
        KbResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_full() {
        for page in PAGES {
            for row in page {
                assert_eq!(row.chars().count(), COLS, "{row}");
            }
        }
    }

    #[test]
    fn types_moves_and_submits() {
        let mut kb = Keyboard::new("");
        // Starts on "q"; Left wraps to "p".
        assert_eq!(kb.handle(Button::A), KbResult::Pending);
        kb.handle(Button::Left);
        kb.handle(Button::A);
        kb.handle(Button::Y);
        kb.handle(Button::X); // upper case
        kb.handle(Button::A);
        kb.handle(Button::R1);
        assert_eq!(kb.text, "qp P.com");
        kb.handle(Button::B);
        assert_eq!(kb.handle(Button::Start), KbResult::Submit("qp P.co".into()));
        kb.handle(Button::L1);
        assert_eq!(kb.text, "");
        assert_eq!(kb.handle(Button::Select), KbResult::Cancel);
    }

    #[test]
    fn cursor_wraps_vertically() {
        let mut kb = Keyboard::new("");
        kb.handle(Button::Up);
        kb.handle(Button::Up);
        assert_eq!(kb.row, ROWS - 1);
    }
}
