//! Touch screens: what the fingers on a camera page are doing ([`Tracker`], read by
//! `navigation::handle`, the one place camera input is turned into motion), and the fixes raw
//! touch input needs before egui sees it ([`RawTouch`]).
//!
//! The gestures follow touch-first CAD conventions: one finger dragged from the model turns the
//! view (or pans it where turning is off), two fingers pan and pinch to zoom (and twist to roll,
//! when that is on), a quick two-finger tap opens the context menu, and a three-finger swipe
//! undoes (left) or redoes (right). A finger held still before it moves selects by a box instead.
//! A second finger always takes over from the first: whatever the first had started is let go.

use std::collections::BTreeMap;

use egui::{Event, Pos2, TouchPhase, Vec2, pos2};

/// How far a finger may move and still be a tap or a hold, in points.
pub(crate) const SLOP: f32 = 10.0;

/// How long a finger rests before a drag selects by a box instead of turning the view, in seconds.
pub(crate) const HOLD: f64 = 0.45;

/// Longest two-finger tap, in seconds.
const TAP: f64 = 0.35;

/// Shortest three-finger swipe, in points, and its longest duration in seconds.
const SWIPE: f32 = 60.0;
const SWIPE_TIME: f64 = 1.0;

/// More fingers than any gesture uses are ignored (a palm, a stray hand).
const MAX_FINGERS: usize = 10;

#[derive(Clone, Copy, Debug)]
struct Finger {
    start: Pos2,
    now: Pos2,
}

/// The fingers' centre, their spread (mean distance from the centre) and the angle of the line
/// from the first finger to the second.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    center: Pos2,
    spread: f32,
    angle: f32,
    count: usize,
}

/// A gesture that ended this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Gesture {
    /// Two fingers tapped together at this point.
    TwoFingerTap(Pos2),
    /// Three fingers swiped left (undo) or right (redo).
    SwipeLeft,
    SwipeRight,
}

/// What the fingers did in one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TouchFrame {
    /// Fingers down now.
    pub fingers: usize,
    /// Most fingers down at once since the first one landed (until all are up again).
    pub most: usize,
    /// A finger landed this frame on an otherwise empty screen.
    pub first_down: bool,
    /// With two or more fingers: how far their centre moved, the factor their spread changed
    /// by, and the angle they turned through (radians), since the last frame.
    pub pan: Vec2,
    pub zoom: f32,
    pub roll: f32,
    pub center: Option<Pos2>,
    /// The farthest any finger has moved from where it landed, in points.
    pub travel: f32,
    /// Seconds since the first finger landed.
    pub held: f64,
    pub ended: Option<Gesture>,
    /// The touch was cancelled by the system (an alert, a system gesture).
    pub cancelled: bool,
}

impl TouchFrame {
    fn idle() -> TouchFrame {
        TouchFrame {
            fingers: 0,
            most: 0,
            first_down: false,
            pan: Vec2::ZERO,
            zoom: 1.0,
            roll: 0.0,
            center: None,
            travel: 0.0,
            held: 0.0,
            ended: None,
            cancelled: false,
        }
    }

    /// Two or more fingers are, or were in this gesture, on the screen: the camera is theirs, and
    /// a single finger left behind does nothing until all are up.
    pub fn multi(&self) -> bool {
        self.most >= 2
    }
}

/// The fingers on the screen, followed through one gesture (first finger down to last one up).
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    fingers: BTreeMap<(u64, u64), Finger>,
    began: f64,
    most: usize,
    travel: f32,
    last: Option<Shape>,
    /// The fingers' centre when a third finger joined, for the swipe.
    swipe_from: Option<Pos2>,
    swipe_to: Option<Pos2>,
    /// The two fingers' centre, the last frame two were down (for a two-finger tap).
    two_at: Option<Pos2>,
}

