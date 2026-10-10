use super::entities::decode;

#[derive(Default)]
pub struct Node {
    pub tag: String,
    pub text: String,
    pub attrs: Vec<(String, String)>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

impl Node {
    pub fn attr(&self, name: &str) -> &str {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
    pub fn block(&self) -> bool {
        matches!(
            self.tag.as_str(),
            "address"
                | "article"
                | "aside"
                | "audio"
                | "blockquote"
                | "body"
                | "canvas"
                | "center"
                | "dd"
                | "dir"
                | "div"
                | "dl"
                | "dt"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "frameset"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "header"
                | "hgroup"
                | "hr"
                | "html"
                | "isindex"
                | "li"
                | "main"
                | "menu"
                | "nav"
                | "noframes"
                | "noscript"
                | "ol"
                | "output"
                | "p"
                | "pre"
                | "section"
                | "table"
                | "tbody"
                | "td"
                | "tfoot"
                | "th"
                | "thead"
                | "tr"
                | "ul"
        )
    }
    pub fn void(&self) -> bool {
        matches!(
            self.tag.as_str(),
            "area"
                | "base"
                | "br"
                | "col"
                | "command"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "keygen"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
    }
    pub fn meaningful(&self) -> bool {
        matches!(
            self.tag.as_str(),
            "a" | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "th"
                | "td"
                | "iframe"
                | "script"
                | "audio"
                | "video"
        )
    }
}

pub struct Dom(pub Vec<Node>);

fn take_while(input: &str, pos: &mut usize, predicate: impl Fn(char) -> bool) -> String {
    let start = *pos;
    while let Some(c) = input[*pos..].chars().next() {
        if !predicate(c) {
            break;
        }
        *pos += c.len_utf8();
    }
    input[start..*pos].to_string()
}

impl Dom {
    pub fn parse(input: &str) -> Self {
        let mut dom = Self(vec![Node::default()]);
        let mut stack = vec![0];
        let mut pos = 0;
        let mut text = String::new();
        while pos < input.len() {
            let tail = &input[pos..];
            if tail.starts_with("<!--") {
                dom.flush(*stack.last().unwrap(), &mut text);
                pos += tail.find("-->").map_or(tail.len(), |n| n + 3);
            } else if let Some(cdata) = tail.strip_prefix("<![CDATA[") {
                let end = cdata.find("]]>");
                text.push_str(&cdata[..end.unwrap_or(cdata.len())]);
                pos += end.map_or(tail.len(), |n| n + 12);
            } else if tail.starts_with("<!") || tail.starts_with("<?") {
                dom.flush(*stack.last().unwrap(), &mut text);
                pos += tail.find('>').map_or(tail.len(), |n| n + 1);
            } else if tail.starts_with("</") {
                dom.flush(*stack.last().unwrap(), &mut text);
                if let Some(end) = tail.find('>') {
                    let name = tail[2..end].trim().to_lowercase();
                    if let Some(index) = stack
                        .iter()
                        .rposition(|&id| id != 0 && dom.0[id].tag == name)
                    {
                        stack.truncate(index);
                    }
                    pos += end + 1;
                } else {
                    pos = input.len();
                }
            } else if tail.starts_with('<')
                && tail[1..].chars().next().is_some_and(char::is_alphabetic)
            {
                dom.flush(*stack.last().unwrap(), &mut text);
                pos += 1;
                let tag = take_while(input, &mut pos, |c| {
                    !c.is_whitespace() && !matches!(c, '>' | '/' | '=')
                })
                .to_lowercase();
                let mut node = Node {
                    tag,
                    parent: stack.last().copied(),
                    ..Node::default()
                };
                while pos < input.len() {
                    take_while(input, &mut pos, char::is_whitespace);
                    if pos == input.len() || input[pos..].starts_with(['>', '/']) {
                        break;
                    }
                    let name = take_while(input, &mut pos, |c| {
                        !c.is_whitespace() && !matches!(c, '=' | '>' | '/')
                    })
                    .to_lowercase();
                    if name.is_empty() {
                        pos += input[pos..].chars().next().unwrap().len_utf8();
                        continue;
                    }
                    take_while(input, &mut pos, char::is_whitespace);
                    let mut value = String::new();
                    if input[pos..].starts_with('=') {
                        pos += 1;
                        take_while(input, &mut pos, char::is_whitespace);
                        if input[pos..].starts_with(['\'', '"']) {
                            let quote = input.as_bytes()[pos] as char;
                            pos += 1;
                            let end = input[pos..].find(quote).map(|n| pos + n);
                            value = decode(&input[pos..end.unwrap_or(input.len())]);
                            pos = end.map_or(input.len(), |n| n + 1);
                        } else {
                            value = decode(&take_while(input, &mut pos, |c| {
                                c != '>' && !c.is_whitespace()
                            }));
                        }
                    }
                    if let Some((_, old)) = node.attrs.iter_mut().find(|(k, _)| k == &name) {
                        *old = value;
                    } else {
                        node.attrs.push((name, value));
                    }
                }
                let closed = input[pos..].starts_with('/');
                if closed {
                    pos += 1;
                }
                if input[pos..].starts_with('>') {
                    pos += 1;
                }
                let void = node.void() && !matches!(node.tag.as_str(), "command" | "keygen");
                let id = dom.0.len();
                dom.0[*stack.last().unwrap()].children.push(id);
                dom.0.push(node);
                if !closed && !void {
                    stack.push(id);
                }
            } else if tail.starts_with('<') {
                text.push('<');
                pos += 1;
            } else {
                let end = tail.find('<').unwrap_or(tail.len());
                text.push_str(&decode(&tail[..end]));
                pos += end;
            }
        }
        dom.flush(*stack.last().unwrap(), &mut text);
        dom
    }
    fn flush(&mut self, parent: usize, text: &mut String) {
        if !text.is_empty() {
            let id = self.0.len();
            self.0[parent].children.push(id);
            self.0.push(Node {
                text: std::mem::take(text),
                parent: Some(parent),
                ..Node::default()
            });
        }
    }
    pub fn text(&self, id: usize) -> String {
        if self.0[id].tag.is_empty() {
            return self.0[id].text.clone();
        }
        self.0[id].children.iter().map(|&c| self.text(c)).collect()
    }
    pub fn blank(&self, id: usize) -> bool {
        let n = &self.0[id];
        !n.void()
            && !n.meaningful()
            && self.text(id).chars().all(char::is_whitespace)
            && !self.has_meaningful(id)
    }
    fn has_meaningful(&self, id: usize) -> bool {
        self.0[id]
            .children
            .iter()
            .any(|&c| self.0[c].void() || self.0[c].meaningful() || self.has_meaningful(c))
    }
    pub fn siblings(&self, id: usize) -> (Option<usize>, Option<usize>) {
        let Some(p) = self.0[id].parent else {
            return (None, None);
        };
        let children = &self.0[p].children;
        let index = children.iter().position(|&n| n == id).unwrap();
        (
            index.checked_sub(1).map(|i| children[i]),
            children.get(index + 1).copied(),
        )
    }
    pub fn outer_html(&self, id: usize) -> String {
        let n = &self.0[id];
        if n.tag.is_empty() {
            return n.text.clone();
        }
        let mut s = format!("<{}", n.tag);
        for (k, v) in &n.attrs {
            s.push_str(&format!(
                " {k}=\"{}\"",
                v.replace('&', "&amp;").replace('"', "&quot;")
            ));
        }
        if n.children.is_empty() && n.void() {
            s.push_str(" />");
        } else {
            s.push('>');
            for &c in &n.children {
                s.push_str(&self.outer_html(c));
            }
            s.push_str(&format!("</{}>", n.tag));
        }
        s
    }
    pub fn collapse(&mut self, root: usize) {
        let mut events = Vec::new();
        let mut stack = self.0[root]
            .children
            .iter()
            .rev()
            .map(|&id| (id, false))
            .collect::<Vec<_>>();
        while let Some((id, exit)) = stack.pop() {
            events.push(id);
            if !exit && !self.0[id].children.is_empty() && self.0[id].tag != "pre" {
                stack.push((id, true));
                stack.extend(self.0[id].children.iter().rev().map(|&c| (c, false)));
            }
        }
        let mut previous: Option<usize> = None;
        let mut previous_void = false;
        for id in events {
            let n = &self.0[id];
            if n.tag.is_empty() {
                let mut value = String::new();
                let mut space = false;
                for c in n.text.chars() {
                    if matches!(c, ' ' | '\r' | '\n' | '\t') {
                        if !space {
                            value.push(' ');
                        }
                        space = true;
                    } else {
                        value.push(c);
                        space = false;
                    }
                }
                if previous.is_none_or(|p| self.0[p].text.ends_with(' '))
                    && !previous_void
                    && value.starts_with(' ')
                {
                    value.remove(0);
                }
                self.0[id].text = value;
                if !self.0[id].text.is_empty() {
                    previous = Some(id);
                }
            } else if n.block() || n.tag == "br" {
                if let Some(p) = previous {
                    if self.0[p].text.ends_with(' ') {
                        self.0[p].text.pop();
                    }
                }
                previous = None;
                previous_void = false;
            } else if n.void() {
                previous = None;
                previous_void = true;
            }
        }
        if let Some(p) = previous {
            if self.0[p].text.ends_with(' ') {
                self.0[p].text.pop();
            }
        }
        let empty = self
            .0
            .iter()
            .map(|n| n.tag.is_empty() && n.text.is_empty())
            .collect::<Vec<_>>();
        for n in &mut self.0 {
            n.children.retain(|&c| !empty[c]);
        }
    }
}
