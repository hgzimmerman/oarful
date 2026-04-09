//! Milestone 1 + 2 CLI.
//!
//! Usage:
//!   cargo run -p lineup_cli                     # dump snapshot for default date
//!   cargo run -p lineup_cli -- 2026-04-11       # dump snapshot for given date
//!   cargo run -p lineup_cli -- solve            # solve for default date (strict, dry-run)
//!   cargo run -p lineup_cli -- solve 2026-04-11
//!   cargo run -p lineup_cli -- solve --partial N [date]
//!                                               # allow up to N empty optional seats per boat
//!   cargo run -p lineup_cli -- solve --commit [date]
//!                                               # solve AND persist the chosen lineups
//!   cargo run -p lineup_cli -- solve --novelty N [date]
//!                                               # penalise lineups within N seats of a historical one
//!   cargo run -p lineup_cli -- history [date]   # show committed lineups for a date
//!   cargo run -p lineup_cli -- sync-sheet <ID> [--gid N]
//!                                               # pull availability from a public Google Sheet

mod bench;

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use lineup_db::{
    fixture,
    lineup::{CommitSeat, Lineup},
    practice::Practice,
    snapshot::DbSnapshot,
    state::Db,
};
use lineup_solver::{
    solve, PartialFillPolicy, ProposedLineup, SolveRequest, SolveStatus, SolverConfig,
};
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
        Some("solve") => {
            let opts = parse_solve_args(&args[1..])?;
            cmd_solve(&db, opts).await
        }
        Some("history") => cmd_history(&db, parse_date(args.get(1))?).await,
        Some("bench") => bench::run(),
        Some("sync-sheet") => cmd_sync_sheet(&db, &args[1..]).await,
        Some(other) if other != "dump" => {
            cmd_dump(&db, parse_date(Some(&other.to_string()))?).await
        }
        Some(_) => cmd_dump(&db, parse_date(args.get(1))?).await,
        None => cmd_dump(&db, default_date()).await,
    }
}

#[derive(Debug, Clone)]
struct SolveOpts {
    date: NaiveDate,
    partial: PartialFillPolicy,
    commit: bool,
    novelty: i32,
}

/// Parse `solve` subcommand arguments: `--partial N`, `--commit`,
/// `--novelty N`, and an optional date. Missing date defaults to
/// `DEFAULT_DATE`.
fn parse_solve_args(args: &[String]) -> Result<SolveOpts> {
    let mut partial = PartialFillPolicy::Strict;
    let mut commit = false;
    let mut novelty: i32 = 0;
    let mut date: Option<NaiveDate> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--partial" => {
                let n: i32 = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--partial requires a number"))?
                    .parse()
                    .map_err(|e| anyhow::anyhow!("--partial N must be an integer: {e}"))?;
                partial = if n <= 0 {
                    PartialFillPolicy::Strict
                } else {
                    PartialFillPolicy::Allowed(n)
                };
                i += 2;
            }
            "--commit" => {
                commit = true;
                i += 1;
            }
            "--novelty" => {
                novelty = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--novelty requires a number"))?
                    .parse()
                    .map_err(|e| anyhow::anyhow!("--novelty N must be an integer: {e}"))?;
                if novelty < 0 {
                    novelty = 0;
                }
                i += 2;
            }
            other => {
                date = Some(parse_date(Some(&other.to_string()))?);
                i += 1;
            }
        }
    }
    Ok(SolveOpts {
        date: date.unwrap_or_else(default_date),
        partial,
        commit,
        novelty,
    })
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

