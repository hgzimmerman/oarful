//! Dev fixture: 14 rowers, 3 boats, 1 practice — used by baseline tests.

use crate::app_user::{AppUser, NewAppUser, Role};
use crate::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use crate::boat::types::{CoxPosition, WeightClass as BoatWeightClass};
use crate::boat::{Boat, NewBoat};
use crate::pair_affinity::{NewPairAffinity, PairAffinity};
use crate::practice::Practice;
use crate::rower::types::{Height, RowerWeightClass, Side, Skill, Strength, SweepBias};
use crate::rower::{NewRower, Rower};
use crate::seat_affinity::{NewSeatAffinity, SeatAffinity, SeatZone};
use crate::team::{NewTeam, Team, TeamMembership};
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

/// Seed only the fleet (boats) + team + coach account, no rowers.
#[tracing::instrument(level = "info", skip(conn), err)]
pub fn seed_fleet_only(conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    use crate::schema::boat;
    let boat_count: i64 = boat::table.count().get_result(conn)?;
    if boat_count > 0 {
        tracing::info!("Fleet already seeded, skipping");
        return Ok(());
    }
    conn.transaction(|conn| {
        tracing::info!("Seeding fleet-only fixture");
        let now = chrono::Utc::now().naive_utc();
        let _team = match Team::first(conn)? {
            Some(t) => t,
            None => Team::create(
                conn,
                NewTeam {
                    name: "Sweep".to_string(),
                    created_at: now,
                },
            )?,
        };
        for b in toy_boats() {
            Boat::insert(conn, b)?;
        }
        let hash = "$2b$04$GM6z8WroCGjpPpOAzMpVwu3WOrWykUBFY40rmEXs.JJemMkBRsUXK";
        let user = AppUser::create(
            conn,
            NewAppUser {
                email: "coach@test.com".to_string(),
                password_hash: Some(hash.to_string()),
                name: "Dev Coach".to_string(),
                status: "active".to_string(),
                created_at: now,
                updated_at: now,
            },
        )?;
        AppUser::set_role(conn, user.id, Role::ProgramDirector)?;
        Ok(())
    })
}

fn seed_all(conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    tracing::info!("Seeding toy fixture");

    let now = chrono::Utc::now().naive_utc();
    let team = match Team::first(conn)? {
        Some(t) => t,
        None => Team::create(
            conn,
            NewTeam {
                name: "Sweep".to_string(),
                created_at: now,
            },
        )?,
    };

    for b in toy_boats() {
        Boat::insert(conn, b)?;
    }

    let mut rower_ids = Vec::new();
    for r in toy_rowers() {
        let inserted = Rower::insert(conn, r)?;
        TeamMembership::add(conn, team.id, inserted.id)?;
        rower_ids.push(inserted.id);
    }

    let date = NaiveDate::from_ymd_opt(2026, 4, 11).expect("valid date");
    let practice = Practice::upsert(
        conn,
        team.id,
        date,
        None,
        Some("Toy seeded practice".to_string()),
    )?;

    let statuses = [
        AvailabilityStatus::Yes, // Alice
        AvailabilityStatus::Yes, // Bob
        AvailabilityStatus::Yes, // Carla
        AvailabilityStatus::Yes, // Diego
        AvailabilityStatus::Yes, // Erin
        AvailabilityStatus::Yes, // Finn
        AvailabilityStatus::Yes, // Grace
        AvailabilityStatus::Yes, // Hana
        AvailabilityStatus::Yes, // Ivan
        AvailabilityStatus::No,  // Juno
        AvailabilityStatus::No,  // Kai
        AvailabilityStatus::Yes, // Lena (designated cox)
        AvailabilityStatus::Yes, // Mika (non-designated cox)
        AvailabilityStatus::Yes, // Nico (sweep_bias handles scull distinction)
    ];
    for (rower_id, status) in rower_ids.iter().copied().zip(statuses) {
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id,
                practice_id: practice.id,
                status,
            },
        )?;
    }

    for (idx, zone, weight) in [
        (0usize, SeatZone::Stroke, 3),
        (7, SeatZone::BowPair, 3),
        (8, SeatZone::Bow, -2),
    ] {
        SeatAffinity::insert(
            conn,
            NewSeatAffinity {
                rower_id: rower_ids[idx],
                zone,
                weight: AffinityWeight::new(weight),
            },
        )?;
    }

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

    let hash = "$2b$04$GM6z8WroCGjpPpOAzMpVwu3WOrWykUBFY40rmEXs.JJemMkBRsUXK";
    let user = AppUser::create(
        conn,
        NewAppUser {
            email: "coach@test.com".to_string(),
            password_hash: Some(hash.to_string()),
            name: "Dev Coach".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        },
    )?;
    AppUser::set_role(conn, user.id, Role::ProgramDirector)?;

    Ok(())
}

fn toy_boats() -> Vec<NewBoat> {
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
            cox_position: CoxPosition::Stern,
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
            cox_position: CoxPosition::Bow,
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
            cox_position: CoxPosition::Stern,
        },
    ]
}

fn toy_rowers() -> Vec<NewRower> {
    use Height as H;
    use RowerWeightClass::{Heavy, Light, Medium};
    use Side::{Either, Port, Starboard};
    use Skill as Sk;
    use Strength as St;
    vec![
        NewRower::sweep("Alice", Medium, Sk::Expert, St::Strong, H::Tall, Port),
        NewRower::sweep(
            "Bob",
            Heavy,
            Sk::Master,
            St::VeryStrong,
            H::VeryTall,
            Starboard,
        ),
        NewRower::sweep(
            "Carla",
            Light,
            Sk::Intermediate,
            St::Intermediate,
            H::Short,
            Port,
        ),
        NewRower::sweep("Diego", Medium, Sk::Master, St::Strong, H::Tall, Starboard),
        NewRower::sweep("Erin", Medium, Sk::Expert, St::Strong, H::Medium, Either),
        NewRower::sweep(
            "Finn",
            Heavy,
            Sk::Intermediate,
            St::Strong,
            H::VeryTall,
            Port,
        ),
        NewRower::sweep(
            "Grace",
            Light,
            Sk::Expert,
            St::Intermediate,
            H::Short,
            Starboard,
        ),
        NewRower::sweep("Hana", Medium, Sk::Master, St::VeryStrong, H::Tall, Port),
        {
            let mut r = NewRower::sweep("Ivan", Heavy, Sk::Novice, St::Weak, H::Tall, Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        NewRower::sweep(
            "Juno",
            Medium,
            Sk::Intermediate,
            St::Intermediate,
            H::Medium,
            Either,
        ),
        NewRower::sweep("Kai", Light, Sk::Master, St::Strong, H::Medium, Port),
        {
            let mut r = NewRower::sweep("Lena", Light, Sk::Expert, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        NewRower::sweep("Mika", Medium, Sk::Master, St::Strong, H::Medium, Starboard),
        {
            let mut r = NewRower::sweep(
                "Nico",
                Medium,
                Sk::Intermediate,
                St::Strong,
                H::Tall,
                Either,
            );
            r.sweep_bias = SweepBias::SCULL_HARD;
            r
        },
    ]
}
