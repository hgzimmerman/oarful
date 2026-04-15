//! Constraint-level unit tests for the lineup solver.
//!
//! Each test constructs a minimal synthetic `DbSnapshot` directly
//! (bypassing the database layer) and runs `solve()` against it
//! with a tailored `SolverConfig` that isolates a single soft
//! constraint. The assertion then checks that the placements match
//! what the constraint is supposed to encourage.
//!
//! **Isolating a single constraint.** The default `SolverConfig`
//! has every weight on at 1, which is great for normal use but
//! makes unit tests ambiguous — two constraints tugging in
//! different directions can both be "right" in their own sense.
//! The helper `solo_config()` builds a config where only one named
//! field is non-zero, plus a positive placement reward (so the
//! solver actually fields boats) and a positive weight_class slack
//! (so it picks the right boat class when multiple candidates are
//! offered). Every other soft weight is set to 0, which skips the
//! block entirely in the refactored solver.
//!
//! **Standard alternating rig note.** With `stroke_side = Starboard`:
//!   - 4-boat: seat 1 Port, seat 2 Starboard, seat 3 Port, seat 4 Starboard
//!   - 8-boat: seat 1 Port, seat 2 Starboard, ..., seat 8 Starboard
//! So Port seats are the odd numbers and Starboard seats are even.
//! The fixtures below rely on this pattern when deciding which
//! side to assign a synthetic rower.

use std::collections::HashMap;

use chrono::NaiveDate;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::boat::{
    types::{BoatId, CoxPosition, WeightClass},
    Boat,
};
use lineup_db::pair_affinity::PairAffinity;
use lineup_db::rower::types::{
    Height, RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength,
};
use lineup_db::rower::Rower;
use lineup_db::seat_affinity::{SeatAffinity, SeatZone};
use lineup_db::snapshot::DbSnapshot;
use lineup_db::rower::types::SweepBias;
use lineup_db::types::{AffinityWeight, IntBool};
use lineup_solver::{
    solve, PartialFillPolicy, ProposedLineup, SolveRequest, SolveStatus, SolverConfig,
};

const TEST_DATE: (i32, u32, u32) = (2026, 5, 1);

fn test_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(TEST_DATE.0, TEST_DATE.1, TEST_DATE.2).unwrap()
}

/// Construct a minimal rower with lots of defaulted fields. Tests
/// override only the attributes they care about by passing them
/// positionally.
fn rower(
    id: i32,
    name: &str,
    wc: RowerWeightClass,
    skill: Skill,
    strength: Strength,
    height: Height,
    side: Side,
) -> Rower {
    let now = chrono::Utc::now().naive_utc();
    Rower {
        id: RowerId::new(id),
        name: name.into(),
        weight_class: wc,
        skill,
        strength,
        height,
        side,
        side_strength: SideStrength::default(),
        sweep_bias: SweepBias::SWEEP_HARD,
        can_cox: IntBool::TRUE,
        is_designated_cox: IntBool::FALSE,
        active: IntBool::TRUE,
        created_at: now,
        updated_at: now,
    }
}

/// Build a designated-cox rower. Designated coxes are rejected from
/// all rowing seats by eligibility, so they only ever occupy seat 0.
fn cox_rower(id: i32, name: &str) -> Rower {
    let mut r = rower(
        id,
        name,
        RowerWeightClass::Light,
        Skill::Expert,
        Strength::Weak,
        Height::Short,
        Side::Either,
    );
    r.is_designated_cox = IntBool::TRUE;
    r
}

/// Standard 4+ boat, Medium class, stroke on Starboard. Under
/// alternating rig this means Port seats are {1, 3} and Starboard
/// seats are {2, 4}.
fn four_boat(id: i32, name: &str) -> Boat {
    Boat {
        id: BoatId::new(id),
        name: name.into(),
        weight_class: WeightClass::Medium,
        seat_count: 4,
        has_cox: IntBool::TRUE,
        oars_per_seat: 1,
        acquired_at: None,
        manufactured_at: None,
        relinquished_at: None,
        stroke_side: Side::Starboard,
        cox_position: CoxPosition::Bow,
    }
}

/// Standard 8+ boat, Medium class, stroke on Starboard. Port seats
/// are {1, 3, 5, 7}, Starboard seats are {2, 4, 6, 8}.
fn eight_boat(id: i32, name: &str) -> Boat {
    Boat {
        id: BoatId::new(id),
        name: name.into(),
        weight_class: WeightClass::Medium,
        seat_count: 8,
        has_cox: IntBool::TRUE,
        oars_per_seat: 1,
        acquired_at: None,
        manufactured_at: None,
        relinquished_at: None,
        stroke_side: Side::Starboard,
        cox_position: CoxPosition::Stern,
    }
}