async fn cmd_solve(db: &Db, opts: SolveOpts) -> Result<()> {
    let SolveOpts {
        date,
        partial,
        commit,
        novelty,
    } = opts;
    let snapshot = db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await?;

    let num_available = snapshot.available_rowers().count();

    println!("=== Solving lineup for {date} ===");
    println!("  {num_available} rowers available for sweep");
    println!("  partial-fill policy: {partial:?}");
    println!("  novelty factor: {novelty}");
    println!("  commit mode: {}", if commit { "yes" } else { "dry-run" });
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
    // variables (see S8). Default to a 10-second time budget so real
    // fleets don't hang the coach UI — the bench shows the solver
    // finds near-optimal solutions within the first ~second; the
    // remaining budget is proof-of-optimality / marginal redistribution.
    let request = SolveRequest {
        date,
        boats: vec![],
        partial_fill: partial,
        novelty_factor: novelty,
        // No CLI flags for individual per-constraint weights yet —
        // a config-file loader is future work. Default preserves
        // the historical behaviour.
        config: SolverConfig::default(),
        time_budget: Some(std::time::Duration::from_secs(10)),
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

            if commit {
                let used_owned: Vec<ProposedLineup> =
                    used.into_iter().cloned().collect();
                let committed = commit_lineups(db, date, &used_owned).await?;
                println!(
                    "\nCommitted {} lineup(s) to the database.",
                    committed
                );
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

/// Persist the given `ProposedLineup`s to `lineup` + `lineup_seat`.
/// Looks up (or creates) the practice for the date, then replaces any
/// existing committed lineup per boat with the solver's choice.
async fn commit_lineups(
    db: &Db,
    date: NaiveDate,
    used: &[ProposedLineup],
) -> Result<usize> {
    let used_owned: Vec<ProposedLineup> = used.to_vec();
    db.with_conn(move |conn| {
        let practice = Practice::upsert_by_date(conn, date, None)?;
        let mut count = 0usize;
        for lineup in &used_owned {
            let seats: Vec<CommitSeat> = lineup
                .seats
                .iter()
                .map(|(seat, rower_id)| CommitSeat {
                    seat_position: *seat,
                    rower_id: *rower_id,
                    is_cox: *seat == 0,
                })
                .collect();
            Lineup::commit_for_boat(conn, practice.id, lineup.boat_id, &seats)?;
            count += 1;
        }
        Ok(count)
    })
    .await
}

async fn cmd_history(db: &Db, date: NaiveDate) -> Result<()> {
    let snapshot = db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await?;
    let committed = db
        .with_conn(move |conn| {
            let Some(practice) = Practice::find_by_date(conn, date)? else {
                return Ok(None);
            };
            Lineup::for_practice(conn, practice.id).map(Some)
        })
        .await?;

    let Some(committed) = committed else {
        println!("No practice committed for {date}.");
        return Ok(());
    };

    if committed.is_empty() {
        println!("Practice exists for {date} but no lineups are committed yet.");
        return Ok(());
    }

    println!("=== Committed lineups for {date} ({}) ===", committed.len());
    for c in &committed {
        let boat = snapshot
            .sweep_boats
            .iter()
            .find(|b| b.id == c.lineup.boat_id);
        let boat_name = boat.map(|b| b.name.as_str()).unwrap_or("<unknown>");
        println!(
            "\nLineup #{} — boat #{} {} (committed {})",
            c.lineup.id, c.lineup.boat_id, boat_name, c.lineup.created_at,
        );
        for seat in &c.seats {
            let rower = snapshot.rowers.iter().find(|r| r.id == seat.rower_id);
            let name = rower.map(|r| r.name.as_str()).unwrap_or("<unknown>");
            let label = if seat.is_cox.as_bool() {
                "cox".to_string()
            } else {
                format!("seat {}", seat.seat_position)
            };
            println!("  {:<8} {name}", label);
        }
    }
    Ok(())
}

async fn cmd_sync_sheet(db: &Db, args: &[String]) -> Result<()> {
    // Parse positional sheet ID + optional --gid flag.
    let mut sheet_id: Option<String> = None;
    let mut gid: u32 = 0;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--gid" => {
                gid = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--gid requires a number"))?
                    .parse()
                    .map_err(|e| anyhow::anyhow!("--gid N must be a non-negative integer: {e}"))?;
                i += 2;
            }
            positional if sheet_id.is_none() => {
                sheet_id = Some(positional.to_string());
                i += 1;
            }
            other => {
                anyhow::bail!("unexpected argument to sync-sheet: {other}");
            }
        }
    }
    let sheet_id = sheet_id.ok_or_else(|| {
        anyhow::anyhow!(
            "usage: lineup_cli sync-sheet <SPREADSHEET_ID> [--gid N]"
        )
    })?;

    println!("=== Syncing sheet {sheet_id} (gid={gid}) ===");

    // `db.with_conn` takes a sync closure, so we can't .await the HTTP
    // fetch inside it. Split the work: fetch the CSV in the outer async
    // context, then hand the resulting String into a sync closure that
    // runs `lineup_sheets::sync_csv` on a pooled connection.
    let url = format!(
        "https://docs.google.com/spreadsheets/d/{sheet_id}/export?format=csv&gid={gid}"
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "sheet csv export returned HTTP {}: is the sheet set to \
             'Anyone with the link can view'?",
            resp.status()
        );
    }
    let csv_text: String = resp.text().await?;

    let year = chrono::Utc::now().date_naive().year();

    // `lineup_sheets::sync_csv` returns `anyhow::Result` but
    // `db.with_conn` demands a closure returning
    // `Result<_, diesel::result::Error>`. We flatten anyhow errors
    // into a sqlite-shaped error inside the closure, then unwrap it
    // on the outside.
    let sync_result: Result<lineup_sheets::SyncSummary> = db
        .with_conn(move |conn| {
            match lineup_sheets::sync_csv(&csv_text, year, conn) {
                Ok(summary) => Ok(Ok(summary)),
                Err(e) => Ok(Err(e)),
            }
        })
        .await?;
    let summary = sync_result?;

    println!(
        "Sync complete. Read {} rows ({} sweep, {} sculling). \
         Created {} rowers, updated {}. Upserted {} availability entries.",
        summary.rows_read,
        summary.sweep_rows,
        summary.sculling_rows,
        summary.rowers_created,
        summary.rowers_updated,
        summary.availabilities_upserted,
    );
    if summary.rows_skipped_no_email > 0 {
        println!(
            "  Skipped {} rows without an email.",
            summary.rows_skipped_no_email
        );
    }
    if !summary.warnings.is_empty() {
        println!("\nWarnings ({}):", summary.warnings.len());
        for w in &summary.warnings {
            println!("  - {w}");
        }
    }
    Ok(())
}
