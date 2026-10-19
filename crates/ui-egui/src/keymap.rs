//! Keyboard shortcuts: every command's default key comes from its `CommandSpec`; a preset
//! (SolidCraft, or keys laid out as another CAD application documents its own, for people used
//! to it) changes some of them, and the user can rebind or clear any of them on top
//! (Preferences › Keyboard Shortcuts). A new key that another command already uses is reported
//! and only taken over on request; keys the app itself handles (Esc, S, V, Delete, Ctrl+Z, Space…)
//! can't be bound. Changes are kept between runs, apart from the design.
//!
//! Keys are written with `Ctrl` for the command key (Cmd on macOS) and `Alt` for Option, so a
//! file exported on one system works on another; they are shown with the system's own names.

use std::collections::BTreeMap;

use egui::RichText;
use serde_json::{Value, json};
use solidcraft_engine::command_specs;

use crate::SolveApp;
use crate::theme::Tokens;

/// Keys the application handles before any command shortcut.
pub const RESERVED: &[&str] = &[
    "Escape",
    "S",
    "V",
    "F2",
    "F6",
    "Delete",
    "Backspace",
    "Ctrl+Z",
    "Ctrl+Y",
    "Ctrl+S",
    "Ctrl+N",
    "Ctrl+O",
    "Ctrl+Comma",
    "Ctrl+Shift+A",
    "Ctrl+C",
    "Ctrl+X",
    "Ctrl+V",
    // View keys (`navigation::view_key`).
    "Left",
    "Right",
    "Up",
    "Down",
    "Shift+Left",
    "Shift+Right",
    "Shift+Up",
    "Shift+Down",
    "Ctrl+Left",
    "Ctrl+Right",
    "Ctrl+Up",
    "Ctrl+Down",
    "Ctrl+8",
    // Space held pans (`navigation::handle`).
    "Space",
    // The interface scale (`prefs::apply`).
    "Ctrl+Plus",
    "Ctrl+Equals",
    "Ctrl+Minus",
    "Ctrl+0",
    // A new agent chat; expression completion in a value box.
    "Ctrl+Shift+L",
    "Ctrl+Space",
];

/// A starting set of keys: SolidCraft's own, or keys laid out the way another CAD application's
/// public documentation lists its defaults, for people used to it. A preset changes only the keys
/// it names; the user's own changes go on top of it.
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// The mouse preset (Preferences › Navigation) chosen with it.
    pub nav: &'static str,
    /// Command id → key, over the commands' own keys (`""`: no key).
    pub keys: &'static [(&'static str, &'static str)],
}

/// The presets, SolidCraft's first (the default).
pub const PRESETS: &[Preset] = &[
    Preset { id: "solidcraft", label: "SolidCraft", nav: "fusion", keys: &[] },
    Preset { id: "fusion", label: "Fusion style", nav: "fusion", keys: FUSION },
    Preset { id: "solidworks", label: "SolidWorks style", nav: "solidworks", keys: SOLIDWORKS },
    Preset { id: "onshape", label: "Onshape style", nav: "onshape", keys: ONSHAPE },
];

// The presets' keys follow each application's published list of default shortcuts (Autodesk's
// Fusion help, the SOLIDWORKS help and blog, Onshape's help); only the keys that have a SolidCraft
// command, and that SolidCraft does not keep for itself, are given.

/// SolidCraft's own keys already follow Fusion's letters; these add the rest it documents.
const FUSION: &[(&str, &str)] = &[("joint.as_built", "Shift+J"), ("timeline.compute_all", "Ctrl+B")];

/// Standard views on Ctrl+1…7 (Ctrl+8, Look At, is SolidCraft's own), F fits, Ctrl+B rebuilds.
const SOLIDWORKS: &[(&str, &str)] = &[
    ("view.front", "Ctrl+1"),
    ("view.back", "Ctrl+2"),
    ("view.left", "Ctrl+3"),
    ("view.right", "Ctrl+4"),
    ("view.top", "Ctrl+5"),
    ("view.bottom", "Ctrl+6"),
    ("view.iso", "Ctrl+7"),
    ("view.fit", "F"),
    ("solid.fillet", ""),
    ("timeline.compute_all", "Ctrl+B"),
];

