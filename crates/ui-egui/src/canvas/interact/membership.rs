//! Container membership from the canvas. A move drag that takes shapes into or fully out of a
//! container, and a master dropped inside one, commit as one `edit.batch` with
//! `container.drop`, so the move and the membership change are one undo step. Moves that change
//! no membership stay a plain `shape.move`.

use serde_json::json;
use vizcraft_diagram::container as ct;
use vizcraft_diagram::{Point, Rect};

use super::run;
use crate::CraftApp;

/// Commit a move of `ids` by `d`, with the membership changes it makes.
pub fn commit_move(app: &mut CraftApp, ids: &[String], d: Point) {
    let mv = json!({"ids": ids, "dx": d.x, "dy": d.y});
    let changes = app.session.doc.page().map(|p| ct::drop_changes(p, ids, d.x, d.y)).unwrap_or_default();
    // Devices moved into a ladder frame settle on a rung in the same undo step.
    let onto_ladder: Vec<String> = app
        .session
        .doc
        .page()
        .map(|p| {
            let doc = &app.session.doc;
            ids.iter()
                .filter(|id| !super::ladder::is_ladder(doc, p, id))
                .filter(|id| p.shape(id).is_some_and(|s| super::ladder::in_ladder(doc, p, Point::new(s.center().x + d.x, s.center().y + d.y))))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if changes.is_empty() && !onto_ladder.is_empty() {
        let place = json!({"shapes": onto_ladder});
        run(
            app,
            "edit.batch",
            json!({"label": "Move", "commands": [{"command": "shape.move", "params": mv}, {"command": "ladder.place", "params": place}]}),
        );
        return;
    }
    if changes.is_empty() {
        run(app, "shape.move", mv);
        return;
    }
    let drop = json!({"ids": ids, "dx": d.x, "dy": d.y});
    run(
        app,
        "edit.batch",
        json!({"label": "Move", "commands": [{"command": "container.drop", "params": drop}, {"command": "shape.move", "params": mv}]}),
    );
}

/// Add master `stencil` with its top-left at `x, y` (`w` by `h`), joining the container it lands
/// fully inside.
pub fn insert(app: &mut CraftApp, stencil: &str, x: f64, y: f64, w: f64, h: f64) {
    let add = json!({"stencil": stencil, "x": x, "y": y});
    let r = Rect::from_xywh(x, y, w, h);
    let inside = app
        .session
        .doc
        .page()
        .is_some_and(|p| p.shapes.iter().any(|s| s.container.as_ref().is_some_and(ct::Container::holds_members) && s.bounds().contains_rect(&r)));
    let on_ladder = app.session.doc.page().is_some_and(|p| super::ladder::in_ladder(&app.session.doc, p, Point::new(x + w / 2.0, y + h / 2.0)));
    if on_ladder && !inside {
        // `shape.add` selects the new shape, so `ladder.place` without shapes sets it on a rung.
        run(app, "edit.batch", json!({"label": "Add Shape", "commands": [{"command": "shape.add", "params": add}, {"command": "ladder.place"}]}));
        return;
    }
    if inside {
        // `shape.add` selects the new shape, so `container.drop` without ids settles it.
        run(app, "edit.batch", json!({"label": "Add Shape", "commands": [{"command": "shape.add", "params": add}, {"command": "container.drop"}]}));
    } else {
        run(app, "shape.add", add);
    }
}
