//! Binary entrypoint for lineup_server.
//!
//! Usage:
//!   DATABASE_URL=lineup.sql PORT=3000 cargo run -p lineup_server

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let conn_string =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "lineup.sql".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let public_dir = std::env::var("PUBLIC_DIR")
        .unwrap_or_else(|_| "crates/server/public".to_string());

    tracing::info!(%conn_string, %port, %public_dir, "starting lineup_server");

    let app = lineup_server::build_router(&conn_string, &public_dir)?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    println!("running at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
