//! `keys` → taginfo `/api/4/keys/all` (design §4.2): every key (optionally
//! prefix-filtered) with per-type counts, fractions, and distinct-value totals.

use crate::cli::{Cli, KeysArgs, Order};
use crate::model::KeyRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::{bail, Result};
use osmflat_ext::taginfo::KeyView;
use std::cmp::Ordering;

pub fn run(cli: &Cli, ctx: &Ctx, args: &KeysArgs) -> Result<()> {
    let rows = rows(
        ctx,
        args.search.as_deref(),
        cli.sortname.as_deref(),
        cli.sortorder,
    )?;
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The full, sorted keys table (pre-pagination). Split from [`run`] so tests can
/// assert on the rows without constructing a [`Cli`] or printing.
pub(crate) fn rows(
    ctx: &Ctx,
    search: Option<&str>,
    sortname: Option<&str>,
    sortorder: Order,
) -> Result<Vec<KeyRow>> {
    let tq = ctx.taginfo()?;

    // The query layer returns keys sorted by string; we collect, then sort by
    // the requested field below.
    let mut rows: Vec<KeyRow> = match search {
        Some(prefix) => tq
            .keys_with_prefix(prefix.as_bytes())
            .map(|k| row(ctx, &k))
            .collect(),
        None => tq.keys().map(|k| row(ctx, &k)).collect(),
    };

    // Under a bbox clip, a key with no in-scope occurrences is noise: drop it
    // instead of listing every archive-wide key at `count_all: 0`.
    if ctx.bbox.is_some() {
        rows.retain(|r| r.count_all > 0);
    }

    sort(&mut rows, sortname, sortorder)?;
    Ok(rows)
}

fn row(ctx: &Ctx, k: &KeyView) -> KeyRow {
    let summary = crate::bbox::key_summary(k, ctx.bbox_clip.as_ref());
    let c = summary.counts;
    let count_all = c.nodes + c.ways + c.relations;
    KeyRow {
        key: util::lossy(k.key()),
        count_all,
        count_all_fraction: fraction(count_all, ctx.totals.objects),
        count_nodes: c.nodes,
        count_nodes_fraction: fraction(c.nodes, ctx.totals.nodes),
        count_ways: c.ways,
        count_ways_fraction: fraction(c.ways, ctx.totals.ways),
        count_relations: c.relations,
        count_relations_fraction: fraction(c.relations, ctx.totals.relations),
        values_all: summary.distinct_values,
        users_all: None,
        in_wiki: None,
        projects: None,
    }
}

/// taginfo's keys table defaults to `count_all desc`. Sorts on the chosen field
/// with a deterministic key-string tie-break, then applies the direction.
fn sort(rows: &mut [KeyRow], sortname: Option<&str>, sortorder: Order) -> Result<()> {
    let field = sortname.unwrap_or("count_all");
    let primary = |r: &KeyRow| -> u64 {
        match field {
            "count_all" => r.count_all,
            "count_nodes" => r.count_nodes,
            "count_ways" => r.count_ways,
            "count_relations" => r.count_relations,
            "values_all" => r.values_all,
            _ => 0, // "key" sorts on the string tie-break alone
        }
    };
    if !matches!(
        field,
        "count_all" | "count_nodes" | "count_ways" | "count_relations" | "values_all" | "key"
    ) {
        bail!(
            "unknown --sortname {field:?} for `keys`; allowed: \
             count_all, count_nodes, count_ways, count_relations, values_all, key"
        );
    }

    rows.sort_by(|a, b| match primary(a).cmp(&primary(b)) {
        Ordering::Equal => a.key.cmp(&b.key),
        ord => ord,
    });
    if sortorder == Order::Desc {
        rows.reverse();
    }
    Ok(())
}
