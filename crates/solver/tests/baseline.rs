//! End-to-end solver regression test against the toy fixture.
//!
//! Seeds `lineup_db::fixture` into a fresh in-memory sqlite, builds
//! a `DbSnapshot` for the fixture's practice date, then runs
//! `solve()` in four scenarios (default, --partial 2, --novelty 1,
//! --novelty 2) and compares a compact, deterministic rendering of
//! the result against a checked-in expected file.
//!
//! # Philosophy
//!
//! This is the test-suite equivalent of the tmp `check.sh` script
//! used during the ModelBuilder refactor. The point is catching
//! "did a refactor (or a seemingly-innocuous constraint tweak)
//! silently change the optimum on a representative problem?". Any
//! change to the output means **something** about the model or the
//! fixture shifted, and the developer needs to decide whether it's
//! intentional (accept the new output) or a regression (fix the
//! code).
//!
//! # The `UPDATE_BASELINES` env var
//!
//! When the solver's behaviour *legitimately* changes — e.g. a new
//! soft constraint lands, or a bug fix shifts the optimum — the
//! expected files need to be regenerated. Run:
//!
//! ```text
//! UPDATE_BASELINES=1 cargo test -p lineup_solver --test baseline
//! ```
//!
//! and the tests will overwrite the expected files with the
//! current output instead of diffing. Review the diff in git
//! before committing, and include a note in the commit message
//! explaining *why* the baseline moved.
//!
//! # Why a custom format
//!
//! The CLI renders a verbose, human-friendly block that includes
//! the policy echo, elapsed-time line, and rower trait details.
//! That's great for coaches but noisy for regression testing — a
//! single cosmetic tweak to the CLI output format would break
//! every baseline. The format below is minimal: solve status,
//! counts, skipped-boat names, and per-boat seat → rower name.

use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::NaiveDate;
use lineup_db::fixture;
use lineup_db::rower::types::RowerId;
use lineup_db::snapshot::DbSnapshot;
use lineup_db::test_support::in_memory_conn;
use lineup_solver::{
    solve, PartialFillPolicy, ProposedLineup, SolveRequest, SolveResult, SolveStatus,
    SolverConfig,
};

/// The practice date baked into the toy fixture by
/// `fixture::seed_all`. Used for every scenario so the availability
/// set is stable.
const FIXTURE_DATE: (i32, u32, u32) = (2026, 4, 11);

fn fixture_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(FIXTURE_DATE.0, FIXTURE_DATE.1, FIXTURE_DATE.2).unwrap()
}

/// Build a snapshot by seeding the toy fixture into a scratch
/// in-memory db and then reading it back the same way the CLI
/// does. This intentionally goes through the full db layer
/// (migrations + inserts + queries) so the regression test also
/// exercises the schema + query path, not just the solver.
fn fixture_snapshot() -> DbSnapshot {
    let mut conn = in_memory_conn();
    fixture::seed_if_empty(&mut conn).expect("seeding toy fixture");
    let team = lineup_db::team::Team::first(&mut conn)
        .expect("querying first team")
        .expect("fixture should seed a team");
    DbSnapshot::for_team_date(&mut conn, team.id, fixture_date())
        .expect("building snapshot from seeded fixture")
}

/// Assemble a `SolveRequest` for the given scenario.
///
/// **Time budget.** Pumpkin on the toy fixture finds its optimum
/// within the first few hundred milliseconds but then spends the
/// rest of its budget trying to *prove* optimality — that proof
/// search doesn't terminate cheaply on this size problem, so the
/// solver always burns the full budget. We pick 2 seconds: long
/// enough that a single test doesn't slow down the watch loop,
/// short enough that `cargo test` doesn't take 30+ seconds. The
/// returned "best found so far" is deterministic because Pumpkin's
/// search order is deterministic given a fixed variable creation
/// order (which the fixture + constraint builder guarantees).
fn request(partial_fill: PartialFillPolicy, novelty_factor: i32) -> SolveRequest {
    SolveRequest {
        date: fixture_date(),
        boats: Vec::new(), // empty = consider every in-service sweep boat
        partial_fill,
        novelty_factor,
        config: SolverConfig::default(),
        time_budget: Some(std::time::Duration::from_secs(5)),
        top_n: 1,
        tabu_min_diff: 2,
    }
}

