//! Atlassian Document Format → a small block model the views paint, plus
//! plain text for triage screens and reports.

use serde_json::{Value, json};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Span>),
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    Panel {
        kind: String,
        blocks: Vec<Block>,
    },
    /// rows → cells → spans. Header cells (tableHeader) come through bold; a cell's blocks are joined by
    /// hardBreaks.
    Table(Vec<Vec<Vec<Span>>>),
    Rule,
    /// A Jira action list: (done, the item's blocks) per item.
    Tasks(Vec<(bool, Vec<Block>)>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
    /// Stands in for something the pane cannot show (an image, an attachment): painted quietly in the
    /// placeholder colour, never as a link, even when it keeps one.
    pub placeholder: bool,
}

pub fn parse(v: &Value) -> Doc {
    Doc { blocks: typeset_plain(blocks(v.get("content"))) }
}

/// Structure for descriptions pasted as plain text (e-mail bodies, wiki leftovers): a paragraph that is only a
/// run of `-`, `=`, `_` or `*` is a rule; a short all-caps line on its own is a heading; and a paragraph whose
/// hardBreak lines start "1. ", "2. " … or "• " / "- " / "* " becomes its lead-in paragraph and a real list.
pub fn typeset_plain(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        match b {
            Block::Paragraph(s) if is_rule_text(&spans_text(&s)) => out.push(Block::Rule),
            Block::Paragraph(s) if is_caps_heading(&s) => out.push(Block::Heading { level: 3, spans: s }),
            Block::Paragraph(s) => out.extend(split_plain_list(s)),
            other => out.push(other),
        }
    }
    out
}

/// "----", "=====", "____" (three or more, spaces allowed): a rule typed as text.
pub fn is_rule_text(t: &str) -> bool {
    let t: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    t.chars().count() >= 3 && ['-', '=', '_', '*', '─', '—'].iter().any(|&m| t.chars().all(|c| c == m))
}

/// One unstyled line of two to eight words, all capitals, no closing punctuation ("YOUR API TOKEN IS ABOUT
/// TO EXPIRE"): an e-mail's title.
fn is_caps_heading(s: &[Span]) -> bool {
    if s.iter().any(|sp| sp.link.is_some() || sp.code || sp.text == "\n") {
        return false;
    }
    let t = spans_text(s);
    let t = t.trim();
    let words = t.split_whitespace().count();
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    (2..=8).contains(&words) && t.chars().count() <= 60 && letters >= 6 && t.chars().all(|c| !c.is_lowercase()) && !t.ends_with(['.', ',', ';', ':', '!', '?'])
}

