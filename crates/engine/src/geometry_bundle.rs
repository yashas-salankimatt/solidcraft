//! Replay bundles (`docs/geometry-providers.md`): the operations a feature or a command hands to
//! a geometry provider because SolidCraft's kernel cannot do them, written to a folder so they can
//! be tried again without the design: in SolidCraft, in a provider program, or anywhere else.
//!
//! A bundle holds each operation's request (what was asked, and its operands as STEP) and never
//! the design: no sketches, parameters, other bodies or file names. The operands are bodies of the
//! design all the same, so a bundle is written only where it is asked to be and is never sent
//! anywhere.
//!
//! ```text
//! bundle.json            what the bundle is: format, app, source, provider, requests, error
//! request-1.json         the request line, as a provider program reads it on standard input
//! request-1-target.step  its operands, to open on their own
//! request-1-tool.step
//! request-1-target.scbrep  and in SolidCraft's own encoding, so a replay builds the very bodies
//! request-1-tool.scbrep    the kernel failed on (STEP refits their intersection curves)
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use solidcraft_doc::{Document, Model};
use solidcraft_io::vfs;
use solidcraft_kernel::provider::{GeometryProvider, ProviderReply, ProviderRequest, with_provider};

use crate::Session;

/// The bundle format.
pub const FORMAT: &str = "solidcraft-geometry-fallback/1";

/// Most requests one bundle keeps (a feature hands over a handful at most).
const MAX_REQUESTS: usize = 64;

/// Most bytes of operands one bundle keeps.
const MAX_OPERAND_BYTES: usize = 256 << 20;

/// Most bytes of a `bundle.json` read back.
const MAX_INDEX_BYTES: usize = 1 << 20;

/// Most bytes of a request read back.
const MAX_REQUEST_BYTES: usize = 2 * MAX_OPERAND_BYTES;

/// A provider that keeps the requests it is asked, then answers as the provider it stands in
/// front of does (or fails, when there is none).
struct Recorder {
    inner: Option<Arc<dyn GeometryProvider>>,
    requests: Mutex<Vec<ProviderRequest>>,
    bytes: AtomicUsize,
}

impl Recorder {
    fn new(inner: Option<Arc<dyn GeometryProvider>>) -> Arc<Self> {
        Arc::new(Recorder { inner, requests: Mutex::new(Vec::new()), bytes: AtomicUsize::new(0) })
    }

    fn count(&self) -> usize {
        self.requests.lock().map_or(0, |r| r.len())
    }

    /// The requests kept after the first `skip`.
    fn since(&self, skip: usize) -> Vec<ProviderRequest> {
        self.requests.lock().map(|r| r.iter().skip(skip).cloned().collect()).unwrap_or_default()
    }
}

impl GeometryProvider for Recorder {
    fn id(&self) -> &str {
        self.inner.as_deref().map_or("none", |p| p.id())
    }

    fn version(&self) -> &str {
        self.inner.as_deref().map_or("", |p| p.version())
    }

    fn wants_native_operands(&self) -> bool {
        true
    }

    fn run(&self, request: &ProviderRequest) -> Result<ProviderReply, String> {
        let size: usize = request.operands.iter().map(String::len).chain(request.native.iter().map(Vec::len)).sum();
        if let Ok(mut kept) = self.requests.lock()
            && kept.len() < MAX_REQUESTS
            && self.bytes.load(Ordering::Relaxed).saturating_add(size) <= MAX_OPERAND_BYTES
        {
            kept.push(request.clone());
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }
        match &self.inner {
            // The provider gets the request as a program would: its STEP operands only.
            Some(p) => p.run(&ProviderRequest { native: Vec::new(), ..request.clone() }),
            None => Err("no geometry provider is installed".into()),
        }
    }
}

/// The requests evaluating `doc` hands over, evaluated afresh in front of the current provider.
fn evaluated(doc: &Document, recorder: &Arc<Recorder>) {
    let provider: Arc<dyn GeometryProvider> = recorder.clone();
    with_provider(Some(provider), || {
        let mut model = Model::default();
        model.no_cache = true;
        model.evaluate(doc);
    });
}

/// The requests feature `id` of `doc` hands over: those of the timeline through it, less those of
/// the features before it.
pub fn feature_requests(doc: &Document, id: u64) -> Result<Vec<ProviderRequest>, String> {
    let i = doc.features.iter().position(|f| f.id == id).ok_or_else(|| format!("no feature {id}"))?;
    let current = solidcraft_kernel::provider::current();
    let (before, through) = (Recorder::new(current.clone()), Recorder::new(current));
    let (mut a, mut b) = (doc.clone(), doc.clone());
    a.marker = Some(i);
    b.marker = Some(i + 1);
    evaluated(&a, &before);
    evaluated(&b, &through);
    Ok(through.since(before.count()))
}

/// The requests `command` hands over when it runs on `doc` (as saved: no sketch being edited),
/// and what it replied.
pub fn command_requests(doc: &Document, command: &str, params: &Value) -> (Vec<ProviderRequest>, Result<Value, String>) {
    let recorder = Recorder::new(solidcraft_kernel::provider::current());
    let provider: Arc<dyn GeometryProvider> = recorder.clone();
    with_provider(Some(provider), || {
        let mut session = Session::new(doc.clone());
        let before = recorder.count();
        let reply = session.execute(command, params).map_err(|e| e.to_string());
        (recorder.since(before), reply)
    })
}

