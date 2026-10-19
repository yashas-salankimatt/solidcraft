//! Touch mode (Preferences › General › Touch mode): SolidCraft laid out for fingers, after the
//! conventions of touch-first CAD apps. The model takes the window; the commands live in a column
//! of large buttons along one edge (left, or right for the other hand), each opening the list of
//! its tools; undo and redo sit at the column's foot, where the thumb rests. What applies to the
//! selection floats in a bar above the view's controls. Everything is a little larger, taps add to the selection (a tap
//! on empty space clears it), and a finger may wander a little further and still tap.
//!
//! Nothing here is a second command surface: the column starts the toolbar's commands, and the
//! selection bar's actions are the context menu's.

use egui::{FontId, Rect, Sense, Stroke, pos2, vec2};

use crate::context_menu::{Item, Target};
use crate::theme::Tokens;
use crate::{SolveApp, icons};

/// The interface's scale in touch mode, on top of Preferences › General › Interface scale: a
/// 36-point button becomes the 44 points a fingertip needs.
pub(crate) const SCALE: f32 = 1.2;

/// How far a finger may wander and still tap, in points (egui's default is 6, for a mouse).
pub(crate) const CLICK_SLOP: f32 = 12.0;
const MOUSE_CLICK_SLOP: f32 = 6.0;

/// A column button's size, the gap between buttons and the column's margin, in points.
const BUTTON_W: f32 = 54.0;
const BUTTON_H: f32 = 50.0;
const GAP: f32 = 4.0;
const MARGIN: f32 = 4.0;
const ICON: f32 = 22.0;

/// A flyout's row height, its columns' width and its padding, in points.
const ROW: f32 = 40.0;
const FLYOUT_W: f32 = 210.0;
const PAD: f32 = 6.0;

/// Most of the selection's actions the selection bar shows before More.
const SELECTION_ACTIONS: usize = 5;

/// Which touch layout, if any: on, off, or on where the pointer is a finger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchMode {
    /// On where the host says the pointer is coarse (a tablet, a phone).
    #[default]
    Auto,
    On,
    Off,
}

impl TouchMode {
    pub const ALL: [TouchMode; 3] = [TouchMode::Auto, TouchMode::On, TouchMode::Off];

    pub fn label(self) -> &'static str {
        match self {
            TouchMode::Auto => "Automatic",
            TouchMode::On => "On",
            TouchMode::Off => "Off",
        }
    }
}

/// The column's open list, if any (`y`: the top of the button that opened it).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Flyout {
    #[default]
    None,
    /// The tabs (SOLID, SURFACE…).
    Tabs { y: f32 },
    /// One panel's commands.
    Panel { panel: &'static str, y: f32 },
}

/// Touch mode's own state.
#[derive(Clone, Debug, Default)]
pub struct TouchUi {
    pub flyout: Flyout,
}

/// Whether the touch layout is on.
pub fn active(app: &SolveApp) -> bool {
    match app.preferences.touch_mode {
        TouchMode::On => true,
        TouchMode::Off => false,
        TouchMode::Auto => app.touch_first,
    }
}

/// The interface scale and tap tolerance touch mode asks for (with the user's own scale).
pub(crate) fn apply(app: &SolveApp, ctx: &egui::Context) {
    let slop = if active(app) { CLICK_SLOP } else { MOUSE_CLICK_SLOP };
    if ctx.options(|o| o.input_options.max_click_dist) != slop {
        ctx.options_mut(|o| o.input_options.max_click_dist = slop);
    }
}

/// The interface scale for touch mode on top of `scale`.
pub(crate) fn scale(app: &SolveApp, scale: f32) -> f32 {
    if active(app) { scale * SCALE } else { scale }
}

/// The tab the column's groups come from: the open sketch's, else the toolbar's.
fn tab(app: &SolveApp) -> String {
    if app.session.active_sketch.is_some() { "SKETCH".into() } else { app.ui.tab.clone() }
}

/// The panels the column shows for `tab` (selection filters stay in the toolbar's menus).
fn panels(tab: &str) -> Vec<&'static str> {
    crate::toolbar::panels(tab).iter().copied().filter(|p| *p != "SELECT").collect()
}

/// A panel's commands, as its toolbar drop-down lists them.
fn commands(tab: &str, panel: &str) -> Vec<&'static solidcraft_engine::CommandSpec> {
    crate::toolbar_custom::panel_lists(tab).into_iter().find(|(p, _, _)| *p == panel).map(|(_, cmds, _)| cmds).unwrap_or_default()
}