/// The marker of a plain-text list line: Some(Some(n)) for "n. ", Some(None) for a bullet, with the byte
/// length to drop.
fn plain_marker(line: &str) -> Option<(Option<u32>, usize)> {
    let trimmed = line.trim_start();
    let lead = line.len() - trimmed.len();
    for b in ["• ", "- ", "* ", "– "] {
        if let Some(rest) = trimmed.strip_prefix(b) {
            return Some((None, lead + b.len() + (rest.len() - rest.trim_start().len())));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 3 {
        let rest = &trimmed[digits..];
        if let Some(after) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            let n = trimmed[..digits].parse().ok()?;
            return Some((Some(n), lead + digits + 2 + (after.len() - after.trim_start().len())));
        }
    }
    None
}

/// A paragraph's hardBreak lines as a lead-in paragraph, a list, and a trailing paragraph, when at least two
/// lines carry one kind of marker (numbers counting up from 1, or bullets). Lines without a marker after an
/// item continue it. Anything else stays one paragraph.
fn split_plain_list(s: Vec<Span>) -> Vec<Block> {
    let brk = |sp: &Span| sp.text == "\n" && sp.link.is_none();
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    for sp in &s {
        if brk(sp) {
            lines.push(Vec::new());
        } else if let Some(l) = lines.last_mut() {
            l.push(sp.clone());
        }
    }
    let markers: Vec<Option<(Option<u32>, usize)>> =
        lines.iter().map(|l| l.first().filter(|sp| sp.link.is_none() && !sp.code).and_then(|sp| plain_marker(&sp.text))).collect();
    let marked: Vec<usize> = (0..lines.len()).filter(|&i| markers[i].is_some()).collect();
    if marked.len() < 2 {
        return vec![Block::Paragraph(s)];
    }
    let ordered = markers[marked[0]].is_some_and(|m| m.0.is_some());
    let consistent = marked.iter().enumerate().all(|(k, &i)| match markers[i] {
        Some((Some(n), _)) => ordered && n as usize == k + 1,
        Some((None, _)) => !ordered,
        None => false,
    });
    if !consistent {
        return vec![Block::Paragraph(s)];
    }
    let join = |ls: &[Vec<Span>]| -> Vec<Span> {
        let mut out = Vec::new();
        for (k, l) in ls.iter().enumerate() {
            if k > 0 {
                out.push(Span { text: "\n".into(), ..Default::default() });
            }
            out.extend(l.iter().cloned());
        }
        out
    };
    let mut out = Vec::new();
    let first = marked[0];
    if first > 0 {
        out.push(Block::Paragraph(join(&lines[..first])));
    }
    let mut items = Vec::new();
    for (k, &i) in marked.iter().enumerate() {
        let end = marked.get(k + 1).copied().unwrap_or(lines.len());
        let mut item = lines[i..end].to_vec();
        if let (Some(sp), Some((_, cut))) = (item[0].first_mut(), markers[i]) {
            sp.text = sp.text[cut..].to_string();
        }
        // Whitespace-only lines (e-mail padding) carry nothing.
        item.retain(|l| l.iter().any(|sp| !sp.text.trim().is_empty()));
        items.push(nested(vec![Block::Paragraph(join(&item))]));
    }
    out.push(Block::List { ordered, items });
    out
}

/// A node's child blocks, without empty or hardBreak-only paragraphs (each would paint a blank line and a
/// second block gap), and with each paragraph's leading and trailing hardBreaks dropped.
fn blocks(content: Option<&Value>) -> Vec<Block> {
    content.and_then(Value::as_array).map(|a| a.iter().filter_map(block).map(trim_breaks).filter(|b| !is_empty_paragraph(b)).collect()).unwrap_or_default()
}

/// A paragraph without hardBreak spans at its start or end.
fn trim_breaks(b: Block) -> Block {
    match b {
        Block::Paragraph(mut s) => {
            let brk = |sp: &Span| sp.text == "\n" && sp.link.is_none();
            while s.last().is_some_and(brk) {
                s.pop();
            }
            let lead = s.iter().take_while(|sp| brk(sp)).count();
            Block::Paragraph(s.split_off(lead))
        }
        other => other,
    }
}

fn block(n: &Value) -> Option<Block> {
    let t = n["type"].as_str().unwrap_or("");
    Some(match t {
        "paragraph" => Block::Paragraph(spans(n.get("content"))),
        "heading" => Block::Heading { level: n["attrs"]["level"].as_u64().unwrap_or(1) as u8, spans: spans(n.get("content")) },
        "bulletList" | "orderedList" => Block::List {
            ordered: t == "orderedList",
            items: n["content"].as_array().map(|a| a.iter().map(|li| list_item(blocks(li.get("content")))).collect()).unwrap_or_default(),
        },
        "codeBlock" => Block::Code { lang: n["attrs"]["language"].as_str().map(String::from), text: collect_text(n) },
        "blockquote" => Block::Quote(nested(blocks(n.get("content")))),
        "panel" => Block::Panel { kind: n["attrs"]["panelType"].as_str().unwrap_or("info").to_string(), blocks: nested(blocks(n.get("content"))) },
        "rule" => Block::Rule,
        "taskList" | "decisionList" => Block::Tasks(n["content"].as_array().map(|a| a.iter().flat_map(task_items).collect()).unwrap_or_default()),
        "blockCard" | "embedCard" => {
            let url = n["attrs"]["url"].as_str().unwrap_or("").to_string();
            if url.is_empty() {
                return None;
            }
            Block::Paragraph(vec![Span { text: url.clone(), link: Some(url), ..Default::default() }])
        }
        "table" => Block::Table(
            n["content"]
                .as_array()
                .map(|rows| rows.iter().map(|row| row["content"].as_array().map(|cells| cells.iter().map(cell_spans).collect()).unwrap_or_default()).collect())
                .unwrap_or_default(),
        ),
        "mediaSingle" | "mediaGroup" | "media" => Block::Paragraph(vec![placeholder(media_label(n))]),
        _ => {
            let text = collect_text(n);
            if text.is_empty() {
                return None;
            }
            Block::Paragraph(vec![Span { text, ..Default::default() }])
        }
    })
}

/// A taskItem (inline content, done when its state is DONE) and the items of a taskList nested in it,
/// flattened in order. A decisionItem reads as an open task.
fn task_items(n: &Value) -> Vec<(bool, Vec<Block>)> {
    match n["type"].as_str().unwrap_or("") {
        "taskList" | "decisionList" => n["content"].as_array().map(|a| a.iter().flat_map(task_items).collect()).unwrap_or_default(),
        _ => {
            let done = n["attrs"]["state"].as_str().is_some_and(|s| s == "DONE" || s == "DECIDED");
            let inline = spans(n.get("content"));
            vec![(done, nested(vec![Block::Paragraph(inline)]))]
        }
    }
}

/// One table cell's spans: its blocks joined by a hardBreak each; header cells bold.
fn cell_spans(c: &Value) -> Vec<Span> {
    let header = c["type"].as_str() == Some("tableHeader");
    let mut out = Vec::new();
    for (i, b) in blocks(c.get("content")).into_iter().enumerate() {
        if i > 0 {
            out.push(Span { text: "\n".into(), ..Default::default() });
        }
        out.extend(block_spans(b));
    }
    if header {
        out.iter_mut().for_each(|s| s.bold = true);
    }
    out
}

/// A list item's blocks without its empty paragraphs (Jira often ends an item with one), which would
/// otherwise paint a blank line and an extra gap before whatever follows the list.
pub fn list_item(blocks: Vec<Block>) -> Vec<Block> {
    nested(blocks.into_iter().filter(|b| !is_empty_paragraph(b)).collect())
}

/// Blocks inside a list item, panel or blockquote: the skip rule applied again inside each paragraph, so a run
/// of hardBreaks (a blank line, CI-2119's "…)" ⏎ ⏎ "(more ideas to come)") is one line break, and a paragraph
/// left empty is dropped.
fn nested(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| match b {
            Block::Paragraph(s) => trim_breaks(Block::Paragraph(collapse_breaks(s))),
            other => other,
        })
        .filter(|b| !is_empty_paragraph(b))
        .collect()
}