/// Build a DbSnapshot with every supplied rower marked as available
/// for sweep. Callers that need a less-friendly availability mix
/// should build the HashMap themselves.
fn snapshot(rowers: Vec<Rower>, boats: Vec<Boat>) -> DbSnapshot {
    let availability: HashMap<RowerId, AvailabilityStatus> = rowers
        .iter()
        .map(|r| (r.id, AvailabilityStatus::Yes))
        .collect();
    DbSnapshot {
        date: test_date(),
        rowers,
        availability,
        sweep_boats: boats,
        last_coxed: HashMap::new(),
        last_benched: HashMap::new(),
        seat_affinities: Vec::new(),
        pair_affinities: Vec::new(),
        recent_placements: Vec::new(),
    }
}

/// Config where every soft weight is zero except the few that are
/// needed to keep the solver fielding boats at all. Tests opt one
/// constraint back in by mutating the returned config.
fn silent_config() -> SolverConfig {
    SolverConfig {
        skill_variance_weight: 0,
        pair_affinity_weight: 0,
        seat_affinity_weight: 0,
        side_preference_weight: 0,
        weight_class_slack_weight: 0,
        cox_cooldown_penalty: 0,
        // S8 is kept on so the solver actually prefers fielding the
        // test boat over leaving it on the dock. Without this, a
        // "nothing is rewarded" model has no incentive to produce
        // any assignment and the solver may bench everyone.
        placement_reward_weight: 1,
        pair_strength_weight: 0,
        bow_pair_strength_weight: 0,
        height_balance_weight: 0,
        end_pair_skill_weight: 0,
        engine_room_strength_weight: 0,
        // Partial-fill bonus off by default in silent_config so
        // tests only opt it in explicitly when they're exercising
        // the partial-fill path.
        partial_fill_bonus: 0,
        // S13 retention off so tests don't accidentally couple
        // sweep_bias values into their assertions; the retention
        // test opts it back in explicitly.
        non_scull_retention_weight: 0,
        bow_cox_fit_weight: 0,
        top_boat_stacking_weight: 0,
        pair_eligibility_weight: 0,
        minimize_bench_weight: 0,
        boat_size_stacking_weight: 0,
        bench_cooldown_penalty: 0,
        stroke_spread_weight: 0,
        eight_bias: 0,
        coxed_four_bias: 0,
        four_bias: 0,
        quad_bias: 0,
        pair_bias: 0,
        double_bias: 0,
        single_bias: 0,
    }
}

/// Standard request wrapping a config. Strict partial-fill, no
/// novelty, and no time budget — these test instances are tiny
/// (one boat, <10 rowers) and prove-optimal in milliseconds, so
/// we let Pumpkin run to proof rather than risking a "timed out
/// with a good-but-not-optimal solution" false negative.
fn request(config: SolverConfig) -> SolveRequest {
    SolveRequest {
        date: test_date(),
        boats: Vec::new(),
        partial_fill: PartialFillPolicy::Strict,
        config,
        time_budget: None,
        top_n: 1,
        tabu_min_diff: 2,
        reference_lineups: vec![],
        locks: vec![],
        required_boats: vec![],
    }
}

/// Find the single fielded lineup and assert the solver actually
/// fielded it. Returns a reference to the lineup so tests can
/// inspect its seats.
fn single_used<'a>(lineups: &'a [ProposedLineup]) -> &'a ProposedLineup {
    let used: Vec<&ProposedLineup> = lineups.iter().filter(|l| l.used).collect();
    assert_eq!(
        used.len(),
        1,
        "expected exactly one boat fielded, got {} ({:?})",
        used.len(),
        used.iter().map(|l| &l.boat_name).collect::<Vec<_>>()
    );
    used[0]
}

/// Look up which rower ended up in a given seat. Panics if the
/// seat is empty (test bug).
fn rower_in_seat(lineup: &ProposedLineup, seat: i32) -> RowerId {
    lineup
        .seats
        .iter()
        .find(|(s, _)| *s == seat)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("no rower in seat {seat} (seats: {:?})", lineup.seats))
}

/// Return the 2-seat partition `(s_lo, s_hi)` containing the given
/// rower in the lineup, or None if the rower isn't in any partition
/// (e.g. they're the cox).
fn partition_of(lineup: &ProposedLineup, rower_id: RowerId) -> Option<(i32, i32)> {
    let seat = lineup
        .seats
        .iter()
        .find(|(_, r)| *r == rower_id)
        .map(|(s, _)| *s)?;
    if seat == 0 {
        return None;
    }
    // Partitions are (1,2), (3,4), (5,6), (7,8). Given a seat `s`,
    // the partition starts at the odd number at or below `s`.
    let s_lo = if seat % 2 == 1 { seat } else { seat - 1 };
    Some((s_lo, s_lo + 1))
}

