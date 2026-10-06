//! Marquee modifiers through the real canvas (#208): ⇧ during the drag draws a square or circle,
//! ⌥ draws from the centre (both together work too), a "W × H px" readout follows the cursor, and
//! modifiers held before the drag still pick add/subtract (#188) instead of constraining.

use egui::{Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_geom::Rect;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::Tool;

fn harness(tool: Tool) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = tool;
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    // 100 %: one document pixel per point, so drags land on whole pixels.
    let v = &mut h.state_mut().ui.views[0];
    v.zoom = 1.0;
    v.center = [200.0, 150.0];
    v.fit_pending = false;
    h.run_steps(2);
    h
}

fn screen(h: &Harness<'static, PhotocraftApp>, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    let xf = ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: app.ui.view.flip_horizontal };
    xf.to_screen(x, y)
}

fn mods(h: &mut Harness<'static, PhotocraftApp>, m: Modifiers) {
    h.event(egui::Event::ModifiersChanged(m));
    h.run_steps(1);
}

fn button(h: &mut Harness<'static, PhotocraftApp>, p: Pos2, down: bool, m: Modifiers) {
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: m });
    h.run_steps(1);
}

/// Move the pointer to document point `(x, y)` in a few steps.
fn move_to(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32) {
    let p = screen(h, x, y);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(2);
}

fn press_at(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32, m: Modifiers) {
    let p = screen(h, x, y);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    button(h, p, true, m);
    // Past egui's click distance, so the drag starts at the press point.
    move_to(h, x + 8.0, y + 6.0);
}

fn release_at(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32, m: Modifiers) {
    move_to(h, x, y);
    button(h, screen(h, x, y), false, m);
    h.run_steps(2);
}

fn selection(h: &Harness<'static, PhotocraftApp>) -> Rect {
    h.state().session.active().unwrap().doc.selection.as_ref().map_or(Rect::EMPTY, |s| s.content_bounds())
}

/// The two-row readout shows `W: {w} px` and `H: {ht} px` (Photoshop's format).
fn readout(h: &Harness<'static, PhotocraftApp>, w: i32, ht: i32) -> bool {
    let labels = h.query_by_label("W:").is_some() && h.query_by_label("H:").is_some();
    let count = |v: i32| h.query_all_by_label(&format!("{v} px")).count();
    let values = if w == ht { count(w) == 2 } else { count(w) == 1 && count(ht) == 1 };
    labels && values && h.query_all_by_label_contains(" px").count() == 2
}

#[test]
fn shift_during_the_drag_draws_a_square_and_the_readout_follows() {
    let mut h = harness(Tool::RectMarquee);
    press_at(&mut h, 50.0, 50.0, Modifiers::NONE);
    move_to(&mut h, 150.0, 90.0);
    assert!(readout(&h, 100, 40), "free drag: 100 × 40");
    mods(&mut h, Modifiers::SHIFT);
    move_to(&mut h, 150.0, 91.0);
    assert!(readout(&h, 100, 100), "⇧: the larger extent wins");
    release_at(&mut h, 150.0, 91.0, Modifiers::SHIFT);
    mods(&mut h, Modifiers::NONE);
    assert_eq!(selection(&h), Rect::new(50, 50, 150, 150));
    assert!(h.query_by_label("W:").is_none() && h.query_by_label_contains(" px").is_none(), "the readout goes away with the drag");
}

#[test]
fn alt_draws_from_the_centre_and_shift_alt_a_centred_circle() {
    let mut h = harness(Tool::RectMarquee);
    press_at(&mut h, 200.0, 150.0, Modifiers::NONE);
    mods(&mut h, Modifiers::ALT);
    move_to(&mut h, 240.0, 170.0);
    assert!(readout(&h, 80, 40));
    release_at(&mut h, 240.0, 170.0, Modifiers::ALT);
    mods(&mut h, Modifiers::NONE);
    assert_eq!(selection(&h), Rect::new(160, 130, 240, 170), "centred on the press point");
    // Elliptical, ⇧⌥: a circle centred on the press point (replacing the selection).
    let mut h = harness(Tool::EllipseMarquee);
    press_at(&mut h, 200.0, 150.0, Modifiers::NONE);
    mods(&mut h, Modifiers::SHIFT | Modifiers::ALT);
    move_to(&mut h, 230.0, 160.0);
    assert!(readout(&h, 60, 60));
    release_at(&mut h, 230.0, 160.0, Modifiers::SHIFT | Modifiers::ALT);
    mods(&mut h, Modifiers::NONE);
    let r = selection(&h);
    assert!(r.x0.abs_diff(170) <= 1 && r.x1.abs_diff(230) <= 1 && r.y0.abs_diff(120) <= 1 && r.y1.abs_diff(180) <= 1, "{r:?}");
}

