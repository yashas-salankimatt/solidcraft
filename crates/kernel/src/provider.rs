//! Alternate geometry providers (`docs/geometry-providers.md`): another kernel an operation
//! may hand over to when SolidCraft's own fails with a geometry error: a Boolean of two solids,
//! a constant-radius fillet or an equal-distance chamfer of a solid's edges.
//!
//! A provider is only a trait here, taking and returning STEP text, so nothing of it is linked
//! into this crate: the engine runs it as a separate program, and the browser build has none.
//! With no provider installed every operation is exactly what it was.
//!
//! The kernel's own result always comes first. A provider's result is kept only once it passes
//! checks that do not rely on the kernel that just failed (`check`, `blend_check`): one closed,
//! sound solid whose volume a join, cut or intersection of these operands can have, nothing
//! changed away from the tool (or the blended edges), and its surface where the operands' surfaces
//! say it must be. Every attempt is
//! recorded ([`FallbackEvent`]), so a result a provider made is never passed off as the
//! kernel's. Its faces carry no history: they are named from geometry (unverified).
//!
//! A kept result can be saved with the design and offered back to the feature that made it, while
//! the design up to it is as it was ([`with_saved_results`]). When no provider gives a result now it
//! is used, after the same checks, for the operation of that kind whose operands it fits
//! ([`OperandPrint`]): so a computer without the provider (the browser) still shows, measures and
//! exports what one made.

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
    /// More about the provider as a JSON object (its program, the operations it does), for
    /// `geometry.providers`.
    fn describe(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    /// Whether requests should also carry the operands in SolidCraft's own encoding
    /// ([`ProviderRequest::native`]): a replay bundle's recorder does, so a replay builds the
    /// very bodies the kernel failed on. A provider program never gets them.
    fn wants_native_operands(&self) -> bool {
        false
    }
    /// Whether it does `operation` ([`ProviderOperation::name`]): one it doesn't is never handed
    /// to it, so the operation fails as it does without a provider.
    fn does(&self, operation: &str) -> bool {
        let _ = operation;
        true
    }
}

/// An operation a provider is asked to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProviderOperation {
    /// A Boolean of the two operands: the first is the target, the second the tool.
    Boolean { kind: BoolOp },
    /// A round of `radius` on the one operand's edges, each given by a point on it (mm).
    Fillet { radius: f64, edges: Vec<[f64; 3]> },
    /// A chamfer `distance` back along both faces of the one operand's edges, each given by a
    /// point on it (mm).
    Chamfer { distance: f64, edges: Vec<[f64; 3]> },
}

impl ProviderOperation {
    /// The operation's name on the wire, as a provider lists the ones it does (`boolean`).
    pub fn name(&self) -> &'static str {
        match self {
            ProviderOperation::Boolean { .. } => "boolean",
            ProviderOperation::Fillet { .. } => "fillet",
            ProviderOperation::Chamfer { .. } => "chamfer",
        }
    }

    /// The operation in words (`boolean cut`, `fillet`).
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
            other => other.name().to_string(),
        }
    }

    /// How many operands it takes.
    fn arity(&self) -> usize {
        match self {
            ProviderOperation::Boolean { .. } => 2,
            ProviderOperation::Fillet { .. } | ProviderOperation::Chamfer { .. } => 1,
        }
    }

    /// The refusal of a request with `n` operands when it takes another number.
    fn wrong_operands(&self, n: usize) -> KernelError {
        let takes = if self.arity() == 2 { "two operands" } else { "one operand" };
        KernelError::Invalid(format!("a {} takes {takes}; the request has {n}", self.label()))
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
    /// The operands in SolidCraft's native B-rep encoding, when the provider asked for them
    /// ([`GeometryProvider::wants_native_operands`]); never part of the wire form.
    #[serde(skip)]
    pub native: Vec<Vec<u8>>,
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
    /// No provider gave a result, and the one a provider made before and the design saved, for
    /// exactly these operands, passed the checks and was kept.
    Saved,
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
    /// A kept result, as STEP text, and what its operands were like, for the design to save; not
    /// reported.
    #[serde(skip)]
    pub result_step: Option<Arc<str>>,
    #[serde(skip)]
    pub operands: Option<Vec<OperandPrint>>,
}

/// The kernel's first reason in an error (`boolean Union: no result`): the whole of it, every
/// retry included, is in [`FallbackEvent::kernel_error`].
fn first_reason(kernel_error: &str) -> String {
    kernel_error.split("; ").next().unwrap_or(kernel_error).replace("the operation failed: ", "")
}

impl FallbackEvent {
    /// Whether the operation's result is the provider's (made now, or as the design saved it).
    pub fn kept(&self) -> bool {
        matches!(self.outcome, FallbackOutcome::Accepted | FallbackOutcome::Saved)
    }

