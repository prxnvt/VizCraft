//! JGraph's draw.io style strings: `ellipse;whiteSpace=wrap;html=1;fillColor=#dae8fc;`.
//!
//! A style is a semicolon-separated list. An item with `=` is a key/value pair; an item without
//! one is a bare name, which names a registered style (`ellipse`, `text`, `swimlane`) applied
//! before the pairs. Order is kept exactly, duplicates included, so serialising a parsed style
//! gives back an equivalent string. When a key appears twice the later value wins, matching how
//! draw.io applies the list left to right.

use serde::{Deserialize, Serialize};

use crate::attrs::{format_f64, parse_bool, parse_f64};

/// One item of a style string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleItem {
    /// A bare style name such as `ellipse` or `text`.
    Name(String),
    /// A `key=value` pair. The value may be empty (`key=`).
    Pair(String, String),
}

/// A parsed style string.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    /// Items in source order, duplicates kept.
    pub items: Vec<StyleItem>,
    /// Whether the source ended with `;`, so serialisation reproduces it.
    pub trailing_semicolon: bool,
}

/// An RGB colour parsed from a style value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
}

impl Style {
    /// Parses a style string. Empty items (`;;`) are dropped; nothing here can fail.
    pub fn parse(s: &str) -> Self {
        let mut items = Vec::new();
        for part in s.split(';') {
            if part.trim().is_empty() {
                continue;
            }
            match part.split_once('=') {
                Some((k, v)) => items.push(StyleItem::Pair(k.to_string(), v.to_string())),
                None => items.push(StyleItem::Name(part.to_string())),
            }
        }
        // Only meaningful with items: an all-empty style serialises to "" either way.
        let trailing_semicolon = !items.is_empty() && s.trim_end().ends_with(';');
        Style { items, trailing_semicolon }
    }

    /// Serialises back to a style string.
    pub fn to_style_string(&self) -> String {
        let mut out = String::new();
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            match item {
                StyleItem::Name(n) => out.push_str(n),
                StyleItem::Pair(k, v) => {
                    out.push_str(k);
                    out.push('=');
                    out.push_str(v);
                }
            }
        }
        if self.trailing_semicolon && !self.items.is_empty() {
            out.push(';');
        }
        out
    }

    /// Whether there are no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The bare style names in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|i| match i {
            StyleItem::Name(n) => Some(n.as_str()),
            StyleItem::Pair(..) => None,
        })
    }

    /// Whether the bare name `name` is present.
    pub fn has_name(&self, name: &str) -> bool {
        self.names().any(|n| n == name)
    }

    /// The effective value of `key`: the last occurrence.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.items.iter().rev().find_map(|i| match i {
            StyleItem::Pair(k, v) if k == key => Some(v.as_str()),
            _ => None,
        })
    }

    /// Sets `key`: the last occurrence is updated in place and earlier duplicates are removed,
    /// so the effective value does not change position; a new key is appended.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        let last = self.items.iter().rposition(|i| matches!(i, StyleItem::Pair(k, _) if k == key));
        match last {
            Some(idx) => {
                if let Some(StyleItem::Pair(_, v)) = self.items.get_mut(idx) {
                    *v = value;
                }
                let mut seen = 0usize;
                self.items.retain(|i| {
                    let pos = seen;
                    seen += 1;
                    pos == idx || !matches!(i, StyleItem::Pair(k, _) if k == key)
                });
            }
            None => self.items.push(StyleItem::Pair(key.to_string(), value)),
        }
    }

    /// Removes every occurrence of `key` and returns the effective value it had.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let old = self.get(key).map(str::to_string);
        self.items.retain(|i| !matches!(i, StyleItem::Pair(k, _) if k == key));
        old
    }

    /// The value of `key` as a finite number.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(parse_f64)
    }

    /// The value of `key` as a flag (`1`/`0`, `true`/`false`).
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(parse_bool)
    }

    /// Sets `key` to a number.
    pub fn set_f64(&mut self, key: &str, value: f64) {
        self.set(key, format_f64(value));
    }

    /// `fillColor`, raw (may be `none` or `default`).
    pub fn fill_color(&self) -> Option<&str> {
        self.get("fillColor")
    }

    /// `strokeColor`, raw.
    pub fn stroke_color(&self) -> Option<&str> {
        self.get("strokeColor")
    }

    /// `fontColor`, raw.
    pub fn font_color(&self) -> Option<&str> {
        self.get("fontColor")
    }

    /// `fontSize` in points.
    pub fn font_size(&self) -> Option<f64> {
        self.get_f64("fontSize")
    }

    /// `rounded`.
    pub fn rounded(&self) -> Option<bool> {
        self.get_bool("rounded")
    }

    /// `dashed`.
    pub fn dashed(&self) -> Option<bool> {
        self.get_bool("dashed")
    }

    /// `html`: whether the label is HTML.
    pub fn html(&self) -> Option<bool> {
        self.get_bool("html")
    }

    /// The `shape` key. A bare name such as `ellipse` may also imply a shape; see
    /// [`Style::names`].
    pub fn shape(&self) -> Option<&str> {
        self.get("shape")
    }

    /// `edgeStyle` (for example `orthogonalEdgeStyle`).
    pub fn edge_style(&self) -> Option<&str> {
        self.get("edgeStyle")
    }

    /// `startArrow`.
    pub fn start_arrow(&self) -> Option<&str> {
        self.get("startArrow")
    }

    /// `endArrow`.
    pub fn end_arrow(&self) -> Option<&str> {
        self.get("endArrow")
    }

    /// `exitX`: the relative x of the source connection point.
    pub fn exit_x(&self) -> Option<f64> {
        self.get_f64("exitX")
    }

    /// `exitY`.
    pub fn exit_y(&self) -> Option<f64> {
        self.get_f64("exitY")
    }

    /// `entryX`: the relative x of the target connection point.
    pub fn entry_x(&self) -> Option<f64> {
        self.get_f64("entryX")
    }

    /// `entryY`.
    pub fn entry_y(&self) -> Option<f64> {
        self.get_f64("entryY")
    }
}

impl std::fmt::Display for Style {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_style_string())
    }
}

/// Parses a colour value: `#rgb`, `#rrggbb` (case-insensitive). `none`, `default`, named colours
/// and anything malformed give `None`.
pub fn parse_color(s: &str) -> Option<Rgb> {
    let hex = s.trim().strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let digit = |i: usize| hex.get(i..i + 1).and_then(|d| u8::from_str_radix(d, 16).ok());
    match hex.len() {
        3 => Some(Rgb { r: digit(0)? * 17, g: digit(1)? * 17, b: digit(2)? * 17 }),
        6 => {
            let byte = |i: usize| hex.get(i..i + 2).and_then(|d| u8::from_str_radix(d, 16).ok());
            Some(Rgb { r: byte(0)?, g: byte(2)?, b: byte(4)? })
        }
        _ => None,
    }
}
