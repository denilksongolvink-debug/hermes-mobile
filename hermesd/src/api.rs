use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{sse::{Event, Sse}, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures::stream::StreamExt;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct LogsQuery {
    #[serde(default)]
    pub follow: Option<u8>,
    #[serde(default = "default_tail")]
    pub tail: usize,
}

fn default_tail() -> usize {
    200
}

fn auth(state: &AppState, headers: &HeaderMap) -> bool {
    if state.token.is_empty() {
        return true;
    }
    // Bearer
    if let Some(v) = headers.get("authorization") {
        if let Ok(s) = v.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                if t == state.token {
                    return true;
                }
            }
        }
    }
    if let Some(v) = headers.get("x-token") {
        if v.to_str().ok() == Some(state.token.as_str()) {
            return true;
        }
    }
    false
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/stacks", get(list_stacks))
        .route("/v1/stacks/:stack/start", post(stack_start))
        .route("/v1/stacks/:stack/stop", post(stack_stop))
        .route("/v1/stacks/:stack/restart", post(stack_restart))
        .route("/v1/stacks/:stack/services/:svc/start", post(svc_start))
        .route("/v1/stacks/:stack/services/:svc/stop", post(svc_stop))
        .route("/v1/stacks/:stack/services/:svc/restart", post(svc_restart))
        .route("/v1/stacks/:stack/services/:svc/logs", get(svc_logs))
        .route("/v1/stats", get(stats))
        .with_state(state)
}

#[derive(serde::Serialize)]
struct StackView {
    name: String,
    services: Vec<crate::supervisor::ServiceSnapshot>,
}

async fn list_stacks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<StackView>>, StatusCode> {
    if !auth(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    // group services by stack
    let mut map: std::collections::BTreeMap<String, Vec<crate::supervisor::ServiceSnapshot>> =
        std::collections::BTreeMap::new();
    for h in state.manager.list() {
        let snap = h.snapshot();
        map.entry(snap.stack.clone()).or_default().push(snap);
    }
    let out: Vec<StackView> = map
        .into_iter()
        .map(|(name, services)| StackView { name, services })
        .collect();
    Ok(Json(out))
}

async fn svc_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((stack, svc)): Path<(String, String)>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if let Some(h) = state.manager.get(&format!("{}/{}", stack, svc)) {
        let _ = h.start();
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn svc_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((stack, svc)): Path<(String, String)>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if let Some(h) = state.manager.get(&format!("{}/{}", stack, svc)) {
        let _ = h.stop();
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn svc_restart(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((stack, svc)): Path<(String, String)>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if let Some(h) = state.manager.get(&format!("{}/{}", stack, svc)) {
        let _ = h.restart();
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn stack_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack): Path<String>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    for h in state.manager.list() {
        if h.stack == stack {
            let _ = h.start();
        }
    }
    StatusCode::NO_CONTENT
}

async fn stack_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack): Path<String>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    for h in state.manager.list() {
        if h.stack == stack {
            let _ = h.stop();
        }
    }
    StatusCode::NO_CONTENT
}

async fn stack_restart(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack): Path<String>,
) -> StatusCode {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    for h in state.manager.list() {
        if h.stack == stack {
            let _ = h.restart();
        }
    }
    StatusCode::NO_CONTENT
}

async fn svc_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((stack, svc)): Path<(String, String)>,
    Query(q): Query<LogsQuery>,
) -> Response {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let h = match state.manager.get(&format!("{}/{}", stack, svc)) {
        Some(h) => h,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    if q.follow.unwrap_or(0) == 1 {
        let rx = h.log.subscribe();
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|m| async move {
            match m {
                Ok(line) => Some(Ok::<Event, Infallible>(Event::default().data(line))),
                Err(_) => None,
            }
        });
        Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()).into_response()
    } else {
        let lines = h.log.tail(q.tail);
        Json(lines).into_response()
    }
}

async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !auth(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // basic: count services, sum rss
    let mut total_rss: u64 = 0;
    let mut n = 0;
    for h in state.manager.list() {
        n += 1;
        if let Some(rss) = h.snapshot().rss_kb {
            total_rss += rss;
        }
    }
    Json(serde_json::json!({
        "services": n,
        "total_rss_kb": total_rss,
    })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::supervisor::ServiceManager;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn list_stacks_unauthorized_without_token() {
        let mgr = Arc::new(ServiceManager::new(None, None, None));
        let state = Arc::new(AppState::new(mgr, "secret".into()));
        let app = router(state);
        let resp = app
            .oneshot(Request::builder().uri("/v1/stacks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_stacks_authorized() {
        let mut mgr = ServiceManager::new(None, None, None);
        mgr.add_service("s", crate::manifest::ServiceSpec {
            name: "svc".into(),
            cmd: "echo hi".into(),
            cwd: None,
            restart: crate::manifest::RestartPolicy::No,
            max_rss_mb: None,
            env: std::collections::BTreeMap::new(),
            enabled: true,
        });
        let mgr = Arc::new(mgr);
        let state = Arc::new(AppState::new(mgr, "secret".into()));
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/stacks")
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stats_ok() {
        let mgr = Arc::new(ServiceManager::new(None, None, None));
        let state = Arc::new(AppState::new(mgr, "".into()));
        let app = router(state);
        let resp = app
            .oneshot(Request::builder().uri("/v1/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