    /// The event in words, for a feature's warning or error.
    pub fn description(&self) -> String {
        let FallbackEvent { operation, kernel_error, provider, .. } = self;
        let kernel_error = first_reason(kernel_error);
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
            FallbackOutcome::Saved => format!(
                "the {operation} is the result the {provider} geometry provider ({}) made and the design saved, because SolidCraft's \
                 kernel could not do it ({kernel_error}) and no provider here made it: it is shown, measured and exported as saved, and \
                 rebuilding it once what it is given changes needs the {provider} geometry provider",
                self.provider_version
            ),
        }
    }
}

/// The note a feature carries for the operations a provider made for it: none when it made none.
pub fn warning(events: &[FallbackEvent]) -> Option<String> {
    let accepted: Vec<&FallbackEvent> = events.iter().filter(|e| e.kept()).collect();
    match accepted.as_slice() {
        [] => None,
        [one] => Some(format!("Geometry provider: {}", one.description())),
        [first, ..] if accepted.iter().any(|e| e.outcome == FallbackOutcome::Saved) => Some(format!(
            "Geometry provider: {} operations SolidCraft's kernel could not do (first: {}) are results the {} geometry provider made, \
             some of them as the design saved them: rebuilding those once what they are given changes needs the provider",
            accepted.len(),
            first_reason(&first.kernel_error),
            first.provider
        )),
        [first, ..] => Some(format!(
            "Geometry provider: {} operations were made by the {} geometry provider because SolidCraft's kernel could not do them \
             (first: {}); their faces are named from geometry, so check features that refer to them after an edit",
            accepted.len(),
            first.provider,
            first_reason(&first.kernel_error)
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

/// What a Boolean's operand was like, to tell which saved result belongs to which operation of a
/// feature: its volume, area, centre of mass and box. Close is enough: the same design measures
/// a little differently on another computer (the browser's floating point is not the desktop's),
/// whether the design is unchanged is settled by the feature's design (`provider_results`), and a
/// saved result is checked against the actual operands before it is used.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperandPrint {
    pub volume: f64,
    pub area: f64,
    pub centroid: [f64; 3],
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl OperandPrint {
    /// The print of `body`, when it can be measured.
    pub fn of(body: &Body) -> Option<Self> {
        let m = crate::measure(body).ok()?;
        let v = |p: Vec3| [p.x, p.y, p.z];
        let print = OperandPrint { volume: m.volume, area: m.area, centroid: v(m.centroid), min: v(m.bbox.min), max: v(m.bbox.max) };
        print.finite().then_some(print)
    }

    fn finite(&self) -> bool {
        [self.volume, self.area].iter().chain(&self.centroid).chain(&self.min).chain(&self.max).all(|x| x.is_finite())
    }

    /// Whether `self` and `other` are the same operand to within a thousandth of its size.
    fn fits(&self, other: &OperandPrint) -> bool {
        let size = (0..3).map(|k| (self.max[k] - self.min[k]).abs()).fold(0.0, f64::max).max(1e-9);
        let near = |a: f64, b: f64, scale: f64| (a - b).abs() <= 1e-3 * scale.abs().max(1e-12);
        other.finite()
            && near(self.volume, other.volume, self.volume)
            && near(self.area, other.area, self.area)
            && (0..3).all(|k| {
                near(self.centroid[k], other.centroid[k], size) && near(self.min[k], other.min[k], size) && near(self.max[k], other.max[k], size)
            })
    }
}

/// A result a provider made, as a design saved it: offered back by [`with_saved_results`].
#[derive(Clone, Debug, PartialEq)]
pub struct SavedResult {
    pub provider: String,
    pub version: String,
    /// The operation (`boolean cut`).
    pub operation: String,
    /// What its operands were like, in order.
    pub operands: Vec<OperandPrint>,
    /// The result as STEP text.
    pub step: Arc<str>,
}

/// The saved results offered to a feature.
pub type SavedResults = Vec<SavedResult>;

thread_local! {
    static SAVED: RefCell<Option<Arc<SavedResults>>> = const { RefCell::new(None) };
}

/// Run `f` with `saved` offered to every operation it hands over on this thread (`None`: none).
pub fn with_saved_results<R>(saved: Option<Arc<SavedResults>>, f: impl FnOnce() -> R) -> R {
    let previous = SAVED.with(|s| s.replace(saved));
    struct Restore(Option<Option<Arc<SavedResults>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                SAVED.with(|s| *s.borrow_mut() = previous);
            }
        }
    }
    let _restore = Restore(Some(previous));
    f()
}

fn saved_results() -> Option<Arc<SavedResults>> {
    SAVED.with(|s| s.borrow().clone())
}

/// [`crate::boolean`], handed to the current provider when SolidCraft's kernel fails with a
/// geometry error, or answered from a saved result (see the module docs). Exactly
/// [`crate::boolean`] when there is neither.
pub fn boolean_with_fallback(a: &Body, b: &Body, op: BoolOp) -> Result<Option<Body>> {
    let (provider, saved) = (current(), saved_results());
    if provider.is_none() && saved.is_none() {
        return crate::boolean(a, b, op);
    }
    crate::profile::timed(crate::profile::Kind::Boolean, || {
        let started = web_time::Instant::now();
        let kernel = crate::boolean(a, b, op);
        fall_back(&ProviderOperation::Boolean { kind: op }, &[a, b], kernel, ms(started), provider.as_deref(), saved.as_deref())
    })
}

/// [`crate::fillet`], handed over or answered from a saved result as [`boolean_with_fallback`]
/// is. Only a geometry failure is: a radius too large for the faces around the edges
/// ([`KernelError::TooLarge`]) is the kernel's answer, so a fillet's fit can reduce or taper it.
pub fn fillet_with_fallback(body: &Body, edges: &[Vec3], radius: f64) -> Result<Body> {
    blend_with_fallback(body, ProviderOperation::Fillet { radius, edges: points(edges) }, || crate::fillet(body, edges, radius))
}

/// [`crate::chamfer`] (equal distances), handed over as [`fillet_with_fallback`] is.
pub fn chamfer_with_fallback(body: &Body, edges: &[Vec3], distance: f64) -> Result<Body> {
    blend_with_fallback(body, ProviderOperation::Chamfer { distance, edges: points(edges) }, || crate::chamfer(body, edges, distance))
}

fn points(edges: &[Vec3]) -> Vec<[f64; 3]> {
    edges.iter().map(|p| [p.x, p.y, p.z]).collect()
}

fn blend_with_fallback(body: &Body, operation: ProviderOperation, kernel: impl FnOnce() -> Result<Body>) -> Result<Body> {
    let (provider, saved) = (current(), saved_results());
    if provider.is_none() && saved.is_none() {
        return kernel();
    }
    let started = web_time::Instant::now();
    let kernel = kernel().map(Some);
    fall_back(&operation, &[body], kernel, ms(started), provider.as_deref(), saved.as_deref())?
        .ok_or_else(|| KernelError::Failed(format!("the {} left nothing", operation.label())))
}

fn ms(since: web_time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The kernel's result of `operation` on `operands`, or, when the kernel failed with a geometry
/// error, `provider`'s or the one `saved` holds for these operands, whichever comes first and
/// passes the checks.
fn fall_back(
    operation: &ProviderOperation,
    operands: &[&Body],
    kernel: Result<Option<Body>>,
    kernel_ms: f64,
    provider: Option<&dyn GeometryProvider>,
    saved: Option<&SavedResults>,
) -> Result<Option<Body>> {
    // Bad input (a mesh, a refused shape) is no geometry failure, and a provider gets solids only.
    let error = match kernel {
        Err(e @ (KernelError::Failed(_) | KernelError::Internal(_)))
            if operands.iter().all(|x| !x.is_mesh()) && operands.len() == operation.arity() =>
        {
            e
        }
        other => return other,
    };
    let event = |provider: &str, version: &str| FallbackEvent {
        operation: operation.label(),
        kernel_error: error.to_string(),
        provider: provider.to_string(),
        provider_version: version.to_string(),
        outcome: FallbackOutcome::Failed,
        reason: None,
        kernel_ms,
        provider_ms: 0.0,
        check_ms: 0.0,
        result_step: None,
        operands: None,
    };
    let prints = || operands.iter().map(|x| OperandPrint::of(x)).collect::<Option<Vec<_>>>();
    let mut notes = Vec::new();
    if let Some(provider) = provider.filter(|p| p.does(operation.name())) {
        let mut e = event(provider.id(), provider.version());
        let asked = web_time::Instant::now();
        let reply = request(operands, operation.clone(), provider.wants_native_operands())
            .and_then(|request| crate::guard("geometry provider", || provider.run(&request).map_err(KernelError::Failed)));
        e.provider_ms = ms(asked);
        match reply {
            Err(why) => e.reason = Some(why.to_string().trim_start_matches("the operation failed: ").to_string()),
            Ok(reply) => {
                let checking = web_time::Instant::now();
                let checked = checked(operation, operands, &reply.step);
                e.check_ms = ms(checking);
                match checked {
                    Ok(body) => {
                        e.outcome = FallbackOutcome::Accepted;
                        e.result_step = Some(reply.step.into());
                        e.operands = prints();
                        record(e);
                        return Ok(Some(body));
                    }
                    Err(why) => {
                        e.outcome = FallbackOutcome::Refused;
                        e.reason = Some(why);
                    }
                }
            }
        }
        notes.push(e.description());
        record(e);
    }
    let label = operation.label();
    let prints = saved.filter(|saved| saved.iter().any(|k| k.operation == label)).and_then(|_| prints());
    let fits = |k: &SavedResult, prints: &[OperandPrint]| k.operands.len() == prints.len() && k.operands.iter().zip(prints).all(|(k, p)| k.fits(p));
    if let (Some(saved), Some(prints)) = (saved, prints)
        && let Some(kept) = saved.iter().find(|k| k.operation == label && fits(k, &prints))
    {
        let mut e = event(&kept.provider, &kept.version);
        let checking = web_time::Instant::now();
        let checked = checked(operation, operands, &kept.step);
        e.check_ms = ms(checking);
        match checked {
            Ok(body) => {
                e.outcome = FallbackOutcome::Saved;
                e.result_step = Some(kept.step.clone());
                e.operands = Some(prints);
                record(e);
                return Ok(Some(body));
            }
            Err(why) => {
                e.outcome = FallbackOutcome::Refused;
                e.reason = Some(format!("the result the design saved no longer passes the checks: {why}"));
                notes.push(e.description());
                record(e);
            }
        }
    }
    if notes.is_empty() {
        return Err(error);
    }
    let note = notes.join("; ");
    Err(match error {
        KernelError::Failed(m) => KernelError::Failed(format!("{m}; {note}")),
        KernelError::Internal(m) => KernelError::Internal(format!("{m}; {note}")),
        other => other,
    })
}

/// [`accept`], with a panic inside it a refusal.
fn checked(operation: &ProviderOperation, operands: &[&Body], step: &str) -> std::result::Result<Body, String> {
    crate::guard("geometry provider result", || Ok(accept(operation, operands, step))).unwrap_or_else(|e| Err(e.to_string()))
}

/// One request replayed (`geometry.fallback_replay`): SolidCraft's kernel on the operands it
/// carries, then, when the kernel fails and there is a provider, the provider asked and its
/// result checked as an evaluation asks and checks it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Replay {
    /// The operation (`boolean cut`).
    pub operation: String,
    /// Why SolidCraft's kernel could not do it; `None` when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_error: Option<String>,
    pub kernel_ms: f64,
    /// What the operands were built from: `native` (the very bodies the operation was given) or
    /// `step` (read back from STEP, which can differ by the fit of their intersection curves).
    pub operands_from: &'static str,
    /// What the provider did, when it was asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackEvent>,
}

/// Replay `request` (see [`Replay`]). The operands are its native ones when it carries them, else
/// read back from their STEP text, which can differ from the bodies it was written from by the fit
/// of their intersection curves: the kernel may then do what it could not do on the originals.
pub fn replay(request: &ProviderRequest, provider: Option<&dyn GeometryProvider>) -> Result<Replay> {
    let (bodies, operands_from) = if request.native.len() == request.operation.arity() {
        (request.native.iter().map(|n| crate::decode_native_body(n)).collect::<Result<Vec<_>>>()?, "native")
    } else {
        (operands(request)?, "step")
    };
    let refs: Vec<&Body> = bodies.iter().collect();
    let started = web_time::Instant::now();
    let kernel = crate::guard("replayed operation", || run_kernel(&request.operation, &refs));
    let kernel_ms = ms(started);
    let kernel_error = kernel.as_ref().err().map(ToString::to_string);
    let fallback =
        provider.and_then(|p| capture_fallbacks(|| fall_back(&request.operation, &refs, kernel, kernel_ms, Some(p), None)).1.into_iter().next());
    Ok(Replay { operation: request.operation.label(), kernel_error, kernel_ms, operands_from, fallback })
}

/// SolidCraft's kernel doing `operation` on `operands`.
fn run_kernel(operation: &ProviderOperation, operands: &[&Body]) -> Result<Option<Body>> {
    let vec = |edges: &[[f64; 3]]| edges.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect::<Vec<_>>();
    match (operation, operands) {
        (ProviderOperation::Boolean { kind }, [a, b]) => crate::boolean(a, b, *kind),
        (ProviderOperation::Fillet { radius, edges }, [body]) => crate::fillet(body, &vec(edges), *radius).map(Some),
        (ProviderOperation::Chamfer { distance, edges }, [body]) => crate::chamfer(body, &vec(edges), *distance).map(Some),
        _ => Err(operation.wrong_operands(operands.len())),
    }
}

/// The bodies a request's operands hold.
fn operands(request: &ProviderRequest) -> Result<Vec<Body>> {
    let arity = request.operation.arity();
    if request.operands.len() != arity {
        return Err(request.operation.wrong_operands(request.operands.len()));
    }
    let names: &[&str] = if arity == 2 { &["target", "tool"] } else { &["body"] };
    let body = |step: &str, which: &str| -> Result<Body> {
        let import = crate::step_import(step).map_err(|e| KernelError::Invalid(format!("the {which}'s STEP could not be read: {e}")))?;
        crate::ops::join_pieces(import.bodies.into_iter().map(|piece| piece.body).collect())?
            .ok_or_else(|| KernelError::Invalid(format!("the {which} has no solid")))
    };
    request.operands.iter().zip(names).map(|(step, which)| body(step, which)).collect()
}

/// The request for `operation` on `operands` (with their native encoding when `native`).
fn request(operands: &[&Body], operation: ProviderOperation, native: bool) -> Result<ProviderRequest> {
    let step = |body: &Body| {
        crate::step_export_bodies(&[crate::ExportBody { name: "Operand".into(), body, color: None }], &crate::StepHeader::default())
            .map_err(|e| KernelError::Failed(format!("the operands could not be written as STEP: {e}")))
    };
    let tolerance_mm = (operands.iter().map(|x| x.size()).fold(0.0, f64::max) * 1e-7).max(1e-7);
    // A body the native encoding refuses is replayed from its STEP instead.
    let native =
        if native { operands.iter().map(|x| crate::encode_native_body(x)).collect::<Result<Vec<_>>>().unwrap_or_default() } else { Vec::new() };
    Ok(ProviderRequest { operation, operands: operands.iter().map(|x| step(x)).collect::<Result<_>>()?, tolerance_mm, native })
}

/// Most pieces a provider's result may have.
const MAX_PIECES: usize = 4096;

/// The provider's result as a body, if it passes the checks; else why not.
fn accept(operation: &ProviderOperation, operands: &[&Body], step: &str) -> std::result::Result<Body, String> {
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
    match (operation, operands) {
        (ProviderOperation::Boolean { kind }, [a, b]) => check(a, b, *kind, &body)?,
        (ProviderOperation::Fillet { radius: size, edges } | ProviderOperation::Chamfer { distance: size, edges }, [input]) => {
            crate::provider_blend::blend_check(input, edges, *size, &body)?
        }
        _ => return Err("it was asked for with the wrong number of operands".into()),
    }
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
pub(crate) fn check_tolerance(a: &Body, b: &Body) -> f64 {
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
pub(crate) fn samples(mesh: &Mesh) -> Vec<Vec3> {
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
            assert_eq!(request.operands.len(), request.operation.arity());
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
        capture_fallbacks(|| fall_back(&ProviderOperation::Boolean { kind: op }, &[&a, &b], kernel, 1.0, Some(provider), None))
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
            assert!(e.description().contains(&format!("(boolean {op:?}: no result)")), "the kernel's first reason: {}", e.description());
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
            let (r, events) = capture_fallbacks(|| {
                fall_back(&ProviderOperation::Boolean { kind: op }, &[&block, &tool], failed(), 1.0, Some(provider.as_ref()), None)
            });
            assert!(r.is_ok(), "{op:?}: {events:?}");
        }
        let elsewhere = crate::boolean(&block, &drill(12.), BoolOp::Cut).unwrap().unwrap();
        let provider = Fixed::new(Ok(step(&elsewhere)));
        let (r, events) = capture_fallbacks(|| {
            fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&block, &drill(20.)], failed(), 1.0, Some(provider.as_ref()), None)
        });
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
            let (r, events) = capture_fallbacks(|| {
                fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], kernel, 1.0, Some(provider.as_ref()), None)
            });
            assert_eq!(r.as_ref().map(|r| r.as_ref().map(Body::face_count)).map_err(Clone::clone), expected);
            assert!(events.is_empty());
        }
        let mesh = crate::mesh_body(&[Vec3::ZERO, Vec3::X, Vec3::Y], &[[0, 1, 2]]).unwrap();
        let (r, events) = capture_fallbacks(|| {
            fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &mesh], failed(), 1.0, Some(provider.as_ref()), None)
        });
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
            let (_, inner) = capture_fallbacks(|| {
                fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 0.0, Some(provider.as_ref()), None)
            });
            for _ in 0..MAX_EVENTS + 4 {
                let _ = fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 0.0, Some(provider.as_ref()), None);
            }
            inner
        });
        assert_eq!(inner.len(), 1, "the inner capture keeps its own event");
        assert_eq!(outer.len(), MAX_EVENTS, "the outer one has its own, capped");
        assert!(capture_fallbacks(|| ()).1.is_empty());
    }

    #[test]
    fn a_request_replays_from_its_operands() {
        let (a, b) = operands();
        let asked = request(&[&a, &b], ProviderOperation::Boolean { kind: BoolOp::Cut }, true).unwrap();
        assert_eq!(asked.native.len(), 2);
        assert_eq!(replay(&asked, None).unwrap().operands_from, "native");
        // As a bundle carries it: through its JSON.
        let carried: ProviderRequest = serde_json::from_str(&serde_json::to_string(&asked).unwrap()).unwrap();
        assert!(carried.native.is_empty(), "the native operands are not part of the wire form");
        let r = replay(&carried, None).unwrap();
        assert_eq!((r.operation.as_str(), r.kernel_error.as_ref(), r.operands_from, r.fallback.as_ref()), ("boolean cut", None, "step", None));
        let provider = Fixed::new(Ok(step(&a)));
        assert!(replay(&carried, Some(provider.as_ref())).unwrap().fallback.is_none(), "a provider is asked only when the kernel fails");
        let mut one = carried.clone();
        one.operands.truncate(1);
        assert!(replay(&one, None).unwrap_err().to_string().contains("two operands"));
        let mut junk = carried;
        junk.operands[1] = "solid".into();
        assert!(replay(&junk, None).unwrap_err().to_string().contains("tool's STEP"));
    }

    fn saved(a: &Body, b: &Body, op: BoolOp, result: &Body) -> SavedResults {
        let operands = [OperandPrint::of(a).unwrap(), OperandPrint::of(b).unwrap()];
        let operation = ProviderOperation::Boolean { kind: op }.label();
        vec![SavedResult { provider: "fixed".into(), version: "0.9".into(), operation, operands: operands.to_vec(), step: step(result).into() }]
    }

    #[test]
    fn a_saved_result_answers_when_no_provider_does() {
        let (a, b) = operands();
        let right = crate::boolean(&a, &b, BoolOp::Cut).unwrap().unwrap();
        let store = saved(&a, &b, BoolOp::Cut, &right);
        // No provider: the saved result, checked, with what it is and where it came from.
        let (r, events) =
            capture_fallbacks(|| fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 1.0, None, Some(&store)));
        assert!(r.unwrap().unwrap().history().is_none());
        let e = &events[0];
        assert_eq!((e.outcome, e.provider.as_str(), e.provider_version.as_str()), (FallbackOutcome::Saved, "fixed", "0.9"));
        assert!(e.kept() && e.result_step.is_some() && e.operands == Some(store[0].operands.clone()));
        assert!(warning(&events).is_some_and(|w| w.contains("as saved") && w.contains("needs the fixed geometry provider")), "{events:?}");
        // A provider that fails first, then the saved result.
        let failing = Fixed::new(Err("down".into()));
        let (r, events) = capture_fallbacks(|| {
            fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 1.0, Some(failing.as_ref()), Some(&store))
        });
        assert!(r.is_ok());
        assert_eq!(events.iter().map(|e| e.outcome).collect::<Vec<_>>(), [FallbackOutcome::Failed, FallbackOutcome::Saved]);
        // A provider that answers is kept, and the saved result is not looked at.
        let answering = Fixed::new(Ok(step(&right)));
        let (_, events) = capture_fallbacks(|| {
            fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 1.0, Some(answering.as_ref()), Some(&store))
        });
        assert_eq!(events.iter().map(|e| e.outcome).collect::<Vec<_>>(), [FallbackOutcome::Accepted]);
        assert_eq!(events[0].result_step.as_deref(), Some(step(&right).as_str()), "a kept result travels with its event, to be saved");
    }

    #[test]
    fn a_saved_result_for_other_operands_or_a_wrong_one_is_never_used() {
        let (a, b) = operands();
        let right = crate::boolean(&a, &b, BoolOp::Cut).unwrap().unwrap();
        // The tool moved since: another request, so nothing saved matches and the error is as before.
        let moved = crate::transform(&b, Vec3::new(1., 0., 0.), Vec3::ZERO, Vec3::Z, 0.0).unwrap();
        let store = saved(&a, &b, BoolOp::Cut, &right);
        let (r, events) =
            capture_fallbacks(|| fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &moved], failed(), 1.0, None, Some(&store)));
        assert_eq!(r.unwrap_err().to_string(), failed().unwrap_err().to_string());
        assert!(events.is_empty());
        // Saved for these operands, but wrong (the block uncut): refused, never shown.
        let wrong = saved(&a, &b, BoolOp::Cut, &a);
        let (r, events) =
            capture_fallbacks(|| fall_back(&ProviderOperation::Boolean { kind: BoolOp::Cut }, &[&a, &b], failed(), 1.0, None, Some(&wrong)));
        assert!(r.unwrap_err().to_string().contains("the result the design saved no longer passes the checks"));
        assert_eq!(events[0].outcome, FallbackOutcome::Refused);
        // Offered while nothing fails: the kernel answers alone.
        let (r, events) = with_saved_results(Some(Arc::new(store)), || capture_fallbacks(|| boolean_with_fallback(&a, &b, BoolOp::Cut)));
        assert!(r.unwrap().is_some() && events.is_empty());
        assert!(saved_results().is_none(), "the offer ends with its scope");
    }

    /// The same operand measured on another computer differs in its last digits; another operand
    /// (moved, resized) does not fit.
    #[test]
    fn operand_prints_fit_the_same_operand_only() {
        let (a, b) = operands();
        let (pa, pb) = (OperandPrint::of(&a).unwrap(), OperandPrint::of(&b).unwrap());
        let mut noisy = pa;
        noisy.volume *= 1.0 + 1e-12;
        noisy.centroid[0] += 1e-9;
        assert!(pa.fits(&noisy) && !pa.fits(&pb));
        let moved = OperandPrint::of(&crate::transform(&a, Vec3::new(0.5, 0., 0.), Vec3::ZERO, Vec3::Z, 0.0).unwrap()).unwrap();
        let grown = OperandPrint::of(&cube(Vec3::ZERO, Vec3::new(20., 10., 10.5))).unwrap();
        assert!(!pa.fits(&moved) && !pa.fits(&grown));
        let mut broken = pa;
        broken.area = f64::NAN;
        assert!(!pa.fits(&broken));
    }

    #[test]
    fn requests_and_replies_have_a_stable_wire_form() {
        let request = ProviderRequest {
            operation: ProviderOperation::Boolean { kind: BoolOp::Cut },
            operands: vec!["A".into(), "B".into()],
            tolerance_mm: 1e-6,
            native: Vec::new(),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json, serde_json::json!({"op": "boolean", "kind": "cut", "operands": ["A", "B"], "tolerance_mm": 1e-6}));
        assert_eq!(serde_json::from_value::<ProviderRequest>(json).unwrap(), request);
        assert_eq!(request.operation.label(), "boolean cut");
    }

    /// A block, an edge of it (a point on the top front edge) and the blends of it.
    fn blend_case() -> (Body, Vec3) {
        (cube(Vec3::ZERO, Vec3::new(40., 30., 20.)), Vec3::new(20., 0., 20.))
    }

    fn blend_failed() -> Result<Option<Body>> {
        Err(KernelError::Failed("not supported yet: a vertex of the edge chain where other faces meet".into()))
    }

    fn blended(operation: &ProviderOperation, body: &Body, provider: &dyn GeometryProvider) -> (Result<Option<Body>>, Vec<FallbackEvent>) {
        capture_fallbacks(|| fall_back(operation, &[body], blend_failed(), 1.0, Some(provider), None))
    }

    #[test]
    fn a_right_fillet_or_chamfer_is_kept_and_recorded_with_its_one_operand() {
        let (block, edge) = blend_case();
        for (operation, right) in [
            (ProviderOperation::Fillet { radius: 2.0, edges: vec![[edge.x, edge.y, edge.z]] }, crate::fillet(&block, &[edge], 2.0).unwrap()),
            (ProviderOperation::Chamfer { distance: 2.0, edges: vec![[edge.x, edge.y, edge.z]] }, crate::chamfer(&block, &[edge], 2.0).unwrap()),
        ] {
            let provider = Fixed::new(Ok(step(&right)));
            let (r, events) = blended(&operation, &block, provider.as_ref());
            let body = r.unwrap_or_else(|e| panic!("{}: {e}", operation.label())).unwrap();
            let (got, want) = (crate::measure(&body).unwrap().volume, crate::measure(&right).unwrap().volume);
            assert!((got - want).abs() < 1e-6 * want, "{}: {got} vs {want}", operation.label());
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].outcome, FallbackOutcome::Accepted, "{events:?}");
            assert_eq!(events[0].operation, operation.label());
            assert_eq!(events[0].operands.as_ref().map(Vec::len), Some(1), "one operand's print, for the design to save");
            assert_eq!(provider.calls(), 1);
        }
    }

    #[test]
    fn a_blend_that_misses_its_edge_or_changes_the_body_elsewhere_is_refused() {
        let (block, edge) = blend_case();
        let fillet = ProviderOperation::Fillet { radius: 2.0, edges: vec![[edge.x, edge.y, edge.z]] };
        let other_edge = Vec3::new(20., 30., 20.);
        for (why, wrong, expect) in [
            ("left as it was", block.clone(), "not blended"),
            ("another edge rounded", crate::fillet(&block, &[other_edge], 2.0).unwrap(), "away from the blended edges"),
            ("a radius of 8 instead of 2", crate::fillet(&block, &[edge], 8.0).unwrap(), "away from the blended edges"),
            (
                "moved",
                crate::transform(&crate::fillet(&block, &[edge], 2.0).unwrap(), Vec3::new(0.5, 0., 0.), Vec3::ZERO, Vec3::Z, 0.0).unwrap(),
                "outside",
            ),
        ] {
            let (r, events) = blended(&fillet, &block, Fixed::new(Ok(step(&wrong))).as_ref());
            let message = r.unwrap_err().to_string();
            assert!(message.contains("not supported yet") && message.contains("result was refused"), "{why}: {message}");
            assert_eq!(events[0].outcome, FallbackOutcome::Refused, "{why}");
            assert!(events[0].reason.as_deref().is_some_and(|r| r.contains(expect)), "{why}: {:?}", events[0].reason);
        }
    }

    /// A chamfer built the wrong way round (a wedge added outside a convex edge, as a provider once
    /// answered a fuzzed part with): inside the body's box, a plausible volume, new surface only
    /// beside the edge, yet on the wrong side of the body.
    #[test]
    fn a_blend_built_the_wrong_way_round_is_refused() {
        // A block with a step along its foot, so the box reaches past the top front edge.
        let block = crate::boolean(&cube(Vec3::ZERO, Vec3::new(40., 30., 20.)), &cube(Vec3::new(0., -5., 0.), Vec3::new(40., 0., 5.)), BoolOp::Union)
            .unwrap()
            .unwrap();
        let edge = Vec3::new(20., 0., 20.);
        let chamfer = ProviderOperation::Chamfer { distance: 2.0, edges: vec![[edge.x, edge.y, edge.z]] };
        let right = crate::chamfer(&block, &[edge], 2.0).unwrap();
        let (r, events) = blended(&chamfer, &block, Fixed::new(Ok(step(&right))).as_ref());
        assert!(r.is_ok(), "the right chamfer: {events:?}");
        let wedge = crate::boolean(&block, &cube(Vec3::new(10., -1.5, 18.5), Vec3::new(30., 0., 20.)), BoolOp::Union).unwrap().unwrap();
        let (r, events) = blended(&chamfer, &block, Fixed::new(Ok(step(&wedge))).as_ref());
        assert!(r.is_err(), "material added outside a convex edge");
        assert!(events[0].reason.as_deref().is_some_and(|r| r.contains("wrong way round")), "{:?}", events[0].reason);
    }

    #[test]
    fn only_a_geometry_failure_of_a_blend_is_handed_over_and_only_to_a_provider_that_does_it() {
        let (block, edge) = blend_case();
        let fillet = ProviderOperation::Fillet { radius: 2.0, edges: vec![[edge.x, edge.y, edge.z]] };
        let provider = Fixed::new(Ok(step(&crate::fillet(&block, &[edge], 2.0).unwrap())));
        // Too large for the faces around it: the fit reduces or tapers it, nothing is asked.
        let too_large = Err(KernelError::TooLarge("not supported yet: the blend is larger than the neighbouring faces".into()));
        let (r, events) = capture_fallbacks(|| fall_back(&fillet, &[&block], too_large, 1.0, Some(provider.as_ref()), None));
        assert!(matches!(r, Err(KernelError::TooLarge(_))) && events.is_empty() && provider.calls() == 0);
        // A provider that only does Booleans is never asked for a fillet.
        struct BooleansOnly(Arc<Fixed>);
        impl GeometryProvider for BooleansOnly {
            fn id(&self) -> &str {
                "booleans"
            }
            fn version(&self) -> &str {
                "1"
            }
            fn run(&self, request: &ProviderRequest) -> std::result::Result<ProviderReply, String> {
                self.0.run(request)
            }
            fn does(&self, operation: &str) -> bool {
                operation == "boolean"
            }
        }
        let only = BooleansOnly(provider.clone());
        let (r, events) = blended(&fillet, &block, &only);
        assert_eq!(r.unwrap_err().to_string(), blend_failed().unwrap_err().to_string(), "the error as it was");
        assert!(events.is_empty() && provider.calls() == 0);
        // The wire form: the operation's name, its size and edge points, one operand.
        let asked = request(&[&block], fillet.clone(), false).unwrap();
        let wire = serde_json::to_value(&asked).unwrap();
        assert_eq!((wire["op"].as_str(), wire["radius"].as_f64(), wire["operands"].as_array().map(Vec::len)), (Some("fillet"), Some(2.0), Some(1)));
        assert_eq!(wire["edges"], serde_json::json!([[20.0, 0.0, 20.0]]));
        let back: ProviderRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back.operation, fillet);
    }

    #[test]
    fn a_saved_fillet_is_offered_back_when_no_provider_makes_it() {
        let (block, edge) = blend_case();
        let fillet = ProviderOperation::Fillet { radius: 2.0, edges: vec![[edge.x, edge.y, edge.z]] };
        let right = crate::fillet(&block, &[edge], 2.0).unwrap();
        let saved = vec![SavedResult {
            provider: "fixed".into(),
            version: "1".into(),
            operation: "fillet".into(),
            operands: vec![OperandPrint::of(&block).unwrap()],
            step: step(&right).into(),
        }];
        let (r, events) = capture_fallbacks(|| fall_back(&fillet, &[&block], blend_failed(), 1.0, None, Some(&saved)));
        assert!(r.is_ok(), "{events:?}");
        assert_eq!(events[0].outcome, FallbackOutcome::Saved);
        // Not for another body.
        let other = cube(Vec3::ZERO, Vec3::new(41., 30., 20.));
        let (r, events) = capture_fallbacks(|| fall_back(&fillet, &[&other], blend_failed(), 1.0, None, Some(&saved)));
        assert!(r.is_err() && events.is_empty());
    }

    #[test]
    fn replaying_a_blend_request_runs_the_kernels_blend_then_the_provider() {
        let (block, edge) = blend_case();
        let fillet = ProviderOperation::Fillet { radius: 2.0, edges: vec![[edge.x, edge.y, edge.z]] };
        let asked = request(&[&block], fillet, true).unwrap();
        // The kernel can round this edge: nothing is handed over.
        let replayed = replay(&asked, Some(Fixed::new(Err("unused".into())).as_ref())).unwrap();
        assert_eq!((replayed.operation.as_str(), replayed.kernel_error.as_deref(), replayed.operands_from), ("fillet", None, "native"));
        assert!(replayed.fallback.is_none());
    }
}
