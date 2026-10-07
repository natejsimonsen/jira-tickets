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

fn blocks(content: Option<&Value>) -> Vec<Block> {
    content.and_then(Value::as_array).map(|a| a.iter().filter_map(block).collect()).unwrap_or_default()
}

fn block(n: &Value) -> Option<Block> {
    let t = n["type"].as_str().unwrap_or("");
    Some(match t {
        "paragraph" => Block::Paragraph(spans(n.get("content"))),
        "heading" => Block::Heading { level: n["attrs"]["level"].as_u64().unwrap_or(1) as u8, spans: spans(n.get("content")) },
        "bulletList" | "orderedList" => Block::List {
            ordered: t == "orderedList",
            items: n["content"].as_array().map(|a| a.iter().map(|li| blocks(li.get("content"))).collect()).unwrap_or_default(),
        },
        "codeBlock" => Block::Code { lang: n["attrs"]["language"].as_str().map(String::from), text: collect_text(n) },
        "blockquote" => Block::Quote(blocks(n.get("content"))),
        "panel" => Block::Panel { kind: n["attrs"]["panelType"].as_str().unwrap_or("info").to_string(), blocks: blocks(n.get("content")) },
        "rule" => Block::Rule,
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
                out.push(s);
            }
            "hardBreak" => out.push(Span { text: "\n".into(), ..Default::default() }),
            "mention" => out.push(Span { text: n["attrs"]["text"].as_str().unwrap_or("@someone").to_string(), bold: true, ..Default::default() }),
            "emoji" => out.push(Span { text: emoji_text(n), ..Default::default() }),
            "inlineCard" => {
                let url = n["attrs"]["url"].as_str().unwrap_or("").to_string();
                out.push(Span { text: url.clone(), link: Some(url), ..Default::default() });
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
    fn null_is_empty() {
        assert_eq!(to_plain_text(&serde_json::Value::Null), "");
        assert!(parse(&serde_json::Value::Null).blocks.is_empty());
    }
}
