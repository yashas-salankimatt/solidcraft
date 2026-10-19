//! The geometry provider program (`docs/geometry-providers.md`): another kernel, run as a
//! separate program, that a feature's Boolean is handed to when SolidCraft's kernel fails
//! (`solidcraft_kernel::provider`). Nothing of the program is linked: SolidCraft starts it for
//! each operation, writes one request and reads one reply.
//!
//! The program speaks [`PROTOCOL`]. It reads one JSON object, on one line, from standard input,
//! writes one JSON object, on one line, to standard output and exits:
//! - `{"protocol", "op": "hello"}` → `{"ok": true, "provider": "occt", "version": "8.0.1",
//!   "operations": ["boolean"]}`;
//! - `{"protocol", "op": "boolean", "kind": "cut", "operands": [STEP, STEP], "tolerance_mm"}` →
//!   `{"ok": true, "step": STEP}`, or `{"ok": false, "error": "…"}` from any request it cannot do.
//!
//! The reply is the first line of standard output that starts with `{`. No reply, a reply that
//! is not one of these, an exit before replying and a reply after the time limit are errors (the
//! program is stopped at the limit). Standard error is the program's log: its last lines go into
//! the error.
//!
//! The browser build cannot start programs: it never has a provider.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use solidcraft_kernel::provider::{GeometryProvider, ProviderOperation, ProviderReply, ProviderRequest};

/// The protocol a provider program speaks.
pub const PROTOCOL: &str = "solidcraft-geometry-provider/1";

/// The environment variable naming the provider program (`off`, or unset: none).
pub const ENV: &str = "SOLIDCRAFT_GEOMETRY_PROVIDER";

/// Longest an operation may take before the program is stopped.
const OPERATION_LIMIT: Duration = Duration::from_secs(120);

/// Longest the program may take to say who it is.
const HELLO_LIMIT: Duration = Duration::from_secs(10);

/// Most bytes read from the program's standard output (a reply carries a whole STEP file).
#[cfg(not(target_arch = "wasm32"))]
const MAX_REPLY_BYTES: usize = 512 << 20;

/// Most bytes of the program's standard error kept for an error message.
#[cfg(not(target_arch = "wasm32"))]
const MAX_LOG_BYTES: usize = 16 << 10;

/// A provider program: who it said it is, and what it can do.
#[derive(Clone, Debug)]
pub struct ProcessProvider {
    program: PathBuf,
    id: String,
    version: String,
    operations: Vec<String>,
    limit: Duration,
}

impl ProcessProvider {
    /// Start `program` once to ask who it is (`hello`).
    pub fn open(program: &Path) -> Result<Self, String> {
        Self::open_with(program, HELLO_LIMIT, OPERATION_LIMIT)
    }

    fn open_with(program: &Path, hello: Duration, limit: Duration) -> Result<Self, String> {
        let reply = exchange(program, &json!({"protocol": PROTOCOL, "op": "hello"}), hello)?;
        let text = |key: &str| reply.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && s.len() <= 64);
        let id = text("provider").ok_or("its hello reply has no `provider` name (at most 64 characters)")?;
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
            return Err(format!("its name `{id}` has characters other than letters, digits, `-`, `_` and `.`"));
        }
        let version = text("version").ok_or("its hello reply has no `version` (at most 64 characters)")?;
        let operations: Vec<String> =
            reply.get("operations").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(64).map(str::to_string).collect();
        Ok(ProcessProvider { program: program.to_path_buf(), id: id.to_string(), version: version.to_string(), operations, limit })
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The operations it said it can do (`boolean`).
    pub fn operations(&self) -> &[String] {
        &self.operations
    }
}

impl GeometryProvider for ProcessProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn run(&self, request: &ProviderRequest) -> Result<ProviderReply, String> {
        let op = match request.operation {
            ProviderOperation::Boolean { .. } => "boolean",
        };
        if !self.operations.iter().any(|o| o == op) {
            return Err(format!("it does not do `{op}` (it does {})", self.operations.join(", ")));
        }
        let mut message = serde_json::to_value(request).map_err(|e| e.to_string())?;
        if let Some(m) = message.as_object_mut() {
            m.insert("protocol".into(), json!(PROTOCOL));
        }
        let reply = exchange(&self.program, &message, self.limit)?;
        let step = reply.get("step").and_then(Value::as_str).ok_or("its reply has no `step`")?;
        Ok(ProviderReply { step: step.to_string() })
    }

    fn describe(&self) -> Value {
        json!({"program": self.program, "protocol": PROTOCOL, "operations": self.operations})
    }
}

/// Install the provider program [`ENV`] names, for every evaluation in this process, and say which
/// it is (`occt 8.0.1 (/path/to/program)`). `Ok(None)` when it names none (unset, empty or `off`);
/// an error when the program can't be used, in which case nothing is installed and every feature
/// evaluates as it does without one.
pub fn install_from_env() -> Result<Option<String>, String> {
    let named = std::env::var_os(ENV).unwrap_or_default();
    if named.is_empty() || named == "off" {
        solidcraft_kernel::provider::install(None);
        return Ok(None);
    }
    let program = PathBuf::from(named);
    match ProcessProvider::open(&program) {
        Ok(p) => {
            let said = format!("{} {} ({})", p.id, p.version, p.program.display());
            solidcraft_kernel::provider::install(Some(Arc::new(p)));
            Ok(Some(said))
        }
        Err(e) => {
            solidcraft_kernel::provider::install(None);
            Err(format!("{ENV}={}: {e}", program.display()))
        }
    }
}

