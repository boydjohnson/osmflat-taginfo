//! `osmflat-taginfo` — a taginfo.openstreetmap.org-compatible CLI over an
//! osmflat archive and its osmflat-ext `Taginfo` sidecar.
//!
//! This binary is a pure reader/formatter: it opens the parent + sidecar, runs
//! queries through `osmflat_ext::taginfo`, and prints taginfo v4-shaped JSON.
//! See `osmflat-taginfo-design.md`.

mod bbox;
mod cli;
mod endpoints;
mod freshness;
mod model;
mod open;
mod output;
#[cfg(feature = "serve")]
mod routes;
#[cfg(feature = "serve")]
mod serve;
mod util;

#[cfg(all(test, feature = "serve"))]
mod serve_tests;
#[cfg(test)]
mod tests;

use clap::Parser;
use cli::{Cli, Command, KeyVerb, TagVerb};
use open::Ctx;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    #[cfg(feature = "serve")]
    if let Command::Serve(args) = &cli.command {
        return run_serve(&cli, args);
    }

    let bbox = cli::parse_bbox(&cli)?;
    let ctx = Ctx::open(&cli.archive, &cli.ext, bbox)?;

    match &cli.command {
        Command::Keys(args) => endpoints::keys::run(&cli, &ctx, args),
        Command::Key(args) => match args.verb {
            // No verb defaults to `stats`, mirroring the website's key landing.
            None | Some(KeyVerb::Stats) => endpoints::key_stats::run(&cli, &ctx, &args.key),
            Some(KeyVerb::Values) => endpoints::key_values::run(&cli, &ctx, &args.key),
            Some(KeyVerb::Combinations) => endpoints::key_combinations::run(&cli, &ctx, &args.key),
        },
        Command::Tag(args) => {
            let (key, value) = util::split_tag(&args.key, args.value.as_deref())?;
            match args.verb {
                None | Some(TagVerb::Stats) => endpoints::tag_stats::run(&cli, &ctx, &key, &value),
                Some(TagVerb::Combinations) => {
                    endpoints::tag_combinations::run(&cli, &ctx, &key, &value)
                }
            }
        }
        // Handled by the early return above, before `ctx` is even built.
        #[cfg(feature = "serve")]
        Command::Serve(_) => unreachable!(),
    }
}

#[cfg(feature = "serve")]
fn run_serve(cli: &Cli, args: &cli::ServeArgs) -> anyhow::Result<()> {
    let archive = cli
        .archive
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing --archive (parent osmflat archive directory)"))?;
    let ext = cli
        .ext
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing --ext (Ext sidecar directory)"))?;
    let opened = open::OpenedArchive::open(archive, ext)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve::run(opened, &args.bind_addr))
}