/// Spans with each run of hardBreaks (and whitespace-only spans between them) cut to one hardBreak.
fn collapse_breaks(spans: Vec<Span>) -> Vec<Span> {
    let brk = |sp: &Span| sp.text == "\n" && sp.link.is_none();
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for sp in spans {
        if brk(&sp) {
            while out.last().is_some_and(|l| l.link.is_none() && !l.code && l.text.trim().is_empty() && !brk(l)) {
                out.pop();
            }
            if out.last().is_some_and(brk) {
                continue;
            }
        }
        out.push(sp);
    }
    out
}

fn is_empty_paragraph(b: &Block) -> bool {
    matches!(b, Block::Paragraph(s) if s.iter().all(|sp| sp.text.trim().is_empty()))
}

/// Splits bare `http(s)://` URLs out of plain (non-link, non-code) spans as link spans.
pub fn linkify(spans: &[Span]) -> Vec<Span> {
    let mut out = Vec::new();
    for sp in spans {
        if sp.link.is_some() || sp.code || !sp.text.contains("://") {
            out.push(sp.clone());
            continue;
        }
        let mut rest = sp.text.as_str();
        while let Some(at) = ["https://", "http://"].iter().filter_map(|s| rest.find(s)).min() {
            if at > 0 {
                out.push(Span { text: rest[..at].to_string(), link: None, ..sp.clone() });
            }
            let len = rest[at..].find(char::is_whitespace).unwrap_or(rest.len() - at);
            let mut url = &rest[at..at + len];
            // Sentence punctuation after a URL is not part of it.
            let trimmed = url.trim_end_matches(['.', ',', ')', ';', ':']);
            url = trimmed;
            out.push(Span { text: url.to_string(), link: Some(url.to_string()), ..sp.clone() });
            rest = &rest[at + url.len()..];
        }
        if !rest.is_empty() {
            out.push(Span { text: rest.to_string(), link: None, ..sp.clone() });
        }
    }
    out
}

/// Bare URLs longer than this show a short label (`short_url`).
pub const LONG_URL: usize = 50;

/// The one way a bare URL reads in a document: no scheme, no "www.", no trailing slash
/// ("http://www.atlassian.com/" → "atlassian.com"); past LONG_URL characters, the short label (`short_url`).
pub fn link_label(url: &str) -> String {
    if url.chars().count() > LONG_URL {
        return short_url(url);
    }
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_string()
}

/// What an image shows instead of itself (a pasted screenshot, an e-mail's logo URL).
pub const IMAGE: &str = "Image";
/// What any other attachment shows.
pub const ATTACHMENT: &str = "Attachment";

/// A URL whose path names an image file (an e-mail's logo on a CDN): the pane shows `IMAGE`, not the address.
pub fn is_image_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    url.contains("://") && [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".bmp"].iter().any(|e| path.ends_with(e))
}

/// A media node's placeholder: "Image" for an image (a mediaSingle, or a file Jira calls an image), else
/// "Attachment"; with the file's name or alt text when the node has one ("Image · dashboard.png").
fn media_label(n: &Value) -> String {
    let media = if n["type"].as_str() == Some("media") || n["type"].as_str() == Some("mediaInline") {
        n.clone()
    } else {
        n["content"].as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null)
    };
    let name = media["attrs"]["alt"].as_str().or(media["attrs"]["name"].as_str()).map(str::trim).filter(|s| !s.is_empty());
    let image = n["type"].as_str() == Some("mediaSingle")
        || name.is_some_and(|s| is_image_url(&format!("x://{s}")))
        || media["attrs"]["type"].as_str() == Some("external") && media["attrs"]["url"].as_str().is_some_and(is_image_url);
    let kind = if image { IMAGE } else { ATTACHMENT };
    match name {
        Some(name) => format!("{kind} · {name}"),
        None => kind.to_string(),
    }
}

/// A quiet placeholder span.
fn placeholder(text: String) -> Span {
    Span { text, placeholder: true, ..Default::default() }
}

/// A short URL label's head (host and leading path) and tail, in characters.
const SHORT_HEAD: usize = 34;
const SHORT_TAIL: usize = 20;

/// A long URL's label, shortened in the middle so the end that tells two links apart stays: the host and the
/// leading path up to SHORT_HEAD characters, "…", then the trailing path segments up to SHORT_TAIL (or the last
/// SHORT_TAIL characters of a longer last segment). No scheme, "www.", query or fragment
/// ("https://github.com/acme-corp/widget-stores/pull/129247" → "github.com/acme-corp/widget-stores/…/pull/129247").
pub fn short_url(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let bare = rest.split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    let n = |t: &str| t.chars().count();
    if n(bare) <= SHORT_HEAD + SHORT_TAIL + 3 {
        return if bare.len() < rest.trim_end_matches('/').len() { format!("{bare}…") } else { bare.to_string() };
    }
    let segs: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return format!("{}…", bare.chars().take(SHORT_HEAD + SHORT_TAIL).collect::<String>());
    }
    let mut head = segs[0].to_string();
    let mut used = 1;
    while used < segs.len() - 1 && n(&head) + 1 + n(segs[used]) <= SHORT_HEAD {
        head = format!("{head}/{}", segs[used]);
        used += 1;
    }
    let mut tail: Vec<&str> = Vec::new();
    let mut k = segs.len();
    while k > used && tail.iter().map(|t| n(t) + 1).sum::<usize>() + n(segs[k - 1]) <= SHORT_TAIL {
        tail.insert(0, segs[k - 1]);
        k -= 1;
    }
    if k == used {
        return format!("{head}/{}", tail.join("/"));
    }
    if tail.is_empty() {
        let last = segs[segs.len() - 1];
        let cut: String = last.chars().skip(n(last) - SHORT_TAIL).collect();
        return format!("{head}/…{}", cut.trim_start_matches(['-', '_', '.']));
    }
    format!("{head}/…/{}", tail.join("/"))
}

