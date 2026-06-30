//! Small shared helpers.

use crate::cli::Cli;

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
