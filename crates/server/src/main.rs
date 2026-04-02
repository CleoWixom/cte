mod db;
mod normalizer;
mod opencellid;
mod routes;
mod state;

use anyhow::Context;
use axum::Router;
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let redis_url    = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let oci_key      = std::env::var("OPENCELLID_KEY").unwrap_or_default();
    let listen_addr  = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    // Optional: serve pre-built frontend from this directory (e.g. web/dist)
    let static_dir   = std::env::var("STATIC_DIR").ok();

    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .context("Failed to connect to Postgres")?;

    sqlx::migrate!("../../migrations").run(&db).await.context("Migrations failed")?;

    let redis = redis::Client::open(redis_url.as_str()).context("Redis client failed")?;
    let http  = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let state = Arc::new(AppState { db, redis, http, oci_key });

    let api_router = routes::router().with_state(Arc::clone(&state));

    // Combine API + optional static file serving
    let app = if let Some(ref dir) = static_dir {
        tracing::info!("Serving static files from {dir}");
        let index = format!("{dir}/index.html");
        Router::new()
            .nest("/", api_router)
            .fallback_service(
                ServeDir::new(dir).fallback(ServeFile::new(index))
            )
    } else {
        Router::new().nest("/", api_router)
    };

    let app = app
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("Listening on http://{listen_addr}");
    if let Some(dir) = static_dir {
        tracing::info!("Frontend: http://{listen_addr}  (static from {dir})");
    }
    axum::serve(listener, app).await?;
    Ok(())
}
