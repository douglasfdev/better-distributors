//! Isolated, capability-authorized mobile capture bridge. Never mount admin routes here.
use crate::{ApiError, error};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower_http::services::ServeFile;

const TTL: Duration = Duration::from_secs(15 * 60);
const MAX_SESSIONS: usize = 100;
const MAX_TEXT: usize = 32_000;

#[derive(Clone, Default)]
pub struct Captures {
    ocr: crate::ocr::Ocr,
    origin: Option<String>,
    bind: Option<String>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}
impl std::fmt::Debug for Captures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Sessions hold token material; represent them only by counts.
        f.debug_struct("Captures")
            .field("configured", &self.origin.is_some())
            .field(
                "sessions",
                &self.sessions.lock().map(|s| s.len()).unwrap_or(usize::MAX),
            )
            .finish()
    }
}
struct Session {
    read: String,
    write: String,
    deadline: Instant,
    connected: bool,
    text: Option<String>,
    sequence: u64,
    ended: bool,
}

impl Captures {
    pub fn configured(value: Option<String>, allow_http: bool) -> Result<Self, &'static str> {
        let invalid = "CAPTURE_PUBLIC_URL deve ser apenas uma origem (https://capture.example ou, em rede local, http://192.168.0.10:3001), sem credenciais, caminho, query ou fragmento";
        let origin = value
            .map(|value| {
                let uri = value.parse::<axum::http::Uri>().map_err(|_| invalid)?;
                let scheme = uri.scheme_str().ok_or(invalid)?;
                if !(scheme == "https" || (allow_http && scheme == "http"))
                    || uri.host().is_none()
                    || uri.authority().is_some_and(|a| a.as_str().contains('@'))
                    || uri
                        .path_and_query()
                        .is_some_and(|p| !p.as_str().is_empty() && p.as_str() != "/")
                    || value.contains(['#', '?', '\\'])
                {
                    return Err(invalid);
                }
                Ok(format!("{scheme}://{}", uri.authority().ok_or(invalid)?))
            })
            .transpose()?;
        let bind = origin.as_ref().map(|_| {
            std::env::var("CAPTURE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".into())
        });
        Ok(Self {
            origin,
            bind,
            ..Self::default()
        })
    }
    /// Invalid setup never aborts startup: returns the server plus a warning to print.
    pub fn from_values(value: Option<String>, allow_http: bool) -> (Self, Option<String>) {
        match Self::configured(value, allow_http) {
            Ok(captures) => (captures, None),
            Err(reason) => (Self::default(), Some(reason.to_owned())),
        }
    }
    pub fn from_env() -> (Self, Option<String>) {
        let allow_http = matches!(
            std::env::var("CAPTURE_ALLOW_HTTP").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        );
        let (mut state, warning) = Self::from_values(std::env::var("CAPTURE_PUBLIC_URL").ok(), allow_http);
        state.ocr = crate::ocr::Ocr::from_env();
        (state, warning)
    }
    pub fn enabled(&self) -> bool {
        self.origin.is_some()
    }
    pub fn origin_url(&self) -> Option<&str> {
        self.origin.as_deref()
    }
    pub fn public_bind(&self) -> Option<String> {
        self.bind.clone()
    }
}

