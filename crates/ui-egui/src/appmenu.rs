//! Linux global menu: exports the in-window menu bar over the Canonical AppMenu (dbusmenu)
//! protocol, so global-menu environments host PhotoCraft's menus natively. One exporter per
//! app, started lazily on the first frame; a 1 Hz publish cadence keeps the global menu in
//! step with document state (checked states, enablement, recent files) nearly as fast as
//! opening the menu. Activations come back as
//! [`photocraft_appmenu::MenuEvent`]s and dispatch exactly like an in-window menu click
//! ([`crate::menus::invoke`]), so an item behaves identically in both hosts.
//!
//! Nothing runs outside Linux/X11/BSD with a D-Bus session bus:
//! [`photocraft_appmenu::AppMenu::start`] fails gracefully there and every call below
//! becomes a no-op (the web build has no menus at all). `PHOTOCRAFT_NO_GLOBAL_MENU=1`
//! skips the export entirely (debugging).

use crate::menus::{MenuItem, TOP_MENUS};
use photocraft_appmenu::{MenuEntry, MenuModel};
use serde_json::json;
use std::time::{Duration, Instant};

/// Publish cadence: the export reflects document state, so it is refreshed like a background
/// poller — cheap next to opening a real menu, since the exporter diffs models on its own
/// thread and only emits what changed.
const REFRESH: Duration = Duration::from_secs(1);

/// The app's name on the bus (machine names are lowercase; AGENTS.md).
const APP_NAME: &str = "photocraft";

/// Starts (once), publishes (on a duty cycle) and drains menu clicks. Called from the frame
/// loop, right after the control channel is drained.
pub fn tick(app: &mut crate::PhotocraftApp, ctx: &egui::Context) {
    // The flag follows the registrar's registration state (cheap atomic) while an exporter
    // runs: while a host serves the menus, the in-window menu bar is hidden; it comes back at
    // the same second the host (or its service) disappears. With no exporter the field is
    // never touched (it is only ever flipped true here), so the in-window bar stays.
    if let Some(menu) = &app.global_menu {
        app.global_menu_hosted = menu.hosted();
    }
    if app.global_menu.is_none() && !app.global_menu_unavailable {
        if std::env::var_os("PHOTOCRAFT_NO_GLOBAL_MENU").is_some() {
            app.global_menu_unavailable = true;
            log::debug!("AppMenu: disabled by PHOTOCRAFT_NO_GLOBAL_MENU");
        } else {
            match photocraft_appmenu::AppMenu::start(APP_NAME) {
                Ok(menu) => {
                    log::debug!("AppMenu: global menu exporting");
                    app.global_menu = Some(menu);
                }
                Err(e) => {
                    app.global_menu_unavailable = true;
                    log::debug!("AppMenu: global menu stays off ({e})");
                }
            }
        }
    }
    // Publish at most once per cycle; the very first publish happens right away.
    if app.global_menu_last.is_none_or(|last| last.elapsed() >= REFRESH) {
        let items = crate::menus::menu_items(app);
        if let Some(menu) = &app.global_menu {
            menu.replace(&model(&items));
        }
        app.global_menu_last = Some(Instant::now());
    }
    // The activations come in a batch (the exporter queues while the frame runs); collect one
    // before dispatching, because dispatch takes the app mutably.
    let events: Vec<photocraft_appmenu::MenuEvent> = match app.global_menu.as_ref() {
        Some(menu) => std::iter::from_fn(|| menu.try_event()).collect(),
        None => return,
    };
    for ev in events {
        let photocraft_appmenu::MenuEvent::Activated { action, .. } = ev;
        // The behaviour path is the one an in-window click takes, so a global-menu
        // activation is identical to the same item in the in-window menu bar.
        if let Err(e) = crate::menus::invoke(app, ctx, &action, json!({})) {
            log::debug!("AppMenu: activating {action} failed: {e}");
        }
    }
}

/// Builds the global-menu model from the same item list the in-window menu bar renders, so
/// both menus present the same structure, order, labels (translated), shortcuts, enablement
/// and check marks.
pub fn model(items: &[MenuItem]) -> MenuModel {
    let lang = crate::i18n::current();
    let tops: Vec<MenuEntry> = TOP_MENUS
        .iter()
        .filter_map(|&top| {
            let mine: Vec<&MenuItem> =
                items.iter().filter(|i| i.path.first().is_some_and(|p| p == top)).collect();
            if mine.is_empty() {
                return None; // the in-window bar shows "(coming soon)" instead
            }
            Some(
                MenuEntry::command(crate::i18n::tr(lang, top), "", true)
                    .submenu(entries(&mine, 1)),
            )
        })
        .collect();
    MenuModel::top(tops)
}

