//! Toy fixture data so `cargo run -p lineup_cli` has something to show on an
//! empty database. This is intentionally small — 14 rowers, 3 sweep boats,
//! one practice date, hand-authored availabilities.

use crate::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use crate::boat::types::WeightClass as BoatWeightClass;
use crate::boat::{Boat, NewBoat};
use crate::pair_affinity::{NewPairAffinity, PairAffinity};
use crate::practice::Practice;
use crate::rower::types::{RowerWeightClass, Side, Skill, Strength};
use crate::rower::{NewRower, Rower};
use crate::seat_affinity::{NewSeatAffinity, SeatAffinity};
use crate::types::{AffinityWeight, IntBool};
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;

/// Seed the db only if there are zero rowers. Safe to call on every startup.
#[tracing::instrument(level = "info", skip(conn), err)]
pub fn seed_if_empty(conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    if Rower::count(conn)? > 0 {
        tracing::info!("Fixture already seeded, skipping");
        return Ok(());
    }
    conn.transaction(|conn| seed_all(conn))
}

fn seed_all(conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    tracing::info!("Seeding toy fixture");

    // --- boats (sweep only; scullers handle their own) ---
    for b in toy_boats() {
        Boat::insert(conn, b)?;
    }

    // --- rowers ---
    let mut rower_ids = Vec::new();
    for r in toy_rowers() {
        let inserted = Rower::insert(conn, r)?;
        rower_ids.push(inserted.id);
    }

    // --- practice + availabilities for an upcoming date ---
    let date = NaiveDate::from_ymd_opt(2026, 4, 11).expect("valid date");
    Practice::upsert_by_date(conn, date, Some("Toy seeded practice".to_string()))?;

    // Rough mix: most Yes, a couple No, one Maybe, one ScullingOnly.
    let statuses = [
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::Yes,
        AvailabilityStatus::No,
        AvailabilityStatus::Maybe,
        AvailabilityStatus::Yes,
        AvailabilityStatus::ScullingOnly,
        AvailabilityStatus::No,
    ];
    for (rower_id, status) in rower_ids.iter().copied().zip(statuses) {
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id,
                date,
                status,
            },
        )?;
    }

    // Seat affinities. Indices correspond to `toy_rowers()`:
    //   0 Alice, 1 Bob, 2 Carla, 3 Diego, 4 Erin, 5 Finn, 6 Grace,
    //   7 Hana, 8 Ivan, 9 Juno, 10 Kai, 11 Lena, 12 Mika, 13 Nico
    //
    // - Alice (Port, Expert) likes seat 4: she loves being at stroke of a
    //   4-boat. Weight +3.
    // - Hana (Port, Master) likes seat 2. Weight +3.
    // - Ivan (Starboard, Novice) avoids seat 1 (the exposed bow seat).
    //   Weight -2.
    for (idx, seat, weight) in [(0usize, 4, 3), (7, 2, 3), (8, 1, -2)] {
        SeatAffinity::insert(
            conn,
            NewSeatAffinity {
                rower_id: rower_ids[idx],
                seat_position: seat,
                weight: AffinityWeight::new(weight),
            },
        )?;
    }

    // Pair affinities. Two examples:
    //   - Alice (0, Port/Medium/Expert) + Erin (4, Either/Medium/Expert):
    //     +4. These two train together as a pair. In the current
    //     optimum they're in the same boat but in different 2-seat
    //     partitions, so this one should demonstrably reshuffle.
    //   - Carla (2, Port/Light/Intermediate) + Diego (3, Starboard/
    //     Medium/Master): +2. Currently already in the same partition
    //     (Artemis seats 1-2); the affinity just makes that choice
    //     locked rather than accidental.
    for (idx_a, idx_b, weight) in [(0usize, 4usize, 4), (2, 3, 2)] {
        PairAffinity::insert(
            conn,
            NewPairAffinity::canonical(
                rower_ids[idx_a],
                rower_ids[idx_b],
                AffinityWeight::new(weight),
            ),
        )?;
    }
    Ok(())
}

fn toy_boats() -> Vec<NewBoat> {
    // Two rigs to exercise the side-constraint logic: Persephone + Artemis
    // are starboard-rigged (stroke on starboard), Hestia is port-rigged.
    vec![
        NewBoat {
            name: "Persephone".into(),
            weight_class: BoatWeightClass::Heavy,
            seat_count: 8,
            has_cox: IntBool::TRUE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Starboard,
        },
        NewBoat {
            name: "Artemis".into(),
            weight_class: BoatWeightClass::Medium,
            seat_count: 4,
            has_cox: IntBool::TRUE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Starboard,
        },
        NewBoat {
            name: "Hestia".into(),
            weight_class: BoatWeightClass::Light,
            seat_count: 4,
            has_cox: IntBool::FALSE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Port,
        },
    ]
}

fn toy_rowers() -> Vec<NewRower> {
    // `Intermediate` exists on both Skill and Strength, so we avoid glob
    // imports and reference each enum explicitly.
    use RowerWeightClass::{Heavy, Light, Medium};
    use Side::{Either, Port, Starboard};
    use Skill as Sk;
    use Strength as St;
    vec![
        NewRower::sweep("Alice", Medium, Sk::Expert, St::Strong, Port),
        NewRower::sweep("Bob", Heavy, Sk::Master, St::VeryStrong, Starboard),
        NewRower::sweep("Carla", Light, Sk::Intermediate, St::Intermediate, Port),
        NewRower::sweep("Diego", Medium, Sk::Master, St::Strong, Starboard),
        NewRower::sweep("Erin", Medium, Sk::Expert, St::Strong, Either),
        NewRower::sweep("Finn", Heavy, Sk::Intermediate, St::Strong, Port),
        NewRower::sweep("Grace", Light, Sk::Expert, St::Intermediate, Starboard),
        NewRower::sweep("Hana", Medium, Sk::Master, St::VeryStrong, Port),
        NewRower::sweep("Ivan", Heavy, Sk::Novice, St::Weak, Starboard),
        NewRower::sweep("Juno", Medium, Sk::Intermediate, St::Intermediate, Either),
        NewRower::sweep("Kai", Light, Sk::Master, St::Strong, Port),
        {
            // Lena is the designated cox.
            let mut r = NewRower::sweep("Lena", Light, Sk::Expert, St::Weak, Either);
            r.can_cox = IntBool::TRUE;
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        NewRower::sweep("Mika", Medium, Sk::Master, St::Strong, Starboard),
        {
            // Nico can be pushed to the scullers as overflow.
            let mut r = NewRower::sweep("Nico", Medium, Sk::Intermediate, St::Strong, Either);
            r.can_scull = IntBool::TRUE;
            r
        },
    ]
}