#[test]
fn modifiers_held_before_the_drag_pick_the_mode_not_the_shape() {
    // #188: ⇧ held at the press adds (no square); released and pressed again it constrains.
    let mut h = harness(Tool::RectMarquee);
    h.state_mut().run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    mods(&mut h, Modifiers::SHIFT);
    press_at(&mut h, 100.0, 100.0, Modifiers::SHIFT);
    move_to(&mut h, 180.0, 130.0);
    assert!(readout(&h, 80, 30), "not squared by the add-mode ⇧");
    release_at(&mut h, 180.0, 130.0, Modifiers::SHIFT);
    mods(&mut h, Modifiers::NONE);
    assert_eq!(selection(&h), Rect::new(10, 10, 180, 130), "added to the first selection");
    let st = h.state().session.active().unwrap();
    let sel = st.doc.selection.as_ref().unwrap();
    assert!(sel.sample_channel(20, 20, 0) > 0.99 && sel.sample_channel(150, 120, 0) > 0.99 && sel.sample_channel(60, 60, 0) < 0.01);
    // ⇧ at the press, released, pressed again: add mode and a square.
    let mut h = harness(Tool::RectMarquee);
    h.state_mut().run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    mods(&mut h, Modifiers::SHIFT);
    press_at(&mut h, 100.0, 100.0, Modifiers::SHIFT);
    mods(&mut h, Modifiers::NONE);
    move_to(&mut h, 180.0, 130.0);
    mods(&mut h, Modifiers::SHIFT);
    move_to(&mut h, 180.0, 131.0);
    assert!(readout(&h, 80, 80));
    release_at(&mut h, 180.0, 131.0, Modifiers::SHIFT);
    mods(&mut h, Modifiers::NONE);
    let st = h.state().session.active().unwrap();
    let sel = st.doc.selection.as_ref().unwrap();
    assert!(sel.sample_channel(20, 20, 0) > 0.99, "still added");
    assert!(sel.sample_channel(170, 170, 0) > 0.99 && sel.sample_channel(170, 185, 0) < 0.01, "an 80 px square");
    // ⌥ held at the press subtracts.
    let mut h = harness(Tool::RectMarquee);
    h.state_mut().run("select.rect", json!({"x": 0, "y": 0, "width": 400, "height": 300})).unwrap();
    mods(&mut h, Modifiers::ALT);
    press_at(&mut h, 100.0, 100.0, Modifiers::ALT);
    release_at(&mut h, 150.0, 120.0, Modifiers::ALT);
    mods(&mut h, Modifiers::NONE);
    let st = h.state().session.active().unwrap();
    let sel = st.doc.selection.as_ref().unwrap();
    assert!(sel.sample_channel(120, 110, 0) < 0.01 && sel.sample_channel(90, 110, 0) > 0.99, "subtracted, not centred");
}

/// `cargo test --release -p photocraft-ui-egui marquee_drag_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn marquee_drag_bench() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 6000, "height": 4000})).unwrap();
    app.run("filter.render.clouds", json!({})).unwrap();
    app.sync_views();
    app.ui.tool = Tool::RectMarquee;
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(6);
    press_at(&mut h, 1000.0, 1000.0, Modifiers::NONE);
    mods(&mut h, Modifiers::SHIFT | Modifiers::ALT);
    let n = 60;
    let t0 = std::time::Instant::now();
    for i in 0..n {
        let p = screen(&h, 1500.0 + i as f32 * 20.0, 1300.0 + i as f32 * 7.0);
        h.event(egui::Event::PointerMoved(p));
        h.step();
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3 / f64::from(n);
    eprintln!("marquee drag on 6000x4000 (shift+alt, readout): {ms:.2} ms per frame");
    assert!(h.query_by_label("W:").is_some());
}

/// Dragging inside the selection with a marquee moves it (Photoshop): plain drag = the outline,
/// ⌘-drag = cut the selected pixels and move them; ⇧ still adds and a click deselects.
#[test]
fn drag_inside_the_selection_moves_or_cuts_it() {
    use crate::canvas::{ToolEvent, tool_event};
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 80, "height": 60, "background": "transparent"})).unwrap();
    app.sync_views();
    app.ui.extras.snap = false;
    app.ui.tool = Tool::RectMarquee;
    app.session
        .edit("paint", |doc, a| {
            doc.layer_mut(a.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    let sel = |app: &PhotocraftApp| app.session.active().unwrap().doc.selection.as_ref().map(|s| s.content_bounds());
    let alpha = |app: &PhotocraftApp, x: i32, y: i32| {
        let st = app.session.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().rgba(x, y)[3]
    };
    let drag = |app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], m: Modifiers| {
        tool_event(app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, m);
        tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, m);
        tool_event(app, ToolEvent::Up { x: to[0], y: to[1] }, m);
    };
    app.run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    // A plain drag inside moves the outline only.
    drag(&mut app, [20.0, 20.0], [30.0, 25.0], Modifiers::NONE);
    assert_eq!(sel(&app), Some(Rect::new(20, 15, 40, 35)));
    assert_eq!(alpha(&app, 12, 12), 1.0, "pixels stay put");
    // ⌘-drag cuts the selected pixels: they float 5 px right (the document waits for the drop).
    drag(&mut app, [30.0, 25.0], [35.0, 25.0], Modifiers::COMMAND);
    assert_eq!(crate::floating::active(&app).map(|f| f.offset), Some((5, 0)));
    assert_eq!(alpha(&app, 22, 20), 1.0, "not dropped yet");
    // ⇧-drag draws (and adds): the floating piece drops first, with its outline.
    drag(&mut app, [30.0, 20.0], [60.0, 50.0], Modifiers::SHIFT);
    assert!(crate::floating::active(&app).is_none());
    assert_eq!(alpha(&app, 22, 20), 0.0, "cut from where it was");
    assert_eq!(alpha(&app, 32, 20), 1.0, "and dropped 5 px right");
    assert_eq!(alpha(&app, 12, 12), 1.0, "unselected pixels stay");
    assert_eq!(sel(&app), Some(Rect::new(25, 15, 60, 50)));
    // A click inside (no move) deselects, like a marquee click.
    drag(&mut app, [30.0, 20.0], [30.0, 20.0], Modifiers::NONE);
    assert_eq!(sel(&app), None);
}

