//! Merge-writer for XMP sidecars.
//!
//! The sidecar is shared with Lightroom (and Capture One), so this edits the
//! text in place instead of re-serialising it: quick-xml only locates byte
//! spans, and the handful of fields PhotoSift owns are spliced in or out.
//! Everything else, including every `crs:*` setting, keeps its exact bytes.
//!
//! Only the first `rdf:Description` is edited; that is where Lightroom keeps
//! `xmp:Rating`, `xmp:Label` and the keyword bags.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use quick_xml::{events::Event, Reader};

use crate::outcome::{XmpFields, BLOOPER_KEYWORD, DELETE_LABEL};

const NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const NS_XMP: &str = "http://ns.adobe.com/xap/1.0/";
const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
const NS_LR: &str = "http://ns.adobe.com/lightroom/1.0/";
/// PhotoSift's own namespace; `photosift:LabelSet="1"` records that we set the label.
pub const NS_PHOTOSIFT: &str = "http://photosift.app/ns/1.0/";

const NEW_SIDECAR: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"PhotoSift\">\n \
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  \
<rdf:Description rdf:about=\"\"/>\n \
</rdf:RDF>\n\
</x:xmpmeta>\n";

/// Serialises sidecar edits so two requests for one photo cannot interleave
/// their read-modify-write and drop each other's change.
static APPLY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, thiserror::Error)]
pub enum XmpError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("malformed XMP sidecar {}: {reason}", path.display())]
    Malformed { path: PathBuf, reason: String },
    /// `Settings::read_only` (the eval harness): nothing was read or written.
    #[error("read-only")]
    ReadOnly,
}

/// `<stem>.xmp` beside the image.
pub fn sidecar_path(image_path: &Path) -> PathBuf {
    image_path.with_extension("xmp")
}

/// Applies `fields` to the sidecar of `image_path`, creating it if there is
/// something to write. The file is only rewritten when its content changes.
/// `read_only` (`Settings::read_only`) refuses before touching anything.
pub fn apply(image_path: &Path, fields: &XmpFields, read_only: bool) -> Result<(), XmpError> {
    if read_only {
        return Err(XmpError::ReadOnly);
    }
    let path = sidecar_path(image_path);
    let io_err = |source| XmpError::Io {
        path: path.clone(),
        source,
    };
    let _guard = APPLY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let existing = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_err(e)),
    };
    let (bom, original) = match existing {
        Some(bytes) => {
            let text = String::from_utf8(bytes).map_err(|_| XmpError::Malformed {
                path: path.clone(),
                reason: "not valid UTF-8".into(),
            })?;
            match text.strip_prefix('\u{feff}') {
                Some(rest) => ("\u{feff}", rest.to_string()),
                None => ("", text),
            }
        }
        None => {
            let creates = fields.rating.is_some()
                || fields.label.is_some()
                || !fields.add_keywords.is_empty();
            if !creates {
                return Ok(());
            }
            ("", NEW_SIDECAR.to_string())
        }
    };
    let merged = merge(&original, fields).map_err(|reason| XmpError::Malformed {
        path: path.clone(),
        reason,
    })?;
    if merged == original {
        return Ok(());
    }
    write_atomic(&path, &format!("{bom}{merged}")).map_err(io_err)
}

fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.psift-tmp"));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

// ---------------------------------------------------------------------------
// Scanning: locate the byte spans we may edit.
// ---------------------------------------------------------------------------

type Span = (usize, usize);

struct Attr {
    name: String,
    value: String,
    span: Span,
    value_span: Span,
}

struct Tag {
    attrs: Vec<Attr>,
    /// End of the last attribute (or of the tag name): where new attributes go.
    attrs_end: usize,
}

/// Parses the attributes of a start tag at `doc[span]`. Values are taken
/// verbatim; the ones we read (ratings, labels, namespace URIs) need no unescaping.
fn parse_tag(doc: &str, span: Span) -> Option<Tag> {
    let b = doc.as_bytes();
    let (start, end) = span;
    let mut i = start + 1;
    while i < end && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let mut attrs = Vec::new();
    let mut attrs_end = i;
    loop {
        while i < end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end || b[i] == b'>' || b[i] == b'/' {
            break;
        }
        let name_start = i;
        while i < end && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let attr_name = doc[name_start..i].to_string();
        while i < end && (b[i].is_ascii_whitespace() || b[i] == b'=') {
            i += 1;
        }
        let quote = *b.get(i)?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let value_start = i + 1;
        let value_end = value_start + doc[value_start..end].find(quote as char)?;
        i = value_end + 1;
        attrs_end = i;
        attrs.push(Attr {
            name: attr_name,
            value: doc[value_start..value_end].to_string(),
            span: (name_start, i),
            value_span: (value_start, value_end),
        });
    }
    Some(Tag { attrs, attrs_end })
}

