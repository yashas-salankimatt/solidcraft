//! Geometry providers (docs/geometry-providers.md): which one SolidCraft hands the Booleans its
//! kernel cannot do to.

use serde_json::Value;

use super::CommandSpec;
use crate::{Result, Session};

pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new("geometry.providers", "Geometry Providers", providers).icon("inspect").noundo().params(
    "The geometry provider SolidCraft hands a feature's Boolean to when its own kernel cannot do it (docs/geometry-providers.md): \
     {installed: true, provider, version, program, protocol, operations}, or {installed: false, message} saying why there is none \
     (the browser build never has one)",
)];

fn providers(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(crate::geometry_provider::status())
}
