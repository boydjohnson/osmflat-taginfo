//! `key <KEY> stats` → taginfo `/api/4/key/stats` (design §4.4): four rows for
//! one key — `all`, `nodes`, `ways`, `relations` — each with count, fraction,
//! and distinct-value totals.
//!
//! The sidecar stores distinct values per *key*, not per *(key, type)*, so the
//! per-type `values` columns are derived by scanning the key's values and
//! counting those with a non-empty postings list for that type (O(values)).

use crate::cli::Cli;
use crate::model::StatRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::Result;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str) -> Result<()> {
    let rows = rows(ctx, key)?;
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The four `all`/`nodes`/`ways`/`relations` rows for one key, or an empty vec
/// for an unknown key (taginfo returns empty, not an error). Split from [`run`]
/// for testing.
pub(crate) fn rows(ctx: &Ctx, key: &str) -> Result<Vec<StatRow>> {
    let tq = ctx.taginfo()?;

    let Some(k) = tq.key(key.as_bytes()) else {
        return Ok(Vec::new());
    };

    // Per-type counts and distinct-value tallies. A value contributes to a
    // type's tally when it has at least one occurrence (posting, or
    // bbox∩posting) of that type.
    //
    // Under a clip these come from the key-level postings plus a single
    // existence pass, not a per-value count: `value_counts` per value costs
    // `O(values · ranges)`, which put `key addr:street stats --bbox` (810k
    // values, ~30k ranges) at ~130s.
    let (c, t) = match ctx.bbox_clip.as_ref() {
        Some(clip) => (
            k.counts_within(&clip.node_ranges, &clip.way_ranges, &clip.relation_ranges),
            k.value_tallies_within(&clip.node_ranges, &clip.way_ranges, &clip.relation_ranges),
        ),
        // Archive-wide there is nothing to clip, so the per-type tallies are
        // just which postings lists are non-empty -- O(1) per value.
        None => {
            let mut t = osmflat_ext::taginfo::ValueTallies::default();
            for v in k.values() {
                let (n, w, r) = (
                    !v.nodes().is_empty(),
                    !v.ways().is_empty(),
                    !v.relations().is_empty(),
                );
                t.nodes += n as u64;
                t.ways += w as u64;
                t.relations += r as u64;
                t.any += (n || w || r) as u64;
            }
            (k.counts(), t)
        }
    };
    let (v_nodes, v_ways, v_rels, v_all) = (t.nodes, t.ways, t.relations, t.any);
    let count_all = c.nodes + c.ways + c.relations;
    debug_assert!(ctx.bbox.is_some() || v_all == k.distinct_values());

    let rows = vec![
        StatRow {
            r#type: "all",
            count: count_all,
            count_fraction: fraction(count_all, ctx.totals.objects),
            values: v_all,
        },
        StatRow {
            r#type: "nodes",
            count: c.nodes,
            count_fraction: fraction(c.nodes, ctx.totals.nodes),
            values: v_nodes,
        },
        StatRow {
            r#type: "ways",
            count: c.ways,
            count_fraction: fraction(c.ways, ctx.totals.ways),
            values: v_ways,
        },
        StatRow {
            r#type: "relations",
            count: c.relations,
            count_fraction: fraction(c.relations, ctx.totals.relations),
            values: v_rels,
        },
    ];

    // key/stats is a fixed 4-row shape; it is not paged or sorted.
    Ok(rows)
}