/// Spans ready to paint: a bare URL (a link whose text is its URL) reads as `link_label`, or as the `IMAGE`
/// placeholder when it names an image, the full URL staying the link; and two links (or cards) side by side get a space between them so
/// they never read as one address.
pub fn tidy_links(spans: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for sp in spans {
        let mut sp = sp.clone();
        if let Some(url) = sp.link.clone() {
            let bare = sp.text.trim_end_matches('/') == url.trim_end_matches('/') || sp.text.starts_with("http://") || sp.text.starts_with("https://");
            if bare && is_image_url(&url) {
                sp = Span { text: IMAGE.into(), placeholder: true, ..sp };
            } else if bare {
                sp.text = link_label(&sp.text);
            }
            if let Some(prev) = out.last()
                && prev.link.is_some()
                && prev.link != sp.link
                && !prev.text.ends_with(char::is_whitespace)
                && !sp.text.starts_with(char::is_whitespace)
            {
                out.push(Span { text: " ".into(), ..Default::default() });
            }
        }
        out.push(sp);
    }
    out
}

/// Flatten a block to spans (for table cells and plain text).
fn block_spans(b: Block) -> Vec<Span> {
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => s,
        Block::Code { text, .. } => vec![Span { text, code: true, ..Default::default() }],
        Block::List { items, .. } => items.into_iter().flat_map(|i| i.into_iter().flat_map(block_spans)).collect(),
        Block::Quote(b) | Block::Panel { blocks: b, .. } => b.into_iter().flat_map(block_spans).collect(),
        Block::Table(rows) => rows.into_iter().flatten().flatten().collect(),
        Block::Rule => Vec::new(),
        Block::Tasks(items) => items.into_iter().flat_map(|(_, i)| i.into_iter().flat_map(block_spans)).collect(),
    }
}

fn spans(content: Option<&Value>) -> Vec<Span> {
    unwiki_strikes(raw_spans(content))
}

/// Jira's wiki-markup import reads "-x/--export" as "-x/-" struck plus "-export": the dashes vanish and a
/// strike mark appears mid-word. A struck span with no spaces, glued to a word before it and to a `-` after
/// it, is that artifact: it goes back to plain "-x/-" so the text reads as typed. Real strikes (word-bounded,
/// or with spaces in them) stay.
pub fn unwiki_strikes(mut s: Vec<Span>) -> Vec<Span> {
    for i in 0..s.len() {
        let glued_after = s.get(i + 1).is_some_and(|n| !n.strike && n.text.starts_with('-'));
        let after_space = i == 0 || s[i - 1].text.ends_with(char::is_whitespace);
        let sp = &mut s[i];
        if sp.strike && !sp.code && sp.link.is_none() && !sp.text.is_empty() && !sp.text.contains(char::is_whitespace) && glued_after && after_space {
            sp.strike = false;
            sp.text = format!("-{}-", sp.text);
        }
    }
    s
}

fn raw_spans(content: Option<&Value>) -> Vec<Span> {
    let mut out = Vec::new();
    for n in content.and_then(Value::as_array).into_iter().flatten() {
        match n["type"].as_str().unwrap_or("") {
            "text" => {
                let mut s = Span { text: n["text"].as_str().unwrap_or("").to_string(), ..Default::default() };
                for m in n["marks"].as_array().into_iter().flatten() {
                    match m["type"].as_str().unwrap_or("") {
                        "strong" => s.bold = true,
                        "em" => s.italic = true,
                        "code" => s.code = true,
                        "strike" => s.strike = true,
                        "link" => s.link = m["attrs"]["href"].as_str().map(String::from),
                        _ => {}
                    }
                }
                // A smart link's card is followed by " )" or " ," in Jira's ADF: drop that stray space.
                if after_card(&out) {
                    s.text = trim_space_before_punctuation(&s.text);
                }
                out.push(s);
            }
            "hardBreak" => out.push(Span { text: "\n".into(), ..Default::default() }),
            "mention" => out.push(Span { text: n["attrs"]["text"].as_str().unwrap_or("@someone").to_string(), bold: true, ..Default::default() }),
            "emoji" => out.push(Span { text: emoji_text(n), ..Default::default() }),
            "inlineCard" => {
                let url = n["attrs"]["url"].as_str().unwrap_or("").to_string();
                let text = browse_key(&url).map_or_else(|| url.clone(), str::to_string);
                out.push(Span { text, link: Some(url), ..Default::default() });
            }
            "status" => out.push(Span { text: format!("[{}]", n["attrs"]["text"].as_str().unwrap_or("")), code: true, ..Default::default() }),
            "date" => out.push(Span { text: n["attrs"]["timestamp"].as_str().unwrap_or("").to_string(), ..Default::default() }),
            "mediaInline" | "media" => out.push(placeholder(media_label(n))),
            _ => {
                let t = collect_text(n);
                if !t.is_empty() {
                    out.push(Span { text: t, ..Default::default() });
                }
            }
        }
    }
    out
}

