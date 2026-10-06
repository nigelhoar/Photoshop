//! Polygonal Lasso through the real canvas: a double-click closes the polygon, as in Photoshop.

use egui::{Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use photocraft_geom::Rect;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::Tool;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = Tool::PolygonLasso;
    // 60 fps, so a double-click's two clicks fall inside egui's 0.3 s window.
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).with_step_dt(1.0 / 60.0).build_ui_state(
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
    // 100 %: one document pixel per point.
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

fn button(h: &mut Harness<'static, PhotocraftApp>, p: Pos2, down: bool, m: Modifiers) {
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: m });
    h.run_steps(1);
}

/// A click at document point `(x, y)`, the pointer still between press and release.
fn click(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32, m: Modifiers) {
    let p = screen(h, x, y);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    button(h, p, true, m);
    button(h, p, false, m);
}

fn double_click(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32) {
    click(h, x, y, Modifiers::NONE);
    click(h, x, y, Modifiers::NONE);
    h.run_steps(1);
}

fn selection(h: &Harness<'static, PhotocraftApp>) -> Option<Rect> {
    h.state().session.active().unwrap().doc.selection.as_ref().map(|s| s.content_bounds())
}

fn near(r: Rect, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    r.x0.abs_diff(x0) <= 1 && r.y0.abs_diff(y0) <= 1 && r.x1.abs_diff(x1) <= 1 && r.y1.abs_diff(y1) <= 1
}

#[test]
fn double_click_closes_the_polygon_at_the_last_point() {
    let mut h = harness();
    click(&mut h, 50.0, 50.0, Modifiers::NONE);
    h.run_steps(30); // Well past egui's double-click delay: separate clicks.
    click(&mut h, 150.0, 50.0, Modifiers::NONE);
    h.run_steps(30);
    double_click(&mut h, 150.0, 150.0);
    assert!(h.state().ui.polygon.is_empty(), "the polygon closed: {:?}", h.state().ui.polygon);
    let r = selection(&h).expect("a selection");
    assert!(near(r, 50, 50, 150, 150), "the triangle's bounds: {r:?}");
    // Only one vertex was added at the double-click, not a duplicate starting a new polygon.
    h.run_steps(30);
    assert!(h.state().ui.polygon.is_empty());
}

/// Vertices placed quickly: egui counts the double-click's second click as a triple click (the
/// vertex before it was under 0.6 s earlier). It still closes the polygon.
#[test]
fn double_click_closes_the_polygon_after_quickly_placed_vertices() {
    let mut h = harness();
    click(&mut h, 50.0, 50.0, Modifiers::NONE);
    h.run_steps(12);
    click(&mut h, 150.0, 50.0, Modifiers::NONE);
    h.run_steps(12);
    double_click(&mut h, 150.0, 150.0);
    assert!(h.state().ui.polygon.is_empty(), "{:?}", h.state().ui.polygon);
    assert!(selection(&h).is_some_and(|r| near(r, 50, 50, 150, 150)), "{:?}", selection(&h));
}

/// Double-clicking on the first vertex: the first click closes the polygon, and the second must
/// not start a new one (which would deselect what was just made).
#[test]
fn double_click_on_the_first_vertex_keeps_the_new_selection() {
    let mut h = harness();
    for (x, y) in [(50.0, 50.0), (150.0, 50.0), (150.0, 150.0)] {
        click(&mut h, x, y, Modifiers::NONE);
        h.run_steps(30);
    }
    double_click(&mut h, 50.0, 50.0);
    assert!(h.state().ui.polygon.is_empty());
    assert!(selection(&h).is_some_and(|r| near(r, 50, 50, 150, 150)), "{:?}", selection(&h));
}
