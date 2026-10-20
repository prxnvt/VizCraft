//! Connector line ends: the [`Arrow`] enum and the one geometry table ([`outline`]) that the
//! canvas, SVG and PDF export all draw from.
//!
//! The first five ends (triangle, open, diamond, circle) are the general-purpose heads. The rest
//! are the notation ends later packs need: hollow triangle and diamond (OMG UML inheritance and
//! aggregation), bar and the crow's foot family (Information Engineering cardinality) and open
//! circle. Every size grows with the line width, so a heavy line gets a heavy head.

use crate::geom::{Path, PathEl, Point, Rect, Vec2};

/// An arrowhead style at one end of a connector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Arrow {
    /// No arrowhead. Unknown arrow names in a file load as this.
    #[default]
    None,
    /// Filled triangle.
    Triangle,
    /// Two open strokes (a chevron).
    Open,
    /// Filled diamond.
    Diamond,
    /// Filled circle.
    Circle,
    /// Hollow triangle (UML generalisation).
    HollowTriangle,
    /// Hollow diamond (UML aggregation).
    HollowDiamond,
    /// A short bar across the line (exactly one).
    Bar,
    /// A hollow circle.
    OpenCircle,
    /// Crow's foot: three prongs fanning out at the shape (many).
    CrowsFoot,
    /// Crow's foot with a bar behind it (one or many).
    CrowsFootBar,
    /// Crow's foot with a hollow circle behind it (zero or many).
    CrowsFootCircle,
    /// A bar with a hollow circle behind it (zero or one).
    BarCircle,
    /// A short slanted stroke across the line near the end (OMG BPMN's default-flow marker, drawn
    /// at the source end). The line runs through it.
    Slash,
    /// Two bars across the line (one and only one, Information Engineering).
    DoubleBar,
    /// A small filled dot on the line end (the IDEF1X child end).
    Dot,
}

crate::serde_util::named_enum!(Arrow, Arrow::None, {
    Arrow::None => "none",
    Arrow::Triangle => "triangle",
    Arrow::Open => "open",
    Arrow::Diamond => "diamond",
    Arrow::Circle => "circle",
    Arrow::HollowTriangle => "hollowTriangle",
    Arrow::HollowDiamond => "hollowDiamond",
    Arrow::Bar => "bar",
    Arrow::OpenCircle => "openCircle",
    Arrow::CrowsFoot => "crowsFoot",
    Arrow::CrowsFootBar => "crowsFootBar",
    Arrow::CrowsFootCircle => "crowsFootCircle",
    Arrow::BarCircle => "barCircle",
    Arrow::Slash => "slash",
    Arrow::DoubleBar => "doubleBar",
    Arrow::Dot => "dot",
});

impl Arrow {
    /// Every end, in the order pickers list them.
    pub const ALL: [Arrow; 14] = [
        Arrow::None,
        Arrow::Triangle,
        Arrow::Open,
        Arrow::Diamond,
        Arrow::Circle,
        Arrow::HollowTriangle,
        Arrow::HollowDiamond,
        Arrow::Bar,
        Arrow::OpenCircle,
        Arrow::CrowsFoot,
        Arrow::CrowsFootBar,
        Arrow::CrowsFootCircle,
        Arrow::BarCircle,
        Arrow::Slash,
    ];
    /// Every end there is: [`Arrow::ALL`] (the picker list) plus the notation ends the pickers
    /// do not offer yet.
    pub const EVERY: [Arrow; 16] = [
        Arrow::None,
        Arrow::Triangle,
        Arrow::Open,
        Arrow::Diamond,
        Arrow::Circle,
        Arrow::HollowTriangle,
        Arrow::HollowDiamond,
        Arrow::Bar,
        Arrow::OpenCircle,
        Arrow::CrowsFoot,
        Arrow::CrowsFootBar,
        Arrow::CrowsFootCircle,
        Arrow::BarCircle,
        Arrow::Slash,
        Arrow::DoubleBar,
        Arrow::Dot,
    ];
}

/// The drawing of one line end, from [`outline`].
#[derive(Clone, Debug, PartialEq)]
pub struct ArrowGeometry {
    /// The strokes and outlines of the head: closed for triangles, diamonds and circles, open for
    /// chevrons, bars and prongs.
    pub paths: Vec<Path>,
    /// Whether to fill the paths with the line colour (otherwise they are only stroked).
    pub filled: bool,
    /// How far back from the tip the connector line is shortened so it does not poke through the
    /// head, in world units.
    pub trim: f64,
}

