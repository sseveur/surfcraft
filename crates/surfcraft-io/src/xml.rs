//! Minimal non-validating XML reader: enough DOM for `.s3d` files (elements and text; attributes,
//! comments, processing instructions and doctypes are skipped). Iterative, so deep nesting cannot overflow
//! the stack.

use crate::Error;

#[derive(Debug, Default)]
struct Node {
    name: String,
    text: String,
    children: Vec<usize>,
}

#[derive(Debug)]
pub struct Document {
    nodes: Vec<Node>,
}

/// Index of an element; `Document::ROOT` is a virtual node holding the top-level element.
pub type Id = usize;

fn err(message: impl Into<String>) -> Error {
    Error::Invalid(format!("XML: {}", message.into()))
}

impl Document {
    pub const ROOT: Id = 0;

    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut nodes = vec![Node::default()];
        let mut stack: Vec<Id> = vec![Self::ROOT];
        let mut rest = text;
        while !rest.is_empty() {
            let Some(lt) = rest.find('<') else {
                append_text(&mut nodes, &stack, rest)?;
                break;
            };
            append_text(&mut nodes, &stack, &rest[..lt])?;
            rest = &rest[lt..];
            if let Some(r) = rest.strip_prefix("<!--") {
                rest = skip_past(r, "-->")?;
            } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
                let end = r.find("]]>").ok_or_else(|| err("unterminated CDATA"))?;
                let top = *stack.last().unwrap_or(&Self::ROOT);
                nodes[top].text.push_str(&r[..end]);
                rest = &r[end + 3..];
            } else if let Some(r) = rest.strip_prefix("<?") {
                rest = skip_past(r, "?>")?;
            } else if rest.starts_with("<!") {
                rest = skip_declaration(rest)?;
            } else if let Some(r) = rest.strip_prefix("</") {
                let end = r.find('>').ok_or_else(|| err("unterminated end tag"))?;
                let name = r[..end].trim();
                let open = stack.pop().filter(|&id| id != Self::ROOT);
                match open {
                    Some(id) if nodes[id].name == name => {}
                    _ => return Err(err(format!("unexpected </{name}>"))),
                }
                rest = &r[end + 1..];
            } else {
                let r = &rest[1..];
                let end = tag_end(r).ok_or_else(|| err("unterminated tag"))?;
                let tag = &r[..end];
                let self_closing = tag.ends_with('/');
                let tag = tag.trim_end_matches('/');
                let name_len = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
                let name = &tag[..name_len];
                if name.is_empty() {
                    return Err(err("empty tag name"));
                }
                let id = nodes.len();
                nodes.push(Node {
                    name: name.to_owned(),
                    ..Default::default()
                });
                let parent = *stack.last().ok_or_else(|| err("content after root"))?;
                nodes[parent].children.push(id);
                if !self_closing {
                    stack.push(id);
                }
                rest = &r[end + 1..];
            }
        }
        if stack.len() != 1 {
            return Err(err("unclosed element"));
        }
        Ok(Self { nodes })
    }

    pub fn name(&self, id: Id) -> &str {
        &self.nodes[id].name
    }

    /// Descendants of `id` (not `id` itself) in document order, like DOM `getElementsByTagName`.
    pub fn descendants(&self, id: Id) -> impl Iterator<Item = Id> + '_ {
        let mut stack: Vec<Id> = self.nodes[id].children.iter().rev().copied().collect();
        std::iter::from_fn(move || {
            let next = stack.pop()?;
            stack.extend(self.nodes[next].children.iter().rev());
            Some(next)
        })
    }

    pub fn find(&self, id: Id, name: &str) -> Option<Id> {
        self.descendants(id).find(|&d| self.name(d) == name)
    }

    pub fn find_all(&self, id: Id, name: &str) -> Vec<Id> {
        self.descendants(id)
            .filter(|&d| self.name(d) == name)
            .collect()
    }

    /// All text inside the element, like DOM `getTextContent`.
    pub fn text(&self, id: Id) -> String {
        let mut out = self.nodes[id].text.clone();
        for d in self.descendants(id) {
            out.push_str(&self.nodes[d].text);
        }
        out
    }
}

fn append_text(nodes: &mut [Node], stack: &[Id], raw: &str) -> Result<(), Error> {
    if raw.is_empty() {
        return Ok(());
    }
    let top = *stack.last().unwrap_or(&Document::ROOT);
    let text = unescape(raw)?;
    nodes[top].text.push_str(&text);
    Ok(())
}

fn skip_past<'a>(s: &'a str, end: &str) -> Result<&'a str, Error> {
    s.find(end)
        .map(|i| &s[i + end.len()..])
        .ok_or_else(|| err(format!("missing {end:?}")))
}

/// `<!DOCTYPE ...>` possibly with an internal `[...]` subset.
fn skip_declaration(s: &str) -> Result<&str, Error> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '>' if depth == 0 => return Ok(&s[i + 1..]),
            _ => {}
        }
    }
    Err(err("unterminated declaration"))
}

/// Index of the `>` closing a start tag, skipping quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn unescape(s: &str) -> Result<String, Error> {
    if !s.contains('&') {
        return Ok(s.to_owned());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let semi = rest.find(';').ok_or_else(|| err("unterminated entity"))?;
        let entity = &rest[..semi];
        let c = match entity {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32)
                    .ok_or_else(|| err(format!("unknown entity &{entity};")))?
            }
        };
        out.push(c);
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_elements() {
        let doc = Document::parse(
            "<?xml version=\"1.0\"?><!-- c --><!DOCTYPE a [<!ENTITY x \"y\">]>\
             <a k='>'><b>1 &amp; 2</b><c/><b><![CDATA[<x>]]></b></a>",
        )
        .unwrap();
        let a = doc.find(Document::ROOT, "a").unwrap();
        let bs = doc.find_all(a, "b");
        assert_eq!(bs.len(), 2);
        assert_eq!(doc.text(bs[0]), "1 & 2");
        assert_eq!(doc.text(bs[1]), "<x>");
        assert_eq!(doc.text(a), "1 & 2<x>");
    }

    #[test]
    fn rejects_malformed() {
        for bad in [
            "<a>",
            "<a></b>",
            "</a>",
            "<a><b></a></b>",
            "<a &x;>",
            "<a>&bogus;</a>",
            "<",
        ] {
            assert!(Document::parse(bad).is_err(), "{bad}");
        }
    }
}
