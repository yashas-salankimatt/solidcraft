//! Alternate geometry providers (`docs/geometry-providers.md`): another kernel an operation
//! may hand over to when SolidCraft's own fails with a geometry error. Today that is a
//! Boolean of two solids.
//!
//! A provider is only a trait here, taking and returning STEP text, so nothing of it is linked
//! into this crate: the engine runs it as a separate program, and the browser build has none.
//! With no provider installed every operation is exactly what it was.
//!
//! The kernel's own result always comes first. A provider's result is kept only once it passes
//! checks that do not rely on the kernel that just failed (`check`): one closed, sound solid
//! whose volume a join, cut or intersection of these operands can have, nothing changed away
//! from the tool, and its surface where the operands' surfaces say it must be. Every attempt is
//! recorded ([`FallbackEvent`]), so a result a provider made is never passed off as the
//! kernel's. Its faces carry no history: they are named from geometry (unverified).

use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use solidcraft_geom::{Mesh, Vec3};

use crate::body::Body;
use crate::boolean_guard::Near;
use crate::{BoolOp, KernelError, Result};

/// Another kernel that can take over an operation SolidCraft's kernel could not do.
pub trait GeometryProvider: Send + Sync {
    /// Short name shown in diagnostics (`occt`).
    fn id(&self) -> &str;
    /// The provider's own version, for diagnostics and replay bundles.
    fn version(&self) -> &str;
    /// Do one operation. An error is the provider's own message.
    fn run(&self, request: &ProviderRequest) -> std::result::Result<ProviderReply, String>;
}

/// An operation a provider is asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProviderOperation {
    /// A Boolean of the two operands: the first is the target, the second the tool.
    Boolean { kind: BoolOp },
}

impl ProviderOperation {
    /// The operation in words (`boolean cut`).
    pub fn label(&self) -> String {
        match self {
            ProviderOperation::Boolean { kind } => {
                let kind = match kind {
                    BoolOp::Union => "union",
                    BoolOp::Cut => "cut",
                    BoolOp::Intersect => "intersect",
                };
                format!("boolean {kind}")
            }
        }
    }
}

/// What a provider is asked: an operation on solids written as STEP text, in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequest {
    #[serde(flatten)]
    pub operation: ProviderOperation,
    /// One STEP file per operand, each holding one solid (of one or more pieces).
    pub operands: Vec<String>,
    /// The distance under which two points are the same, in millimetres.
    pub tolerance_mm: f64,
}

/// A provider's answer: the result as STEP text, whose solids are the result's pieces.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderReply {
    pub step: String,
}

static INSTALLED: RwLock<Option<Arc<dyn GeometryProvider>>> = RwLock::new(None);

thread_local! {
    /// The provider for this thread only, in place of the installed one (`Some(None)`: none).
    static OVERRIDE: RefCell<Option<Option<Arc<dyn GeometryProvider>>>> = const { RefCell::new(None) };
}

/// Install the provider this process uses (`None`: none). The application does this once at
/// start-up, from its settings.
pub fn install(provider: Option<Arc<dyn GeometryProvider>>) {
    if let Ok(mut installed) = INSTALLED.write() {
        *installed = provider;
    }
}

/// Run `f` with `provider` in place of the installed one on this thread (`None`: none).
pub fn with_provider<R>(provider: Option<Arc<dyn GeometryProvider>>, f: impl FnOnce() -> R) -> R {
    let previous = OVERRIDE.with(|o| o.replace(Some(provider)));
    struct Restore(Option<Option<Option<Arc<dyn GeometryProvider>>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                OVERRIDE.with(|o| *o.borrow_mut() = previous);
            }
        }
    }
    let _restore = Restore(Some(previous));
    f()
}

/// The provider operations on this thread hand over to, if any.
pub fn current() -> Option<Arc<dyn GeometryProvider>> {
    match OVERRIDE.with(|o| o.borrow().clone()) {
        Some(provider) => provider,
        None => INSTALLED.read().ok().and_then(|p| p.clone()),
    }
}

/// A digest of the current provider's id and version, 0 when there is none: a result evaluated
/// with one provider is never taken for one evaluated without it.
pub fn digest() -> u64 {
    use std::hash::{Hash, Hasher};
    current().map_or(0, |p| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (p.id(), p.version()).hash(&mut h);
        h.finish() | 1
    })
}

