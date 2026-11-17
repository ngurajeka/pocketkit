//! Screens (home, page, keyboard), navigation, history and drawing.

use std::{env, fs, path::PathBuf, sync::mpsc::TryRecvError, time::Duration};

use pocketkit_core::ui::{self, Ui};

use crate::{
    html::{layout, Doc, Layout},
    input::Button,
    keyboard::{KbResult, Keyboard},
    net::{self, Event, Job, Kind},
    render::{Canvas, W},
};

const STATUS_SECS: f32 = 4.0;
const MAX_HISTORY: usize = 30;
/// Page text sizes (MinUI's "small" and "tiny"); SELECT switches.
const TEXT_NORMAL: f32 = ui::FONT_SMALL;
const TEXT_SMALL: f32 = ui::FONT_TINY;
/// Room on the right of the page for the scrollbar.
const SCROLLBAR_W: i32 = 10;
/// Home list rows shown at once.
const HOME_ROWS: usize = 5;

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
    /// SELECT toggles a smaller page font.
    pub small_text: bool,
    job: Option<Job>,
    job_label: String,
    progress: (u64, Option<u64>),
    status: Option<(String, f32)>,
    agent: ureq::Agent,
    time: f32,
    quitting: bool,
    ui: Ui,
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
            small_text: false,
            job: None,
            job_label: String::new(),
            progress: (0, None),
            status: None,
            agent: net::agent(),
            time: 0.0,
            quitting: false,
            ui: Ui::new(),
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

    fn text_size(&self) -> f32 {
        if self.small_text {
            TEXT_SMALL
        } else {
            TEXT_NORMAL
        }
    }

    fn line_h(&self) -> i32 {
        self.ui.font.height(self.text_size()) + 6
    }

    /// Page text area: below the title pill, above the hints.
    fn body(&self) -> (i32, i32, i32, i32) {
        let top = ui::CONTENT_TOP;
        (ui::PADDING, top, W as i32 - 2 * ui::PADDING - SCROLLBAR_W, ui::HINTS_TOP - ui::PADDING / 2 - top)
    }

    /// Lines of page text that fit on screen.
    pub fn page_rows(&self) -> usize {
        (self.body().3 / self.line_h()).max(1) as usize
    }

    fn relayout(&mut self) {
        let size = self.text_size();
        let width = self.body().2 as usize;
        let font = &self.ui.font;
        let measure = |s: &str| font.measure(s, size) as usize;
        let new = self.history.last().map(|e| layout(&e.doc, width, &measure)).unwrap_or_default();
        self.layout = new;
        let max = self.max_scroll();
        if let Some(e) = self.history.last_mut() {
            e.scroll = e.scroll.min(max);
        }
    }

    fn max_scroll(&self) -> usize {
        self.layout.lines.len().saturating_sub(self.page_rows())
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
        self.small_text = !self.small_text;
        self.relayout();
        if let (Some(r), true) = (ratio, !self.history.is_empty()) {
            let target = (r * self.layout.lines.len() as f32) as usize;
            let max = self.max_scroll();
            let rows = self.page_rows();
            let e = self.history.last_mut().unwrap();
            e.scroll = target.saturating_sub(rows / 3).min(max);
        }
        self.flash(if self.small_text { "Small text" } else { "Normal text" });
    }

    fn handle_home(&mut self, button: Button) {
        let n = 1 + self.bookmarks.len();
        match button {
            Button::Up => self.home_sel = (self.home_sel + n - 1) % n,
            Button::Down => self.home_sel = (self.home_sel + 1) % n,
            Button::L1 | Button::Left => self.home_sel = self.home_sel.saturating_sub(HOME_ROWS),
            Button::R1 | Button::Right => self.home_sel = (self.home_sel + HOME_ROWS).min(n - 1),
            Button::A if self.home_sel == 0 => self.open_keyboard(""),
            Button::A => {
                let url = self.bookmarks[self.home_sel - 1].url.clone();
                self.start(Kind::Page, url);
            }
            Button::X => self.open_keyboard(""),
            Button::B if !self.history.is_empty() => self.screen = Screen::Page,
            Button::B => self.quitting = true,
            _ => {}
        }
    }

    fn handle_page(&mut self, button: Button) {
        let rows = self.page_rows();
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
        let rows = self.page_rows();
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
        let rows = self.page_rows();
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
        self.layout.link_line.get(link).copied().flatten().is_some_and(|l| l >= top && l < top + self.page_rows())
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
        match self.screen {
            Screen::Home => self.draw_home(c),
            Screen::Page => self.draw_page(c),
            Screen::Keyboard => self.ui.keyboard(c, &self.kb, "Address or search", "GO", self.time),
        }
        if self.busy() {
            let (done, total) = self.progress;
            let dots = ".".repeat((self.time * 3.0) as usize % 4);
            let (text, frac) = match total {
                Some(t) if t > 0 => (format!("{}  {}%", self.job_label, (done * 100 / t).min(100)), Some(done as f32 / t as f32)),
                _ if done > 0 => (format!("{}  {}", self.job_label, net::human_size(done)), None),
                _ => (format!("{}{dots}", self.job_label), None),
            };
            self.ui.progress(c, &text, frac);
        } else if let Some((msg, _)) = &self.status {
            self.ui.toast(c, msg);
        }
    }

    fn draw_home(&self, c: &mut Canvas) {
        self.ui.clear(c);
        self.ui.title(c, "Web Browser");
        let items: Vec<&str> =
            std::iter::once("Type an address or search").chain(self.bookmarks.iter().map(|b| b.name.as_str())).collect();
        let first = self.home_sel.saturating_sub(HOME_ROWS - 1);
        for (row, (i, item)) in items.iter().enumerate().skip(first).take(HOME_ROWS).enumerate() {
            self.ui.row(c, row, item, i == self.home_sel, None, false);
        }
        self.ui.scroll_marks(c, first, HOME_ROWS, items.len());
        if self.history.is_empty() {
            self.ui.hints(c, &[("B", "QUIT")], false);
        } else {
            self.ui.hints(c, &[("B", "PAGE")], false);
        }
        self.ui.hints(c, &[("X", "TYPE"), ("A", "OPEN")], true);
    }

    fn draw_page(&self, c: &mut Canvas) {
        self.ui.clear(c);
        let Some(e) = self.current() else { return };
        let title = if e.doc.title.is_empty() { e.url.as_str() } else { e.doc.title.as_str() };
        self.ui.title(c, title);
        let size = self.text_size();
        let font = &self.ui.font;
        let (bx, by, _, bh) = self.body();
        let lh = self.line_h();
        let th = font.height(size);
        let rows = self.page_rows();
        for (row, line) in self.layout.lines.iter().skip(e.scroll).take(rows).enumerate() {
            let y = by + row as i32 * lh + (lh - th) / 2;
            let mut x = bx;
            for seg in line {
                let w = font.measure(&seg.text, size);
                let color = match seg.link {
                    Some(i) if Some(i) == e.selected => {
                        ui::rounded_rect(c, x - 6, y - 3, w + 12, th + 6, 8, ui::WHITE);
                        ui::BLACK
                    }
                    Some(_) => {
                        c.rect(x, y + th - 2, w, 2, ui::LIGHT_GRAY);
                        ui::WHITE
                    }
                    None => ui::LIGHT_TEXT,
                };
                font.draw(c, x, y, &seg.text, size, color);
                x += w;
            }
        }
        let total = self.layout.lines.len();
        if total > rows {
            let x = W as i32 - ui::PADDING / 2 - 6;
            let thumb = (bh * rows as i32 / total as i32).max(24);
            let ty = by + (bh - thumb) * e.scroll as i32 / self.max_scroll().max(1) as i32;
            ui::rounded_rect(c, x, by, 6, bh, 3, ui::DARK_GRAY);
            ui::rounded_rect(c, x, ty, 6, thumb, 3, ui::GRAY);
        }
        self.ui.hints(c, &[("B", "BACK"), ("X", "URL")], false);
        if e.selected.is_some() {
            self.ui.hints(c, &[("Y", "SAVE"), ("A", "OPEN")], true);
        } else {
            self.ui.hints(c, &[("START", "HOME")], true);
        }
    }
}

impl pocketkit_core::App for App {
    fn handle(&mut self, button: Button) {
        App::handle(self, button);
    }
    fn update(&mut self, dt: f32) {
        App::update(self, dt);
    }
    fn render(&self, canvas: &mut Canvas) {
        App::render(self, canvas);
    }
    fn quit(&self) -> bool {
        self.quitting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::H;

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
        // Enough half-page scrolls to reach the end at any text size.
        for _ in 0..200 {
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
        assert!(app.small_text);
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