/// Write a bundle of `requests` into `folder` (created when missing), through the file access the
/// session allows. `source` says what they came from; `error` is what that said, if anything.
pub fn write(folder: &str, source: Value, requests: &[ProviderRequest], error: Option<&str>) -> Result<Value, String> {
    if requests.is_empty() {
        return Err("nothing to bundle: SolidCraft's kernel did every operation (no operation was handed to a geometry provider)".into());
    }
    let path = |name: &str| std::path::Path::new(folder).join(name).to_string_lossy().into_owned();
    let put = |name: &str, bytes: &[u8]| vfs::write(&path(name), bytes).map_err(|e| format!("{}: {e}", path(name)));
    vfs::create_dir_all(folder).map_err(|e| format!("{folder}: {e}"))?;
    let mut entries = Vec::new();
    for (k, request) in requests.iter().enumerate() {
        let n = k + 1;
        let mut line = serde_json::to_value(request).map_err(|e| e.to_string())?;
        if let Some(o) = line.as_object_mut() {
            o.insert("protocol".into(), json!(crate::geometry_provider::PROTOCOL));
        }
        let file = format!("request-{n}.json");
        put(&file, format!("{line}\n").as_bytes())?;
        // A Boolean's operands are its target and tool; a fillet's or chamfer's one is the body.
        let roles: &[&str] = if request.operands.len() == 1 { &["body"] } else { &["target", "tool"] };
        let mut steps = Vec::new();
        for (role, step) in roles.iter().zip(&request.operands) {
            let name = format!("request-{n}-{role}.step");
            put(&name, step.as_bytes())?;
            steps.push(name);
        }
        let mut natives = Vec::new();
        for (role, bytes) in roles.iter().zip(&request.native) {
            let name = format!("request-{n}-{role}.scbrep");
            put(&name, bytes)?;
            natives.push(name);
        }
        entries.push(json!({"file": file, "operation": request.operation.label(), "operands": steps, "native": natives}));
    }
    let provider = solidcraft_kernel::provider::current().map(|p| json!({"provider": p.id(), "version": p.version()}));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let index = json!({
        "format": FORMAT,
        "app": format!("solidcraft {}", env!("CARGO_PKG_VERSION")),
        "created": crate::crash::utc(now),
        "source": source,
        "provider": provider,
        "error": error,
        "requests": entries,
        "note": "Holds only the operands of these operations, as STEP, and what was asked of them: not the design. \
                 Replay it with the geometry.fallback_replay command, or pipe a request-N.json into a provider program.",
    });
    put("bundle.json", serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?.as_bytes())?;
    Ok(json!({"folder": folder, "requests": entries.len(), "bundle": index}))
}

/// Replay the bundle in `folder`: each request in SolidCraft's kernel and, when the kernel fails,
/// in the current provider.
pub fn replay(folder: &str) -> Result<Value, String> {
    let path = |name: &str| std::path::Path::new(folder).join(name).to_string_lossy().into_owned();
    let read = |name: &str, max: usize| vfs::read_to_string(&path(name), max).map_err(|e| format!("{}: {e}", path(name)));
    let index: Value = serde_json::from_str(&read("bundle.json", MAX_INDEX_BYTES)?).map_err(|e| format!("bundle.json: {e}"))?;
    if index.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!("{folder} is not a geometry fallback bundle ({FORMAT})"));
    }
    let provider = solidcraft_kernel::provider::current();
    let mut replays = Vec::new();
    for entry in index.get("requests").and_then(Value::as_array).into_iter().flatten().take(MAX_REQUESTS) {
        let Some(file) = entry.get("file").and_then(Value::as_str).filter(|f| is_plain_name(f)) else {
            return Err("bundle.json names a request file that is not a plain file name".into());
        };
        let mut request: ProviderRequest = serde_json::from_str(&read(file, MAX_REQUEST_BYTES)?).map_err(|e| format!("{file}: {e}"))?;
        for name in entry.get("native").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(2) {
            if !is_plain_name(name) {
                return Err("bundle.json names a native operand that is not a plain file name".into());
            }
            request.native.push(vfs::read(&path(name), MAX_OPERAND_BYTES).map_err(|e| format!("{}: {e}", path(name)))?);
        }
        let replayed = solidcraft_kernel::provider::replay(&request, provider.as_deref()).map_err(|e| format!("{file}: {e}"))?;
        let mut v = serde_json::to_value(&replayed).map_err(|e| e.to_string())?;
        if let Some(o) = v.as_object_mut() {
            o.insert("file".into(), json!(file));
        }
        replays.push(v);
    }
    Ok(json!({"folder": folder, "provider": crate::geometry_provider::status(), "replays": replays}))
}

/// A file name of the bundle's own folder: no separators, no `..`.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

/// A default folder for a new bundle, next to the recovery and crash folders.
pub fn default_folder(name: &str) -> Option<String> {
    let base = crate::recovery::default_dir()?.parent()?.join("geometry-fallbacks");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let clean: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(40).collect();
    Some(base.join(format!("{}-{clean}", crate::crash::stamp(now))).to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_files_are_plain_names() {
        assert!(is_plain_name("request-1.json"));
        for bad in ["", ".", "..", "../x", "a/b", "a\\b", "C:x"] {
            assert!(!is_plain_name(bad), "{bad}");
        }
    }
}