/// The issue key of a Jira `/browse/<KEY>` URL ("https://x.atlassian.net/browse/CI-1?a=b" → "CI-1").
pub fn browse_key(url: &str) -> Option<&str> {
    let rest = &url[url.find("/browse/")? + "/browse/".len()..];
    let key = rest.split(['/', '?', '#']).next()?;
    let (project, num) = key.split_once('-')?;
    let ok = !project.is_empty()
        && project.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && project.starts_with(|c: char| c.is_ascii_uppercase())
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit());
    ok.then_some(key)
}

/// The last span pushed came from an inlineCard (a link whose text is its key or URL).
fn after_card(out: &[Span]) -> bool {
    out.last().is_some_and(|s| s.link.as_deref().is_some_and(|l| s.text == l || browse_key(l) == Some(s.text.as_str())))
}

/// " ) and more" → ") and more": leading spaces go when the first other character is punctuation.
pub fn trim_space_before_punctuation(text: &str) -> String {
    let t = text.trim_start_matches(' ');
    if t.starts_with([')', ',', '.', ';', ':', '!', '?', ']']) { t.to_string() } else { text.to_string() }
}

/// Inline code with trailing whitespace inside the mark ("`foo `") would wrap with a padded fragment of
/// background; the whitespace moves out of the code span into a plain one.
pub fn tidy_code_spans(spans: &[Span]) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len());
    for sp in spans {
        let trimmed = sp.text.trim_end();
        if sp.code && trimmed.len() < sp.text.len() && !trimmed.is_empty() {
            out.push(Span { text: trimmed.to_string(), ..sp.clone() });
            out.push(Span { text: sp.text[trimmed.len()..].to_string(), code: false, ..sp.clone() });
        } else {
            out.push(sp.clone());
        }
    }
    out
}

fn emoji_text(n: &Value) -> String {
    n["attrs"]["text"].as_str().or(n["attrs"]["shortName"].as_str()).unwrap_or("").to_string()
}

fn collect_text(n: &Value) -> String {
    if let Some(t) = n["text"].as_str() {
        return t.to_string();
    }
    n["content"].as_array().map(|a| a.iter().map(collect_text).collect::<Vec<_>>().join("")).unwrap_or_default()
}

pub fn to_plain_text(v: &Value) -> String {
    let mut out = Vec::new();
    for b in parse(v).blocks {
        push_plain(&b, &mut out, 0);
    }
    out.join("\n").trim_end().to_string()
}

fn spans_text(s: &[Span]) -> String {
    s.iter().map(|x| x.text.as_str()).collect()
}

/// Spans as plain text, placeholders in brackets ("[Image]") so they never read as typed words.
fn plain_text(s: &[Span]) -> String {
    s.iter().map(|x| if x.placeholder { format!("[{}]", x.text) } else { x.text.clone() }).collect()
}

fn push_plain(b: &Block, out: &mut Vec<String>, depth: usize) {
    let indent = "  ".repeat(depth);
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => out.push(format!("{indent}{}", plain_text(s))),
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                let marker = if *ordered { format!("{}. ", i + 1) } else { "- ".to_string() };
                let mut lines = Vec::new();
                for ib in item {
                    push_plain(ib, &mut lines, 0);
                }
                let mut it = lines.into_iter();
                if let Some(first) = it.next() {
                    out.push(format!("{indent}{marker}{first}"));
                }
                for rest in it {
                    out.push(format!("{indent}  {rest}"));
                }
            }
        }
        Block::Code { text, .. } => out.push(format!("{indent}{text}")),
        Block::Quote(bs) | Block::Panel { blocks: bs, .. } => {
            for ib in bs {
                push_plain(ib, out, depth + 1);
            }
        }
        Block::Table(rows) => {
            for r in rows {
                out.push(format!("{indent}{}", r.iter().map(|c| plain_text(c)).collect::<Vec<_>>().join(" | ")));
            }
        }
        Block::Rule => out.push(format!("{indent}---")),
        Block::Tasks(items) => {
            for (done, item) in items {
                let mut lines = Vec::new();
                for ib in item {
                    push_plain(ib, &mut lines, 0);
                }
                let marker = if *done { "[x] " } else { "[ ] " };
                let mut it = lines.into_iter();
                if let Some(first) = it.next() {
                    out.push(format!("{indent}{marker}{first}"));
                }
                for rest in it {
                    out.push(format!("{indent}    {rest}"));
                }
            }
        }
    }
}

/// Plain text → ADF, one paragraph per line (empty lines are skipped).
pub fn text_doc(text: &str) -> Value {
    let paras: Vec<Value> =
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| json!({ "type": "paragraph", "content": [ { "type": "text", "text": l } ] })).collect();
    json!({ "type": "doc", "version": 1, "content": paras })
}

