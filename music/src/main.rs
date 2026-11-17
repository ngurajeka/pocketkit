use music::{
    app::{App, Config},
    audio::Sink,
};

#[cfg(feature = "device")]
fn sink() -> Box<dyn Sink> {
    match music::audio::miao::MiAoSink::open() {
        Ok(s) => Box::new(s),
        Err(e) => {
            eprintln!("{e}; playing silently");
            Box::new(music::audio::NullSink::new(true))
        }
    }
}

#[cfg(not(feature = "device"))]
fn sink() -> Box<dyn Sink> {
    Box::new(music::audio::PipeSink::new())
}

fn main() {
    let mut app = App::new(Config::from_env(), sink());
    minui_common::run("Music", &mut app);
    app.shutdown();
}
