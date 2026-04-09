//! Milestone 1 + 2 CLI.
//!
//! Usage:
//!   cargo run -p lineup_cli                    # dump snapshot for default date
//!   cargo run -p lineup_cli -- 2026-04-11      # dump snapshot for given date
//!   cargo run -p lineup_cli -- solve           # solve for default date
//!   cargo run -p lineup_cli -- solve 2026-04-11

use anyhow::Result;
use chrono::NaiveDate;
use lineup_db::{fixture, snapshot::DbSnapshot, state::Db};
use lineup_solver::{solve, SolveRequest, SolveStatus};
use tracing_subscriber::EnvFilter;

const DEFAULT_DATE: (i32, u32, u32) = (2026, 4, 11);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let db_path = std::env::var("DATABASE_URL").unwrap_or_else(|_| "lineup.sql".to_string());
    let db = Db::connect(&db_path)?;
    db.with_conn(|conn| fixture::seed_if_empty(conn)).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("solve") => cmd_solve(&db, parse_date(args.get(1))?).await,
        Some(other) if other != "dump" => cmd_dump(&db, parse_date(Some(&other.to_string()))?).await,
        Some(_) => cmd_dump(&db, parse_date(args.get(1))?).await,
        None => cmd_dump(&db, default_date()).await,
    }
}

fn default_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(DEFAULT_DATE.0, DEFAULT_DATE.1, DEFAULT_DATE.2)
        .expect("valid default date")
}

fn parse_date(s: Option<&String>) -> Result<NaiveDate> {
    match s {
        None => Ok(default_date()),
        Some(raw) => NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid date '{raw}': {e}")),
    }
}

async fn cmd_dump(db: &Db, date: NaiveDate) -> Result<()> {
    let snapshot = db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await?;
    print!("{snapshot}");
    Ok(())
}

async fn cmd_solve(db: &Db, date: NaiveDate) -> Result<()> {
    let snapshot = db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await?;

    let num_available = snapshot.available_rowers().count();

    println!("=== Solving lineup for {date} ===");
    println!("  {num_available} rowers available for sweep");
    println!("  candidate fleet: {} boat(s)", snapshot.sweep_boats.len());
    for b in &snapshot.sweep_boats {
        println!(
            "    #{} {} (seats={} cox={})",
            b.id,
            b.name,
            b.seat_count,
            b.has_cox.as_bool()
        );
    }
    println!();

    // Empty boat list = "consider every in-service sweep boat"; the
    // solver now picks which to field via its own `use[b]` decision
    // variables (see S8).
    let request = SolveRequest {
        date,
        boats: vec![],
    };
    let started = std::time::Instant::now();
    let result = solve(&snapshot, &request)?;
    let elapsed = started.elapsed();

    match result.status {
        SolveStatus::Satisfied => {
            let used: Vec<_> = result.lineups.iter().filter(|l| l.used).collect();
            let skipped: Vec<_> = result.lineups.iter().filter(|l| !l.used).collect();
            // `seats` includes the cox slot; subtract it for the "rowers
            // placed" count so cox isn't double-counted with rowers.
            let rowers_placed: usize = used
                .iter()
                .map(|l| l.seats.iter().filter(|(s, _)| *s != 0).count())
                .sum();
            let coxes_placed: usize = used
                .iter()
                .map(|l| l.seats.iter().filter(|(s, _)| *s == 0).count())
                .sum();
            let benched = num_available.saturating_sub(rowers_placed + coxes_placed);
            println!(
                "--- Solved in {:?}. Fielding {}/{} boats, {} rowers placed (+{} cox), {} on the dock. ---",
                elapsed,
                used.len(),
                result.lineups.len(),
                rowers_placed,
                coxes_placed,
                benched,
            );

            if !skipped.is_empty() {
                print!("Skipped:");
                for lineup in &skipped {
                    print!(" {}", lineup.boat_name);
                }
                println!();
            }

            for lineup in &used {
                println!("\nBoat #{} {}", lineup.boat_id, lineup.boat_name);
                for (seat, rower_id) in &lineup.seats {
                    let rower = snapshot
                        .rowers
                        .iter()
                        .find(|r| r.id == *rower_id)
                        .expect("solver returned an unknown rower id");
                    let label = if *seat == 0 {
                        "cox".to_string()
                    } else {
                        format!("seat {seat}")
                    };
                    println!(
                        "  {:<8} {:<20} [{}/{}/{}, side={}]",
                        label,
                        rower.name,
                        rower.weight_class,
                        rower.skill,
                        rower.strength,
                        rower.side
                    );
                }
            }
        }
        SolveStatus::Unsatisfiable => {
            println!("UNSATISFIABLE: no seat assignment exists under the current constraints.");
        }
        SolveStatus::Timeout => {
            println!("TIMEOUT: solver did not finish within its budget.");
        }
    }
    Ok(())
}