/// Sketch tools and constraints on letters, features on Shift+letters, standard views on
/// Shift+1…7, F fits.
const ONSHAPE: &[(&str, &str)] = &[
    // Sketch tools.
    ("sketch.arc.three_point", "A"),
    ("sketch.rectangle.center", "R"),
    ("sketch.rectangle.two_point", "G"),
    ("sketch.construction", "Q"),
    ("sketch.trim", "M"),
    ("sketch.extend", "X"),
    ("sketch.project", "U"),
    ("sketch.fillet", "Shift+F"),
    ("sketch.point", "Shift+S"),
    // Constraints.
    ("sketch.constraint.coincident", "I"),
    ("sketch.constraint.horizontal_vertical", "H"),
    ("sketch.constraint.equal", "E"),
    ("sketch.constraint.parallel", "B"),
    ("sketch.constraint.perpendicular", "Shift+L"),
    ("sketch.constraint.tangent", "T"),
    ("sketch.constraint.concentric", "Shift+O"),
    ("sketch.constraint.midpoint", "Shift+M"),
    ("sketch.constraint.fix", "Shift+J"),
    ("sketch.constraint.symmetry", "Shift+Q"),
    // Features.
    ("solid.extrude", "Shift+E"),
    ("solid.revolve", "Shift+W"),
    ("solid.fillet", "Shift+F"),
    ("sketch.create", "Shift+S"),
    // Views.
    ("view.front", "Shift+1"),
    ("view.back", "Shift+2"),
    ("view.left", "Shift+3"),
    ("view.right", "Shift+4"),
    ("view.top", "Shift+5"),
    ("view.bottom", "Shift+6"),
    ("view.iso", "Shift+7"),
    ("view.fit", "F"),
];

/// The preset `id`, or SolidCraft's.
pub fn preset(id: &str) -> &'static Preset {
    PRESETS.iter().find(|p| p.id == id).unwrap_or(&PRESETS[0])
}

#[derive(Default)]
pub struct Keymap {
    /// The preset the keys start from (a [`PRESETS`] id; empty or unknown: SolidCraft's).
    pub preset: String,
    /// Command id → key (`""`: no key). Only the ones changed from the preset.
    pub custom: BTreeMap<String, String>,
    /// Asks for the Keyboard Shortcuts page of Preferences (File and Help menus).
    pub open: bool,
    /// Opened from Help: a list to read, not to edit.
    pub read_only: bool,
    filter: String,
    /// The command waiting for a key press.
    capturing: Option<String>,
    /// A key that another command uses: (command, key, the other commands).
    conflict: Option<(String, String, Vec<String>)>,
    message: Option<String>,
}

impl Keymap {
    pub fn prefs(&self) -> Value {
        json!(self.custom)
    }
    pub fn load(&mut self, v: &Value) {
        self.custom =
            v.as_object().map(|o| o.iter().take(2000).filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
    }
}

/// A key press as a shortcut name: `E`, `Shift+E`, `Ctrl+Alt+K`, `F5`, `Delete`.
pub fn key_name(key: egui::Key, mods: egui::Modifiers) -> String {
    let mut s = String::new();
    if mods.command || mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    s.push_str(key.name());
    s
}

/// The key a command has in the preset, before the user's changes.
pub fn preset_key(app: &SolveApp, id: &str) -> Option<&'static str> {
    match preset(&app.keymap.preset).keys.iter().find(|(c, _)| *c == id) {
        Some((_, k)) => Some(*k).filter(|k| !k.is_empty()),
        None => solidcraft_engine::find_command(id).and_then(|c| c.shortcut),
    }
}

/// The key a press names: a digit key by its place on the keyboard, so Shift+1 is `Shift+1`
/// whatever character the layout puts there (`!` on a US keyboard).
pub fn pressed_key(key: egui::Key, physical: Option<egui::Key>) -> egui::Key {
    use egui::Key::*;
    match physical {
        Some(p @ (Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9)) => p,
        _ => key,
    }
}

/// The key a command runs from now.
pub fn effective(app: &SolveApp, id: &str) -> Option<String> {
    match app.keymap.custom.get(id) {
        Some(k) if k.is_empty() => None,
        Some(k) => Some(k.clone()),
        None => preset_key(app, id).map(str::to_string),
    }
}

