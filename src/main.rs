//! `osmflat-taginfo` — a taginfo.openstreetmap.org-compatible CLI over an
//! osmflat archive and its osmflat-ext `Taginfo` sidecar.
//!
//! This binary is a pure reader/formatter: it opens the parent + sidecar, runs
//! queries through `osmflat_ext::taginfo`, and prints taginfo v4-shaped JSON.
//! See `osmflat-taginfo-design.md`.

mod cli;
mod endpoints;
mod freshness;
mod model;
mod open;
mod output;
mod util;

use clap::Parser;
use cli::{Cli, Command, KeyVerb};
use open::Ctx;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ctx = Ctx::open(&cli.archive, &cli.ext)?;

    match &cli.command {
        Command::Keys(args) => endpoints::keys::run(&cli, &ctx, args),
        Command::Key(args) => match args.verb {
            // No verb defaults to `stats`, mirroring the website's key landing.
            None | Some(KeyVerb::Stats) => endpoints::key_stats::run(&cli, &ctx, &args.key),
        },
    }
}
