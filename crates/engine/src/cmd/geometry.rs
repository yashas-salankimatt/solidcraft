//! Geometry providers (docs/geometry-providers.md): which one SolidCraft hands the Booleans its
//! kernel cannot do to, and replay bundles of those operations.

use serde_json::{Value, json};

use super::CommandSpec;
use crate::params::{bad, str_};
use crate::{EngineError, Result, Session, geometry_bundle};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("geometry.providers", "Geometry Providers", providers).icon("inspect").noundo().params(
        "The geometry provider SolidCraft hands a feature's Boolean to when its own kernel cannot do it (docs/geometry-providers.md): \
         {installed: true, provider, version, program, protocol, operations}, or {installed: false, message} saying why there is none \
         (the browser build never has one)",
    ),
    CommandSpec::new("geometry.fallback_bundle", "Export Geometry Fallback Bundle", bundle).icon("inspect").noundo().params(
        "feature: id|name, the operations that feature hands to a geometry provider; or command: id and params?: {}, the operations \
         that command would hand over, tried on a copy of the design (as saved: no sketch being edited; the design itself is left as \
         it is). folder?: where to write it (default: a new folder beside the crash reports). Writes each operation's request with its \
         operands as STEP, never the design: {folder, requests, bundle}. Works without a provider too: then it bundles the operations \
         SolidCraft's kernel could not do",
    ),
    CommandSpec::new("geometry.fallback_replay", "Replay Geometry Fallback Bundle", replay).icon("inspect").noundo().params(
        "folder: a bundle's folder (geometry.fallback_bundle). Replays each request in SolidCraft's kernel and, where it fails, in the \
         installed geometry provider, checked as a feature's would be: {folder, provider, replays: [{file, operation, kernel_error?, \
         kernel_ms, fallback?}]}",
    ),
];

fn providers(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(crate::geometry_provider::status())
}

fn bundle(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "geometry.fallback_bundle";
    let (source, requests, error, name) = match (p.get("feature"), str_(p, "command")) {
        (Some(key), None) => {
            let key = match key {
                Value::Number(n) => n.to_string(),
                Value::String(x) => x.clone(),
                _ => return Err(bad(cmd, "`feature` must be an id or name")),
            };
            let f = s.doc.find_feature(&key).ok_or_else(|| bad(cmd, format!("no feature `{key}`")))?;
            let (id, name, kind) = (f.id, f.name.clone(), f.kind.type_name());
            let requests = geometry_bundle::feature_requests(&s.doc, id).map_err(|e| bad(cmd, e))?;
            let error = s.model.result(id).and_then(|r| r.error.clone());
            (json!({"feature": {"id": id, "name": name, "type": kind}}), requests, error, name)
        }
        (None, Some(command)) => {
            // Only what builds geometry: never a file, a design reload or another bundle.
            let builds = super::find_command(command).is_some_and(|c| c.undoable)
                && !["file.", "doc.", "geometry.", "diagnostics.", "app.", "prefs.", "render."].iter().any(|a| command.starts_with(a));
            if !builds {
                return Err(bad(cmd, format!("`{command}` is not a command that builds geometry")));
            }
            let params = p.get("params").cloned().unwrap_or_else(|| json!({}));
            let (requests, reply) = geometry_bundle::command_requests(&s.doc, command, &params);
            (json!({"command": {"id": command, "params": params}}), requests, reply.err(), command.to_string())
        }
        _ => return Err(bad(cmd, "give `feature`, or `command` (and its `params`)")),
    };
    let folder = match str_(p, "folder") {
        Some(f) if !f.trim().is_empty() => f.to_string(),
        _ => geometry_bundle::default_folder(&name).ok_or_else(|| bad(cmd, "no folder for bundles on this system: give `folder`"))?,
    };
    geometry_bundle::write(&folder, source, &requests, error.as_deref()).map_err(|e| EngineError::Other(format!("{cmd}: {e}")))
}

fn replay(_s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "geometry.fallback_replay";
    let folder = str_(p, "folder").filter(|f| !f.trim().is_empty()).ok_or_else(|| bad(cmd, "`folder` must name a bundle's folder"))?;
    geometry_bundle::replay(folder).map_err(|e| EngineError::Other(format!("{cmd}: {e}")))
}
