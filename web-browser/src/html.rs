//! Tiny HTML-to-text converter: keeps readable text and links, drops everything else
//! (scripts, styles, images, layout). Then `layout` word-wraps it to the screen width.

/// One piece of a converted page.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Text, optionally part of link number `link` (an index into `Doc::links`).
    Text { text: String, link: Option<usize> },
    /// End the current line.
    Break,
    /// End the current line and leave one empty line (paragraph gap).
    Blank,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doc {
    pub title: String,
    pub nodes: Vec<Node>,
    /// Raw `href` values, unresolved. `Node::Text::link` indexes into this.
    pub links: Vec<String>,
}

/// Elements whose content is never shown. `title` is captured separately.
const RAW: &[&str] = &[
    "script", "style", "svg", "template", "iframe", "object", "canvas", "noembed", "title",
];
/// Elements that start and end a paragraph.
const BLANK: &[&str] = &[
    "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "dl", "table", "blockquote", "pre",
    "section", "article", "header", "footer", "nav", "main", "aside", "figure", "form",
    "fieldset",
];
/// Elements that start and end a line.
const BREAK: &[&str] = &[
    "div", "tr", "dt", "dd", "li", "figcaption", "address", "center", "details", "summary",
    "option", "caption",
];

impl Doc {
    /// Plain text: one line per source line.
    pub fn from_plain(text: &str) -> Doc {
        let mut b = Builder { pre: 1, ..Builder::new() };
        b.text(text);
        b.doc
    }

    pub fn from_html(html: &str) -> Doc {
        // Same byte offsets as `html` (ASCII lowercasing never changes lengths).
        let lower = html.to_ascii_lowercase();
        let mut b = Builder::new();
        let mut i = 0;
        while i < html.len() {
            let Some(off) = html[i..].find('<') else {
                b.text(&html[i..]);
                break;
            };
            if off > 0 {
                b.text(&html[i..i + off]);
            }
            i += off;
            if html[i..].starts_with("<!--") {
                i = html[i + 4..].find("-->").map_or(html.len(), |e| i + 4 + e + 3);
                continue;
            }
            let next = html[i + 1..].chars().next();
            if !matches!(next, Some(c) if c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')) {
                b.text("<");
                i += 1;
                continue;
            }
            let end = tag_end(html, i);
            let inner = html[i + 1..end].trim_end_matches('>');
            i = end;
            let Some(tag) = parse_tag(inner) else { continue };

            if !tag.closing && !tag.self_closing && RAW.contains(&tag.name.as_str()) {
                let close = format!("</{}", tag.name);
                let (content, next_i) = match lower[i..].find(&close) {
                    Some(p) => (&html[i..i + p], tag_end(html, i + p)),
                    None => (&html[i..], html.len()),
                };
                if tag.name == "title" && b.doc.title.is_empty() {
                    let mut t = String::new();
                    for c in decode_entities(content).chars() {
                        fold_char(c, &mut t);
                    }
                    b.doc.title = t.split_whitespace().collect::<Vec<_>>().join(" ");
                }
                i = next_i;
                continue;
            }
            b.tag(&tag);
        }
        b.doc
    }
}

struct Tag {
    name: String,
    closing: bool,
    self_closing: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Index just past the `>` closing the tag that starts at `start`, honouring quotes.
fn tag_end(html: &str, start: usize) -> usize {
    let bytes = html.as_bytes();
    let mut quote = None;
    for (j, &c) in bytes.iter().enumerate().skip(start + 1) {
        match (quote, c) {
            (None, b'"' | b'\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, b'>') => return j + 1,
            _ => {}
        }
    }
    html.len()
}

/// Parses the inside of `<...>`. Returns `None` for doctypes, `<?xml ...?>` and junk.
fn parse_tag(inner: &str) -> Option<Tag> {
    if inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }
    let closing = inner.starts_with('/');
    let body = inner.trim_start_matches('/');
    let self_closing = body.trim_end().ends_with('/');
    let name_len = body.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(body.len());
    if name_len == 0 {
        return None;
    }
    let name = body[..name_len].to_ascii_lowercase();

    let mut attrs = Vec::new();
    let mut rest = &body[name_len..];
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            break;
        }
        let key_len = rest.find(|c: char| c.is_whitespace() || c == '=' || c == '/').unwrap_or(rest.len());
        let key = rest[..key_len].to_ascii_lowercase();
        rest = rest[key_len..].trim_start();
        let mut value = String::new();
        if let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(q) = r.chars().next().filter(|&c| c == '"' || c == '\'') {
                let end = r[1..].find(q).map_or(r.len(), |e| e + 1);
                value = r[1..end].to_string();
                rest = r.get(end + 1..).unwrap_or("");
            } else {
                let end = r.find(char::is_whitespace).unwrap_or(r.len());
                value = r[..end].to_string();
                rest = &r[end..];
            }
        } else if key_len == 0 {
            // Stray character; skip it so we always make progress.
            rest = &rest[rest.chars().next().map_or(0, char::len_utf8)..];
            continue;
        }
        attrs.push((key, decode_entities(&value)));
    }
    Some(Tag { name, closing, self_closing, attrs })
}

