//! Help ▸ About PhotoCraft: the About, Contributors and Models tabs. A thin view over
//! [`crate::credits`] (which aggregates and sorts); all state lives in the dialog's fields, so the
//! control channel can drive it (`help.about {tab?}`, then `ui.dialog.set`):
//!
//! - `tab`: `about` (default) | `contributors` | `models`
//! - `names`: `login` (default) | `display` | `real`: falls back to `@login` without consent
//! - `sort` / `desc`: Contributors column (`name`, `first` (default), `last`, `prs`, `commits`,
//!   `added`, `removed`, `firstMerge`, `lastMerge`) and direction
//! - `modelSort` / `modelDesc`: Models column (`company`, `model`, `version`, `commits` (default,
//!   descending), `shareAssisted`, `shareAll`, `prs`, `added`, `removed`)
//! - `selected`: the login whose PR and commit links are listed under the table

use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::credits::{self, ContributorCol, Credits, ModelCol, NameMode};
use crate::theme::Tokens;

pub const TABS: [(&str, &str); 3] = [("about", "About"), ("contributors", "Contributors"), ("models", "Models")];

/// The About dialog's width (the credit tables need room; narrower windows scroll them); `None`
/// for System Info.
pub fn width(fields: &Map<String, Value>) -> Option<f32> {
    (fields.get("systemInfo").and_then(Value::as_bool) != Some(true)).then_some(980.0)
}

fn str_field<'a>(fields: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    fields.get(key).and_then(Value::as_str)
}

pub fn tab(fields: &Map<String, Value>) -> &str {
    str_field(fields, "tab").filter(|t| TABS.iter().any(|(k, _)| k == t)).unwrap_or("about")
}

pub fn names(fields: &Map<String, Value>) -> NameMode {
    str_field(fields, "names").and_then(NameMode::from_key).unwrap_or_default()
}

pub fn contributor_sort(fields: &Map<String, Value>) -> (ContributorCol, bool) {
    let col = str_field(fields, "sort").and_then(ContributorCol::from_key).unwrap_or(ContributorCol::First);
    (col, fields.get("desc").and_then(Value::as_bool).unwrap_or(false))
}

pub fn model_sort(fields: &Map<String, Value>) -> (ModelCol, bool) {
    match str_field(fields, "modelSort").and_then(ModelCol::from_key) {
        Some(col) => (col, fields.get("modelDesc").and_then(Value::as_bool).unwrap_or(false)),
        None => (ModelCol::Commits, fields.get("modelDesc").and_then(Value::as_bool).unwrap_or(true)),
    }
}

/// A header click: the same column flips direction, another column sorts ascending.
fn click_sort(fields: &mut Map<String, Value>, sort_key: &str, desc_key: &str, col: &str, was: (&str, bool)) {
    let desc = if was.0 == col { !was.1 } else { false };
    fields.insert(sort_key.into(), json!(col));
    fields.insert(desc_key.into(), json!(desc));
}

/// The dialog body (below the title).
pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, fields: &mut Map<String, Value>) {
    let current = tab(fields).to_string();
    ui.horizontal(|ui| {
        for (key, label) in TABS {
            if crate::widgets::pill_tab(ui, label, current == key).clicked() {
                fields.insert("tab".into(), json!(key));
            }
        }
    });
    ui.add_space(8.0);
    match current.as_str() {
        "contributors" => contributors_tab(app, ui, fields),
        "models" => models_tab(ui, fields),
        _ => about_tab(app, ui),
    }
}

fn about_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.label(tl!("PhotoCraft — an open-source, native image editor written in Rust."));
    ui.label(crate::i18n::fmt(tl!("Version {version}"), &[("version", &photocraft_engine::build_info::long_version())]));
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        crate::links::discord_button(app, ui, 220.0);
        ui.add_space(8.0);
        crate::links::link_row(app, ui);
    });
    ui.add_space(10.0);
    ui.weak("egui · wgpu · photocraft-engine");
}

