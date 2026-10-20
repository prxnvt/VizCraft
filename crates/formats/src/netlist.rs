//! VizCraft's own netlist format: the nets of a document and the pins each joins, written by
//! `net.export` as JSON or as text. It is our own design, not another tool's netlist.
//!
//! JSON (`"format": "vizcraft-netlist"`, `"version": 1`):
//!
//! ```json
//! {
//!   "format": "vizcraft-netlist",
//!   "version": 1,
//!   "title": "Motor starter",
//!   "nets": [
//!     {"id": "n4", "number": "3", "name": "", "kind": "wire", "properties": {"colour": "BK"},
//!      "pins": [{"device": "S1", "pin": "14", "element": "e7"}, {"device": "K1", "pin": "A1", "element": "e9"}]}
//!   ]
//! }
//! ```
//!
//! Text, one net per block, a blank line between blocks:
//!
//! ```text
//! # vizcraft-netlist 1
//! # title: Motor starter
//! net 3 (wire)
//!   S1:14
//!   K1:A1
//! ```
//!
//! The net heading is its number, else its name, else its id; `colour=BK`-style properties follow
//! the kind in the parentheses. Pins are `device:pin`, the device's tag (its element id when it
//! has none) and the port id. [`from_json`] reads the JSON form back.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// The format's name in the JSON form.
pub const FORMAT: &str = "vizcraft-netlist";

/// The format version this build writes.
pub const VERSION: u64 = 1;

/// Most nets read back from one file.
pub const MAX_NETS: usize = 100_000;

/// Most pins read back per net.
pub const MAX_PINS: usize = 10_000;

/// A netlist: a title and its nets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Netlist {
    /// The document title.
    pub title: String,
    /// The nets, in model order.
    pub nets: Vec<NetlistNet>,
}

/// One net.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetlistNet {
    /// Net id in the document.
    pub id: String,
    /// Wire number, possibly empty.
    pub number: String,
    /// Potential name, possibly empty.
    pub name: String,
    /// `wire`, `potential`, `pipe`, `fluid` or `signal`.
    pub kind: String,
    /// The net's properties as text (colour, cross-section, cable, core).
    pub properties: BTreeMap<String, String>,
    /// The pins it joins.
    pub pins: Vec<NetlistPin>,
}

/// One pin of a net.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetlistPin {
    /// The device's tag (or element id).
    pub device: String,
    /// The port id.
    pub pin: String,
    /// The element id.
    pub element: String,
}

impl NetlistNet {
    /// The heading a reader sees: the number, else the name, else the id.
    pub fn label(&self) -> &str {
        if !self.number.is_empty() {
            &self.number
        } else if !self.name.is_empty() {
            &self.name
        } else {
            &self.id
        }
    }
}

/// The JSON form.
pub fn to_json(n: &Netlist) -> Value {
    let nets: Vec<Value> = n
        .nets
        .iter()
        .map(|net| {
            json!({
                "id": net.id, "number": net.number, "name": net.name, "kind": net.kind, "properties": net.properties,
                "pins": net.pins.iter().map(|p| json!({"device": p.device, "pin": p.pin, "element": p.element})).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({"format": FORMAT, "version": VERSION, "title": n.title, "nets": nets})
}

/// The text form.
pub fn to_text(n: &Netlist) -> String {
    let mut out = format!("# {FORMAT} {VERSION}\n");
    if !n.title.is_empty() {
        out.push_str(&format!("# title: {}\n", one_line(&n.title)));
    }
    for net in &n.nets {
        out.push('\n');
        let props: Vec<String> = net.properties.iter().map(|(k, v)| format!("{}={}", one_line(k), one_line(v))).collect();
        let extra = if props.is_empty() { String::new() } else { format!(", {}", props.join(", ")) };
        out.push_str(&format!("net {} ({}{extra})\n", one_line(net.label()), one_line(&net.kind)));
        for p in &net.pins {
            out.push_str(&format!("  {}:{}\n", one_line(&p.device), one_line(&p.pin)));
        }
    }
    out
}

/// Text on one line (line breaks become spaces).
fn one_line(t: &str) -> String {
    t.replace(['\n', '\r'], " ")
}

/// Read the JSON form back; anything that is not a netlist of this format is an error.
pub fn from_json(v: &Value) -> Result<Netlist, String> {
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!("not a {FORMAT} file"));
    }
    if v.get("version").and_then(Value::as_u64).is_none_or(|n| n > VERSION) {
        return Err(format!("only {FORMAT} version {VERSION} is read"));
    }
    let text = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).unwrap_or_default().chars().take(4096).collect::<String>();
    let mut out = Netlist { title: text(v, "title"), nets: Vec::new() };
    for net in v.get("nets").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default().iter().take(MAX_NETS) {
        let properties = net
            .get("properties")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter().take(64).map(|(k, v)| (k.chars().take(64).collect(), v.as_str().unwrap_or_default().chars().take(1024).collect())).collect()
            })
            .unwrap_or_default();
        let pins = net
            .get("pins")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .take(MAX_PINS)
            .map(|p| NetlistPin { device: text(p, "device"), pin: text(p, "pin"), element: text(p, "element") })
            .collect();
        out.nets.push(NetlistNet {
            id: text(net, "id"),
            number: text(net, "number"),
            name: text(net, "name"),
            kind: text(net, "kind"),
            properties,
            pins,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Netlist {
        Netlist {
            title: "Starter".into(),
            nets: vec![
                NetlistNet {
                    id: "n1".into(),
                    number: "3".into(),
                    kind: "wire".into(),
                    properties: [("colour".to_string(), "BK".to_string())].into(),
                    pins: vec![
                        NetlistPin { device: "S1".into(), pin: "14".into(), element: "e1".into() },
                        NetlistPin { device: "K1".into(), pin: "A1".into(), element: "e2".into() },
                    ],
                    ..NetlistNet::default()
                },
                NetlistNet { id: "n2".into(), name: "L1".into(), kind: "potential".into(), ..NetlistNet::default() },
            ],
        }
    }

    #[test]
    fn json_round_trips_and_text_reads_as_documented() {
        let n = sample();
        assert_eq!(from_json(&to_json(&n)).unwrap(), n);
        let t = to_text(&n);
        assert!(t.starts_with("# vizcraft-netlist 1\n# title: Starter\n"), "{t}");
        assert!(t.contains("net 3 (wire, colour=BK)\n  S1:14\n  K1:A1\n"), "{t}");
        assert!(t.contains("net L1 (potential)\n"), "{t}");
    }

    #[test]
    fn other_files_are_refused_and_junk_is_capped() {
        assert!(from_json(&json!({"format": "other"})).is_err());
        assert!(from_json(&json!({"format": FORMAT, "version": 99})).is_err());
        let junk = json!({"format": FORMAT, "version": 1, "nets": [{"pins": [{"device": 5}], "properties": {"a": [1]}}, 7]});
        let n = from_json(&junk).unwrap();
        assert_eq!(n.nets.len(), 2);
        assert_eq!(n.nets[0].pins[0].device, "");
    }
}