#[derive(Default)]
struct Builder {
    doc: Doc,
    link: Option<usize>,
    link_has_text: bool,
    pending_space: bool,
    /// Nothing written on the current line yet (so a line break would be a no-op).
    at_line_start: bool,
    /// Drop whitespace until the next visible character (after breaks and bullets).
    no_space: bool,
    /// Inside `<pre>`: keep line breaks and spaces.
    pre: usize,
}

impl Builder {
    fn new() -> Self {
        Self { at_line_start: true, no_space: true, ..Self::default() }
    }

    fn emit(&mut self, s: &str, link: Option<usize>) {
        if let Some(Node::Text { text, link: l }) = self.doc.nodes.last_mut() {
            if *l == link {
                text.push_str(s);
                return;
            }
        }
        self.doc.nodes.push(Node::Text { text: s.to_string(), link });
    }

    fn visible(&mut self, c: char) {
        let mut folded = String::new();
        fold_char(c, &mut folded);
        if folded.is_empty() {
            return;
        }
        if self.pending_space && !self.no_space && !self.at_line_start {
            // A space inside a link belongs to it; one before the link text doesn't.
            let l = if self.link_has_text { self.link } else { None };
            self.emit(" ", l);
        }
        self.pending_space = false;
        self.no_space = false;
        self.at_line_start = false;
        self.emit(&folded, self.link);
        if self.link.is_some() {
            self.link_has_text = true;
        }
    }

    fn text(&mut self, raw: &str) {
        for c in decode_entities(raw).chars() {
            if self.pre > 0 {
                match c {
                    '\n' => {
                        self.at_line_start = false; // keep empty lines in preformatted text
                        self.doc.nodes.push(Node::Break);
                    }
                    '\r' => {}
                    ' ' | '\t' => {
                        self.at_line_start = false;
                        self.emit(if c == '\t' { "    " } else { " " }, self.link);
                    }
                    _ => self.visible(c),
                }
            } else if c.is_whitespace() {
                self.pending_space = true;
            } else {
                self.visible(c);
            }
        }
    }

    fn brk(&mut self) {
        if !self.at_line_start {
            self.doc.nodes.push(Node::Break);
            self.at_line_start = true;
        }
        self.pending_space = false;
        self.no_space = true;
    }

    fn blank(&mut self) {
        self.brk();
        if !self.doc.nodes.is_empty() && self.doc.nodes.last() != Some(&Node::Blank) {
            self.doc.nodes.push(Node::Blank);
        }
    }

    fn end_link(&mut self) {
        if self.link.is_some() && !self.link_has_text {
            self.visible('[');
            for c in "link]".chars() {
                self.visible(c);
            }
        }
        self.link = None;
    }

    fn tag(&mut self, tag: &Tag) {
        let name = tag.name.as_str();
        match name {
            "br" => self.brk(),
            "hr" => {
                self.brk();
                for _ in 0..10 {
                    self.visible('-');
                }
                self.brk();
            }
            "pre" => {
                self.blank();
                self.pre = if tag.closing { self.pre.saturating_sub(1) } else { self.pre + 1 };
            }
            "li" if !tag.closing => {
                self.brk();
                self.at_line_start = false;
                self.emit("* ", None);
                self.no_space = true;
            }
            "a" if tag.closing => self.end_link(),
            "a" => {
                self.end_link();
                if let Some(href) = tag.attr("href").filter(|h| !h.trim().is_empty()) {
                    self.doc.links.push(href.trim().to_string());
                    self.link = Some(self.doc.links.len() - 1);
                    self.link_has_text = false;
                }
            }
            "img" => {
                if let Some(alt) = tag.attr("alt").map(str::trim).filter(|a| !a.is_empty()) {
                    self.pending_space = true;
                    self.text(&format!("[{alt}]"));
                    self.pending_space = true;
                }
            }
            "td" | "th" => self.pending_space = true,
            _ if BLANK.contains(&name) => self.blank(),
            _ if BREAK.contains(&name) => self.brk(),
            _ => {}
        }
    }
}

