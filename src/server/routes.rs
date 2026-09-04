use super::{AppState, auth};
use crate::desktop::remote::{H_RECT, H_SIZE};
use crate::proto::*;
use axum::{
    Json, Router,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/state", get(state_h))
        .route("/v1/screenshot", get(screenshot_h))
        .route("/v1/act", post(act_h))
        .route("/v1/clipboard", get(clipboard_h))
        .route("/v1/whoami", get(whoami_h))
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(state.clone(), auth::middleware))
        .with_state(state)
}

struct Api<T>(Result<T>);

impl<T: serde::Serialize> IntoResponse for Api<T> {
    fn into_response(self) -> Response {
        match self.0 {
            Ok(v) => Json(v).into_response(),
            Err(e) => auth::error_response(&e),
        }
    }
}

async fn state_h(State(s): State<AppState>) -> Api<crate::proto::State> {
    Api(s.desktop.state().await)
}

#[derive(Deserialize)]
struct ShotQuery {
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    max: Option<u32>,
}

async fn screenshot_h(State(s): State<AppState>, Query(q): Query<ShotQuery>) -> Response {
    let display = match q.display.as_deref().map(str::parse::<DisplayTarget>) {
        Some(Err(e)) => return auth::error_response(&RdcError::BadRequest(e)),
        Some(Ok(d)) => d,
        None => DisplayTarget::All,
    };
    let format = match q.format.as_deref() {
        None | Some("png") => ImageFormat::Png,
        Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
        Some(o) => return auth::error_response(&RdcError::BadRequest(format!("bad format {o:?}"))),
    };
    match s.desktop.screenshot(ScreenshotReq { display, format, max_long_edge: q.max }).await {
        Ok(shot) => {
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static(shot.format.mime()));
            let r = shot.rect;
            h.insert(H_RECT, HeaderValue::from_str(&format!("{},{},{},{}", r.x, r.y, r.w, r.h)).unwrap());
            h.insert(H_SIZE, HeaderValue::from_str(&format!("{},{}", shot.width, shot.height)).unwrap());
            (StatusCode::OK, h, shot.data).into_response()
        }
        Err(e) => auth::error_response(&e),
    }
}

async fn act_h(State(s): State<AppState>, Json(a): Json<Action>) -> Api<serde_json::Value> {
    Api(s.desktop.act(a).await.map(|_| serde_json::json!({ "ok": true })))
}

async fn clipboard_h(State(s): State<AppState>) -> Api<serde_json::Value> {
    Api(s.desktop.clipboard_get().await.map(|text| serde_json::json!({ "text": text })))
}

async fn whoami_h(Extension(id): Extension<Identity>) -> Json<Identity> {
    Json(id)
}
