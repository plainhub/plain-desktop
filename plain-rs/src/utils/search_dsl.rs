//! Plain-app search-DSL parser — tokens split on whitespace (quotes
//! preserve spaces, `\` escapes), each token `field:op_value`; bare
//! tokens become `text:` fields, `NOT` inverts the next field's op.
//! 1:1 port of the Go `internal/search/search.go` parser, shared by
//! plain-nas (fs search) and plain-desktop (tag/queue query fields).

// ---------------------------------------------------------------------------
// FilterField model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterField {
    pub name: String,
    pub op: String,
    pub value: String,
}

const NOT_TYPE: &str = "NOT";

/// Operator-invert table. Mirrors Go `invert` exactly.
fn invert_op(op: &str) -> String {
    match op {
        "=" => "!=".into(),
        ">=" => "<".into(),
        ">" => "<=".into(),
        "!=" => "=".into(),
        "<=" => ">".into(),
        "<" => ">=".into(),
        "in" => "nin".into(),
        "nin" => "in".into(),
        // NOT-aliased / empty: drop the op (the Go side blanks it).
        other => {
            if other == NOT_TYPE {
                String::new()
            } else {
                other.to_string()
            }
        }
    }
}

const ORDERED_GROUP_OPS: &[&str] = &["<=", ">=", "!=", "=", ">", "<"];

fn split_in_group(input: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut quote: u8 = 0;
    let mut escape = false;
    for c in input.chars() {
        if escape {
            buf.push(c);
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if quote != 0 {
            buf.push(c);
            if c as u8 == quote {
                quote = 0;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = c as u8;
            buf.push(c);
            continue;
        }
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            if !buf.is_empty() {
                result.push(std::mem::take(&mut buf));
            }
            continue;
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        result.push(buf);
    }
    result
}

fn remove_quotation(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn detect_group_type(group: &str) -> String {
    for op in ORDERED_GROUP_OPS {
        if group.starts_with(op) {
            return op.to_string();
        }
    }
    for op in ORDERED_GROUP_OPS {
        if group.contains(op) {
            return op.to_string();
        }
    }
    "=".to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueryGroup {
    length: usize,
    field: String,
    query: String,
    op: String,
    value: String,
}

fn split_group(q: &str) -> QueryGroup {
    let (field_part, query_part) = match q.split_once(':') {
        Some((f, rest)) => (remove_quotation(f), remove_quotation(rest)),
        None => (remove_quotation(q), String::new()),
    };
    let length = if q.contains(':') { 2 } else { 1 };
    let typ = detect_group_type(&query_part);
    let typ = if typ.is_empty() { "=".to_string() } else { typ };
    let value = if !typ.is_empty() && query_part.starts_with(&typ) {
        query_part[typ.len()..].to_string()
    } else {
        query_part.clone()
    };
    QueryGroup {
        length,
        field: field_part,
        query: query_part,
        op: typ,
        value,
    }
}

fn parse_group(group: &str) -> FilterField {
    if group == NOT_TYPE {
        return FilterField {
            name: String::new(),
            op: NOT_TYPE.to_string(),
            value: String::new(),
        };
    }
    let parts = split_group(group);
    if parts.field == "is" {
        return FilterField {
            name: parts.query,
            op: String::new(),
            value: "true".to_string(),
        };
    }
    if parts.length == 1 {
        return FilterField {
            name: "text".to_string(),
            op: String::new(),
            value: parts.field,
        };
    }
    FilterField {
        name: parts.field,
        op: parts.op,
        value: parts.value,
    }
}

/// Parse a search query into a list of `FilterField`s, applying NOT inversions
/// and stripping the NOT markers. Mirrors `search.Parse` in the Go side.
pub fn parse(q: &str) -> Vec<FilterField> {
    if q.trim().is_empty() {
        return Vec::new();
    }
    let groups = split_in_group(q);
    let mut fields: Vec<FilterField> = groups.iter().map(|g| parse_group(g)).collect();

    // Walk once, applying NOT flips to the next non-NOT field.
    let mut invert_next = false;
    for f in fields.iter_mut() {
        if f.op == NOT_TYPE {
            invert_next = true;
            continue;
        }
        if invert_next {
            f.op = invert_op(&f.op);
            invert_next = false;
        }
    }
    fields.retain(|f| f.op != NOT_TYPE);
    fields
}

/// Value of the first `name` field (quoting already resolved by the
/// parser); `None` when absent. The `text:` filter of a page query and
/// the `ids:a,b,c` checkbox selection are the two consumers.
pub fn field_value(q: &str, name: &str) -> Option<String> {
    parse(q)
        .into_iter()
        .find(|f| f.name == name)
        .map(|f| f.value)
}

#[cfg(test)]
#[path = "../../tests/unit/utils/search_dsl.rs"]
mod tests;
