//! A small, bounded XML element tree over quick-xml, plus the escaping the writers need.
//!
//! The codec works on a tree rather than on events because the structure of JGraph's draw.io files
//! is shallow and the model reader needs to look ahead (a `UserObject` holds its `mxCell`). Every
//! element keeps its byte span in the source so unchanged pages and inline models can be copied
//! verbatim.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::attrs::Attributes;
use crate::{DrawioError, Result, limits};

/// One parsed element: name, attributes in document order, child elements and the text directly
/// inside it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Element {
    pub name: String,
    pub attrs: Attributes,
    pub children: Vec<Element>,
    pub text: String,
    /// Byte range of the whole element in the source, from `<` of the start tag to the end of
    /// the end tag.
    pub span: (usize, usize),
}

impl Element {
    /// The first child with this element name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Parses `src` into its root element, ignoring the declaration, comments, processing
/// instructions and the doctype. Errors on malformed XML, more than one root, or when the depth
/// or element count limits are exceeded.
pub(crate) fn parse(src: &str) -> Result<Element> {
    let mut reader = Reader::from_str(src);
    let config = reader.config_mut();
    config.trim_text(false);
    config.check_end_names = true;
    config.expand_empty_elements = false;

    // The stack holds open elements; finished ones are attached to their parent, so the tree is
    // built without recursion and the depth cap bounds the stack.
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut count: usize = 0;
    loop {
        let before = pos(&reader);
        let event = reader.read_event().map_err(|e| DrawioError::Xml(format!("at byte {before}: {e}")))?;
        let after = pos(&reader);
        match event {
            Event::Start(start) => {
                count = bump(count)?;
                if stack.len() >= limits::MAX_XML_DEPTH {
                    return Err(DrawioError::Limit(format!("XML nesting deeper than {}", limits::MAX_XML_DEPTH)));
                }
                if root.is_some() && stack.is_empty() {
                    return Err(DrawioError::Xml("more than one root element".into()));
                }
                let mut el = element_from(&start)?;
                el.span = (before, after);
                stack.push(el);
            }
            Event::Empty(start) => {
                count = bump(count)?;
                let mut el = element_from(&start)?;
                el.span = (before, after);
                attach(&mut stack, &mut root, el)?;
            }
            Event::End(_) => {
                let Some(mut el) = stack.pop() else {
                    return Err(DrawioError::Xml("unmatched end tag".into()));
                };
                el.span.1 = after;
                attach(&mut stack, &mut root, el)?;
            }
            Event::Text(text) => {
                let s = text.xml_content().map_err(|e| DrawioError::Xml(e.to_string()))?;
                push_text(&mut stack, &s)?;
            }
            Event::CData(data) => {
                let s = data.decode().map_err(|e| DrawioError::Xml(e.to_string()))?;
                push_text(&mut stack, &s)?;
            }
            Event::GeneralRef(r) => {
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => c.to_string(),
                    Ok(None) => {
                        let name = r.decode().map_err(|e| DrawioError::Xml(e.to_string()))?;
                        predefined_entity(&name).map(str::to_string).unwrap_or_else(|| format!("&{name};"))
                    }
                    Err(e) => return Err(DrawioError::Xml(e.to_string())),
                };
                push_text(&mut stack, &resolved)?;
            }
            Event::Eof => break,
            Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => {}
        }
    }
    if !stack.is_empty() {
        return Err(DrawioError::Xml("unexpected end of document inside an element".into()));
    }
    root.ok_or_else(|| DrawioError::Xml("no root element".into()))
}

fn pos(reader: &Reader<&[u8]>) -> usize {
    usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX)
}

fn bump(count: usize) -> Result<usize> {
    let next = count.saturating_add(1);
    if next > limits::MAX_XML_ELEMENTS {
        return Err(DrawioError::Limit(format!("more than {} XML elements", limits::MAX_XML_ELEMENTS)));
    }
    Ok(next)
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(el);
        Ok(())
    } else if root.is_some() {
        Err(DrawioError::Xml("more than one root element".into()))
    } else {
        *root = Some(el);
        Ok(())
    }
}

fn push_text(stack: &mut [Element], s: &str) -> Result<()> {
    // Text outside the root is only whitespace in a well-formed document; ignore it.
    if let Some(top) = stack.last_mut() {
        if top.text.len().saturating_add(s.len()) > limits::MAX_PAGE_BYTES {
            return Err(DrawioError::Limit("element text too long".into()));
        }
        top.text.push_str(s);
    }
    Ok(())
}

fn predefined_entity(name: &str) -> Option<&'static str> {
    match name {
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        _ => None,
    }
}

fn element_from(start: &BytesStart<'_>) -> Result<Element> {
    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attrs = Attributes::default();
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| DrawioError::Xml(format!("bad attribute in <{name}>: {e}")))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        // An unknown entity such as `&nbsp;` makes strict unescaping fail; keep the raw text
        // rather than rejecting a whole file over one label.
        let value = match attr.unescape_value() {
            Ok(v) => v.into_owned(),
            Err(_) => String::from_utf8_lossy(&attr.value).into_owned(),
        };
        attrs.push(key, value);
    }
    Ok(Element { name, attrs, ..Element::default() })
}

/// Escapes an attribute value for double-quoted output. Newlines and tabs become character
/// references because XML attribute normalisation would otherwise turn them into spaces.
pub(crate) fn escape_attr(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xa;"),
            '\r' => out.push_str("&#xd;"),
            '\t' => out.push_str("&#x9;"),
            _ => out.push(c),
        }
    }
}

/// Writes ` key="value"` for each attribute.
pub(crate) fn write_attrs(out: &mut String, attrs: &Attributes) {
    for (k, v) in attrs.iter() {
        write_attr(out, k, v);
    }
}

/// Writes one ` key="value"` pair.
pub(crate) fn write_attr(out: &mut String, key: &str, value: &str) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    escape_attr(out, value);
    out.push('"');
}
