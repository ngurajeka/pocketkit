/// Logical buttons, named after the Miyoo Mini+ layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Start,
    Select,
    /// The MENU button. MinUI convention: quit the app.
    Menu,
    /// The power button. `run` uses it to switch the screen off and on.
    Power,
}

impl Button {
    /// Buttons that auto-repeat while held (scrolling, moving, deleting).
    pub fn repeats(self) -> bool {
        matches!(self, Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 | Button::L2 | Button::R2 | Button::B)
    }
}