// ---------- Tests ----------

#[test]
fn s2_pair_affinity_seats_the_pair_together() {
    // 4+ boat, 4 rowers (2 port + 2 starboard). A pair affinity
    // links Alice (Port) and Diego (Starboard). Without the
    // affinity, the solver might split them across the two
    // partitions; with a positive weight, they must land in the
    // same 2-seat partition.
    let rowers = vec![
        rower(1, "Alice", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "Bob",   RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "Carla", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "Diego", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let mut snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);
    snap.pair_affinities.push(PairAffinity {
        rower_a_id: RowerId::new(1), // Alice
        rower_b_id: RowerId::new(4), // Diego  (canonical: a_id < b_id)
        weight: AffinityWeight::new(5),
    });

    let mut cfg = silent_config();
    cfg.pair_affinity_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    let alice_part = partition_of(lineup, RowerId::new(1)).unwrap();
    let diego_part = partition_of(lineup, RowerId::new(4)).unwrap();
    assert_eq!(
        alice_part, diego_part,
        "Alice and Diego should share a 2-seat partition (Alice in {alice_part:?}, Diego in {diego_part:?})"
    );
}

#[test]
fn s3_seat_affinity_places_rower_in_preferred_seat() {
    // Grace has a +5 Stroke zone affinity. In a 4+ that maps to seat 4.
    // With S3 on and everything else off, she should land there.
    let rowers = vec![
        rower(1, "Alice", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "Bob",   RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "Carla", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "Grace", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let mut snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);
    snap.seat_affinities.push(SeatAffinity {
        rower_id: RowerId::new(4),
        zone: SeatZone::Stroke,
        weight: AffinityWeight::new(5),
    });

    let mut cfg = silent_config();
    cfg.seat_affinity_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    assert_eq!(
        rower_in_seat(lineup, 4),
        RowerId::new(4),
        "Grace should be in seat 4, but seats are {:?}",
        lineup.seats
    );
}

#[test]
fn s4_soft_side_prefers_on_side_placement() {
    // Three Starboard-by-default rowers; two are HARD-locked,
    // one is soft. Fleet needs two Port and two Starboard seats
    // filled. The hard locks prevent HardA/HardB from ever
    // appearing on Port seats (eligibility filter), so the sole
    // Starboard-origin candidate for the remaining Port slot is
    // SoftSide. This is a hard-constraint test dressed as S4 —
    // the soft weight is only on so the S4 block is populated,
    // but the test would pass even without it thanks to
    // eligibility.
    let rowers = vec![
        {
            let mut r = rower(1, "HardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard);
            r.side_strength = SideStrength::HARD;
            r
        },
        {
            let mut r = rower(2, "HardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard);
            r.side_strength = SideStrength::HARD;
            r
        },
        {
            // SoftSide is nominally Starboard but soft-locked, so
            // the eligibility filter allows her on Port seats with
            // a per-placement S4 penalty.
            let mut r = rower(3, "SoftSide", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard);
            r.side_strength = SideStrength::soft(3);
            r
        },
        {
            let mut r = rower(4, "PortOne", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.side_strength = SideStrength::HARD;
            r
        },
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut cfg = silent_config();
    cfg.side_preference_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // Port seats in a Starboard-stroke 4+ are {1, 3}. HardA / HardB
    // are eligibility-filtered out of them entirely, so the only
    // rowers that can occupy them are PortOne and SoftSide.
    let port_rowers: Vec<RowerId> = [1, 3]
        .iter()
        .map(|&s| rower_in_seat(lineup, s))
        .collect();
    let starboard_rowers: Vec<RowerId> = [2, 4]
        .iter()
        .map(|&s| rower_in_seat(lineup, s))
        .collect();

    assert!(
        port_rowers.contains(&RowerId::new(4)),
        "PortOne (hard Port) must end up on a Port seat; got port={port_rowers:?}"
    );
    assert!(
        port_rowers.contains(&RowerId::new(3)),
        "SoftSide is the only other Port-eligible candidate; got port={port_rowers:?}"
    );
    // Hard-locked Starboard rowers cannot be on Port seats at all.
    assert!(!port_rowers.contains(&RowerId::new(1)));
    assert!(!port_rowers.contains(&RowerId::new(2)));
    // They must both be on the two Starboard seats.
    assert_eq!(
        starboard_rowers.iter().filter(|&&r| r == RowerId::new(1) || r == RowerId::new(2)).count(),
        2,
        "HardA and HardB must both be on Starboard seats; got {starboard_rowers:?}"
    );
}

#[test]
fn s6_cox_cooldown_picks_the_cold_cox() {
    // Two cox-capable rowers. Alice coxed 3 days ago — inside the
    // 14-day cooldown. Bob never coxed. All other rowers have
    // `can_cox = false` so the cox seat has exactly two candidates;
    // S6 must pick the cold one.
    //
    // We strip cox capability from Carla/Diego/Erin so there's no
    // tie-breaking between equally-valid non-Alice candidates —
    // without that, the solver can satisfy S6 by putting any of
    // them in seat 0 and the test assertion becomes ambiguous.
    let rowers = vec![
        rower(1, "Alice", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Either),
        rower(2, "Bob",   RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Either),
        {
            let mut r = rower(3, "Carla", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.can_cox = IntBool::FALSE;
            r
        },
        {
            let mut r = rower(4, "Diego", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        {
            let mut r = rower(5, "Erin", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.can_cox = IntBool::FALSE;
            r
        },
    ];
    let mut snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);
    // Alice coxed 3 days before the test date. Bob has no history.
    snap.last_coxed.insert(
        RowerId::new(1),
        test_date() - chrono::Duration::days(3),
    );

    let mut cfg = silent_config();
    cfg.cox_cooldown_penalty = 5;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    let cox = rower_in_seat(lineup, 0);
    assert_eq!(
        cox,
        RowerId::new(2),
        "Bob should be chosen as cox (Alice is inside the cooldown window); got {cox:?}"
    );
}

#[test]
fn s6_cox_cooldown_prefers_least_recently_coxed() {
    // Two cox-capable rowers, **both** inside the cooldown window
    // but at different days_since: Alice coxed 2 days ago, Bob
    // coxed 12 days ago. Under the old flat penalty both pay the
    // same `cox_cooldown_penalty` so the solver is indifferent.
    // Under the linear decay Alice's effective penalty is ~4
    // (ceil of 5 × 12/14) and Bob's is ~1 (ceil of 5 × 2/14), so
    // Bob is the cheaper cox and the solver must pick him.
    //
    // Every other rower has `can_cox = false` so the cox slot
    // has exactly these two candidates — keeps the assertion
    // unambiguous.
    let rowers = vec![
        rower(1, "Alice", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Either),
        rower(2, "Bob",   RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Either),
        {
            let mut r = rower(3, "Carla", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.can_cox = IntBool::FALSE;
            r
        },
        {
            let mut r = rower(4, "Diego", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        {
            let mut r = rower(5, "Erin", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.can_cox = IntBool::FALSE;
            r
        },
    ];
    let mut snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);
    // Alice coxed 2 days ago, Bob coxed 12 days ago. Both inside
    // the 14-day window, so both incur S6 penalty — but at
    // different magnitudes under linear decay.
    snap.last_coxed
        .insert(RowerId::new(1), test_date() - chrono::Duration::days(2));
    snap.last_coxed
        .insert(RowerId::new(2), test_date() - chrono::Duration::days(12));

    let mut cfg = silent_config();
    cfg.cox_cooldown_penalty = 5;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    let cox = rower_in_seat(lineup, 0);
    assert_eq!(
        cox,
        RowerId::new(2),
        "Bob (12 days out) should be preferred over Alice (2 days out) \
         under linear cooldown decay; got {cox:?}"
    );
}

#[test]
fn s9_pair_strength_matches_strengths_per_partition() {
    // Four rowers, two Port / two Starboard, two Strong / two
    // Weak. S9 should group them so each partition contains
    // rowers of the same strength.
    let rowers = vec![
        rower(1, "PortStrong",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardStrong", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortWeak",        RowerWeightClass::Medium, Skill::Expert, Strength::Weak,   Height::Medium, Side::Port),
        rower(4, "StarboardWeak",   RowerWeightClass::Medium, Skill::Expert, Strength::Weak,   Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut cfg = silent_config();
    cfg.pair_strength_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // For each partition, look up the two rowers' strengths and
    // assert they match.
    for partition in [(1, 2), (3, 4)] {
        let a = rower_in_seat(lineup, partition.0);
        let b = rower_in_seat(lineup, partition.1);
        // The rower IDs 1, 2 are Strong; 3, 4 are Weak. Both rowers
        // in a partition must belong to the same strength bucket.
        let a_strong = a == RowerId::new(1) || a == RowerId::new(2);
        let b_strong = b == RowerId::new(1) || b == RowerId::new(2);
        assert_eq!(
            a_strong, b_strong,
            "partition {partition:?} has mismatched strengths (a={a:?}, b={b:?})"
        );
    }
}

#[test]
fn s9b_bow_pair_gets_the_matched_strengths() {
    // Six rowers so we can field an 8+... no, an 8 needs 8 rowing
    // seats. Use a 4+ with a deliberate strength imbalance: three
    // Strong + one Weak. Only one partition can be perfectly
    // balanced (two Strongs); the other has to be Strong+Weak.
    //
    // With S9 regular on at weight 1 and S9b on at weight 2, the
    // bow partition (1, 2) should be the balanced one.
    let rowers = vec![
        rower(1, "PortStrongA",    RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardStrong", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortStrongB",    RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "StarboardWeak",  RowerWeightClass::Medium, Skill::Expert, Strength::Weak,   Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut cfg = silent_config();
    cfg.pair_strength_weight = 1;
    cfg.bow_pair_strength_weight = 2;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // The bow partition (1, 2) should contain the Strong/Strong
    // pair. The Weak rower (#4) must land in partition (3, 4).
    let weak_partition = partition_of(lineup, RowerId::new(4)).unwrap();
    assert_eq!(
        weak_partition,
        (3, 4),
        "StarboardWeak should be in the stern partition (3, 4), not the bow; got {weak_partition:?}"
    );
}

#[test]
fn s10_pair_height_matches_heights_per_partition() {
    let rowers = vec![
        rower(1, "PortShort",     RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Short,    Side::Port),
        rower(2, "StarboardShort", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Short,    Side::Starboard),
        rower(3, "PortTall",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::VeryTall, Side::Port),
        rower(4, "StarboardTall", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::VeryTall, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut cfg = silent_config();
    cfg.height_balance_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    for partition in [(1, 2), (3, 4)] {
        let a = rower_in_seat(lineup, partition.0);
        let b = rower_in_seat(lineup, partition.1);
        let a_short = a == RowerId::new(1) || a == RowerId::new(2);
        let b_short = b == RowerId::new(1) || b == RowerId::new(2);
        assert_eq!(
            a_short, b_short,
            "partition {partition:?} has mismatched heights (a={a:?}, b={b:?})"
        );
    }
}

#[test]
fn s11_end_pair_skill_puts_experts_in_the_ends() {
    // 8-boat with 4 Expert rowers and 4 Novice rowers. With S11
    // on and S12 off (so strength doesn't compete for placement),
    // the Experts should end up in seats {1, 2, 7, 8} and the
    // Novices in seats {3, 4, 5, 6}.
    //
    // S11 uses a skill gradient: full weight on end pairs, tapering
    // into the engine room. At weight=4 the gradient is:
    //   dist 0 (seats 1,2,7,8) → 4
    //   dist 1 (seats 3,6)     → 3
    //   dist 2 (seats 4,5)     → 2
    // This is enough to pull Experts to the ends.
    //
    // Side layout on a Starboard-stroke 8+:
    //   Port seats     = {1, 3, 5, 7}  (4 Port rowers needed)
    //   Starboard seats= {2, 4, 6, 8}  (4 Starboard rowers needed)
    //
    // We give the 4 Experts a 2 Port / 2 Starboard split, same
    // for the 4 Novices, so the side constraints are satisfiable
    // with the expected placement.
    let rowers = vec![
        rower(1, "ExpertPortA",     RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "ExpertStarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "ExpertPortB",     RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "ExpertStarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(5, "NovicePortA",     RowerWeightClass::Medium, Skill::Novice, Strength::Strong, Height::Medium, Side::Port),
        rower(6, "NoviceStarboardA", RowerWeightClass::Medium, Skill::Novice, Strength::Strong, Height::Medium, Side::Starboard),
        rower(7, "NovicePortB",     RowerWeightClass::Medium, Skill::Novice, Strength::Strong, Height::Medium, Side::Port),
        rower(8, "NoviceStarboardB", RowerWeightClass::Medium, Skill::Novice, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(9, "Cox"),
    ];
    let snap = snapshot(rowers, vec![eight_boat(1, "TestEight")]);

    let mut cfg = silent_config();
    cfg.end_pair_skill_weight = 4;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    let experts: [RowerId; 4] = [RowerId::new(1), RowerId::new(2), RowerId::new(3), RowerId::new(4)];
    for seat in [1, 2, 7, 8] {
        let r = rower_in_seat(lineup, seat);
        assert!(
            experts.contains(&r),
            "seat {seat} should hold an Expert, got {r:?}"
        );
    }
    for seat in [3, 4, 5, 6] {
        let r = rower_in_seat(lineup, seat);
        assert!(
            !experts.contains(&r),
            "seat {seat} should hold a Novice (engine room), got Expert {r:?}"
        );
    }
}

#[test]
fn s12_engine_room_strength_puts_strong_rowers_in_middle() {
    // Symmetric to s11: 4 VeryStrong + 4 Weak rowers on an 8+,
    // S11 off, S12 on. Expect VeryStrong in seats {3, 4, 5, 6}.
    let rowers = vec![
        rower(1, "VSPortA",      RowerWeightClass::Medium, Skill::Master, Strength::VeryStrong, Height::Medium, Side::Port),
        rower(2, "VSStarboardA", RowerWeightClass::Medium, Skill::Master, Strength::VeryStrong, Height::Medium, Side::Starboard),
        rower(3, "VSPortB",      RowerWeightClass::Medium, Skill::Master, Strength::VeryStrong, Height::Medium, Side::Port),
        rower(4, "VSStarboardB", RowerWeightClass::Medium, Skill::Master, Strength::VeryStrong, Height::Medium, Side::Starboard),
        rower(5, "WeakPortA",    RowerWeightClass::Medium, Skill::Master, Strength::Weak,       Height::Medium, Side::Port),
        rower(6, "WeakStarboardA", RowerWeightClass::Medium, Skill::Master, Strength::Weak,       Height::Medium, Side::Starboard),
        rower(7, "WeakPortB",    RowerWeightClass::Medium, Skill::Master, Strength::Weak,       Height::Medium, Side::Port),
        rower(8, "WeakStarboardB", RowerWeightClass::Medium, Skill::Master, Strength::Weak,       Height::Medium, Side::Starboard),
        cox_rower(9, "Cox"),
    ];
    let snap = snapshot(rowers, vec![eight_boat(1, "TestEight")]);

    let mut cfg = silent_config();
    cfg.engine_room_strength_weight = 1;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    let very_strong: [RowerId; 4] = [
        RowerId::new(1),
        RowerId::new(2),
        RowerId::new(3),
        RowerId::new(4),
    ];
    for seat in [3, 4, 5, 6] {
        let r = rower_in_seat(lineup, seat);
        assert!(
            very_strong.contains(&r),
            "engine room seat {seat} should hold a VeryStrong rower, got {r:?}"
        );
    }
    for seat in [1, 2, 7, 8] {
        let r = rower_in_seat(lineup, seat);
        assert!(
            !very_strong.contains(&r),
            "end-pair seat {seat} should hold a Weak rower, got VeryStrong {r:?}"
        );
    }
}

// ---------- S13 non-scull retention ----------

#[test]
fn s13_non_scull_retention_prefers_benching_sculler() {
    // A 4+ (4 rowing + cox = 5 seats) with 6 available rowers:
    // 4 Port/Starboard rowers needed for rowing seats, 1
    // designated cox, and 1 extra rower who leans toward
    // sculling. The solver has to bench someone. Without the
    // retention bonus, any non-cox rower could be chosen; with
    // it, the sculling-leaning one should be the bench pick.
    //
    // Layout: rowers 1-4 cover the 4 rowing seats (2 Port + 2
    // Starboard). Rower 5 is a third Port rower with
    // `sweep_bias = -1` — the overflow candidate. Rower 6 is
    // the designated cox. Since the 4+ needs exactly 2 Port
    // and 2 Starboard, one Port rower must be benched. The
    // solver should pick rower 5 (sculling-leaning) over
    // rowers 1 or 3 (sweep_bias = 2, strong sweep preference).
    let rowers = vec![
        rower(1, "PortA",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortB",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        {
            // Sculling-leaning Port rower — the solver should
            // prefer to bench *this* one because they have a
            // fallback.
            let mut r = rower(5, "Scullable", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port);
            r.sweep_bias = SweepBias::new(-1);
            r
        },
        cox_rower(6, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut cfg = silent_config();
    cfg.non_scull_retention_weight = 2;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // Both sweep-biased Port rowers (PortA, PortB) should be
    // seated; the sculling-leaning rower should be in the unplaced
    // `to_sculling` bucket.
    let placed_ids: std::collections::HashSet<RowerId> = lineup
        .seats
        .iter()
        .map(|(_, r)| *r)
        .collect();
    assert!(
        placed_ids.contains(&RowerId::new(1)),
        "PortA (sweep-biased) should be seated, got lineup {:?}",
        lineup.seats
    );
    assert!(
        placed_ids.contains(&RowerId::new(3)),
        "PortB (sweep-biased) should be seated, got lineup {:?}",
        lineup.seats
    );
    assert!(
        !placed_ids.contains(&RowerId::new(5)),
        "Scullable (sculling-leaning) should be benched, but got lineup {:?}",
        lineup.seats
    );

    // And the unplaced breakdown should confirm the classification.
    assert_eq!(
        result.primary.unplaced.to_sculling,
        vec![RowerId::new(5)],
        "the sculling-leaning rower should land in to_sculling",
    );
    assert!(
        result.primary.unplaced.benched.is_empty(),
        "no sweep-biased rower should be benched, got {:?}",
        result.primary.unplaced.benched
    );
}

// ---------- Partial-fill bonus ----------

#[test]
fn partial_fill_bonus_prefers_filling_optional_seats() {
    // An 8+ under `Allowed(2)` could legitimately leave seats 3
    // and 4 empty while still fielding the boat — the cap
    // permits it and S8's per-boat reward doesn't distinguish
    // full-fill from partial-fill. With only the partial-fill
    // bonus on, the solver should choose to *fill* both
    // optional seats rather than leave them empty.
    //
    // Fixture: one 8+, 8 rowers (4 Port + 4 Starboard) that
    // together exactly fill all 8 rowing seats, plus a
    // designated cox for seat 0. Under `Allowed(2)` every
    // arrangement — full fill or partial fill — is feasible
    // for the hard constraints; only the partial-fill bonus
    // tells the solver which one to pick.
    let rowers = vec![
        rower(1, "PortA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(5, "PortC", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(6, "StarboardC", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(7, "PortD", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(8, "StarboardD", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(9, "Cox"),
    ];
    let snap = snapshot(rowers, vec![eight_boat(1, "TestEight")]);

    let mut cfg = silent_config();
    cfg.partial_fill_bonus = 1;

    let mut req = request(cfg);
    req.partial_fill = PartialFillPolicy::Allowed(2);

    let result = solve(&snap, &req).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // Assert seats 3 and 4 — the optional pair — are both
    // occupied. Without the bonus the solver is free to leave
    // them empty.
    let seat_3 = lineup.seats.iter().find(|(s, _)| *s == 3);
    let seat_4 = lineup.seats.iter().find(|(s, _)| *s == 4);
    assert!(
        seat_3.is_some(),
        "seat 3 should be filled under the partial-fill bonus; \
         got seats {:?}",
        lineup.seats
    );
    assert!(
        seat_4.is_some(),
        "seat 4 should be filled under the partial-fill bonus; \
         got seats {:?}",
        lineup.seats
    );

    // And the full rowing crew should be present (8 rowing +
    // 1 cox = 9 entries).
    assert_eq!(
        lineup.seats.len(),
        9,
        "expected a full 8+ crew (8 rowing + cox); got {} seats",
        lineup.seats.len()
    );
}

#[test]
fn partial_fill_bonus_is_inert_under_strict() {
    // Under `Strict` partial-fill, the H1 equality forces every
    // seat to be filled and the bonus has nothing to push pressure
    // against. The solver should produce a valid full-crew lineup
    // without the bonus affecting any decision, and — crucially —
    // solve() must not panic on the (inert) bonus block.
    let rowers = vec![
        rower(1, "PortA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    // Enable the bonus explicitly and keep partial_fill = Strict
    // (the default in `request`).
    let mut cfg = silent_config();
    cfg.partial_fill_bonus = 5;

    let result = solve(&snap, &request(cfg)).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);
    assert_eq!(
        lineup.seats.len(),
        5,
        "expected a full 4+ crew (4 rowing + cox); got {} seats",
        lineup.seats.len()
    );
}

// ---------- Top-N tabu re-solve tests ----------

/// Collect every `(boat_id, seat, rower_id)` placement from a
/// lineup set so two alternatives can be compared by set-difference.
fn placement_set(lineups: &[ProposedLineup]) -> std::collections::HashSet<(i32, i32, i32)> {
    let mut out = std::collections::HashSet::new();
    for l in lineups.iter().filter(|l| l.used) {
        for (seat, rower_id) in &l.seats {
            out.insert((l.boat_id.as_int(), *seat, rower_id.as_int()));
        }
    }
    out
}

/// How many placements differ between two lineup sets. Since both
/// sets contain the same number of filled seats under H1 (when
/// both field the same boats), the symmetric-difference size is
/// 2 × the number of "flipped" placements. We return the raw
/// symmetric-difference count so callers can compare against
/// `2 * tabu_min_diff`.
fn placements_flipped(a: &[ProposedLineup], b: &[ProposedLineup]) -> usize {
    let sa = placement_set(a);
    let sb = placement_set(b);
    sa.symmetric_difference(&sb).count()
}

#[test]
fn topn_returns_requested_number_of_alternatives() {
    // 4+ with 4 rowers (2 port, 2 starboard) + a designated cox.
    // With every soft weight off, the solver is free to swap
    // rowers between same-side seats at no cost, so many distinct
    // placements are equally optimal. Ask for 3 alternatives and
    // confirm we get primary + 2 more.
    let rowers = vec![
        rower(1, "PortA",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "PortB",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(3, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut req = request(silent_config());
    req.top_n = 3;
    req.tabu_min_diff = 2;

    let result = solve(&snap, &req).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    assert_eq!(
        result.alternatives.len(),
        2,
        "primary + 2 alternatives = 3 lineups total; got {} extras",
        result.alternatives.len()
    );
}

#[test]
fn topn_alternatives_respect_tabu_min_diff() {
    // Same 4+ / 4 rowers setup, but ask for 3 alternatives with
    // tabu_min_diff = 2. Each consecutive pair of alternatives
    // must differ by at least 2 placements × 2 (symmetric
    // difference counts both "removed" and "added").
    let rowers = vec![
        rower(1, "PortA",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "PortB",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(3, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut req = request(silent_config());
    req.top_n = 3;
    req.tabu_min_diff = 2;

    let result = solve(&snap, &req).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);

    // Build an iterator that walks [primary, alt1, alt2, ...] as
    // &Vec<ProposedLineup> slices so pairwise comparisons are
    // straightforward. Alternatives are `ProposedSolution`s now,
    // so we project out their `lineups` field to match the
    // primary's shape.
    let mut all: Vec<&Vec<ProposedLineup>> = vec![&result.primary.lineups];
    all.extend(result.alternatives.iter().map(|alt| &alt.lineups));

    // Every unordered pair of lineups must differ by at least
    // `tabu_min_diff` placements in each direction (so the
    // symmetric-difference size is at least 2 * tabu_min_diff).
    // The tabu constraint only forces successive pairs apart, but
    // the accumulated constraints ensure every pair is distinct.
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate().skip(i + 1) {
            let diff = placements_flipped(a, b);
            assert!(
                diff >= 2 * req.tabu_min_diff as usize,
                "alternatives {i} and {j} differ by only {diff} placements, \
                 expected ≥ {} (2 * tabu_min_diff={})",
                2 * req.tabu_min_diff,
                req.tabu_min_diff
            );
        }
    }
}

#[test]
fn topn_one_is_identical_to_single_solve() {
    // Regression guard: `top_n == 1` must produce exactly the
    // same SolveResult shape as the pre-Top-N code path, which
    // means an empty alternatives vec. This protects callers
    // who read `result.lineups` and never look at alternatives.
    let rowers = vec![
        rower(1, "Alice", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "Bob",   RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "Carla", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "Diego", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let result = solve(&snap, &request(silent_config())).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    assert!(
        result.alternatives.is_empty(),
        "top_n=1 must produce no alternatives, got {} extras",
        result.alternatives.len()
    );
    // And the primary is still populated as before.
    assert!(!result.primary.lineups.is_empty());
}

#[test]
fn topn_gracefully_caps_at_feasible_region() {
    // A pathological case: tabu_min_diff is set so high that
    // after the primary solve, no further feasible solutions
    // exist. The solver should return the primary plus an empty
    // alternatives vec, rather than erroring or panicking.
    //
    // With 4 rowers on a 4+ and tabu_min_diff = 10, we're asking
    // for "differ by at least 10 placements" but there are only
    // 5 filled seats total — the tabu constraint is trivially
    // infeasible, so no second alternative can be found.
    let rowers = vec![
        rower(1, "PortA",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "PortB",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(3, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "Cox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let mut req = request(silent_config());
    req.top_n = 5;
    req.tabu_min_diff = 10;

    let result = solve(&snap, &req).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    assert!(!result.primary.lineups.is_empty(), "primary should still solve");
    assert!(
        result.alternatives.is_empty(),
        "no alternative should satisfy tabu_min_diff = 10 on a 5-seat boat"
    );
}

#[test]
fn h2_designated_cox_never_rows() {
    // Even with an expert-level cox-only rower in an otherwise
    // underpopulated fleet, the solver must leave them in seat 0
    // (or on the dock) rather than seating them in a rowing seat.
    // This is an eligibility-layer rule (not an objective-layer one)
    // so the test doesn't need any particular soft-weight setup.
    let rowers = vec![
        rower(1, "PortA",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(2, "StarboardA", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        rower(3, "PortB",      RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Port),
        rower(4, "StarboardB", RowerWeightClass::Medium, Skill::Expert, Strength::Strong, Height::Medium, Side::Starboard),
        cox_rower(5, "DesignatedCox"),
    ];
    let snap = snapshot(rowers, vec![four_boat(1, "TestBoat")]);

    let result = solve(&snap, &request(SolverConfig::default())).unwrap();
    assert_eq!(result.status, SolveStatus::Satisfied);
    let lineup = single_used(&result.primary.lineups);

    // Cox is in seat 0.
    assert_eq!(
        rower_in_seat(lineup, 0),
        RowerId::new(5),
        "designated cox should occupy seat 0"
    );
    // And only seat 0 — not any rowing seat.
    for seat in 1..=4 {
        let r = rower_in_seat(lineup, seat);
        assert_ne!(
            r,
            RowerId::new(5),
            "designated cox must not row seat {seat}"
        );
    }
}
