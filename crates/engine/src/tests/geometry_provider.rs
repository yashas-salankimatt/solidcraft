//! Features handed to a geometry provider when the kernel cannot do their Boolean
//! (`solidcraft_kernel::provider`, docs/geometry-providers.md).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use solidcraft_geom::Vec3;
use solidcraft_kernel::provider::{FallbackOutcome, GeometryProvider, ProviderReply, ProviderRequest, with_provider};

use super::*;

/// A provider that answers every request with one STEP text, counting the requests.
struct Answer {
    step: String,
    calls: AtomicUsize,
}

impl Answer {
    fn new(step: String) -> Arc<Self> {
        Arc::new(Answer { step, calls: AtomicUsize::new(0) })
    }
}

impl GeometryProvider for Answer {
    fn id(&self) -> &str {
        "test"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn run(&self, request: &ProviderRequest) -> std::result::Result<ProviderReply, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(request.operation.label(), "boolean union");
        Ok(ProviderReply { step: self.step.clone() })
    }
}

/// Two cylinders of one radius crossing at right angles, which the kernel cannot join (found by
/// the Boolean property tests): the first is body `A`, the second joins it.
const CROSSING: f64 = 12.5;

fn first(s: &mut Session) {
    run(s, "solid.cylinder", json!({"base": [5, 22.5, 5], "axis": "Y", "radius": CROSSING, "height": 12.5, "body_name": "A"}));
}

fn crossing() -> Value {
    json!({"base": [0, 27.5, 2.5], "axis": "X", "radius": CROSSING, "height": 5, "operation": "join", "targets": ["A"], "name": "Crossing"})
}

/// The join as another kernel would give it, standing in for one: the second cylinder 2 µm
/// thinner, which the kernel can join, and well inside what the checks allow for chords.
fn near_answer() -> String {
    let a = solidcraft_kernel::cylinder(Vec3::new(5., 22.5, 5.), Vec3::Y, CROSSING, 12.5).unwrap();
    let b = solidcraft_kernel::cylinder(Vec3::new(0., 27.5, 2.5), Vec3::X, CROSSING - 0.002, 5.).unwrap();
    let joined = solidcraft_kernel::boolean(&a, &b, solidcraft_kernel::BoolOp::Union).unwrap().unwrap();
    solidcraft_kernel::step_export(&[&joined], "test").unwrap()
}

#[test]
fn without_a_provider_the_crossing_join_fails_as_before() {
    with_provider(None, || {
        let mut s = Session::default();
        first(&mut s);
        let e = s.execute("solid.cylinder", &crossing()).unwrap_err().to_string();
        assert!(e.contains("boolean Union") && !e.contains("geometry provider"), "{e}");
    });
}