/// A panel's icon: its first tool's (after Create Sketch, which starts every Create panel).
fn panel_icon(cmds: &[&'static solidcraft_engine::CommandSpec]) -> Option<&'static str> {
    cmds.iter().find(|c| c.id != "sketch.create").or(cmds.first()).map(|c| c.icon)
}

/// What the selection can do, from the context menu: its enabled actions that have an icon.
fn selection_items(app: &SolveApp) -> Vec<Item> {
    if app.session.selection.is_empty() || app.tool.is_some() || app.dialog.is_some() {
        return Vec::new();
    }
    crate::context_menu::items(app, &Target::Viewport)
        .into_iter()
        .filter(|i| i.enabled && !i.icon.is_empty() && i.id != "-" && !i.id.starts_with('#'))
        .collect()
}

/// `label` on one line within `width`: in smaller type first, then cut short.
fn fit_label(p: &egui::Painter, label: &str, width: f32, color: egui::Color32) -> std::sync::Arc<egui::Galley> {
    for size in [10.0, 9.0] {
        let g = p.layout_no_wrap(label.to_string(), FontId::proportional(size), color);
        if g.size().x <= width {
            return g;
        }
    }
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), FontId::proportional(8.0), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    p.layout_job(job)
}

/// A column button with `icon` and a short `label` under it, lit when `on`.
fn button(ui: &mut egui::Ui, at: Rect, name: &str, icon: &str, label: &str, on: bool, enabled: bool) -> egui::Response {
    let t = Tokens::get();
    let resp = ui.interact(at, ui.id().with(("touch_button", name)), if enabled { Sense::click() } else { Sense::hover() });
    // Published for tests and the control channel (`ui.handles {"prefix": "touch:"}`).
    crate::scenario::publish_rect(&format!("touch:{name}"), at);
    crate::scenario::publish_handle(&format!("touch:{name}"), at.center());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let p = ui.painter();
    if on {
        p.rect_filled(at, 8.0, t.accent_soft);
    } else if enabled && (resp.is_pointer_button_down_on() || resp.hovered()) {
        p.rect_filled(at, 8.0, t.hover);
    }
    let (ink, fill) = if enabled { (t.icon, t.icon_fill) } else { (t.border, t.panel_header) };
    let accent = if enabled { crate::icon_svg::primary(icon, crate::theme::is_dark(), crate::theme::vision()) } else { t.border };
    icons::paint(p, Rect::from_center_size(pos2(at.center().x, at.top() + 18.0), vec2(ICON, ICON)), icon, ink, fill, accent);
    let galley = fit_label(p, label, at.width() - 4.0, if enabled { t.text } else { t.text_dim });
    p.galley(pos2(at.center().x - galley.size().x / 2.0, at.bottom() - galley.size().y - 4.0), galley, t.text);
    resp
}

/// A thin line across the column between its groups, at `y`.
fn divider(ui: &egui::Ui, column: Rect, y: f32) {
    let t = Tokens::get();
    ui.painter().line_segment([pos2(column.left() + 2.0 * MARGIN, y), pos2(column.right() - 2.0 * MARGIN, y)], Stroke::new(1.0, t.border));
}

/// The touch column, as a side panel (call it where the toolbar would be).
pub fn column(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let right = app.preferences.touch_column_right;
    let width = BUTTON_W + 2.0 * MARGIN;
    let panel = if right { egui::Panel::right("sc_touch_column") } else { egui::Panel::left("sc_touch_column") };
    let mut column_rect = Rect::NOTHING;
    panel.exact_size(width).resizable(false).frame(egui::Frame::NONE.fill(t.toolbar)).show(ui, |ui| {
        let r = ui.max_rect();
        column_rect = r;
        crate::scenario::publish_rect("touch:column", r);
        let slot = |y: f32| Rect::from_min_size(pos2(r.left() + MARGIN, y), vec2(BUTTON_W, BUTTON_H));
        let step = BUTTON_H + GAP;
        let mut y = r.top() + MARGIN;
        let tab = tab(app);
        let busy = app.applying.is_some();

        // The tab the groups come from (a sketch keeps its own).
        let tab_label = app.tr_label(&tab).to_string();
        let open_tabs = matches!(app.touch_ui.flyout, Flyout::Tabs { .. });
        if button(ui, slot(y), "tab", "ws_design", &tab_label, open_tabs, app.session.active_sketch.is_none()).clicked() {
            app.touch_ui.flyout = if open_tabs { Flyout::None } else { Flyout::Tabs { y } };
        }
        y += step + GAP;
        divider(ui, r, y - 1.5 * GAP);

        // At the foot, where the thumb rests: undo and redo, and Done while a tool is open.
        let mut foot = r.bottom() - MARGIN - BUTTON_H;
        let redo = solidcraft_engine::find_command("edit.redo").is_some_and(|c| c.info(&app.session).enabled);
        if button(ui, slot(foot), "redo", "redo", app.tr_label("Redo"), false, redo && !busy).clicked() {
            let _ = app.run("edit.redo", serde_json::json!({}));
        }
        foot -= step;
        let undo = solidcraft_engine::find_command("edit.undo").is_some_and(|c| c.info(&app.session).enabled);
        if button(ui, slot(foot), "undo", "undo", app.tr_label("Undo"), false, undo && !busy).clicked() {
            let _ = app.run("edit.undo", serde_json::json!({}));
        }
        if app.tool.is_some() {
            foot -= step;
            if button(ui, slot(foot), "done", "finish", app.tr_label("Done"), false, true).clicked() {
                crate::tools::finish(app);
            }
        }
        let room = |y: f32| y + BUTTON_H <= foot - 2.0 * GAP;

        // The tab's panels, each opening its tools.
        for panel in panels(&tab) {
            let cmds = commands(&tab, panel);
            let Some(icon) = panel_icon(&cmds) else { continue };
            if !room(y) {
                break;
            }
            let on = matches!(app.touch_ui.flyout, Flyout::Panel { panel: p, .. } if p == panel);
            let label = app.tr_label(panel).to_string();
            if button(ui, slot(y), &format!("panel:{panel}"), icon, &label, on, !busy).clicked() {
                app.touch_ui.flyout = if on { Flyout::None } else { Flyout::Panel { panel, y } };
            }
            y += step;
        }
    });
    flyout(app, ui.ctx(), column_rect, right);
}

/// The open list beside the column: the tabs, or a panel's tools, in as many columns as the
/// window's height needs.
fn flyout(app: &mut SolveApp, ctx: &egui::Context, column: Rect, right: bool) {
    // (id, label, icon, enabled)
    let (rows, y): (Vec<(String, String, String, bool)>, f32) = match &app.touch_ui.flyout {
        Flyout::None => return,
        Flyout::Tabs { y } => {
            let mut rows: Vec<_> =
                crate::toolbar::TABS.iter().map(|tab| (format!("tab:{tab}"), app.tr_label(tab).to_string(), String::new(), true)).collect();
            // The other workspaces (Render), as the toolbar's switcher offers them.
            rows.push(("workspace".into(), app.tr("ui.change_workspace").to_string(), String::new(), true));
            (rows, *y)
        }
        Flyout::Panel { panel, y } => {
            let tab = tab(app);
            let rows = commands(&tab, panel)
                .iter()
                .map(|c| (c.id.to_string(), app.tr_label(c.label).to_string(), c.icon.to_string(), c.info(&app.session).enabled))
                .collect();
            (rows, *y)
        }
    };
    if rows.is_empty() || !column.is_positive() {
        app.touch_ui.flyout = Flyout::None;
        return;
    }
    let t = Tokens::get();
    let screen = ctx.content_rect();
    let fits = (((screen.height() - 3.0 * PAD) / ROW).floor().max(1.0)) as usize;
    let columns = rows.len().div_ceil(fits);
    let per_column = rows.len().div_ceil(columns);
    let size = vec2(columns as f32 * FLYOUT_W + 2.0 * PAD, per_column as f32 * ROW + 2.0 * PAD);
    let top = y.min(screen.bottom() - size.y - PAD).max(screen.top() + PAD);
    let left = if right { column.left() - size.x - GAP } else { column.right() + GAP };
    let frame = Rect::from_min_size(pos2(left, top), size);
    let mut chosen: Option<String> = None;
    egui::Area::new(egui::Id::new("sc_touch_flyout")).order(egui::Order::Foreground).fixed_pos(frame.min).show(ctx, |ui| {
        // The card takes the presses between its rows.
        ui.allocate_rect(frame, Sense::click());
        ui.painter().rect(frame, 10.0, t.panel, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        for (i, (id, label, icon, enabled)) in rows.iter().enumerate() {
            let at = pos2(left + PAD + (i / per_column) as f32 * FLYOUT_W, top + PAD + (i % per_column) as f32 * ROW);
            let r = Rect::from_min_size(at, vec2(FLYOUT_W, ROW)).shrink2(vec2(2.0, 1.0));
            let resp = ui.interact(r, ui.id().with(("touch_row", id)), if *enabled { Sense::click() } else { Sense::hover() });
            crate::scenario::publish_rect(&format!("touch:row:{id}"), r);
            crate::scenario::publish_handle(&format!("touch:row:{id}"), r.center());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, *enabled, label));
            let p = ui.painter();
            if *enabled && (resp.hovered() || resp.is_pointer_button_down_on()) {
                p.rect_filled(r, 6.0, t.hover);
            }
            let (ink, fill, text) = if *enabled { (t.icon, t.icon_fill, t.text) } else { (t.border, t.panel_header, t.text_dim) };
            let mut x = r.left() + 12.0;
            if !icon.is_empty() {
                let accent = if *enabled { crate::icon_svg::primary(icon, crate::theme::is_dark(), crate::theme::vision()) } else { t.border };
                icons::paint(p, Rect::from_center_size(pos2(x + ICON / 2.0, r.center().y), vec2(ICON, ICON)), icon, ink, fill, accent);
                x += ICON + 10.0;
            }
            let mut job = egui::text::LayoutJob::simple_singleline(label.clone(), FontId::proportional(14.0), text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(r.right() - x - 6.0);
            let galley = p.layout_job(job);
            p.galley(pos2(x, r.center().y - galley.size().y / 2.0), galley, text);
            if resp.clicked() {
                chosen = Some(id.clone());
            }
        }
    });
    let area_rect = frame;
    crate::scenario::publish_rect("touch:flyout", area_rect);
    if let Some(id) = chosen {
        app.touch_ui.flyout = Flyout::None;
        if let Some(tab) = id.strip_prefix("tab:") {
            app.ui.tab = tab.to_string();
        } else if id == "workspace" {
            crate::context_menu::open_for(app, frame.min, Target::Workspace);
        } else {
            app.start(&id);
        }
        return;
    }
    // A press anywhere but the list or the column closes it.
    let pressed = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.press_origin() } else { None });
    if let Some(p) = pressed
        && !frame.contains(p)
        && !column.contains(p)
    {
        app.touch_ui.flyout = Flyout::None;
    }
}

