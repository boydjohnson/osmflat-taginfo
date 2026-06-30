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

    let c = k.counts();
    let count_all = c.nodes + c.ways + c.relations;

    // Per-type distinct-value counts: one value contributes to a type's tally
    // when it has at least one posting of that type.
    let (mut v_nodes, mut v_ways, mut v_rels) = (0u64, 0u64, 0u64);
    let mut v_all = 0u64;
    for v in k.values() {
        v_all += 1;
        let vc = v.counts();
        v_nodes += (vc.nodes > 0) as u64;
        v_ways += (vc.ways > 0) as u64;
        v_rels += (vc.relations > 0) as u64;
    }
    debug_assert_eq!(v_all, k.distinct_values());

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
