use settings::app::{App, Paths};

fn main() {
    let mut app = App::new(Paths::from_env());
    minui_common::run("Settings", &mut app);
}
