pub(crate) mod dom;
mod entities;
mod table;

use dom::Dom;
use regex::Regex;
use std::sync::LazyLock;

pub fn html_to_markdown(html: &str) -> String {
    let mut dom = Dom::parse(&format!(
        "<x-html2md id=\"html2md-root\">{html}</x-html2md>"
    ));
    let root = dom.0[0].children[0];
    dom.collapse(root);
    render_children(&dom, root, false)
        .trim_start_matches(['\t', '\n', '\r'])
        .trim_end_matches(markdown_whitespace)
        .to_string()
}

fn markdown_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c')
}

fn join(left: &mut String, right: &str) {
    let trailing = left.len() - left.trim_end_matches('\n').len();
    let leading = right.len() - right.trim_start_matches('\n').len();
    left.truncate(left.len() - trailing);
    left.push_str(&"\n".repeat(trailing.max(leading).min(2)));
    left.push_str(&right[leading..]);
}

fn escape(text: &str) -> String {
    static ESCAPES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r"\\", r"\\"),
            (r"\*", r"\*"),
            (r"^-", r"\-"),
            (r"^\+ ", r"\+ "),
            (r"^(=+)", r"\$1"),
            (r"^(#{1,6}) ", r"\$1 "),
            (r"`", r"\`"),
            (r"^~~~", r"\~~~"),
            (r"\[", r"\["),
            (r"\]", r"\]"),
            (r"^>", r"\>"),
            (r"_", r"\_"),
            (r"^([0-9]+)\. ", r"$1\. "),
        ]
        .into_iter()
        .map(|(p, r)| (Regex::new(p).unwrap(), r))
        .collect()
    });
    ESCAPES.iter().fold(text.to_string(), |s, (p, r)| {
        p.replace_all(&s, *r).into_owned()
    })
}

fn clean_attribute(value: &str) -> String {
    static NEWLINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\n+(?-u:\s)*)+").unwrap());
    NEWLINES.replace_all(value, "\n").into_owned()
}

fn title(value: &str) -> String {
    let value = clean_attribute(value);
    if value.is_empty() {
        String::new()
    } else {
        format!(" \"{value}\"")
    }
}

fn render_children(dom: &Dom, id: usize, code: bool) -> String {
    let mut out = String::new();
    for &child in &dom.0[id].children {
        join(&mut out, &render(dom, child, code));
    }
    out
}

