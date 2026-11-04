use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{get, put},
};
use distributor_model::Store;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{str::FromStr, time::Duration};
use tower_http::services::{ServeDir, ServeFile};

type ApiError = (StatusCode, Json<serde_json::Value>);
mod capture;
mod ocr;
pub use crate::capture::{Captures, local_router, public_router};
fn error(status: StatusCode, message: &str) -> ApiError {
    (status, Json(serde_json::json!({"error": message})))
}
fn internal(_e: impl std::fmt::Display) -> ApiError {
    eprintln!("Database operation or stored data decoding failed");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Não foi possível acessar os dados. Tente novamente.",
    )
}

fn connection_options(url: &str) -> Result<PgConnectOptions, &'static str> {
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err("DATABASE_URL must be a PostgreSQL connection URL");
    }
    let mut options = PgConnectOptions::from_str(url)
        .map_err(|_| "Invalid PostgreSQL connection URL; check DATABASE_URL and percent-encode password special characters")?;
    if matches!(options.get_ssl_mode(), PgSslMode::Prefer | PgSslMode::Allow) {
        options = options.ssl_mode(PgSslMode::Require);
    }
    Ok(options.statement_cache_capacity(0))
}

async fn database(options: PgConnectOptions) -> Result<PgPool, &'static str> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(
            |_| "PostgreSQL connection failed; check database credentials, TLS, and network access",
        )?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stores (id TEXT PRIMARY KEY NOT NULL, data TEXT NOT NULL, insertion_order BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE NOT NULL)",
    )
    .persistent(false)
    .execute(&pool)
    .await
    .map_err(|_| "PostgreSQL schema initialization failed; check schema compatibility and CREATE permissions")?;
    Ok(pool)
}
async fn local_guard(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let allowed =
        std::env::var("ALLOWED_HOSTS").unwrap_or_else(|_| "localhost,127.0.0.1,[::1]".into());
    if let Some(host) = request.headers().get("host").and_then(|h| h.to_str().ok()) {
        let hostname = host
            .parse::<axum::http::uri::Authority>()
            .ok()
            .map(|a| a.host().to_owned())
            .unwrap_or_default();
        if !allowed.split(',').any(|h| h.trim() == hostname) {
            return error(StatusCode::FORBIDDEN, "Host não autorizado.").into_response();
        }
    }
    if let Some(origin) = request
        .headers()
        .get("origin")
        .and_then(|h| h.to_str().ok())
    {
        let hostname = origin
            .parse::<axum::http::Uri>()
            .ok()
            .and_then(|u| u.host().map(str::to_owned))
            .unwrap_or_default();
        if !allowed.split(',').any(|h| h.trim() == hostname) {
            return error(StatusCode::FORBIDDEN, "Origem não autorizada.").into_response();
        }
    }
    next.run(request).await
}

