//! Synthetic-snapshot benchmark for the solver. Generates DbSnapshots
//! of varying roster sizes without touching the database, runs the
//! solver on each, and reports wall-clock solve time.
//!
//! The goal is a cheap smoke test for scaling properties — specifically
//! "does doubling the roster double the solve time, or does it explode?"
//! Not a statistical benchmark; each N runs once.

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use std::io::Write;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::boat::{types::WeightClass as BoatWeightClass, Boat};
use lineup_db::boat::types::BoatId;
use lineup_db::rower::{
    types::{Height, RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength},
    Rower,
};
use lineup_db::snapshot::DbSnapshot;
use lineup_db::types::IntBool;
use lineup_solver::{solve, PartialFillPolicy, SolveRequest, SolveStatus, SolverConfig};
use std::collections::HashMap;

pub fn run() -> Result<()> {
    println!("=== Solver scaling benchmark ===");
    println!("5s time budget per case — measuring 'time to good answer',");
    println!("not time to proven-optimal.");
    println!("`soft fleet` = the 10-boat target (4 fours + 6 eights)");
    println!("`small fleet` = the original 3-boat fixture (8+, 4+, 4-)\n");

    let full_fleet = generate_full_fleet();
    let small_fleet = generate_small_fleet();
    let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

    // Axis A: vary roster size with the fixed small fleet. Shows how
    // the model scales with ROWER count on a simple 3-boat problem.
    println!("-- axis A: vary rowers, 3-boat fleet --");
    println!("{:<10} {:<18} {:<12} {}", "N rowers", "solve time", "status", "notes");
    println!("{}", "-".repeat(70));
    std::io::stdout().flush().ok();
    for &n in &[10usize, 15, 20, 25, 30, 40] {
        run_one(&small_fleet, date, n)?;
    }

    // Axis B: vary fleet size with a fixed 20-rower roster. Shows how
    // the model scales with BOAT count, which is the `use[b]` fleet
    // selection search space.
    println!("\n-- axis B: vary boats, 20-rower roster --");
    println!("{:<10} {:<18} {:<12} {}", "N boats", "solve time", "status", "notes");
    println!("{}", "-".repeat(70));
    std::io::stdout().flush().ok();
    for &n in &[1usize, 2, 3, 5, 7, 10] {
        let subfleet: Vec<_> = full_fleet.iter().take(n).cloned().collect();
        run_one_with_label(&subfleet, date, 20, n)?;
    }

    // Axis C: UI scenarios. Mirrors the lineup_server defaults
    // (top_n=3, partial=Strict, small fleet) and runs each cell N
    // times to surface tail latencies, since that's what budget
    // tuning actually cares about. This is the data feeding the
    // chosen DEFAULT_BUDGET_SECS in handlers/solve.rs.
    println!("\n-- axis C: UI defaults (top_n=3, small fleet), repeated --");
    println!(
        "{:<10} {:<10} {:<11} {:<11} {:<11} {:<11} {:<10}",
        "N rowers", "budget", "min", "median", "p95", "max", "outcomes",
    );
    println!("{}", "-".repeat(80));
    std::io::stdout().flush().ok();
    let samples = 5usize;
    for &n_rowers in &[10usize, 14, 20] {
        for &budget_secs in &[1u64, 2, 3, 5, 10] {
            run_repeated(&small_fleet, date, n_rowers, budget_secs, samples, 3)?;
        }
    }

    println!();
    Ok(())
}

fn run_one(fleet: &[Boat], date: NaiveDate, n_rowers: usize) -> Result<()> {
    run_one_with_label(fleet, date, n_rowers, n_rowers)
}

