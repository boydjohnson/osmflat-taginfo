//! `tag <KEY> <VALUE> combinations` → taginfo `/api/4/tag/combinations`
//! (design §4.6): other `key=value` tags used by objects that also carry this
//! tag.
//!
//! `from_fraction` is `together_count` over *this* tag's objects; `to_fraction`
//! is over the *other* tag's objects (a second `(key,value)` lookup per row).
//! Empty when the sidecar was built without `osmflat-extc --combinations`.

use crate::cli::{Cli, Order};
use crate::model::TagComboRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::{bail, Result};
use osmflat_ext::taginfo::TaginfoQuery;
use std::cmp::Ordering;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str, value: &str) -> Result<()> {
    let rows = rows(ctx, key, value, cli.sortname.as_deref(), cli.sortorder)?;
    if rows.is_empty() {
        eprintln!(
            "note: no co-occurring tags for {key}={value} \
             (if unexpected, rebuild the sidecar with `osmflat-extc --combinations`)"
        );
    }
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The full, sorted co-occurring-tags table (pre-pagination), or an empty vec
/// for an unknown tag / a sidecar built without `--combinations`. Split from
/// [`run`] for testing.
pub(crate) fn rows(
    ctx: &Ctx,
    key: &str,
    value: &str,
    sortname: Option<&str>,
    sortorder: Order,
) -> Result<Vec<TagComboRow>> {
    let tq = ctx.taginfo()?;

    let Some(v) = tq.kv(key.as_bytes(), value.as_bytes()) else {
        return Ok(Vec::new());
    };

    let from_total = count_all(&v.counts());

    let mut rows: Vec<TagComboRow> = v
        .combinations()
        .map(|c| {
            let together = c.together_count();
            TagComboRow {
                other_key: util::lossy(c.key()),
                other_value: util::lossy(c.value()),
                together_count: together,
                to_fraction: fraction(together, tag_total(&tq, c.key(), c.value())),
                from_fraction: fraction(together, from_total),
            }
        })
        .collect();

    sort(&mut rows, sortname, sortorder)?;
    Ok(rows)
}

fn count_all(c: &osmflat_ext::taginfo::TypeCounts) -> u64 {
    c.nodes + c.ways + c.relations
}

/// Total objects carrying `key=value` (the `to_fraction` denominator), 0 if
/// unknown.
fn tag_total(tq: &TaginfoQuery, key: &[u8], value: &[u8]) -> u64 {
    tq.kv(key, value)
        .map(|v| count_all(&v.counts()))
        .unwrap_or(0)
}

/// taginfo defaults to `together_count desc`; the sidecar already stores combos
/// in that order. Ties break by (other_key, other_value) for determinism.
fn sort(rows: &mut [TagComboRow], sortname: Option<&str>, sortorder: Order) -> Result<()> {
    let field = sortname.unwrap_or("together_count");
    let cmp = |a: &TagComboRow, b: &TagComboRow| -> Ordering {
        let primary = match field {
            "together_count" => a.together_count.cmp(&b.together_count),
            "to_fraction" => a.to_fraction.total_cmp(&b.to_fraction),
            "from_fraction" => a.from_fraction.total_cmp(&b.from_fraction),
            // "other_key"/"other_value" sort on the tie-break alone
            _ => Ordering::Equal,
        };
        primary
            .then_with(|| a.other_key.cmp(&b.other_key))
            .then_with(|| a.other_value.cmp(&b.other_value))
    };
    if !matches!(
        field,
        "together_count" | "to_fraction" | "from_fraction" | "other_key" | "other_value"
    ) {
        bail!(
            "unknown --sortname {field:?} for `tag … combinations`; allowed: \
             together_count, to_fraction, from_fraction, other_key, other_value"
        );
    }

    rows.sort_by(cmp);
    if sortorder == Order::Desc {
        rows.reverse();
    }
    Ok(())
}