/// A clickable column header with a sort arrow when it is the sort column.
fn header(ui: &mut egui::Ui, label: &str, sorted: Option<bool>) -> bool {
    let t = Tokens::get(ui.ctx());
    let text = egui::RichText::new(tl!(label)).font(crate::theme::medium(12.0)).color(if sorted.is_some() { t.text } else { t.text_dim });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()).selectable(false)).on_hover_cursor(egui::CursorIcon::PointingHand);
        // Room for the arrow in every header, so sorting doesn't shift the columns.
        let (a, _) = ui.allocate_exact_size(egui::vec2(8.0, r.rect.height()), egui::Sense::hover());
        if let Some(desc) = sorted {
            let c = a.center();
            let (tip, base) = if desc { (3.0, -3.0) } else { (-3.0, 3.0) };
            ui.painter().add(egui::Shape::convex_polygon(
                vec![egui::pos2(c.x, c.y + tip), egui::pos2(c.x - 3.5, c.y + base), egui::pos2(c.x + 3.5, c.y + base)],
                t.text,
                egui::Stroke::NONE,
            ));
        }
        r.clicked()
    })
    .inner
}

fn num(ui: &mut egui::Ui, v: impl std::fmt::Display) {
    ui.label(egui::RichText::new(v.to_string()).font(crate::theme::mono(12.0)));
}

fn contributors_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui, fields: &mut Map<String, Value>) {
    let data = Credits::compiled();
    let mode = names(fields);
    let (col, desc) = contributor_sort(fields);
    let mut rows = credits::contributors(&data);
    credits::sort_contributors(&mut rows, col, desc, mode);
    ui.horizontal(|ui| {
        ui.label(tl!("Show"));
        for m in NameMode::ALL {
            if crate::widgets::pill_tab(ui, m.label(), m == mode).clicked() {
                fields.insert("names".into(), json!(m.key()));
            }
        }
    });
    ui.add_space(6.0);
    if rows.is_empty() {
        ui.weak(tl!("No contributor data in this build."));
        return;
    }
    let selected = str_field(fields, "selected").map(str::to_string);
    let mut clicked_col = None;
    let mut clicked_row = None;
    egui::ScrollArea::both().id_salt("about-contributors").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("about-contributors-grid").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for c in ContributorCol::ALL {
                if header(ui, c.label(), (c == col).then_some(desc)) {
                    clicked_col = Some(c);
                }
            }
            ui.end_row();
            for r in &rows {
                let on = selected.as_deref() == Some(r.login);
                if ui.selectable_label(on, r.name(mode)).on_hover_text(format!("@{}", r.login)).clicked() {
                    clicked_row = Some(r.login.to_string());
                }
                num(ui, credits::day(r.first));
                num(ui, credits::day(r.last));
                num(ui, r.prs);
                num(ui, r.commits);
                num(ui, r.additions);
                num(ui, r.deletions);
                num(ui, r.first_merge.map_or("", credits::day));
                num(ui, r.last_merge.map_or("", credits::day));
                ui.end_row();
            }
        });
    });
    if let Some(c) = clicked_col {
        click_sort(fields, "sort", "desc", c.key(), (col.key(), desc));
    }
    if let Some(login) = clicked_row {
        fields.insert("selected".into(), json!(login));
    }
    let Some(login) = str_field(fields, "selected").map(str::to_string) else {
        ui.add_space(6.0);
        ui.weak(tl!("Select a contributor to list their pull requests and commits."));
        return;
    };
    let (prs, commits) = credits::links_for(&data, &login);
    ui.add_space(8.0);
    crate::widgets::hairline(ui);
    ui.add_space(6.0);
    let t = Tokens::get(ui.ctx());
    let mut open: Option<&str> = None;
    egui::ScrollArea::vertical().id_salt("about-links").max_height(130.0).auto_shrink([false, true]).show(ui, |ui| {
        crate::widgets::section_label(ui, "Pull requests");
        ui.horizontal_wrapped(|ui| {
            if prs.is_empty() {
                ui.weak("—");
            }
            for p in &prs {
                if ui.link(egui::RichText::new(format!("#{}", p.number)).color(t.accent)).on_hover_text(p.url).clicked() {
                    open = Some(p.url);
                }
            }
        });
        ui.add_space(4.0);
        crate::widgets::section_label(ui, "Commits");
        ui.horizontal_wrapped(|ui| {
            if commits.is_empty() {
                ui.weak("—");
            }
            for k in &commits {
                let short = k.sha.get(..7).unwrap_or(k.sha);
                if ui.link(egui::RichText::new(short).font(crate::theme::mono(12.0)).color(t.accent)).on_hover_text(k.url).clicked() {
                    open = Some(k.url);
                }
            }
        });
    });
    if let Some(url) = open {
        crate::links::open(app, ui.ctx(), url);
    }
}

