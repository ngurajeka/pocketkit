//! Screens (home, page, keyboard), navigation, history and drawing.

use std::{env, fs, path::PathBuf, sync::mpsc::TryRecvError, time::Duration};

use crate::{
    html::{layout, Doc, Layout},
    input::Button,
    keyboard::{KbResult, Keyboard, COLS as KB_COLS, PAGE_NAMES, ROWS as KB_ROWS},
    net::{self, Event, Job, Kind},
    render::{fit, fit_end, Canvas, H, W},
};

const BG: u32 = 0xFAF8F2;
const FG: u32 = 0x24262B;
const DIM: u32 = 0x8A8F98;
const LINK: u32 = 0x1F5FBF;
const SEL_BG: u32 = 0x1F5FBF;
const SEL_FG: u32 = 0xFFFFFF;
const BAR: u32 = 0x2B2D42;
const BAR_FG: u32 = 0xEDF2F4;
const ACCENT: u32 = 0xE07A3F;
const KEY_BG: u32 = 0xE4E1D8;

const MARGIN: usize = 8;
const STATUS_SECS: f32 = 4.0;
const MAX_HISTORY: usize = 30;

fn line_h(scale: usize) -> usize {
    10 * scale
}
/// Text columns that fit on screen at this text size.
pub fn cols(scale: usize) -> usize {
    (W - 2 * MARGIN - 6) / (8 * scale)
}
/// Text rows between the title bar and the status bar.
pub fn rows(scale: usize) -> usize {
    (H - 2 * line_h(scale)) / line_h(scale)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
}

/// `Name | URL` or just `URL` per line; `#` comments and blank lines are skipped.
pub fn parse_bookmarks(text: &str) -> Vec<Bookmark> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (name, url) = match l.split_once('|') {
                Some((n, u)) => (n.trim(), u.trim()),
                None => (l, l),
            };
            let url = net::normalize_input(url)?;
            Some(Bookmark { name: if name.is_empty() { url.clone() } else { name.to_string() }, url })
        })
        .collect()
}

pub struct Config {
    pub bookmarks: Vec<Bookmark>,
    pub download_dir: PathBuf,
}

