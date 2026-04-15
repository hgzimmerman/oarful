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
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,lineup_server=debug,lineup_solver=debug,lineup_db=debug")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Subcommands: reset-tenant, reset-all
    match args.first().map(String::as_str) {
        Some("reset-tenant") => return cmd_reset_tenant(&args[1..]),
        Some("reset-all") => return cmd_reset_all(),
        _ => {}
    }

    let master_db =
        std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());
    let data_dir =
        std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let public_dir = std::env::var("PUBLIC_DIR").ok().unwrap_or_else(|| {
        // Fallback chain: exe_dir/public → workspace default.
        if let Ok(exe) = std::env::current_exe() {
            let exe_public = exe.parent().unwrap_or(exe.as_ref()).join("public");
            if exe_public.is_dir() {
                return exe_public.to_string_lossy().into_owned();
            }
        }
        "crates/server/public".to_string()
    });

    tracing::info!(%master_db, %data_dir, %port, %public_dir, "starting lineup_server");

    let mailer: std::sync::Arc<dyn lineup_server::mailer::Mailer> =
        std::sync::Arc::new(lineup_server::mailer::LogMailer);
    let app = lineup_server::build_router(&master_db, &data_dir, &public_dir, mailer)?;
    let host: std::net::IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|h| h.parse().ok())
        .unwrap_or_else(|| [127, 0, 0, 1].into());
    let addr = std::net::SocketAddr::from((host, port));
    println!("running at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

/// `cargo run -p lineup_server -- reset-tenant <slug>`
/// Deletes the tenant's DB file and removes it from the master DB.
fn cmd_reset_tenant(args: &[String]) -> Result<()> {
    let slug = args.first().ok_or_else(|| anyhow::anyhow!(
        "Usage: lineup_server reset-tenant <slug>\n  e.g. lineup_server reset-tenant default"
    ))?;
    let master_db = std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());

    if !std::path::Path::new(&master_db).exists() {
        println!("Master DB {master_db} does not exist — nothing to reset.");
        return Ok(());
    }

    let mut conn = lineup_master_db::connect_sync(&master_db)?;
    let tenant = lineup_master_db::tenant::Tenant::find_by_slug(&mut conn, slug)?;
    let Some(tenant) = tenant else {
        println!("No tenant with slug '{slug}' found.");
        return Ok(());
    };

    // Delete the DB file.
    let db_path = &tenant.db_path;
    if std::path::Path::new(db_path).exists() {
        std::fs::remove_file(db_path)?;
        println!("Deleted {db_path}");
    } else {
        println!("{db_path} does not exist (already deleted?)");
    }

    // Remove from master DB.
    lineup_master_db::tenant::Tenant::delete(&mut conn, tenant.id)?;
    println!("Removed tenant '{}' (id={}) from master DB.", tenant.name, tenant.id);
    Ok(())
}

/// `cargo run -p lineup_server -- reset-all`
/// Deletes ALL tenant DB files and the master DB itself.
fn cmd_reset_all() -> Result<()> {
    let master_db = std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());

    if std::path::Path::new(&master_db).exists() {
        let mut conn = lineup_master_db::connect_sync(&master_db)?;
        let tenants = lineup_master_db::tenant::Tenant::list_all(&mut conn)?;
        for t in &tenants {
            if std::path::Path::new(&t.db_path).exists() {
                std::fs::remove_file(&t.db_path)?;
                println!("Deleted {}", t.db_path);
            }
        }
        drop(conn);
        std::fs::remove_file(&master_db)?;
        println!("Deleted {master_db}");
    } else {
        println!("Master DB {master_db} does not exist — nothing to reset.");
    }

    Ok(())
}
