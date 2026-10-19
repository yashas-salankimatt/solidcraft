//! Touch mode (`crate::touch_mode`) through the whole app, with fingers as a desktop touch screen
//! reports them (`touch_ui_tests`).

use serde_json::json;

use crate::scenario::{Harness, rect_of};
use crate::touch_ui_tests::{boxed, down, up};

/// A quick tap at `at`, then long enough for the next one not to make a double tap.
fn tap(h: &mut Harness, at: egui::Pos2) {
    h.input(down(1, at));
    h.frame();
    h.input(up(1, at));
    h.frames(30);
}

/// Tap the touch-mode button or row published as `name`.
fn tap_on(h: &mut Harness, name: &str) {
    let Some(r) = rect_of(name) else { panic!("`{name}` is not on screen") };
    tap(h, r.center());
}

fn touch_mode(h: &mut Harness, mode: &str) {
    h.call("ui.prefs", json!({ "touch_mode": mode }));
    h.frames(3);
}

#[test]
fn touch_mode_puts_the_tools_in_a_column_beside_the_model_on_either_edge() {
    let (mut h, _) = boxed();
    assert!(rect_of("toolbar").is_some() && rect_of("touch:column").is_none(), "off by default on a desktop");
    touch_mode(&mut h, "on");
    let column = rect_of("touch:column").expect("the column is shown");
    assert!(rect_of("toolbar").is_none(), "in place of the toolbar");
    let view = h.app.viewport.rect.expect("the view is shown");
    assert!(column.left() < 1.0 && view.left() >= column.right() - 1.0, "along the left edge: {column:?} {view:?}");
    assert!(view.top() < 60.0, "the model takes the window's height: {view:?}");
    assert_eq!(h.ctx.options(|o| o.input_options.max_click_dist), crate::touch_mode::CLICK_SLOP, "a finger may wander further");
    let undo = rect_of("touch:undo").expect("undo at the column's foot");
    assert!(undo.bottom() > column.bottom() - 60.0, "where the thumb rests: {undo:?} in {column:?}");

    h.call("ui.prefs", json!({ "touch_column_right": true }));
    h.frames(3);
    let column = rect_of("touch:column").expect("the column is shown");
    assert!(column.right() > h.ctx.content_rect().right() - 1.0, "along the right edge: {column:?}");

    touch_mode(&mut h, "off");
    assert!(rect_of("toolbar").is_some() && rect_of("touch:column").is_none());
    assert_eq!(h.ctx.options(|o| o.input_options.max_click_dist), 6.0);
}

#[test]
fn automatic_touch_mode_follows_whether_the_pointer_is_a_finger() {
    let mut h = Harness::new();
    touch_mode(&mut h, "auto");
    assert!(rect_of("touch:column").is_none());
    h.app.touch_first = true;
    h.frames(3);
    assert!(rect_of("touch:column").is_some(), "a tablet's browser gets touch mode");
    assert_eq!(serde_json::to_value(&h.app.preferences).unwrap()["touch_mode"], "auto");
    assert!(crate::prefs::control(&mut h.app, &json!({ "touch_mode": "sideways" })).is_err());
    assert_eq!(h.app.preferences.touch_mode, crate::touch_mode::TouchMode::Auto);
}

#[test]
fn a_group_opens_its_tools_and_a_tap_on_one_starts_it() {
    let mut h = Harness::new();
    touch_mode(&mut h, "on");
    tap_on(&mut h, "touch:panel:CREATE");
    assert!(rect_of("touch:flyout").is_some(), "the group's tools are listed");
    tap_on(&mut h, "touch:row:sketch.create");
    assert!(rect_of("touch:flyout").is_none(), "the list closed");
    assert!(h.app.dialog.as_ref().is_some_and(|d| matches!(d.kind, crate::dialogs::Kind::Sketch)), "Create Sketch started");

    // The tab button lists the tabs.
    h.app.dialog = None;
    h.frames(2);
    tap_on(&mut h, "touch:tab");
    tap_on(&mut h, "touch:row:tab:SURFACE");
    assert_eq!(h.app.ui.tab, "SURFACE");
    assert!(rect_of("touch:panel:CREATE").is_some());

    // A tap outside closes an open list.
    tap_on(&mut h, "touch:panel:MODIFY");
    assert!(rect_of("touch:flyout").is_some());
    let view = h.app.viewport.rect.expect("the view is shown");
    tap(&mut h, view.right_bottom() - egui::vec2(40.0, 40.0));
    assert!(rect_of("touch:flyout").is_none());
}

#[test]
fn taps_add_to_the_selection_and_a_tap_on_empty_space_clears_it() {
    let (mut h, _) = boxed();
    touch_mode(&mut h, "on");
    let view = h.app.viewport.rect.expect("the view is shown");
    let on_box = view.center();
    tap(&mut h, on_box);
    let first = h.app.session.selection.clone();
    assert!(!first.is_empty(), "the tap selected the face");
    // Tapping it again takes it out (with a mouse the same click would pick what lies under it).
    tap(&mut h, on_box);
    assert!(h.app.session.selection.is_empty(), "{:?}", h.app.session.selection);
    tap(&mut h, on_box);
    assert_eq!(h.app.session.selection, first);
    tap(&mut h, view.right_bottom() - egui::vec2(40.0, 40.0));
    assert!(h.app.session.selection.is_empty(), "a tap on empty space clears");
}

#[test]
fn the_selections_actions_float_above_the_views_controls() {
    let (mut h, _) = boxed();
    touch_mode(&mut h, "on");
    assert!(rect_of("touch:selection_bar").is_none(), "nothing selected, no actions");
    let view = h.app.viewport.rect.expect("the view is shown");
    tap(&mut h, view.center());
    let items = crate::context_menu::items(&h.app, &crate::context_menu::Target::Viewport);
    let first = items.iter().find(|i| i.enabled && !i.icon.is_empty() && i.id != "-" && !i.id.starts_with('#')).expect("an action");
    let action = rect_of(&format!("touch:sel:{}", first.id)).expect("the first action has a button");
    let bar = rect_of("touch:selection_bar").expect("the bar is shown");
    let controls = rect_of("nav:outer").expect("the view's controls are shown");
    assert!(view.contains_rect(bar) && bar.contains_rect(action) && bar.bottom() <= controls.top(), "{bar:?} above {controls:?} in {view:?}");
    // A tap on the bar runs the action, and never reaches the model under it.
    let selected = h.app.session.selection.clone();
    tap(&mut h, bar.center_top() + egui::vec2(0.0, 3.0));
    assert_eq!(h.app.session.selection, selected, "the tap between its buttons selected nothing");
    tap_on(&mut h, "touch:sel:more");
    assert!(h.app.viewport.context_menu.is_some(), "More opens the whole menu");
}

#[test]
fn undo_redo_and_done_sit_at_the_columns_foot() {
    let (mut h, _) = boxed();
    touch_mode(&mut h, "on");
    let features = |h: &Harness| h.app.session.doc.features.len();
    tap_on(&mut h, "touch:undo");
    assert_eq!(features(&h), 0);
    tap_on(&mut h, "touch:redo");
    assert_eq!(features(&h), 1);

    h.app.run("sketch.create", json!({ "plane": "XY" })).unwrap();
    h.frames(30);
    assert!(rect_of("touch:panel:CONSTRAINTS").is_some(), "a sketch shows its own tools");
    h.app.start("sketch.line");
    h.frames(3);
    assert!(h.app.tool.is_some());
    tap_on(&mut h, "touch:done");
    assert!(h.app.tool.is_none(), "Done ends the tool");
}
