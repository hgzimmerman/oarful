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
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("info,lineup_server=debug,lineup_solver=debug,lineup_db=debug")
        }))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Subcommands
    match args.first().map(String::as_str) {
        Some("reset-tenant") => return cmd_reset_tenant(&args[1..]),
        Some("reset-all") => return cmd_reset_all(),
        Some("seed") => return cmd_seed().await,
        Some("import") => return cmd_import(&args[1..]).await,
        _ => {}
    }

    let master_db = std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
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
    let slug = args.first().ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: lineup_server reset-tenant <slug>\n  e.g. lineup_server reset-tenant default"
        )
    })?;
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
    println!(
        "Removed tenant '{}' (id={}) from master DB.",
        tenant.name, tenant.id
    );
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

/// `cargo run -p lineup_server -- import <path.db>`
/// Imports an exported SQLite database as the default tenant. Copies
/// the file into the data directory and registers (or reuses) the
/// "default" tenant in the master DB. Runs migrations on the imported
/// DB to bring it up to date.
async fn cmd_import(args: &[String]) -> Result<()> {
    let path = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("Usage: lineup_server import <path.db>"))?;
    if !std::path::Path::new(path).exists() {
        anyhow::bail!("File not found: {path}");
    }

    let master_db = std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());

    // Ensure data directories exist.
    std::fs::create_dir_all(format!("{data_dir}/demos"))?;
    std::fs::create_dir_all(format!("{data_dir}/tenants"))?;

    // Ensure master DB exists (MasterDb::connect runs migrations).
    let master = lineup_master_db::state::MasterDb::connect(&master_db)?;

    // Copy the file to data/default.db.
    let dest = format!("{data_dir}/default.db");
    std::fs::copy(path, &dest)?;
    println!("Copied {path} -> {dest}");

    // Register in master DB if not already present.
    let dest_for_closure = dest.clone();
    master
        .with_conn(move |conn| {
            if lineup_master_db::tenant::Tenant::find_by_slug(conn, "default")?.is_none() {
                let now = chrono::Utc::now().naive_utc();
                lineup_master_db::tenant::Tenant::create(
                    conn,
                    lineup_master_db::tenant::NewTenant {
                        name: "Default Club".to_string(),
                        slug: "default".to_string(),
                        db_path: dest_for_closure,
                        created_at: now,
                        billing_status: "active".to_string(),
                        trial_expires_at: None,
                    },
                )?;
                println!("Registered 'default' tenant in master DB.");
            } else {
                println!("'default' tenant already exists in master DB.");
            }
            Ok(())
        })
        .await?;

    // Connect to tenant DB — this runs migrations to bring it up to date.
    let db = lineup_db::state::Db::connect(&dest)?;
    // Verify it opens successfully by running a trivial query.
    db.with_conn(|conn| {
        use diesel::prelude::*;
        diesel::sql_query("SELECT 1").execute(conn)?;
        Ok(())
    })
    .await?;

    println!("Imported {path} as default tenant at {dest}");
    println!("Migrations applied (if any). Ready to start the server.");
    Ok(())
}

/// `cargo run -p lineup_server -- seed`
/// Ensures the default tenant exists and seeds it with the fleet-only
/// fixture (boats + team + dev coach account, no rowers). Safe to run
/// repeatedly — skips if already seeded.
async fn cmd_seed() -> Result<()> {
    let master_db = std::env::var("MASTER_DB").unwrap_or_else(|_| "master.db".to_string());
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());

    // Ensure data directories exist.
    std::fs::create_dir_all(format!("{data_dir}/demos"))?;
    std::fs::create_dir_all(format!("{data_dir}/tenants"))?;

    // Ensure master DB + default tenant exist (MasterDb::connect runs migrations).
    let master = lineup_master_db::state::MasterDb::connect(&master_db)?;

    let db_path = format!("{data_dir}/default.db");
    let db_path_for_closure = db_path.clone();
    master
        .with_conn(move |conn| {
            if lineup_master_db::tenant::Tenant::find_by_slug(conn, "default")?.is_none() {
                let now = chrono::Utc::now().naive_utc();
                lineup_master_db::tenant::Tenant::create(
                    conn,
                    lineup_master_db::tenant::NewTenant {
                        name: "Default Club".to_string(),
                        slug: "default".to_string(),
                        db_path: db_path_for_closure,
                        created_at: now,
                        billing_status: "active".to_string(),
                        trial_expires_at: None,
                    },
                )?;
            }
            Ok(())
        })
        .await?;

    // Connect to tenant DB (runs migrations), seed fixture.
    let db = lineup_db::state::Db::connect(&db_path)?;
    db.with_conn(|conn| lineup_db::fixture::seed_fleet_only(conn))
        .await?;

    println!("Default tenant seeded (fleet + dev coach account).");
    println!("  Login: coach@test.com / 12345");
    Ok(())
}
