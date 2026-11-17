//! Networking: fetches pages and downloads files on a background thread so the
//! UI keeps responding (and B can cancel).

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use percent_encoding::percent_decode_str;
use rustls::{
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        WebPkiServerVerifier,
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use url::Url;

use crate::html::Doc;

/// Default search engine (returns plain HTML results). Override with
/// `BROWSER_SEARCH_URL`; the query is appended to it.
pub const SEARCH_URL: &str = "https://search.brave.com/search?q=";

/// `BROWSER_SEARCH_URL` if set, else the engine chosen in Settings.pak, else Brave.
fn search_url() -> String {
    use minui_common::settings::{self, Settings};
    std::env::var("BROWSER_SEARCH_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Settings::load(&settings::default_path()).get(settings::SEARCH_URL).map(str::to_string))
        .unwrap_or_else(|| SEARCH_URL.into())
}
/// Pages bigger than this are cut off (the rest wouldn't be readable on a handheld anyway).
const MAX_PAGE: usize = 3 * 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0 (Linux; Miyoo Mini) MinUIWebBrowser/0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Show it if it's a web page or text, otherwise save it.
    Page,
    /// Always save it.
    Download,
}

#[derive(Debug)]
pub enum Event {
    Progress { name: String, done: u64, total: Option<u64> },
    Page { url: String, doc: Doc, note: Option<String> },
    Saved { path: PathBuf, bytes: u64 },
    Failed(String),
}

pub struct Job {
    pub rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn start(agent: ureq::Agent, kind: Kind, url: String, dir: PathBuf) -> Job {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    thread::spawn(move || {
        let ev = fetch(&agent, kind, &url, &dir, &flag, &tx).unwrap_or_else(Event::Failed);
        let _ = tx.send(ev);
    });
    Job { rx, cancel }
}

// ------------------------------------------------------------------ TLS

/// Build time, used as "now" for certificate checks when the device clock is
/// obviously wrong (the Mini+ clock can reset to 1970 when the battery runs flat,
/// which would make every certificate look "not yet valid").
fn build_time() -> u64 {
    env!("BUILD_UNIX_TIME").parse().unwrap_or(0)
}

fn effective_now(now: UnixTime) -> UnixTime {
    let built = build_time();
    if now.as_secs() < built {
        UnixTime::since_unix_epoch(Duration::from_secs(built))
    } else {
        now
    }
}

/// Normal webpki certificate verification, except for the clock fix above.
#[derive(Debug)]
struct ClockSafeVerifier(Arc<WebPkiServerVerifier>);

impl ServerCertVerifier for ClockSafeVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.0.verify_server_cert(end_entity, intermediates, server_name, ocsp, effective_now(now))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

fn tls_config() -> ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let webpki = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .expect("bundled root certificates are valid");
    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(ClockSafeVerifier(webpki)))
        .with_no_client_auth()
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .tls_config(Arc::new(tls_config()))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .redirects(10)
        .user_agent(USER_AGENT)
        .build()
}

// ------------------------------------------------------------- fetching

fn short_error(e: &ureq::Error) -> String {
    use ureq::ErrorKind::*;
    let msg = e.to_string();
    eprintln!("request failed: {msg}");
    if msg.contains("certificate") {
        return "Site's security certificate is invalid".into();
    }
    if msg.contains("AlertReceived") || msg.contains("HandshakeFailure") {
        return "Site refused the secure connection".into();
    }
    match e.kind() {
        Dns => "Can't find that site. Is Wi-Fi on?".into(),
        ConnectionFailed => "Can't connect. Is Wi-Fi on?".into(),
        TooManyRedirects => "Too many redirects".into(),
        InvalidUrl | UnknownScheme => "That address doesn't look right".into(),
        _ => {
            // Drop the "https://...: " prefix ureq adds.
            msg.rsplit(": ").next().unwrap_or(&msg).to_string()
        }
    }
}

fn is_page_type(ctype: &str) -> bool {
    matches!(ctype, "text/html" | "application/xhtml+xml" | "text/plain")
}