pub fn local_router(state: Captures) -> Router {
    Router::new()
        .route("/api/ocr", post(desktop_ocr).layer(DefaultBodyLimit::max(crate::ocr::MAX_IMAGE)))
        .route("/api/ocr/config", get(ocr_config))
        .route("/api/captures", post(create))
        .route("/api/captures/{id}", get(read).delete(cancel))
        .route("/api/captures/{id}/ack", post(ack))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

pub fn public_router(state: Captures, frontend: &str) -> Router {
    Router::new()
        .route_service(
            "/capture",
            ServeFile::new(format!("{frontend}/capture.html")),
        )
        .route_service(
            "/bridge.js",
            ServeFile::new(format!("{frontend}/bridge.js")),
        )
        .route("/api/capture/ocr", post(phone_ocr).layer(DefaultBodyLimit::max(crate::ocr::MAX_IMAGE)))
        .route("/api/capture/ocr/config", get(phone_ocr_config))
        .route("/api/capture/connect", post(connect))
        .route("/api/capture/submit", post(submit))
        .route("/api/capture/status", post(connect))
        .route("/api/capture/end", post(end))
        // JSON escaping may take six bytes per character; bound bytes as well as characters.
        .layer(DefaultBodyLimit::max(MAX_TEXT * 6 + 1024))
        .layer(middleware::from_fn_with_state(state.clone(), public_guard))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

async fn ocr_config(State(state): State<Captures>) -> Json<Value> {
    Json(json!({"enabled": state.ocr.enabled()}))
}
fn authorize_ocr(state: &Captures, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = bearer(headers)?;
    let mut sessions = state.sessions.lock().unwrap();
    let s = writer(&mut sessions, token)?;
    let sequence = headers.get("x-capture-sequence").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if !s.connected || s.ended || s.text.is_some() || sequence != Some(s.sequence) {
        return Err(error(StatusCode::CONFLICT, "Captura fora de sequência ou encerrada."));
    }
    Ok(())
}
async fn phone_ocr_config(State(state): State<Captures>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    authorize_ocr(&state, &headers)?;
    Ok(ocr_config(State(state)).await)
}
async fn phone_ocr(State(state): State<Captures>, request: Request) -> Result<Json<Value>, ApiError> {
    let headers = request.headers().clone();
    authorize_ocr(&state, &headers)?;
    let result = state.ocr.recognize(request).await;
    authorize_ocr(&state, &headers)?;
    Ok(Json(json!({"text": result?})))
}
async fn desktop_ocr(State(state): State<Captures>, request: Request) -> Result<Json<Value>, ApiError> {
    let headers = request.headers();
    let origin = headers.get("origin").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<axum::http::Uri>().ok());
    let host = headers.get("host").and_then(|v| v.to_str().ok());
    if headers.get("x-ocr-request").and_then(|v| v.to_str().ok()) != Some("1")
        || headers.get("sec-fetch-site").is_some_and(|v| v != "same-origin")
        || origin.as_ref().is_some_and(|u| u.authority().map(|a| a.as_str()) != host)
    { return Err(error(StatusCode::FORBIDDEN, "Origem não autorizada.")); }
    Ok(Json(json!({"text": state.ocr.recognize(request).await?})))
}

async fn protect(State(state): State<Captures>, request: Request, next: Next) -> Response {
    // Keep the requested reader entry just long enough to authenticate an expired status.
    let reader_id = request.uri().path().strip_prefix("/api/captures/");
    state
        .sessions
        .lock()
        .unwrap()
        .retain(|id, s| s.deadline > Instant::now() || reader_id == Some(id.as_str()));
    let mut response = next.run(request).await;
    for (name, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self' https://cdn.jsdelivr.net https://tessdata.projectnaptha.com data:; worker-src 'self' blob:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
async fn public_guard(State(state): State<Captures>, request: Request, next: Next) -> Response {
    if !state.enabled()
        || request
            .headers()
            .get("origin")
            .is_some_and(|v| v.to_str().ok() != state.origin.as_deref())
    {
        return error(StatusCode::FORBIDDEN, "Origem não autorizada.").into_response();
    }
    next.run(request).await
}
fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|s| !s.is_empty())
        .ok_or_else(unauthorized)
}
fn unauthorized() -> ApiError {
    error(
        StatusCode::UNAUTHORIZED,
        "Token de captura inválido ou expirado.",
    )
}
async fn create(State(state): State<Captures>) -> Result<(StatusCode, Json<Value>), ApiError> {
    let origin = state.origin.as_ref().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "Captura por QR indisponível. Configure CAPTURE_PUBLIC_URL com uma origem HTTPS e encaminhe o túnel somente para 127.0.0.1:3001."))?;
    let mut sessions = state.sessions.lock().unwrap();
    if sessions.len() >= MAX_SESSIONS {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "Limite de capturas atingido. Cancele uma captura ou aguarde 15 minutos.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let read = uuid::Uuid::new_v4().to_string();
    let write = uuid::Uuid::new_v4().to_string();
    let result = json!({"id":id, "read_token":read, "url":format!("{origin}/capture#token={write}"), "expires_at":(chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()});
    sessions.insert(
        id,
        Session {
            read,
            write,
            deadline: Instant::now() + TTL,
            connected: false,
            text: None,
            sequence: 1,
            ended: false,
        },
    );
    Ok((StatusCode::CREATED, Json(result)))
}
async fn read(
    State(state): State<Captures>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = bearer(&headers)?;
    let sessions = state.sessions.lock().unwrap();
    let s = sessions
        .get(&id)
        .filter(|s| s.read == token)
        .ok_or_else(unauthorized)?;
    if s.deadline <= Instant::now() {
        return Ok(Json(json!({"status":"expired"})));
    }
    let mut result = session_status(s);
    if !s.ended { result["text"] = json!(s.text); }
    Ok(Json(result))
}
async fn cancel(
    State(state): State<Captures>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer(&headers)?;
    let mut sessions = state.sessions.lock().unwrap();
    sessions
        .get(&id)
        .filter(|s| s.read == token)
        .ok_or_else(unauthorized)?;
    sessions.remove(&id);
    Ok(StatusCode::NO_CONTENT)
}
fn writer<'a>(
    sessions: &'a mut HashMap<String, Session>,
    token: &str,
) -> Result<&'a mut Session, ApiError> {
    let s = sessions
        .values_mut()
        .find(|s| s.write == token && s.deadline > Instant::now())
        .ok_or_else(unauthorized)?;
    Ok(s)
}
async fn connect(
    State(state): State<Captures>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = bearer(&headers)?;
    let mut sessions = state.sessions.lock().unwrap();
    let s = writer(&mut sessions, token)?;
    s.connected = true;
    Ok(Json(session_status(s)))
}
async fn submit(
    State(state): State<Captures>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = bearer(&headers)?;
    let mut sessions = state.sessions.lock().unwrap();
    let s = writer(&mut sessions, token)?;
    let text = body.get("text").and_then(Value::as_str).ok_or_else(|| {
        error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Informe o texto da captura.",
        )
    })?;
    if text.chars().count() > MAX_TEXT {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "O texto deve ter no máximo 32000 caracteres.",
        ));
    }
    if text.trim().is_empty() {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "O texto não pode estar vazio.",
        ));
    }
    let sequence = body["sequence"].as_u64().ok_or_else(|| error(StatusCode::UNPROCESSABLE_ENTITY, "Informe a sequência."))?;
    if s.ended || sequence != s.sequence || s.text.as_ref().is_some_and(|old| old != text) {
        return Err(error(StatusCode::CONFLICT, "Captura fora de sequência ou encerrada."));
    }
    s.text = Some(text.to_owned());
    s.connected = true;
    Ok(Json(json!({"status":"received"})))
}