/// What became of an operation handed to a provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackOutcome {
    /// Its result passed the checks and was kept.
    Accepted,
    /// It gave a result that failed the checks: the operation failed as it did before.
    Refused,
    /// It gave no result (its own error, or it could not be reached).
    Failed,
}

/// One operation handed to a provider.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FallbackEvent {
    /// The operation (`boolean cut`).
    pub operation: String,
    /// Why SolidCraft's kernel could not do it.
    pub kernel_error: String,
    pub provider: String,
    pub provider_version: String,
    pub outcome: FallbackOutcome,
    /// Why the result was refused, or the provider's error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Time in SolidCraft's kernel before the provider was asked.
    pub kernel_ms: f64,
    /// Time the provider took, writing the operands and reading its result included.
    pub provider_ms: f64,
    /// Time checking the provider's result.
    pub check_ms: f64,
}

impl FallbackEvent {
    /// The event in words, for a feature's warning or error.
    pub fn description(&self) -> String {
        let FallbackEvent { operation, kernel_error, provider, .. } = self;
        let kernel_error = kernel_error.trim_start_matches("the operation failed: ");
        match self.outcome {
            FallbackOutcome::Accepted => format!(
                "the {operation} was made by the {provider} geometry provider in {:.0} ms because SolidCraft's kernel could not do it \
                 ({kernel_error}); its faces are named from geometry, so check features that refer to them after an edit",
                self.provider_ms
            ),
            FallbackOutcome::Refused => {
                format!("the {provider} geometry provider was tried, but its result was refused: {}", self.reason.as_deref().unwrap_or("no reason"))
            }
            FallbackOutcome::Failed => {
                format!("the {provider} geometry provider was tried and failed: {}", self.reason.as_deref().unwrap_or("no reason"))
            }
        }
    }
}

/// The note a feature carries for the operations a provider made for it: none when it made none.
pub fn warning(events: &[FallbackEvent]) -> Option<String> {
    let accepted: Vec<&FallbackEvent> = events.iter().filter(|e| e.outcome == FallbackOutcome::Accepted).collect();
    match accepted.as_slice() {
        [] => None,
        [one] => Some(format!("Geometry provider: {}", one.description())),
        [first, ..] => Some(format!(
            "Geometry provider: {} operations were made by the {} geometry provider because SolidCraft's kernel could not do them \
             (first: {}); their faces are named from geometry, so check features that refer to them after an edit",
            accepted.len(),
            first.provider,
            first.kernel_error.trim_start_matches("the operation failed: ")
        )),
    }
}

/// Events kept per capture: a feature hands over a handful of operations at most.
const MAX_EVENTS: usize = 64;

thread_local! {
    static REPORT: RefCell<Option<Vec<FallbackEvent>>> = const { RefCell::new(None) };
}

/// Run `f`, and the operations it handed to a provider on this thread. Captures nest: an inner
/// capture's events are its own.
pub fn capture_fallbacks<T>(f: impl FnOnce() -> T) -> (T, Vec<FallbackEvent>) {
    let previous = REPORT.with(|cell| cell.try_borrow_mut().ok().map(|mut report| report.replace(Vec::new())));
    struct Restore(Option<Option<Vec<FallbackEvent>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                REPORT.with(|cell| {
                    if let Ok(mut report) = cell.try_borrow_mut() {
                        *report = previous;
                    }
                });
            }
        }
    }
    let restore = Restore(previous);
    let r = f();
    let events = REPORT.with(|cell| cell.try_borrow_mut().ok().and_then(|mut report| report.take())).unwrap_or_default();
    drop(restore);
    (r, events)
}

fn record(event: FallbackEvent) {
    REPORT.with(|cell| {
        if let Ok(mut report) = cell.try_borrow_mut()
            && let Some(report) = report.as_mut()
            && report.len() < MAX_EVENTS
        {
            report.push(event);
        }
    });
}

/// [`crate::boolean`], handed to the current provider when SolidCraft's kernel fails with a
/// geometry error (see the module docs). Exactly [`crate::boolean`] when there is no provider.
pub fn boolean_with_fallback(a: &Body, b: &Body, op: BoolOp) -> Result<Option<Body>> {
    let Some(provider) = current() else { return crate::boolean(a, b, op) };
    crate::profile::timed(crate::profile::Kind::Boolean, || {
        let started = web_time::Instant::now();
        let kernel = crate::boolean(a, b, op);
        fall_back(a, b, op, kernel, ms(started), provider.as_ref())
    })
}