impl Tracker {
    /// Follow this frame's touch `events` (at `time`, in seconds).
    pub fn update(&mut self, events: &[Event], time: f64) -> TouchFrame {
        let mut frame = TouchFrame::idle();
        let mut ended_now = false;
        for event in events {
            let Event::Touch { device_id, id, phase, pos, .. } = event else { continue };
            if !pos.is_finite() {
                continue;
            }
            let key = (device_id.0, id.0);
            match phase {
                TouchPhase::Start => {
                    if self.fingers.len() >= MAX_FINGERS {
                        continue;
                    }
                    if self.fingers.is_empty() {
                        self.began = time;
                        self.most = 0;
                        self.travel = 0.0;
                        self.swipe_from = None;
                        self.swipe_to = None;
                        self.two_at = None;
                        frame.first_down = true;
                    }
                    self.fingers.insert(key, Finger { start: *pos, now: *pos });
                    self.most = self.most.max(self.fingers.len());
                    // The shape is new: its deltas start from here, so a finger landing never jumps.
                    self.last = None;
                    if self.fingers.len() == 3 {
                        self.swipe_from = self.shape().map(|s| s.center);
                    }
                }
                TouchPhase::Move => {
                    if let Some(f) = self.fingers.get_mut(&key) {
                        f.now = *pos;
                        self.travel = self.travel.max(f.now.distance(f.start));
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if let Some(f) = self.fingers.get_mut(&key) {
                        f.now = *pos;
                    }
                    if self.fingers.len() == 3 {
                        self.swipe_to = self.shape().map(|s| s.center);
                    }
                    if self.fingers.remove(&key).is_some() {
                        self.last = None;
                        frame.cancelled |= *phase == TouchPhase::Cancel;
                        ended_now |= self.fingers.is_empty();
                    }
                }
            }
        }
        frame.fingers = self.fingers.len();
        // The gesture's count lasts through the frame its last finger lifts, then it is over.
        frame.most = if self.fingers.is_empty() && !ended_now { 0 } else { self.most };
        frame.travel = self.travel;
        frame.held = (time - self.began).max(0.0);
        if let Some(now) = self.shape() {
            frame.center = Some(now.center);
            if now.count >= 2
                && let Some(last) = self.last.filter(|l| l.count == now.count)
            {
                frame.pan = now.center - last.center;
                frame.zoom = if last.spread > 1.0 { (now.spread / last.spread).clamp(0.2, 5.0) } else { 1.0 };
                frame.roll = wrap(now.angle - last.angle);
            }
            if now.count == 2 {
                self.two_at = Some(now.center);
            }
            self.last = Some(now);
        }
        if ended_now {
            frame.ended = self.gesture(frame.held, frame.cancelled);
        }
        frame
    }

    /// The gesture a finished touch was, if any.
    fn gesture(&self, held: f64, cancelled: bool) -> Option<Gesture> {
        if cancelled {
            return None;
        }
        if self.most == 2 && held <= TAP && self.travel <= SLOP {
            return self.two_at.map(Gesture::TwoFingerTap);
        }
        if self.most == 3 && held <= SWIPE_TIME {
            let (from, to) = (self.swipe_from?, self.swipe_to?);
            let d = to - from;
            if d.x.abs() >= SWIPE && d.x.abs() > 2.0 * d.y.abs() {
                return Some(if d.x < 0.0 { Gesture::SwipeLeft } else { Gesture::SwipeRight });
            }
        }
        None
    }

    fn shape(&self) -> Option<Shape> {
        let count = self.fingers.len();
        if count == 0 {
            return None;
        }
        let sum = self.fingers.values().fold(Vec2::ZERO, |acc, f| acc + f.now.to_vec2());
        let center = (sum / count as f32).to_pos2();
        let spread = self.fingers.values().map(|f| f.now.distance(center)).sum::<f32>() / count as f32;
        let mut it = self.fingers.values();
        let angle = match (it.next(), it.next()) {
            (Some(a), Some(b)) => (b.now - a.now).angle(),
            _ => 0.0,
        };
        Some(Shape { center, spread, angle, count })
    }
}

/// An angle wrapped into (−π, π].
fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let a = a.rem_euclid(TAU);
    if a > PI { a - TAU } else { a }
}