/// `xmp:Rating` / `xmp:Label`: simple text children, when not written as attributes.
struct Simple {
    outer: Span,
    inner: Option<Span>,
}

struct Li {
    text: String,
    span: Span,
}

enum Container {
    None,
    Open { close_start: usize },
    SelfClosed { name: String, span: Span },
}

/// `dc:subject` / `lr:hierarchicalSubject`.
struct Bag {
    name: String,
    outer: Span,
    inner: Option<Span>,
    container: Container,
    items: Vec<Li>,
}

struct Scan {
    decls: Vec<(String, String)>,
    desc_name: String,
    desc_tag: Span,
    desc: Tag,
    desc_close: Option<usize>,
    first_child: Option<usize>,
    rating: Option<Simple>,
    label: Option<Simple>,
    subject: Option<Bag>,
    hier: Option<Bag>,
    rdf_prefix: String,
}

fn find_prefix(decls: &[(String, String)], uri: &str) -> Option<String> {
    decls.iter().find(|(_, u)| u == uri).map(|(p, _)| p.clone())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Rating,
    Label,
    Subject,
    Hier,
}

struct Open {
    kind: Kind,
    name: String,
    outer_start: usize,
    inner_start: usize,
    container: Container,
    items: Vec<Li>,
}

/// True for the fields PhotoSift writes (including its own bookkeeping), given
/// the namespaces in scope. Used to refuse sidecars that keep them in a
/// second rdf:Description, where a merge into the first would duplicate them.
fn is_owned_field(scope: &[(String, String)], qname: &str) -> bool {
    let Some((prefix, local)) = qname.split_once(':') else {
        return false;
    };
    let Some((_, uri)) = scope.iter().find(|(p, _)| p == prefix) else {
        return false;
    };
    matches!(
        (uri.as_str(), local),
        (NS_XMP, "Rating" | "Label")
            | (NS_DC, "subject")
            | (NS_LR, "hierarchicalSubject")
            | (NS_PHOTOSIFT, "LabelSet" | "BlooperSet")
    )
}

fn owned_elsewhere(qname: &str) -> String {
    format!("{qname} is in a second rdf:Description; refusing to edit a sidecar split that way")
}

