//! Demo fixture: 24 rowers, 6 boats, 3 practices with committed lineups.

use crate::app_user::{AppUser, NewAppUser, Role};
use crate::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use crate::boat::types::{CoxPosition, WeightClass as BoatWeightClass};
use crate::boat::{Boat, NewBoat};
use crate::lineup::{CommitSeat, Lineup};
use crate::pair_affinity::{NewPairAffinity, PairAffinity};
use crate::practice::Practice;
use crate::rower::types::{Height, RowerWeightClass, Side, Skill, Strength, SweepBias};
use crate::rower::{NewRower, Rower};
use crate::seat_affinity::{NewSeatAffinity, SeatAffinity, SeatZone};
use crate::team::{NewTeam, Team, TeamMembership};
use crate::types::{AffinityWeight, IntBool};
use chrono::{Datelike, NaiveDate, Weekday};
use diesel::prelude::*;
use diesel::SqliteConnection;

fn next_weekday(from: NaiveDate, day: Weekday) -> NaiveDate {
    let days_ahead =
        (day.num_days_from_monday() as i64 - from.weekday().num_days_from_monday() as i64 + 7) % 7;
    let days_ahead = if days_ahead == 0 { 7 } else { days_ahead };
    from + chrono::TimeDelta::try_days(days_ahead).unwrap()
}

/// Result of seeding a demo tenant.
pub struct DemoSeed {
    pub user_id: crate::app_user::UserId,
    pub team_id: crate::team::TeamId,
}

/// Seed a demo tenant with a rich fixture.
#[tracing::instrument(level = "info", skip(conn), err)]
pub fn seed_demo(conn: &mut SqliteConnection) -> Result<DemoSeed, diesel::result::Error> {
    conn.transaction(|conn| seed_demo_inner(conn))
}

fn seed_demo_inner(conn: &mut SqliteConnection) -> Result<DemoSeed, diesel::result::Error> {
    tracing::info!("Seeding demo fixture");
    let now = chrono::Utc::now().naive_utc();
    let today = chrono::Utc::now().date_naive();

    let team = Team::create(
        conn,
        NewTeam {
            name: "Demo Rowing Club".to_string(),
            created_at: now,
        },
    )?;

    let boat_specs = demo_boats();
    let mut boat_ids = Vec::new();
    for b in boat_specs {
        let inserted = Boat::insert(conn, b)?;
        boat_ids.push(inserted.id);
    }

    let rower_specs = demo_rowers();
    let rower_names: Vec<String> = rower_specs.iter().map(|r| r.name.clone()).collect();
    let mut rower_ids = Vec::new();
    for r in rower_specs {
        let inserted = Rower::insert(conn, r)?;
        TeamMembership::add(conn, team.id, inserted.id)?;
        rower_ids.push(inserted.id);
    }

    // Seat affinities
    for (idx, zone, weight) in [
        (0usize, SeatZone::Stroke, 3),
        (1, SeatZone::EngineRoom, 2),
        (7, SeatZone::SternHalf, 3),
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

    // Pair affinities
    for (a, b, w) in [(0usize, 4usize, 4), (3, 10, 3), (12, 16, 2)] {
        PairAffinity::insert(
            conn,
            NewPairAffinity::canonical(rower_ids[a], rower_ids[b], AffinityWeight::new(w)),
        )?;
    }

    // 3 practices: next Mon, Wed, Fri
    let mon = next_weekday(today, Weekday::Mon);
    let wed = next_weekday(today, Weekday::Wed);
    let fri = next_weekday(today, Weekday::Fri);

    let p_mon = Practice::upsert(
        conn,
        team.id,
        mon,
        None,
        Some("Steady state pieces".to_string()),
    )?;
    let p_wed = Practice::upsert(
        conn,
        team.id,
        wed,
        None,
        Some("Technique drills".to_string()),
    )?;
    let p_fri = Practice::upsert(conn, team.id, fri, None, None)?;

    // Availability
    let mon_no = [19usize, 21, 23];
    for (i, rid) in rower_ids.iter().enumerate() {
        let status = if mon_no.contains(&i) {
            AvailabilityStatus::No
        } else {
            AvailabilityStatus::Yes
        };
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id: *rid,
                practice_id: p_mon.id,
                status,
            },
        )?;
    }

    for rid in &rower_ids {
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id: *rid,
                practice_id: p_wed.id,
                status: AvailabilityStatus::Yes,
            },
        )?;
    }

    for (i, rid) in rower_ids.iter().enumerate() {
        let status = match i {
            5 | 8 => AvailabilityStatus::No,
            _ => AvailabilityStatus::Yes,
        };
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id: *rid,
                practice_id: p_fri.id,
                status,
            },
        )?;
    }

    // Committed lineup for Monday
    let athena_id = boat_ids[1];
    let athena_seats: Vec<CommitSeat> = vec![
        CommitSeat {
            seat_position: 0,
            rower_id: rower_ids[17],
            is_cox: true,
        },
        CommitSeat {
            seat_position: 1,
            rower_id: rower_ids[6],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 2,
            rower_id: rower_ids[2],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 3,
            rower_id: rower_ids[3],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 4,
            rower_id: rower_ids[8],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 5,
            rower_id: rower_ids[1],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 6,
            rower_id: rower_ids[7],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 7,
            rower_id: rower_ids[4],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 8,
            rower_id: rower_ids[0],
            is_cox: false,
        },
    ];
    Lineup::commit_for_boat(conn, p_mon.id, athena_id, &athena_seats)?;

    let artemis_id = boat_ids[3];
    let artemis_seats: Vec<CommitSeat> = vec![
        CommitSeat {
            seat_position: 0,
            rower_id: rower_ids[25],
            is_cox: true,
        },
        CommitSeat {
            seat_position: 1,
            rower_id: rower_ids[14],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 2,
            rower_id: rower_ids[20],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 3,
            rower_id: rower_ids[22],
            is_cox: false,
        },
        CommitSeat {
            seat_position: 4,
            rower_id: rower_ids[13],
            is_cox: false,
        },
    ];
    Lineup::commit_for_boat(conn, p_mon.id, artemis_id, &artemis_seats)?;

    // App users for rowers (mirrors sheet-sync behaviour)
    let rower_emails: &[(usize, &str)] =
        &[(0, "alice@test.example.com"), (1, "bob@test.example.com")];
    for &(idx, email) in rower_emails {
        let u = AppUser::create(
            conn,
            NewAppUser {
                email: email.to_string(),
                password_hash: None,
                name: rower_names[idx].clone(),
                status: "active".to_string(),
                created_at: now,
                updated_at: now,
            },
        )?;
        AppUser::set_role(conn, u.id, Role::Member)?;
        AppUser::set_rower_id(conn, u.id, Some(rower_ids[idx]))?;
    }

    // Demo user (ProgramDirector, no password)
    let user = AppUser::create(
        conn,
        NewAppUser {
            email: "demo@localhost".to_string(),
            password_hash: None,
            name: "Demo Coach".to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        },
    )?;
    AppUser::set_role(conn, user.id, Role::ProgramDirector)?;

    Ok(DemoSeed {
        user_id: user.id,
        team_id: team.id,
    })
}