/// The key a command runs from now, with this system's names for its modifiers.
pub fn shown(app: &SolveApp, id: &str) -> Option<String> {
    effective(app, id).map(|k| display(&k))
}

/// `key` (as stored: `Ctrl+Shift+E`) with this system's names for its modifiers.
pub fn display(key: &str) -> String {
    display_for(key, crate::tool_modifiers::Platform::host())
}

/// `key` with `platform`'s names for its modifiers: Cmd and Option on macOS.
pub fn display_for(key: &str, platform: crate::tool_modifiers::Platform) -> String {
    if platform != crate::tool_modifiers::Platform::Mac {
        return key.to_string();
    }
    let mut parts: Vec<&str> = key.split('+').collect();
    let last = parts.pop().unwrap_or_default();
    let mut out: Vec<&str> = parts
        .into_iter()
        .map(|m| match m {
            "Ctrl" => "Cmd",
            "Alt" => "Option",
            other => other,
        })
        .collect();
    out.push(last);
    out.join("+")
}

/// Start from the preset `id`: its keys (the user's changes go) and its mouse preset.
pub fn apply_preset(app: &mut SolveApp, id: &str) -> Result<(), String> {
    let Some(p) = PRESETS.iter().find(|p| p.id == id) else {
        return Err(format!("`{id}` is not a shortcut preset ({})", PRESETS.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")));
    };
    app.keymap.preset = p.id.to_string();
    app.keymap.custom.clear();
    app.keymap.conflict = None;
    app.keymap.capturing = None;
    app.keymap.message = None;
    crate::prefs::set_nav(app, p.nav);
    Ok(())
}

/// Commands bound to `key` now.
pub fn commands_for(app: &SolveApp, key: &str) -> Vec<&'static str> {
    command_specs().into_iter().filter(|c| effective(app, c.id).as_deref() == Some(key)).map(|c| c.id).collect()
}

/// Keys that two commands of the same place (a sketch, or the model) both use: only the first
/// enabled one would ever run. (key, the commands)
pub fn conflicts(app: &SolveApp) -> Vec<(String, Vec<&'static str>)> {
    let mut by_key: BTreeMap<(bool, String), Vec<&'static str>> = BTreeMap::new();
    for c in command_specs() {
        if let Some(k) = effective(app, c.id) {
            by_key.entry((c.tab == "SKETCH", k)).or_default().push(c.id);
        }
    }
    by_key.into_iter().filter(|(_, ids)| ids.len() > 1).map(|((_, k), ids)| (k, ids)).collect()
}

/// Load keys saved or imported: the preset (an unknown one is SolidCraft's) and the changes on
/// top, without unknown commands or keys SolidCraft keeps for itself. Returns what was left out.
pub fn load_checked(app: &mut SolveApp, preset_id: Option<&str>, keys: &Value) -> Vec<String> {
    app.keymap.preset = preset_id.filter(|id| PRESETS.iter().any(|p| p.id == *id)).unwrap_or(PRESETS[0].id).to_string();
    app.keymap.load(keys);
    let mut left_out = Vec::new();
    app.keymap.custom.retain(|id, key| {
        let keep = solidcraft_engine::find_command(id).is_some() && !RESERVED.contains(&key.as_str());
        if !keep {
            left_out.push(format!("{id} = {key}"));
        }
        keep
    });
    left_out
}

/// The enabled command a key press runs, if any: a sketch's own command while a sketch is open,
/// else the model's (a preset may give a key to one of each).
pub fn command_for(app: &SolveApp, key: &str) -> Option<&'static str> {
    let sketching = app.session.active_sketch.is_some();
    let mut ids = commands_for(app, key);
    ids.sort_by_key(|id| solidcraft_engine::find_command(id).is_none_or(|c| (c.tab == "SKETCH") != sketching));
    ids.into_iter().find(|id| solidcraft_engine::find_command(id).is_some_and(|c| c.info(&app.session).enabled))
}