impl Config {
    /// `BROWSER_BOOKMARKS` (file) and `BROWSER_DOWNLOAD_DIR`, both set by launch.sh.
    pub fn from_env() -> Self {
        let path = env::var("BROWSER_BOOKMARKS").unwrap_or_else(|_| "pak/bookmarks.txt".into());
        let bookmarks = parse_bookmarks(&fs::read_to_string(path).unwrap_or_default());
        let download_dir = env::var_os("BROWSER_DOWNLOAD_DIR").map_or_else(|| PathBuf::from("downloads"), PathBuf::from);
        Self { bookmarks, download_dir }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Page,
    Keyboard,
}

struct Entry {
    url: String,
    doc: Doc,
    scroll: usize,
    selected: Option<usize>,
}

pub struct App {
    pub screen: Screen,
    /// Where the keyboard returns to when cancelled.
    kb_return: Screen,
    pub kb: Keyboard,
    pub home_sel: usize,
    bookmarks: Vec<Bookmark>,
    download_dir: PathBuf,
    history: Vec<Entry>,
    layout: Layout,
    /// Text size: 2 = big (default), 1 = small.
    pub scale: usize,
    job: Option<Job>,
    job_label: String,
    progress: (u64, Option<u64>),
    status: Option<(String, f32)>,
    agent: ureq::Agent,
    time: f32,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            screen: Screen::Home,
            kb_return: Screen::Home,
            kb: Keyboard::new(""),
            home_sel: 0,
            bookmarks: config.bookmarks,
            download_dir: config.download_dir,
            history: Vec::new(),
            layout: Layout::default(),
            scale: 2,
            job: None,
            job_label: String::new(),
            progress: (0, None),
            status: None,
            agent: net::agent(),
            time: 0.0,
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), STATUS_SECS));
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn current(&self) -> Option<&Entry> {
        self.history.last()
    }

    fn relayout(&mut self) {
        let cols = cols(self.scale);
        self.layout = self.history.last().map(|e| layout(&e.doc, cols)).unwrap_or_default();
        let max = self.max_scroll();
        if let Some(e) = self.history.last_mut() {
            e.scroll = e.scroll.min(max);
        }
    }

    fn max_scroll(&self) -> usize {
        self.layout.lines.len().saturating_sub(rows(self.scale))
    }

    /// Shows a fetched page (also used by the snapshot example).
    pub fn open_doc(&mut self, url: String, doc: Doc) {
        self.history.push(Entry { url, doc, scroll: 0, selected: None });
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        self.relayout();
        self.screen = Screen::Page;
    }

    pub fn open_keyboard(&mut self, text: &str) {
        self.kb_return = if self.screen == Screen::Keyboard { self.kb_return } else { self.screen };
        self.kb = Keyboard::new(text);
        self.screen = Screen::Keyboard;
    }

    fn start(&mut self, kind: Kind, url: String) {
        if self.busy() {
            self.flash("Busy - press B to cancel");
            return;
        }
        let host = url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
        self.job_label = match kind {
            Kind::Page => format!("Loading {host}"),
            Kind::Download => format!("Downloading from {host}"),
        };
        self.progress = (0, None);
        self.status = None;
        self.job = Some(net::start(self.agent.clone(), kind, url, self.download_dir.clone()));
    }

    fn cancel_job(&mut self) {
        if let Some(job) = &self.job {
            job.cancel();
            self.job_label = "Cancelling...".into();
        }
    }

    /// Cancels any download and waits briefly so it can delete its `.part` file.
    pub fn shutdown(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
            let _ = job.rx.recv_timeout(Duration::from_secs(2));
        }
    }

    // ---------------------------------------------------------------- input

    pub fn handle(&mut self, button: Button) {
        if button == Button::B && self.busy() && self.screen != Screen::Keyboard {
            self.cancel_job();
            return;
        }
        if button == Button::Select && self.screen != Screen::Keyboard {
            self.toggle_text_size();
            return;
        }
        match self.screen {
            Screen::Home => self.handle_home(button),
            Screen::Page => self.handle_page(button),
            Screen::Keyboard => match self.kb.handle(button) {
                KbResult::Pending => {}
                KbResult::Cancel => self.screen = self.kb_return,
                KbResult::Submit(text) => {
                    self.screen = self.kb_return;
                    if let Some(url) = net::normalize_input(&text) {
                        self.start(Kind::Page, url);
                    }
                }
            },
        }
    }

    fn toggle_text_size(&mut self) {
        // Keep the same part of the page on screen.
        let anchor = self.current().and_then(|e| e.selected.and_then(|i| self.layout.link_line[i]).or(Some(e.scroll)));
        let ratio = anchor.map(|a| a as f32 / self.layout.lines.len().max(1) as f32);
        self.scale = if self.scale == 2 { 1 } else { 2 };
        self.relayout();
        if let (Some(r), true) = (ratio, !self.history.is_empty()) {
            let target = (r * self.layout.lines.len() as f32) as usize;
            let max = self.max_scroll();
            let rows = rows(self.scale);
            let e = self.history.last_mut().unwrap();
            e.scroll = target.saturating_sub(rows / 3).min(max);
        }
        self.flash(if self.scale == 2 { "Big text" } else { "Small text" });
    }

    fn handle_home(&mut self, button: Button) {
        let n = 1 + self.bookmarks.len();
        match button {
            Button::Up => self.home_sel = (self.home_sel + n - 1) % n,
            Button::Down => self.home_sel = (self.home_sel + 1) % n,
            Button::L1 | Button::Left => self.home_sel = self.home_sel.saturating_sub(rows(self.scale)),
            Button::R1 | Button::Right => self.home_sel = (self.home_sel + rows(self.scale)).min(n - 1),
            Button::A if self.home_sel == 0 => self.open_keyboard(""),
            Button::A => {
                let url = self.bookmarks[self.home_sel - 1].url.clone();
                self.start(Kind::Page, url);
            }
            Button::X => self.open_keyboard(""),
            Button::B if !self.history.is_empty() => self.screen = Screen::Page,
            _ => {}
        }
    }

    fn handle_page(&mut self, button: Button) {
        let rows = rows(self.scale);
        match button {
            Button::Down => self.next_link(),
            Button::Up => self.prev_link(),
            Button::Right | Button::R1 => self.scroll_by(rows as isize - 1),
            Button::Left | Button::L1 => self.scroll_by(-(rows as isize - 1)),
            Button::A => self.follow(Kind::Page),
            Button::Y => self.follow(Kind::Download),
            Button::B => {
                self.history.pop();
                if self.history.is_empty() {
                    self.screen = Screen::Home;
                }
                self.relayout();
            }
            Button::X => {
                let url = self.current().map(|e| e.url.clone()).unwrap_or_default();
                self.open_keyboard(&url);
            }
            Button::Start => self.screen = Screen::Home,
            _ => {}
        }
    }

    fn follow(&mut self, kind: Kind) {
        let Some(e) = self.current() else { return };
        let Some(href) = e.selected.and_then(|i| e.doc.links.get(i)) else {
            self.flash("No link selected - use Up/Down");
            return;
        };
        match net::resolve(&e.url, href) {
            Ok(url) if kind == Kind::Page && net::same_page(&url, &e.url) => self.flash("That link is on this page"),
            Ok(url) => self.start(kind, url),
            Err(msg) => self.flash(msg),
        }
    }

    /// Like a text browser: Down selects the next link if it's on screen,
    /// otherwise scrolls half a screen towards it.
    fn next_link(&mut self) {
        let rows = rows(self.scale);
        let Some(e) = self.history.last() else { return };
        let top = e.scroll;
        let start = match e.selected {
            Some(i) if self.on_screen(i) => i + 1,
            _ => 0,
        };
        let next = (start..self.layout.link_line.len())
            .find(|&j| self.layout.link_line[j].is_some_and(|l| l >= top));
        match next {
            Some(j) if self.layout.link_line[j].unwrap() < top + rows => self.select(Some(j)),
            Some(_) | None => self.scroll_by(rows as isize / 2),
        }
    }

    fn prev_link(&mut self) {
        let rows = rows(self.scale);
        let Some(e) = self.history.last() else { return };
        let top = e.scroll;
        let end = match e.selected {
            Some(i) if self.on_screen(i) => i,
            _ => self.layout.link_line.len(),
        };
        let prev = (0..end)
            .rev()
            .find(|&j| self.layout.link_line[j].is_some_and(|l| l < top + rows));
        match prev {
            Some(j) if self.layout.link_line[j].unwrap() >= top => self.select(Some(j)),
            Some(_) | None => self.scroll_by(-(rows as isize / 2)),
        }
    }

    fn on_screen(&self, link: usize) -> bool {
        let top = self.current().map_or(0, |e| e.scroll);
        self.layout.link_line.get(link).copied().flatten().is_some_and(|l| l >= top && l < top + rows(self.scale))
    }

    fn select(&mut self, link: Option<usize>) {
        if let Some(e) = self.history.last_mut() {
            e.selected = link;
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        let max = self.max_scroll();
        if let Some(e) = self.history.last_mut() {
            e.scroll = (e.scroll as isize + delta).clamp(0, max as isize) as usize;
        }
        // Keep the highlight on screen: drop it if it scrolled away.
        if let Some(i) = self.current().and_then(|e| e.selected) {
            if !self.on_screen(i) {
                self.select(None);
            }
        }
    }

    // --------------------------------------------------------------- update

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some((_, t)) = &mut self.status {
            *t -= dt;
            if *t <= 0.0 {
                self.status = None;
            }
        }
        let mut events = Vec::new();
        let mut gone = false;
        if let Some(job) = &self.job {
            loop {
                match job.rx.try_recv() {
                    Ok(ev) => events.push(ev),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        gone = true;
                        break;
                    }
                }
            }
        }
        for ev in events {
            match ev {
                Event::Progress { name, done, total } => {
                    self.job_label = format!("Saving {name}");
                    self.progress = (done, total);
                }
                Event::Page { url, doc, note } => {
                    self.job = None;
                    self.open_doc(url, doc);
                    if let Some(n) = note {
                        self.flash(n);
                    }
                }
                Event::Saved { path, bytes } => {
                    self.job = None;
                    eprintln!("saved {} ({bytes} bytes)", path.display());
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.flash(format!("Saved {name} ({})", net::human_size(bytes)));
                }
                Event::Failed(msg) => {
                    self.job = None;
                    eprintln!("failed: {msg}");
                    self.flash(msg);
                }
            }
        }
        if gone {
            self.job = None;
        }
    }

    // --------------------------------------------------------------- render

    pub fn render(&self, c: &mut Canvas) {
        c.fill(BG);
        match self.screen {
            Screen::Home => self.draw_home(c),
            Screen::Page => self.draw_page(c),
            Screen::Keyboard => self.draw_keyboard(c),
        }
        self.draw_status_bar(c);
    }

    fn draw_title_bar(&self, c: &mut Canvas, title: &str) {
        let s = self.scale;
        c.rect(0, 0, W as i32, line_h(s) as i32, BAR);
        c.text(MARGIN as i32, s as i32, &fit(title, cols(s)), s, BAR_FG);
    }

    fn row_y(&self, row: usize) -> i32 {
        ((row + 1) * line_h(self.scale) + self.scale) as i32
    }

    fn draw_home(&self, c: &mut Canvas) {
        let s = self.scale;
        self.draw_title_bar(c, "Web Browser");
        let rows = rows(s);
        let items: Vec<String> = std::iter::once("> Type an address or search".to_string())
            .chain(self.bookmarks.iter().map(|b| b.name.clone()))
            .collect();
        // Leave the last row for the download folder hint.
        let visible = rows - 1;
        let first = self.home_sel.saturating_sub(visible - 1);
        for (row, (i, item)) in items.iter().enumerate().skip(first).take(visible).enumerate() {
            let y = self.row_y(row);
            let selected = i == self.home_sel;
            if selected {
                c.rect(0, y - s as i32, W as i32, line_h(s) as i32, SEL_BG);
            }
            let color = if selected { SEL_FG } else if i == 0 { ACCENT } else { FG };
            c.text(MARGIN as i32, y, &fit(item, cols(s)), s, color);
        }
        let hint = format!("Saves to {}", self.download_dir.display());
        c.text(MARGIN as i32, self.row_y(rows - 1), &fit_end(&hint, cols(s)), s, DIM);
    }

    fn draw_page(&self, c: &mut Canvas) {
        let s = self.scale;
        let Some(e) = self.current() else { return };
        let title = if e.doc.title.is_empty() { e.url.as_str() } else { e.doc.title.as_str() };
        self.draw_title_bar(c, title);
        let rows = rows(s);
        let cw = (8 * s) as i32;
        for (row, line) in self.layout.lines.iter().skip(e.scroll).take(rows).enumerate() {
            let y = self.row_y(row);
            let mut x = MARGIN as i32;
            for seg in line {
                let width = seg.text.chars().count() as i32 * cw;
                let color = match seg.link {
                    Some(i) if Some(i) == e.selected => {
                        c.rect(x - 1, y - s as i32, width + 2, line_h(s) as i32, SEL_BG);
                        SEL_FG
                    }
                    Some(_) => {
                        c.rect(x, y + 8 * s as i32, width, s as i32, LINK);
                        LINK
                    }
                    None => FG,
                };
                x = c.text(x, y, &seg.text, s, color);
            }
        }
        // Scrollbar.
        let total = self.layout.lines.len();
        if total > rows {
            let top = line_h(s) as i32;
            let track = (rows * line_h(s)) as i32;
            let thumb = (track * rows as i32 / total as i32).max(8);
            let y = top + (track - thumb) * e.scroll as i32 / self.max_scroll().max(1) as i32;
            c.rect(W as i32 - 5, top, 5, track, KEY_BG);
            c.rect(W as i32 - 5, y, 5, thumb, DIM);
        }
    }

    fn draw_keyboard(&self, c: &mut Canvas) {
        let s = self.scale;
        self.draw_title_bar(c, "Type an address or search");
        // Input box (always big text so it's readable).
        let box_y = line_h(s) as i32 + 12;
        c.rect(MARGIN as i32, box_y, (W - 2 * MARGIN) as i32, 28, 0xFFFFFF);
        c.rect(MARGIN as i32, box_y + 27, (W - 2 * MARGIN) as i32, 2, LINK);
        let max_chars = (W - 2 * MARGIN - 16) / 16 - 1;
        let shown = fit_end(&self.kb.text, max_chars);
        let end = c.text(MARGIN as i32 + 6, box_y + 6, &shown, 2, FG);
        if (self.time * 2.0) as i32 % 2 == 0 {
            c.rect(end + 1, box_y + 4, 2, 20, LINK);
        }
        // Keys.
        let (kw, kh, gap) = (56, 44, 4);
        let x0 = (W as i32 - (KB_COLS as i32 * (kw + gap) - gap)) / 2;
        let y0 = box_y + 44;
        for row in 0..KB_ROWS {
            for col in 0..KB_COLS {
                let x = x0 + col as i32 * (kw + gap);
                let y = y0 + row as i32 * (kh + gap);
                let selected = row == self.kb.row && col == self.kb.col;
                c.rect(x, y, kw, kh, if selected { SEL_BG } else { KEY_BG });
                let ch = self.kb.key_at(row, col);
                c.glyph(x + (kw - 16) / 2, y + (kh - 16) / 2, ch, 2, if selected { SEL_FG } else { FG });
            }
        }
        let hints = [
            format!("A type B delete Y space X {}", PAGE_NAMES[(self.kb.page + 1) % PAGE_NAMES.len()]),
            "L1 clear R1 .com START go SELECT back".to_string(),
        ];
        let hy = y0 + KB_ROWS as i32 * (kh + gap) + 8;
        for (i, h) in hints.iter().enumerate() {
            c.text(MARGIN as i32, hy + i as i32 * 20, &fit(h, (W - 2 * MARGIN) / 16), 2, DIM);
        }
    }

    fn draw_status_bar(&self, c: &mut Canvas) {
        let s = self.scale;
        let lh = line_h(s) as i32;
        let y = H as i32 - lh;
        c.rect(0, y, W as i32, lh, BAR);
        let n = cols(s);
        let text = if self.busy() {
            let (done, total) = self.progress;
            let mut t = self.job_label.clone();
            match total {
                Some(total) if total > 0 => {
                    let pct = (done * 100 / total).min(100);
                    c.rect(0, y, (W as u64 * done.min(total) / total) as i32, lh, ACCENT);
                    t = format!("{} {pct}%", fit(&t, n.saturating_sub(5)));
                }
                _ if done > 0 => t = format!("{} {}", fit(&t, n.saturating_sub(10)), net::human_size(done)),
                _ => {}
            }
            let dots = ".".repeat((self.time * 3.0) as usize % 4);
            format!("{}{dots}", fit(&t, n.saturating_sub(4)))
        } else if let Some((msg, _)) = &self.status {
            msg.clone()
        } else {
            match self.screen {
                Screen::Home if !self.history.is_empty() => "A open  X type  B back to page",
                Screen::Home => "A open  X type  SELECT text size",
                Screen::Page => "A open Y save B back X url START home",
                Screen::Keyboard => "",
            }
            .to_string()
        };
        c.text(MARGIN as i32, y + s as i32, &fit(&text, n), s, BAR_FG);
    }
}