#[test]
fn a_provider_makes_the_join_the_kernel_cannot_and_says_so() {
    let provider = Answer::new(near_answer());
    let p: Arc<dyn GeometryProvider> = provider.clone();
    with_provider(Some(p), || {
        let mut s = Session::default();
        first(&mut s);
        let reply = run(&mut s, "solid.cylinder", crossing());
        let id = reply["feature"].as_u64().unwrap();
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
        let check = |s: &Session| {
            let r = s.model.result(id).unwrap();
            assert!(r.error.is_none(), "{r:?}");
            let warning = r.warning.clone().unwrap_or_default();
            assert!(warning.contains("Geometry provider: the boolean union was made by the test geometry provider"), "{warning}");
            assert!(warning.contains("named from geometry"), "{warning}");
            assert_eq!(r.geometry_fallbacks.len(), 1);
            let e = &r.geometry_fallbacks[0];
            assert_eq!((e.outcome, e.provider.as_str(), e.provider_version.as_str()), (FallbackOutcome::Accepted, "test", "1"));
            assert!(e.kernel_error.contains("no result") && e.provider_ms >= 0.0 && e.check_ms > 0.0, "{e:?}");
            r.geometry_fallbacks.clone()
        };
        let original = check(&s);
        assert!(reply["warning"].as_str().is_some_and(|w| w.contains("Geometry provider")), "{reply}");
        // One body, the two cylinders' volume less what they share: between the larger and the sum.
        let (va, vb) = (PI * CROSSING * CROSSING * 12.5, PI * CROSSING * CROSSING * 5.);
        let v = volume(&mut s);
        assert!(v > va.max(vb) && v < va + vb, "{v}");
        // Its faces carry no history: names from geometry, marked unverified.
        let faces = run(&mut s, "model.faces", json!({"body": "A"}));
        let faces = faces["faces"].as_array().unwrap();
        assert!(!faces.is_empty() && faces.iter().all(|f| f["exact"] == json!(false)), "{faces:?}");
        // The inspections say so, the design's and the feature's.
        let doc = run(&mut s, "document.inspect", json!({"measure": false}));
        let entry = doc["timeline"].as_array().unwrap().iter().find(|e| e["id"] == json!(id)).unwrap();
        assert_eq!(entry["geometry_fallbacks"], serde_json::to_value(&original).unwrap());
        let first_entry = &doc["timeline"][0];
        assert!(first_entry.get("geometry_fallbacks").is_none(), "a feature that needed none reads as before: {first_entry}");
        let inspected = run(&mut s, "timeline.inspect", json!({"feature": id}));
        assert_eq!(inspected["geometry_fallbacks"], serde_json::to_value(&original).unwrap());
        // Undo, redo and a refresh give the same result without asking again.
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(check(&s), original);
        s.refresh();
        assert_eq!(check(&s), original);
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1, "undo, redo and refresh reuse the result");
        // A reopened design asks again and gets the same.
        let reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert_eq!(check(&reopened).len(), 1);
        // Without the provider the same design fails as it would have: never a result kept from
        // the provider's evaluation.
        let doc = s.doc.to_json();
        with_provider(None, || {
            let without = Session::new(solidcraft_doc::Document::from_json(&doc).unwrap());
            let r = without.model.result(id).unwrap();
            assert!(r.error.as_ref().is_some_and(|e| e.contains("boolean Union")) && r.geometry_fallbacks.is_empty(), "{r:?}");
        });
    });
}

#[test]
fn a_wrong_answer_is_refused_and_the_join_fails_with_the_reason() {
    // The first cylinder alone: a join that lost the tool.
    let a = solidcraft_kernel::cylinder(Vec3::new(5., 22.5, 5.), Vec3::Y, CROSSING, 12.5).unwrap();
    let provider = Answer::new(solidcraft_kernel::step_export(&[&a], "test").unwrap());
    let p: Arc<dyn GeometryProvider> = provider.clone();
    with_provider(Some(p), || {
        let mut s = Session::default();
        first(&mut s);
        let e = s.execute("solid.cylinder", &crossing()).unwrap_err().to_string();
        assert!(e.contains("boolean Union") && e.contains("test geometry provider was tried, but its result was refused"), "{e}");
        assert_eq!(s.doc.features.len(), 1, "the failed feature is not kept");
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn a_provider_is_never_asked_for_what_the_kernel_can_do() {
    let provider = Answer::new(String::new());
    let p: Arc<dyn GeometryProvider> = provider.clone();
    with_provider(Some(p), || {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 20, "width": 10, "height": 10, "body_name": "Block"}));
        run(&mut s, "solid.cylinder", json!({"base": [10, 5, -1], "radius": 3, "height": 12, "operation": "cut", "targets": ["Block"]}));
        run(&mut s, "solid.box", json!({"corner": [15, 0, 0], "length": 10, "width": 10, "height": 4, "operation": "join", "targets": ["Block"]}));
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        let doc = run(&mut s, "document.inspect", json!({"measure": false}));
        assert!(doc["timeline"].as_array().unwrap().iter().all(|e| e.get("geometry_fallbacks").is_none() && e["warning"].is_null()), "{doc}");
    });
}