/// Render a `SolveResult` into a deterministic string for baseline
/// comparison. Deliberately minimal: status line, fielded-count
/// summary, alphabetised skipped-boat list, and each fielded boat
/// with its seat → rower assignments. No timestamps, no rower
/// trait details, no policy preamble — just the raw decision.
///
/// The snapshot is passed alongside so we can resolve `RowerId`
/// values back to names (the solver result only carries IDs).
fn format_result(result: &SolveResult, snapshot: &DbSnapshot) -> String {
    let name_of = |id: RowerId| -> String {
        snapshot
            .rowers
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| format!("<unknown rower #{id}>"))
    };

    let mut out = String::new();
    writeln!(out, "status: {:?}", result.status).unwrap();

    // Only produce body text for satisfied results. Unsat / timeout
    // are recorded by the status line alone.
    if result.status != SolveStatus::Satisfied {
        return out;
    }

    let used: Vec<&ProposedLineup> =
        result.primary.lineups.iter().filter(|l| l.used).collect();
    let skipped: Vec<&ProposedLineup> =
        result.primary.lineups.iter().filter(|l| !l.used).collect();

    writeln!(
        out,
        "fielded: {}/{}",
        used.len(),
        result.primary.lineups.len()
    )
    .unwrap();

    // Sort skipped boats by name so the line is stable across
    // solver-internal iteration order changes.
    let mut skipped_names: Vec<&str> =
        skipped.iter().map(|l| l.boat_name.as_str()).collect();
    skipped_names.sort_unstable();
    writeln!(out, "skipped: {}", skipped_names.join(", ")).unwrap();

    // Unplaced-rowers breakdown. Always emit both lines — even
    // when a bucket is empty — so the baseline file has a stable
    // shape regardless of solve outcome. Sort each bucket by
    // name so the output is deterministic across runs.
    let mut to_sculling_names: Vec<String> = result
        .primary
        .unplaced
        .to_sculling
        .iter()
        .map(|id| name_of(*id))
        .collect();
    to_sculling_names.sort();
    writeln!(out, "to sculling: {}", to_sculling_names.join(", ")).unwrap();

    let mut benched_names: Vec<String> = result
        .primary
        .unplaced
        .benched
        .iter()
        .map(|id| name_of(*id))
        .collect();
    benched_names.sort();
    writeln!(out, "benched: {}", benched_names.join(", ")).unwrap();
    writeln!(out).unwrap();

    // Sort fielded boats by boat_id so the order is stable. Seats
    // inside each lineup are already sorted by position per
    // `decode_solution`, but we re-sort defensively.
    let mut used_sorted: Vec<&ProposedLineup> = used.clone();
    used_sorted.sort_by_key(|l| l.boat_id);

    for (i, lineup) in used_sorted.iter().enumerate() {
        if i > 0 {
            writeln!(out).unwrap();
        }
        writeln!(out, "boat {}: {}", lineup.boat_id, lineup.boat_name).unwrap();
        let mut seats = lineup.seats.clone();
        seats.sort_by_key(|&(s, _)| s);
        for (seat, rower_id) in seats {
            let label = if seat == 0 {
                "cox".to_string()
            } else {
                format!("s{seat}")
            };
            writeln!(out, "  {:<4} {}", label, name_of(rower_id)).unwrap();
        }
    }

    out
}

/// Compare `actual` against the baseline file at
/// `crates/solver/tests/baselines/{name}.txt`. On mismatch, prints
/// a diff-friendly error message. Set `UPDATE_BASELINES=1` in the
/// environment to overwrite the file with `actual` and pass the
/// assertion — use this after a legitimate behaviour change, and
/// review the resulting diff in git before committing.
fn assert_matches_baseline(name: &str, actual: &str) {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("baselines")
        .join(format!("{name}.txt"));

    if std::env::var("UPDATE_BASELINES").is_ok() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .expect("creating baselines directory");
        }
        std::fs::write(&path, actual)
            .unwrap_or_else(|e| panic!("writing baseline {}: {e}", path.display()));
        eprintln!("updated baseline {}", path.display());
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "reading baseline {} (run with UPDATE_BASELINES=1 to create it): {e}",
            path.display()
        )
    });

    if actual != expected {
        // Show a minimal inline diff so the failing output is
        // obvious without having to pipe through external tools.
        let mut msg = format!(
            "baseline mismatch for {name}\n\
             Re-run with UPDATE_BASELINES=1 to accept the new output.\n\n\
             --- expected ({}) ---\n{expected}\n\
             --- actual ---\n{actual}",
            path.display()
        );
        // Append a line-count summary so tests running in CI logs
        // don't bury the key info.
        let _ = writeln!(
            msg,
            "\n(expected {} lines, got {} lines)",
            expected.lines().count(),
            actual.lines().count()
        );
        panic!("{msg}");
    }
}

/// Convenience wrapper: seed the fixture, solve with the given
/// request, render, and diff against the named baseline.
fn run_baseline(name: &str, request: SolveRequest) {
    let snapshot = fixture_snapshot();
    let result = solve(&snapshot, &request).expect("solve should not error");
    let rendered = format_result(&result, &snapshot);
    assert_matches_baseline(name, &rendered);
}

// ---------- Scenarios ----------

#[test]
fn baseline_default() {
    // Vanilla: Strict partial-fill, no novelty, default config.
    // This is the "does the toy fixture still produce a sane
    // Persephone lineup" sanity check that caught every error
    // during the ModelBuilder refactor.
    run_baseline("default", request(PartialFillPolicy::Strict, 0));
}

#[test]
fn baseline_partial_fill_two() {
    // Allow up to two optional seats empty on an 8+. With 11
    // rowers and the fixture fleet (8+, 4+, 4-), this gives the
    // solver the option to field a partially-filled Persephone
    // rather than being forced into a smaller boat.
    run_baseline("partial2", request(PartialFillPolicy::Allowed(2), 0));
}

#[test]
fn baseline_novelty_one() {
    // No historical lineups in the toy fixture, so S7 has nothing
    // to compare against and novelty=1 is a no-op. Capturing
    // anyway as a regression guard for the S7 gating path — if
    // the "no historical placements" short-circuit ever breaks,
    // this scenario will diverge from default.
    run_baseline("novelty1", request(PartialFillPolicy::Strict, 1));
}

#[test]
fn baseline_novelty_two() {
    run_baseline("novelty2", request(PartialFillPolicy::Strict, 2));
}
