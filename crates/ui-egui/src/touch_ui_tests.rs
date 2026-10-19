//! Touch gestures through the whole app (`crate::touch`, `navigation::handle`): fingers as a
//! desktop touch screen reports them (winit: the first finger drives the pointer, a lift sends the
//! release and the pointer leaving together).

use egui::{Event, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase, Vec2, vec2};
use serde_json::json;

use crate::scenario::Harness;

fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos, force: None }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

/// The first finger lands (it drives the pointer); a later finger only reports its touch.
pub(crate) fn down(id: u64, pos: Pos2) -> Vec<Event> {
    if id == 1 {
        vec![touch(id, TouchPhase::Start, pos), Event::PointerMoved(pos), button(pos, true)]
    } else {
        vec![touch(id, TouchPhase::Start, pos)]
    }
}

pub(crate) fn moved(id: u64, pos: Pos2) -> Vec<Event> {
    if id == 1 { vec![touch(id, TouchPhase::Move, pos), Event::PointerMoved(pos)] } else { vec![touch(id, TouchPhase::Move, pos)] }
}

pub(crate) fn up(id: u64, pos: Pos2) -> Vec<Event> {
    if id == 1 { vec![touch(id, TouchPhase::End, pos), button(pos, false), Event::PointerGone] } else { vec![touch(id, TouchPhase::End, pos)] }
}

/// A box in view, nothing selected; the viewport's centre is on it.
pub(crate) fn boxed() -> (Harness, egui::Rect) {
    let mut h = Harness::new();
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.app.fit_view();
    h.frames(30);
    h.app.session.selection.clear();
    h.frames(2);
    let rect = h.app.viewport.rect.unwrap();
    (h, rect)
}

/// One finger from `from` through `steps` moves of `step` each, then up.
fn drag(h: &mut Harness, from: Pos2, step: Vec2, steps: usize) -> Pos2 {
    h.input(down(1, from));
    h.frame();
    let mut at = from;
    for _ in 0..steps {
        at += step;
        h.input(moved(1, at));
    }
    h.input(up(1, at));
    h.frames(2);
    at
}

#[test]
fn a_tap_selects_what_is_under_the_finger_on_a_desktop_touch_screen() {
    let (mut h, rect) = boxed();
    let c = rect.center();
    h.input(down(1, c));
    h.frame();
    h.input(up(1, c));
    h.frames(2);
    assert!(!h.app.session.selection.is_empty(), "the tap selected the face under it");
}

#[test]
fn one_finger_dragged_from_the_model_turns_the_view_and_selects_nothing() {
    let (mut h, rect) = boxed();
    let before = h.app.cam;
    drag(&mut h, rect.center(), vec2(9.0, 3.0), 12);
    assert!((h.app.cam.yaw - before.yaw).abs() > 1e-3, "{} → {}", before.yaw, h.app.cam.yaw);
    assert!(h.app.session.selection.is_empty());
    assert!(h.app.viewport.boxsel.is_none() && h.app.viewport.primary_nav.is_none());
}

#[test]
fn a_finger_that_rests_first_selects_by_a_box_instead() {
    let (mut h, rect) = boxed();
    let before = h.app.cam;
    // From the bottom right up to the top left: a crossing box, which takes what it touches.
    let from = rect.max - vec2(60.0, 90.0);
    h.input(down(1, from));
    h.frames(35);
    let to = rect.min + vec2(40.0, 60.0);
    let step = (to - from) / 12.0;
    let mut at = from;
    for _ in 0..12 {
        at += step;
        h.input(moved(1, at));
    }
    assert!(h.app.viewport.boxsel.is_some(), "the drag became a box");
    h.input(up(1, at));
    h.frames(2);
    assert_eq!((h.app.cam.yaw, h.app.cam.pitch), (before.yaw, before.pitch), "the view did not turn");
    assert!(!h.app.session.selection.is_empty(), "the box selected the model it crossed");
}

