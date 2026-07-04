//! Per-value/per-key object counts, optionally clipped to a `--bbox`.
//!
//! Archive-wide counts are `O(1)` (stored aggregates); a bbox clip has no
//! stored aggregate, so it's derived by summing the per-value bbox∩postings
//! merge-join ([`osmflat_ext::taginfo::ValueView::nodes_in_bbox`] & co.).

use osmflat_ext::query::Bbox;
use osmflat_ext::taginfo::{KeyView, TypeCounts, ValueView};

/// Per-type object counts for one `(key, value)`, archive-wide or bbox-clipped.
pub fn value_counts(v: &ValueView, bbox: Option<Bbox>) -> TypeCounts {
    match bbox {
        None => v.counts(),
        Some(bbox) => TypeCounts {
            nodes: v.nodes_in_bbox(bbox).len() as u64,
            ways: v.ways_in_bbox(bbox).len() as u64,
            relations: v.relations_in_bbox(bbox).len() as u64,
        },
    }
}

/// A key's aggregate object counts and distinct-value total, archive-wide or
/// bbox-clipped.
pub struct KeySummary {
    pub counts: TypeCounts,
    /// Distinct values with at least one occurrence in scope (equals
    /// [`KeyView::distinct_values`] archive-wide; only a subset may survive a
    /// bbox clip).
    pub distinct_values: u64,
}

/// Archive-wide, this is the key's `O(1)` stored aggregate. A bbox clip has no
/// stored aggregate, so it sums each value's bbox-filtered counts — `O(values)`.
pub fn key_summary(k: &KeyView, bbox: Option<Bbox>) -> KeySummary {
    match bbox {
        None => KeySummary {
            counts: k.counts(),
            distinct_values: k.distinct_values(),
        },
        Some(bbox) => {
            let mut counts = TypeCounts::default();
            let mut distinct_values = 0u64;
            for v in k.values() {
                let vc = value_counts(&v, Some(bbox));
                if vc.nodes + vc.ways + vc.relations > 0 {
                    distinct_values += 1;
                }
                counts.nodes += vc.nodes;
                counts.ways += vc.ways;
                counts.relations += vc.relations;
            }
            KeySummary {
                counts,
                distinct_values,
            }
        }
    }
}

/// Count of common elements between two ascending, deduplicated slices —
/// `|a ∩ b|` by two-pointer merge, `O(len a + len b)`.
fn intersect_count(a: &[u64], b: &[u64]) -> u64 {
    let (mut i, mut j, mut n) = (0usize, 0usize, 0u64);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

/// Together-count for two specific tags, bbox-clipped: entities carrying both
/// `(key,value)` tags, restricted per type since an entity's type must match
/// on both sides to co-occur.
pub fn tag_together_count_in_bbox(a: &ValueView, b: &ValueView, bbox: Bbox) -> u64 {
    intersect_count(&a.nodes_in_bbox(bbox), &b.nodes_in_bbox(bbox))
        + intersect_count(&a.ways_in_bbox(bbox), &b.ways_in_bbox(bbox))
        + intersect_count(&a.relations_in_bbox(bbox), &b.relations_in_bbox(bbox))
}

/// A key's bbox-clipped object-index sets, one ascending+deduped list per
/// type — the union of its values' bbox-filtered postings (an entity carries
/// at most one value per key, so the values' sets are disjoint).
pub struct KeyBboxIndices {
    pub nodes: Vec<u64>,
    pub ways: Vec<u64>,
    pub relations: Vec<u64>,
}

impl KeyBboxIndices {
    /// Total objects in scope for this key (the `to_fraction`/`from_fraction`
    /// denominator), matching [`KeySummary::counts`] summed.
    pub fn total(&self) -> u64 {
        (self.nodes.len() + self.ways.len() + self.relations.len()) as u64
    }
}

/// Build a key's bbox-clipped index sets by unioning its values' postings.
/// `O(values)` bbox queries — the same per-value cost as [`key_summary`].
pub fn key_indices_in_bbox(k: &KeyView, bbox: Bbox) -> KeyBboxIndices {
    let (mut nodes, mut ways, mut relations) = (Vec::new(), Vec::new(), Vec::new());
    for v in k.values() {
        nodes.extend(v.nodes_in_bbox(bbox));
        ways.extend(v.ways_in_bbox(bbox));
        relations.extend(v.relations_in_bbox(bbox));
    }
    nodes.sort_unstable();
    ways.sort_unstable();
    relations.sort_unstable();
    KeyBboxIndices {
        nodes,
        ways,
        relations,
    }
}

/// Together-count for two keys, bbox-clipped: entities carrying both keys
/// (with any value), restricted per type.
pub fn key_together_count_in_bbox(a: &KeyBboxIndices, b: &KeyBboxIndices) -> u64 {
    intersect_count(&a.nodes, &b.nodes)
        + intersect_count(&a.ways, &b.ways)
        + intersect_count(&a.relations, &b.relations)
}