fn ms(since: web_time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The kernel's result of a Boolean, or `provider`'s when the kernel failed with a geometry
/// error and the provider's result passes the checks.
fn fall_back(a: &Body, b: &Body, op: BoolOp, kernel: Result<Option<Body>>, kernel_ms: f64, provider: &dyn GeometryProvider) -> Result<Option<Body>> {
    // Bad input (a mesh, a refused shape) is no geometry failure, and a provider gets solids only.
    let error = match kernel {
        Err(e @ (KernelError::Failed(_) | KernelError::Internal(_))) if !a.is_mesh() && !b.is_mesh() => e,
        other => return other,
    };
    let operation = ProviderOperation::Boolean { kind: op };
    let mut event = FallbackEvent {
        operation: operation.label(),
        kernel_error: error.to_string(),
        provider: provider.id().to_string(),
        provider_version: provider.version().to_string(),
        outcome: FallbackOutcome::Failed,
        reason: None,
        kernel_ms,
        provider_ms: 0.0,
        check_ms: 0.0,
    };
    let asked = web_time::Instant::now();
    let reply =
        request(a, b, operation).and_then(|request| crate::guard("geometry provider", || provider.run(&request).map_err(KernelError::Failed)));
    event.provider_ms = ms(asked);
    let result = match reply {
        Err(e) => {
            event.reason = Some(e.to_string().trim_start_matches("the operation failed: ").to_string());
            None
        }
        Ok(reply) => {
            let checking = web_time::Instant::now();
            let checked = crate::guard("geometry provider result", || Ok(accept(a, b, op, &reply.step))).unwrap_or_else(|e| Err(e.to_string()));
            event.check_ms = ms(checking);
            match checked {
                Ok(body) => {
                    event.outcome = FallbackOutcome::Accepted;
                    Some(body)
                }
                Err(reason) => {
                    event.outcome = FallbackOutcome::Refused;
                    event.reason = Some(reason);
                    None
                }
            }
        }
    };
    let note = event.description();
    record(event);
    match result {
        Some(body) => Ok(Some(body)),
        None => Err(match error {
            KernelError::Failed(m) => KernelError::Failed(format!("{m}; {note}")),
            KernelError::Internal(m) => KernelError::Internal(format!("{m}; {note}")),
            other => other,
        }),
    }
}

/// The request for `operation` on `a` and `b`.
fn request(a: &Body, b: &Body, operation: ProviderOperation) -> Result<ProviderRequest> {
    let step = |body: &Body| {
        crate::step_export_bodies(&[crate::ExportBody { name: "Operand".into(), body, color: None }], &crate::StepHeader::default())
            .map_err(|e| KernelError::Failed(format!("the operands could not be written as STEP: {e}")))
    };
    let tolerance_mm = (a.size().max(b.size()) * 1e-7).max(1e-7);
    Ok(ProviderRequest { operation, operands: vec![step(a)?, step(b)?], tolerance_mm })
}

/// Most pieces a provider's result may have.
const MAX_PIECES: usize = 4096;

/// The provider's result as a body, if it passes the checks; else why not.
fn accept(a: &Body, b: &Body, op: BoolOp, step: &str) -> std::result::Result<Body, String> {
    let import = crate::step_import(step).map_err(|e| format!("its STEP could not be read: {e}"))?;
    if import.bodies.len() > MAX_PIECES {
        return Err(format!("it has {} pieces (at most {MAX_PIECES})", import.bodies.len()));
    }
    if import.bodies.iter().any(|piece| !piece.closed) {
        return Err("it is not a closed solid".into());
    }
    let pieces = import.bodies.into_iter().map(|piece| piece.body).collect();
    let body = crate::ops::join_pieces(pieces).map_err(|e| e.to_string())?.ok_or("it has no solid")?.without_history();
    let problems = body.validity();
    if !problems.is_empty() {
        return Err(format!("it is not a sound solid: {}", problems.join("; ")));
    }
    check(a, b, op, &body)?;
    Ok(body)
}

/// Whether `r` can be `op` of `a` and `b`, judged without the kernel's Boolean: the checks a
/// retry's result must pass (`boolean_guard`; an intersection's volume and box instead), then
/// where its surface lies (`surface_check`).
fn check(a: &Body, b: &Body, op: BoolOp, r: &Body) -> std::result::Result<(), String> {
    let refused = |e: KernelError| e.to_string().trim_start_matches("the operation failed: ").to_string();
    match op {
        BoolOp::Intersect => intersection_bounds(a, b, r)?,
        _ => crate::boolean_guard::check_retry(a, b, op, Some(r)).map_err(refused)?,
    }
    surface_check(a, b, op, r)
}

/// The chord tolerance the checks mesh the bodies with.
fn check_tolerance(a: &Body, b: &Body) -> f64 {
    (a.size().max(b.size()) * 2e-4).max(1e-5)
}

fn meshes(a: &Body, b: &Body, r: &Body) -> std::result::Result<[Mesh; 3], String> {
    let t = check_tolerance(a, b);
    let mesh = |body: &Body| body.tessellate_with(t, true).map_err(|e| format!("it could not be checked: {e}"));
    Ok([mesh(a)?, mesh(b)?, mesh(r)?])
}

/// An intersection is no bigger than either operand and lies within both their boxes.
fn intersection_bounds(a: &Body, b: &Body, r: &Body) -> std::result::Result<(), String> {
    let [ma, mb, mr] = meshes(a, b, r)?;
    let (va, vb, vr) = (ma.measure().volume, mb.measure().volume, mr.measure().volume);
    let slack = 2e-4 * (va.abs() + vb.abs()) + 1e-9;
    if !vr.is_finite() || vr > va.min(vb) + slack {
        return Err(format!("the intersection's volume would be {vr:.4} mm³ (the operands are {va:.4} mm³ and {vb:.4} mm³)"));
    }
    let tol = 2.0 * check_tolerance(a, b);
    let (ba, bb, br) = (ma.bounds(), mb.bounds(), mr.bounds());
    let within = |lo: f64, hi: f64, a: (f64, f64), b: (f64, f64)| lo >= a.0.max(b.0) - tol && hi <= a.1.min(b.1) + tol;
    if !(within(br.min.x, br.max.x, (ba.min.x, ba.max.x), (bb.min.x, bb.max.x))
        && within(br.min.y, br.max.y, (ba.min.y, ba.max.y), (bb.min.y, bb.max.y))
        && within(br.min.z, br.max.z, (ba.min.z, ba.max.z), (bb.min.z, bb.max.z)))
    {
        return Err("the intersection reaches outside an operand".into());
    }
    Ok(())
}

/// About how many points are sampled from each surface.
const SURFACE_SAMPLES: usize = 2000;

/// Points spread over a mesh by area, so a flat face of two triangles is sampled as densely as
/// a round one of hundreds: a triangle owed `n` points gives the centres of the `m × m ≤ n`
/// triangles it divides into, and what a small one is owed carries over to the next.
fn samples(mesh: &Mesh) -> Vec<Vec3> {
    let triangles: Vec<[Vec3; 3]> = mesh.triangles.iter().filter_map(|t| mesh.tri(t)).collect();
    let area = |[a, b, c]: &[Vec3; 3]| (*b - *a).cross(*c - *a).len() * 0.5;
    let total: f64 = triangles.iter().map(area).sum();
    if !(total.is_finite() && total > 0.0) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(SURFACE_SAMPLES * 2);
    let mut owed = 0.0;
    for t @ [a, b, c] in &triangles {
        owed += area(t) / total * SURFACE_SAMPLES as f64;
        if owed < 1.0 {
            continue;
        }
        let m = (owed.sqrt().floor() as usize).clamp(1, 64);
        owed -= (m * m) as f64;
        let point = |u: f64, v: f64| *a + (*b - *a) * (u / m as f64) + (*c - *a) * (v / m as f64);
        for i in 0..m {
            for j in 0..m - i {
                out.push(point(i as f64 + 1.0 / 3.0, j as f64 + 1.0 / 3.0));
                if i + j + 1 < m {
                    out.push(point(i as f64 + 2.0 / 3.0, j as f64 + 2.0 / 3.0));
                }
            }
        }
    }
    out
}

/// The result's surface lies where the operands' surfaces say it must: every sampled point of
/// it on an operand's surface, and every sampled point of an operand's surface that the
/// operation keeps on it. Which points are kept follows from whether they lie in the other
/// operand: a join keeps each surface outside the other, a cut keeps the target's surface
/// outside the tool and the tool's inside the target, an intersection each inside the other.
/// Points near the other operand's surface say nothing and are left out. A result with faces
/// moved, dropped or wrongly kept fails here even when its volume is plausible.
fn surface_check(a: &Body, b: &Body, op: BoolOp, r: &Body) -> std::result::Result<(), String> {
    let [ma, mb, mr] = meshes(a, b, r)?;
    let reach = 4.0 * check_tolerance(a, b);
    let unchecked = || "it could not be checked: a surface has no triangles".to_string();
    let (na, nb, nr) =
        (Near::new(&ma, reach).ok_or_else(unchecked)?, Near::new(&mb, reach).ok_or_else(unchecked)?, Near::new(&mr, reach).ok_or_else(unchecked)?);
    let (inside_a, inside_b) = (ma.inside_index(), mb.inside_index());
    // A few stray samples are the meshes' chords where they curve; a wrong result misses whole faces.
    let too_many = |bad: usize, of: usize| bad > of / 200 + 2;
    let on_result = samples(&mr);
    let (of, stray) = (on_result.len(), on_result.iter().filter(|p| !na.close(**p) && !nb.close(**p)).count());
    if too_many(stray, of) {
        return Err(format!("{stray} of {of} points sampled on its surface lie on neither operand's surface"));
    }
    let keeps_a = |in_b: bool| match op {
        BoolOp::Union | BoolOp::Cut => !in_b,
        BoolOp::Intersect => in_b,
    };
    let keeps_b = |in_a: bool| match op {
        BoolOp::Union => !in_a,
        BoolOp::Cut | BoolOp::Intersect => in_a,
    };
    for (name, mesh, other_surface, other, keeps) in [
        ("the target's", &ma, &nb, &inside_b, &keeps_a as &dyn Fn(bool) -> bool),
        ("the tool's", &mb, &na, &inside_a, &keeps_b as &dyn Fn(bool) -> bool),
    ] {
        let (mut of, mut missing) = (0, 0);
        for p in samples(mesh).into_iter().filter(|p| !other_surface.close(*p)) {
            if keeps(other.contains(p)) {
                of += 1;
                if !nr.close(p) {
                    missing += 1;
                }
            }
        }
        if too_many(missing, of) {
            return Err(format!("{missing} of {of} points sampled on {name} surface that the result must keep are not on its surface"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcraft_geom::Vec3;

    /// A provider that answers with a fixed STEP text (or error), counting its calls.
    struct Fixed {
        reply: std::result::Result<String, String>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl Fixed {
        fn new(reply: std::result::Result<String, String>) -> Arc<Self> {
            Arc::new(Fixed { reply, calls: Default::default() })
        }
        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    impl GeometryProvider for Fixed {
        fn id(&self) -> &str {
            "fixed"
        }
        fn version(&self) -> &str {
            "1"
        }
        fn run(&self, request: &ProviderRequest) -> std::result::Result<ProviderReply, String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(request.operands.len(), 2);
            assert!(request.operands.iter().all(|s| s.starts_with("ISO-10303-21;")));
            self.reply.clone().map(|step| ProviderReply { step })
        }
    }

    fn cube(min: Vec3, max: Vec3) -> Body {
        crate::box_solid(min, max).unwrap()
    }

    fn step(body: &Body) -> String {
        crate::step_export(&[body], "test").unwrap()
    }

    /// A block and a tool through one end of it, and the right answer of each operation.
    fn operands() -> (Body, Body) {
        (cube(Vec3::ZERO, Vec3::new(20., 10., 10.)), cube(Vec3::new(15., -5., -5.), Vec3::new(30., 15., 15.)))
    }

    fn failed() -> Result<Option<Body>> {
        Err(KernelError::Failed("boolean Cut: no result".into()))
    }

    fn handed_over(op: BoolOp, provider: &Fixed) -> (Result<Option<Body>>, Vec<FallbackEvent>) {
        let (a, b) = operands();
        let kernel = if op == BoolOp::Cut { failed() } else { Err(KernelError::Failed(format!("boolean {op:?}: no result"))) };
        capture_fallbacks(|| fall_back(&a, &b, op, kernel, 1.0, provider))
    }

    #[test]
    fn a_right_result_is_kept_without_history_and_recorded() {
        let (a, b) = operands();
        for op in [BoolOp::Union, BoolOp::Cut, BoolOp::Intersect] {
            let right = crate::boolean(&a, &b, op).unwrap().unwrap();
            let provider = Fixed::new(Ok(step(&right)));
            let (r, events) = handed_over(op, &provider);
            let body = r.unwrap().unwrap();
            let (got, want) = (crate::measure(&body).unwrap().volume, crate::measure(&right).unwrap().volume);
            assert!((got - want).abs() < 1e-6 * want, "{op:?}: {got} vs {want}");
            assert!(body.history().is_none(), "{op:?}: a provider's faces have no history");
            assert_eq!(events.len(), 1);
            let e = &events[0];
            assert_eq!((e.outcome, e.provider.as_str(), e.reason.as_deref()), (FallbackOutcome::Accepted, "fixed", None), "{op:?}");
            assert!(e.kernel_error.contains("no result"));
            assert!(e.description().contains("named from geometry"), "{}", e.description());
        }
    }

    /// Round faces mesh as chords: a right result through STEP is not refused for them, and a
    /// hole drilled in the wrong place is.
    #[test]
    fn round_results_are_judged_by_their_surfaces() {
        let block = cube(Vec3::ZERO, Vec3::new(40., 30., 10.));
        let drill = |x: f64| crate::cylinder(Vec3::new(x, 15., -5.), Vec3::Z, 6.0, 20.0).unwrap();
        let ball = crate::sphere(Vec3::new(40., 30., 10.), 9.0).unwrap();
        for (tool, op) in [(drill(20.), BoolOp::Cut), (drill(20.), BoolOp::Union), (ball.clone(), BoolOp::Cut), (ball, BoolOp::Union)] {
            let right = crate::boolean(&block, &tool, op).unwrap().unwrap();
            let provider = Fixed::new(Ok(step(&right)));
            let (r, events) = capture_fallbacks(|| fall_back(&block, &tool, op, failed(), 1.0, provider.as_ref()));
            assert!(r.is_ok(), "{op:?}: {events:?}");
        }
        let elsewhere = crate::boolean(&block, &drill(12.), BoolOp::Cut).unwrap().unwrap();
        let provider = Fixed::new(Ok(step(&elsewhere)));
        let (r, events) = capture_fallbacks(|| fall_back(&block, &drill(20.), BoolOp::Cut, failed(), 1.0, provider.as_ref()));
        assert!(r.is_err(), "a hole 8 mm off is not the cut asked for");
        assert_eq!(events[0].outcome, FallbackOutcome::Refused);
    }

    #[test]
    fn wrong_results_are_refused_with_the_kernels_error_and_the_reason() {
        let (a, b) = operands();
        // A cut that left the block whole: its volume is possible, its surface is not.
        let provider = Fixed::new(Ok(step(&a)));
        let (r, events) = handed_over(BoolOp::Cut, &provider);
        let message = r.unwrap_err().to_string();
        assert!(message.contains("no result") && message.contains("result was refused"), "{message}");
        assert_eq!(events.first().map(|e| e.outcome), Some(FallbackOutcome::Refused));
        assert!(events[0].reason.as_deref().is_some_and(|r| r.contains("must keep")), "{events:?}");
        // A join that lost the tool, an intersection bigger than the tool, a moved result.
        let union = crate::boolean(&a, &b, BoolOp::Union).unwrap().unwrap();
        let cut = crate::boolean(&a, &b, BoolOp::Cut).unwrap().unwrap();
        let moved = crate::transform(&cut, Vec3::new(0.5, 0., 0.), Vec3::ZERO, Vec3::Z, 0.0).unwrap();
        for (op, wrong) in [(BoolOp::Union, step(&a)), (BoolOp::Intersect, step(&union)), (BoolOp::Cut, step(&moved))] {
            let (r, events) = handed_over(op, &Fixed::new(Ok(wrong)));
            assert!(r.is_err(), "{op:?}");
            assert_eq!(events.first().map(|e| e.outcome), Some(FallbackOutcome::Refused), "{op:?}: {events:?}");
        }
        // No solid, an open shell, text that is not STEP.
        for wrong in ["".to_string(), "ISO-10303-21;\nEND-ISO-10303-21;\n".to_string(), "solid".to_string()] {
            let (r, events) = handed_over(BoolOp::Cut, &Fixed::new(Ok(wrong)));
            assert!(r.is_err());
            assert_eq!(events.first().map(|e| e.outcome), Some(FallbackOutcome::Refused), "{events:?}");
        }
    }

    #[test]
    fn a_provider_error_fails_the_operation_with_both_messages() {
        let provider = Fixed::new(Err("unsupported surface".into()));
        let (r, events) = handed_over(BoolOp::Cut, &provider);
        let message = r.unwrap_err().to_string();
        assert!(message.contains("no result") && message.contains("tried and failed: unsupported surface"), "{message}");
        assert_eq!(events[0].outcome, FallbackOutcome::Failed);
    }

    #[test]
    fn only_geometry_failures_of_solids_are_handed_over() {
        let (a, b) = operands();
        let provider = Fixed::new(Ok(step(&a)));
        let ok = crate::boolean(&a, &b, BoolOp::Cut);
        let invalid = Err(KernelError::Invalid("bad input".into()));
        let large = Err(KernelError::TooLarge("radius".into()));
        for kernel in [ok, invalid, large] {
            let expected = kernel.as_ref().map(|r| r.as_ref().map(Body::face_count)).map_err(Clone::clone);
            let (r, events) = capture_fallbacks(|| fall_back(&a, &b, BoolOp::Cut, kernel, 1.0, provider.as_ref()));
            assert_eq!(r.as_ref().map(|r| r.as_ref().map(Body::face_count)).map_err(Clone::clone), expected);
            assert!(events.is_empty());
        }
        let mesh = crate::mesh_body(&[Vec3::ZERO, Vec3::X, Vec3::Y], &[[0, 1, 2]]).unwrap();
        let (r, events) = capture_fallbacks(|| fall_back(&a, &mesh, BoolOp::Cut, failed(), 1.0, provider.as_ref()));
        assert!(r.is_err() && events.is_empty());
        assert_eq!(provider.calls(), 0);
    }

    #[test]
    fn without_a_provider_the_kernel_answers_alone() {
        let (a, b) = operands();
        let provider = Fixed::new(Ok(step(&a)));
        assert!(with_provider(None, current).is_none());
        assert_eq!(with_provider(None, digest), 0);
        let (r, events) = with_provider(None, || capture_fallbacks(|| boolean_with_fallback(&a, &b, BoolOp::Cut)));
        assert!(r.unwrap().is_some() && events.is_empty());
        // A provider is asked only when the kernel fails: a cut that works never reaches it.
        let p: Arc<dyn GeometryProvider> = provider.clone();
        let (r, events) = with_provider(Some(p), || {
            assert_ne!(digest(), 0);
            capture_fallbacks(|| boolean_with_fallback(&a, &b, BoolOp::Cut))
        });
        assert!(r.unwrap().is_some() && events.is_empty());
        assert_eq!(provider.calls(), 0);
        assert!(with_provider(None, current).is_none(), "the override is restored");
    }

    #[test]
    fn captures_nest_and_are_bounded() {
        let (a, b) = operands();
        let provider = Fixed::new(Err("no".into()));
        let (inner, outer) = capture_fallbacks(|| {
            let (_, inner) = capture_fallbacks(|| fall_back(&a, &b, BoolOp::Cut, failed(), 0.0, provider.as_ref()));
            for _ in 0..MAX_EVENTS + 4 {
                let _ = fall_back(&a, &b, BoolOp::Cut, failed(), 0.0, provider.as_ref());
            }
            inner
        });
        assert_eq!(inner.len(), 1, "the inner capture keeps its own event");
        assert_eq!(outer.len(), MAX_EVENTS, "the outer one has its own, capped");
        assert!(capture_fallbacks(|| ()).1.is_empty());
    }

    #[test]
    fn requests_and_replies_have_a_stable_wire_form() {
        let request = ProviderRequest {
            operation: ProviderOperation::Boolean { kind: BoolOp::Cut },
            operands: vec!["A".into(), "B".into()],
            tolerance_mm: 1e-6,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json, serde_json::json!({"op": "boolean", "kind": "cut", "operands": ["A", "B"], "tolerance_mm": 1e-6}));
        assert_eq!(serde_json::from_value::<ProviderRequest>(json).unwrap(), request);
        assert_eq!(request.operation.label(), "boolean cut");
    }
}
