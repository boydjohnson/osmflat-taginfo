//! `tag <KEY> <VALUE> combinations` → taginfo `/api/4/tag/combinations`
//! (design §4.6): other `key=value` tags used by objects that also carry this
//! tag.
//!
//! `from_fraction` is `together_count` over *this* tag's objects; `to_fraction`
//! is over the *other* tag's objects (a second `(key,value)` lookup per row).
//! Empty when the sidecar was built without `osmflat-extc --combinations`.
//!
//! Under `--bbox`, `together_count` has no stored aggregate, so it's
//! recomputed by intersecting the two tags' bbox-clipped postings (see
//! [`crate::bbox::tag_together_count_in_bbox`]) — one bbox query pair per row.
//!
//! A popular tag co-occurs with millions of others, and building a row costs a
//! `(key, value)` lookup for `to_fraction`. So without `--bbox`, unless sorting
//! by `to_fraction`, and when a page size is given, [`page`] picks the
//! requested page from each combination's cheap sort key first and builds
//! only that page's rows. Otherwise every row is needed, so the full table is
//! built and sliced.

use crate::cli::{Cli, Order};
use crate::model::TagComboRow;
use crate::open::{fraction, Ctx};
use crate::output::{self, Page};
use crate::util;
use anyhow::{bail, Result};
use osmflat_ext::taginfo::{TagCombinationView, TaginfoQuery};
use std::borrow::Cow;
use std::cmp::Ordering;

pub fn run(cli: &Cli, ctx: &Ctx, key: &str, value: &str) -> Result<()> {
    let page = page(
        ctx,
        key,
        value,
        cli.sortname.as_deref(),
        cli.sortorder,
        cli.page,
        cli.rp,
    )?;
    if page.total == 0 {
        eprintln!(
            "note: no co-occurring tags for {key}={value} in scope \
             (if unexpected, rebuild the sidecar with `osmflat-extc --combinations`{})",
            if ctx.bbox.is_some() {
                ", or none co-occur within --bbox"
            } else {
                ""
            }
        );
    }
    output::emit_page(cli, &ctx.data_until, util::url(cli), page)
}

/// The requested page of the sorted co-occurring-tags table, with the table's
/// total row count. Equal to paging [`rows`], without building every row when
/// the sort doesn't need them.
pub(crate) fn page(
    ctx: &Ctx,
    key: &str,
    value: &str,
    sortname: Option<&str>,
    sortorder: Order,
    page: usize,
    rp: usize,
) -> Result<Page<TagComboRow>> {
    let field = sort_field(sortname)?;
    // Choosing a page first only pays when it skips rows: with `--bbox` or a
    // `to_fraction` sort every row's sort key is the expensive part, and with
    // `rp == 0` every row is wanted anyway.
    if ctx.bbox.is_some() || field == "to_fraction" || rp == 0 {
        return Ok(Page::from_rows(
            rows(ctx, key, value, sortname, sortorder)?,
            page,
            rp,
        ));
    }

    let tq = ctx.taginfo()?;
    let Some(v) = tq.kv(key.as_bytes(), value.as_bytes()) else {
        return Ok(Page {
            total: 0,
            rows: Vec::new(),
        });
    };

    // One cheap candidate per combination: its stored count, `from_fraction`
    // (over this tag's constant total, so no lookup), and borrowed key and
    // value strings. `from_fraction` is compared as the rounded value rows
    // carry, since rounding ties rows `together_count` would still separate.
    let from_total = count_all(&v.counts());
    let mut cands: Vec<Candidate> = v
        .combinations()
        .enumerate()
        .map(|(idx, combo)| Candidate {
            together: combo.together_count(),
            from_fraction: fraction(combo.together_count(), from_total),
            key: String::from_utf8_lossy(combo.key()),
            value: String::from_utf8_lossy(combo.value()),
            idx,
            combo,
        })
        .collect();
    let total = cands.len();
    let (start, end) = output::page_bounds(total, page, rp);

    // Same order as `sort`: ascending by field then key then value, reversed
    // for `desc`. `sort` is stable and then reverses, so equal rows come out
    // in reverse table order under `desc`; the `idx` tie-break reproduces that
    // and makes the order total, so an unstable selection is exact.
    let ascending = |a: &Candidate, b: &Candidate| -> Ordering {
        let primary = match field {
            "together_count" => a.together.cmp(&b.together),
            "from_fraction" => a.from_fraction.total_cmp(&b.from_fraction),
            _ => Ordering::Equal,
        };
        primary
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.idx.cmp(&b.idx))
    };
    let cmp = |a: &Candidate, b: &Candidate| match sortorder {
        Order::Asc => ascending(a, b),
        Order::Desc => ascending(b, a),
    };

    if end < total {
        cands.select_nth_unstable_by(end, cmp);
        cands.truncate(end);
    }
    cands.sort_unstable_by(cmp);

    let rows = cands[start..]
        .iter()
        .map(|c| combo_row(&tq, &c.combo, c.together, from_total))
        .collect();
    Ok(Page { total, rows })
}

/// A combination's sort key, for choosing a page before building rows.
struct Candidate<'a> {
    together: u64,
    from_fraction: f64,
    key: Cow<'a, str>,
    value: Cow<'a, str>,
    /// Position in the stored table, the final tie-break.
    idx: usize,
    combo: TagCombinationView<'a>,
}

/// The unclipped row for one stored combination.
fn combo_row(
    tq: &TaginfoQuery,
    combo: &TagCombinationView,
    together: u64,
    from_total: u64,
) -> TagComboRow {
    TagComboRow {
        other_key: util::lossy(combo.key()),
        other_value: util::lossy(combo.value()),
        together_count: together,
        to_fraction: fraction(together, tag_total(tq, combo.key(), combo.value())),
        from_fraction: fraction(together, from_total),
    }
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

    let mut rows: Vec<TagComboRow> = match ctx.bbox {
        None => {
            let from_total = count_all(&v.counts());
            v.combinations()
                .map(|c| combo_row(&tq, &c, c.together_count(), from_total))
                .collect()
        }
        Some(_) => {
            let clip = ctx.bbox_clip.as_ref().expect("bbox clip exists");
            let from_idx = crate::bbox::value_indices_in_bbox(&v, clip);
            let from_total = from_idx.total();
            v.combinations()
                .filter_map(|c| {
                    let other = tq.kv(c.key(), c.value())?;
                    let other_idx = crate::bbox::value_indices_in_bbox(&other, clip);
                    let together = crate::bbox::tag_together_count_in_bbox(&from_idx, &other_idx);
                    let to_total = other_idx.total();
                    (together > 0).then(|| TagComboRow {
                        other_key: util::lossy(c.key()),
                        other_value: util::lossy(c.value()),
                        together_count: together,
                        to_fraction: fraction(together, to_total),
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
    let field = sort_field(sortname)?;
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
    rows.sort_by(cmp);
    if sortorder == Order::Desc {
        rows.reverse();
    }
    Ok(())
}

/// The sort field, defaulting to `together_count`, or an error naming the
/// allowed fields.
fn sort_field(sortname: Option<&str>) -> Result<&str> {
    let field = sortname.unwrap_or("together_count");
    if !matches!(
        field,
        "together_count" | "to_fraction" | "from_fraction" | "other_key" | "other_value"
    ) {
        bail!(
            "unknown --sortname {field:?} for `tag … combinations`; allowed: \
             together_count, to_fraction, from_fraction, other_key, other_value"
        );
    }
    Ok(field)
}