/// Run a shortcut key: its enabled command; outside a sketch, a sketch tool's key opens Create
/// Sketch (on the selected plane or planar face, else on the plane picked next) and the tool
/// starts in the new sketch.
pub fn press(app: &mut SolveApp, key: &str) {
    if let Some(id) = command_for(app, key) {
        app.start(id);
    } else if let Some(id) = sketch_tool_for(app, key) {
        app.start("sketch.create");
        if let Some(d) = app.dialog.as_mut().filter(|d| matches!(d.kind, crate::dialogs::Kind::Sketch)) {
            d.then_start = Some(id);
        }
    }
}

/// Outside a sketch: the SKETCH-tab command bound to `key` that a new sketch would start. Not
/// Normal/Construction (a mode of the drawing in progress) nor the Finish Sketch panel.
fn sketch_tool_for(app: &SolveApp, key: &str) -> Option<&'static str> {
    if app.session.active_sketch.is_some() {
        return None;
    }
    commands_for(app, key).into_iter().find(|id| {
        solidcraft_engine::find_command(id).is_some_and(|c| c.tab == "SKETCH" && c.panel != "FINISH SKETCH" && c.id != "sketch.construction")
    })
}

/// Bind `key` to a command (`""` clears it). Refused for reserved keys, and when other commands
/// use the key unless `replace` (they lose it).
pub fn bind(app: &mut SolveApp, id: &str, key: &str, replace: bool) -> Result<(), String> {
    solidcraft_engine::find_command(id).ok_or_else(|| format!("no command `{id}`"))?;
    if RESERVED.contains(&key) {
        return Err(format!("{key} is used by SolidCraft itself"));
    }
    if !key.is_empty() {
        let others: Vec<&'static str> = commands_for(app, key).into_iter().filter(|o| *o != id).collect();
        if !others.is_empty() && !replace {
            let names: Vec<&str> = others.iter().filter_map(|o| solidcraft_engine::find_command(o).map(|c| c.label)).collect();
            return Err(format!("{key} is already used by {}", names.join(", ")));
        }
        for o in others {
            app.keymap.custom.insert(o.to_string(), String::new());
        }
    }
    if preset_key(app, id).unwrap_or("") == key {
        app.keymap.custom.remove(id);
    } else {
        app.keymap.custom.insert(id.to_string(), key.to_string());
    }
    Ok(())
}

/// Back to the preset's key for `id`, or for every command.
pub fn reset(app: &mut SolveApp, id: Option<&str>) {
    match id {
        Some(id) => drop(app.keymap.custom.remove(id)),
        None => app.keymap.custom.clear(),
    }
}