/// The geometry of one arrowhead as a connector carries it: the paths merged into one, and the
/// point where the line should stop.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrowhead {
    /// The head's outline: every stroke of [`ArrowGeometry::paths`] as subpaths of one path.
    pub path: Path,
    /// Whether to fill the outline with the line colour.
    pub filled: bool,
    /// Where the connector line should stop so it does not poke through the head.
    pub line_end: Point,
}

/// A circle of radius `r` centred on `c`, as a closed path.
fn circle(c: Point, r: f64) -> Path {
    crate::kind::ellipse(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r))
}

/// The three prongs of a crow's foot: the fork sits `len` behind `tip` and the outer prongs end
/// `half` to each side of the line at the tip.
fn prongs(tip: Point, dir: Vec2, len: f64, half: f64) -> Vec<Path> {
    let side = dir.perp() * half;
    let fork = tip - dir * len;
    vec![Path::polyline(&[tip + side, fork, tip - side]), Path::polyline(&[fork, tip])]
}

/// A bar across the line, `at` behind `tip`, reaching `half` to each side.
fn bar(tip: Point, dir: Vec2, at: f64, half: f64) -> Path {
    let c = tip - dir * at;
    let side = dir.perp() * half;
    Path::polyline(&[c + side, c - side])
}

/// The drawing of `arrow` at an end `tip`, with the line arriving along unit direction `dir`
/// (pointing into the tip). Sizes grow with `line_width`. `None` for [`Arrow::None`] or a
/// direction that is zero or not finite.
pub fn outline(arrow: Arrow, tip: Point, dir: Vec2, line_width: f64) -> Option<ArrowGeometry> {
    let dir = dir.normalized()?;
    let lw = if line_width.is_finite() { line_width.clamp(0.0, 100.0) } else { 1.0 };
    let len = 8.0 + 2.5 * lw;
    let half = len * 0.4;
    let back = tip - dir * len;
    let side = dir.perp() * half;
    // Open circles and the circles of the crow's foot family are smaller than the end circle so
    // they sit beside the bar or fork without crowding it.
    let small = len * 0.3;
    let geometry = |paths: Vec<Path>, filled: bool, trim: f64| Some(ArrowGeometry { paths, filled, trim });
    match arrow {
        Arrow::None => None,
        Arrow::Triangle => geometry(vec![Path::polygon(&[tip, back + side, back - side])], true, len),
        Arrow::Open => geometry(vec![Path::polyline(&[back + side, tip, back - side])], false, 0.0),
        Arrow::Diamond | Arrow::HollowDiamond => {
            let mid = tip - dir * (len * 0.6);
            let tail = tip - dir * (len * 1.2);
            geometry(vec![Path::polygon(&[tip, mid + side, tail, mid - side])], arrow == Arrow::Diamond, len * 1.2)
        }
        Arrow::Circle => geometry(vec![circle(tip - dir * half, half)], true, 2.0 * half),
        Arrow::HollowTriangle => geometry(vec![Path::polygon(&[tip, back + side, back - side])], false, len),
        Arrow::Bar => geometry(vec![bar(tip, dir, len * 0.25, half)], false, 0.0),
        Arrow::OpenCircle => geometry(vec![circle(tip - dir * half, half)], false, 2.0 * half),
        Arrow::CrowsFoot => geometry(prongs(tip, dir, len, half), false, len),
        Arrow::CrowsFootBar => {
            let at = len * 1.25;
            let mut paths = prongs(tip, dir, len, half);
            paths.push(bar(tip, dir, at, half));
            geometry(paths, false, at)
        }
        Arrow::CrowsFootCircle => {
            let mut paths = prongs(tip, dir, len, half);
            paths.push(circle(tip - dir * (len + small), small));
            geometry(paths, false, len + 2.0 * small)
        }
        Arrow::BarCircle => {
            let at = len * 0.25;
            let gap = len * 0.3;
            geometry(vec![bar(tip, dir, at, half), circle(tip - dir * (at + gap + small), small)], false, at + gap + 2.0 * small)
        }
        Arrow::Slash => {
            // Slanted towards the tip, a line length back from it, so it reads as a mark on the
            // line rather than a head.
            let c = tip - dir * len;
            let lean = dir * (half * 0.6);
            geometry(vec![Path::polyline(&[c - side - lean, c + side + lean])], false, 0.0)
        }
        Arrow::DoubleBar => geometry(vec![bar(tip, dir, len * 0.25, half), bar(tip, dir, len * 0.55, half)], false, 0.0),
        Arrow::Dot => {
            let r = len * 0.3;
            geometry(vec![circle(tip - dir * r, r)], true, 2.0 * r)
        }
    }
}