fn models_tab(ui: &mut egui::Ui, fields: &mut Map<String, Value>) {
    let data = Credits::compiled();
    let (col, desc) = model_sort(fields);
    let mut rows = credits::models(&data);
    credits::sort_models(&mut rows, col, desc);
    if rows.is_empty() {
        ui.weak(tl!("No model data in this build."));
        return;
    }
    let mut clicked_col = None;
    egui::ScrollArea::both().id_salt("about-models").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("about-models-grid").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for c in ModelCol::ALL {
                if header(ui, c.label(), (c == col).then_some(desc)) {
                    clicked_col = Some(c);
                }
            }
            ui.end_row();
            for r in &rows {
                ui.label(r.company);
                ui.label(r.model).on_hover_text(r.id);
                ui.label(r.version);
                num(ui, r.commits);
                num(ui, format!("{:.1}%", r.share_assisted));
                num(ui, format!("{:.1}%", r.share_all));
                num(ui, r.prs);
                num(ui, r.additions);
                num(ui, r.deletions);
                ui.end_row();
            }
        });
    });
    ui.add_space(6.0);
    ui.weak(tl!("From the Co-Authored-By trailers of commits on the main branch."));
    if let Some(c) = clicked_col {
        click_sort(fields, "modelSort", "modelDesc", c.key(), (col.key(), desc));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DialogKind;
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(tab: &str) -> Harness<'static, PhotocraftApp> {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(|ui, app| crate::dialogs::show(app, ui.ctx()), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        let ctx = h.ctx.clone();
        crate::menus::invoke(h.state_mut(), &ctx, "help.about", json!({"tab": tab})).expect("help.about opens the dialog");
        h.run_steps(3);
        h
    }

    fn fields(h: &Harness<'static, PhotocraftApp>) -> Map<String, Value> {
        h.state().ui.dialogs.first().map(|d| d.fields.clone()).expect("the About dialog is open")
    }

    fn click(h: &mut Harness<'static, PhotocraftApp>, label: &str) {
        h.get_by_label(label).click();
        h.run_steps(3);
    }

    #[test]
    fn about_opens_on_the_contributors_tab_and_sorts_by_header_clicks() {
        let mut h = harness("contributors");
        assert_eq!(h.state().ui.dialogs[0].kind, DialogKind::About);
        // Default sort: first contribution, oldest first, so the founder leads.
        let _ = h.get_by_label("@echelon");
        assert_eq!(contributor_sort(&fields(&h)), (ContributorCol::First, false));
        click(&mut h, "Commits");
        assert_eq!(contributor_sort(&fields(&h)), (ContributorCol::Commits, false));
        click(&mut h, "Commits");
        assert_eq!(contributor_sort(&fields(&h)), (ContributorCol::Commits, true));
        // Real names only with consent: the owner's shows, everyone else stays @login.
        click(&mut h, "Real name");
        assert_eq!(names(&fields(&h)), NameMode::Real);
        let _ = h.get_by_label("Brandon Thomas");
        // Selecting a contributor lists their links.
        click(&mut h, "Brandon Thomas");
        assert_eq!(str_field(&fields(&h), "selected"), Some("echelon"));
        let _ = h.get_by_label("Pull requests");
    }

    #[test]
    fn models_tab_lists_models_and_sorts() {
        let mut h = harness("models");
        assert!(h.get_all_by_label("Anthropic").count() >= 2, "Claude Opus and Sonnet rows");
        assert_eq!(model_sort(&fields(&h)), (ModelCol::Commits, true));
        click(&mut h, "Company");
        assert_eq!(model_sort(&fields(&h)), (ModelCol::Company, false));
        click(&mut h, "About");
        assert_eq!(tab(&fields(&h)), "about");
    }

    #[test]
    fn field_parsing_falls_back_on_bad_values() {
        let mut f = Map::new();
        f.insert("tab".into(), json!("nope"));
        f.insert("sort".into(), json!(42));
        f.insert("names".into(), json!("real"));
        assert_eq!(tab(&f), "about");
        assert_eq!(contributor_sort(&f), (ContributorCol::First, false));
        assert_eq!(names(&f), NameMode::Real);
        f.insert("systemInfo".into(), json!(true));
        assert_eq!(width(&f), None);
    }
}