/// The Keyboard Shortcuts page of Preferences: search, rebind (click, then press the key), clear,
/// reset; a key another command uses is reported and taken over only on request.
pub fn page(app: &mut SolveApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    // A key press while waiting for one binds it (Esc cancels).
    if let Some(id) = app.keymap.capturing.clone() {
        let press = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => Some((pressed_key(*key, *physical_key), *modifiers)),
                _ => None,
            })
        });
        if let Some((k, m)) = press {
            app.keymap.capturing = None;
            if k != egui::Key::Escape {
                let name = key_name(k, m);
                match bind(app, &id, &name, false) {
                    Ok(()) => app.keymap.message = None,
                    Err(e) if e.contains("already used") => {
                        app.keymap.conflict = Some((id.clone(), name.clone(), commands_for(app, &name).iter().map(|s| s.to_string()).collect()));
                        app.keymap.message = Some(e);
                    }
                    Err(e) => app.keymap.message = Some(e),
                }
            }
        }
    }
    let t = Tokens::get();
    let mut action: Option<(String, &str)> = None;
    {
        {
            if !app.keymap.read_only {
                ui.horizontal(|ui| {
                    ui.label(app.tr("ui.shortcut_preset"));
                    let current = preset(&app.keymap.preset);
                    let name = app.tr_label(current.label).to_string();
                    let text = if app.keymap.custom.is_empty() { name } else { format!("{name} {}", app.tr("ui.shortcut_preset_changed")) };
                    let combo = egui::ComboBox::from_id_salt("pref_shortcut_preset").selected_text(text).show_ui(ui, |ui| {
                        for p in PRESETS {
                            let item = ui.selectable_label(p.id == current.id && app.keymap.custom.is_empty(), app.tr_label(p.label));
                            crate::scenario::publish_handle(&format!("prefs:shortcut_preset:{}", p.id), item.rect.center());
                            if item.clicked() {
                                action = Some((p.id.to_string(), "preset"));
                            }
                        }
                    });
                    crate::scenario::publish_handle("prefs:shortcut_preset", combo.response.rect.center());
                });
                ui.label(RichText::new(app.tr("ui.shortcut_preset_note")).size(11.0).color(t.text_dim));
            }
            ui.horizontal(|ui| {
                let filter = ui.add(
                    egui::TextEdit::singleline(&mut app.keymap.filter)
                        .hint_text(crate::i18n::tr_label("Filter commands or keys"))
                        .desired_width(300.0),
                );
                crate::scenario::publish_handle("prefs:shortcut_filter", filter.rect.center());
                if !app.keymap.read_only && ui.button(app.tr_label("Reset All")).clicked() {
                    action = Some((String::new(), "reset_all"));
                }
                if ui.button(app.tr_label("Export…")).on_hover_text(app.tr_label("Keys, S box favourites and the toolbar, as a file")).clicked() {
                    action = Some((String::new(), "export"));
                }
                if !app.keymap.read_only && ui.button(app.tr_label("Import…")).clicked() {
                    action = Some((String::new(), "import"));
                }
            });
            if let Some(m) = &app.keymap.message {
                ui.label(RichText::new(m).color(t.warning));
            }
            // Keys two commands of the same place share: only the first enabled one runs.
            for (key, ids) in conflicts(app).into_iter().take(4) {
                let names: Vec<&str> = ids.iter().filter_map(|id| solidcraft_engine::find_command(id).map(|c| app.tr_label(c.label))).collect();
                let text = crate::i18n::format("ui.shortcut_conflict", &[("key", &display(&key)), ("commands", &names.join(", "))]);
                ui.label(RichText::new(text).color(t.warning));
            }
            if let Some((id, key, _)) = app.keymap.conflict.clone() {
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::format("ui.shortcut_use_anyway", &[("key", &display(&key))])).clicked() {
                        action = Some((format!("{id}\n{key}"), "replace"));
                    }
                    if ui.button(app.tr_label("Cancel")).clicked() {
                        action = Some((String::new(), "cancel"));
                    }
                });
            }
            ui.label(
                RichText::new(
                    app.tr_label("Select Other: hold the left button still on the model (or right-click › Select Other…) to list everything under the cursor; click again on the same spot to select the next thing there. Double-click the middle button to go Home (Preferences › Navigation)."),
                )
                .size(11.0)
                .color(t.text_dim),
            );
            ui.separator();
            let q = app.keymap.filter.to_lowercase();
            egui::ScrollArea::both().max_height(330.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("sc_keymap_grid").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for c in command_specs() {
                        let key = effective(app, c.id).map(|k| display(&k)).unwrap_or_default();
                        let translated_label = app.tr_label(c.label);
                        if !q.is_empty() && ![c.label, c.id, key.as_str(), translated_label].iter().any(|s| s.to_lowercase().contains(&q)) {
                            continue;
                        }
                        ui.label(translated_label);
                        ui.label(RichText::new(c.id).size(10.5).color(t.text_dim));
                        let capturing = app.keymap.capturing.as_deref() == Some(c.id);
                        let changed = app.keymap.custom.contains_key(c.id);
                        let text = if capturing {
                            app.tr_label("press a key…").to_string()
                        } else if key.is_empty() {
                            "—".to_string()
                        } else {
                            key.clone()
                        };
                        if app.keymap.read_only {
                            ui.label(RichText::new(text).strong());
                            ui.end_row();
                            continue;
                        }
                        let b = egui::Button::new(RichText::new(text).color(if changed { t.accent } else { t.text })).min_size(egui::vec2(80.0, 0.0));
                        if ui.add(b).on_hover_text(app.tr_label("Click, then press the new key")).clicked() {
                            action = Some((c.id.to_string(), "capture"));
                        }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!key.is_empty(), egui::Button::new(app.tr_label("Clear")).small()).clicked() {
                                action = Some((c.id.to_string(), "clear"));
                            }
                            if ui.add_enabled(changed, egui::Button::new(app.tr_label("Reset")).small()).clicked() {
                                action = Some((c.id.to_string(), "reset"));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
    }
    match action {
        Some((id, "capture")) => {
            app.keymap.capturing = Some(id);
            app.keymap.conflict = None;
            app.keymap.message = None;
        }
        Some((id, "clear")) => drop(bind(app, &id, "", false)),
        Some((id, "reset")) => reset(app, Some(&id)),
        Some((_, "reset_all")) => reset(app, None),
        Some((id, "preset")) => {
            if let Err(e) = apply_preset(app, &id) {
                app.keymap.message = Some(e);
            }
        }
        Some((_, "export")) => {
            let text = serde_json::to_string_pretty(&crate::toolbar_custom::export(app)).unwrap_or_default();
            app.pick_save("SolidCraft shortcuts.json", &["json"], move |app, path| {
                if let Some(path) = path {
                    // Through the engine's files, so the browser hands it out as a download.
                    app.keymap.message = solidcraft_engine::io::vfs::write(&path, text.as_bytes()).err().map(|e| format!("export: {e}"));
                }
            });
        }
        Some((_, "import")) => {
            app.pick_open_for(&["json"], |app, path| {
                if let Some(path) = path {
                    import_file(app, &path);
                }
            });
        }
        Some((pair, "replace")) => {
            if let Some((id, key)) = pair.split_once('\n') {
                let _ = bind(app, id, key, true);
            }
            app.keymap.conflict = None;
            app.keymap.message = None;
        }
        Some((_, "cancel")) => {
            app.keymap.conflict = None;
            app.keymap.message = None;
        }
        _ => {}
    }
}

