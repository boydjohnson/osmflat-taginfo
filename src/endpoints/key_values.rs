//! `key <KEY> values` → taginfo `/api/4/key/values` (design §4.3): the distinct
//! values of one key, each with its object count and the fraction of the key's
//! objects that carry it.

use crate::cli::{Cli, Order};
use crate::model::ValueRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::{bail, Result};
use std::cmp::Ordering;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str) -> Result<()> {
    let rows = rows(ctx, key, cli.sortname.as_deref(), cli.sortorder)?;
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The full, sorted values table for one key (pre-pagination), or an empty vec
/// for an unknown key. Split from [`run`] for testing.
pub(crate) fn rows(
    ctx: &Ctx,
    key: &str,
    sortname: Option<&str>,
    sortorder: Order,
) -> Result<Vec<ValueRow>> {
    let tq = ctx.taginfo()?;

    let Some(k) = tq.key(key.as_bytes()) else {
        return Ok(Vec::new());
    };

    // taginfo's value `fraction` is over the *key's* total objects (bbox-clipped
    // when `--bbox` is given).
    let c = crate::bbox::key_summary(&k, ctx.bbox).counts;
    let key_count_all = c.nodes + c.ways + c.relations;

    let mut rows: Vec<ValueRow> = k
        .values()
        .map(|v| {
            let vc = crate::bbox::value_counts(&v, ctx.bbox);
            let count = vc.nodes + vc.ways + vc.relations;
            ValueRow {
                value: util::lossy(v.value()),
                count,
                fraction: fraction(count, key_count_all),
                in_wiki: None,
                description: None,
                desclang: None,
                descdir: None,
            }
        })
        .collect();

    // Under a bbox clip, a value with no in-scope occurrences is noise.
    if ctx.bbox.is_some() {
        rows.retain(|r| r.count > 0);
    }

    sort(&mut rows, sortname, sortorder)?;
    Ok(rows)
}

/// taginfo's values table defaults to `count desc`; allowed: count, fraction,
/// value. Ties break by value string for determinism.
fn sort(rows: &mut [ValueRow], sortname: Option<&str>, sortorder: Order) -> Result<()> {
    let field = sortname.unwrap_or("count");
    let cmp = |a: &ValueRow, b: &ValueRow| -> Ordering {
        let primary = match field {
            // `count` and `fraction` rank identically (fraction is count scaled
            // by a constant), so one comparator serves both.
            "count" | "fraction" => a.count.cmp(&b.count),
            "value" => Ordering::Equal,
            _ => Ordering::Equal,
        };
        primary.then_with(|| a.value.cmp(&b.value))
    };
    if !matches!(field, "count" | "fraction" | "value") {
        bail!("unknown --sortname {field:?} for `key … values`; allowed: count, fraction, value");
    }

    rows.sort_by(cmp);
    if sortorder == Order::Desc {
        rows.reverse();
    }
    Ok(())
}
