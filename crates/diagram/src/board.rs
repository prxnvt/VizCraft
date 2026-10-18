//! The board a page is drawn on: an infinite canvas (the default) or a fixed sheet of paper.
//!
//! Everything that needs "the page rectangle" (the canvas, zoom to fit, rulers, exports, print,
//! the draw.io writer) asks [`Page::frame`] instead of reading `width` and `height`, so the two
//! kinds of board behave the same way everywhere. `width` and `height` stay on the page as the
//! sheet size a fixed board uses and a switch back to Fixed restores.

use crate::connector_geometry;
use crate::geom::Rect;
use crate::model::Page;

/// Space around the drawing in an infinite board's frame, and the margin `page.fitToDrawing`
/// leaves by default.
pub const FIT_MARGIN: f64 = 25.0;

/// Whether a page is an unbounded canvas or a sheet of paper.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Board {
    /// No sheet: the page is as large as its drawing. The default for new pages.
    #[default]
    Infinite,
    /// A sheet of `width` by `height` at the origin. Unknown names load as this, and so does a
    /// page with no `board` key (a file from before boards keeps its look).
    Fixed,
}

crate::serde_util::named_enum!(Board, Board::Fixed, {
    Board::Infinite => "infinite",
    Board::Fixed => "fixed",
});

impl Board {
    /// The board a page gets when its file has no `board` key.
    pub(crate) fn fixed() -> Self {
        Board::Fixed
    }
    /// Whether this is the infinite board.
    pub fn is_infinite(self) -> bool {
        self == Board::Infinite
    }
}

/// Bounds of every shape and connector route on `page`, or `None` when it has none.
pub fn drawing_bounds(page: &Page) -> Option<Rect> {
    let shapes = page.shapes.iter().map(|s| s.bounds());
    let connectors = page.connectors.iter().filter_map(|c| connector_geometry(page, c).path.bounds());
    shapes.chain(connectors).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
}

impl Page {
    /// The page rectangle everything frames itself on. A fixed board gives its sheet,
    /// `(0, 0, width, height)`. An infinite board gives the drawing's bounds grown by
    /// [`FIT_MARGIN`], or a Letter-sized frame at the origin when the page is empty.
    pub fn frame(&self) -> Rect {
        match self.board {
            Board::Fixed => self.rect(),
            Board::Infinite => match drawing_bounds(self) {
                Some(b) => b.inflate(FIT_MARGIN),
                None => Rect::from_xywh(0.0, 0.0, Self::DEFAULT_WIDTH, Self::DEFAULT_HEIGHT),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ShapeKind;
    use crate::model::{Connector, End, Shape};

    fn shape(id: &str, x: f64, y: f64) -> Shape {
        Shape::new(id, ShapeKind::Rectangle, x, y, 100.0, 60.0)
    }

    #[test]
    fn names_round_trip_and_unknown_is_fixed() {
        assert_eq!(serde_json::to_string(&Board::Infinite).unwrap(), "\"infinite\"");
        assert_eq!(serde_json::to_string(&Board::Fixed).unwrap(), "\"fixed\"");
        assert_eq!(serde_json::from_str::<Board>("\"infinite\"").unwrap(), Board::Infinite);
        assert_eq!(serde_json::from_str::<Board>("\"endless\"").unwrap(), Board::Fixed);
        assert_eq!(serde_json::from_str::<Board>("7").unwrap(), Board::Fixed);
    }

    #[test]
    fn a_page_without_the_key_is_fixed_and_a_new_one_is_infinite() {
        let p: Page = serde_json::from_str(r#"{"id":"p1","name":"A","width":500,"height":400}"#).unwrap();
        assert_eq!(p.board, Board::Fixed);
        assert_eq!(Page::new("p1", "A").board, Board::Infinite);
        assert_eq!(Page::default().board, Board::Infinite);
        let p: Page = serde_json::from_str(r#"{"board":"infinite"}"#).unwrap();
        assert_eq!(p.board, Board::Infinite);
        let p: Page = serde_json::from_str(r#"{"board":"sideways"}"#).unwrap();
        assert_eq!(p.board, Board::Fixed);
        let back: Page = serde_json::from_str(&serde_json::to_string(&Page::new("p1", "A")).unwrap()).unwrap();
        assert_eq!(back.board, Board::Infinite);
    }

    #[test]
    fn fixed_frame_is_the_sheet() {
        let mut p = Page::new("p1", "A");
        p.board = Board::Fixed;
        p.width = 500.0;
        p.height = 300.0;
        p.shapes.push(shape("a", 2000.0, 2000.0));
        assert_eq!(p.frame(), Rect::from_xywh(0.0, 0.0, 500.0, 300.0));
    }

    #[test]
    fn empty_infinite_frame_is_letter_at_the_origin() {
        let mut p = Page::new("p1", "A");
        p.width = 10.0;
        assert_eq!(p.frame(), Rect::from_xywh(0.0, 0.0, Page::DEFAULT_WIDTH, Page::DEFAULT_HEIGHT));
    }

    #[test]
    fn infinite_frame_wraps_shapes_and_routes_with_the_margin() {
        let mut p = Page::new("p1", "A");
        p.shapes.push(shape("a", 100.0, 200.0));
        let b = p.shapes[0].bounds();
        assert_eq!(p.frame(), b.inflate(FIT_MARGIN));
        p.shapes.push(shape("b", -300.0, 50.0));
        p.shapes.push(shape("c", 900.0, 700.0));
        let glued = |id: &str| End::Glued { shape: id.into(), point: None };
        p.connectors.push(Connector::new("c1", End::Free { x: -500.0, y: 40.0 }, glued("c")));
        let f = p.frame();
        assert!(f.x0 <= -500.0 - FIT_MARGIN + 1e-6, "{f:?}");
        assert!(f.x1 >= p.shapes[2].bounds().x1 + FIT_MARGIN - 1e-6, "{f:?}");
    }
}
