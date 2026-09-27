//! Local HTTP API used by the browser extension (and scripts).
//!
//! Listens on 127.0.0.1 only. A request is accepted when it comes from a
//! browser extension (`Origin: chrome-extension://…` / `moz-extension://…`,
//! which web pages cannot forge) or carries the `X-FitDL-Token` from
//! `config.toml`. Websites therefore cannot drive the downloader.

use std::net::{Ipv4Addr, SocketAddr};

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::manager::{ItemId, Manager, NewLink};

pub const TOKEN_HEADER: &str = "x-fitdl-token";

#[derive(Deserialize)]
struct AddRequest {
    links: Vec<NewLink>,
    #[serde(default)]
    game: Option<String>,
}

#[derive(Deserialize)]
struct PageRequest {
    page_url: String,
    #[serde(default)]
    include_optional: bool,
}

#[derive(Deserialize)]
struct RemoveRequest {
    #[serde(default)]
    delete_files: bool,
}

pub fn router(manager: Manager) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            is_extension_origin(origin)
        }))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::HeaderName::from_static(TOKEN_HEADER),
        ]);

    let protected = Router::new()
        .route("/api/items", get(items))
        .route("/api/add", post(add))
        .route("/api/add-page", post(add_page))
        .route("/api/scrape", post(scrape))
        .route("/api/items/{id}/pause", post(pause))
        .route("/api/items/{id}/resume", post(resume))
        .route("/api/items/{id}/remove", post(remove))
        .route("/api/pause-all", post(pause_all))
        .route("/api/resume-all", post(resume_all))
        .route_layer(middleware::from_fn_with_state(manager.clone(), authorize));

    Router::new()
        .route("/api/ping", get(ping))
        .merge(protected)
        .layer(cors)
        .with_state(manager)
}

/// Serve the API on 127.0.0.1:`port` until the task is dropped.
pub async fn serve(manager: Manager, port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("cannot listen on {addr}: {e} (is FitDL already running?)"))?;
    tracing::info!("API listening on http://{addr}");
    axum::serve(listener, router(manager)).await?;
    Ok(())
}

fn is_extension_origin(origin: &HeaderValue) -> bool {
    origin.to_str().is_ok_and(|o| {
        o.starts_with("chrome-extension://")
            || o.starts_with("moz-extension://")
            || o.starts_with("extension://")
    })
}

async fn authorize(State(manager): State<Manager>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let from_extension = headers.get(header::ORIGIN).is_some_and(is_extension_origin);
    let token_ok = headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t == manager.config().api_token);
    if from_extension || token_ok {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "only the FitDL browser extension or a valid X-FitDL-Token may use this API" })),
        )
            .into_response()
    }
}

async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "app": "FitDL", "version": env!("CARGO_PKG_VERSION") }))
}

async fn items(State(m): State<Manager>) -> Json<serde_json::Value> {
    Json(json!({ "items": m.items(), "totals": m.totals() }))
}

async fn add(State(m): State<Manager>, Json(req): Json<AddRequest>) -> Response {
    Json(m.add(req.links, req.game)).into_response()
}

async fn add_page(State(m): State<Manager>, Json(req): Json<PageRequest>) -> Response {
    match m.add_page(&req.page_url, req.include_optional).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, format!("{e:#}")),
    }
}

async fn scrape(State(m): State<Manager>, Json(req): Json<PageRequest>) -> Response {
    match m.scrape(&req.page_url).await {
        Ok(page) => Json(page).into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, format!("{e:#}")),
    }
}

async fn pause(State(m): State<Manager>, Path(id): Path<ItemId>) -> StatusCode {
    m.pause(id);
    StatusCode::NO_CONTENT
}

async fn resume(State(m): State<Manager>, Path(id): Path<ItemId>) -> StatusCode {
    m.resume(id);
    StatusCode::NO_CONTENT
}

async fn remove(
    State(m): State<Manager>,
    Path(id): Path<ItemId>,
    body: Option<Json<RemoveRequest>>,
) -> StatusCode {
    m.remove(id, body.is_some_and(|Json(b)| b.delete_files));
    StatusCode::NO_CONTENT
}

async fn pause_all(State(m): State<Manager>) -> StatusCode {
    m.pause_all();
    StatusCode::NO_CONTENT
}

async fn resume_all(State(m): State<Manager>) -> StatusCode {
    m.resume_all();
    StatusCode::NO_CONTENT
}

fn error(code: StatusCode, message: String) -> Response {
    (code, Json(json!({ "error": message }))).into_response()
}