/// The selection's actions in a bar above the view's controls (`view`: the viewport, `above`:
/// the top of its view controls): the context menu's first actions, then More for the rest.
pub(crate) fn selection_bar(app: &mut SolveApp, ctx: &egui::Context, view: Rect, above: f32) {
    if !active(app) {
        return;
    }
    let items = selection_items(app);
    if items.is_empty() {
        return;
    }
    let t = Tokens::get();
    let busy = app.applying.is_some();
    let shown = items.len().min(SELECTION_ACTIONS);
    let width = (shown + 1) as f32 * (BUTTON_W + GAP) - GAP + 2.0 * PAD;
    let bar = Rect::from_min_size(pos2(view.center().x - width / 2.0, above - GAP - BUTTON_H - 2.0 * PAD), vec2(width, BUTTON_H + 2.0 * PAD));
    if bar.left() < view.left() || bar.top() < view.top() {
        return;
    }
    crate::scenario::publish_rect("touch:selection_bar", bar);
    let mut run: Option<(Item, egui::Pos2)> = None;
    let mut more: Option<egui::Pos2> = None;
    // Its own layer above the view, so a press on it never reaches the camera or the selection.
    egui::Area::new(egui::Id::new("sc_touch_selection_bar")).order(egui::Order::Foreground).fixed_pos(bar.min).show(ctx, |ui| {
        ui.allocate_rect(bar, Sense::click());
        ui.painter().rect(bar, 12.0, t.panel, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let slot = |i: usize| Rect::from_min_size(pos2(bar.left() + PAD + i as f32 * (BUTTON_W + GAP), bar.top() + PAD), vec2(BUTTON_W, BUTTON_H));
        for (i, item) in items.iter().take(shown).enumerate() {
            let label = app.tr_label(&item.label).to_string();
            if button(ui, slot(i), &format!("sel:{}", item.id), &item.icon, &label, false, !busy).clicked() {
                run = Some((item.clone(), slot(i).center_top()));
            }
        }
        let label = app.tr("ui.touch_more").to_string();
        if button(ui, slot(shown), "sel:more", "dots", &label, false, true).clicked() {
            more = Some(slot(shown).center_top());
        }
    });
    if let Some((item, at)) = run {
        app.touch_ui.flyout = Flyout::None;
        crate::context_menu::run_item(app, &item, at);
    } else if let Some(at) = more {
        app.touch_ui.flyout = Flyout::None;
        crate::context_menu::open_for(app, at, Target::Viewport);
    }
}