fn render(dom: &Dom, id: usize, code: bool) -> String {
    let n = &dom.0[id];
    if n.tag.is_empty() {
        return if code {
            n.text.clone()
        } else {
            escape(&n.text)
        };
    }
    let mut content = render_children(dom, id, code || n.tag == "code");
    let (prev, next) = dom.siblings(id);
    let text = dom.text(id);
    let blank = dom.blank(id);
    let mut leading = "";
    let mut trailing = "";
    if !n.block() && text != "\n" {
        let has_leading = text.chars().next().is_some_and(markdown_whitespace);
        let has_trailing = text.chars().last().is_some_and(markdown_whitespace);
        if has_leading && !prev.is_some_and(|p| dom.outer_html(p).ends_with(' ')) {
            leading = " ";
        }
        if !(blank && has_leading && has_trailing)
            && has_trailing
            && !next.is_some_and(|p| dom.outer_html(p).starts_with(' '))
        {
            trailing = " ";
        }
        if !leading.is_empty() || !trailing.is_empty() {
            content = content.trim_matches(|c| c <= ' ').to_string();
        }
    }
    let replacement = if blank {
        if n.block() {
            "\n\n".to_string()
        } else {
            String::new()
        }
    } else {
        match n.tag.as_str() {
            "p" => format!("\n\n{content}\n\n"),
            "br" => "  \n".to_string(),
            "h1" | "h2" => format!(
                "\n\n{content}\n{}\n\n",
                if n.tag == "h1" { "=" } else { "-" }.repeat(content.encode_utf16().count())
            ),
            "h3" | "h4" | "h5" | "h6" => format!(
                "\n\n{} {content}\n\n",
                "#".repeat(n.tag.as_bytes()[1] as usize - b'0' as usize)
            ),
            "blockquote" => format!(
                "\n\n{}\n\n",
                content
                    .trim_matches('\n')
                    .split('\n')
                    .map(|s| format!("> {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            "ul" | "ol" => {
                if n.parent.is_some_and(|p| {
                    dom.0[p].tag == "li"
                        && dom.0[p]
                            .children
                            .iter()
                            .rev()
                            .find(|&&c| !dom.0[c].tag.is_empty())
                            == Some(&id)
                }) {
                    format!("\n{content}")
                } else {
                    format!("\n\n{content}\n\n")
                }
            }
            "li" => {
                let mut c = content.trim_start_matches('\n').to_string();
                if c.ends_with('\n') {
                    c = format!("{}\n", c.trim_end_matches('\n'));
                }
                c = c.replace('\n', "\n    ");
                let prefix = n
                    .parent
                    .filter(|&p| dom.0[p].tag == "ol")
                    .map(|p| {
                        let start = dom.0[p].attr("start").parse::<i32>().unwrap_or(1);
                        let index = dom.0[p]
                            .children
                            .iter()
                            .filter(|&&c| !dom.0[c].tag.is_empty())
                            .position(|&c| c == id)
                            .unwrap_or(0);
                        format!("{}.  ", start.wrapping_add(index as i32))
                    })
                    .unwrap_or_else(|| "*   ".to_string());
                format!(
                    "{prefix}{c}{}",
                    if next.is_some() && !c.ends_with('\n') {
                        "\n"
                    } else {
                        ""
                    }
                )
            }
            "pre" if n.children.first().is_some_and(|&c| dom.0[c].tag == "code") => format!(
                "\n\n    {}\n\n",
                dom.text(n.children[0]).replace('\n', "\n    ")
            ),
            "hr" => "\n\n* * *\n\n".to_string(),
            "a" if !n.attr("href").is_empty() => {
                format!("[{content}]({}{})", n.attr("href"), title(n.attr("title")))
            }
            "em" | "i" => {
                if content.trim_matches(|c| c <= ' ').is_empty() {
                    String::new()
                } else {
                    format!("_{content}_")
                }
            }
            "strong" | "b" => {
                if content.trim().is_empty() {
                    String::new()
                } else {
                    format!("**{content}**")
                }
            }
            "code"
                if !n
                    .parent
                    .is_some_and(|p| dom.0[p].tag == "pre" && prev.is_none() && next.is_none()) =>
            {
                if content.trim_matches(|c| c <= ' ').is_empty() {
                    String::new()
                } else {
                    let singles = content.split(|c| c != '`').filter(|s| s.len() == 1).count();
                    let delimiter = "`".repeat(singles + 1);
                    format!(
                        "{delimiter}{}{content}{}{delimiter}",
                        if content.starts_with('`') { " " } else { "" },
                        if content.ends_with('`') { " " } else { "" }
                    )
                }
            }
            "img" => {
                if n.attr("src").is_empty() {
                    String::new()
                } else {
                    format!(
                        "![{}]({}{})",
                        clean_attribute(n.attr("alt")),
                        n.attr("src"),
                        title(n.attr("title"))
                    )
                }
            }
            "del" | "s" | "strike" => format!("~{content}~"),
            "input" if n.attr("type") == "checkbox" => {
                if n.attrs.iter().any(|(k, _)| k == "checked") {
                    "[x] ".to_string()
                } else {
                    "[ ] ".to_string()
                }
            }
            "div"
                if n.children.first().is_some_and(|&c| dom.0[c].tag == "pre")
                    && highlight(n.attr("class")).is_some() =>
            {
                format!(
                    "\n\n```{}\n{}\n```\n\n",
                    highlight(n.attr("class")).unwrap(),
                    dom.outer_html(n.children[0])
                )
            }
            "td" | "th" => table::cell(dom, id, &content) + &table::spanned(dom, id, ""),
            "tr" => table::row(dom, id, &content),
            "table" if !table::nested(dom, id) => {
                format!("\n\n{}\n\n", content.replace("\n\n", "\n"))
            }
            "thead" | "tbody" | "tfoot" => content,
            "caption" => {
                if n.parent.is_some_and(|p| {
                    dom.0[p].tag == "table" && dom.0[p].children.first() == Some(&id)
                }) {
                    content
                } else {
                    String::new()
                }
            }
            _ => {
                if n.block() {
                    format!("\n\n{content}\n\n")
                } else {
                    content
                }
            }
        }
    };
    format!("{leading}{replacement}{trailing}")
}

fn highlight(class: &str) -> Option<&str> {
    let language = class
        .strip_prefix("highlight-text-")
        .or_else(|| class.strip_prefix("highlight-source-"))?;
    (!language.is_empty()
        && language
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
    .then_some(language)
}

#[cfg(test)]
#[path = "../../../tests/unit/utils/html_to_markdown.rs"]
mod tests;