#[cfg(test)]
mod tests {
    #[test]
    fn linkify_splits_bare_urls() {
        let plain = |t: &str| super::Span { text: t.into(), ..Default::default() };
        let out = super::linkify(&[plain("PR: https://github.com/x/pull/1. Done")]);
        let texts: Vec<_> = out.iter().map(|s| (s.text.as_str(), s.link.as_deref())).collect();
        assert_eq!(texts, [("PR: ", None), ("https://github.com/x/pull/1", Some("https://github.com/x/pull/1")), (". Done", None)]);
        let code = super::Span { code: true, ..plain("see https://x.y") };
        assert_eq!(super::linkify(std::slice::from_ref(&code)), vec![code], "code stays code");
        assert_eq!(super::linkify(&[plain("no links")]), vec![plain("no links")]);
    }

    #[test]
    fn inline_card_shows_the_key_and_drops_the_stray_space() {
        let v = serde_json::json!({"content": [{"type": "paragraph", "content": [
            {"type": "text", "text": "(see "},
            {"type": "inlineCard", "attrs": {"url": "https://x.atlassian.net/browse/CI-3779"}},
            {"type": "text", "text": " ) and "},
            {"type": "inlineCard", "attrs": {"url": "https://github.com/a/b/pull/1"}},
            {"type": "text", "text": " too"}
        ]}]});
        let d = super::parse(&v);
        let super::Block::Paragraph(s) = &d.blocks[0] else { panic!() };
        let texts: Vec<&str> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["(see ", "CI-3779", ") and ", "https://github.com/a/b/pull/1", " too"]);
        assert_eq!(s[1].link.as_deref(), Some("https://x.atlassian.net/browse/CI-3779"));
    }

    #[test]
    fn browse_keys_and_punctuation() {
        assert_eq!(super::browse_key("https://x.atlassian.net/browse/CI-1?focus=1"), Some("CI-1"));
        assert_eq!(super::browse_key("https://x.atlassian.net/browse/CI-12/"), Some("CI-12"));
        assert_eq!(super::browse_key("https://x.atlassian.net/browse/ci-1"), None);
        assert_eq!(super::browse_key("https://x.atlassian.net/browse/CI-"), None);
        assert_eq!(super::browse_key("https://github.com/x"), None);
        assert_eq!(super::trim_space_before_punctuation(" ),"), "),");
        assert_eq!(super::trim_space_before_punctuation(" and"), " and");
        assert_eq!(super::trim_space_before_punctuation(""), "");
    }

    #[test]
    fn code_spans_lose_trailing_whitespace() {
        let code = |t: &str| super::Span { text: t.into(), code: true, ..Default::default() };
        let out = super::tidy_code_spans(&[code("foo bar  "), code("ok"), code("  ")]);
        let got: Vec<(&str, bool)> = out.iter().map(|s| (s.text.as_str(), s.code)).collect();
        assert_eq!(got, [("foo bar", true), ("  ", false), ("ok", true), ("  ", true)]);
    }

    #[test]
    fn empty_and_break_only_paragraphs_are_skipped() {
        // Round 8 (CI-2119): an empty paragraph after a list doubled the gap before the next block.
        let v = serde_json::json!({"content": [
            {"type": "bulletList", "content": [{"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "three"}]}]}]},
            {"type": "paragraph"},
            {"type": "paragraph", "content": [{"type": "hardBreak"}, {"type": "hardBreak"}]},
            {"type": "paragraph", "content": [{"type": "hardBreak"}, {"type": "text", "text": "next"}, {"type": "hardBreak"}, {"type": "text", "text": "line"}, {"type": "hardBreak"}]}
        ]});
        let d = super::parse(&v);
        assert_eq!(d.blocks.len(), 2, "{:?}", d.blocks);
        let super::Block::Paragraph(s) = &d.blocks[1] else { panic!() };
        let texts: Vec<&str> = s.iter().map(|sp| sp.text.as_str()).collect();
        assert_eq!(texts, ["next", "\n", "line"], "inner breaks stay, outer ones go");
    }

    #[test]
    fn list_items_drop_empty_paragraphs() {
        let v = serde_json::json!({"content": [{"type": "bulletList", "content": [
            {"type": "listItem", "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": "one"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": " "}]},
                {"type": "paragraph"}
            ]}
        ]}, {"type": "paragraph", "content": [{"type": "text", "text": "after"}]}]});
        let d = super::parse(&v);
        let super::Block::List { items, .. } = &d.blocks[0] else { panic!() };
        assert_eq!(items[0].len(), 1, "only the paragraph with text: {:?}", items[0]);
        assert_eq!(d.blocks.len(), 2);
    }

    #[test]
    fn block_card_is_a_link_paragraph() {
        let v = serde_json::json!({"content": [{"type": "blockCard", "attrs": {"url": "https://a.b/c"}}]});
        let d = super::parse(&v);
        assert_eq!(d.blocks.len(), 1);
        match &d.blocks[0] {
            super::Block::Paragraph(s) => assert_eq!(s[0].link.as_deref(), Some("https://a.b/c")),
            b => panic!("{b:?}"),
        }
    }

    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/adf_sample.json").unwrap()).unwrap()
    }

    #[test]
    fn parses_block_types() {
        let d = parse(&sample());
        assert!(matches!(d.blocks[0], Block::Heading { level: 2, .. }));
        let Block::Paragraph(spans) = &d.blocks[1] else { panic!("paragraph") };
        assert!(spans.iter().any(|s| s.bold && s.text == "canary"));
        assert!(spans.iter().any(|s| s.link.as_deref() == Some("https://gateway-api.sigs.k8s.io")));
        assert!(spans.iter().any(|s| s.text == "\n"), "hardBreak becomes a newline span");
        assert!(spans.iter().any(|s| s.text == "@Ada Lovelace"));
        assert!(matches!(&d.blocks[2], Block::List { ordered: true, items } if items.len() == 2));
        assert!(matches!(&d.blocks[3], Block::Code { lang: Some(l), text } if l == "sh" && text == "kubectl get pods"));
        assert!(matches!(&d.blocks[4], Block::Panel { kind, .. } if kind == "warning"));
        assert!(matches!(d.blocks[5], Block::Rule));
        assert!(matches!(&d.blocks[6], Block::Table(rows) if rows.len() == 2 && rows[0].len() == 2));
        // Unknown node falls back to a paragraph of its text.
        assert!(matches!(&d.blocks[7], Block::Paragraph(s) if s[0].text == "fallback text"));
    }

    #[test]
    fn plain_text_joins_blocks() {
        let t = to_plain_text(&sample());
        assert!(t.starts_with("Goal\nShip canary via Gateway API\n@Ada Lovelace owns it."), "{t}");
        assert!(t.contains("1. first\n2. second"), "{t}");
        assert!(t.contains("kubectl get pods"));
        assert!(t.contains("A | B\n1 | 2"), "{t}");
    }

    #[test]
    fn text_doc_builds_paragraphs() {
        let v = text_doc("one\ntwo");
        assert_eq!(v["content"].as_array().unwrap().len(), 2);
        assert_eq!(v["content"][1]["content"][0]["text"], "two");
        assert_eq!(to_plain_text(&v), "one\ntwo");
    }

    #[test]
    fn breaks_inside_a_list_item_make_no_blank_line() {
        // CI-2119's shape: a list item paragraph with text, two hardBreaks, then more text.
        let doc = json!({ "type": "doc", "content": [ { "type": "orderedList", "content": [ { "type": "listItem", "content": [
            { "type": "paragraph", "content": [
                { "type": "text", "text": "find a way )" }, { "type": "hardBreak" }, { "type": "hardBreak" },
                { "type": "text", "text": "(more ideas to come)" }, { "type": "hardBreak" } ] },
            { "type": "paragraph", "content": [ { "type": "hardBreak" } ] } ] } ] } ] });
        let Block::List { items, .. } = &parse(&doc).blocks[0] else { panic!("a list") };
        assert_eq!(items[0].len(), 1, "the break-only paragraph is dropped");
        let Block::Paragraph(s) = &items[0][0] else { panic!("a paragraph") };
        let texts: Vec<&str> = s.iter().map(|sp| sp.text.as_str()).collect();
        assert_eq!(texts, ["find a way )", "\n", "(more ideas to come)"], "one break, none trailing");
        // Panels and quotes too.
        let quote = json!({ "type": "doc", "content": [ { "type": "blockquote", "content": [ { "type": "paragraph", "content": [
            { "type": "text", "text": "a" }, { "type": "hardBreak" }, { "type": "hardBreak" }, { "type": "text", "text": "b" } ] } ] } ] });
        let Block::Quote(b) = &parse(&quote).blocks[0] else { panic!("a quote") };
        assert_eq!(
            b,
            &vec![Block::Paragraph(vec![
                Span { text: "a".into(), ..Default::default() },
                Span { text: "\n".into(), ..Default::default() },
                Span { text: "b".into(), ..Default::default() }
            ])]
        );
    }

    fn rich() -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/adf_plain_and_tasks.json").unwrap()).unwrap()
    }

    #[test]
    fn task_lists_are_one_item_per_line() {
        let d = parse(&rich());
        let Some(Block::Tasks(items)) = d.blocks.iter().find(|b| matches!(b, Block::Tasks(_))) else { panic!("{:?}", d.blocks) };
        assert_eq!(items.len(), 3);
        assert_eq!(items.iter().map(|i| i.0).collect::<Vec<_>>(), [false, true, false]);
        let Block::Paragraph(s) = &items[1].1[0] else { panic!() };
        assert_eq!(spans_text(s), "Determine if CSV2 data can do it");
        assert!(to_plain_text(&rich()).contains("[ ] Find out\n[x] Determine"), "{}", to_plain_text(&rich()));
    }

    #[test]
    fn wiki_strike_artifacts_read_as_typed() {
        // A ticket like this: "unset -x/--export" came back from Jira as "unset " + strike("x/") + "-export".
        let d = parse(&rich());
        let Block::Paragraph(s) = &d.blocks[0] else { panic!() };
        assert_eq!(spans_text(s), "blocks unset -x/--export and a real old here.");
        assert!(s.iter().all(|sp| !sp.strike || sp.text == "old"), "{s:?}");
    }

    #[test]
    fn plain_text_structure_is_typeset() {
        let d = parse(&rich());
        let kinds: Vec<&str> = d
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(_) => "p",
                Block::Heading { .. } => "h",
                Block::List { ordered: true, .. } => "ol",
                Block::List { .. } => "ul",
                Block::Rule => "hr",
                Block::Tasks(_) => "tasks",
                Block::Table(_) => "table",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, ["p", "tasks", "hr", "h", "p", "p", "ol", "p", "ul", "table"]);
        let Some(Block::List { items, .. }) = d.blocks.iter().find(|b| matches!(b, Block::List { ordered: true, .. })) else { panic!() };
        assert_eq!(items.len(), 3);
        let Block::Paragraph(first) = &items[0][0] else { panic!() };
        assert_eq!(spans_text(first), "Visit https://id.example.com\nand create a new token.", "the marker goes, continuation lines stay");
        // Not a list: numbers that do not count up, or one marked line.
        let p = |t: &str| Span { text: t.into(), ..Default::default() };
        let br = || p("\n");
        assert_eq!(split_plain_list(vec![p("2. a"), br(), p("5. b")]).len(), 1);
        assert_eq!(split_plain_list(vec![p("intro"), br(), p("- only one")]).len(), 1);
        assert!(is_rule_text("------") && is_rule_text("= = = =") && !is_rule_text("--") && !is_rule_text("-x-"));
    }

    #[test]
    fn table_headers_are_bold_and_cells_keep_their_breaks() {
        let d = parse(&rich());
        let Some(Block::Table(rows)) = d.blocks.last() else { panic!() };
        assert!(rows[0].iter().flatten().all(|s| s.bold));
        assert!(rows[1].iter().flatten().all(|s| !s.bold || s.text == "b"));
        assert_eq!(spans_text(&rows[1][1]), "one\ntwo");
    }

    #[test]
    fn long_bare_urls_get_a_short_label_and_links_stay_apart() {
        let url = "https://track.atlassian.com/tracking/381b8edb-9032-4b0a-851d-6a224458bfc9?m=eyJhbGciOiJIUzI1NiJ9";
        assert_eq!(short_url(url), "track.atlassian.com/tracking/…0a-851d-6a224458bfc9");
        assert_eq!(short_url("https://www.example.com/a/b/c?x=1"), "example.com/a/b/c…");
        // Two links into one repo keep the ends that tell them apart.
        let (adr, pr) = (
            "https://github.com/acme-corp/widget-stores/blob/main/docs/decisions/062-code-review-policy-exceptions.md",
            "https://github.com/acme-corp/widget-stores/pull/129247",
        );
        assert_eq!(short_url(pr), "github.com/acme-corp/widget-stores/pull/129247", "short enough to show whole");
        assert_eq!(short_url(adr), "github.com/acme-corp/widget-stores/…policy-exceptions.md");
        assert_eq!(
            short_url("https://github.com/acme-corp/widget-stores/tree/main/deploy/charts/web/templates"),
            "github.com/acme-corp/widget-stores/…/charts/web/templates"
        );
        let link = |t: &str, h: &str| Span { text: t.into(), link: Some(h.into()), ..Default::default() };
        let out = tidy_links(&[
            link(url, url),
            link("http://www.atlassian.com", "http://www.atlassian.com/"),
            link("named", "https://x.y/long/long/long/long/long/long/long/long/path"),
        ]);
        let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["track.atlassian.com/tracking/…0a-851d-6a224458bfc9", " ", "atlassian.com", " ", "named"], "one rule for every bare URL");
        assert_eq!(out[0].link.as_deref(), Some(url), "the link keeps the whole URL");
        let short = link("https://a.b/c", "https://a.b/c");
        assert_eq!(tidy_links(std::slice::from_ref(&short))[0].text, "a.b/c");
        assert_eq!(link_label("https://a.b/c/"), "a.b/c");
    }

    #[test]
    fn images_are_quiet_placeholders_not_urls() {
        let logo = "https://commerce-cdn.prod.atl-paas.net/logos/id-summit/id-summit-email.png?v=2";
        assert!(is_image_url(logo) && !is_image_url("https://a.b/page") && !is_image_url("logo.png"));
        let out = tidy_links(&[Span { text: logo.into(), link: Some(logo.into()), ..Default::default() }]);
        assert_eq!((out[0].text.as_str(), out[0].placeholder), (IMAGE, true));
        assert_eq!(out[0].link.as_deref(), Some(logo), "it still opens");
        let named = Span { text: "the logo".into(), link: Some(logo.into()), ..Default::default() };
        assert_eq!(tidy_links(std::slice::from_ref(&named)), vec![named], "a named link keeps its words");
        let v = json!({"content": [
            {"type": "mediaSingle", "content": [{"type": "media", "attrs": {"type": "file", "id": "x", "alt": "dashboard.png"}}]},
            {"type": "mediaGroup", "content": [{"type": "media", "attrs": {"type": "file", "id": "y", "name": "notes.pdf"}}]},
            {"type": "mediaSingle", "content": [{"type": "media", "attrs": {"type": "file", "id": "z"}}]}
        ]});
        let d = parse(&v);
        let texts: Vec<(String, bool)> = d
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(s) => (spans_text(s), s[0].placeholder),
                b => panic!("{b:?}"),
            })
            .collect();
        assert_eq!(texts, [("Image · dashboard.png".into(), true), ("Attachment · notes.pdf".into(), true), ("Image".into(), true)]);
        assert_eq!(to_plain_text(&v), "[Image · dashboard.png]\n[Attachment · notes.pdf]\n[Image]");
    }

    #[test]
    fn null_is_empty() {
        assert_eq!(to_plain_text(&serde_json::Value::Null), "");
        assert!(parse(&serde_json::Value::Null).blocks.is_empty());
    }
}
