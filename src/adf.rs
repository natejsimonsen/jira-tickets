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
    /// rows → cells → spans. The first row is the header when it came from tableHeader cells.
    Table(Vec<Vec<Vec<Span>>>),
    Rule,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

pub fn parse(v: &Value) -> Doc {
    Doc { blocks: blocks(v.get("content")) }
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
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row["content"]
                                .as_array()
                                .map(|cells| cells.iter().map(|c| blocks(c.get("content")).into_iter().flat_map(block_spans).collect()).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "mediaSingle" | "mediaGroup" | "media" => Block::Paragraph(vec![Span { text: "[attachment]".into(), italic: true, ..Default::default() }]),
        _ => {
            let text = collect_text(n);
            if text.is_empty() {
                return None;
            }
            Block::Paragraph(vec![Span { text, ..Default::default() }])
        }
    })
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

/// Flatten a block to spans (for table cells and plain text).
fn block_spans(b: Block) -> Vec<Span> {
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => s,
        Block::Code { text, .. } => vec![Span { text, code: true, ..Default::default() }],
        Block::List { items, .. } => items.into_iter().flat_map(|i| i.into_iter().flat_map(block_spans)).collect(),
        Block::Quote(b) | Block::Panel { blocks: b, .. } => b.into_iter().flat_map(block_spans).collect(),
        Block::Table(rows) => rows.into_iter().flatten().flatten().collect(),
        Block::Rule => Vec::new(),
    }
}

fn spans(content: Option<&Value>) -> Vec<Span> {
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

fn push_plain(b: &Block, out: &mut Vec<String>, depth: usize) {
    let indent = "  ".repeat(depth);
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => out.push(format!("{indent}{}", spans_text(s))),
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
                out.push(format!("{indent}{}", r.iter().map(|c| spans_text(c)).collect::<Vec<_>>().join(" | ")));
            }
        }
        Block::Rule => out.push(format!("{indent}---")),
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

    #[test]
    fn null_is_empty() {
        assert_eq!(to_plain_text(&serde_json::Value::Null), "");
        assert!(parse(&serde_json::Value::Null).blocks.is_empty());
    }
}
