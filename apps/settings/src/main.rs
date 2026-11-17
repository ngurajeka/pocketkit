use settings::app::{App, Paths};

fn main() {
    let mut app = App::new(Paths::from_env());
    pocketkit_platform::run("Settings", &mut app);
}