impl minui_common::App for App {
    fn handle(&mut self, button: Button) {
        App::handle(self, button);
    }
    fn update(&mut self, dt: f32) {
        App::update(self, dt);
    }
    fn render(&self, canvas: &mut Canvas) {
        App::render(self, canvas);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(html: &str) -> App {
        let mut app = App::new(Config { bookmarks: vec![], download_dir: PathBuf::from("dl") });
        app.open_doc("https://ex.org/".into(), Doc::from_html(html));
        app
    }

    fn selected(app: &App) -> Option<usize> {
        app.current().unwrap().selected
    }

    #[test]
    fn parses_bookmarks() {
        let b = parse_bookmarks("# c\n\nNews | text.npr.org\nhttps://a.org/x\n | b.org\n");
        assert_eq!(
            b,
            vec![
                Bookmark { name: "News".into(), url: "https://text.npr.org".into() },
                Bookmark { name: "https://a.org/x".into(), url: "https://a.org/x".into() },
                Bookmark { name: "https://b.org".into(), url: "https://b.org".into() },
            ]
        );
    }

    #[test]
    fn down_walks_links_then_scrolls() {
        let mut html = String::from("<a href=a>one</a><br><a href=b>two</a>");
        for i in 0..100 {
            html += &format!("<p>filler {i}</p>");
        }
        html += "<a href=c>far</a>";
        let mut app = app_with(&html);
        app.handle(Button::Down);
        assert_eq!(selected(&app), Some(0));
        app.handle(Button::Down);
        assert_eq!(selected(&app), Some(1));
        app.handle(Button::Down); // next link is far away: scroll instead
        assert!(app.current().unwrap().scroll > 0);
        assert_eq!(selected(&app), None);
        for _ in 0..50 {
            app.handle(Button::Down);
        }
        assert_eq!(selected(&app), Some(2));
        app.handle(Button::Up);
        assert_ne!(selected(&app), Some(2));
    }

    #[test]
    fn back_returns_home_after_last_page() {
        let mut app = app_with("<p>hi</p>");
        app.handle(Button::B);
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn keyboard_cancel_returns_to_page() {
        let mut app = app_with("<p>hi</p>");
        app.handle(Button::X);
        assert_eq!(app.screen, Screen::Keyboard);
        assert_eq!(app.kb.text, "https://ex.org/");
        app.handle(Button::Select);
        assert_eq!(app.screen, Screen::Page);
    }

    #[test]
    fn non_web_links_are_refused() {
        let mut app = app_with("<a href='mailto:x@y.z'>mail</a>");
        app.handle(Button::Down);
        app.handle(Button::A);
        assert!(!app.busy());
        assert!(app.status.as_ref().unwrap().0.contains("mailto"));
    }

    #[test]
    fn text_size_toggle_relayouts() {
        let mut app = app_with(&"<p>word </p>".repeat(200));
        let big = app.layout.lines.len();
        app.handle(Button::Select);
        assert_eq!(app.scale, 1);
        assert!(app.layout.lines.len() <= big);
    }

    #[test]
    fn renders_every_screen() {
        let mut app = app_with("<title>T</title><p>Hello <a href=x>link</a></p>");
        let mut c = Canvas::new(W, H);
        app.handle(Button::Down);
        app.render(&mut c);
        app.handle(Button::X);
        app.render(&mut c);
        app.handle(Button::Select); // back to the page without loading anything
        app.handle(Button::Start); // home
        app.render(&mut c);
    }
}
