//! serde structs for the taginfo v4 envelope and per-endpoint item shapes
//! (design §4). Field names and order are chosen to match the live API exactly.

use serde::Serialize;

/// The taginfo response envelope wrapping every endpoint's `data` array.
#[derive(Serialize, Debug)]
pub struct Envelope<T> {
    /// taginfo puts the request URL here; we put the canonical CLI invocation.
    pub url: String,
    /// Freshness timestamp, `YYYY-MM-DDThh:mm:ssZ` (see `freshness`).
    pub data_until: String,
    /// 1-based page echoed back.
    pub page: usize,
    /// Rows per page echoed back (0 = all).
    pub rp: usize,
    /// Total rows before paging.
    pub total: usize,
    pub data: Vec<T>,
}

/// One row of `/api/4/keys/all` (design §4.2).
#[derive(Serialize, Debug)]
pub struct KeyRow {
    pub key: String,
    pub count_all: u64,
    pub count_all_fraction: f64,
    pub count_nodes: u64,
    pub count_nodes_fraction: f64,
    pub count_ways: u64,
    pub count_ways_fraction: f64,
    pub count_relations: u64,
    pub count_relations_fraction: f64,
    pub values_all: u64,
    // --- fields osmflat has no source for (§4.4-bis): all `null`, the honest
    // "unknown" rather than an asserted `false`/`0`.
    pub users_all: Option<u64>,
    pub in_wiki: Option<bool>,
    pub projects: Option<u64>,
}

/// One row of `/api/4/key/stats` (design §4.4): four rows per key, one per type
/// plus `all`.
#[derive(Serialize, Debug)]
pub struct StatRow {
    /// `all` | `nodes` | `ways` | `relations`.
    pub r#type: &'static str,
    pub count: u64,
    pub count_fraction: f64,
    /// Distinct values for this object type.
    pub values: u64,
}

/// One row of `/api/4/tag/stats` (design §4.4): four rows per tag, one per type
/// plus `all`. Unlike [`StatRow`], a tag has no distinct values, so taginfo
/// omits the `values` column here.
#[derive(Serialize, Debug)]
pub struct TagStatRow {
    /// `all` | `nodes` | `ways` | `relations`.
    pub r#type: &'static str,
    pub count: u64,
    pub count_fraction: f64,
}

/// One row of `/api/4/key/values` (design §4.3): a distinct value of a key with
/// its count and the fraction of the key's objects that carry it.
#[derive(Serialize, Debug)]
pub struct ValueRow {
    pub value: String,
    pub count: u64,
    pub fraction: f64,
    // --- wiki-sourced, no osmflat source: `null` per the §4.4-bis contract.
    pub in_wiki: Option<bool>,
    pub description: Option<String>,
    pub desclang: Option<String>,
    pub descdir: Option<String>,
}

/// One row of `/api/4/key/combinations` (design §4.5): another key that
/// co-occurs with the queried key.
#[derive(Serialize, Debug)]
pub struct ComboRow {
    pub other_key: String,
    pub together_count: u64,
    /// `together_count` over the *other* key's objects.
    pub to_fraction: f64,
    /// `together_count` over *this* key's objects.
    pub from_fraction: f64,
}

/// One row of `/api/4/tag/combinations` (design §4.6): another `key=value` tag
/// that co-occurs with the queried tag.
#[derive(Serialize, Debug)]
pub struct TagComboRow {
    pub other_key: String,
    pub other_value: String,
    pub together_count: u64,
    /// `together_count` over the *other* tag's objects.
    pub to_fraction: f64,
    /// `together_count` over *this* tag's objects.
    pub from_fraction: f64,
}
