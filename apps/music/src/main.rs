use music::{
    app::{App, Config},
    audio::Sink,
};

#[cfg(feature = "minui")]
fn sink() -> Box<dyn Sink> {
    music::audio::device::open()
}

#[cfg(not(feature = "minui"))]
fn sink() -> Box<dyn Sink> {
    Box::new(music::audio::PipeSink::new())
}

fn main() {
    let mut app = App::new(Config::from_env(), sink());
    pocketkit_platform::run("Music", &mut app);
    app.shutdown();
}
