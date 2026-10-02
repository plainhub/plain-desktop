use super::dom::Dom;

pub(super) fn cell(dom: &Dom, id: usize, content: &str) -> String {
    let first = dom.0[id]
        .parent
        .is_some_and(|p| dom.0[p].children.first() == Some(&id));
    format!(
        "{}{} |",
        if first { "| " } else { " " },
        content
            .replace("\r\n", "\n")
            .replace('\n', " ")
            .replace('|', "\\|")
    )
}

pub(super) fn spanned(dom: &Dom, id: usize, content: &str) -> String {
    let span = dom.0[id].attr("colspan").parse::<i32>().unwrap_or(1);
    if span <= 1 {
        String::new()
    } else {
        format!(" {content} | ").repeat((span - 1) as usize)
    }
}

fn first_row(dom: &Dom, id: usize) -> Option<usize> {
    for &c in &dom.0[id].children {
        if dom.0[c].tag == "tr" {
            return Some(c);
        }
        if let Some(row) = first_row(dom, c) {
            return Some(row);
        }
    }
    None
}

pub(super) fn row(dom: &Dom, id: usize, content: &str) -> String {
    let mut table = dom.0[id].parent;
    if table.is_some_and(|p| matches!(dom.0[p].tag.as_str(), "thead" | "tbody" | "tfoot")) {
        table = dom.0[table.unwrap()].parent;
    }
    let heading = table.is_some_and(|p| dom.0[p].tag == "table" && first_row(dom, p) == Some(id));
    let mut border = String::new();
    if heading {
        for &c in &dom.0[id].children {
            let align = dom.0[c].attr("align").to_lowercase();
            let value = match align.as_str() {
                "left" => ":--",
                "right" => "--:",
                "center" => ":-:",
                _ => "---",
            };
            border.push_str(&(cell(dom, c, value) + &spanned(dom, c, value)));
        }
    }
    format!(
        "\n{content}{}",
        if border.is_empty() {
            border
        } else {
            format!("\n{border}")
        }
    )
}

pub(super) fn nested(dom: &Dom, id: usize) -> bool {
    let mut parent = dom.0[id].parent;
    while let Some(p) = parent {
        if dom.0[p].tag == "table" {
            return true;
        }
        parent = dom.0[p].parent;
    }
    false
}
