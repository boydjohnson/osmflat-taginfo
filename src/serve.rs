//! `serve` mode: wraps the same `rows()` functions the CLI uses behind axum
//! HTTP routes matching taginfo's real `/api/4/...` URLs (design doc §11).

use crate::open::OpenedArchive;
use crate::routes;
use axum::routing::get;
use axum::Router;

/// Build the router without binding a socket, so tests can drive it directly
/// via `tower::ServiceExt::oneshot`.
pub fn app(state: OpenedArchive) -> Router {
    Router::new()
        .route("/api/4/keys/all", get(routes::keys_all))
        .route("/api/4/key/stats", get(routes::key_stats))
        .route("/api/4/key/values", get(routes::key_values))
        .route("/api/4/key/combinations", get(routes::key_combinations))
        .route("/api/4/tag/stats", get(routes::tag_stats))
        .route("/api/4/tag/combinations", get(routes::tag_combinations))
        // `permissive()` allows any origin/method/header -- fine for a
        // read-only local-dev API on a loopback/private port, but should
        // become an explicit origin allow-list before this is ever reachable
        // outside a dev machine.
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// Bind `bind_addr` and serve until the process is killed.
pub async fn run(state: OpenedArchive, bind_addr: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    eprintln!("osmflat-taginfo serve listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
