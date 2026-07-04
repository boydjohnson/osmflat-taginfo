//! `key <KEY> combinations` → taginfo `/api/4/key/combinations` (design §4.5):
//! other keys used by objects that also carry this key.
//!
//! `from_fraction` is `together_count` over *this* key's objects; `to_fraction`
//! is over the *other* key's objects (a second key lookup per row). Empty when
//! the sidecar was built without `osmflat-extc --combinations`.
//!
//! Under `--bbox`, `together_count` has no stored aggregate, so it's
//! recomputed by intersecting the two keys' bbox-clipped object-index sets
//! (see [`crate::bbox::key_indices_in_bbox`]) — `O(combos × values-per-key)`,
//! against a single key rather than the whole archive.

use crate::cli::{Cli, Order};
use crate::model::ComboRow;
use crate::open::{fraction, Ctx};
use crate::{output, util};
use anyhow::{bail, Result};
use osmflat_ext::taginfo::TaginfoQuery;
use std::cmp::Ordering;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str) -> Result<()> {
    let rows = rows(ctx, key, cli.sortname.as_deref(), cli.sortorder)?;
    if rows.is_empty() {
        eprintln!(
            "note: no co-occurring keys for {key:?} in scope \
             (if unexpected, rebuild the sidecar with `osmflat-extc --combinations`{})",
            if ctx.bbox.is_some() {
                ", or none co-occur within --bbox"
            } else {
                ""
            }
        );
    }
    output::emit(cli, &ctx.data_until, util::url(cli), rows)
}

/// The full, sorted co-occurring-keys table (pre-pagination), or an empty vec
/// for an unknown key / a sidecar built without `--combinations`. Split from
/// [`run`] for testing.
pub(crate) fn rows(
    ctx: &Ctx,
    key: &str,
    sortname: Option<&str>,
    sortorder: Order,
) -> Result<Vec<ComboRow>> {
    let tq = ctx.taginfo()?;

    let Some(k) = tq.key(key.as_bytes()) else {
        return Ok(Vec::new());
    };

    let mut rows: Vec<ComboRow> = match ctx.bbox {
        None => {
            let from_total = count_all(&k.counts());
            k.combinations()
                .map(|c| {
                    let together = c.together_count();
                    ComboRow {
                        other_key: util::lossy(c.key()),
                        together_count: together,
                        to_fraction: fraction(together, key_total(&tq, c.key())),
                        from_fraction: fraction(together, from_total),
                    }
                })
                .collect()
        }
        Some(bbox) => {
            let this_idx = crate::bbox::key_indices_in_bbox(&k, bbox);
            let from_total = this_idx.total();
            k.combinations()
                .filter_map(|c| {
                    let other = tq.key(c.key())?;
                    let other_idx = crate::bbox::key_indices_in_bbox(&other, bbox);
                    let together = crate::bbox::key_together_count_in_bbox(&this_idx, &other_idx);
                    (together > 0).then(|| ComboRow {
                        other_key: util::lossy(c.key()),
                        together_count: together,
                        to_fraction: fraction(together, other_idx.total()),
                        from_fraction: fraction(together, from_total),
                    })
                })
                .collect()
        }
    };

    sort(&mut rows, sortname, sortorder)?;
    Ok(rows)
}

fn count_all(c: &osmflat_ext::taginfo::TypeCounts) -> u64 {
    c.nodes + c.ways + c.relations
}

/// Total objects carrying `key` (the `to_fraction` denominator), 0 if unknown.
fn key_total(tq: &TaginfoQuery, key: &[u8]) -> u64 {
    tq.key(key).map(|k| count_all(&k.counts())).unwrap_or(0)
}

/// taginfo defaults to `together_count desc`; the sidecar already stores combos
/// in that order. Ties break by other-key string for determinism.
fn sort(rows: &mut [ComboRow], sortname: Option<&str>, sortorder: Order) -> Result<()> {
    let field = sortname.unwrap_or("together_count");
    let cmp = |a: &ComboRow, b: &ComboRow| -> Ordering {
        let primary = match field {
            "together_count" => a.together_count.cmp(&b.together_count),
            "to_fraction" => a.to_fraction.total_cmp(&b.to_fraction),
            "from_fraction" => a.from_fraction.total_cmp(&b.from_fraction),
            _ => Ordering::Equal, // "other_key" sorts on the tie-break alone
        };
        primary.then_with(|| a.other_key.cmp(&b.other_key))
    };
    if !matches!(
        field,
        "together_count" | "to_fraction" | "from_fraction" | "other_key"
    ) {
        bail!(
            "unknown --sortname {field:?} for `key … combinations`; allowed: \
             together_count, to_fraction, from_fraction, other_key"
        );
    }

    rows.sort_by(cmp);
    if sortorder == Order::Desc {
        rows.reverse();
    }
    Ok(())
}
