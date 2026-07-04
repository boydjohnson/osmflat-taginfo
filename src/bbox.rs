//! Per-value/per-key object counts, optionally clipped to a `--bbox`.
//!
//! Archive-wide counts are `O(1)` (stored aggregates); a bbox clip has no
//! stored aggregate, so it's derived by summing the per-value bbox∩postings
//! merge-join. The expensive spatial candidate ranges are cached once in
//! [`BboxClip`] and reused for every value.

use osmflat::Osm;
use osmflat_ext::query::{self, Bbox};
use osmflat_ext::taginfo::{KeyView, TypeCounts, ValueView};
use std::ops::Range;

/// A `--bbox` resolved once against the parent archive.
///
/// The range vectors are the reusable spatial side of every `tag ∩ bbox`
/// merge-join. Without this cache, every value count would rerun the same
/// spatial query for nodes, ways, and relations.
#[derive(Clone, Debug)]
pub struct BboxClip {
    pub node_ranges: Vec<Range<u64>>,
    pub way_ranges: Vec<Range<u64>>,
    pub relation_ranges: Vec<Range<u64>>,
    pub totals: TypeCounts,
}

impl BboxClip {
    pub fn new(parent: &Osm, bbox: Bbox) -> Self {
        let node_indices = query::node_indices_in_bbox(parent, bbox);
        let way_indices = query::way_indices_in_bbox(parent, bbox);
        let relation_indices = query::relation_indices_in_bbox(parent, bbox);
        let totals = TypeCounts {
            nodes: node_indices.len() as u64,
            ways: way_indices.len() as u64,
            relations: relation_indices.len() as u64,
        };
        BboxClip {
            node_ranges: query::to_index_ranges(&node_indices),
            way_ranges: query::to_index_ranges(&way_indices),
            relation_ranges: query::to_index_ranges(&relation_indices),
            totals,
        }
    }
}

/// Per-type object counts for one `(key, value)`, archive-wide or bbox-clipped.
pub fn value_counts(v: &ValueView, clip: Option<&BboxClip>) -> TypeCounts {
    match clip {
        None => v.counts(),
        Some(clip) => TypeCounts {
            nodes: query::intersect_bbox(v.nodes(), &clip.node_ranges).count() as u64,
            ways: query::intersect_bbox(v.ways(), &clip.way_ranges).count() as u64,
            relations: query::intersect_bbox(v.relations(), &clip.relation_ranges).count() as u64,
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
pub fn key_summary(k: &KeyView, clip: Option<&BboxClip>) -> KeySummary {
    match clip {
        None => KeySummary {
            counts: k.counts(),
            distinct_values: k.distinct_values(),
        },
        Some(clip) => {
            let mut counts = TypeCounts::default();
            let mut distinct_values = 0u64;
            for v in k.values() {
                let vc = value_counts(&v, Some(clip));
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

/// Bbox-clipped postings for one `(key,value)`, one sorted list per type.
pub struct ValueBboxIndices {
    pub nodes: Vec<u64>,
    pub ways: Vec<u64>,
    pub relations: Vec<u64>,
}

impl ValueBboxIndices {
    pub fn total(&self) -> u64 {
        (self.nodes.len() + self.ways.len() + self.relations.len()) as u64
    }
}

/// Materialize the bbox-clipped postings for one `(key,value)`.
pub fn value_indices_in_bbox(v: &ValueView, clip: &BboxClip) -> ValueBboxIndices {
    ValueBboxIndices {
        nodes: query::intersect_bbox(v.nodes(), &clip.node_ranges).collect(),
        ways: query::intersect_bbox(v.ways(), &clip.way_ranges).collect(),
        relations: query::intersect_bbox(v.relations(), &clip.relation_ranges).collect(),
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
pub fn tag_together_count_in_bbox(a: &ValueBboxIndices, b: &ValueBboxIndices) -> u64 {
    intersect_count(&a.nodes, &b.nodes)
        + intersect_count(&a.ways, &b.ways)
        + intersect_count(&a.relations, &b.relations)
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
/// `O(values)` cached-range intersections — the same per-value cost as
/// [`key_summary`].
pub fn key_indices_in_bbox(k: &KeyView, clip: &BboxClip) -> KeyBboxIndices {
    let (mut nodes, mut ways, mut relations) = (Vec::new(), Vec::new(), Vec::new());
    for v in k.values() {
        nodes.extend(query::intersect_bbox(v.nodes(), &clip.node_ranges));
        ways.extend(query::intersect_bbox(v.ways(), &clip.way_ranges));
        relations.extend(query::intersect_bbox(v.relations(), &clip.relation_ranges));
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