/// Fixes raw touch input needs before egui sees it, the same on every platform:
/// - a finger's press arrives one frame after the finger is first seen, so every widget that
///   reacts to what is under the pointer (the move handles, the view cube, preselection) has had
///   a frame to see it, as it would have under a mouse;
/// - the pointer leaving (`PointerGone`) arrives one frame after a finger lifts, so the release
///   still has a position and a tap is a click there (winit reports both in the same frame);
/// - a cancelled touch also releases the button (the web platform reports only the cancel);
/// - pinch and rotate gestures the platform also reports (Safari) are dropped while two fingers
///   are down: the fingers themselves already zoom.
#[derive(Debug, Default)]
pub(crate) struct RawTouch {
    deferred: Vec<Event>,
    active: BTreeMap<(u64, u64), Pos2>,
}

impl RawTouch {
    /// Fix this frame's `events`. Returns whether events wait for the next frame (the caller asks
    /// for one). Platforms order a frame's events differently (winit reports the touch first,
    /// the browser its pointer events first), so a frame is looked at whole.
    pub fn normalize(&mut self, events: &mut Vec<Event>) -> bool {
        let incoming = std::mem::take(events);
        let started = incoming.iter().any(|e| matches!(e, Event::Touch { phase: TouchPhase::Start, .. }));
        let lifted = incoming.iter().any(|e| matches!(e, Event::Touch { phase: TouchPhase::End | TouchPhase::Cancel, .. }));
        // What waited from the last frame goes first, as it is: it has had its frame.
        let mut out = Vec::with_capacity(incoming.len() + self.deferred.len());
        for event in std::mem::take(&mut self.deferred) {
            self.follow(&event);
            out.push(event);
        }
        let mut it = incoming.into_iter();
        while let Some(event) = it.next() {
            match &event {
                Event::Touch { phase: TouchPhase::Cancel, pos, .. } => {
                    self.follow(&event);
                    let pos = if pos.is_finite() { *pos } else { pos2(0.0, 0.0) };
                    out.push(event);
                    if self.active.is_empty() {
                        out.push(Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                        out.push(Event::PointerGone);
                    }
                }
                Event::Touch { .. } => {
                    self.follow(&event);
                    out.push(event);
                }
                Event::PointerButton { pressed: true, pos, .. } if started => {
                    // The finger's press waits a frame, with the pointer already where it is.
                    out.push(Event::PointerMoved(*pos));
                    self.deferred.push(event);
                    self.deferred.extend(it.by_ref().filter(|e| !matches!(e, Event::Zoom(_) | Event::Rotate(_))));
                    break;
                }
                Event::PointerGone if lifted => self.deferred.push(event),
                Event::Zoom(_) | Event::Rotate(_) if self.active.len() >= 2 => {}
                _ => out.push(event),
            }
        }
        *events = out;
        !self.deferred.is_empty()
    }

    /// Follow the fingers a touch event reports.
    fn follow(&mut self, event: &Event) {
        if let Event::Touch { device_id, id, phase, pos, .. } = event {
            let key = (device_id.0, id.0);
            match phase {
                TouchPhase::Start | TouchPhase::Move => {
                    self.active.insert(key, *pos);
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    self.active.remove(&key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{PointerButton, TouchDeviceId, TouchId};

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos: pos2(x, y), force: None }
    }

    #[test]
    fn two_fingers_pan_pinch_and_twist_from_their_own_frame_without_jumps() {
        let mut t = Tracker::default();
        let f = t.update(&[touch(1, TouchPhase::Start, 100.0, 100.0)], 0.0);
        assert!(f.first_down && f.fingers == 1 && !f.multi());
        // A second finger landing moves nothing.
        let f = t.update(&[touch(2, TouchPhase::Start, 200.0, 100.0)], 0.02);
        assert_eq!((f.fingers, f.pan, f.zoom, f.roll), (2, Vec2::ZERO, 1.0, 0.0));
        assert!(f.multi());
        // Both move right: a pan.
        let f = t.update(&[touch(1, TouchPhase::Move, 110.0, 100.0), touch(2, TouchPhase::Move, 210.0, 100.0)], 0.04);
        assert!((f.pan - Vec2::new(10.0, 0.0)).length() < 1e-4 && (f.zoom - 1.0).abs() < 1e-5);
        // Spread apart: zoom 2×, centre still.
        let f = t.update(&[touch(1, TouchPhase::Move, 60.0, 100.0), touch(2, TouchPhase::Move, 260.0, 100.0)], 0.06);
        assert!((f.zoom - 2.0).abs() < 1e-4 && f.pan.length() < 1e-4, "{f:?}");
        // Turn a quarter: roll.
        let f = t.update(&[touch(1, TouchPhase::Move, 160.0, 0.0), touch(2, TouchPhase::Move, 160.0, 200.0)], 0.08);
        assert!((f.roll - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "{f:?}");
        // One finger lifts: the other moving alone is no pinch or pan, and the gesture stays multi.
        let f = t.update(&[touch(2, TouchPhase::End, 160.0, 200.0)], 0.1);
        assert_eq!((f.fingers, f.pan, f.zoom), (1, Vec2::ZERO, 1.0));
        let f = t.update(&[touch(1, TouchPhase::Move, 200.0, 50.0)], 0.12);
        assert!(f.multi() && f.pan == Vec2::ZERO);
        let f = t.update(&[touch(1, TouchPhase::End, 200.0, 50.0)], 0.14);
        assert_eq!((f.fingers, f.ended), (0, None), "a long two-finger gesture is no tap");
    }

    #[test]
    fn quick_still_two_finger_taps_and_three_finger_swipes_end_as_gestures() {
        let mut t = Tracker::default();
        t.update(&[touch(1, TouchPhase::Start, 100.0, 100.0), touch(2, TouchPhase::Start, 140.0, 100.0)], 0.0);
        let f = t.update(&[touch(1, TouchPhase::End, 101.0, 100.0), touch(2, TouchPhase::End, 141.0, 100.0)], 0.2);
        assert_eq!(f.ended, Some(Gesture::TwoFingerTap(pos2(120.0, 100.0))));
        for (dx, gesture) in [(-120.0, Gesture::SwipeLeft), (120.0, Gesture::SwipeRight)] {
            let mut t = Tracker::default();
            let start: Vec<Event> = (1..=3).map(|i| touch(i, TouchPhase::Start, 300.0, 100.0 + 40.0 * i as f32)).collect();
            t.update(&start, 0.0);
            let moved: Vec<Event> = (1..=3).map(|i| touch(i, TouchPhase::Move, 300.0 + dx, 100.0 + 40.0 * i as f32)).collect();
            t.update(&moved, 0.2);
            let up: Vec<Event> = (1..=3).map(|i| touch(i, TouchPhase::End, 300.0 + dx, 100.0 + 40.0 * i as f32)).collect();
            assert_eq!(t.update(&up, 0.3).ended, Some(gesture));
        }
        // Too short, too slow, too vertical, or cancelled: no gesture.
        for (dx, dy, time, cancel) in [(30.0, 0.0, 0.3, false), (120.0, 0.0, 1.5, false), (80.0, 70.0, 0.3, false), (120.0, 0.0, 0.3, true)] {
            let mut t = Tracker::default();
            t.update(&(1..=3).map(|i| touch(i, TouchPhase::Start, 300.0, 100.0 * i as f32)).collect::<Vec<_>>(), 0.0);
            t.update(&(1..=3).map(|i| touch(i, TouchPhase::Move, 300.0 + dx, 100.0 * i as f32 + dy)).collect::<Vec<_>>(), time * 0.5);
            let phase = if cancel { TouchPhase::Cancel } else { TouchPhase::End };
            let f = t.update(&(1..=3).map(|i| touch(i, phase, 300.0 + dx, 100.0 * i as f32 + dy)).collect::<Vec<_>>(), time);
            assert_eq!(f.ended, None, "{dx} {dy} {time} {cancel}");
        }
    }

    #[test]
    fn hostile_touch_input_is_ignored() {
        let mut t = Tracker::default();
        let f =
            t.update(&[touch(1, TouchPhase::Start, f32::NAN, 0.0), touch(9, TouchPhase::End, 1.0, 1.0), touch(9, TouchPhase::Move, 1.0, 1.0)], 0.0);
        assert_eq!(f, TouchFrame::idle());
        let many: Vec<Event> = (0..50).map(|i| touch(i, TouchPhase::Start, i as f32, 0.0)).collect();
        assert_eq!(t.update(&many, 0.0).fingers, MAX_FINGERS);
    }

    #[test]
    fn a_finger_press_waits_a_frame_and_a_lift_keeps_its_position_for_the_click() {
        let mut raw = RawTouch::default();
        let at = pos2(50.0, 60.0);
        let press = Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE };
        let mut events = vec![touch(1, TouchPhase::Start, 50.0, 60.0), Event::PointerMoved(at), press.clone()];
        assert!(raw.normalize(&mut events), "the press waits");
        assert_eq!(events, vec![touch(1, TouchPhase::Start, 50.0, 60.0), Event::PointerMoved(at), Event::PointerMoved(at)]);
        let mut next = vec![];
        assert!(!raw.normalize(&mut next));
        assert_eq!(next, vec![press]);
        let release = Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE };
        let mut up = vec![touch(1, TouchPhase::End, 50.0, 60.0), release.clone(), Event::PointerGone];
        assert!(raw.normalize(&mut up));
        assert_eq!(up, vec![touch(1, TouchPhase::End, 50.0, 60.0), release], "the release keeps its position");
        let mut next = vec![];
        raw.normalize(&mut next);
        assert_eq!(next, vec![Event::PointerGone]);
    }

    /// The browser reports the press before the touch, and the release and the pointer leaving
    /// before the lift: the same fixes apply, and a press waits only once.
    #[test]
    fn the_browsers_event_order_gets_the_same_fixes() {
        let mut raw = RawTouch::default();
        let at = pos2(50.0, 60.0);
        let press = Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE };
        let mut events = vec![press.clone(), touch(1, TouchPhase::Start, 50.0, 60.0)];
        assert!(raw.normalize(&mut events));
        assert_eq!(events, vec![Event::PointerMoved(at)]);
        let mut next = vec![];
        assert!(!raw.normalize(&mut next), "it waits only once");
        assert_eq!(next, vec![press, touch(1, TouchPhase::Start, 50.0, 60.0)]);
        let release = Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE };
        let mut up = vec![release.clone(), Event::PointerGone, touch(1, TouchPhase::End, 50.0, 60.0)];
        assert!(raw.normalize(&mut up));
        assert_eq!(up, vec![release, touch(1, TouchPhase::End, 50.0, 60.0)]);
        let mut next = vec![];
        raw.normalize(&mut next);
        assert_eq!(next, vec![Event::PointerGone]);
    }

    #[test]
    fn a_cancelled_touch_releases_and_platform_pinches_are_dropped_under_two_fingers() {
        let mut raw = RawTouch::default();
        let mut events = vec![touch(1, TouchPhase::Start, 1.0, 1.0), touch(2, TouchPhase::Start, 9.0, 9.0), Event::Zoom(1.5), Event::Rotate(0.2)];
        raw.normalize(&mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::Zoom(_) | Event::Rotate(_))));
        let mut events = vec![touch(1, TouchPhase::Cancel, 1.0, 1.0), touch(2, TouchPhase::Cancel, 9.0, 9.0)];
        raw.normalize(&mut events);
        assert!(events.iter().any(|e| matches!(e, Event::PointerButton { pressed: false, .. })), "{events:?}");
        assert!(events.contains(&Event::PointerGone));
        // A trackpad pinch (no fingers on a screen) still zooms.
        let mut events = vec![Event::Zoom(1.2)];
        raw.normalize(&mut events);
        assert_eq!(events, vec![Event::Zoom(1.2)]);
    }
}
