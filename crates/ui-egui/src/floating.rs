//! Floating selection (Photoshop): ⌘-dragging a selection with a selection tool cuts the selected
//! pixels and leaves them *floating* above the layer. Further drags inside the ants (no ⌘ needed)
//! move the floating piece again without cutting anything new. Nothing touches the document until
//! the piece is dropped: deselecting, or any other command, commits it as one `edit.transform`
//! (a whole-pixel move of the selected pixels and the selection, one history step); ⌘Z puts it
//! back where it came from.
//!
//! The document itself never changes while floating: the canvas shows it through
//! `move_ui::display_doc` (the cut piece at `offset`) and draws the ants at `offset`.

use photocraft_doc::{DocId, LayerId};
use serde_json::json;

use crate::PhotocraftApp;

/// A cut piece floating `offset` whole pixels from where it was cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Floating {
    pub doc: DocId,
    /// The document revision it floats over; any edit since drops it (see [`active`]).
    pub revision: u64,
    pub layer: LayerId,
    pub offset: (i32, i32),
}

/// The floating piece over the active document, if any (and still valid for it).
pub fn active(app: &PhotocraftApp) -> Option<Floating> {
    let f = app.floating?;
    let st = app.session.active()?;
    (st.doc.id == f.doc && st.revision == f.revision && st.doc.selection.is_some()).then_some(f)
}

/// A drag of the floating piece ended: it now floats `delta` further.
pub fn moved(app: &mut PhotocraftApp, delta: (i32, i32)) {
    let current = active(app);
    let Some(st) = app.session.active() else { return };
    let (layer, base) = match current {
        Some(f) => (f.layer, f.offset),
        None => match st.active_layer {
            Some(l) => (l, (0, 0)),
            None => return,
        },
    };
    let offset = (base.0 + delta.0, base.1 + delta.1);
    app.floating = Some(Floating { doc: st.doc.id, revision: st.revision, layer, offset });
}

/// Is document point `p` on the floating piece (inside its ants, at its offset)?
pub fn hit(app: &PhotocraftApp, p: [f64; 2]) -> bool {
    active(app).is_some_and(|f| crate::canvas_menu::inside_selection(app, [p[0] - f64::from(f.offset.0), p[1] - f64::from(f.offset.1)]))
}

/// Drop the floating piece into its layer (one `edit.transform`). True if there was one.
pub fn commit(app: &mut PhotocraftApp) -> bool {
    let Some(f) = active(app) else {
        app.floating = None;
        return false;
    };
    app.floating = None;
    app.move_preview = None;
    if f.offset == (0, 0) {
        return true;
    }
    let (dx, dy) = f.offset;
    let prev = app.session.active().and_then(|st| st.active_layer);
    if prev != Some(f.layer) {
        let _ = app.run("layer.select", json!({"layer": f.layer.0}));
    }
    if let Err(e) = app.run("edit.transform", json!({"matrix": [1, 0, 0, 1, dx, dy], "interpolation": "nearest"})) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    true
}

/// ⌘Z while floating: put the piece back (the document never changed). True if there was one.
pub fn cancel(app: &mut PhotocraftApp) -> bool {
    let was = active(app).is_some();
    app.floating = None;
    app.move_preview = None;
    was
}

/// Commands that leave a floating piece floating: view and window changes, and the commands the
/// floating selection runs itself.
fn keeps_floating(id: &str) -> bool {
    id.starts_with("view.") || id.starts_with("window.") || id.starts_with("help.") || matches!(id, "layer.select" | "edit.transform" | "layer.pickAt")
}

/// Before any command: ⌘Z / Undo puts the piece back (and is used up); everything else that
/// isn't view-only drops it first. Returns true when `id` was handled here (undo).
pub fn before_command(app: &mut PhotocraftApp, id: &str) -> bool {
    if active(app).is_none() {
        app.floating = None;
        return false;
    }
    match id {
        "edit.undo" | "edit.stepBackward" => cancel(app),
        _ if keeps_floating(id) => false,
        _ => {
            commit(app);
            false
        }
    }
}
