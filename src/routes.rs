//! HTTP handlers for `serve` mode -- one per taginfo route. Each parses its
//! query params, builds a per-request [`Ctx`] via [`Ctx::for_bbox`], calls the
//! matching endpoint's `rows()` (the same pure function the CLI uses), and
//! wraps the result in the taginfo envelope via [`output::envelope`].

use crate::cli::Order;
use crate::open::{Ctx, OpenedArchive};
use crate::{endpoints, output};
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use osmflat_ext::query::Bbox;
use serde::Deserialize;
use serde_json::{json, Value};

pub enum ApiError {
    BadRequest(String),
    NoTaginfoIndex,
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NoTaginfoIndex => (
                StatusCode::SERVICE_UNAVAILABLE,
                "sidecar has no taginfo index (rebuild with `osmflat-extc --taginfo`)".to_string(),
            ),
            ApiError::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err)
    }
}

/// Parse an HTTP `?bbox=minx,miny,maxx,maxy` query param -- the comma-separated
/// counterpart of the CLI's space-separated `--bbox MINX MINY MAXX MAXY`,
/// sharing the same min<=max validation via [`crate::cli::bbox_from_parts`].
fn parse_bbox_query(raw: Option<&str>) -> Result<Option<Bbox>, ApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() != 4 {
        return Err(ApiError::BadRequest(format!(
            "invalid bbox {raw:?}: expected 4 comma-separated numbers MINX,MINY,MAXX,MAXY"
        )));
    }
    let parse = |s: &str| {
        s.trim().parse::<f64>().map_err(|_| {
            ApiError::BadRequest(format!("invalid bbox {raw:?}: {s:?} is not a number"))
        })
    };
    let bbox = crate::cli::bbox_from_parts(
        parse(parts[0])?,
        parse(parts[1])?,
        parse(parts[2])?,
        parse(parts[3])?,
    )
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Some(bbox))
}

fn require_taginfo(ctx: &Ctx) -> Result<(), ApiError> {
    if ctx.has_taginfo() {
        Ok(())
    } else {
        Err(ApiError::NoTaginfoIndex)
    }
}

fn envelope_response<T: serde::Serialize>(
    uri: &Uri,
    ctx: &Ctx,
    page: usize,
    rp: usize,
    no_envelope: bool,
    rows: Vec<T>,
) -> Result<Json<Value>, ApiError> {
    let value = output::envelope(
        uri.to_string(),
        &ctx.data_until,
        page,
        rp,
        no_envelope,
        rows,
    )
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(value))
}

// --- GET /api/4/keys/all ------------------------------------------------------

#[derive(Deserialize)]
pub struct KeysAllQuery {
    query: Option<String>,
    page: Option<usize>,
    rp: Option<usize>,
    sortname: Option<String>,
    sortorder: Option<Order>,
    no_envelope: Option<bool>,
    bbox: Option<String>,
}

pub async fn keys_all(
    State(state): State<OpenedArchive>,
    Query(q): Query<KeysAllQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let rows = endpoints::keys::rows(
        &ctx,
        q.query.as_deref(),
        q.sortname.as_deref(),
        q.sortorder.unwrap_or(Order::Desc),
    )?;
    envelope_response(
        &uri,
        &ctx,
        q.page.unwrap_or(1),
        q.rp.unwrap_or(0),
        q.no_envelope.unwrap_or(false),
        rows,
    )
}

// --- GET /api/4/key/stats -----------------------------------------------------

#[derive(Deserialize)]
pub struct KeyStatsQuery {
    key: String,
    no_envelope: Option<bool>,
    bbox: Option<String>,
}

pub async fn key_stats(
    State(state): State<OpenedArchive>,
    Query(q): Query<KeyStatsQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let rows = endpoints::key_stats::rows(&ctx, &q.key)?;
    // key/stats is a fixed 4-row shape: not paged.
    envelope_response(&uri, &ctx, 1, 0, q.no_envelope.unwrap_or(false), rows)
}

// --- GET /api/4/key/values, GET /api/4/key/combinations -----------------------

/// Shared shape for the two `key <KEY> ...` endpoints that page/sort a table.
#[derive(Deserialize)]
pub struct KeyPagedQuery {
    key: String,
    page: Option<usize>,
    rp: Option<usize>,
    sortname: Option<String>,
    sortorder: Option<Order>,
    no_envelope: Option<bool>,
    bbox: Option<String>,
}

pub async fn key_values(
    State(state): State<OpenedArchive>,
    Query(q): Query<KeyPagedQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let rows = endpoints::key_values::rows(
        &ctx,
        &q.key,
        q.sortname.as_deref(),
        q.sortorder.unwrap_or(Order::Desc),
    )?;
    envelope_response(
        &uri,
        &ctx,
        q.page.unwrap_or(1),
        q.rp.unwrap_or(0),
        q.no_envelope.unwrap_or(false),
        rows,
    )
}

pub async fn key_combinations(
    State(state): State<OpenedArchive>,
    Query(q): Query<KeyPagedQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let rows = endpoints::key_combinations::rows(
        &ctx,
        &q.key,
        q.sortname.as_deref(),
        q.sortorder.unwrap_or(Order::Desc),
    )?;
    envelope_response(
        &uri,
        &ctx,
        q.page.unwrap_or(1),
        q.rp.unwrap_or(0),
        q.no_envelope.unwrap_or(false),
        rows,
    )
}

// --- GET /api/4/tag/stats -----------------------------------------------------

#[derive(Deserialize)]
pub struct TagStatsQuery {
    key: String,
    value: String,
    no_envelope: Option<bool>,
    bbox: Option<String>,
}

pub async fn tag_stats(
    State(state): State<OpenedArchive>,
    Query(q): Query<TagStatsQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let rows = endpoints::tag_stats::rows(&ctx, &q.key, &q.value)?;
    // tag/stats is a fixed 4-row shape: not paged.
    envelope_response(&uri, &ctx, 1, 0, q.no_envelope.unwrap_or(false), rows)
}

// --- GET /api/4/tag/combinations ----------------------------------------------

#[derive(Deserialize)]
pub struct TagCombinationsQuery {
    key: String,
    value: String,
    page: Option<usize>,
    rp: Option<usize>,
    sortname: Option<String>,
    sortorder: Option<Order>,
    no_envelope: Option<bool>,
    bbox: Option<String>,
}

pub async fn tag_combinations(
    State(state): State<OpenedArchive>,
    Query(q): Query<TagCombinationsQuery>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let bbox = parse_bbox_query(q.bbox.as_deref())?;
    let ctx = Ctx::for_bbox(&state, bbox);
    require_taginfo(&ctx)?;
    let (page, rp) = (q.page.unwrap_or(1), q.rp.unwrap_or(0));
    let rows = endpoints::tag_combinations::page(
        &ctx,
        &q.key,
        &q.value,
        q.sortname.as_deref(),
        q.sortorder.unwrap_or(Order::Desc),
        page,
        rp,
    )?;
    let value = output::envelope_page(
        uri.to_string(),
        &ctx.data_until,
        page,
        rp,
        q.no_envelope.unwrap_or(false),
        rows,
    )
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(value))
}
