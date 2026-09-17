//! Endpoint unit tests against a synthetic in-memory archive (design §9).
//!
//! A small fixture with known tags is built into a parent `Osm` + `Ext` sidecar
//! via osmflat-extc's `test-support`, wrapped in a [`Ctx`], and each endpoint's
//! pure `rows()` function is asserted field-by-field. The fixture is the oracle.

use crate::cli::Order;
use crate::endpoints::{
    key_combinations, key_stats, key_values, keys, tag_combinations, tag_stats,
};
use crate::open::{fraction, Ctx};
use osmflat_ext::query::Bbox;
use osmflat_extc::test_support::{
    build_ext_archive, build_parent_archive, Fixture, MemberSpec, NodeSpec, RelationSpec, TagSpec,
    WaySpec,
};
use osmflat_extc::BuildOptions;

// --- the fixture --------------------------------------------------------------
//
// nodes: n0 amenity=cafe + name=A | n1 amenity=cafe | n2 amenity=bench | n3 shop=bakery
// ways:  w0 highway=primary + name=Main | w1 highway=residential | w2 highway=primary
// rels:  r0 type=route + name=Loop (member w0)
//
// Derived per-key (object) counts and distinct values:
//   amenity  nodes=3 ways=0 rels=0  all=3  values={cafe:2, bench:1}      distinct=2
//   name     nodes=1 ways=1 rels=1  all=3  values={A:1, Main:1, Loop:1}  distinct=3
//   shop     nodes=1               all=1  values={bakery:1}             distinct=1
//   highway  ways=3               all=3  values={primary:2, residential:1} distinct=2
//   type     rels=1               all=1  values={route:1}              distinct=1

pub(crate) fn n(lon: f64, lat: f64, tags: &[(&'static str, &'static str)]) -> NodeSpec {
    NodeSpec {
        lon,
        lat,
        tags: tags.iter().map(|(k, v)| TagSpec::new(k, v)).collect(),
    }
}

pub(crate) fn w(refs: Vec<usize>, tags: &[(&'static str, &'static str)]) -> WaySpec {
    WaySpec {
        refs,
        tags: tags.iter().map(|(k, v)| TagSpec::new(k, v)).collect(),
    }
}

pub(crate) fn fixture() -> Fixture {
    Fixture {
        nodes: vec![
            n(0.0, 0.0, &[("amenity", "cafe"), ("name", "A")]),
            n(1.0, 1.0, &[("amenity", "cafe")]),
            n(2.0, 2.0, &[("amenity", "bench")]),
            n(3.0, 3.0, &[("shop", "bakery")]),
        ],
        ways: vec![
            w(vec![0, 1], &[("highway", "primary"), ("name", "Main")]),
            w(vec![1, 2], &[("highway", "residential")]),
            w(vec![2, 3], &[("highway", "primary")]),
        ],
        relations: vec![RelationSpec {
            bbox: Some((0.0, 0.0, 1.0, 1.0)),
            members: vec![MemberSpec::Way(0)],
            tags: vec![TagSpec::new("type", "route"), TagSpec::new("name", "Loop")],
        }],
    }
}

fn ctx(combinations: bool) -> Ctx {
    let parent = build_parent_archive(&fixture()).expect("build parent");
    let opts = BuildOptions {
        taginfo: true,
        combinations,
        ..Default::default()
    };
    let archive = build_ext_archive(parent, &opts).expect("build sidecar");
    Ctx::from_archive(archive, "1970-01-01T00:00:00Z".to_string())
}

