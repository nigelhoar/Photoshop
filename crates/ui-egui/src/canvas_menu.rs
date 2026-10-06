//! Canvas right-click menu over a selection (Photoshop): right-clicking inside the marching ants
//! with any tool that doesn't use the right button itself (painting tools open the Brush Preset
//! picker or erase, `paint_mouse`) lists the selection commands. Items go through
//! `menus::invoke`, so they open their dialogs and grey out exactly like their menu-bar twins.

use serde_json::json;

use crate::PhotocraftApp;
use crate::state::Tool;

/// One entry: label, command id. `None` = separator.
pub type Entry = Option<(&'static str, &'static str)>;

/// Photoshop 2026's selection context menu, in its order. Items this build doesn't implement
/// (Generative Fill, Delete and Fill Selection) are skipped when shown.
pub const ENTRIES: &[Entry] = &[
    Some(("Deselect", "select.deselect")),
    Some(("Select Inverse", "select.inverse")),
    Some(("Feather…", "select.modify.feather")),
    Some(("Select and Mask…", "select.selectAndMask")),
    None,
    Some(("Save Selection…", "select.saveSelection")),
    Some(("Make Work Path…", "select.toWorkPath")),
    None,
    Some(("Layer Via Copy", "layer.new.layerViaCopy")),
    Some(("Layer Via Cut", "layer.new.layerViaCut")),
    Some(("New Layer…", "layer.new.layer")),
    None,
    Some(("Free Transform", "edit.freeTransform")),
    Some(("Transform Selection", "select.transformSelection")),
    Some(("Distort", "edit.transform.distort")),
    Some(("Perspective", "edit.transform.perspective")),
    None,
    Some(("Fill…", "edit.fill")),
    Some(("Stroke…", "edit.stroke")),
    Some(("Content-Aware Fill…", "edit.contentAwareFill")),
    Some(("Generative Fill…", "edit.generativeFill")),
    Some(("Delete and Fill Selection", "edit.deleteAndFillSelection")),
    None,
    Some(("Last Filter", "filter.lastFilter")),
    None,
    Some(("Fade…", "edit.fade")),
];

/// Right-click while the transform box is up (Photoshop): switch what the handles do.
pub const TRANSFORM_ENTRIES: &[Entry] = &[
    Some(("Free Transform", "edit.freeTransform")),
    None,
    Some(("Scale", "edit.transform.scale")),
    Some(("Rotate", "edit.transform.rotate")),
    Some(("Skew", "edit.transform.skew")),
    Some(("Distort", "edit.transform.distort")),
    Some(("Perspective", "edit.transform.perspective")),
];

/// The transform menu's mode items, checked when they're the box's current mode.
fn mode_checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    let t = app.ui.transform.as_ref()?;
    matches!(id, "edit.freeTransform" | "edit.transform.skew" | "edit.transform.distort" | "edit.transform.perspective")
        .then(|| crate::state::TransformMode::for_command(id) == t.mode)
}

/// Is document point `p` inside the active document's selection?
pub fn inside_selection(app: &PhotocraftApp, p: [f64; 2]) -> bool {
    let Some(sel) = app.session.active().and_then(|st| st.doc.selection.as_ref()) else { return false };
    let mut v = [0.0f32];
    sel.read_pixel(p[0].floor() as i32, p[1].floor() as i32, &mut v);
    v[0] > 0.0
}

/// Does a right-click with `tool` at document point `p` open the selection menu?
pub fn opens(app: &PhotocraftApp, tool: Tool, p: [f64; 2]) -> bool {
    !crate::paint_mouse::has_brush_picker(tool) && !crate::paint_mouse::right_erases(app, tool) && app.ui.transform.is_none() && inside_selection(app, p)
}

/// Does a right-click open the transform menu (a Free Transform box is up, not warping)?
pub fn opens_transform(app: &PhotocraftApp) -> bool {
    app.ui.transform.as_ref().is_some_and(|t| t.warp.is_none())
}

