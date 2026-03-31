use sqlx::PgPool;

pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub http: reqwest::Client,
    pub oci_key: String,
}