fn session_status(s: &Session) -> Value {
    json!({"status": if s.ended {"ended"} else if s.text.is_some() {"received"} else if s.connected {"connected"} else {"waiting"}, "sequence": s.sequence})
}

async fn end(State(state): State<Captures>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let mut sessions = state.sessions.lock().unwrap();
    let s = writer(&mut sessions, bearer(&headers)?)?;
    s.ended = true;
    s.text = None;
    Ok(Json(session_status(s)))
}

// Only the local reader can acknowledge a successfully saved capture. Duplicate
// acknowledgements never advance a newer capture; all transitions share one lock.
async fn ack(State(state): State<Captures>, Path(id): Path<String>, headers: HeaderMap, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let mut sessions = state.sessions.lock().unwrap();
    let token = bearer(&headers)?;
    let s = sessions.get_mut(&id).filter(|s| s.read == token && s.deadline > Instant::now()).ok_or_else(unauthorized)?;
    let sequence = body["sequence"].as_u64().ok_or_else(|| error(StatusCode::UNPROCESSABLE_ENTITY, "Informe a sequência."))?;
    if s.ended || sequence == 0 || sequence > s.sequence || (sequence == s.sequence && s.text.is_none()) {
        return Err(error(StatusCode::CONFLICT, "Captura não disponível para confirmação."));
    }
    if sequence == s.sequence {
        s.text = None;
        s.sequence += 1;
    }
    Ok(Json(session_status(s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    async fn open_session(state: Captures) -> (String, String, String) {
        // Returns (id, read_token, write_token).
        let request = Request::builder()
            .method("POST")
            .uri("/api/captures")
            .header("host", "localhost")
            .body(Body::empty())
            .unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
        let id = body["id"].as_str().unwrap().to_owned();
        let read = body["read_token"].as_str().unwrap().to_owned();
        let write = body["url"]
            .as_str()
            .unwrap()
            .split("#token=")
            .nth(1)
            .unwrap()
            .to_owned();
        (id, read, write)
    }
    async fn poll(state: Captures, id: &str, read: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .uri(format!("/api/captures/{id}"))
            .header("host", "localhost")
            .header("authorization", format!("Bearer {read}"))
            .body(Body::empty())
            .unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        (
            response.status(),
            serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap(),
        )
    }
    async fn public_post(
        state: Captures,
        path: &str,
        write: &str,
        body: Option<Value>,
        origin: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("host", "capture.test")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {write}"));
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let request = request
            .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
            .unwrap();
        let response = public_router(state.clone(), "frontend")
            .oneshot(request)
            .await
            .unwrap();
        (
            response.status(),
            serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap())
                .unwrap_or_default(),
        )
    }

    #[tokio::test]
    async fn rejects_foreign_hosts_and_origins() {
        crate::tests::rejects_foreign_hosts_and_origins_impl().await;
    }

    #[tokio::test]
    async fn pairing_flow() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (id, read, write) = open_session(state.clone()).await;
        let (status, body) = poll(state.clone(), &id, &read).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "waiting");
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/connect",
            &write,
            None,
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, body) = poll(state.clone(), &id, &read).await;
        assert_eq!(body["status"], "connected");
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            Some(json!({"sequence":1,"text":"hello there"})),
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, body) = poll(state.clone(), &id, &read).await;
        assert_eq!(body["status"], "received");
        assert_eq!(body["text"], "hello there");
        assert_eq!(id.len(), 36);
        assert_eq!(read.len(), 36);
        assert_eq!(write.len(), 36);
    }

    #[tokio::test]
    async fn reusable_sequence_ack_and_end() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (id, read, write) = open_session(state.clone()).await;
        for sequence in 1..=2 {
            let (status, _) = public_post(state.clone(), "/api/capture/submit", &write, Some(json!({"sequence":sequence,"text":"card"})), None).await;
            assert_eq!(status, StatusCode::OK);
            let (status, _) = public_post(state.clone(), "/api/capture/submit", &write, Some(json!({"sequence":sequence+1,"text":"next"})), None).await;
            assert_eq!(status, StatusCode::CONFLICT);
            for token in [&write, &read, &read] {
                let request = Request::builder().method("POST").uri(format!("/api/captures/{id}/ack"))
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"sequence":sequence}).to_string())).unwrap();
                let response = local_router(state.clone()).oneshot(request).await.unwrap();
                assert_eq!(response.status(), if token == &write {StatusCode::UNAUTHORIZED} else {StatusCode::OK});
            }
            let (_, body) = poll(state.clone(), &id, &read).await;
            assert_eq!(body["sequence"], sequence + 1);
            assert_eq!(body["status"], "connected");
            let (status, _) = public_post(state.clone(), "/api/capture/submit", &write, Some(json!({"sequence":sequence,"text":"stale"})), None).await;
            assert_eq!(status, StatusCode::CONFLICT);
        }
        for _ in 0..2 {
            let (status, body) = public_post(state.clone(), "/api/capture/end", &write, None, None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["status"], "ended");
        }
        let (status, _) = public_post(state.clone(), "/api/capture/submit", &write, Some(json!({"sequence":3,"text":"late"})), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn expiry_prunes_and_frees_slot() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (first, read_first, _) = open_session(state.clone()).await;
        let (second, read_second, _) = open_session(state.clone()).await;
        {
            let mut lock = state.sessions.lock().unwrap();
            lock.get_mut(&first).unwrap().deadline = Instant::now() - Duration::from_secs(1);
        }
        let (status, body) = poll(state.clone(), &first, &read_first).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "expired");
        // The expired session survived only for the reading request itself.
        open_session(state.clone()).await;
        assert_eq!(state.sessions.lock().unwrap().len(), 2);
        let (status, _) = poll(state.clone(), &second, &read_second).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn max_sessions_enforced() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        for _ in 0..MAX_SESSIONS {
            open_session(state.clone()).await;
        }
        let request = Request::builder()
            .method("POST")
            .uri("/api/captures")
            .header("host", "localhost")
            .body(Body::empty())
            .unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn cancel_removes_session() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (id, read, _) = open_session(state.clone()).await;
        let request = Request::builder()
            .method("DELETE")
            .uri(format!("/api/captures/{id}"))
            .header("host", "localhost")
            .header("authorization", format!("Bearer {read}"))
            .body(Body::empty())
            .unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (status, _) = poll(state.clone(), &id, &read).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn submit_retries_are_idempotent() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, _, write) = open_session(state.clone()).await;
        let body = Some(json!({"sequence":1,"text":"first"}));
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            body.clone(),
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            body,
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/connect",
            &write,
            None,
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn token_authorization_enforced() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (id, read, write) = open_session(state.clone()).await;
        for (path, write_) in [
            ("/api/capture/connect", read.as_str()),
            ("/api/capture/submit", read.as_str()),
            ("/api/capture/submit", "not-a-token"),
        ] {
            let body = Some(json!({"sequence":1,"text":"x"}));
            let (status, _) = public_post(
                state.clone(),
                path,
                write_,
                body,
                Some("https://capture.test"),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} with wrong token");
        }
        let request = Request::builder()
            .uri(format!("/api/captures/{id}"))
            .header("host", "localhost")
            .header("authorization", format!("Bearer {write}"))
            .body(Body::empty())
            .unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "read endpoint rejects write token"
        );
    }

    #[tokio::test]
    async fn public_routes_never_serve_admin() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        for path in [
            "/",
            "/index.html",
            "/assets/app.js",
            "/api/stores",
            "/api/health",
            "/api/captures",
        ] {
            let request = Request::builder()
                .uri(path)
                .header("host", "capture.test")
                .header("origin", "https://capture.test")
                .body(Body::empty())
                .unwrap();
            let router = public_router(state.clone(), "frontend")
                .fallback_service(ServeDir::new("no-such-dir"));
            let response = router.oneshot(request).await.unwrap();
            assert_ne!(
                response.status(),
                StatusCode::OK,
                "{path} must not resolve on public listener"
            );
        }
    }

    #[tokio::test]
    async fn public_requires_matching_origin() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, _, write) = open_session(state.clone()).await;
        let body = Some(json!({"sequence":1,"text":"x"}));
        for origin in ["https://evil.example", "http://capture.test"] {
            let (status, _) = public_post(
                state.clone(),
                "/api/capture/submit",
                &write,
                body.clone(),
                Some(origin),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "origin {origin}");
        }
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            body,
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn text_limits() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, _, write) = open_session(state.clone()).await;
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            Some(json!({"sequence":1,"text": "x".repeat(MAX_TEXT + 1)})),
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            Some(json!({"sequence":1,"text":"   "})),
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = public_post(
            state.clone(),
            "/api/capture/submit",
            &write,
            Some(json!({"note":"no text field"})),
            Some("https://capture.test"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn config_rejects_http_scheme() {
        let err = Captures::configured(Some("http://capture.test".into()), false).unwrap_err();
        assert!(err.contains("origem") || err.contains("http://"));
        assert!(
            Captures::configured(Some("https://capture.test/login?next=x".into()), false).is_err()
        );
        assert!(Captures::configured(Some("https://user@capture.test".into()), false).is_err());
        assert!(Captures::configured(Some("https://capture.test/path".into()), false).is_err());
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        assert!(state.enabled());
        // Modo rede local: HTTP só com o flag explícito CAPTURE_ALLOW_HTTP.
        assert!(Captures::configured(Some("http://capture.test".into()), false).is_err());
        let lan = Captures::configured(Some("http://192.168.0.10:3001".into()), true).unwrap();
        assert!(lan.enabled());
        assert_eq!(lan.origin_url(), Some("http://192.168.0.10:3001"));
        let unconfigured = Captures::configured(None, false).unwrap();
        assert!(!unconfigured.enabled());
        assert!(unconfigured.public_bind().is_none());
    }

    #[tokio::test]
    async fn invalid_config_never_aborts_startup() {
        let (captures, warning) =
            Captures::from_values(Some("https://capture.test/login".into()), false);
        assert!(warning.is_some());
        assert!(!captures.enabled());
        let (captures, warning) = Captures::from_values(None, false);
        assert!(warning.is_none());
        assert!(!captures.enabled());
    }

    #[tokio::test]
    async fn create_without_config_is_actionable_503() {
        let state = Captures::configured(None, false).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/api/captures")
            .header("host", "localhost")
            .body(Body::empty())
            .unwrap();
        let response = local_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("CAPTURE_PUBLIC_URL")
        );
    }

    #[tokio::test]
    async fn tokens_unique_per_session() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, read_a, write_a) = open_session(state.clone()).await;
        let (_, read_b, write_b) = open_session(state.clone()).await;
        assert_ne!(read_a, read_b);
        assert_ne!(write_b, write_a);
    }

    #[tokio::test]
    async fn phone_ocr_requires_sequence_header_and_active_state() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, _, _write) = open_session(state.clone()).await;
        let send = |write: &str, sequence: u64, body: Body| {
            let request = Request::builder()
                .method("POST")
                .uri("/api/capture/ocr")
                .header("host", "capture.test")
                .header("origin", "https://capture.test")
                .header("authorization", format!("Bearer {write}"))
                .header("x-capture-sequence", sequence.to_string())
                .header("content-type", "image/jpeg")
                .body(body)
                .unwrap();
            public_router(state.clone(), "frontend").oneshot(request)
        };
        let status = |response: axum::response::Response| response.status();
        let (_, _, write) = open_session(state.clone()).await;
        assert_eq!(status(send(&write, 1, Body::empty()).await.unwrap()), StatusCode::CONFLICT, "not connected");
        let _ = public_post(state.clone(), "/api/capture/connect", &write, None, Some("https://capture.test")).await;
        let response = send(&write, 2, Body::empty()).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "sequence ahead");
        let response = send(&write, 1, Body::empty()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "no ocr provider");
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert!(body["text"].is_null(), "error body never carries text: {body}");
        let _ = public_post(state.clone(), "/api/capture/submit", &write, Some(json!({"text":"hello","sequence":1})), None).await;
        let response = send(&write, 1, Body::empty()).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "pending text blocks cloud ocr");
    }

    #[tokio::test]
    async fn desktop_ocr_requires_same_origin_header() {
        let state = Captures::configured(None, false).unwrap();
        let build = |origin: Option<&str>, same_site: bool, marker: bool| {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/ocr")
                .header("host", "localhost")
                .header("content-type", "image/jpeg")
                .header("sec-fetch-site", if same_site { "same-origin" } else { "cross-site" });
            if marker { request = request.header("x-ocr-request", "1"); }
            if let Some(origin) = origin { request = request.header("origin", origin); }
            request.body(Body::empty()).unwrap()
        };

        for bad in [
            build(None, true, false),
            build(None, false, true),
            build(Some("http://evil.example"), true, true),
        ] {
            let response = local_router(state.clone()).oneshot(bad).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "same-origin guard");
        }
        let good = build(Some("http://localhost"), true, true);
        let response = local_router(state.clone()).oneshot(good).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "OCR disabled falls back to local Tesseract");
    }

    #[tokio::test]
    async fn ocr_config_exposes_only_enabled_boolean() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let request = Request::builder().uri("/api/ocr/config").header("host", "localhost").body(Body::empty()).unwrap();
        let response = local_router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        let keys: Vec<&str> = body.as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap();
        assert_eq!(keys, vec!["enabled"]);
        assert_eq!(body["enabled"], false, "OCR_PROVIDER unset in tests");
    }

    #[tokio::test]
    async fn phone_ocr_config_requires_authorization() {
        let state = Captures::configured(Some("https://capture.test".into()), false).unwrap();
        let (_, _read, _write) = open_session(state.clone()).await;
        let request = Request::builder()
            .method("GET")
            .uri("/api/capture/ocr/config")
            .header("host", "capture.test".to_string())
            .header("authorization", format!("Bearer {}", "wrong"))
            .body(Body::empty())
            .unwrap();
        let response = public_router(state.clone(), "frontend").oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
