//! `tag <KEY> <VALUE> stats` → taginfo `/api/4/tag/stats` (design §4.4): four
//! rows for one `key=value` — `all`, `nodes`, `ways`, `relations` — each with a
//! count and the fraction of that object type. A tag has no distinct values, so
//! (unlike key/stats) there is no `values` column.

use crate::cli::Cli;
use crate::model::TagStatRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::Result;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str, value: &str) -> Result<()> {
    let rows = rows(ctx, key, value)?;
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The four `all`/`nodes`/`ways`/`relations` rows for one tag, or an empty vec
/// for an unknown tag. Split from [`run`] for testing.
pub(crate) fn rows(ctx: &Ctx, key: &str, value: &str) -> Result<Vec<TagStatRow>> {
    let tq = ctx.taginfo()?;

    let Some(v) = tq.kv(key.as_bytes(), value.as_bytes()) else {
        return Ok(Vec::new());
    };

    let c = crate::bbox::value_counts(&v, ctx.bbox_clip.as_ref());
    let count_all = c.nodes + c.ways + c.relations;

    let rows = vec![
        TagStatRow {
            r#type: "all",
            count: count_all,
            count_fraction: fraction(count_all, ctx.totals.objects),
        },
        TagStatRow {
            r#type: "nodes",
            count: c.nodes,
            count_fraction: fraction(c.nodes, ctx.totals.nodes),
        },
        TagStatRow {
            r#type: "ways",
            count: c.ways,
            count_fraction: fraction(c.ways, ctx.totals.ways),
        },
        TagStatRow {
            r#type: "relations",
            count: c.relations,
            count_fraction: fraction(c.relations, ctx.totals.relations),
        },
    ];

    // tag/stats is a fixed 4-row shape; not paged or sorted.
    Ok(rows)
}