#[test]
fn two_fingers_pinch_and_pan_the_view_and_select_nothing() {
    let (mut h, rect) = boxed();
    let c = rect.center();
    let (a, b) = (c - vec2(60.0, 0.0), c + vec2(60.0, 0.0));
    let before = h.app.cam;
    h.input(down(1, a));
    h.frame();
    h.input(down(2, b));
    // Spread apart: zoom in.
    for k in 1..=10 {
        let d = vec2(6.0 * k as f32, 0.0);
        let mut events = moved(1, a - d);
        events.extend(moved(2, b + d));
        h.input(events);
    }
    let zoomed = h.app.cam;
    assert!(zoomed.distance < before.distance * 0.9, "{} → {}", before.distance, zoomed.distance);
    // Both move together, down: pan.
    let (a, b) = (a - vec2(60.0, 0.0), b + vec2(60.0, 0.0));
    for k in 1..=10 {
        let d = vec2(0.0, 8.0 * k as f32);
        let mut events = moved(1, a + d);
        events.extend(moved(2, b + d));
        h.input(events);
    }
    let mut lift = up(2, b);
    lift.extend(up(1, a));
    h.input(lift);
    h.frames(2);
    assert!((h.app.cam.target - zoomed.target).len() > 1e-6, "the fingers moved together: the view panned");
    assert!(h.app.session.selection.is_empty(), "the first finger's press selected nothing");
    assert_eq!((h.app.cam.yaw, h.app.cam.pitch), (before.yaw, before.pitch), "two fingers don't turn the view (roll is off)");
    assert!(!h.app.viewport.touch_multi && h.app.viewport.primary_nav.is_none(), "the gesture ended with the fingers");
}

#[test]
fn a_second_finger_lets_go_of_the_first_fingers_box() {
    let (mut h, rect) = boxed();
    let from = rect.min + vec2(40.0, 60.0);
    h.input(down(1, from));
    h.frames(35);
    let mut at = from;
    for _ in 0..6 {
        at += vec2(30.0, 20.0);
        h.input(moved(1, at));
    }
    assert!(h.app.viewport.boxsel.is_some());
    h.input(down(2, rect.center()));
    h.frame();
    assert!(h.app.viewport.boxsel.is_none(), "the box is dropped");
    let mut lift = up(2, rect.center());
    lift.extend(up(1, at));
    h.input(lift);
    h.frames(2);
    assert!(h.app.session.selection.is_empty());
}

#[test]
fn a_three_finger_swipe_undoes_left_and_redoes_right() {
    let (mut h, rect) = boxed();
    let features = |h: &Harness| h.app.session.doc.features.len();
    assert_eq!(features(&h), 1);
    for (dx, expect) in [(-150.0, 0), (150.0, 1)] {
        let ys = [rect.center().y - 60.0, rect.center().y, rect.center().y + 60.0];
        let x = rect.center().x;
        let mut start = Vec::new();
        for (i, y) in ys.iter().enumerate() {
            start.extend(down(i as u64 + 1, egui::pos2(x, *y)));
        }
        h.input(start);
        h.frame();
        for k in 1..=6 {
            let mut events = Vec::new();
            for (i, y) in ys.iter().enumerate() {
                events.extend(moved(i as u64 + 1, egui::pos2(x + dx * k as f32 / 6.0, *y)));
            }
            h.input(events);
        }
        let mut lift = Vec::new();
        for (i, y) in ys.iter().enumerate().rev() {
            lift.extend(up(i as u64 + 1, egui::pos2(x + dx, *y)));
        }
        h.input(lift);
        h.frames(2);
        assert_eq!(features(&h), expect, "swipe {dx}");
    }
}

#[test]
fn a_quick_two_finger_tap_opens_the_context_menu() {
    let (mut h, rect) = boxed();
    let c = rect.center();
    let mut both = down(1, c - vec2(20.0, 0.0));
    both.extend(down(2, c + vec2(20.0, 0.0)));
    h.input(both);
    h.frame();
    let mut lift = up(2, c + vec2(20.0, 0.0));
    lift.extend(up(1, c - vec2(20.0, 0.0)));
    h.input(lift);
    h.frames(2);
    assert!(h.app.viewport.context_menu.is_some(), "the context menu opened");
    assert!(h.app.session.selection.len() <= 1, "at most what the menu is for is selected");
}

#[test]
fn in_a_sketch_one_finger_draws_with_the_tool_and_never_turns_the_view() {
    let (mut h, rect) = boxed();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.frames(60);
    h.app.start("sketch.rectangle.two_point");
    h.frames(3);
    let before = h.app.cam;
    let from = rect.center() - vec2(80.0, 50.0);
    drag(&mut h, from, vec2(12.0, 8.0), 12);
    assert_eq!((h.app.cam.yaw, h.app.cam.pitch, h.app.cam.target), (before.yaw, before.pitch, before.target), "the view stayed");
    let sketch = h.app.run("sketch.inspect", json!({})).unwrap();
    assert!(sketch["curves"].as_array().is_some_and(|c| c.len() >= 4), "the drag drew a rectangle: {sketch}");
}

#[test]
fn a_long_press_opens_the_context_menu() {
    let (mut h, rect) = boxed();
    let c = rect.center();
    h.input(down(1, c));
    h.frames(60);
    h.input(up(1, c));
    h.frames(2);
    assert!(h.app.viewport.context_menu.is_some(), "the long press opened the context menu");
}
