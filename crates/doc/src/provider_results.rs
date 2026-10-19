//! Results geometry providers made for a design's features, saved with the design
//! (`docs/geometry-providers.md`), so a computer without the provider (the browser, a machine
//! without OCCT) still shows, measures and exports them, and says which feature needs the provider
//! to be rebuilt.
//!
//! A saved result is a cache, not the design: the feature keeps its parameters and inputs, and its
//! result is rebuilt whenever it can be. A saved one is used only for exactly the operands it was
//! made from, and only after the checks a provider's answer gets (`solidcraft_kernel::provider`).
//! Nothing native to a provider is saved: the result is STEP text.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use solidcraft_kernel as kernel;
use solidcraft_kernel::provider::{SavedResult, SavedResults};

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
    /// The operation's request digest (`kernel::provider::request_digest`): the operands it was
    /// made from.
    pub request: String,
    /// The result, as STEP text (a deduplicated blob in a container).
    pub step: Arc<str>,
}

/// A name or version as a provider gives it: short, printable.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control)
}

/// The saved results to offer an evaluation of `doc`, by request digest; `None` when it has none.
/// Entries that are too large, too many or malformed are left out (the design file is hostile).
pub fn offered(doc: &Document) -> Option<Arc<SavedResults>> {
    let mut bytes = 0usize;
    let mut out = SavedResults::new();
    for r in doc.provider_results.iter().take(MAX_RESULTS) {
        if !(plain(&r.provider) && plain(&r.version) && r.request.len() == 16 && !r.step.is_empty()) {
            continue;
        }
        bytes = bytes.saturating_add(r.step.len());
        if bytes > MAX_BYTES {
            break;
        }
        out.entry(r.request.clone()).or_insert_with(|| SavedResult {
            provider: r.provider.clone(),
            version: r.version.clone(),
            step: r.step.clone(),
        });
    }
    (!out.is_empty()).then(|| Arc::new(out))
}

/// A digest of what [`offered`] offers for `doc`: 0, and so every evaluation key as before, when
/// it offers nothing.
pub fn digest(doc: &Document) -> u64 {
    use std::hash::{Hash, Hasher};
    if doc.provider_results.is_empty() {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in doc.provider_results.iter().take(MAX_RESULTS) {
        (&r.request, &r.provider, &r.version, r.step.len()).hash(&mut h);
    }
    h.finish() | 1
}

/// The results to save with `doc` once `model` has evaluated it: those its features keep now (made
/// by a provider, or taken as saved), and those saved for features not evaluated now (suppressed,
/// rolled back) so they are not lost.
pub fn to_save(doc: &Document, model: &Model) -> Vec<ProviderResult> {
    let mut out: Vec<ProviderResult> = Vec::new();
    let mut seen: BTreeSet<(u64, String)> = BTreeSet::new();
    let evaluated: BTreeSet<u64> = model.results.iter().filter(|r| !r.skipped).map(|r| r.id).collect();
    for r in model.results.iter().filter(|r| !r.skipped) {
        for e in r.geometry_fallbacks.iter().filter(|e| e.kept()) {
            if let Some(step) = &e.result_step
                && !e.request.is_empty()
                && seen.insert((r.id, e.request.clone()))
            {
                out.push(ProviderResult {
                    feature: r.id,
                    provider: e.provider.clone(),
                    version: e.provider_version.clone(),
                    operation: e.operation.clone(),
                    request: e.request.clone(),
                    step: step.clone(),
                });
            }
        }
    }
    let live: BTreeSet<u64> = doc.features.iter().map(|f| f.id).collect();
    for r in &doc.provider_results {
        if live.contains(&r.feature) && !evaluated.contains(&r.feature) && seen.insert((r.feature, r.request.clone())) {
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
