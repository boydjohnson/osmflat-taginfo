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
    // --- fields osmflat has no source for: documented neutral stubs (§4.4-bis).
    pub users_all: u64,
    pub in_wiki: bool,
    pub projects: u64,
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
