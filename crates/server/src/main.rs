//! Binary entrypoint for lineup_server.
//!
//! Usage:
//!   DATABASE_URL=lineup.sql PORT=3000 cargo run -p lineup_server
//!
//! The master database (`MASTER_DB`, defaults to `master.db`) tracks
//! the tenant registry. The tenant database (`DATABASE_URL`) holds
//! the actual rowing data. Phase 4 will resolve tenants dynamically
//! from JWT claims; for now a single tenant is hard-coded.

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let master_db =
        std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());
    let tenant_db =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "lineup.sql".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let public_dir = std::env::var("PUBLIC_DIR")
        .unwrap_or_else(|_| "crates/server/public".to_string());

    tracing::info!(%master_db, %tenant_db, %port, %public_dir, "starting lineup_server");

    let mailer: std::sync::Arc<dyn lineup_server::mailer::Mailer> =
        std::sync::Arc::new(lineup_server::mailer::LogMailer);
    let app = lineup_server::build_router(&master_db, &tenant_db, &public_dir, mailer)?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    println!("running at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