/// The same through the real canvas (mouse events, snapping on): a drag inside the ants moves
/// the selection.
#[test]
fn mouse_drag_inside_the_selection_moves_it() {
    let mut h = harness(Tool::RectMarquee);
    press_at(&mut h, 100.0, 80.0, Modifiers::NONE);
    release_at(&mut h, 200.0, 160.0, Modifiers::NONE);
    let first = selection(&h);
    assert_eq!(first, Rect::new(100, 80, 200, 160), "drawn");
    press_at(&mut h, 150.0, 120.0, Modifiers::NONE);
    release_at(&mut h, 170.0, 130.0, Modifiers::NONE);
    assert_eq!(selection(&h), Rect::new(120, 90, 220, 170), "moved by (20, 10)");
}

/// ⌘-drag floats the cut piece (Photoshop): it shows at the pointer while dragging, plain drags
/// move it again without cutting anything new, and only deselecting drops it into the layer.
#[test]
fn cmd_drag_floats_the_cut_piece_until_deselected() {
    use crate::canvas::{ToolEvent, tool_event};
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 80, "height": 60, "background": "transparent"})).unwrap();
    app.sync_views();
    app.ui.extras.snap = false;
    app.ui.tool = Tool::RectMarquee;
    app.session
        .edit("paint", |doc, a| {
            doc.layer_mut(a.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    app.run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let alpha = |d: &photocraft_doc::Document, x, y| d.layer(layer).unwrap().surface().unwrap().rgba(x, y)[3];
    let doc = |app: &PhotocraftApp| app.session.active().unwrap().doc.clone();
    let drag = |app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], m: Modifiers| {
        tool_event(app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, m);
        tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, m);
    };
    let untouched = app.session.active().unwrap().history.past_len();
    // ⌘-drag: the piece is at the pointer while dragging.
    drag(&mut app, [20.0, 20.0], [35.0, 20.0], Modifiers::COMMAND);
    let (shown, _) = crate::move_ui::display_doc(&mut app, 0).expect("a live preview while dragging");
    assert!(alpha(&shown, 12, 20) == 0.0 && alpha(&shown, 40, 20) == 1.0, "cut and at the pointer");
    tool_event(&mut app, ToolEvent::Up { x: 35.0, y: 20.0 }, Modifiers::COMMAND);
    assert_eq!(alpha(&doc(&app), 12, 20), 1.0, "the document waits for the drop");
    // A plain drag on the floating piece moves it again (no ⌘, no new cut).
    drag(&mut app, [30.0, 20.0], [30.0, 30.0], Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Up { x: 30.0, y: 30.0 }, Modifiers::NONE);
    assert_eq!(crate::floating::active(&app).map(|f| f.offset), Some((15, 10)));
    let (shown, _) = crate::move_ui::display_doc(&mut app, 0).unwrap();
    assert!(alpha(&shown, 27, 22) == 1.0 && alpha(&shown, 12, 20) == 0.0);
    assert_eq!(app.session.active().unwrap().history.past_len(), untouched, "nothing committed yet");
    // ⌘Z puts it back; nothing was ever cut.
    crate::menus::invoke(&mut app, &egui::Context::default(), "edit.undo", json!({})).unwrap();
    assert!(crate::floating::active(&app).is_none() && alpha(&doc(&app), 12, 20) == 1.0);
    assert_eq!(app.session.active().unwrap().history.past_len(), untouched);
    // Float it again, then deselect: dropped into the layer in one step, selection gone.
    drag(&mut app, [20.0, 20.0], [35.0, 30.0], Modifiers::COMMAND);
    tool_event(&mut app, ToolEvent::Up { x: 35.0, y: 30.0 }, Modifiers::COMMAND);
    crate::menus::invoke(&mut app, &egui::Context::default(), "select.deselect", json!({})).unwrap();
    let d = doc(&app);
    assert!(d.selection.is_none());
    assert!(alpha(&d, 12, 12) == 0.0 && alpha(&d, 26, 21) == 1.0 && alpha(&d, 44, 39) == 1.0, "dropped 15, 10 from where it was cut");
}
