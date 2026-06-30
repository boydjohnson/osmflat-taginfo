//! Small shared helpers.

use crate::cli::Cli;
use anyhow::{bail, Result};

/// Render a parent-stringtable byte slice as a `String`, lossy (OSM strings are
/// conventionally UTF-8 but not guaranteed; design §2).
pub fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The canonical CLI invocation for the envelope's `url` field — taginfo puts a
/// request URL there; we record the equivalent command (design §4.1).
pub fn url(_cli: &Cli) -> String {
    std::env::args().collect::<Vec<_>>().join(" ")
}

/// Resolve a tag from either two positionals (`KEY VALUE`) or a single
/// `KEY=VALUE` token (design §3.2). The two-arg form wins when both are present.
pub fn split_tag(key: &str, value: Option<&str>) -> Result<(String, String)> {
    if let Some(value) = value {
        return Ok((key.to_string(), value.to_string()));
    }
    match key.split_once('=') {
        Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
        _ => bail!("expected `KEY VALUE` or `KEY=VALUE`, got {key:?}"),
    }
}
