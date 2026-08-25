use crate::types::{PdfDictionary, PdfStream, PdfValue};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XfaDocument {
    pub packets: Vec<XfaPacket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XfaScriptStats {
    pub script_nodes: usize,
    pub has_scripts: bool,
    pub script_node_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XfaPacket {
    pub name: String,
    pub root: XfaNode,
    pub source_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XfaNode {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub text: Option<String>,
    pub children: Vec<XfaNode>,
}

impl XfaDocument {
    pub fn from_acroform(acroform: &PdfDictionary) -> Result<Self, String> {
        let packets = parse_xfa_packets(acroform)?;
        Ok(Self { packets })
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    pub fn script_stats(&self) -> XfaScriptStats {
        let mut count = 0usize;
        let mut names = Vec::new();
        for packet in &self.packets {
            count_scripts(&packet.root, &mut count, &mut names);
        }
        XfaScriptStats {
            script_nodes: count,
            has_scripts: count > 0,
            script_node_names: unique_names(names),
        }
    }
}

pub fn parse_xfa_packets(acroform: &PdfDictionary) -> Result<Vec<XfaPacket>, String> {
    let xfa_value = match acroform.get("XFA") {
        Some(value) => value,
        None => return Ok(Vec::new()),
    };

    let mut packets = Vec::new();

    match xfa_value {
        PdfValue::Stream(stream) => {
            if let Some(packet) = parse_xfa_packet("xfa", stream)? {
                packets.push(packet);
            }
        }
        PdfValue::Array(items) => {
            let mut iter = items.iter();
            while let Some(name_value) = iter.next() {
                let packet_name = match name_value {
                    PdfValue::String(s) => s.decode_pdf_encoding(),
                    PdfValue::Name(n) => n.without_slash().to_string(),
                    _ => "packet".to_string(),
                };

                if let Some(packet_value) = iter.next() {
                    if let Some(packet) = parse_xfa_value(&packet_name, packet_value)? {
                        packets.push(packet);
                    }
                }
            }
        }
        _ => return Ok(Vec::new()),
    }

    Ok(packets)
}

fn parse_xfa_value(name: &str, value: &PdfValue) -> Result<Option<XfaPacket>, String> {
    match value {
        PdfValue::Stream(stream) => parse_xfa_packet(name, stream),
        PdfValue::String(s) => parse_xfa_from_bytes(name, s.as_bytes()),
        _ => Ok(None),
    }
}

fn parse_xfa_packet(name: &str, stream: &PdfStream) -> Result<Option<XfaPacket>, String> {
    let data = match stream.decode() {
        Ok(decoded) => decoded,
        Err(_) => stream.raw_data().map(|d| d.to_vec()).unwrap_or_default(),
    };
    parse_xfa_from_bytes(name, &data)
}

fn parse_xfa_from_bytes(name: &str, bytes: &[u8]) -> Result<Option<XfaPacket>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }

    let xml = String::from_utf8_lossy(bytes).to_string();
    let root = parse_xml_root(&xml)?;

    Ok(Some(XfaPacket {
        name: name.to_string(),
        root,
        source_len: bytes.len(),
    }))
}

fn parse_xml_root(xml: &str) -> Result<XfaNode, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut stack: Vec<XfaNode> = Vec::new();
    let mut root: Option<XfaNode> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let node = XfaNode {
                    name: String::from_utf8_lossy(e.name().as_ref()).to_string(),
                    attributes: parse_attributes(&reader, &e)?,
                    text: None,
                    children: Vec::new(),
                };
                stack.push(node);
            }
            Ok(Event::Empty(e)) => {
                let node = XfaNode {
                    name: String::from_utf8_lossy(e.name().as_ref()).to_string(),
                    attributes: parse_attributes(&reader, &e)?,
                    text: None,
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.is_none() {
                    root = Some(node);
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(current) = stack.last_mut() {
                    let decoded = e.decode().map_err(|e| e.to_string())?;
                    let text = quick_xml::escape::unescape(&decoded).map_err(|e| e.to_string())?;
                    let new_text = text.trim();
                    if !new_text.is_empty() {
                        let existing = current.text.take().unwrap_or_default();
                        let combined = if existing.is_empty() {
                            new_text.to_string()
                        } else {
                            format!("{}{}", existing, new_text)
                        };
                        current.text = Some(combined);
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if let Some(current) = stack.last_mut() {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    let new_text = text.trim();
                    if !new_text.is_empty() {
                        let existing = current.text.take().unwrap_or_default();
                        let combined = if existing.is_empty() {
                            new_text.to_string()
                        } else {
                            format!("{}{}", existing, new_text)
                        };
                        current.text = Some(combined);
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        root = Some(node);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XFA XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    root.ok_or_else(|| "XFA XML document has no root element".to_string())
}

fn parse_attributes(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart,
) -> Result<HashMap<String, String>, String> {
    let mut attrs = HashMap::new();
    for attr in element.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = attr
            .unescape_value()
            .map_err(|e| e.to_string())?
            .to_string();
        attrs.insert(key, value);
    }
    Ok(attrs)
}

fn count_scripts(node: &XfaNode, count: &mut usize, names: &mut Vec<String>) {
    if is_script_node(&node.name) || has_script_attribute(node) {
        *count += 1;
        names.push(node.name.clone());
    }
    for child in &node.children {
        count_scripts(child, count, names);
    }
}

fn is_script_node(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "event"
            | "calculate"
            | "validate"
            | "execute"
            | "exec"
            | "init"
            | "preSubmit"
            | "postSubmit"
            | "preOpen"
            | "postOpen"
    )
}

fn has_script_attribute(node: &XfaNode) -> bool {
    node.attributes
        .get("runAt")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || node
            .attributes
            .get("script")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
}

fn unique_names(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PdfString;

    #[test]
    fn parse_simple_xfa_xml() {
        let xml = r#"<xfa><form><field name="a">1</field></form></xfa>"#;
        let root = parse_xml_root(xml).unwrap();
        assert_eq!(root.name, "xfa");
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn parse_xfa_packet_from_string() {
        let xml = PdfString::new_literal(b"<xfa><data>ok</data></xfa>");
        let packet = parse_xfa_from_bytes("form", xml.as_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(packet.name, "form");
        assert_eq!(packet.root.name, "xfa");
    }

    #[test]
    fn xfa_script_detection() {
        let xml = PdfString::new_literal(
            b"<xfa><form><event><script>app.alert('x')</script></event></form></xfa>",
        );
        let packet = parse_xfa_from_bytes("form", xml.as_bytes())
            .unwrap()
            .unwrap();
        let doc = XfaDocument {
            packets: vec![packet],
        };
        let stats = doc.script_stats();
        assert!(stats.has_scripts);
        assert!(stats.script_nodes >= 1);
    }
}