/// One request to `program` and its reply (see the module docs).
#[cfg(not(target_arch = "wasm32"))]
fn exchange(program: &Path, message: &Value, limit: Duration) -> Result<Value, String> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let mut line = serde_json::to_string(message).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("it could not be started: {e}"))?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("its standard input and output could not be opened".into());
    };
    std::thread::scope(|scope| {
        // A program that exits without reading closes the pipe: its reply (or exit) says why.
        scope.spawn(move || {
            let _ = stdin.write_all(line.as_bytes());
        });
        let out = scope.spawn(move || {
            let mut bytes = Vec::new();
            let read = stdout.take(MAX_REPLY_BYTES as u64 + 1).read_to_end(&mut bytes);
            read.map(|_| bytes)
        });
        let log = scope.spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(4 * MAX_LOG_BYTES as u64).read_to_end(&mut bytes);
            bytes
        });
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if started.elapsed() > limit => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("it took longer than {} s and was stopped", limit.as_secs()));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("it could not be waited for: {e}"));
                }
            }
        };
        let out = out.join().unwrap_or_else(|_| Ok(Vec::new()));
        let log = log.join().unwrap_or_default();
        let tail = log_tail(&log);
        let with_log = |e: String| if tail.is_empty() { e } else { format!("{e} (its log ends: {tail})") };
        let status = status.map_err(with_log)?;
        let out = out.map_err(|e| with_log(format!("its reply could not be read: {e}")))?;
        if out.len() > MAX_REPLY_BYTES {
            return Err(format!("its reply is larger than {} MiB", MAX_REPLY_BYTES >> 20));
        }
        let Some(reply) = String::from_utf8_lossy(&out).lines().map(str::trim).find(|l| l.starts_with('{')).map(str::to_string) else {
            let exit = status.code().map_or_else(|| "was stopped".to_string(), |c| format!("exited with status {c}"));
            return Err(with_log(format!("it {exit} without a reply")));
        };
        let reply: Value = serde_json::from_str(&reply).map_err(|e| with_log(format!("its reply is not JSON: {e}")))?;
        match reply.get("ok").and_then(Value::as_bool) {
            Some(true) => Ok(reply),
            Some(false) => Err(reply.get("error").and_then(Value::as_str).unwrap_or("it gave no reason").chars().take(4096).collect()),
            None => Err(with_log("its reply has no `ok`".into())),
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn exchange(_program: &Path, _message: &Value, _limit: Duration) -> Result<Value, String> {
    Err("the browser build cannot start a geometry provider program".into())
}

/// The last lines of a program's log, on one line.
#[cfg(not(target_arch = "wasm32"))]
fn log_tail(log: &[u8]) -> String {
    let text = String::from_utf8_lossy(log);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines.get(lines.len().saturating_sub(4)..).unwrap_or_default().join(" | ");
    let skip = tail.chars().count().saturating_sub(MAX_LOG_BYTES);
    tail.chars().skip(skip).collect()
}

/// What `geometry.providers` reports: the installed provider, or why there is none.
pub fn status() -> Value {
    match solidcraft_kernel::provider::current() {
        Some(p) => {
            let mut v = json!({"installed": true, "provider": p.id(), "version": p.version()});
            if let (Some(v), Value::Object(more)) = (v.as_object_mut(), p.describe()) {
                v.extend(more);
            }
            v
        }
        None if cfg!(target_arch = "wasm32") => json!({
            "installed": false,
            "message": "the browser build has no geometry provider: a feature SolidCraft's kernel cannot build shows the result a provider made when the design saved one, and fails otherwise",
        }),
        None => json!({
            "installed": false,
            "message": format!("no geometry provider is installed: set {ENV} to a provider program before starting SolidCraft to hand it the Booleans SolidCraft's kernel cannot do"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set in a child of the test binary started as a provider program: how it answers.
    const ROLE: &str = "SOLIDCRAFT_TEST_PROVIDER_ROLE";

    /// The test binary itself, started as a provider program that answers as [`ROLE`] says.
    /// libtest writes lines of its own before the test runs (`--nocapture` lets the test write to
    /// the real standard output and error); the reply is the first line starting with `{`.
    #[test]
    fn act_as_provider() {
        use std::io::{BufRead, Write};
        let Ok(role) = std::env::var(ROLE) else { return };
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        let request: Value = serde_json::from_str(&line).unwrap_or_default();
        assert_eq!(request["protocol"], json!(PROTOCOL));
        let reply = match (role.as_str(), request["op"].as_str()) {
            ("silent", _) => {
                eprintln!("starting\nno licence found");
                std::process::exit(3)
            }
            ("slow", _) => {
                std::thread::sleep(Duration::from_secs(30));
                json!({})
            }
            (_, Some("hello")) if role == "nameless" => json!({"ok": true, "version": "1"}),
            (_, Some("hello")) => json!({"ok": true, "provider": "test-child", "version": "2.1", "operations": ["boolean"]}),
            ("refuse", Some("boolean")) => json!({"ok": false, "error": "no solid in operand 2"}),
            // The join of two crossing cylinders the kernel cannot do, as another kernel gives it.
            ("crossing", Some("boolean")) => json!({"ok": true, "step": crate::tests::geometry_provider::near_answer()}),
            (_, Some("boolean")) => {
                // Echo the target back, with what was asked for in the log.
                eprintln!("{} {} operands", request["kind"], request["operands"].as_array().map_or(0, Vec::len));
                json!({"ok": true, "step": request["operands"][0]})
            }
            _ => json!({"ok": false, "error": "unknown request"}),
        };
        let mut out = std::io::stdout().lock();
        // On a line of its own: libtest has just written the test's name without ending the line.
        let _ = writeln!(out, "\n{reply}");
        let _ = out.flush();
        std::process::exit(0);
    }

    /// The test binary, run with only the test above, as a provider program: a tiny shell
    /// wrapper names it and sets its role.
    #[cfg(unix)]
    fn child(role: &str, tag: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("solidcraft-provider-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let program = dir.join("provider");
        let exe = std::env::current_exe().unwrap();
        let script = format!(
            "#!/bin/sh\n{ROLE}={role} exec '{}' --exact geometry_provider::tests::act_as_provider --test-threads 1 --nocapture\n",
            exe.display()
        );
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    #[cfg(unix)]
    fn request() -> ProviderRequest {
        let operation = ProviderOperation::Boolean { kind: solidcraft_kernel::BoolOp::Cut };
        ProviderRequest { operation, operands: vec!["A".into(), "B".into()], tolerance_mm: 1e-6, native: Vec::new() }
    }

    #[test]
    #[cfg(unix)]
    fn a_provider_program_says_who_it_is_and_answers_requests() {
        let p = ProcessProvider::open(&child("echo", "echo")).unwrap();
        assert_eq!((p.id(), p.version(), p.operations()), ("test-child", "2.1", &["boolean".to_string()][..]));
        assert_eq!(p.run(&request()).unwrap().step, "A");
        assert_eq!(p.describe()["protocol"], json!(PROTOCOL));
        let refusing = ProcessProvider::open(&child("refuse", "refuse")).unwrap();
        assert_eq!(refusing.run(&request()).unwrap_err(), "no solid in operand 2");
    }

    #[test]
    #[cfg(unix)]
    fn a_program_that_cannot_answer_is_an_error_never_a_hang() {
        let missing = ProcessProvider::open(Path::new("/nonexistent/solidcraft-provider")).unwrap_err();
        assert!(missing.contains("could not be started"), "{missing}");
        let silent = ProcessProvider::open(&child("silent", "silent")).unwrap_err();
        assert!(silent.contains("exited with status 3 without a reply") && silent.contains("no licence found"), "{silent}");
        let nameless = ProcessProvider::open(&child("nameless", "nameless")).unwrap_err();
        assert!(nameless.contains("no `provider` name"), "{nameless}");
        let started = std::time::Instant::now();
        let slow = ProcessProvider::open_with(&child("slow", "slow"), Duration::from_millis(500), Duration::from_millis(500)).unwrap_err();
        assert!(slow.contains("was stopped"), "{slow}");
        assert!(started.elapsed() < Duration::from_secs(20), "the program is stopped at the limit");
    }

    /// The whole way: a feature the kernel cannot build, handed to a program, its STEP reply read
    /// back, checked and kept, and the program named in the feature's record.
    #[test]
    #[cfg(unix)]
    fn a_feature_is_built_by_the_provider_program() {
        use crate::tests::geometry_provider::{crossing, first};
        let provider: Arc<dyn GeometryProvider> = Arc::new(ProcessProvider::open(&child("crossing", "crossing")).unwrap());
        solidcraft_kernel::provider::with_provider(Some(provider), || {
            let status = status();
            assert_eq!((status["installed"].clone(), status["provider"].clone()), (json!(true), json!("test-child")), "{status}");
            let mut s = crate::Session::default();
            first(&mut s);
            let reply = s.execute("solid.cylinder", &crossing()).unwrap();
            let id = reply["feature"].as_u64().unwrap();
            let r = s.model.result(id).unwrap();
            assert!(r.error.is_none() && r.warning.as_ref().is_some_and(|w| w.contains("test-child geometry provider")), "{r:?}");
            let e = &r.geometry_fallbacks[0];
            assert_eq!(
                (e.outcome, e.provider.as_str(), e.provider_version.as_str()),
                (solidcraft_kernel::provider::FallbackOutcome::Accepted, "test-child", "2.1")
            );
        });
    }

    #[test]
    fn status_says_what_is_installed() {
        let none = solidcraft_kernel::provider::with_provider(None, status);
        assert_eq!(none["installed"], json!(false));
        assert!(none["message"].as_str().is_some_and(|m| m.contains(ENV)), "{none}");
    }
}