fn scan(doc: &str) -> Result<Scan, String> {
    let mut reader = Reader::from_str(doc);
    let mut decls: Vec<(String, String)> = Vec::new();
    let mut depth = 0usize;
    let mut found: Option<(usize, String, Span, Tag)> = None; // (depth, name, tag span, tag)
    let mut in_desc = false;
    let mut desc_close = None;
    let mut first_child = None;
    let mut names: [Option<(Kind, String)>; 4] = Default::default();
    let mut rdf_prefix = String::from("rdf");
    let (mut rating, mut label, mut subject, mut hier) = (None, None, None, None);
    let mut cur: Option<Open> = None;
    let mut li_open: Option<(usize, String)> = None;
    // Inside a later rdf:Description: (its depth, namespaces in scope).
    let mut other: Option<(usize, Vec<(String, String)>)> = None;

    loop {
        let before = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if found.is_none() {
                    let tag = parse_tag(doc, (before, after))
                        .ok_or_else(|| format!("unparseable tag <{name}>"))?;
                    for a in &tag.attrs {
                        if let Some(p) = a.name.strip_prefix("xmlns:") {
                            decls.push((p.to_string(), a.value.clone()));
                        }
                    }
                    let want = format!(
                        "{}:Description",
                        find_prefix(&decls, NS_RDF).unwrap_or_else(|| "rdf".into())
                    );
                    if name == want {
                        rdf_prefix = want.trim_end_matches(":Description").to_string();
                        let p = |uri, local: &str| {
                            find_prefix(&decls, uri).map(|p| format!("{p}:{local}"))
                        };
                        for (slot, n, k) in [
                            (0, p(NS_XMP, "Rating"), Kind::Rating),
                            (1, p(NS_XMP, "Label"), Kind::Label),
                            (2, p(NS_DC, "subject"), Kind::Subject),
                            (3, p(NS_LR, "hierarchicalSubject"), Kind::Hier),
                        ] {
                            names[slot] = n.map(|n| (k, n));
                        }
                        found = Some((depth, name.clone(), (before, after), tag));
                        in_desc = !empty;
                    }
                } else if in_desc {
                    let desc_depth = found.as_ref().map_or(0, |f| f.0);
                    match depth - desc_depth {
                        1 => {
                            first_child.get_or_insert(before);
                            let tracked = names.iter().flatten().find(|(_, n)| *n == name);
                            if let Some((kind, _)) = tracked {
                                if empty {
                                    match kind {
                                        Kind::Rating => {
                                            rating = Some(Simple {
                                                outer: (before, after),
                                                inner: None,
                                            })
                                        }
                                        Kind::Label => {
                                            label = Some(Simple {
                                                outer: (before, after),
                                                inner: None,
                                            })
                                        }
                                        Kind::Subject | Kind::Hier => {
                                            let bag = Bag {
                                                name: name.clone(),
                                                outer: (before, after),
                                                inner: None,
                                                container: Container::None,
                                                items: vec![],
                                            };
                                            if *kind == Kind::Subject {
                                                subject = Some(bag)
                                            } else {
                                                hier = Some(bag)
                                            }
                                        }
                                    }
                                } else {
                                    cur = Some(Open {
                                        kind: *kind,
                                        name: name.clone(),
                                        outer_start: before,
                                        inner_start: after,
                                        container: Container::None,
                                        items: vec![],
                                    });
                                }
                            }
                        }
                        2 => {
                            if let Some(c) = cur.as_mut() {
                                c.container = if empty {
                                    Container::SelfClosed {
                                        name: name.clone(),
                                        span: (before, after),
                                    }
                                } else {
                                    Container::Open { close_start: 0 }
                                };
                            }
                        }
                        3 if cur.is_some() && !empty && name == format!("{rdf_prefix}:li") => {
                            li_open = Some((before, String::new()));
                        }
                        _ => {}
                    }
                } else if let Some((other_depth, scope)) = other.as_ref() {
                    if depth == other_depth + 1 && is_owned_field(scope, &name) {
                        return Err(owned_elsewhere(&name));
                    }
                } else if name == found.as_ref().map_or("", |f| f.1.as_str()) {
                    let tag = parse_tag(doc, (before, after))
                        .ok_or_else(|| format!("unparseable tag <{name}>"))?;
                    let mut scope = decls.clone();
                    for a in &tag.attrs {
                        if let Some(p) = a.name.strip_prefix("xmlns:") {
                            scope.push((p.to_string(), a.value.clone()));
                        }
                    }
                    if let Some(a) = tag.attrs.iter().find(|a| is_owned_field(&scope, &a.name)) {
                        return Err(owned_elsewhere(&a.name));
                    }
                    if !empty {
                        other = Some((depth, scope));
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::Text(ref t) => {
                if let Some((_, text)) = li_open.as_mut() {
                    text.push_str(&t.unescape().map_err(|e| e.to_string())?);
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or("unbalanced end tag")?;
                if other.as_ref().is_some_and(|(d, _)| *d == depth) {
                    other = None;
                }
                if let Some((desc_depth, ..)) = found.as_ref() {
                    if in_desc {
                        match depth - desc_depth {
                            0 => {
                                desc_close = Some(before);
                                in_desc = false;
                            }
                            1 => {
                                if let Some(c) = cur.take() {
                                    let inner = Some((c.inner_start, before));
                                    let outer = (c.outer_start, after);
                                    match c.kind {
                                        Kind::Rating => rating = Some(Simple { outer, inner }),
                                        Kind::Label => label = Some(Simple { outer, inner }),
                                        Kind::Subject | Kind::Hier => {
                                            let bag = Bag {
                                                name: c.name,
                                                outer,
                                                inner,
                                                container: c.container,
                                                items: c.items,
                                            };
                                            if c.kind == Kind::Subject {
                                                subject = Some(bag)
                                            } else {
                                                hier = Some(bag)
                                            }
                                        }
                                    }
                                }
                            }
                            2 => {
                                if let Some(Open {
                                    container: Container::Open { close_start },
                                    ..
                                }) = cur.as_mut()
                                {
                                    *close_start = before;
                                }
                            }
                            3 => {
                                if let (Some(c), Some((start, text))) =
                                    (cur.as_mut(), li_open.take())
                                {
                                    c.items.push(Li {
                                        text,
                                        span: (start, after),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        return Err("document ends inside an open element".into());
    }
    let (_, desc_name, desc_tag, desc) = found.ok_or("no rdf:Description")?;
    Ok(Scan {
        decls,
        desc_name,
        desc_tag,
        desc,
        desc_close,
        first_child,
        rating,
        label,
        subject,
        hier,
        rdf_prefix,
    })
}

// ---------------------------------------------------------------------------
// Merging
// ---------------------------------------------------------------------------

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

/// Whitespace-only text between the start of `pos`'s line and `pos`.
fn indent_at(doc: &str, pos: usize) -> Option<&str> {
    let line_start = doc[..pos].rfind('\n').map_or(0, |i| i + 1);
    let pre = &doc[line_start..pos];
    pre.chars().all(|c| c == ' ' || c == '\t').then_some(pre)
}

/// Extends a removal backwards over the element's own indentation and line break.
fn with_leading_line(doc: &str, (start, end): Span) -> Span {
    let b = doc.as_bytes();
    let mut s = start;
    while s > 0 && (b[s - 1] == b' ' || b[s - 1] == b'\t') {
        s -= 1;
    }
    if s > 0 && b[s - 1] == b'\n' {
        s -= 1;
        if s > 0 && b[s - 1] == b'\r' {
            s -= 1;
        }
        (s, end)
    } else {
        (start, end)
    }
}

fn with_leading_space(doc: &str, (start, end): Span) -> Span {
    let b = doc.as_bytes();
    let mut s = start;
    while s > 0 && b[s - 1].is_ascii_whitespace() {
        s -= 1;
    }
    (s, end)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn leaf(keyword: &str) -> &str {
    keyword.rsplit('|').next().unwrap_or(keyword)
}

struct Merge<'a> {
    doc: &'a str,
    sc: &'a Scan,
    nl: &'static str,
    edits: Vec<Edit>,
    new_attrs: Vec<String>,
    decls: Vec<(String, String)>,
    new_decls: Vec<(String, String)>,
}

impl Merge<'_> {
    fn edit(&mut self, (start, end): Span, text: impl Into<String>) {
        self.edits.push(Edit {
            start,
            end,
            text: text.into(),
        });
    }

    fn prefix(&mut self, uri: &str, preferred: &str) -> Result<String, String> {
        if let Some(p) = find_prefix(&self.decls, uri) {
            return Ok(p);
        }
        if self.decls.iter().any(|(p, _)| p == preferred) {
            return Err(format!(
                "prefix {preferred:?} is bound to another namespace"
            ));
        }
        self.decls.push((preferred.into(), uri.into()));
        self.new_decls.push((preferred.into(), uri.into()));
        Ok(preferred.into())
    }

    fn attr(&self, qname: &str) -> Option<&Attr> {
        self.sc.desc.attrs.iter().find(|a| a.name == qname)
    }

    fn set_attr(&mut self, qname: &str, value: &str) {
        match self.attr(qname) {
            Some(a) if a.value == value => {}
            Some(a) => {
                let span = a.value_span;
                self.edit(span, value);
            }
            None => self.new_attrs.push(format!("{qname}=\"{value}\"")),
        }
    }

    fn remove_attr(&mut self, qname: &str) {
        if let Some(a) = self.attr(qname) {
            let span = with_leading_space(self.doc, a.span);
            self.edit(span, "");
        }
    }

    fn simple_text(&self, s: &Simple) -> String {
        s.inner
            .map(|(a, b)| self.doc[a..b].trim().to_string())
            .unwrap_or_default()
    }

    fn rating(&mut self, want: i32) -> Result<(), String> {
        let existing = find_prefix(&self.decls, NS_XMP);
        let attr_name = existing.as_ref().map(|p| format!("{p}:Rating"));
        let attr_value = attr_name
            .as_deref()
            .and_then(|n| self.attr(n))
            .map(|a| a.value.clone());
        let child = self.sc.rating.as_ref();
        let current = attr_value
            .clone()
            .or_else(|| child.map(|c| self.simple_text(c)))
            .and_then(|v| v.trim().parse::<i32>().ok());
        if current == Some(5) || current == Some(want) {
            return Ok(());
        }
        if attr_value.is_some() {
            if let Some(n) = attr_name {
                self.set_attr(&n, &want.to_string());
            }
        } else if let Some(c) = child {
            match c.inner {
                Some(inner) => self.edit(inner, want.to_string()),
                None => {
                    let name = format!("{}:Rating", existing.unwrap_or_default());
                    self.edit(c.outer, format!("<{name}>{want}</{name}>"));
                }
            }
        } else {
            let p = self.prefix(NS_XMP, "xmp")?;
            self.set_attr(&format!("{p}:Rating"), &want.to_string());
        }
        Ok(())
    }

    fn label(&mut self, want: Option<&str>) -> Result<(), String> {
        let xmp = find_prefix(&self.decls, NS_XMP);
        let attr_name = xmp.as_ref().map(|p| format!("{p}:Label"));
        let attr_value = attr_name
            .as_deref()
            .and_then(|n| self.attr(n))
            .map(|a| a.value.clone());
        let child = self.sc.label.as_ref();
        let current = attr_value
            .clone()
            .or_else(|| child.map(|c| self.simple_text(c)))
            .filter(|v| !v.trim().is_empty());
        let ps = find_prefix(&self.decls, NS_PHOTOSIFT);
        let set_name = ps.as_ref().map(|p| format!("{p}:LabelSet"));
        let label_set = set_name
            .as_deref()
            .and_then(|n| self.attr(n))
            .is_some_and(|a| a.value == "1");

        match want {
            Some(label) if current.is_none() => {
                if let (Some(c), None) = (child, &attr_value) {
                    match c.inner {
                        Some(inner) => self.edit(inner, label),
                        None => {
                            let name = format!("{}:Label", xmp.unwrap_or_default());
                            self.edit(c.outer, format!("<{name}>{label}</{name}>"));
                        }
                    }
                } else {
                    let p = self.prefix(NS_XMP, "xmp")?;
                    self.set_attr(&format!("{p}:Label"), label);
                }
                let p = self.prefix(NS_PHOTOSIFT, "photosift")?;
                self.set_attr(&format!("{p}:LabelSet"), "1");
            }
            // A label that is already there is the user's: never overwritten.
            Some(_) => {}
            None if label_set => {
                if current.as_deref() == Some(DELETE_LABEL) {
                    if let Some(n) = attr_name.filter(|_| attr_value.is_some()) {
                        self.remove_attr(&n);
                    } else if let Some(c) = child {
                        let span = with_leading_line(self.doc, c.outer);
                        self.edit(span, "");
                    }
                }
                if let Some(n) = set_name {
                    self.remove_attr(&n);
                }
            }
            None => {}
        }
        Ok(())
    }

    /// Queues the edits that bring one keyword bag in line; returns the items
    /// for a new element when the bag does not exist yet.
    fn bag(
        &mut self,
        bag: Option<&Bag>,
        removes: &[String],
        adds: &[String],
    ) -> Option<Vec<String>> {
        let rdf = self.sc.rdf_prefix.clone();
        let lis: String = adds
            .iter()
            .map(|k| format!("<{rdf}:li>{}</{rdf}:li>", escape(k)))
            .collect();
        let Some(bag) = bag else {
            return (!adds.is_empty()).then(|| adds.to_vec());
        };
        let gone: Vec<&Li> = bag
            .items
            .iter()
            .filter(|l| removes.contains(&l.text))
            .collect();
        if !bag.items.is_empty() && bag.items.len() == gone.len() && adds.is_empty() {
            let span = with_leading_line(self.doc, bag.outer);
            self.edit(span, "");
            return None;
        }
        for li in gone {
            let span = with_leading_line(self.doc, li.span);
            self.edit(span, "");
        }
        if adds.is_empty() {
            return None;
        }
        match &bag.container {
            Container::Open { close_start } => {
                if let Some(close_indent) = indent_at(self.doc, *close_start) {
                    let li_indent = bag
                        .items
                        .last()
                        .and_then(|l| indent_at(self.doc, l.span.0))
                        .map_or_else(|| format!("{close_indent} "), str::to_string);
                    let text: String = adds
                        .iter()
                        .map(|k| {
                            format!("{li_indent}<{rdf}:li>{}</{rdf}:li>{}", escape(k), self.nl)
                        })
                        .collect();
                    let at = close_start - close_indent.len();
                    self.edit((at, at), text);
                } else {
                    self.edit((*close_start, *close_start), lis);
                }
            }
            Container::SelfClosed { name, span } => {
                self.edit(*span, format!("<{name}>{lis}</{name}>"));
            }
            Container::None => {
                let body = format!("<{rdf}:Bag>{lis}</{rdf}:Bag>");
                match bag.inner {
                    Some(inner) => self.edit(inner, body),
                    None => self.edit(bag.outer, format!("<{0}>{body}</{0}>", bag.name)),
                }
            }
        }
        None
    }

    fn new_element(&self, qname: &str, items: &[String], indent: Option<&str>) -> String {
        let rdf = &self.sc.rdf_prefix;
        let li = |k: &String, pad: &str| format!("{pad}<{rdf}:li>{}</{rdf}:li>", escape(k));
        match indent {
            Some(i) => {
                let nl = self.nl;
                let lis = items
                    .iter()
                    .map(|k| li(k, &format!("{i}  ")))
                    .collect::<Vec<_>>()
                    .join(nl);
                format!(
                    "{i}<{qname}>{nl}{i} <{rdf}:Bag>{nl}{lis}{nl}{i} </{rdf}:Bag>{nl}{i}</{qname}>"
                )
            }
            None => {
                let lis: String = items.iter().map(|k| li(k, "")).collect();
                format!("<{qname}><{rdf}:Bag>{lis}</{rdf}:Bag></{qname}>")
            }
        }
    }

    /// Returns `(qualified name, items)` for bags that must be created.
    fn keywords(&mut self, f: &XmpFields) -> Result<Vec<(String, Vec<String>)>, String> {
        let sc = self.sc;
        let present = |bag: &Option<Bag>, k: &str| {
            bag.as_ref()
                .is_some_and(|b| b.items.iter().any(|l| l.text == k))
        };
        // The hierarchical keyword is ours to remove. The mirrored dc leaf is
        // only ours if PhotoSift added it (BlooperSet) and no other hierarchical
        // keyword still ends in the same leaf (e.g. a user's `Funny|Blooper`).
        let remove_hier: Vec<String> = f
            .remove_keywords
            .iter()
            .filter(|k| present(&sc.hier, k))
            .map(|k| k.to_string())
            .collect();
        let ps = find_prefix(&self.decls, NS_PHOTOSIFT);
        let set_name = ps.map(|p| format!("{p}:BlooperSet"));
        let owned = set_name
            .as_deref()
            .and_then(|n| self.attr(n))
            .is_some_and(|a| a.value == "1");
        let mut remove_leaf: Vec<String> = Vec::new();
        let mut clear_owned = false;
        if f.remove_keywords.contains(&BLOOPER_KEYWORD) && owned {
            clear_owned = true;
            let shared = sc
                .hier
                .iter()
                .flat_map(|b| &b.items)
                .any(|l| l.text != BLOOPER_KEYWORD && leaf(&l.text) == leaf(BLOOPER_KEYWORD));
            if !shared {
                remove_leaf.push(leaf(BLOOPER_KEYWORD).to_string());
            }
        }
        let add_hier: Vec<String> = f
            .add_keywords
            .iter()
            .filter(|k| !present(&sc.hier, k))
            .map(|k| k.to_string())
            .collect();
        let add_leaf: Vec<String> = f
            .add_keywords
            .iter()
            .filter(|k| !present(&sc.subject, leaf(k)))
            .map(|k| leaf(k).to_string())
            .collect();
        let set_owned = add_leaf.iter().any(|l| l == leaf(BLOOPER_KEYWORD))
            && f.add_keywords.contains(&BLOOPER_KEYWORD);

        let mut new_children = Vec::new();
        // dc:subject first, so a new element lands before lr:hierarchicalSubject.
        for (is_dc, bag, removes, adds) in [
            (true, &sc.subject, &remove_leaf, &add_leaf),
            (false, &sc.hier, &remove_hier, &add_hier),
        ] {
            if bag.is_none() && adds.is_empty() {
                continue;
            }
            if let Some(items) = self.bag(bag.as_ref(), removes, adds) {
                let (uri, pref, local) = if is_dc {
                    (NS_DC, "dc", "subject")
                } else {
                    (NS_LR, "lr", "hierarchicalSubject")
                };
                let p = self.prefix(uri, pref)?;
                new_children.push((format!("{p}:{local}"), items));
            }
        }
        if set_owned {
            let p = self.prefix(NS_PHOTOSIFT, "photosift")?;
            self.set_attr(&format!("{p}:BlooperSet"), "1");
        } else if clear_owned {
            if let Some(n) = set_name {
                self.remove_attr(&n);
            }
        }
        Ok(new_children)
    }
}

fn merge(doc: &str, f: &XmpFields) -> Result<String, String> {
    let sc = scan(doc)?;
    let nl = if doc.contains("\r\n") { "\r\n" } else { "\n" };
    let mut m = Merge {
        doc,
        sc: &sc,
        nl,
        edits: Vec::new(),
        new_attrs: Vec::new(),
        decls: sc.decls.clone(),
        new_decls: Vec::new(),
    };

    if let Some(r) = f.rating {
        m.rating(r)?;
    }
    m.label(f.label)?;
    let new_children = m.keywords(f)?;

    // New child elements go at the end of the Description.
    let desc_indent = indent_at(doc, sc.desc_tag.0).map(str::to_string);
    let child_indent = sc
        .first_child
        .and_then(|p| indent_at(doc, p))
        .map(str::to_string)
        .or_else(|| desc_indent.as_ref().map(|i| format!("{i} ")));
    let mut convert_children = None;
    if !new_children.is_empty() {
        let render = |ind: Option<&str>| -> Vec<String> {
            new_children
                .iter()
                .map(|(q, items)| m.new_element(q, items, ind))
                .collect()
        };
        match (sc.desc_close, &desc_indent) {
            (Some(close), _) => {
                if let Some(ci) = indent_at(doc, close) {
                    let ind = child_indent.clone().unwrap_or_else(|| format!("{ci} "));
                    let text: String = render(Some(&ind))
                        .iter()
                        .map(|e| format!("{e}{nl}"))
                        .collect();
                    let at = close - ci.len();
                    m.edit((at, at), text);
                } else {
                    let text = render(None).concat();
                    m.edit((close, close), text);
                }
            }
            (None, Some(di)) => {
                let ind = format!("{di} ");
                let body = render(Some(&ind)).join(nl);
                convert_children = Some(format!("{nl}{body}{nl}{di}"));
            }
            (None, None) => convert_children = Some(render(None).concat()),
        }
    }

    // The Description's own start tag: namespace declarations, new attributes,
    // and the `/>` -> `>` ... `</rdf:Description>` conversion.
    let tag_end = sc.desc_tag.1;
    let mut attr_text = String::new();
    if !m.new_decls.is_empty() || !m.new_attrs.is_empty() {
        let tag_text = &doc[sc.desc_tag.0..tag_end];
        let sep = match sc.desc.attrs.last().filter(|_| tag_text.contains('\n')) {
            Some(last) => match indent_at(doc, last.span.0) {
                Some(i) => format!("{nl}{i}"),
                None => " ".into(),
            },
            None => " ".into(),
        };
        let decls = m
            .new_decls
            .iter()
            .map(|(p, u)| format!("xmlns:{p}=\"{u}\""));
        for a in decls.chain(m.new_attrs.iter().cloned()) {
            attr_text.push_str(&sep);
            attr_text.push_str(&a);
        }
    }
    let at = sc.desc.attrs_end;
    if let Some(children) = convert_children {
        m.edit(
            (at, tag_end),
            format!("{attr_text}>{children}</{}>", sc.desc_name),
        );
    } else if !attr_text.is_empty() {
        m.edit((at, at), attr_text);
    }

    let mut edits = m.edits;
    edits.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
    let mut out = doc.to_string();
    let mut limit = doc.len();
    for e in &edits {
        if e.end > limit {
            return Err("internal error: overlapping edits".into());
        }
        out.replace_range(e.start..e.end, &e.text);
        limit = e.start;
    }
    Ok(out)
}