fn app(pool: PgPool, captures: Captures) -> Router {
    Router::new()
        .route(
            "/api/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/api/stores", get(list).post(create))
        .route("/api/stores/{id}", put(update).delete(remove))
        .merge(local_router(captures).with_state(()))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(axum::middleware::from_fn(local_guard))
        .with_state(pool)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configuração lida de .env na raiz do projeto (variáveis de ambiente do
    // processo têm prioridade; não registramos o conteúdo no log).
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required; set a PostgreSQL connection URL")?;
    let pool = database(connection_options(&url)?).await?;
    let (captures, warning) = Captures::from_env();
    if let Some(reason) = warning {
        eprintln!("Captura por QR desativada: {reason}");
    }
    let dist = std::env::var("FRONTEND_DIST").unwrap_or_else(|_| "frontend/dist".into());
    let service =
        ServeDir::new(&dist).not_found_service(ServeFile::new(format!("{dist}/index.html")));
    let address = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!("Better Distributors: http://{address} (uso local, sem autenticação)");
    if let Some(bind) = captures.public_bind() {
        let frontend = std::env::var("CAPTURE_FRONTEND_DIR").unwrap_or_else(|_| "frontend".into());
        let listener = tokio::net::TcpListener::bind(&bind).await?;
        println!("Captura por QR: http://{bind} (bridge público, sem painel)");
        let public = public_router(captures.clone(), &frontend);
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, public).await {
                eprintln!("Capture bridge error: {e}");
            }
        });
    }
    axum::serve(listener, app(pool, captures).fallback_service(service)).await?;
    Ok(())
}
async fn list(State(pool): State<PgPool>) -> Result<Json<Vec<Store>>, ApiError> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT data FROM stores ORDER BY insertion_order DESC")
            .persistent(false)
            .fetch_all(&pool)
            .await
            .map_err(internal)?;
    let stores = rows
        .into_iter()
        .map(|(json,)| serde_json::from_str(&json))
        .collect::<Result<Vec<Store>, _>>()
        .map_err(internal)?;
    Ok(Json(stores))
}
fn prepare(mut store: Store) -> Result<Store, ApiError> {
    store.name = store.name.trim().to_owned();
    store
        .validate()
        .map_err(|m| error(StatusCode::UNPROCESSABLE_ENTITY, m))?;
    store.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(store)
}
async fn create(
    State(pool): State<PgPool>,
    Json(store): Json<Store>,
) -> Result<(StatusCode, Json<Store>), ApiError> {
    let mut store = prepare(store)?;
    store.id = ulid::Ulid::new().to_string();
    let json = serde_json::to_string(&store).map_err(internal)?;
    sqlx::query("INSERT INTO stores(id,data) VALUES ($1,$2)")
        .bind(&store.id)
        .bind(json)
        .persistent(false)
        .execute(&pool)
        .await
        .map_err(internal)?;
    Ok((StatusCode::CREATED, Json(store)))
}
async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(store): Json<Store>,
) -> Result<Json<Store>, ApiError> {
    let mut store = prepare(store)?;
    store.id = id;
    let json = serde_json::to_string(&store).map_err(internal)?;
    let result = sqlx::query("UPDATE stores SET data=$1 WHERE id=$2")
        .bind(json)
        .bind(&store.id)
        .persistent(false)
        .execute(&pool)
        .await
        .map_err(internal)?;
    if result.rows_affected() == 0 {
        return Err(error(StatusCode::NOT_FOUND, "Loja não encontrada."));
    }
    Ok(Json(store))
}
async fn remove(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM stores WHERE id=$1")
        .bind(id)
        .persistent(false)
        .execute(&pool)
        .await
        .map_err(internal)?;
    if result.rows_affected() == 0 {
        return Err(error(StatusCode::NOT_FOUND, "Loja não encontrada."));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    struct TestDatabase {
        admin: PgPool,
        pool: PgPool,
        options: PgConnectOptions,
        schema: String,
    }

    impl TestDatabase {
        async fn new() -> Self {
            let url = std::env::var("TEST_DATABASE_URL")
                .expect("Database tests require TEST_DATABASE_URL pointing to a dedicated PostgreSQL test database; DATABASE_URL is never used");
            let options = connection_options(&url).expect("Invalid TEST_DATABASE_URL");
            let admin = PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(10))
                .connect_with(options.clone())
                .await
                .unwrap_or_else(|_| {
                    panic!("Cannot connect to TEST_DATABASE_URL; start the PostgreSQL test service")
                });
            let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
            sqlx::query(&format!("CREATE SCHEMA {schema}"))
                .persistent(false)
                .execute(&admin)
                .await
                .expect("Test database role requires CREATE SCHEMA permission");
            let options = options.options([("search_path", schema.as_str())]);
            let pool = database(options.clone()).await.unwrap();
            Self {
                admin,
                pool,
                options,
                schema,
            }
        }

        async fn cleanup(self) {
            self.pool.close().await;
            sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
                .persistent(false)
                .execute(&self.admin)
                .await
                .expect("Test schema cleanup failed");
            self.admin.close().await;
        }
    }

    #[test]
    fn postgres_connection_policy() {
        for url in [
            "postgres://localhost/test",
            "postgresql://localhost/test?sslmode=prefer",
        ] {
            let options = connection_options(url).unwrap();
            assert!(matches!(options.get_ssl_mode(), PgSslMode::Require));
        }
        assert!(matches!(
            connection_options("postgres://localhost/test?sslmode=disable")
                .unwrap()
                .get_ssl_mode(),
            PgSslMode::Disable
        ));
        assert!(matches!(
            connection_options("postgres://localhost/test?sslmode=verify-full")
                .unwrap()
                .get_ssl_mode(),
            PgSslMode::VerifyFull
        ));
        for url in [
            "sqlite::memory:",
            "",
            "postgres://user:secret@localhost:invalid/test",
        ] {
            let message = connection_options(url).unwrap_err();
            assert!(!message.contains("secret"));
            assert!(!message.contains(url) || url.is_empty());
        }
    }
    #[tokio::test]
    pub async fn rejects_foreign_hosts_and_origins() {
        rejects_foreign_hosts_and_origins_impl().await;
    }
    pub async fn rejects_foreign_hosts_and_origins_impl() {
        let db = TestDatabase::new().await;
        let router = app(db.pool.clone(), Captures::default());
        for (header, value) in [
            ("host", "evil.example:3000"),
            ("origin", "https://evil.example"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/stores")
                        .header(header, value)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/stores")
                    .header("host", "localhost:8080")
                    .header("origin", "http://localhost:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn persists_across_connections() {
        let db = TestDatabase::new().await;
        let pool = db.pool.clone();
        let store = Store {
            name: "Persistente".into(),
            ..Default::default()
        };
        let _ = create(State(pool.clone()), Json(store)).await.unwrap();
        pool.close().await;
        let reopened = database(db.options.clone()).await.unwrap();
        let Json(stores) = list(State(reopened.clone())).await.unwrap();
        assert_eq!(stores[0].name, "Persistente");
        reopened.close().await;
        db.cleanup().await;
    }

    #[tokio::test]
    async fn insertion_order_survives_updates() {
        let db = TestDatabase::new().await;
        let (_, Json(first)) = create(
            State(db.pool.clone()),
            Json(Store {
                name: "First".into(),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let (_, Json(second)) = create(
            State(db.pool.clone()),
            Json(Store {
                name: "Second".into(),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let _ = update(
            State(db.pool.clone()),
            Path(first.id.clone()),
            Json(first.clone()),
        )
        .await
        .unwrap();
        let Json(stores) = list(State(db.pool.clone())).await.unwrap();
        assert_eq!(
            stores.iter().map(|s| &s.id).collect::<Vec<_>>(),
            vec![&second.id, &first.id]
        );
        let missing = update(State(db.pool.clone()), Path("missing".into()), Json(first))
            .await
            .unwrap_err();
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn crud_and_validation() {
        let db = TestDatabase::new().await;
        let router = app(db.pool.clone(), Captures::default());
        let send = |method: &str, path: &str, data: serde_json::Value| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap()
        };
        let bad = router
            .clone()
            .oneshot(send("POST", "/api/stores", serde_json::json!({"name":""})))
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = router
            .clone()
            .oneshot(send(
                "POST",
                "/api/stores",
                serde_json::json!({"name":"Fornecedor"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), 100000).await.unwrap();
        let mut store: Store = serde_json::from_slice(&bytes).unwrap();
        let path = format!("/api/stores/{}", store.id);
        store.name = "Atualizado".into();
        assert_eq!(
            router
                .clone()
                .oneshot(send("PUT", &path, serde_json::to_value(store).unwrap()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/stores")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let stores: Vec<Store> =
            serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
        assert_eq!(stores.len(), 1);
        assert_eq!(stores[0].name, "Atualizado");
        assert_eq!(
            router
                .clone()
                .oneshot(send("DELETE", &path, serde_json::json!({})))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            router
                .oneshot(send("DELETE", &path, serde_json::json!({})))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        db.cleanup().await;
    }
}
