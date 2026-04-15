//! Toy fixture data so `cargo run -p lineup_cli` has something to show on an
//! empty database. This is intentionally small — 14 rowers, 3 sweep boats,
//! one practice date, hand-authored availabilities.
//!
//! `seed_demo` is a richer fixture for the self-service demo: 24 rowers,
//! 6 boats, 3 practices with varying availability, and a committed lineup
//! on the first date.

use crate::app_user::{AppUser, NewAppUser, Role};
use crate::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use crate::boat::types::{CoxPosition, WeightClass as BoatWeightClass};
use crate::boat::{Boat, NewBoat};
use crate::lineup::{CommitSeat, Lineup};
use crate::pair_affinity::{NewPairAffinity, PairAffinity};
use crate::practice::Practice;
use crate::rower::types::{Height, RowerWeightClass, Side, Skill, Strength};
use crate::rower::{NewRower, Rower};
use crate::seat_affinity::{NewSeatAffinity, SeatAffinity, SeatZone};
use crate::team::{NewTeam, Team, TeamMembership};
use crate::rower::types::SweepBias;
use crate::types::{AffinityWeight, IntBool};
use chrono::{Datelike, NaiveDate, Weekday};
use diesel::prelude::*;
use diesel::SqliteConnection;

// =====================================================================
// Dev fixture (14 rowers, 3 boats — used by baseline tests)
// =====================================================================

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
/// Useful for testing the empty-roster flow. Checks for existing
/// boats to avoid double-seeding.
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
            None => Team::create(conn, NewTeam { name: "Sweep".to_string(), created_at: now })?,
        };
        for b in toy_boats() {
            Boat::insert(conn, b)?;
        }
        // Dev coach account
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

    // --- team (use existing if the migration seeded one, else create) ---
    let now = chrono::Utc::now().naive_utc();
    let team = match Team::first(conn)? {
        Some(t) => t,
        None => Team::create(conn, NewTeam { name: "Sweep".to_string(), created_at: now })?,
    };

    // --- boats (sweep only; scullers handle their own) ---
    for b in toy_boats() {
        Boat::insert(conn, b)?;
    }

    // --- rowers + team membership ---
    let mut rower_ids = Vec::new();
    for r in toy_rowers() {
        let inserted = Rower::insert(conn, r)?;
        TeamMembership::add(conn, team.id, inserted.id)?;
        rower_ids.push(inserted.id);
    }

    // --- practice + availabilities for an upcoming date ---
    let date = NaiveDate::from_ymd_opt(2026, 4, 11).expect("valid date");
    let practice = Practice::upsert(conn, team.id, date, None, Some("Toy seeded practice".to_string()))?;

    let statuses = [
        AvailabilityStatus::Yes,         // Alice
        AvailabilityStatus::Yes,         // Bob
        AvailabilityStatus::Yes,         // Carla
        AvailabilityStatus::Yes,         // Diego
        AvailabilityStatus::Yes,         // Erin
        AvailabilityStatus::Yes,         // Finn
        AvailabilityStatus::Yes,         // Grace
        AvailabilityStatus::Yes,         // Hana
        AvailabilityStatus::Yes,         // Ivan
        AvailabilityStatus::No,          // Juno
        AvailabilityStatus::No,          // Kai
        AvailabilityStatus::Yes,         // Lena (designated cox)
        AvailabilityStatus::Yes,         // Mika (non-designated cox)
        AvailabilityStatus::Yes,         // Nico (sweep_bias handles scull distinction)
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

    // Seat affinities. Indices correspond to `toy_rowers()`:
    //   0 Alice, 1 Bob, 2 Carla, 3 Diego, 4 Erin, 5 Finn, 6 Grace,
    //   7 Hana, 8 Ivan, 9 Juno, 10 Kai, 11 Lena, 12 Mika, 13 Nico
    //
    // - Alice (Port, Expert) likes stroke. Weight +3.
    // - Hana (Port, Master) likes bow pair. Weight +3.
    // - Ivan (Starboard, Novice) avoids bow. Weight -2.
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

    // Pair affinities. Two examples:
    //   - Alice (0, Port/Medium/Expert) + Erin (4, Either/Medium/Expert):
    //     +4. These two train together as a pair.
    //   - Carla (2, Port/Light/Intermediate) + Diego (3, Starboard/
    //     Medium/Master): +2.
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

    // --- dev user (ProgramDirector, password "12345") ---
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
        NewRower::sweep("Bob", Heavy, Sk::Master, St::VeryStrong, H::VeryTall, Starboard),
        NewRower::sweep("Carla", Light, Sk::Intermediate, St::Intermediate, H::Short, Port),
        NewRower::sweep("Diego", Medium, Sk::Master, St::Strong, H::Tall, Starboard),
        NewRower::sweep("Erin", Medium, Sk::Expert, St::Strong, H::Medium, Either),
        NewRower::sweep("Finn", Heavy, Sk::Intermediate, St::Strong, H::VeryTall, Port),
        NewRower::sweep("Grace", Light, Sk::Expert, St::Intermediate, H::Short, Starboard),
        NewRower::sweep("Hana", Medium, Sk::Master, St::VeryStrong, H::Tall, Port),
        {
            let mut r = NewRower::sweep("Ivan", Heavy, Sk::Novice, St::Weak, H::Tall, Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        NewRower::sweep("Juno", Medium, Sk::Intermediate, St::Intermediate, H::Medium, Either),
        NewRower::sweep("Kai", Light, Sk::Master, St::Strong, H::Medium, Port),
        {
            let mut r = NewRower::sweep("Lena", Light, Sk::Expert, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        NewRower::sweep("Mika", Medium, Sk::Master, St::Strong, H::Medium, Starboard),
        {
            let mut r =
                NewRower::sweep("Nico", Medium, Sk::Intermediate, St::Strong, H::Tall, Either);
            r.sweep_bias = SweepBias::SCULL_HARD;
            r
        },
    ]
}

// =====================================================================
// Demo fixture (24 rowers, 6 boats, 3 practices)
// =====================================================================

/// Find the next occurrence of a weekday on or after `from`.
fn next_weekday(from: NaiveDate, day: Weekday) -> NaiveDate {
    let days_ahead = (day.num_days_from_monday() as i64
        - from.weekday().num_days_from_monday() as i64
        + 7) % 7;
    // If today is that weekday, push to next week.
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

    // --- team ---
    let team = Team::create(conn, NewTeam { name: "Demo Rowing Club".to_string(), created_at: now })?;

    // --- boats ---
    // 1 heavy 8+, 2 medium 8+, 2 medium 4+, 1 light pair
    let boat_specs = demo_boats();
    let mut boat_ids = Vec::new();
    for b in boat_specs {
        let inserted = Boat::insert(conn, b)?;
        boat_ids.push(inserted.id);
    }
    // Indices: 0=Titan(H8+), 1=Athena(M8+), 2=Demeter(M8+),
    //          3=Artemis(M4+), 4=Hestia(M4+), 5=Zephyr(L2-)

    // --- 24 rowers ---
    let rower_specs = demo_rowers();
    let mut rower_ids = Vec::new();
    for r in rower_specs {
        let inserted = Rower::insert(conn, r)?;
        TeamMembership::add(conn, team.id, inserted.id)?;
        rower_ids.push(inserted.id);
    }
    // Indices (see demo_rowers for attributes):
    //  0  Alice    Expert/Strong/Port/Tall/Medium
    //  1  Bob      Master/VeryStrong/Starboard/VeryTall/Heavy
    //  2  Carla    Intermediate/Intermediate/Port/Short/Light
    //  3  Diego    Master/Strong/Starboard/Tall/Medium
    //  4  Erin     Expert/Strong/Either/Medium/Medium
    //  5  Finn     Intermediate/Strong/Port/VeryTall/Heavy
    //  6  Grace    Expert/Intermediate/Starboard/Short/Light
    //  7  Hana     Master/VeryStrong/Port/Tall/Medium
    //  8  Juno     Intermediate/Intermediate/Either/Medium/Medium
    //  9  Kai      Master/Strong/Port/Medium/Light
    // 10  Mika     Master/Strong/Starboard/Medium/Medium
    // 11  Nico     Intermediate/Strong/Either/Tall/Medium (ambivalent sweep_bias)
    // 12  Oscar    Expert/VeryStrong/Port/Tall/Heavy
    // 13  Priya    Master/Strong/Starboard/Medium/Medium
    // 14  Quinn    Intermediate/Strong/Port/Medium/Light
    // 15  Rosa     Expert/Intermediate/Starboard/Medium/Medium
    // 16  Sam      Expert/Strong/Either/Tall/Heavy
    // 17  Lena     Expert/Weak/Either/Short/Light (DESIGNATED COX)
    // 18  Yuki     Master/Weak/Either/Short/Light (DESIGNATED COX)
    // 19  Ravi     Novice/Weak/Starboard/Tall/Heavy
    // 20  Sara     Novice/Intermediate/Port/Medium/Medium
    // 21  Theo     Novice/Weak/Either/Short/Light
    // 22  Uma      Novice/Intermediate/Starboard/Medium/Medium
    // 23  Vera     Novice/Weak/Port/Short/Light
    // 24  Will     Novice/Intermediate/Starboard/Tall/Heavy
    // 25  Xena     Novice/Intermediate/Either/Medium/Medium (can cox)

    // --- seat affinities ---
    // Alice likes stroke.
    // Bob likes the engine room.
    // Hana prefers stern half.
    for (idx, zone, weight) in [
        (0usize, SeatZone::Stroke, 3),
        (1, SeatZone::EngineRoom, 2),
        (7, SeatZone::SternHalf, 3),
    ] {
        SeatAffinity::insert(conn, NewSeatAffinity {
            rower_id: rower_ids[idx],
            zone,
            weight: AffinityWeight::new(weight),
        })?;
    }

    // --- pair affinities ---
    // Alice + Erin train together (+4).
    // Diego + Mika are a strong starboard pair (+3).
    // Oscar + Sam are experienced port-side friends (+2).
    for (a, b, w) in [(0usize, 4usize, 4), (3, 10, 3), (12, 16, 2)] {
        PairAffinity::insert(conn, NewPairAffinity::canonical(
            rower_ids[a], rower_ids[b], AffinityWeight::new(w),
        ))?;
    }

    // --- 3 practices: next Mon, Wed, Fri ---
    let mon = next_weekday(today, Weekday::Mon);
    let wed = next_weekday(today, Weekday::Wed);
    let fri = next_weekday(today, Weekday::Fri);

    let p_mon = Practice::upsert(conn, team.id, mon, None, Some("Steady state pieces".to_string()))?;
    let p_wed = Practice::upsert(conn, team.id, wed, None, Some("Technique drills".to_string()))?;
    let p_fri = Practice::upsert(conn, team.id, fri, None, None)?;

    // --- availability ---
    // Monday: most available (Ravi, Theo, Vera say No)
    let mon_no = [19usize, 21, 23];
    for (i, rid) in rower_ids.iter().enumerate() {
        let status = if mon_no.contains(&i) { AvailabilityStatus::No } else { AvailabilityStatus::Yes };
        Availability::upsert(conn, NewAvailability {
            rower_id: *rid, practice_id: p_mon.id, status,
        })?;
    }

    // Wednesday: everyone available
    for rid in &rower_ids {
        Availability::upsert(conn, NewAvailability {
            rower_id: *rid, practice_id: p_wed.id, status: AvailabilityStatus::Yes,
        })?;
    }

    // Friday: 22 available (rowers 5=Finn, 8=Juno say No)
    for (i, rid) in rower_ids.iter().enumerate() {
        let status = match i {
            5 | 8 => AvailabilityStatus::No,
            _ => AvailabilityStatus::Yes,
        };
        Availability::upsert(conn, NewAvailability {
            rower_id: *rid, practice_id: p_fri.id, status,
        })?;
    }

    // --- committed lineup for Monday ---
    // Place 8 rowers in Athena (medium 8+, boat index 1) + cox Lena.
    // This gives the history page something to show.
    let athena_id = boat_ids[1];
    let athena_seats: Vec<CommitSeat> = vec![
        CommitSeat { seat_position: 0, rower_id: rower_ids[17], is_cox: true },  // Lena cox
        CommitSeat { seat_position: 1, rower_id: rower_ids[6],  is_cox: false }, // Grace bow
        CommitSeat { seat_position: 2, rower_id: rower_ids[2],  is_cox: false }, // Carla
        CommitSeat { seat_position: 3, rower_id: rower_ids[3],  is_cox: false }, // Diego
        CommitSeat { seat_position: 4, rower_id: rower_ids[8],  is_cox: false }, // Juno
        CommitSeat { seat_position: 5, rower_id: rower_ids[1],  is_cox: false }, // Bob
        CommitSeat { seat_position: 6, rower_id: rower_ids[7],  is_cox: false }, // Hana
        CommitSeat { seat_position: 7, rower_id: rower_ids[4],  is_cox: false }, // Erin
        CommitSeat { seat_position: 8, rower_id: rower_ids[0],  is_cox: false }, // Alice stroke
    ];
    Lineup::commit_for_boat(conn, p_mon.id, athena_id, &athena_seats)?;

    // Place 4 rowers in Artemis (medium 4+, boat index 3) + cox Xena.
    let artemis_id = boat_ids[3];
    let artemis_seats: Vec<CommitSeat> = vec![
        CommitSeat { seat_position: 0, rower_id: rower_ids[25], is_cox: true },  // Xena cox
        CommitSeat { seat_position: 1, rower_id: rower_ids[14], is_cox: false }, // Quinn bow
        CommitSeat { seat_position: 2, rower_id: rower_ids[20], is_cox: false }, // Sara
        CommitSeat { seat_position: 3, rower_id: rower_ids[22], is_cox: false }, // Uma
        CommitSeat { seat_position: 4, rower_id: rower_ids[13], is_cox: false }, // Priya stroke
    ];
    Lineup::commit_for_boat(conn, p_mon.id, artemis_id, &artemis_seats)?;

    // --- demo user (ProgramDirector, no password) ---
    let user = AppUser::create(conn, NewAppUser {
        email: "demo@localhost".to_string(),
        password_hash: None,
        name: "Demo Coach".to_string(),
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
    })?;
    AppUser::set_role(conn, user.id, Role::ProgramDirector)?;

    Ok(DemoSeed {
        user_id: user.id,
        team_id: team.id,
    })
}

fn demo_boats() -> Vec<NewBoat> {
    vec![
        // 0: Titan — heavy 8+ (starboard rigged, cox stern)
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
        // 1: Athena — medium 8+ (starboard rigged, cox stern)
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
        // 2: Demeter — medium 8+ (port rigged, cox stern)
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
        // 3: Artemis — medium 4+ (starboard rigged, cox bow)
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
        // 4: Hestia — medium 4+ (port rigged, cox bow)
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
        // 5: Zephyr — medium pair (no cox, port rigged)
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
        // --- experienced rowers (17) ---
        NewRower::sweep("Alice", Medium, Sk::Expert, St::Strong, H::Tall, Port), //  0
        NewRower::sweep("Bob",    Heavy,  Sk::Master,        St::VeryStrong,   H::VeryTall, Starboard),  //  1
        NewRower::sweep("Carla",  Light,  Sk::Intermediate,  St::Intermediate, H::Short,    Port),       //  2
        NewRower::sweep("Diego",  Medium, Sk::Master,        St::Strong,       H::Tall,     Starboard),  //  3
        NewRower::sweep("Erin",   Medium, Sk::Expert,        St::Strong,       H::Medium,   Either),     //  4
        NewRower::sweep("Finn",   Heavy,  Sk::Intermediate,  St::Strong,       H::VeryTall, Port),       //  5
        NewRower::sweep("Grace",  Light,  Sk::Expert,        St::Intermediate, H::Short,    Starboard),  //  6
        NewRower::sweep("Hana",   Medium, Sk::Master,        St::VeryStrong,   H::Tall,     Port),       //  7
        NewRower::sweep("Juno",   Medium, Sk::Intermediate,  St::Intermediate, H::Medium,   Either),     //  8
        NewRower::sweep("Kai",    Light,  Sk::Master,        St::Strong,       H::Medium,   Port),       //  9
        NewRower::sweep("Mika",   Medium, Sk::Master,        St::Strong,       H::Medium,   Starboard),  // 10
        {                                                                                                 // 11
            let mut r = NewRower::sweep("Nico", Medium, Sk::Intermediate, St::Strong, H::Tall, Either);
            r.sweep_bias = SweepBias::new(0); // ambivalent
            r
        },
        NewRower::sweep("Oscar",  Heavy,  Sk::Expert,        St::VeryStrong,   H::Tall,     Port),       // 12
        NewRower::sweep("Priya",  Medium, Sk::Master,        St::Strong,       H::Medium,   Starboard),  // 13
        NewRower::sweep("Quinn",  Light,  Sk::Intermediate,  St::Strong,       H::Medium,   Port),       // 14
        NewRower::sweep("Rosa",   Medium, Sk::Expert,        St::Intermediate, H::Medium,   Starboard),  // 15
        NewRower::sweep("Sam",    Heavy,  Sk::Expert,        St::Strong,       H::Tall,     Either),     // 16
        // --- designated cox ---
        {                                                                                                 // 17
            let mut r = NewRower::sweep("Lena", Light, Sk::Expert, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        // --- second designated cox ---
        {                                                                                                 // 18
            let mut r = NewRower::sweep("Yuki", Light, Sk::Master, St::Weak, H::Short, Either);
            r.is_designated_cox = IntBool::TRUE;
            r
        },
        // --- novices (7) ---
        {                                                                                                 // 19
            let mut r = NewRower::sweep("Ravi", Heavy, Sk::Novice, St::Weak, H::Tall, Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        NewRower::sweep("Sara",   Medium, Sk::Novice,        St::Intermediate, H::Medium,   Port),       // 20
        NewRower::sweep("Theo",   Light,  Sk::Novice,        St::Weak,         H::Short,    Either),     // 21
        NewRower::sweep("Uma",    Medium, Sk::Novice,        St::Intermediate, H::Medium,   Starboard),  // 22
        NewRower::sweep("Vera",   Light,  Sk::Novice,        St::Weak,         H::Short,    Port),       // 23
        {                                                                                                 // 24
            let mut r = NewRower::sweep("Will", Heavy, Sk::Novice, St::Intermediate, H::Tall, Starboard);
            r.can_cox = IntBool::FALSE;
            r
        },
        {                                                                                                 // 25
            let mut r = NewRower::sweep("Xena", Medium, Sk::Novice, St::Intermediate, H::Medium, Either);
            r.can_cox = IntBool::TRUE; // backup cox
            r
        },
    ]
}
