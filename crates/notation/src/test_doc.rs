//! Hand-built documents for the rules and sync tests: bound shapes drawn from the Flowchart
//! glyphs (this crate cannot use `vizcraft-stencils`, which depends on it) and flows between them.

use vizcraft_diagram::{Color, Connector, Document, End, Page, Shape, ShapeKind, Style};
use vizcraft_model::{Element, PropValue, Relationship};

use crate::catalog;

/// A free shape of `kind`, as Cascade 1 drew it.
pub fn free_shape(id: &str, x: f64) -> Shape {
    Shape::new(id, ShapeKind::Rectangle, x, 0.0, 100.0, 60.0)
}

/// A shape drawn from the glyph of element type `type_id` (any outline when the type is unknown).
pub fn glyph_shape(id: &str, type_id: &str, x: f64, y: f64) -> Shape {
    let cat = catalog();
    let glyph = cat.master_for_type(type_id).and_then(|m| cat.glyph(m)).map(|(_, g)| g);
    let kind = glyph.and_then(|g| ShapeKind::from_name(&g.outline)).unwrap_or_default();
    let mut s = Shape::new(id, kind, x, y, glyph.map_or(100.0, |g| g.w), glyph.map_or(60.0, |g| g.h));
    s.stencil = glyph.map(|g| g.id.clone()).unwrap_or_default();
    let d = Style::DEFAULT;
    s.style = Style {
        fill: glyph.and_then(|g| Color::parse(&g.fill)).unwrap_or(d.fill),
        line: glyph.and_then(|g| Color::parse(&g.line)).unwrap_or(d.line),
        ..d
    };
    s
}

/// A connector glued (dynamically) between two shapes.
pub fn glued(id: &str, from: &str, to: &str) -> Connector {
    Connector::new(id, End::dynamic(from), End::dynamic(to))
}

/// Builder for a one-page (or more) flowchart document. Element `x` is drawn by shape `s_x`;
/// relationship `r` by connector `c_r`.
pub struct Doc {
    pub doc: Document,
    next_x: f64,
}

impl Doc {
    pub fn new() -> Self {
        Doc { doc: Document::new(), next_x: 0.0 }
    }

    /// Qualify a local Flowchart type name; ids with a `.` are left as they are.
    pub fn ty(t: &str) -> String {
        if t.contains('.') { t.to_string() } else { format!("flowchart.{t}") }
    }

    /// An element with no view.
    pub fn element(&mut self, id: &str, ty: &str, name: &str) -> &mut Self {
        let mut e = Element::new(id, Self::ty(ty));
        e.name = name.into();
        self.doc.model.add_element(e).unwrap();
        self
    }

    /// An element drawn by shape `s_<id>` on page `page`.
    pub fn el_on(&mut self, page: usize, id: &str, ty: &str, name: &str) -> &mut Self {
        self.element(id, ty, name);
        let mut s = glyph_shape(&format!("s_{id}"), &Self::ty(ty), self.next_x, 0.0);
        self.next_x += 200.0;
        s.element = Some(id.into());
        s.text = name.into();
        while self.doc.pages.len() <= page {
            let n = self.doc.pages.len() + 1;
            self.doc.pages.push(Page::new(format!("p{n}"), format!("Page-{n}")));
        }
        self.doc.pages[page].shapes.push(s);
        self
    }

    /// An element drawn on the first page.
    pub fn el(&mut self, id: &str, ty: &str, name: &str) -> &mut Self {
        self.el_on(0, id, ty, name)
    }

    pub fn tag(&mut self, id: &str, tag: &str) -> &mut Self {
        self.doc.model.element_mut(id).unwrap().tag = tag.into();
        self
    }

    pub fn prop(&mut self, id: &str, key: &str, v: PropValue) -> &mut Self {
        self.doc.model.element_mut(id).unwrap().properties.insert(key.into(), v);
        self
    }

    /// A Flow relationship with no view.
    pub fn rel(&mut self, rid: &str, from: &str, to: &str) -> &mut Self {
        self.doc.model.add_relationship(Relationship::new(rid, "flowchart.Flow", from, to)).unwrap();
        self
    }

    /// A Flow relationship drawn by connector `c_<rid>` between the elements' shapes, on the
    /// page of the source shape.
    pub fn flow(&mut self, rid: &str, from: &str, to: &str) -> &mut Self {
        self.rel(rid, from, to);
        let (fs, ts) = (format!("s_{from}"), format!("s_{to}"));
        let page = self.doc.pages.iter_mut().find(|p| p.shape(&fs).is_some()).unwrap();
        let mut c = glued(&format!("c_{rid}"), &fs, &ts);
        c.relationship = Some(rid.into());
        page.connectors.push(c);
        self
    }

    pub fn build(&mut self) -> Document {
        // Keep `next_id` clear of the hand-picked ids.
        self.doc.id_seq = self.doc.id_seq.max(100);
        self.doc.clone()
    }
}
