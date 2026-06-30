//! One module per taginfo endpoint. Each exposes a pure
//! `run(cli, ctx, args) -> Result<()>` that queries, assembles the typed rows,
//! and hands them to [`crate::output::emit`]. Shaped so a future `serve` mode
//! can reuse the same functions behind the real taginfo URL routes.

pub mod key_stats;
pub mod keys;
