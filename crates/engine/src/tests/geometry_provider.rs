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

pub(crate) fn first(s: &mut Session) {
    run(s, "solid.cylinder", json!({"base": [5, 22.5, 5], "axis": "Y", "radius": CROSSING, "height": 12.5, "body_name": "A"}));
}

pub(crate) fn crossing() -> Value {
    json!({"base": [0, 27.5, 2.5], "axis": "X", "radius": CROSSING, "height": 5, "operation": "join", "targets": ["A"], "name": "Crossing"})
}

/// The join as another kernel would give it, standing in for one: the second cylinder 2 µm
/// thinner, which the kernel can join, and well inside what the checks allow for chords.
pub(crate) fn near_answer() -> String {
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

fn scratch(tag: &str) -> String {
    let d = std::env::temp_dir().join(format!("solidcraft-fallback-bundle-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d.to_string_lossy().into_owned()
}

/// The join the kernel cannot do, bundled without a provider (tried on a copy: the design keeps
/// one cylinder), holds its two operands and the request and nothing of the design; replayed,
/// it says what the kernel does with the operands and what a provider makes of them.
#[test]
fn a_failing_join_is_bundled_and_replays_without_the_design() {
    let folder = scratch("join");
    let index = with_provider(None, || {
        let mut s = Session::default();
        first(&mut s);
        let out = run(&mut s, "geometry.fallback_bundle", json!({"command": "solid.cylinder", "params": crossing(), "folder": folder}));
        assert_eq!(s.doc.features.len(), 1, "the design is left as it is");
        assert_eq!(out["requests"], json!(1), "{out}");
        out["bundle"].clone()
    });
    assert_eq!(index["format"], json!("solidcraft-geometry-fallback/1"));
    assert!(index["error"].as_str().is_some_and(|e| e.contains("boolean Union")), "{index}");
    assert_eq!(index["requests"][0]["operation"], json!("boolean union"));
    let dir = std::path::Path::new(&folder);
    let mut files: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(
        files,
        ["bundle.json", "request-1-target.scbrep", "request-1-target.step", "request-1-tool.scbrep", "request-1-tool.step", "request-1.json"]
    );
    // The request is the line a provider program reads.
    let line = std::fs::read_to_string(dir.join("request-1.json")).unwrap();
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        (request["protocol"].clone(), request["op"].clone(), request["kind"].clone()),
        (json!("solidcraft-geometry-provider/1"), json!("boolean"), json!("union"))
    );
    assert!(!line.contains("\"A\"") && !std::fs::read_to_string(dir.join("bundle.json")).unwrap().contains("sketch"), "nothing of the design");
    // Replayed without a provider: the kernel on the very bodies it failed on.
    let plain = with_provider(None, || {
        let mut s = Session::default();
        run(&mut s, "geometry.fallback_replay", json!({"folder": folder}))
    });
    let replayed = &plain["replays"][0];
    assert_eq!((replayed["operation"].clone(), replayed["operands_from"].clone()), (json!("boolean union"), json!("native")));
    assert!(replayed["kernel_error"].as_str().is_some_and(|e| e.contains("boolean Union")), "the kernel fails on them again: {plain}");
    assert!(replayed.get("fallback").is_none());
    // With a provider the replay asks it and checks its answer as an evaluation does.
    let p: Arc<dyn GeometryProvider> = Answer::new(near_answer());
    let with = with_provider(Some(p), || {
        let mut s = Session::default();
        run(&mut s, "geometry.fallback_replay", json!({"folder": folder}))
    });
    assert_eq!(with["replays"][0]["fallback"]["outcome"], json!("accepted"), "{with}");
    assert_eq!(with["provider"]["provider"], json!("test"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_feature_bundle_holds_only_its_own_operations() {
    let folder = scratch("feature");
    let p: Arc<dyn GeometryProvider> = Answer::new(near_answer());
    with_provider(Some(p), || {
        let mut s = Session::default();
        first(&mut s);
        run(&mut s, "solid.cylinder", crossing());
        run(
            &mut s,
            "solid.box",
            json!({"corner": [-5, 25, -12], "length": 10, "width": 5, "height": 5, "operation": "join", "targets": ["A"], "name": "Lug"}),
        );
        let out = run(&mut s, "geometry.fallback_bundle", json!({"feature": "Crossing", "folder": folder}));
        assert_eq!(out["requests"], json!(1), "{out}");
        assert_eq!(out["bundle"]["source"]["feature"]["name"], json!("Crossing"));
        assert_eq!(out["bundle"]["provider"]["provider"], json!("test"));
        let none = s.execute("geometry.fallback_bundle", &json!({"feature": "Lug", "folder": folder})).unwrap_err().to_string();
        assert!(none.contains("nothing to bundle"), "{none}");
        for refused in
            [json!({"command": "file.save", "params": {"path": "/tmp/x.solidcraft"}}), json!({"command": "geometry.fallback_bundle"}), json!({})]
        {
            assert!(s.execute("geometry.fallback_bundle", &refused).is_err(), "{refused}");
        }
        let missing = s.execute("geometry.fallback_replay", &json!({"folder": scratch("missing")})).unwrap_err().to_string();
        assert!(missing.contains("bundle.json"), "{missing}");
    });
    let _ = std::fs::remove_dir_all(&folder);
}