/// Call every frame with the canvas response; `to_doc` maps a screen point to document pixels.
pub fn show(app: &mut PhotocraftApp, response: &egui::Response, tool: Tool, to_doc: impl Fn(egui::Pos2) -> [f64; 2]) {
    let open = if response.secondary_clicked() {
        Some(opens_transform(app) || response.interact_pointer_pos().is_some_and(|p| opens(app, tool, to_doc(p))))
    } else if response.clicked() {
        Some(false)
    } else {
        None
    };
    let mut clicked: Option<&'static str> = None;
    egui::Popup::menu(response).open_memory(open.map(egui::SetOpenCommand::Bool)).at_pointer_fixed().show(|ui| {
        ui.set_min_width(220.0);
        let mut last_sep = true;
        let entries = if opens_transform(app) { TRANSFORM_ENTRIES } else { ENTRIES };
        for &e in entries {
            match e {
                None => {
                    if !last_sep {
                        ui.separator();
                    }
                    last_sep = true;
                }
                Some((label, id)) => {
                    if !crate::menus::is_live(id) {
                        continue;
                    }
                    last_sep = false;
                    let button = egui::Button::selectable(mode_checked(app, id).unwrap_or(false), tl!(label));
                    if ui.add_enabled(crate::menus::is_enabled(app, id), button).clicked() {
                        clicked = Some(id);
                        ui.close();
                    }
                }
            }
        }
    });
    if let Some(id) = clicked
        && let Err(e) = crate::menus::invoke(app, &response.ctx, id, json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
        app.sync_views();
        app
    }

    #[test]
    fn opens_inside_the_selection_with_non_painting_tools() {
        let mut app = app();
        assert!(!opens(&app, Tool::RectMarquee, [20.0, 20.0]), "no selection");
        app.session.execute("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
        assert!(opens(&app, Tool::RectMarquee, [20.0, 20.0]));
        assert!(opens(&app, Tool::Move, [20.0, 20.0]));
        assert!(!opens(&app, Tool::RectMarquee, [50.0, 50.0]), "outside the ants");
        assert!(!opens(&app, Tool::Brush, [20.0, 20.0]), "the Brush keeps its preset picker");
    }

    #[test]
    fn distort_from_the_selection_menu_moves_one_corner_freely() {
        let mut app = app();
        app.session.execute("layer.new.layer", json!({})).unwrap();
        app.session.execute("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
        app.session.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        // Snapping would pull the dragged corner onto nearby edges.
        app.ui.extras.snap = false;
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, "edit.transform.distort", json!({})).unwrap();
        let q0 = app.ui.transform.as_ref().unwrap().quad;
        assert!(opens_transform(&app));
        assert_eq!(mode_checked(&app, "edit.transform.distort"), Some(true));
        let m = egui::Modifiers::NONE;
        crate::canvas::tool_event(&mut app, crate::canvas::ToolEvent::Down { x: q0[1][0], y: q0[1][1], pressure: 1.0 }, m);
        crate::canvas::tool_event(&mut app, crate::canvas::ToolEvent::Up { x: q0[1][0] + 7.0, y: q0[1][1] - 5.0 }, m);
        let q = app.ui.transform.as_ref().unwrap().quad;
        assert_eq!(q[1], [q0[1][0] + 7.0, q0[1][1] - 5.0], "only the dragged corner moved");
        assert_eq!([q[0], q[2], q[3]], [q0[0], q0[2], q0[3]]);
        // Free Transform from the menu switches the live box back.
        crate::menus::invoke(&mut app, &ctx, "edit.freeTransform", json!({})).unwrap();
        assert_eq!(app.ui.transform.as_ref().unwrap().mode, crate::state::TransformMode::Free);
    }

    #[test]
    fn every_live_entry_is_a_known_command() {
        let live: Vec<&str> = ENTRIES.iter().flatten().map(|e| e.1).filter(|id| crate::menus::is_live(id)).collect();
        for id in ["select.deselect", "select.inverse", "select.transformSelection", "edit.freeTransform", "layer.new.layerViaCopy"] {
            assert!(live.contains(&id), "{id} should be in the menu");
        }
    }
}
