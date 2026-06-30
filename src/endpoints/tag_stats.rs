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
    let tq = ctx.taginfo()?;

    let Some(v) = tq.kv(key.as_bytes(), value.as_bytes()) else {
        // Unknown tag → empty result, not an error (matches taginfo).
        return output::emit::<TagStatRow>(cli, &ctx.data_until, util::url(cli), Vec::new());
    };

    let c = v.counts();
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
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}
