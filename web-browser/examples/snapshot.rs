//! Renders each screen to `snapshots/*.ppm` so you can check the look without
//! a window or network: `cargo run --example snapshot`.

use std::{fs, path::PathBuf};

use web_browser::{
    app::{parse_bookmarks, App, Config},
    html::Doc,
    input::Button,
    render::{Canvas, H, W},
};

const SAMPLE: &str = "<title>Example News - text edition</title>
<h1>Today's headlines</h1>
<ul><li><a href=/a>Handheld makers ship tiny Linux consoles in record numbers</a></li>
<li><a href=/b>Open-source launcher MinUI gets a new release</a></li>
<li><a href=/c>Why text-only web pages load fast everywhere</a></li></ul>
<p>Download the full report: <a href=/report.pdf>report.pdf</a> (2.1 MB).
Older issues are in the <a href=/archive>archive</a>.</p>
<p>&copy; 2026 Example &mdash; caf&eacute; edition. Lorem ipsum dolor sit amet, consectetur
adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>";

fn save_ppm(name: &str, c: &Canvas) {
    let mut out = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
    for &p in &c.px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    fs::write(format!("snapshots/{name}.ppm"), out).expect("write snapshot");
}

fn main() {
    fs::create_dir_all("snapshots").unwrap();
    let bookmarks = parse_bookmarks(&fs::read_to_string("pak/bookmarks.txt").unwrap_or_default());
    let mut app = App::new(Config { bookmarks, download_dir: PathBuf::from("/mnt/SDCARD/Downloads") });
    let mut c = Canvas::new(W, H);

    app.handle(Button::Down);
    app.render(&mut c);
    save_ppm("home", &c);

    app.open_doc("https://news.example.org/".into(), Doc::from_html(SAMPLE));
    app.handle(Button::Down);
    app.handle(Button::Down);
    app.render(&mut c);
    save_ppm("page", &c);

    app.handle(Button::Select);
    app.flash("Saved report.pdf (2.1 MB)");
    app.render(&mut c);
    save_ppm("page_small", &c);
    app.handle(Button::Select);

    app.handle(Button::X);
    app.handle(Button::L1);
    for b in [Button::A, Button::Right, Button::Down, Button::A] {
        app.handle(b);
    }
    app.render(&mut c);
    save_ppm("keyboard", &c);
    println!("wrote snapshots/{{home,page,page_small,keyboard}}.ppm");
}
