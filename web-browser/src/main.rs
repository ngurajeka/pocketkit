use web_browser::app::{App, Config};

fn main() {
    let mut app = App::new(Config::from_env());
    minui_common::run("Web Browser", &mut app);
    app.shutdown();
}