fn demo_boats() -> Vec<NewBoat> {
    vec![
        NewBoat {
            name: "Titan".into(),
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
            name: "Athena".into(),
            weight_class: BoatWeightClass::Medium,
            seat_count: 8,
            has_cox: IntBool::TRUE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Starboard,
            cox_position: CoxPosition::Stern,
        },
        NewBoat {
            name: "Demeter".into(),
            weight_class: BoatWeightClass::Medium,
            seat_count: 8,
            has_cox: IntBool::TRUE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Port,
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
            weight_class: BoatWeightClass::Medium,
            seat_count: 4,
            has_cox: IntBool::TRUE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Port,
            cox_position: CoxPosition::Bow,
        },
        NewBoat {
            name: "Zephyr".into(),
            weight_class: BoatWeightClass::Medium,
            seat_count: 2,
            has_cox: IntBool::FALSE,
            oars_per_seat: 1,
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Port,
            cox_position: CoxPosition::Stern,
        },
    ]
}

fn demo_rowers() -> Vec<NewRower> {
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
        NewRower::sweep(
            "Juno",
            Medium,
            Sk::Intermediate,
            St::Intermediate,
            H::Medium,
            Either,
        ),
        NewRower::sweep("Kai", Light, Sk::Master, St::Strong, H::Medium, Port),
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
            r.sweep_bias = SweepBias::new(0);
            r
        },
        NewRower::sweep("Oscar", Heavy, Sk::Expert, St::VeryStrong, H::Tall, Port),
        NewRower::sweep(
            "Priya",
            Medium,
            Sk::Master,
            St::Strong,
            H::Medium,
            Starboard,
        ),
        NewRower::sweep(
            "Quinn",
            Light,
            Sk::Intermediate,
            St::Strong,
            H::Medium,
            Port,
        ),
        NewRower::sweep(
            "Rosa",
            Medium,
            Sk::Expert,
            St::Intermediate,
            H::Medium,
            Starboard,
        ),
        NewRower::sweep("Sam", Heavy, Sk::Expert, St::Strong, H::Tall, Either),
        {
            let mut r = NewRower::sweep("Lena", Light, Sk::Expert, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        {
            let mut r = NewRower::sweep("Yuki", Light, Sk::Master, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        {
            let mut r = NewRower::sweep("Ravi", Heavy, Sk::Novice, St::Weak, H::Tall, Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        NewRower::sweep(
            "Sara",
            Medium,
            Sk::Novice,
            St::Intermediate,
            H::Medium,
            Port,
        ),
        NewRower::sweep("Theo", Light, Sk::Novice, St::Weak, H::Short, Either),
        NewRower::sweep(
            "Uma",
            Medium,
            Sk::Novice,
            St::Intermediate,
            H::Medium,
            Starboard,
        ),
        NewRower::sweep("Vera", Light, Sk::Novice, St::Weak, H::Short, Port),
        {
            let mut r = NewRower::sweep(
                "Will",
                Heavy,
                Sk::Novice,
                St::Intermediate,
                H::Tall,
                Starboard,
            );
            r.can_cox = IntBool::FALSE;
            r
        },
        {
            let mut r = NewRower::sweep(
                "Xena",
                Medium,
                Sk::Novice,
                St::Intermediate,
                H::Medium,
                Either,
            );
            r.can_cox = IntBool::TRUE;
            r
        },
    ]
}