/// Most bytes a shortcuts file may have.
const MAX_FILE: usize = 4 << 20;

/// Import a shortcuts file (Import…, or a file dropped on the window); the outcome goes to the
/// Keyboard Shortcuts page and the status line.
pub fn import_file(app: &mut SolveApp, path: &str) {
    let r = solidcraft_engine::io::vfs::read(path, MAX_FILE)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()))
        .and_then(|v| crate::toolbar_custom::import(app, &v));
    match r {
        Ok(left_out) if left_out.is_empty() => {
            app.keymap.message = None;
            app.set_status(app.tr("ui.shortcuts_imported").to_string(), false);
        }
        Ok(left_out) => {
            let text = crate::i18n::format("ui.shortcuts_left_out", &[("keys", &left_out.join(", "))]);
            app.keymap.message = Some(text.clone());
            app.set_status(text, true);
        }
        Err(e) => {
            app.keymap.message = Some(format!("import: {e}"));
            app.set_status(format!("import: {e}"), true);
        }
    }
}

/// Whether the file at `path` is a SolidCraft shortcuts file (by its contents).
pub fn is_shortcuts_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".json")
        && solidcraft_engine::io::vfs::read(path, MAX_FILE)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|v| crate::toolbar_custom::is_shortcuts(&v))
}

/// Stop waiting for a key (the dialog closed).
pub fn stop_capture(app: &mut SolveApp) {
    app.keymap.capturing = None;
    app.keymap.conflict = None;
    app.keymap.message = None;
}