/// Same fixture, clipped to `bbox`.
fn ctx_bbox(combinations: bool, bbox: Bbox) -> Ctx {
    let parent = build_parent_archive(&fixture()).expect("build parent");
    let opts = BuildOptions {
        taginfo: true,
        combinations,
        ..Default::default()
    };
    let archive = build_ext_archive(parent, &opts).expect("build sidecar");
    Ctx::from_archive_with_bbox(archive, "1970-01-01T00:00:00Z".to_string(), Some(bbox))
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

// --- keys / api/4/keys/all ----------------------------------------------------

#[test]
fn keys_counts_fractions_and_stubs() {
    let ctx = ctx(false);
    let rows = keys::rows(&ctx, None, None, Order::Desc).unwrap();

    let highway = rows.iter().find(|r| r.key == "highway").unwrap();
    assert_eq!(highway.count_all, 3);
    assert_eq!(highway.count_ways, 3);
    assert_eq!(highway.count_nodes, 0);
    assert_eq!(highway.count_relations, 0);
    assert_eq!(highway.values_all, 2);

    // Fraction uses the *ways* denominator (not total objects) and is 4-dp.
    assert_eq!(
        highway.count_ways_fraction,
        round4(3.0 / ctx.totals.ways as f64)
    );
    assert_eq!(
        highway.count_all_fraction,
        round4(3.0 / ctx.totals.objects as f64)
    );

    // Unsupported fields are null, not asserted 0/false (§4.4-bis).
    assert_eq!(highway.users_all, None);
    assert_eq!(highway.in_wiki, None);
    assert_eq!(highway.projects, None);
}

#[test]
fn keys_default_sort_is_count_all_desc() {
    let ctx = ctx(false);
    let rows = keys::rows(&ctx, None, None, Order::Desc).unwrap();
    assert!(rows.windows(2).all(|w| w[0].count_all >= w[1].count_all));
}

#[test]
fn keys_search_prefix_filters() {
    let ctx = ctx(false);
    // Only "name" has prefix "na"; "amenity"/"highway"/"shop"/"type" do not.
    let rows = keys::rows(&ctx, Some("na"), None, Order::Desc).unwrap();
    let got: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(got, vec!["name"]);
}

// --- key/stats ---------------------------------------------------------------

#[test]
fn key_stats_rows_and_per_type_values() {
    let ctx = ctx(false);
    let rows = key_stats::rows(&ctx, "name").unwrap();
    let by_type = |t: &str| rows.iter().find(|r| r.r#type == t).unwrap();

    assert_eq!(by_type("all").count, 3);
    assert_eq!(by_type("all").values, 3); // A, Main, Loop
    assert_eq!(by_type("nodes").count, 1);
    assert_eq!(by_type("nodes").values, 1); // A
    assert_eq!(by_type("ways").count, 1);
    assert_eq!(by_type("ways").values, 1); // Main
    assert_eq!(by_type("relations").count, 1);
    assert_eq!(by_type("relations").values, 1); // Loop
}

#[test]
fn unknown_key_is_empty_not_error() {
    let ctx = ctx(false);
    assert!(key_stats::rows(&ctx, "does_not_exist").unwrap().is_empty());
    assert!(key_values::rows(&ctx, "does_not_exist", None, Order::Desc)
        .unwrap()
        .is_empty());
}

// --- key/values --------------------------------------------------------------

#[test]
fn key_values_counts_fraction_invariant_and_stubs() {
    let ctx = ctx(false);
    let rows = key_values::rows(&ctx, "amenity", None, Order::Desc).unwrap();

    // Sorted by count desc: cafe(2) before bench(1).
    let vals: Vec<(&str, u64)> = rows.iter().map(|r| (r.value.as_str(), r.count)).collect();
    assert_eq!(vals, vec![("cafe", 2), ("bench", 1)]);

    // fraction is over the key's own total (3), 4-dp.
    let cafe = &rows[0];
    assert_eq!(cafe.fraction, round4(2.0 / 3.0));
    assert_eq!(cafe.in_wiki, None);
    assert_eq!(cafe.description, None);
    assert_eq!(cafe.desclang, None);
    assert_eq!(cafe.descdir, None);

    // Invariant: sum of value counts == the key's count_all.
    let sum: u64 = rows.iter().map(|r| r.count).sum();
    assert_eq!(sum, 3);
}

#[test]
fn key_values_sort_by_value_string() {
    let ctx = ctx(false);
    let rows = key_values::rows(&ctx, "amenity", Some("value"), Order::Asc).unwrap();
    let vals: Vec<&str> = rows.iter().map(|r| r.value.as_str()).collect();
    assert_eq!(vals, vec!["bench", "cafe"]);
}

// --- tag/stats ---------------------------------------------------------------

#[test]
fn tag_stats_counts_and_omits_values_field() {
    let ctx = ctx(false);
    let rows = tag_stats::rows(&ctx, "highway", "primary").unwrap();
    let by_type = |t: &str| rows.iter().find(|r| r.r#type == t).unwrap();
    assert_eq!(by_type("all").count, 2); // w0, w2
    assert_eq!(by_type("ways").count, 2);
    assert_eq!(by_type("nodes").count, 0);
    assert_eq!(by_type("relations").count, 0);

    // taginfo's tag/stats has no `values` column (unlike key/stats).
    let json = serde_json::to_value(&rows[0]).unwrap();
    assert!(json.get("values").is_none());
    assert!(json.get("count_fraction").is_some());
}

// --- combinations ------------------------------------------------------------

#[test]
fn key_combinations_fractions() {
    let ctx = ctx(true);
    let rows = key_combinations::rows(&ctx, "highway", None, Order::Desc).unwrap();

    // Only w0 (highway=primary) carries another key: name. together = 1.
    let name = rows.iter().find(|r| r.other_key == "name").unwrap();
    assert_eq!(name.together_count, 1);
    assert_eq!(name.from_fraction, round4(1.0 / 3.0)); // over highway's 3
    assert_eq!(name.to_fraction, round4(1.0 / 3.0)); // over name's 3
}

#[test]
fn tag_combinations_fields() {
    let ctx = ctx(true);
    let rows = tag_combinations::rows(&ctx, "highway", "primary", None, Order::Desc).unwrap();
    // highway=primary co-occurs with name=Main on w0.
    let main = rows
        .iter()
        .find(|r| r.other_key == "name" && r.other_value == "Main")
        .unwrap();
    assert_eq!(main.together_count, 1);
    assert_eq!(main.from_fraction, round4(1.0 / 2.0)); // over highway=primary's 2
}

#[test]
fn combinations_empty_without_combinations_sidecar() {
    let ctx = ctx(false); // taginfo-only sidecar
    assert!(key_combinations::rows(&ctx, "highway", None, Order::Desc)
        .unwrap()
        .is_empty());
    assert!(
        tag_combinations::rows(&ctx, "highway", "primary", None, Order::Desc)
            .unwrap()
            .is_empty()
    );
}

// --- --bbox --------------------------------------------------------------

#[test]
fn bbox_clips_totals_and_counts() {
    // n3 (shop=bakery, 3,3) and w2 (highway=primary, refs [2,3] -> bbox
    // (2,2)-(3,3)) overlap; everything else (n0/n1/n2, w0, w1, r0) sits
    // strictly outside this box.
    let bbox = Bbox {
        min_lon: 2.5,
        min_lat: 2.5,
        max_lon: 3.5,
        max_lat: 3.5,
    };
    let ctx = ctx_bbox(false, bbox);

    assert_eq!(ctx.totals.nodes, 1);
    assert_eq!(ctx.totals.ways, 1);
    assert_eq!(ctx.totals.relations, 0);

    let rows = keys::rows(&ctx, None, None, Order::Desc).unwrap();
    let mut keys_seen: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    keys_seen.sort_unstable();
    // amenity/name/type have no in-bbox occurrences and are dropped entirely.
    assert_eq!(keys_seen, vec!["highway", "shop"]);

    let shop = rows.iter().find(|r| r.key == "shop").unwrap();
    assert_eq!(shop.count_all, 1);
    assert_eq!(shop.count_nodes, 1);
    assert_eq!(shop.values_all, 1);

    let highway = rows.iter().find(|r| r.key == "highway").unwrap();
    assert_eq!(highway.count_all, 1);
    assert_eq!(highway.count_ways, 1);
    assert_eq!(highway.values_all, 1); // only "primary" is in scope, not "residential"

    let values = key_values::rows(&ctx, "highway", None, Order::Desc).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].value, "primary");
    assert_eq!(values[0].count, 1);

    let stats = tag_stats::rows(&ctx, "highway", "primary").unwrap();
    assert_eq!(stats.iter().find(|r| r.r#type == "all").unwrap().count, 1);
}

#[test]
fn bbox_recomputes_combinations() {
    // n3 (shop=bakery) and w2 (highway=primary) are the only entities in
    // scope, and neither carries a second key, so every together_count
    // collapses to 0 and both combinations tables come back empty — unlike
    // the archive-wide `together_count: 1` for highway~name / highway=primary~name=Main.
    let tight = ctx_bbox(
        true,
        Bbox {
            min_lon: 2.5,
            min_lat: 2.5,
            max_lon: 3.5,
            max_lat: 3.5,
        },
    );
    assert!(key_combinations::rows(&tight, "highway", None, Order::Desc)
        .unwrap()
        .is_empty());
    assert!(
        tag_combinations::rows(&tight, "highway", "primary", None, Order::Desc)
            .unwrap()
            .is_empty()
    );

    // A box covering the whole fixture reproduces the archive-wide
    // together_count exactly — a sanity check on the recompute formula.
    let whole = ctx_bbox(
        true,
        Bbox {
            min_lon: -1.0,
            min_lat: -1.0,
            max_lon: 4.0,
            max_lat: 4.0,
        },
    );
    let rows = key_combinations::rows(&whole, "highway", None, Order::Desc).unwrap();
    let name = rows.iter().find(|r| r.other_key == "name").unwrap();
    assert_eq!(name.together_count, 1);
    assert_eq!(name.from_fraction, round4(1.0 / 3.0));
    assert_eq!(name.to_fraction, round4(1.0 / 3.0));

    let tag_rows = tag_combinations::rows(&whole, "highway", "primary", None, Order::Desc).unwrap();
    let main = tag_rows
        .iter()
        .find(|r| r.other_key == "name" && r.other_value == "Main")
        .unwrap();
    assert_eq!(main.together_count, 1);
}

// --- pagination, rounding, round-trip ----------------------------------------

#[test]
fn paginate_slices_pages() {
    use crate::output::paginate;
    assert_eq!(paginate(vec![0, 1, 2, 3, 4], 2, 2), vec![2, 3]);
    assert_eq!(paginate(vec![0, 1, 2, 3, 4], 1, 0), vec![0, 1, 2, 3, 4]); // rp=0 → all
    assert_eq!(paginate(vec![0, 1, 2], 99, 2), Vec::<i32>::new()); // out of range
}

#[test]
fn fraction_rounds_to_4dp_and_guards_zero() {
    assert_eq!(fraction(2, 3), 0.6667);
    assert_eq!(fraction(22, 100), 0.22);
    assert_eq!(fraction(1, 10_000_000), 0.0);
    assert_eq!(fraction(5, 0), 0.0);
}

#[test]
fn rows_round_trip_through_json() {
    let ctx = ctx(false);
    let rows = keys::rows(&ctx, None, None, Order::Desc).unwrap();
    let json = serde_json::to_string(&rows).unwrap();
    let back: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(back.is_array());
    // null stubs survive the round trip as JSON null.
    let first = &back[0];
    assert!(first.get("in_wiki").unwrap().is_null());
}

/// A fixture where `highway=primary` co-occurs with enough tags, in enough
/// count ties, to exercise paging: 11 other tags over 4 ways, counts 1–4.
fn paging_ctx() -> Ctx {
    const WAYS: [&[(&str, &str)]; 4] = [
        &[
            ("highway", "primary"),
            ("name", "Main"),
            ("surface", "asphalt"),
            ("lanes", "2"),
            ("oneway", "yes"),
            ("ref", "A1"),
        ],
        &[
            ("highway", "primary"),
            ("name", "High"),
            ("surface", "asphalt"),
            ("lanes", "2"),
            ("oneway", "no"),
            ("maxspeed", "50"),
        ],
        &[
            ("highway", "primary"),
            ("name", "Main"),
            ("surface", "asphalt"),
            ("lanes", "4"),
            ("oneway", "yes"),
            ("bridge", "yes"),
        ],
        &[
            ("highway", "primary"),
            ("name", "Low"),
            ("surface", "asphalt"),
            ("lanes", "2"),
            ("oneway", "yes"),
            ("maxspeed", "50"),
        ],
    ];
    let fixture = Fixture {
        nodes: vec![n(0.0, 0.0, &[]), n(1.0, 1.0, &[])],
        ways: WAYS.iter().map(|tags| w(vec![0, 1], tags)).collect(),
        relations: vec![],
    };
    let parent = build_parent_archive(&fixture).expect("build parent");
    let opts = BuildOptions {
        taginfo: true,
        combinations: true,
        ..Default::default()
    };
    let archive = build_ext_archive(parent, &opts).expect("build sidecar");
    Ctx::from_archive(archive, "1970-01-01T00:00:00Z".to_string())
}

/// The paged path picks exactly the rows paging the full sorted table would,
/// with the same total, for every sort field, order, page size and page --
/// including ties on the sort field and pages past the end.
#[test]
fn tag_combinations_page_matches_paged_rows() {
    let ctx = paging_ctx();
    assert_page_matches_paged_rows(&ctx, 11);

    let missing =
        tag_combinations::page(&ctx, "highway", "nope", None, Order::Desc, 1, 10).unwrap();
    assert_eq!((missing.total, missing.rows.len()), (0, 0));
    assert!(tag_combinations::page(
        &ctx,
        "highway",
        "primary",
        Some("bogus"),
        Order::Desc,
        1,
        10
    )
    .is_err());
}

/// `from_fraction` is rounded to 4 places, so on a tag carried by enough
/// objects, different `together_count`s round to the same fraction and sort
/// by key/value instead. The paged path must order by the rounded value too.
#[test]
fn tag_combinations_page_matches_on_rounded_fraction_ties() {
    const N: usize = 20_000;
    let ways = (0..N)
        .map(|i| {
            let mut tags = vec![("highway", "primary")];
            // Counts N-2 and N-3 both round to 0.9999 over N, and N-4 to
            // 0.9998. The higher count gets the earlier key, so ordering the
            // tie by count and by key disagree.
            if i > 1 {
                tags.push(("alpha", "b"));
            }
            if i > 2 {
                tags.push(("zeta", "a"));
            }
            if i > 3 {
                tags.push(("mid", "c"));
            }
            if i % 2 == 0 {
                tags.push(("even", "yes"));
            }
            w(vec![0, 1], &tags)
        })
        .collect();
    let fixture = Fixture {
        nodes: vec![n(0.0, 0.0, &[]), n(1.0, 1.0, &[])],
        ways,
        relations: vec![],
    };
    let parent = build_parent_archive(&fixture).expect("build parent");
    let opts = BuildOptions {
        taginfo: true,
        combinations: true,
        ..Default::default()
    };
    let archive = build_ext_archive(parent, &opts).expect("build sidecar");
    let ctx = Ctx::from_archive(archive, "1970-01-01T00:00:00Z".to_string());

    // The fixture really does tie: two different counts share a fraction.
    let rows = tag_combinations::rows(&ctx, "highway", "primary", None, Order::Desc).unwrap();
    let zeta = rows.iter().find(|r| r.other_key == "zeta").unwrap();
    let alpha = rows.iter().find(|r| r.other_key == "alpha").unwrap();
    assert_ne!(zeta.together_count, alpha.together_count);
    assert_eq!(zeta.from_fraction, alpha.from_fraction);

    assert_page_matches_paged_rows(&ctx, 4);
}

/// Compare [`tag_combinations::page`] against paging
/// [`tag_combinations::rows`] for `highway=primary` across every sort field,
/// order, page size and page.
fn assert_page_matches_paged_rows(ctx: &Ctx, expected_len: usize) {
    let fields = [
        None,
        Some("together_count"),
        Some("from_fraction"),
        Some("to_fraction"),
        Some("other_key"),
        Some("other_value"),
    ];
    let as_tuples = |rows: &[crate::model::TagComboRow]| {
        rows.iter()
            .map(|r| {
                (
                    r.other_key.clone(),
                    r.other_value.clone(),
                    r.together_count,
                    r.to_fraction,
                    r.from_fraction,
                )
            })
            .collect::<Vec<_>>()
    };
    for sortname in fields {
        for sortorder in [Order::Asc, Order::Desc] {
            let full =
                tag_combinations::rows(ctx, "highway", "primary", sortname, sortorder).unwrap();
            assert_eq!(full.len(), expected_len);
            for rp in [0, 1, 2, 3, 5, expected_len, expected_len + 9] {
                for page in 1..=expected_len + 1 {
                    let expected = crate::output::paginate(
                        tag_combinations::rows(ctx, "highway", "primary", sortname, sortorder)
                            .unwrap(),
                        page,
                        rp,
                    );
                    let got = tag_combinations::page(
                        ctx, "highway", "primary", sortname, sortorder, page, rp,
                    )
                    .unwrap();
                    let case = format!("{sortname:?} {sortorder:?} rp={rp} page={page}");
                    assert_eq!(got.total, full.len(), "{case}");
                    assert_eq!(as_tuples(&got.rows), as_tuples(&expected), "{case}");
                }
            }
        }
    }
}