/// Decodes `&amp;`, `&#39;`, `&#x2014;` and the common named entities.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        if let Some(e) = rest[1..].find(';').map(|i| i + 1).filter(|&e| e <= 12) {
            if let Some(c) = entity(&rest[1..e]) {
                out.push(c);
                rest = &rest[e + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(n) = name.strip_prefix('#') {
        let v = match n.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => n.parse().ok()?,
        };
        return char::from_u32(v);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "bull" => '\u{2022}',
        "rarr" => '\u{2192}',
        "larr" => '\u{2190}',
        "euro" => '\u{20ac}',
        "trade" => '\u{2122}',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "middot" => '\u{b7}',
        "deg" => '\u{b0}',
        "times" => '\u{d7}',
        "pound" => '\u{a3}',
        "eacute" => '\u{e9}',
        _ => return None,
    })
}

/// Maps characters the 8x8 font lacks onto ASCII look-alikes. Latin-1 is kept as is.
pub fn fold_char(c: char, out: &mut String) {
    match c {
        '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}' | '\u{ad}' => {}
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' => out.push('\''),
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{2033}' => out.push('"'),
        '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
        '\u{2026}' => out.push_str("..."),
        '\u{2022}' | '\u{2023}' | '\u{25e6}' | '\u{25aa}' | '\u{25cf}' => out.push('*'),
        '\u{2192}' => out.push_str("->"),
        '\u{2190}' => out.push_str("<-"),
        '\u{20ac}' => out.push_str("EUR"),
        '\u{2122}' => out.push_str("TM"),
        '\u{a0}' | '\u{2000}'..='\u{200a}' | '\u{202f}' => out.push(' '),
        c if c.is_control() => {}
        c => out.push(c),
    }
}

/// A run of same-styled text on one screen line.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub text: String,
    pub link: Option<usize>,
}

#[derive(Debug, Default, Clone)]
pub struct Layout {
    pub lines: Vec<Vec<Seg>>,
    /// The first line each link appears on (`None` if it has no visible text).
    pub link_line: Vec<Option<usize>>,
}