/// One nesting level, mirroring `menus::render_level_rows`: leaves sit at `path.len() ==
/// depth`, a submenu appears at the position of its first child, `"---"` means a separator
/// (never doubled, never leading or trailing), and submenu titles use the same enable rule.
fn entries(items: &[&MenuItem], depth: usize) -> Vec<MenuEntry> {
    let lang = crate::i18n::current();
    let mut out: Vec<MenuEntry> = Vec::new();
    let mut shown: Vec<String> = Vec::new();
    let mut last_was_sep = true;
    for (i, it) in items.iter().enumerate() {
        if it.path.len() == depth {
            if it.label == "---" {
                if !last_was_sep && i + 1 < items.len() {
                    out.push(MenuEntry::Separator);
                    last_was_sep = true;
                }
                continue;
            }
            let mut entry = MenuEntry::command(crate::i18n::tr_id(lang, &it.id, &it.label), &it.id, it.enabled);
            if let Some(c) = it.checked {
                entry = entry.checked(c);
            }
            if let Some(sc) = &it.shortcut {
                entry = entry.shortcut(sc);
            }
            out.push(entry);
            last_was_sep = false;
        } else if it.path.len() > depth {
            let Some(name) = it.path.get(depth) else { continue };
            if shown.iter().any(|s| s == name) {
                continue;
            }
            shown.push(name.clone());
            let child: Vec<&MenuItem> = items
                .iter()
                .copied()
                .filter(|c| c.path.len() > depth && c.path.get(depth).is_some_and(|p| p == name))
                .collect();
            let enabled = child.iter().any(|c| c.enabled && c.label != "---") || !child.is_empty();
            out.push(
                MenuEntry::command(crate::i18n::tr(lang, name), "", enabled)
                    .submenu(entries(&child, depth + 1)),
            );
            last_was_sep = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, label: &str, path: &[&str]) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: label.into(),
            path: path.iter().map(|s| s.to_string()).collect(),
            shortcut: None,
            enabled: true,
            checked: None,
            color: None,
        }
    }

    fn tree(entries: &[MenuEntry], depth: usize) -> String {
        let mut out = String::new();
        for entry in entries {
            match entry {
                MenuEntry::Separator => {
                    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{}|\n", "  ".repeat(depth)));
                }
                MenuEntry::Command(c) => {
                    let _ = std::fmt::Write::write_fmt(
                        &mut out,
                        format_args!(
                            "{}{}({}){}\n",
                            "  ".repeat(depth),
                            c.label,
                            c.action,
                            c.checked.map(|c| if c { "[✓]" } else { "[ ]]" }).unwrap_or_default(),
                        ),
                    );
                    if let Some(children) = &c.children {
                        out.push_str(&tree(children, depth + 1));
                    }
                }
            }
        }
        out
    }

    fn flat_items() -> Vec<MenuItem> {
        vec![
            item("file.new", "New…", &["File"]),
            item("", "---", &["File"]),
            item("file.save", "Save", &["File"]),
            item("edit.undo", "Undo", &["Edit"]),
            item("view.zoomIn", "Zoom In", &["View"]),
            item("view.grid", "Show Grid", &["View", "Extras"]),
            item("view.rulers", "Show Rulers", &["View", "Extras"]),
        ]
    }

    #[test]
    fn a_hosted_global_menu_hides_the_in_window_titles() {
        use crate::PhotocraftApp;
        use egui::vec2;
        use egui_kittest::{Harness, kittest::Queryable};

        for (hosted, expect_titles) in [(false, true), (true, false)] {
            // Fonts land on the context inside `build_eframe`, before the app's first frame.
            let mut harness = Harness::builder()
                .with_size(vec2(1200.0, 800.0))
                .with_max_steps(16)
                .build_eframe(move |cc| {
                    PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::ALL[0]);
                    let mut app =
                        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                    // Opt out of really starting an exporter (and the field is then never
                    // touched): the hosted state is what the test drives.
                    app.global_menu_unavailable = true;
                    app.global_menu_hosted = hosted;
                    app
                });
            harness.run_steps(3);
            // "File" appears in the app only as a menu title, in-window or global-hosted.
            let has_titles = harness.query_by_label_contains("File").is_some();
            assert_eq!(has_titles, expect_titles, "hosted={hosted}");
        }
    }

    #[test]
    fn the_flat_item_list_becomes_the_same_tree_the_menu_bar_renders() {
        let items = flat_items();
        let printed = tree(&model(&items).children, 0);
        let expected = "File()\n  New…(file.new)\n  |\n  Save(file.save)\n\
Edit()\n  Undo(edit.undo)\nView()\n  Zoom In(view.zoomIn)\n  Extras()\n    Show Grid(view.grid)\n    Show Rulers(view.rulers)\n";
        assert_eq!(printed, expected, "{printed}");
    }

    #[test]
    fn adjacent_separators_collapse_and_empty_tops_are_skipped() {
        let items = vec![
            item("file.new", "New…", &["File"]),
            item("", "---", &["File"]),
            item("", "---", &["File"]),
            item("", "---", &["Edit"]), // nothing before it: suppressed
            item("edit.undo", "Undo", &["Edit"]),
            // No tops but File/Edit exist: the other eight top menus stay away.
        ];
        let printed = tree(&model(&items).children, 0);
        // Two adjacent "---" become one separator (doubled collapse), and a separator with
        // nothing printed before it within the level is suppressed.
        assert_eq!(printed, "File()\n  New…(file.new)\n  |\nEdit()\n  Undo(edit.undo)\n", "{printed}");
    }
}
