//! Router-level integration tests for `serve` mode: build the same synthetic
//! archive `tests.rs` uses, spin up the axum router via [`serve::app`], and
//! drive it with `tower::ServiceExt::oneshot` -- no real socket needed.

use crate::open::OpenedArchive;
use crate::serve;
use crate::tests::fixture;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use osmflat_extc::test_support::{build_ext_archive, build_parent_archive};
use osmflat_extc::BuildOptions;
use serde_json::Value;
use tower::ServiceExt;

fn opened() -> OpenedArchive {
    let parent = build_parent_archive(&fixture()).expect("build parent");
    let opts = BuildOptions {
        taginfo: true,
        combinations: true,
        ..Default::default()
    };
    let archive = build_ext_archive(parent, &opts).expect("build sidecar");
    OpenedArchive::from_archive(archive, "1970-01-01T00:00:00Z".to_string())
}

async fn get(path: &str) -> (StatusCode, Value) {
    let response = serve::app(opened())
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn key_stats_happy_path() {
    let (status, body) = get("/api/4/key/stats?key=highway").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"].as_array().unwrap();
    let all = rows.iter().find(|r| r["type"] == "all").unwrap();
    assert_eq!(all["count"], 3);
}

#[tokio::test]
async fn unknown_key_is_200_empty() {
    let (status, body) = get("/api/4/key/stats?key=does_not_exist").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn malformed_bbox_is_400() {
    let (status, _) = get("/api/4/key/stats?key=highway&bbox=not,a,box").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bbox_clips_results_end_to_end() {
    // Matches the `bbox_clips_totals_and_counts` fixture scenario in tests.rs:
    // only n3 (shop=bakery) and w2 (highway=primary) fall in this box.
    let (status, body) = get("/api/4/keys/all?bbox=2.5,2.5,3.5,3.5").await;
    assert_eq!(status, StatusCode::OK);
    let mut keys: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["highway", "shop"]);
}

#[tokio::test]
async fn no_envelope_returns_bare_array() {
    let (status, body) = get("/api/4/keys/all?no_envelope=true").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn keys_all_query_search_filters() {
    let (status, body) = get("/api/4/keys/all?query=na").await;
    assert_eq!(status, StatusCode::OK);
    let keys: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["name"]);
}

#[tokio::test]
async fn tag_stats_happy_path() {
    let (status, body) = get("/api/4/tag/stats?key=highway&value=primary").await;
    assert_eq!(status, StatusCode::OK);
    let all = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["type"] == "all")
        .unwrap();
    assert_eq!(all["count"], 2);
}

#[tokio::test]
async fn key_values_happy_path() {
    let (status, body) = get("/api/4/key/values?key=amenity").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows[0]["value"], "cafe");
    assert_eq!(rows[0]["count"], 2);
}

#[tokio::test]
async fn key_combinations_happy_path() {
    let (status, body) = get("/api/4/key/combinations?key=highway").await;
    assert_eq!(status, StatusCode::OK);
    let name = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["other_key"] == "name")
        .unwrap();
    assert_eq!(name["together_count"], 1);
}

#[tokio::test]
async fn tag_combinations_happy_path() {
    let (status, body) = get("/api/4/tag/combinations?key=highway&value=primary").await;
    assert_eq!(status, StatusCode::OK);
    let main = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["other_key"] == "name" && r["other_value"] == "Main")
        .unwrap();
    assert_eq!(main["together_count"], 1);
}

#[tokio::test]
async fn envelope_url_reflects_request_uri() {
    let (_, body) = get("/api/4/key/stats?key=highway").await;
    assert_eq!(body["url"], "/api/4/key/stats?key=highway");
}