/// Does the request synchronously. `tx` only receives `Progress` events; the
/// final result is returned.
pub fn fetch(
    agent: &ureq::Agent,
    kind: Kind,
    url: &str,
    dir: &Path,
    cancel: &AtomicBool,
    tx: &Sender<Event>,
) -> Result<Event, String> {
    #[allow(clippy::result_large_err)] // ureq 2 returns its Response inside Error::Status
    let call = || agent.get(url).set("Accept", "text/html,text/plain;q=0.9,*/*;q=0.8").call();
    let mut result = call();
    // musl's resolver occasionally fails a lookup with "try again"; it usually works next time.
    for _ in 0..2 {
        match &result {
            Err(e) if e.kind() == ureq::ErrorKind::Dns && !cancel.load(Ordering::Relaxed) => {
                thread::sleep(Duration::from_millis(500));
                result = call();
            }
            _ => break,
        }
    }
    let (resp, status) = match result {
        Ok(r) => (r, None),
        // Error pages are often still worth reading ("404: try the search").
        Err(ureq::Error::Status(code, r)) if kind == Kind::Page && is_page_type(r.content_type()) => {
            (r, Some(format!("HTTP error {code}")))
        }
        Err(ureq::Error::Status(code, _)) => return Err(format!("HTTP error {code}")),
        Err(e) => return Err(short_error(&e)),
    };
    let final_url = resp.get_url().to_string();
    let ctype = resp.content_type().to_ascii_lowercase();

    if kind == Kind::Page && is_page_type(&ctype) {
        let charset = resp.charset().to_ascii_lowercase();
        let mut body = Vec::new();
        let mut reader = resp.into_reader().take(MAX_PAGE as u64 + 1);
        let mut chunk = [0u8; 16 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let n = reader.read(&mut chunk).map_err(|e| format!("Network error: {e}"))?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        let mut note = status;
        if body.len() > MAX_PAGE {
            body.truncate(MAX_PAGE);
            note.get_or_insert_with(|| "Page too long, showing the start".into());
        }
        let text = match charset.as_str() {
            "iso-8859-1" | "latin1" | "windows-1252" => body.iter().map(|&b| b as char).collect(),
            _ => String::from_utf8_lossy(&body).into_owned(),
        };
        let doc = if ctype == "text/plain" { Doc::from_plain(&text) } else { Doc::from_html(&text) };
        return Ok(Event::Page { url: final_url, doc, note });
    }

    // Anything else (or an explicit download): stream it to the SD card.
    let total = resp.header("Content-Length").and_then(|v| v.trim().parse().ok());
    let name = choose_filename(resp.header("Content-Disposition"), &final_url, &ctype);
    fs::create_dir_all(dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
    let path = unique_path(dir, &name);
    let part = part_path(&path);
    let result = (|| {
        let mut file = File::create(&part).map_err(|e| format!("Can't write {}: {e}", part.display()))?;
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        let mut last = Instant::now();
        let _ = tx.send(Event::Progress { name: name.clone(), done, total });
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Download cancelled".to_string());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("Network error: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| format!("Write failed (SD card full?): {e}"))?;
            done += n as u64;
            if last.elapsed() >= Duration::from_millis(200) {
                last = Instant::now();
                let _ = tx.send(Event::Progress { name: name.clone(), done, total });
            }
        }
        file.sync_all().map_err(|e| format!("Write failed: {e}"))?;
        Ok(done)
    })();
    match result {
        Ok(bytes) => {
            fs::rename(&part, &path).map_err(|e| format!("Can't rename download: {e}"))?;
            Ok(Event::Saved { path, bytes })
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

// ---------------------------------------------------------- pure helpers

/// Turns what the user typed into a URL: adds `https://`, or searches if it
/// doesn't look like an address.
pub fn normalize_input(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains("://") {
        return Some(s.to_string());
    }
    if !s.contains(' ') && (s.contains('.') || s.starts_with("localhost")) {
        return Some(format!("https://{s}"));
    }
    let q: String = url::form_urlencoded::byte_serialize(s.as_bytes()).collect();
    Some(format!("{}{q}", search_url()))
}

/// Resolves a link against the page it's on. Only http(s) links can be opened.
pub fn resolve(base: &str, href: &str) -> Result<String, String> {
    let base = Url::parse(base).map_err(|_| "Bad page address".to_string())?;
    let mut url = base.join(href.trim()).map_err(|_| "Bad link".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        "javascript" => return Err("That link needs JavaScript".into()),
        other => return Err(format!("Can't open {other}: links")),
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

/// Same page, ignoring `#fragments`?
pub fn same_page(a: &str, b: &str) -> bool {
    let strip = |s: &str| {
        Url::parse(s).ok().map(|mut u| {
            u.set_fragment(None);
            u.to_string()
        })
    };
    strip(a).is_some() && strip(a) == strip(b)
}

pub fn choose_filename(disposition: Option<&str>, url: &str, ctype: &str) -> String {
    let from_url = || {
        let u = Url::parse(url).ok()?;
        let seg = u.path_segments()?.rfind(|s| !s.is_empty())?.to_string();
        Some(percent_decode_str(&seg).decode_utf8_lossy().into_owned())
    };
    let mut name = sanitize(&disposition.and_then(disposition_name).or_else(from_url).unwrap_or_default());
    if name.is_empty() {
        name = "download".into();
    }
    if !name.contains('.') {
        if ctype.contains("html") {
            name.push_str(".html");
        } else if ctype == "text/plain" {
            name.push_str(".txt");
        }
    }
    name
}

fn disposition_name(header: &str) -> Option<String> {
    let parts: Vec<&str> = header.split(';').map(str::trim).collect();
    let value = |key: &str| {
        parts.iter().find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.trim().eq_ignore_ascii_case(key).then(|| v.trim().trim_matches('"').to_string())
        })
    };
    if let Some(v) = value("filename*") {
        // RFC 5987: UTF-8''file%20name.zip
        let encoded = v.splitn(3, '\'').nth(2).unwrap_or(&v);
        return Some(percent_decode_str(encoded).decode_utf8_lossy().into_owned());
    }
    value("filename")
}

/// Makes a name safe for the SD card's FAT32 file system.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 120 {
        return trimmed.to_string();
    }
    // Keep a short extension when shortening.
    let ext: String = match trimmed.rfind('.') {
        Some(i) if trimmed.len() - i <= 10 => trimmed[i..].to_string(),
        _ => String::new(),
    };
    let keep = 120 - ext.chars().count();
    chars[..keep].iter().collect::<String>() + &ext
}

fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// `dir/name`, or `dir/name-1.ext`, `dir/name-2.ext`, ... if that's taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let free = |p: &Path| !p.exists() && !part_path(p).exists();
    let first = dir.join(name);
    if free(&first) {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    (1..)
        .map(|n| dir.join(format!("{stem}-{n}{ext}")))
        .find(|p| free(p))
        .expect("some name is free")
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_becomes_url_or_search() {
        assert_eq!(normalize_input("  "), None);
        assert_eq!(normalize_input("example.com").unwrap(), "https://example.com");
        assert_eq!(normalize_input("http://x.org/a").unwrap(), "http://x.org/a");
        assert_eq!(normalize_input("miyoo mini & wifi").unwrap(), format!("{SEARCH_URL}miyoo+mini+%26+wifi"));
        assert_eq!(normalize_input("rust").unwrap(), format!("{SEARCH_URL}rust"));
    }

    #[test]
    fn resolves_links() {
        let base = "https://ex.org/dir/page.html#top";
        assert_eq!(resolve(base, "other.html").unwrap(), "https://ex.org/dir/other.html");
        assert_eq!(resolve(base, "/f.zip#x").unwrap(), "https://ex.org/f.zip");
        assert_eq!(resolve(base, "//cdn.ex.org/a").unwrap(), "https://cdn.ex.org/a");
        assert!(resolve(base, "mailto:me@ex.org").is_err());
        assert!(resolve(base, "javascript:void(0)").is_err());
        assert!(same_page(base, "https://ex.org/dir/page.html#other"));
        assert!(!same_page(base, "https://ex.org/dir/"));
    }

    #[test]
    fn picks_download_names() {
        assert_eq!(choose_filename(None, "https://x.org/files/My%20Game.zip?dl=1", "application/zip"), "My Game.zip");
        assert_eq!(
            choose_filename(Some("attachment; filename=\"a:b.bin\""), "https://x.org/get", "application/octet-stream"),
            "a_b.bin"
        );
        assert_eq!(
            choose_filename(Some("attachment; filename=x.bin; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"), "https://x.org/", ""),
            "r\u{e9}sum\u{e9}.pdf"
        );
        assert_eq!(choose_filename(None, "https://x.org/", "text/html"), "download.html");
        assert_eq!(choose_filename(None, "https://x.org/../..", "application/octet-stream"), "download");
    }

    #[test]
    fn sanitizes_and_shortens() {
        assert_eq!(sanitize("../evil/..name.txt  "), "_evil_..name.txt");
        let long = format!("{}.zip", "a".repeat(300));
        let s = sanitize(&long);
        assert_eq!(s.chars().count(), 120);
        assert!(s.ends_with(".zip"));
    }

    #[test]
    fn avoids_overwriting() {
        let dir = std::env::temp_dir().join(format!("wb_unique_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.zip"), dir.join("a.zip"));
        fs::write(dir.join("a.zip"), "x").unwrap();
        fs::write(dir.join("a-1.zip.part"), "x").unwrap();
        assert_eq!(unique_path(&dir, "a.zip"), dir.join("a-2.zip"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wrong_clock_uses_build_time() {
        let zero = UnixTime::since_unix_epoch(Duration::ZERO);
        assert_eq!(effective_now(zero).as_secs(), build_time());
        let future = UnixTime::since_unix_epoch(Duration::from_secs(build_time() + 100));
        assert_eq!(effective_now(future).as_secs(), build_time() + 100);
    }

    #[test]
    fn tls_config_builds() {
        let _ = agent();
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(5 * 1024 * 1024 + 1024 * 512), "5.5 MB");
    }
}