/// Run the same configuration `samples` times under the realistic UI
/// settings (`top_n` from the caller, partial-fill strict, default
/// weights) and report tail latencies. Counts how many runs ended in
/// each `SolveStatus` so the operator can spot a budget that's
/// causing partial-timeouts.
fn run_repeated(
    fleet: &[Boat],
    date: NaiveDate,
    n_rowers: usize,
    budget_secs: u64,
    samples: usize,
    top_n: usize,
) -> Result<()> {
    print!(
        "{:<10} {:<10} ",
        n_rowers,
        format!("{budget_secs}s"),
    );
    std::io::stdout().flush().ok();

    let rowers = generate_rowers(n_rowers);
    let availability = rowers
        .iter()
        .map(|r| (r.id, AvailabilityStatus::Yes))
        .collect::<HashMap<_, _>>();
    let snapshot = DbSnapshot {
        date,
        rowers,
        availability,
        sweep_boats: fleet.to_vec(),
        last_coxed: HashMap::new(),
        seat_affinities: vec![],
        pair_affinities: vec![],
        recent_placements: vec![],
    };
    let request = SolveRequest {
        date,
        boats: vec![],
        partial_fill: PartialFillPolicy::Strict,
        novelty_factor: 0,
        config: SolverConfig::default(),
        time_budget: Some(std::time::Duration::from_secs(budget_secs)),
        top_n,
        tabu_min_diff: 2,
    };

    let mut elapsed: Vec<std::time::Duration> = Vec::with_capacity(samples);
    let mut sat = 0usize;
    let mut unsat = 0usize;
    let mut timeout = 0usize;
    for _ in 0..samples {
        let start = std::time::Instant::now();
        let result = solve(&snapshot, &request)?;
        elapsed.push(start.elapsed());
        match result.status {
            SolveStatus::Satisfied => sat += 1,
            SolveStatus::Unsatisfiable => unsat += 1,
            SolveStatus::Timeout => timeout += 1,
        }
    }

    elapsed.sort();
    let min = elapsed.first().copied().unwrap_or_default();
    let max = elapsed.last().copied().unwrap_or_default();
    let median = elapsed[elapsed.len() / 2];
    // For N=5 the "p95" is just the max — there aren't enough samples
    // for a real percentile, but reporting both keeps the column
    // structure consistent if we crank `samples` later.
    let p95_idx = ((elapsed.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = elapsed[p95_idx.min(elapsed.len() - 1)];

    let outcomes = format!("{sat}s/{unsat}u/{timeout}t");
    println!(
        "{:<11} {:<11} {:<11} {:<11} {:<10}",
        format!("{min:?}"),
        format!("{median:?}"),
        format!("{p95:?}"),
        format!("{max:?}"),
        outcomes,
    );
    std::io::stdout().flush().ok();
    Ok(())
}

fn run_one_with_label(
    fleet: &[Boat],
    date: NaiveDate,
    n_rowers: usize,
    label: usize,
) -> Result<()> {
    print!("{:<10} ", label);
    std::io::stdout().flush().ok();

    let rowers = generate_rowers(n_rowers);
    let availability = rowers
        .iter()
        .map(|r| (r.id, AvailabilityStatus::Yes))
        .collect::<HashMap<_, _>>();

    let snapshot = DbSnapshot {
        date,
        rowers,
        availability,
        sweep_boats: fleet.to_vec(),
        last_coxed: HashMap::new(),
        seat_affinities: vec![],
        pair_affinities: vec![],
        recent_placements: vec![],
    };

    let request = SolveRequest {
        date,
        boats: vec![],
        partial_fill: PartialFillPolicy::Strict,
        // S7 novelty disabled — the synthetic snapshots carry no
        // history anyway (`recent_placements` is empty), so this
        // is just clarity-of-intent.
        novelty_factor: 0,
        // Default weights — the benchmark measures baseline scaling
        // behaviour, not weight-tuning effects.
        config: SolverConfig::default(),
        time_budget: Some(std::time::Duration::from_secs(5)),
        // Top-N is irrelevant to scaling measurements — stick with
        // the historical single-solution path.
        top_n: 1,
        tabu_min_diff: 2,
    };

    let start = std::time::Instant::now();
    let result = solve(&snapshot, &request)?;
    let elapsed = start.elapsed();

    let status = match result.status {
        SolveStatus::Satisfied => "satisfied",
        SolveStatus::Unsatisfiable => "UNSAT",
        SolveStatus::Timeout => "timeout",
    };
    let used = result.primary.lineups.iter().filter(|l| l.used).count();
    let total_lineups = result.primary.lineups.len();
    let placed: usize = result
        .primary
        .lineups
        .iter()
        .filter(|l| l.used)
        .map(|l| l.seats.len())
        .sum();
    let notes = format!("{used}/{total_lineups} boats, {placed} placed");
    println!("{:<18} {:<12} {}", format!("{:?}", elapsed), status, notes);
    std::io::stdout().flush().ok();
    Ok(())
}

/// The original 3-boat fixture — an 8+, a 4+, and a coxless 4.
fn generate_small_fleet() -> Vec<Boat> {
    let mut boats = Vec::new();
    let mut id = 1i32;
    push_boat(
        &mut boats,
        &mut id,
        "Persephone",
        BoatWeightClass::Heavy,
        8,
        true,
        Side::Starboard,
    );
    push_boat(
        &mut boats,
        &mut id,
        "Artemis",
        BoatWeightClass::Medium,
        4,
        true,
        Side::Starboard,
    );
    push_boat(
        &mut boats,
        &mut id,
        "Hestia",
        BoatWeightClass::Light,
        4,
        false,
        Side::Port,
    );
    boats
}

/// Ten-boat fleet matching the target real-world club inventory: 4
/// fours and 6 eights, weight classes spread across the roster.
fn generate_full_fleet() -> Vec<Boat> {
    let mut boats = Vec::new();
    let mut id = 1i32;

    // Four 4-boats: 2 Medium + 2 Light, mixing coxed/coxless and rigs.
    push_boat(&mut boats, &mut id, "Four-M1", BoatWeightClass::Medium, 4, true, Side::Starboard);
    push_boat(&mut boats, &mut id, "Four-M2", BoatWeightClass::Medium, 4, true, Side::Port);
    push_boat(&mut boats, &mut id, "Four-L1", BoatWeightClass::Light, 4, false, Side::Starboard);
    push_boat(&mut boats, &mut id, "Four-L2", BoatWeightClass::Light, 4, false, Side::Port);

    // Six 8-boats: 2 Light, 2 Medium, 2 Heavy. All coxed.
    push_boat(&mut boats, &mut id, "Eight-L1", BoatWeightClass::Light, 8, true, Side::Starboard);
    push_boat(&mut boats, &mut id, "Eight-L2", BoatWeightClass::Light, 8, true, Side::Port);
    push_boat(&mut boats, &mut id, "Eight-M1", BoatWeightClass::Medium, 8, true, Side::Starboard);
    push_boat(&mut boats, &mut id, "Eight-M2", BoatWeightClass::Medium, 8, true, Side::Port);
    push_boat(&mut boats, &mut id, "Eight-H1", BoatWeightClass::Heavy, 8, true, Side::Starboard);
    push_boat(&mut boats, &mut id, "Eight-H2", BoatWeightClass::Heavy, 8, true, Side::Port);

    boats
}

fn push_boat(
    boats: &mut Vec<Boat>,
    id: &mut i32,
    name: &str,
    weight_class: BoatWeightClass,
    seat_count: i32,
    has_cox: bool,
    stroke_side: Side,
) {
    boats.push(Boat {
        id: BoatId::new(*id),
        name: name.to_string(),
        weight_class,
        seat_count,
        has_cox: IntBool::new(has_cox),
        oars_per_seat: 1,
        acquired_at: None,
        manufactured_at: None,
        relinquished_at: None,
        stroke_side,
    });
    *id += 1;
}

/// Generate N rowers with properties cycling through the enum variants.
/// The first two are designated coxes, the next three are soft-cox
/// eligible, the remainder row only. Sides rotate roughly 40/40/20
/// Port/Starboard/Either.
fn generate_rowers(n: usize) -> Vec<Rower> {
    let now = Utc::now().naive_utc();

    // 40/40/20 Port/Starboard/Either gives the S4 wrong-side term count
    // a realistic distribution. Pattern of length 5 = P, P, S, S, E.
    let side_cycle = [
        Side::Port,
        Side::Port,
        Side::Starboard,
        Side::Starboard,
        Side::Either,
    ];
    let weight_cycle = [
        RowerWeightClass::Light,
        RowerWeightClass::Medium,
        RowerWeightClass::Medium,
        RowerWeightClass::Heavy,
    ];
    let skill_cycle = [
        Skill::Novice,
        Skill::Intermediate,
        Skill::Master,
        Skill::Expert,
    ];
    let strength_cycle = [
        Strength::Weak,
        Strength::Intermediate,
        Strength::Strong,
        Strength::VeryStrong,
    ];
    let height_cycle = [
        Height::Short,
        Height::Medium,
        Height::Tall,
        Height::VeryTall,
    ];

    (1..=n as i32)
        .map(|i| Rower {
            id: RowerId::new(i),
            name: format!("R{i:02}"),
            email: None,
            weight_class: weight_cycle[(i as usize - 1) % weight_cycle.len()],
            skill: skill_cycle[(i as usize - 1) % skill_cycle.len()],
            strength: strength_cycle[(i as usize - 1) % strength_cycle.len()],
            height: height_cycle[(i as usize - 1) % height_cycle.len()],
            side: side_cycle[(i as usize - 1) % side_cycle.len()],
            side_strength: SideStrength::default(),
            can_scull: IntBool::FALSE,
            can_cox: IntBool::new(i <= 5), // first 5 can cox
            is_designated_cox: IntBool::new(i <= 2), // first 2 are designated
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
        })
        .collect()
}
