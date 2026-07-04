//! Command-line surface (clap derive). Noun/verb layout mirroring taginfo's
//! information architecture; see `osmflat-taginfo-design.md` §3.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};
use osmflat_ext::query::Bbox;
use std::path::PathBuf;

/// A taginfo.openstreetmap.org-compatible CLI over an osmflat archive and its
/// osmflat-ext `Taginfo` sidecar.
///
/// Counts come from the loaded extract (not the live planet), and fields osmflat
/// has no source for — `users_all`, `in_wiki`, `projects`, value descriptions —
/// are emitted as documented neutral stubs. Strings are rendered lossy UTF-8.
#[derive(Parser, Debug)]
#[command(
    name = "osmflat-taginfo",
    version,
    about,
    allow_negative_numbers = true
)]
pub struct Cli {
    /// Parent osmflat archive directory.
    #[arg(short = 'a', long, global = true)]
    pub archive: Option<PathBuf>,

    /// Sibling Ext sidecar directory (built with `osmflat-extc --taginfo`).
    #[arg(short = 'x', long, global = true)]
    pub ext: Option<PathBuf>,

    /// Restrict all counts to entities overlapping this box (lon/lat degrees):
    /// `MINX MINY MAXX MAXY`.
    #[arg(long, global = true, num_args = 4, value_names = ["MINX", "MINY", "MAXX", "MAXY"])]
    pub bbox: Option<Vec<f64>>,

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
    /// Inspect a single `key=value` tag.
    Tag(TagArgs),
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
    /// Distinct values of the key with counts (taginfo /api/4/key/values).
    Values,
    /// Other keys co-occurring with this key (taginfo /api/4/key/combinations).
    ///
    /// Requires a sidecar built with `osmflat-extc --combinations`.
    Combinations,
}

#[derive(clap::Args, Debug)]
pub struct TagArgs {
    /// The tag key — or the whole `KEY=VALUE` as one token (then omit VALUE).
    pub key: String,
    /// The tag value (omit if KEY was given as `KEY=VALUE`).
    pub value: Option<String>,
    #[command(subcommand)]
    pub verb: Option<TagVerb>,
}

#[derive(Subcommand, Debug)]
pub enum TagVerb {
    /// Per-type counts for the tag (taginfo /api/4/tag/stats).
    Stats,
    /// Other tags co-occurring with this tag (taginfo /api/4/tag/combinations).
    ///
    /// Requires a sidecar built with `osmflat-extc --combinations`.
    Combinations,
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

/// Validate and convert `--bbox`'s four raw floats into a [`Bbox`].
/// `num_args = 4` guarantees the length; this only checks ordering.
pub fn parse_bbox(cli: &Cli) -> Result<Option<Bbox>> {
    let Some(v) = &cli.bbox else {
        return Ok(None);
    };
    let (min_lon, min_lat, max_lon, max_lat) = (v[0], v[1], v[2], v[3]);
    if min_lon > max_lon || min_lat > max_lat {
        bail!(
            "invalid --bbox {min_lon} {min_lat} {max_lon} {max_lat}: \
             MINX must be <= MAXX and MINY must be <= MAXY"
        );
    }
    Ok(Some(Bbox {
        min_lon,
        min_lat,
        max_lon,
        max_lat,
    }))
}