/// Is the page waiting for a key (so the key must not also run a command)?
pub fn capturing(app: &SolveApp) -> bool {
    app.prefs_window.open && app.prefs_window.section == crate::prefs::PAGE_SHORTCUTS && app.keymap.capturing.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SolveApp {
        SolveApp::new(solidcraft_engine::Session::default(), Default::default())
    }

    #[test]
    fn rebinding_detects_conflicts_and_resets() {
        let mut a = app();
        assert_eq!(effective(&a, "solid.extrude").as_deref(), Some("E"));
        // Shift+E is free.
        bind(&mut a, "solid.extrude", "Shift+E", false).unwrap();
        assert_eq!(commands_for(&a, "Shift+E"), vec!["solid.extrude"]);
        assert!(commands_for(&a, "E").is_empty());
        // M belongs to Move/Copy: refused, then taken over on request.
        let e = bind(&mut a, "solid.revolve", "M", false).unwrap_err();
        assert!(e.contains("already used by Move/Copy"), "{e}");
        bind(&mut a, "solid.revolve", "M", true).unwrap();
        assert_eq!(commands_for(&a, "M"), vec!["solid.revolve"]);
        assert_eq!(effective(&a, "solid.move"), None);
        assert!(bind(&mut a, "solid.revolve", "Ctrl+Z", false).is_err(), "reserved");
        // Kept between runs.
        let mut b = app();
        b.keymap.load(&a.keymap.prefs());
        assert_eq!(effective(&b, "solid.revolve").as_deref(), Some("M"));
        reset(&mut b, None);
        assert_eq!(effective(&b, "solid.move").as_deref(), Some("M"));
        assert_eq!(key_name(egui::Key::K, egui::Modifiers { ctrl: true, shift: true, ..Default::default() }), "Ctrl+Shift+K");
    }

    #[test]
    fn every_preset_binds_real_commands_to_free_keys_without_conflicts() {
        for p in PRESETS {
            assert!(crate::prefs::NAV_PRESETS.iter().any(|n| n.id == p.nav), "{}: mouse preset {}", p.id, p.nav);
            for (id, key) in p.keys {
                assert!(solidcraft_engine::find_command(id).is_some(), "{}: no command {id}", p.id);
                assert!(!RESERVED.contains(key), "{}: {key} is SolidCraft's own", p.id);
            }
            let mut a = app();
            apply_preset(&mut a, p.id).unwrap();
            assert_eq!(conflicts(&a), Vec::<(String, Vec<&str>)>::new(), "{}", p.id);
            for (id, key) in p.keys {
                assert_eq!(effective(&a, id).unwrap_or_default(), *key, "{}: {id}", p.id);
            }
        }
    }

    #[test]
    fn a_preset_sets_keys_and_mouse_and_changes_go_on_top_until_the_next_preset() {
        let mut a = app();
        let p = PRESETS.iter().find(|p| p.id == "solidworks").unwrap();
        apply_preset(&mut a, p.id).unwrap();
        assert_eq!(a.preferences.nav, p.nav);
        assert_eq!(a.preferences.zoom_reverse, crate::prefs::nav_preset(p.nav).reverse_zoom);
        let (id, key) = p.keys.iter().find(|(_, k)| !k.is_empty()).unwrap();
        // A change on top of the preset, and back.
        bind(&mut a, id, "Shift+F12", false).unwrap();
        assert_eq!(effective(&a, id).as_deref(), Some("Shift+F12"));
        reset(&mut a, Some(id));
        assert_eq!(effective(&a, id).as_deref(), Some(*key));
        // Binding the preset's own key again is no change.
        bind(&mut a, id, key, false).unwrap();
        assert!(a.keymap.custom.is_empty());
        bind(&mut a, id, "Shift+F12", false).unwrap();
        // Saved and loaded with the window's settings.
        let mut b = app();
        b.load_prefs(&a.prefs());
        assert_eq!((b.keymap.preset.as_str(), effective(&b, id).as_deref()), ("solidworks", Some("Shift+F12")));
        // Choosing a preset again starts from it.
        apply_preset(&mut b, "solidcraft").unwrap();
        assert!(b.keymap.custom.is_empty());
        assert_eq!(effective(&b, "solid.extrude").as_deref(), Some("E"));
        assert!(apply_preset(&mut b, "no-such").is_err());
    }

    #[test]
    fn keys_show_the_systems_names_and_navigation_keys_cannot_be_taken() {
        use crate::tool_modifiers::Platform;
        assert_eq!(display_for("Ctrl+Shift+E", Platform::Mac), "Cmd+Shift+E");
        assert_eq!(display_for("Ctrl+Alt+K", Platform::Mac), "Cmd+Option+K");
        assert_eq!(display_for("Ctrl+Alt+K", Platform::Other), "Ctrl+Alt+K");
        assert_eq!(display_for("Plus", Platform::Mac), "Plus");
        let mut a = app();
        for key in ["Space", "Ctrl+Plus", "Ctrl+Minus", "Ctrl+0", "Ctrl+Shift+L", "Ctrl+Space", "Left"] {
            assert!(bind(&mut a, "solid.extrude", key, true).is_err(), "{key}");
        }
    }

    #[test]
    fn a_shortcuts_file_keeps_its_preset_and_leaves_out_what_cannot_be_bound() {
        let mut a = app();
        apply_preset(&mut a, "onshape").unwrap();
        bind(&mut a, "solid.revolve", "Shift+F11", false).unwrap();
        let file = crate::toolbar_custom::export(&a);
        let mut b = app();
        assert_eq!(crate::toolbar_custom::import(&mut b, &file).unwrap(), Vec::<String>::new());
        assert_eq!(b.keymap.preset, "onshape");
        assert_eq!(b.preferences.nav, a.preferences.nav);
        assert_eq!(effective(&b, "solid.revolve").as_deref(), Some("Shift+F11"));
        // A file from before presets, with a reserved key and an unknown command.
        let old = json!({"format": "solidcraft-shortcuts/1", "shortcuts": {"solid.extrude": "Space", "no.such": "K", "solid.move": "Shift+M"}});
        let left_out = crate::toolbar_custom::import(&mut b, &old).unwrap();
        assert_eq!(left_out.len(), 2, "{left_out:?}");
        assert_eq!(b.keymap.preset, "solidcraft");
        assert_eq!(effective(&b, "solid.move").as_deref(), Some("Shift+M"));
        assert_eq!(effective(&b, "solid.extrude").as_deref(), Some("E"));
        assert!(crate::toolbar_custom::import(&mut b, &json!({"format": "something-else"})).is_err());
        // The same file dropped on the window.
        let dir = std::env::temp_dir().join(format!("sc-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.json");
        std::fs::write(&path, file.to_string()).unwrap();
        let mut c = app();
        c.drop_path(&path.to_string_lossy());
        assert_eq!(c.keymap.preset, "onshape");
        assert_eq!(effective(&c, "solid.revolve").as_deref(), Some("Shift+F11"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_shared_by_a_sketch_command_and_a_model_command_runs_the_one_for_where_you_are() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.frames(5);
        apply_preset(&mut h.app, "onshape").unwrap();
        assert_eq!(command_for(&h.app, "Shift+S"), Some("sketch.create"));
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.frames(5);
        assert_eq!(command_for(&h.app, "Shift+S"), Some("sketch.point"));
        assert_eq!(command_for(&h.app, "M"), Some("sketch.trim"));
        // Shift and a digit name the digit, whatever the layout types there.
        assert_eq!(pressed_key(egui::Key::Exclamationmark, Some(egui::Key::Num1)), egui::Key::Num1);
        assert_eq!(pressed_key(egui::Key::A, Some(egui::Key::Q)), egui::Key::A, "letters follow the layout");
    }

    #[test]
    fn a_view_key_from_a_preset_turns_the_window_to_that_view() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.frames(5);
        h.call("ui.shortcut", json!({"preset": "onshape"}));
        assert_eq!(h.app.preferences.nav, "onshape");
        for (key, view) in [("5", solidcraft_engine::render::StandardView::Top), ("1", solidcraft_engine::render::StandardView::Front)] {
            h.call("ui.key", json!({"key": key, "shift": true}));
            h.frames(90);
            let (yaw, pitch) = view.angles();
            assert!(
                (h.app.cam.yaw - yaw).abs() < 1e-6 && (h.app.cam.pitch - pitch).abs() < 1e-3,
                "Shift+{key}: {} {}",
                h.app.cam.yaw,
                h.app.cam.pitch
            );
        }
        h.call("ui.shortcut", json!({"preset": "solidworks"}));
        h.call("ui.key", json!({"key": "7", "cmd": true}));
        h.frames(90);
        let (yaw, pitch) = solidcraft_engine::render::StandardView::Iso.angles();
        assert!((h.app.cam.yaw - yaw).abs() < 1e-6 && (h.app.cam.pitch - pitch).abs() < 1e-3, "Ctrl+7: {} {}", h.app.cam.yaw, h.app.cam.pitch);
    }

    #[test]
    fn conflicts_are_keys_two_commands_of_one_place_share() {
        let mut a = app();
        assert!(conflicts(&a).is_empty(), "{:?}", conflicts(&a));
        a.keymap.custom.insert("solid.revolve".into(), "E".into());
        assert_eq!(
            conflicts(&a),
            vec![("E".to_string(), vec!["solid.extrude", "solid.revolve"])]
                .into_iter()
                .map(|(k, mut v)| {
                    v.sort();
                    (k, v)
                })
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
#[path = "sketch_shortcut_tests.rs"]
mod sketch_shortcut_tests;
