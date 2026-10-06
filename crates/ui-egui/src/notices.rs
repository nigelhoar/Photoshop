//! Non-blocking notices: a small stack of cards in the lower-right corner of the window for things
//! the user should see but that must not interrupt work (import/export warnings such as "adjustment
//! flattened", files that couldn't be opened). Notices live in [`UiState`](crate::UiState) so the
//! control channel can read them (`ui.inspect`) and clear them (`ui.set`).

use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;

const WAYLAND_FILE_DROP_DISMISSED: &str = "ui.waylandFileDropGuidanceDismissed";
const WAYLAND_FILE_DROP_TITLE: &str = "Native file drag-and-drop is unavailable";

/// At most this many notices are kept; older ones drop off.
pub const MAX_NOTICES: usize = 3;
/// Lines shown per notice before "…and N more".
const MAX_LINES: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    /// An error (warning colour) rather than an informational notice.
    #[serde(default)]
    pub error: bool,
    /// Persist dismissal in preferences rather than only removing this session's notice.
    #[serde(default)]
    pub persist_dismissal: bool,
}

/// Show a notice (newest last).
pub fn post(app: &mut PhotocraftApp, title: impl Into<String>, lines: Vec<String>, error: bool) {
    let id = app.ui.alloc_id();
    app.ui.notices.push(Notice { id, title: title.into(), lines, error, persist_dismissal: false });
    cap_notices(app);
}

fn cap_notices(app: &mut PhotocraftApp) {
    while app.ui.notices.len() > MAX_NOTICES {
        let oldest_temporary = app.ui.notices.iter().position(|notice| !notice.persist_dismissal).unwrap_or(0);
        app.ui.notices.remove(oldest_temporary);
    }
}

/// Show the Wayland-specific fallback guidance unless the user dismissed it in preferences.
pub fn wayland_file_drop_guidance(app: &mut PhotocraftApp) {
    if !app.services.is_wayland
        || app.session.prefs().dialogs.get(WAYLAND_FILE_DROP_DISMISSED).and_then(serde_json::Value::as_bool) == Some(true)
        || app.ui.notices.iter().any(|notice| notice.persist_dismissal)
    {
        return;
    }
    let id = app.ui.alloc_id();
    app.ui.notices.push(Notice {
        id,
        title: WAYLAND_FILE_DROP_TITLE.into(),
        lines: vec![
            "Native file drag-and-drop is not supported on Wayland yet. Use File > Open, copy an image file in your file manager and press Ctrl+V, or run PhotoCraft under XWayland.".into(),
        ],
        error: false,
        persist_dismissal: true,
    });
    cap_notices(app);
}

fn dismiss(app: &mut PhotocraftApp, id: u64) {
    let persist = app.ui.notices.iter().any(|notice| notice.id == id && notice.persist_dismissal);
    app.ui.notices.retain(|notice| notice.id != id);
    if persist {
        app.session.prefs.edit(|prefs| {
            prefs.dialogs.insert(WAYLAND_FILE_DROP_DISMISSED.into(), serde_json::Value::Bool(true));
        });
    }
}

/// Report import/export `warnings` for the file operation `what` (e.g. "Opened a.psd"): the status
/// bar says how many there were, and a notice lists them. Nothing happens when there are none.
pub fn io_warnings(app: &mut PhotocraftApp, what: &str, warnings: &[String]) {
    let Some(first) = warnings.first() else { return };
    let n = warnings.len();
    app.ui.status = if n == 1 { format!("{what}: {first}") } else { format!("{what} with {n} warnings: {first} …") };
    app.ui.status_error = true;
    post(app, format!("{what} with {n} warning{}", if n == 1 { "" } else { "s" }), warnings.to_vec(), false);
}

/// Report a failed file operation: the status bar shows it as an error and a notice keeps it on
/// screen until dismissed.
pub fn error(app: &mut PhotocraftApp, message: String) {
    app.ui.status = message.clone();
    app.ui.status_error = true;
    post(app, message, Vec::new(), true);
}

/// Draw the notices; each has a close button.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.ui.notices.is_empty() {
        return;
    }
    let t = crate::theme::Tokens::get(ctx);
    let mut dismiss_id = None;
    // Clear the status bar (~24 px) and leave the dock's edge some air.
    egui::Area::new(egui::Id::new("photocraft-notices"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -36.0))
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            for n in &app.ui.notices {
                egui::Frame::NONE
                    .fill(t.card)
                    .stroke(egui::Stroke::new(1.0, if n.error { t.warning } else { t.card_border }))
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::same(10))
                    .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: t.shadow })
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.horizontal(|ui| {
                            let title = egui::RichText::new(&n.title).strong().color(if n.error { t.warning } else { t.text });
                            ui.add(egui::Label::new(title).wrap());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui.add(egui::Button::new(egui::RichText::new("×").color(t.text_dim)).frame(false)).on_hover_text(tl!("Dismiss")).clicked() {
                                    dismiss_id = Some(n.id);
                                }
                            });
                        });
                        for line in n.lines.iter().take(MAX_LINES) {
                            ui.add(egui::Label::new(egui::RichText::new(format!("• {line}")).color(t.text_dim)).wrap());
                        }
                        if n.lines.len() > MAX_LINES {
                            ui.label(egui::RichText::new(format!("…and {} more", n.lines.len() - MAX_LINES)).color(t.text_faint));
                        }
                    });
                ui.add_space(6.0);
            }
        });
    if let Some(id) = dismiss_id {
        dismiss(app, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhotocraftApp, Services};
    use photocraft_engine::Session;
    use std::sync::{Arc, Mutex};

    #[test]
    fn wayland_guidance_is_only_shown_in_wayland_sessions() {
        let mut app = PhotocraftApp::new(Session::new(), Services { is_wayland: true, ..Default::default() });
        assert_eq!(app.ui.notices.len(), 1);
        assert_eq!(app.ui.notices[0].title, WAYLAND_FILE_DROP_TITLE);
        let guidance = app.ui.notices[0].lines.join(" ");
        assert!(guidance.contains("not supported on Wayland yet"));
        assert!(guidance.contains("File > Open"));
        assert!(guidance.contains("Ctrl+V"));
        assert!(guidance.contains("XWayland"));
        for i in 0..MAX_NOTICES {
            post(&mut app, format!("Transient {i}"), Vec::new(), false);
        }
        assert_eq!(app.ui.notices.len(), MAX_NOTICES);
        assert!(app.ui.notices.iter().any(|notice| notice.persist_dismissal));

        let app = PhotocraftApp::new(Session::new(), Services::default());
        assert!(app.ui.notices.is_empty());
    }

    #[test]
    fn dismissing_wayland_guidance_persists_across_launches() {
        let saved = Arc::new(Mutex::new(None::<String>));
        let load_store = saved.clone();
        let save_store = saved.clone();
        let services = Services {
            is_wayland: true,
            load_prefs: Some(Box::new(move || load_store.lock().unwrap_or_else(|e| e.into_inner()).clone())),
            save_prefs: Some(Box::new(move |text| {
                *save_store.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(Session::new(), services);
        let id = app.ui.notices[0].id;
        dismiss(&mut app, id);
        crate::prefs_ui::tick(&mut app, &egui::Context::default());

        let services =
            Services { is_wayland: true, load_prefs: Some(Box::new(move || saved.lock().unwrap_or_else(|e| e.into_inner()).clone())), ..Default::default() };
        let app = PhotocraftApp::new(Session::new(), services);
        assert!(app.ui.notices.is_empty());
    }
}
