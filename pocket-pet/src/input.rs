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
    Start,
    Select,
    /// The MENU button. MinUI convention: quit the app.
    Menu,
}