/// Word-wraps `doc` to `cols` characters per line.
pub fn layout(doc: &Doc, cols: usize) -> Layout {
    let cols = cols.max(4);
    let mut out = Layout { lines: Vec::new(), link_line: vec![None; doc.links.len()] };
    let mut cur: Vec<Seg> = Vec::new();
    let mut col = 0;

    fn add(cur: &mut Vec<Seg>, col: &mut usize, s: &str, link: Option<usize>) {
        *col += s.chars().count();
        match cur.last_mut() {
            Some(seg) if seg.link == link => seg.text.push_str(s),
            _ => cur.push(Seg { text: s.to_string(), link }),
        }
    }
    fn finish(out: &mut Layout, cur: &mut Vec<Seg>, col: &mut usize) {
        while let Some(last) = cur.last_mut() {
            let trimmed = last.text.trim_end_matches(' ').len();
            last.text.truncate(trimmed);
            if !last.text.is_empty() {
                break;
            }
            cur.pop();
        }
        out.lines.push(std::mem::take(cur));
        *col = 0;
    }

    for node in &doc.nodes {
        match node {
            Node::Break => finish(&mut out, &mut cur, &mut col),
            Node::Blank => {
                if !cur.is_empty() {
                    finish(&mut out, &mut cur, &mut col);
                }
                if out.lines.last().is_some_and(|l| !l.is_empty()) {
                    out.lines.push(Vec::new());
                }
            }
            Node::Text { text, link } => {
                for word in text.split_inclusive(' ') {
                    let (word, space) = match word.strip_suffix(' ') {
                        Some(w) => (w, true),
                        None => (word, false),
                    };
                    let chars: Vec<char> = word.chars().collect();
                    if col + chars.len() > cols && col > 0 {
                        finish(&mut out, &mut cur, &mut col);
                    }
                    for piece in chars.chunks(cols) {
                        if col + piece.len() > cols {
                            finish(&mut out, &mut cur, &mut col);
                        }
                        let s: String = piece.iter().collect();
                        add(&mut cur, &mut col, &s, *link);
                        if let Some(i) = link {
                            out.link_line[*i].get_or_insert(out.lines.len());
                        }
                    }
                    if space && col > 0 && col < cols {
                        add(&mut cur, &mut col, " ", *link);
                    }
                }
            }
        }
    }
    if !cur.is_empty() {
        finish(&mut out, &mut cur, &mut col);
    }
    while out.lines.last().is_some_and(|l| l.is_empty()) {
        out.lines.pop();
    }
    let lead = out.lines.iter().take_while(|l| l.is_empty()).count();
    if lead > 0 {
        out.lines.drain(..lead);
        for line in out.link_line.iter_mut().flatten() {
            *line -= lead;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(l: &Layout, i: usize) -> String {
        l.lines[i].iter().map(|s| s.text.as_str()).collect()
    }

    fn all_text(doc: &Doc) -> String {
        let l = layout(doc, 200);
        (0..l.lines.len()).map(|i| line_text(&l, i)).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn keeps_text_and_links_drops_scripts() {
        let doc = Doc::from_html(
            "<html><head><title> My  Page </title><style>p{}</style></head>\
             <body><script>alert('<p>x</p>')</script><p>Hello <a href=\"/a\">the  world</a>!</p></body>",
        );
        assert_eq!(doc.title, "My Page");
        assert_eq!(doc.links, vec!["/a"]);
        assert_eq!(all_text(&doc), "Hello the world!");
        let link_text: String = doc
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::Text { text, link: Some(0) } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(link_text, "the world");
    }

    #[test]
    fn decodes_entities_and_folds_typography() {
        let doc = Doc::from_html("<p>A &amp; B &lt;3 &#8220;hi&#x201D; caf&eacute; &mdash; ok &bogus; &</p>");
        assert_eq!(all_text(&doc), "A & B <3 \"hi\" caf\u{e9} - ok &bogus; &");
    }

    #[test]
    fn block_tags_break_lines_and_lists_get_bullets() {
        let doc = Doc::from_html("<h1>Title</h1><ul><li>one</li><li> two </li></ul><div>a</div><div>b<br>c</div>");
        assert_eq!(all_text(&doc), "Title\n\n* one\n* two\n\na\nb\nc");
    }

    #[test]
    fn image_alt_and_empty_links() {
        let doc = Doc::from_html("<a href='x.zip'><img src=i.png alt='Get it'></a> <a href=y></a>");
        assert_eq!(doc.links, vec!["x.zip", "y"]);
        assert_eq!(all_text(&doc), "[Get it] [link]");
    }

    #[test]
    fn comments_doctype_and_stray_brackets() {
        let doc = Doc::from_html("<!DOCTYPE html><!-- <a href=no>x</a> -->1 < 2 and 3 > 2");
        assert!(doc.links.is_empty());
        assert_eq!(all_text(&doc), "1 < 2 and 3 > 2");
    }

    #[test]
    fn pre_keeps_line_breaks() {
        let doc = Doc::from_html("<pre>a  b\nc</pre>");
        assert_eq!(all_text(&doc), "a  b\nc");
    }

    #[test]
    fn plain_text_keeps_lines() {
        let doc = Doc::from_plain("line one\nline two\n");
        assert_eq!(all_text(&doc), "line one\nline two");
    }

    #[test]
    fn wraps_words_and_splits_long_ones() {
        let doc = Doc::from_html("<p>the quick brown fox abcdefghijklmnop</p>");
        let l = layout(&doc, 10);
        let lines: Vec<String> = (0..l.lines.len()).map(|i| line_text(&l, i)).collect();
        assert_eq!(lines, vec!["the quick", "brown fox", "abcdefghij", "klmnop"]);
        assert!(lines.iter().all(|s| s.chars().count() <= 10));
    }

    #[test]
    fn records_first_line_of_each_link() {
        let doc = Doc::from_html("<p>intro text here</p><p><a href=a>first</a> and <a href=b>second</a></p>");
        let l = layout(&doc, 12);
        // "intro text" / "here" / "" / "first and" / "second"
        assert_eq!(l.link_line, vec![Some(3), Some(4)]);
    }

    #[test]
    fn survives_truncated_markup() {
        for html in ["<a href=\"x", "<p", "<!-- open", "<script>never closed", "&#xZZ;", "<a href=x>t"] {
            let _ = layout(&Doc::from_html(html), 40);
        }
    }
}
