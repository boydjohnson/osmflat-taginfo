//! Command-line surface (clap derive). Noun/verb layout mirroring taginfo's
//! information architecture; see `osmflat-taginfo-design.md` §3.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// A taginfo.openstreetmap.org-compatible CLI over an osmflat archive and its
/// osmflat-ext `Taginfo` sidecar.
///
/// Counts come from the loaded extract (not the live planet), and fields osmflat
/// has no source for — `users_all`, `in_wiki`, `projects`, value descriptions —
/// are emitted as documented neutral stubs. Strings are rendered lossy UTF-8.
#[derive(Parser, Debug)]
#[command(name = "osmflat-taginfo", version, about)]
pub struct Cli {
    /// Parent osmflat archive directory.
    #[arg(short = 'a', long, global = true)]
    pub archive: Option<PathBuf>,

    /// Sibling Ext sidecar directory (built with `osmflat-extc --taginfo`).
    #[arg(short = 'x', long, global = true)]
    pub ext: Option<PathBuf>,

    /// Output format.
    #[arg(long, global = true, default_value = "pretty", value_enum)]
    pub format: Format,

    /// 1-based result page (taginfo pagination).
    #[arg(long, global = true, default_value_t = 1)]
    pub page: usize,

    /// Rows per page; 0 means all rows.
    #[arg(long, global = true, default_value_t = 0)]
    pub rp: usize,

    /// Sort field (allowed set depends on the subcommand).
    #[arg(long, global = true)]
    pub sortname: Option<String>,

    /// Sort direction.
    #[arg(long, global = true, default_value = "desc", value_enum)]
    pub sortorder: Order,

    /// Emit the bare `data` array, dropping the taginfo envelope.
    #[arg(long, global = true)]
    pub no_envelope: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// The keys table — every key with per-type counts (taginfo /api/4/keys/all).
    Keys(KeysArgs),
    /// Inspect a single key.
    Key(KeyArgs),
}

#[derive(clap::Args, Debug)]
pub struct KeysArgs {
    /// Restrict to keys sharing this string prefix (taginfo's key search).
    #[arg(long)]
    pub search: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct KeyArgs {
    /// The key to inspect.
    pub key: String,
    #[command(subcommand)]
    pub verb: Option<KeyVerb>,
}

#[derive(Subcommand, Debug)]
pub enum KeyVerb {
    /// Per-type counts and distinct-value totals (taginfo /api/4/key/stats).
    Stats,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Compact single-line JSON (the wire-faithful form).
    Json,
    /// Indented JSON.
    Pretty,
    /// Human-aligned table (not a stable interface).
    Table,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Order {
    Asc,
    Desc,
}
