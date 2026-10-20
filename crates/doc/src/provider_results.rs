//! Results geometry providers made for a design's features, saved with the design
//! (`docs/geometry-providers.md`), so a computer without the provider (the browser, a machine
//! without OCCT) still shows, measures and exports them, and says which feature needs the provider
//! to be rebuilt.
//!
//! A saved result is a cache, not the design: the feature keeps its parameters and inputs, and its
//! result is rebuilt whenever it can be. A saved one is offered back only to the feature that made
//! it, and only while the design up to that feature is exactly as it was saved ([`intent`]): an
//! edit to it or to anything before it means the result must be made again. Which saved result is
//! which of the feature's operations is told by their operands ([`OperandPrint`]), and each is
//! used only after the checks a provider's answer gets (`solidcraft_kernel::provider`). Nothing
//! native to a provider is saved: the result is STEP text.
//!
//! The design is compared, not the operands' geometry: the same design evaluates to operands that
//! differ in their last digits on another computer (the browser's floating point is not the
//! desktop's), and a saved result must open there all the same.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use solidcraft_kernel as kernel;
use solidcraft_kernel::provider::{OperandPrint, SavedResult, SavedResults};

use crate::{Document, Model};

/// Most results a design offers back.
pub const MAX_RESULTS: usize = 1024;

/// Most bytes of saved STEP a design offers back, in all.
pub const MAX_BYTES: usize = 256 << 20;

/// One result a geometry provider made for a feature's operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderResult {
    /// The feature whose operation it is.
    pub feature: u64,
    /// The provider that made it (`occt`) and its version.
    pub provider: String,
    pub version: String,
    /// The operation (`boolean cut`).
    pub operation: String,
    /// The design up to and including the feature when the result was made ([`intent`]).
    pub intent: String,
    /// What the operation's operands were like, in order (two for a Boolean, one for a fillet or
    /// chamfer). Saved as a list, so a design saved before blends were handed over reads the same.
    pub operands: Vec<OperandPrint>,
    /// The result, as STEP text (a deduplicated blob in a container).
    pub step: Arc<str>,
}

/// A name or version as a provider gives it: short, printable.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control)
}

/// The design up to and including feature `index` of `doc`, as a stable digest (64-bit FNV-1a of
/// its saved JSON, in hex): units, parameters, components and the features through it. The same on
/// every computer and in every build that saves the design alike.
pub fn intent(doc: &Document, index: usize) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |text: String| {
        for b in text.as_bytes().iter().chain(&(text.len() as u64).to_le_bytes()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(doc.units.clone());
    eat(json(&doc.params));
    eat(json(&doc.components));
    eat(json(&doc.body_components));
    eat(json(&doc.sheet));
    eat(json(&doc.plastic));
    eat(json(&doc.configs));
    eat(json(&doc.features.get(..=index).unwrap_or_default()));
    format!("{h:016x}")
}

/// `v` as the design saves it.
fn json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// The saved results to offer feature `index` of `doc` (whose id is `id`): those made for it while
/// the design up to it was as it is now. `None` when there are none.
pub fn offered_for(doc: &Document, index: usize, id: u64) -> Option<Arc<SavedResults>> {
    if !doc.provider_results.iter().any(|r| r.feature == id) {
        return None;
    }
    let now = intent(doc, index);
    let mut bytes = 0usize;
    let mut out = SavedResults::new();
    // An operation has one or two operands: a saved result with more is from a hostile file.
    for r in doc.provider_results.iter().take(MAX_RESULTS).filter(|r| r.feature == id && r.intent == now && (1..=2).contains(&r.operands.len())) {
        if !(plain(&r.provider) && plain(&r.version) && plain(&r.operation) && !r.step.is_empty()) {
            continue;
        }
        bytes = bytes.saturating_add(r.step.len());
        if bytes > MAX_BYTES {
            break;
        }
        out.push(SavedResult {
            provider: r.provider.clone(),
            version: r.version.clone(),
            operation: r.operation.clone(),
            operands: r.operands.clone(),
            step: r.step.clone(),
        });
    }
    (!out.is_empty()).then(|| Arc::new(out))
}

/// A digest of the results `doc` saved: 0, and so every evaluation key as before, when it saved
/// none.
pub fn digest(doc: &Document) -> u64 {
    use std::hash::{Hash, Hasher};
    if doc.provider_results.is_empty() {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in doc.provider_results.iter().take(MAX_RESULTS) {
        (r.feature, &r.intent, &r.provider, &r.version, r.step.len()).hash(&mut h);
    }
    h.finish() | 1
}

/// The results to save with `doc` once `model` has evaluated it: those its features keep now (made
/// by a provider, or taken as saved), and those saved for features not evaluated now (suppressed,
/// rolled back) so they are not lost.
pub fn to_save(doc: &Document, model: &Model) -> Vec<ProviderResult> {
    let mut out: Vec<ProviderResult> = Vec::new();
    let index: std::collections::BTreeMap<u64, usize> = doc.features.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
    let evaluated: BTreeSet<u64> = model.results.iter().filter(|r| !r.skipped).map(|r| r.id).collect();
    for r in model.results.iter().filter(|r| !r.skipped) {
        let Some(&i) = index.get(&r.id) else { continue };
        let kept: Vec<_> =
            r.geometry_fallbacks.iter().filter(|e| e.kept()).filter_map(|e| Some((e, e.result_step.clone()?, e.operands.clone()?))).collect();
        if kept.is_empty() {
            continue;
        }
        let intent = intent(doc, i);
        for (e, step, operands) in kept {
            out.push(ProviderResult {
                feature: r.id,
                provider: e.provider.clone(),
                version: e.provider_version.clone(),
                operation: e.operation.clone(),
                intent: intent.clone(),
                operands,
                step,
            });
        }
    }
    // Results saved for features not evaluated now (suppressed, rolled back) are kept, not lost.
    for r in &doc.provider_results {
        if index.contains_key(&r.feature) && !evaluated.contains(&r.feature) {
            out.push(r.clone());
        }
    }
    out.truncate(MAX_RESULTS);
    out
}

/// Why feature `id` of `doc` may have failed where it was built before: the design saved a result
/// a provider made for it, no provider is installed here, and the saved result is not for what the
/// feature is given now.
pub fn missing_provider_note(doc: &Document, id: u64) -> Option<String> {
    if kernel::provider::current().is_some() {
        return None;
    }
    let r = doc.provider_results.iter().find(|r| r.feature == id && plain(&r.provider) && plain(&r.version))?;
    Some(format!(
        "this feature was built by the {provider} geometry provider ({version}), which is not installed here, and the result the design \
         saved was made from other inputs: rebuilding it needs the {provider} geometry provider",
        provider = r.provider,
        version = r.version
    ))
}