/// Where the label beside one end of a connector is centred: a little along the route from the
/// end (past any head) and a little to its side, so it sits outside the line and away from the
/// shape the end touches. `pts` is the routed polyline; `at_start` picks its first or last end.
/// `font_size` and `line_width` scale the offsets. `None` for a route with no length.
pub fn end_label_anchor(pts: &[Point], at_start: bool, font_size: f64, line_width: f64) -> Option<Point> {
    let end = if at_start { *pts.first()? } else { *pts.last()? };
    // Direction from the end into the route: the first segment with any length.
    let away = |p: &Point| (*p - end).normalized();
    let into = if at_start { pts.iter().skip(1).find_map(away) } else { pts.iter().rev().skip(1).find_map(away) }?;
    let fs = if font_size.is_finite() { font_size.clamp(1.0, 400.0) } else { 12.0 };
    let lw = if line_width.is_finite() { line_width.clamp(0.0, 100.0) } else { 1.0 };
    // Past the largest head (see `outline`) and half a label along.
    let along = 8.0 + 2.5 * lw + fs;
    let side = fs * 0.9 + lw / 2.0;
    Some(end + into * along + into.perp() * side)
}

/// The arrowhead for an end at `tip`, with the line arriving along unit direction `dir`: the
/// [`outline`] with its paths merged and the trim turned into the point where the line stops.
/// `None` for [`Arrow::None`] or a bad direction.
pub fn arrowhead(arrow: Arrow, tip: Point, dir: Vec2, line_width: f64) -> Option<Arrowhead> {
    let g = outline(arrow, tip, dir, line_width)?;
    let unit = dir.normalized()?;
    let els: Vec<PathEl> = g.paths.into_iter().flat_map(|p| p.els).collect();
    Some(Arrowhead { path: Path { els }, filled: g.filled, line_end: tip - unit * g.trim })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIP: Point = Point { x: 100.0, y: 0.0 };
    const DIR: Point = Point { x: 1.0, y: 0.0 };

    #[test]
    fn names_round_trip_and_unknown_names_are_none() {
        for a in Arrow::EVERY {
            assert_eq!(Arrow::from_name(a.name()), Some(a));
        }
        assert_eq!(Arrow::from_name("hollowTriangle"), Some(Arrow::HollowTriangle));
        assert_eq!(Arrow::from_name("crowsFootCircle"), Some(Arrow::CrowsFootCircle));
        assert_eq!(Arrow::from_name("harpoon"), None);
        assert_eq!(serde_json::from_str::<Arrow>("\"harpoon\"").unwrap(), Arrow::None);
        assert_eq!(serde_json::from_str::<Arrow>("7").unwrap(), Arrow::None);
        assert_eq!(serde_json::to_string(&Arrow::BarCircle).unwrap(), "\"barCircle\"");
    }

    #[test]
    fn every_end_stays_behind_the_tip_and_trims_the_line() {
        for a in Arrow::EVERY.into_iter().filter(|a| *a != Arrow::None) {
            let g = outline(a, TIP, DIR, 1.0).unwrap();
            assert!(!g.paths.is_empty(), "{a:?}");
            assert!(g.trim >= 0.0, "{a:?}");
            let b = g.paths.iter().filter_map(|p| p.bounds()).reduce(|x, y| x.union(&y)).unwrap();
            assert!(b.x1 <= TIP.x + 1e-9, "{a:?} reaches past the tip");
            // A head the line runs through (trim 0) may reach back; any other covers the line it cuts.
            assert!(g.trim == 0.0 || b.x0 >= TIP.x - g.trim - 1e-9, "{a:?} draws beyond where the line stops");
            assert!(b.y0 < 0.0 && b.y1 > 0.0, "{a:?} has no width");
        }
        assert!(outline(Arrow::None, TIP, DIR, 1.0).is_none());
        assert!(outline(Arrow::Triangle, TIP, Point::ZERO, 1.0).is_none());
    }

    #[test]
    fn only_solid_heads_are_filled() {
        let filled: Vec<Arrow> = Arrow::EVERY.into_iter().filter(|a| outline(*a, TIP, DIR, 1.0).is_some_and(|g| g.filled)).collect();
        assert_eq!(filled, vec![Arrow::Triangle, Arrow::Diamond, Arrow::Circle, Arrow::Dot]);
    }

    #[test]
    fn heads_grow_with_line_width_and_survive_junk_widths() {
        for a in Arrow::EVERY.into_iter().filter(|a| *a != Arrow::None) {
            let thin = outline(a, TIP, DIR, 1.0).unwrap();
            let thick = outline(a, TIP, DIR, 6.0).unwrap();
            assert!(thick.trim > thin.trim || thin.trim == 0.0, "{a:?}");
            for w in [f64::NAN, f64::INFINITY, -5.0, 1e300] {
                let g = outline(a, TIP, DIR, w).unwrap();
                assert!(g.trim.is_finite() && g.paths.iter().all(|p| p.bounds().is_none_or(|b| b.x1.is_finite())), "{a:?} {w}");
            }
        }
    }

    #[test]
    fn slash_crosses_the_line_behind_the_tip_without_trimming_it() {
        let g = outline(Arrow::Slash, TIP, DIR, 1.0).unwrap();
        assert_eq!((g.paths.len(), g.filled, g.trim), (1, false, 0.0));
        let b = g.paths[0].bounds().unwrap();
        assert!(b.x0 < b.x1 && b.x1 < TIP.x, "slanted and behind the tip: {b:?}");
        assert_eq!(Arrow::from_name("slash"), Some(Arrow::Slash));
    }

    #[test]
    fn double_bar_draws_two_bars_and_dot_a_small_filled_circle() {
        let g = outline(Arrow::DoubleBar, TIP, DIR, 1.0).unwrap();
        assert_eq!((g.paths.len(), g.filled, g.trim), (2, false, 0.0));
        let xs: Vec<f64> = g.paths.iter().map(|p| p.bounds().unwrap().x0).collect();
        assert!(xs[0] > xs[1] && xs[1] < TIP.x, "two bars behind the tip: {xs:?}");
        let d = outline(Arrow::Dot, TIP, DIR, 1.0).unwrap();
        let b = d.paths[0].bounds().unwrap();
        let big = outline(Arrow::Circle, TIP, DIR, 1.0).unwrap().paths[0].bounds().unwrap();
        assert!(d.filled && b.height() < big.height(), "smaller than the end circle");
        assert_eq!(Arrow::from_name("doubleBar"), Some(Arrow::DoubleBar));
        assert_eq!(Arrow::ALL.len(), 14, "the picker list stays as it was");
    }

    #[test]
    fn end_labels_sit_beside_each_end_off_the_line() {
        let pts = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 80.0)];
        let s = end_label_anchor(&pts, true, 12.0, 1.0).unwrap();
        assert!(s.x > 0.0 && s.x < 50.0 && s.y.abs() > 5.0, "{s:?}");
        let e = end_label_anchor(&pts, false, 12.0, 1.0).unwrap();
        assert!(e.y < 80.0 && e.y > 30.0 && (e.x - 100.0).abs() > 5.0, "{e:?}");
        assert!(end_label_anchor(&[Point::ZERO, Point::ZERO], true, 12.0, 1.0).is_none());
        assert!(end_label_anchor(&[], false, 12.0, 1.0).is_none());
        assert!(end_label_anchor(&pts, true, f64::NAN, f64::INFINITY).is_some_and(|p| p.is_finite()));
    }

    #[test]
    fn arrowhead_merges_the_paths_and_stops_the_line_at_the_trim() {
        let t = arrowhead(Arrow::Triangle, TIP, DIR, 1.0).unwrap();
        assert!(t.filled && t.line_end.x < TIP.x);
        let f = arrowhead(Arrow::CrowsFootCircle, TIP, DIR, 1.0).unwrap();
        assert_eq!(f.path.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 3);
        assert!(!f.filled);
        let open = arrowhead(Arrow::Open, TIP, DIR, 1.0).unwrap();
        assert_eq!(open.line_end, TIP);
    }
}
