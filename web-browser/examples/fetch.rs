//! Fetches a page the way the device would and prints it as laid out on screen,
//! with link numbers: `cargo run --example fetch -- https://text.npr.org`.
//! A non-page URL (or `--save`) is downloaded into ./downloads instead.

use std::{path::Path, sync::{atomic::AtomicBool, mpsc}};

use web_browser::{
    app::cols,
    html::layout,
    net::{self, Event, Kind},
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let save = args.iter().any(|a| a == "--save");
    let input = args.iter().find(|a| *a != "--save").expect("usage: fetch [--save] <url or search>");
    let url = net::normalize_input(input).expect("empty input");
    let kind = if save { Kind::Download } else { Kind::Page };
    let (tx, _rx) = mpsc::channel();
    match net::fetch(&net::agent(), kind, &url, Path::new("downloads"), &AtomicBool::new(false), &tx) {
        Ok(Event::Page { url, doc, note }) => {
            println!("== {} <{url}> {}", doc.title, note.unwrap_or_default());
            let l = layout(&doc, cols(2));
            for line in &l.lines {
                let text: String = line
                    .iter()
                    .map(|s| match s.link {
                        Some(i) => format!("[{i}]{}", s.text),
                        None => s.text.clone(),
                    })
                    .collect();
                println!("{text}");
            }
            println!("== {} lines, {} links", l.lines.len(), doc.links.len());
        }
        Ok(Event::Saved { path, bytes }) => println!("saved {} ({})", path.display(), net::human_size(bytes)),
        Ok(other) => println!("{other:?}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
