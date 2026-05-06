//! Zone-based seat affinities. Each rower can have preferences for
//! named boat zones (Stroke, Engine Room, etc.) rather than absolute
//! seat numbers. The solver maps zones to concrete seats per boat
//! size at constraint time.

use crate::rower::types::RowerId;
use crate::schema::rower_seat_affinity;
use crate::types::AffinityWeight;
use diesel::prelude::*;
use diesel::SqliteConnection;

// ---- SeatZone enum ----------------------------------------------------

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    diesel_derive_enum::DbEnum,
)]
#[DbValueStyle = "verbatim"]
pub enum SeatZone {
    Stroke,
    SternPair,
    SternHalf,
    EngineRoom,
    BowHalf,
    BowPair,
    Bow,
}

impl SeatZone {
    pub const ALL: [SeatZone; 7] = [
        SeatZone::Bow,
        SeatZone::BowPair,
        SeatZone::BowHalf,
        SeatZone::EngineRoom,
        SeatZone::SternHalf,
        SeatZone::SternPair,
        SeatZone::Stroke,
    ];

    /// Human-friendly label for the UI.
    pub fn display_name(self) -> &'static str {
        match self {
            SeatZone::Stroke => "Stroke",
            SeatZone::SternPair => "Stern pair",
            SeatZone::SternHalf => "Stern half",
            SeatZone::EngineRoom => "Engine room",
            SeatZone::BowHalf => "Bow half",
            SeatZone::BowPair => "Bow pair",
            SeatZone::Bow => "Bow",
        }
    }

    /// The serialized string stored in the DB / used in form values.
    pub fn as_str(self) -> &'static str {
        match self {
            SeatZone::Stroke => "Stroke",
            SeatZone::SternPair => "SternPair",
            SeatZone::SternHalf => "SternHalf",
            SeatZone::EngineRoom => "EngineRoom",
            SeatZone::BowHalf => "BowHalf",
            SeatZone::BowPair => "BowPair",
            SeatZone::Bow => "Bow",
        }
    }

    /// Parse from the DB/form string representation.
    pub fn from_str_opt(s: &str) -> Option<SeatZone> {
        match s {
            "Stroke" => Some(SeatZone::Stroke),
            "SternPair" => Some(SeatZone::SternPair),
            "SternHalf" => Some(SeatZone::SternHalf),
            "EngineRoom" => Some(SeatZone::EngineRoom),
            "BowHalf" => Some(SeatZone::BowHalf),
            "BowPair" => Some(SeatZone::BowPair),
            "Bow" => Some(SeatZone::Bow),
            _ => None,
        }
    }

    /// Whether this zone targets exactly one seat in any applicable
    /// boat. Single-seat zones (Stroke, Bow) get a solver weight
    /// boost because they have no placement flexibility — the rower
    /// must land in that exact seat to satisfy the affinity.
    pub fn is_single_seat(self) -> bool {
        matches!(self, SeatZone::Stroke | SeatZone::Bow)
    }

    /// Return the concrete seat numbers this zone maps to for a boat
    /// with `n` seats. Returns an empty vec when the zone doesn't
    /// apply (e.g. Engine room in a pair, any zone in a single).
    pub fn seats_for(self, n: i32) -> Vec<i32> {
        match self {
            SeatZone::Stroke => {
                if n > 1 {
                    vec![n]
                } else {
                    vec![]
                }
            }
            SeatZone::SternPair => {
                if n >= 3 {
                    vec![n - 1, n]
                } else {
                    vec![]
                }
            }
            SeatZone::SternHalf => {
                if n >= 3 {
                    let start = n / 2 + 1; // 8→5, 4→3
                    (start..=n).collect()
                } else if n == 2 {
                    vec![2]
                } else {
                    vec![]
                }
            }
            SeatZone::EngineRoom => {
                if n >= 8 {
                    (3..=(n - 2)).collect() // 8→3..6
                } else if n >= 4 {
                    (2..=(n - 1)).collect() // 4→2..3
                } else {
                    vec![]
                }
            }
            SeatZone::BowHalf => {
                if n >= 3 {
                    let end = n / 2; // 8→4, 4→2
                    (1..=end).collect()
                } else if n == 2 {
                    vec![1]
                } else {
                    vec![]
                }
            }
            SeatZone::BowPair => {
                if n >= 3 {
                    vec![1, 2]
                } else {
                    vec![]
                }
            }
            SeatZone::Bow => {
                if n > 1 {
                    vec![1]
                } else {
                    vec![]
                }
            }
        }
    }
}

// ---- SeatAffinity model -----------------------------------------------

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Queryable, Selectable,
)]
#[diesel(table_name = crate::schema::rower_seat_affinity)]
pub struct SeatAffinity {
    pub rower_id: RowerId,
    pub zone: SeatZone,
    pub weight: AffinityWeight,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = crate::schema::rower_seat_affinity)]
pub struct NewSeatAffinity {
    pub rower_id: RowerId,
    pub zone: SeatZone,
    pub weight: AffinityWeight,
}

impl SeatAffinity {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewSeatAffinity,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(rower_seat_affinity::table)
            .values(new)
            .execute(conn)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Self>, diesel::result::Error> {
        rower_seat_affinity::table
            .select(Self::as_select())
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn list_for_rower(
        conn: &mut SqliteConnection,
        rower: RowerId,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        rower_seat_affinity::table
            .filter(rower_seat_affinity::rower_id.eq(rower))
            .select(Self::as_select())
            .get_results(conn)
    }

    /// Insert or update one (rower, zone) preference. The unique key
    /// is `(rower_id, zone)` so the upsert collapses any existing
    /// row's weight to the new value.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        rower: RowerId,
        zone: SeatZone,
        weight: AffinityWeight,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(rower_seat_affinity::table)
            .values(NewSeatAffinity {
                rower_id: rower,
                zone,
                weight,
            })
            .on_conflict((rower_seat_affinity::rower_id, rower_seat_affinity::zone))
            .do_update()
            .set(rower_seat_affinity::weight.eq(weight))
            .execute(conn)?;
        Ok(())
    }

    /// Remove one (rower, zone) preference. Silently no-ops if the
    /// row didn't exist.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn delete(
        conn: &mut SqliteConnection,
        rower: RowerId,
        zone: SeatZone,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            rower_seat_affinity::table
                .filter(rower_seat_affinity::rower_id.eq(rower))
                .filter(rower_seat_affinity::zone.eq(zone)),
        )
        .execute(conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SeatZone str round-trip ─────────────────────────────────────

    #[test]
    fn seat_zone_as_str_from_str_round_trip() {
        for zone in SeatZone::ALL {
            let s = zone.as_str();
            assert_eq!(
                SeatZone::from_str_opt(s),
                Some(zone),
                "round-trip failed for {s}"
            );
        }
    }

    #[test]
    fn seat_zone_from_str_opt_unknown() {
        assert!(SeatZone::from_str_opt("UnknownZone").is_none());
        assert!(SeatZone::from_str_opt("").is_none());
    }

    // ── is_single_seat ──────────────────────────────────────────────

    #[test]
    fn seat_zone_single_seat() {
        assert!(SeatZone::Stroke.is_single_seat());
        assert!(SeatZone::Bow.is_single_seat());
        assert!(!SeatZone::SternPair.is_single_seat());
        assert!(!SeatZone::EngineRoom.is_single_seat());
        assert!(!SeatZone::BowHalf.is_single_seat());
        assert!(!SeatZone::BowPair.is_single_seat());
        assert!(!SeatZone::SternHalf.is_single_seat());
    }

    // ── seats_for: 8-seat ───────────────────────────────────────────

    #[test]
    fn seats_for_eight() {
        assert_eq!(SeatZone::Stroke.seats_for(8), vec![8]);
        assert_eq!(SeatZone::SternPair.seats_for(8), vec![7, 8]);
        assert_eq!(SeatZone::SternHalf.seats_for(8), vec![5, 6, 7, 8]);
        assert_eq!(SeatZone::EngineRoom.seats_for(8), vec![3, 4, 5, 6]);
        assert_eq!(SeatZone::BowHalf.seats_for(8), vec![1, 2, 3, 4]);
        assert_eq!(SeatZone::BowPair.seats_for(8), vec![1, 2]);
        assert_eq!(SeatZone::Bow.seats_for(8), vec![1]);
    }

    // ── seats_for: 4-seat ───────────────────────────────────────────

    #[test]
    fn seats_for_four() {
        assert_eq!(SeatZone::Stroke.seats_for(4), vec![4]);
        assert_eq!(SeatZone::SternPair.seats_for(4), vec![3, 4]);
        assert_eq!(SeatZone::SternHalf.seats_for(4), vec![3, 4]);
        assert_eq!(SeatZone::EngineRoom.seats_for(4), vec![2, 3]);
        assert_eq!(SeatZone::BowHalf.seats_for(4), vec![1, 2]);
        assert_eq!(SeatZone::BowPair.seats_for(4), vec![1, 2]);
        assert_eq!(SeatZone::Bow.seats_for(4), vec![1]);
    }

    // ── seats_for: pair (2-seat) ────────────────────────────────────

    #[test]
    fn seats_for_pair() {
        assert_eq!(SeatZone::Stroke.seats_for(2), vec![2_i32]);
        assert_eq!(SeatZone::SternPair.seats_for(2), Vec::<i32>::new()); // need >= 3
        assert_eq!(SeatZone::SternHalf.seats_for(2), vec![2_i32]);
        assert_eq!(SeatZone::EngineRoom.seats_for(2), Vec::<i32>::new()); // need >= 4
        assert_eq!(SeatZone::BowHalf.seats_for(2), vec![1_i32]);
        assert_eq!(SeatZone::BowPair.seats_for(2), Vec::<i32>::new()); // need >= 3
        assert_eq!(SeatZone::Bow.seats_for(2), vec![1_i32]);
    }

    // ── seats_for: single (1-seat) ─────────────────────────────────

    #[test]
    fn seats_for_single() {
        for zone in SeatZone::ALL {
            assert_eq!(
                zone.seats_for(1),
                Vec::<i32>::new(),
                "zone {zone:?} should be empty for n=1"
            );
        }
    }

    // ── display_name ────────────────────────────────────────────────

    #[test]
    fn seat_zone_display_names() {
        assert_eq!(SeatZone::Stroke.display_name(), "Stroke");
        assert_eq!(SeatZone::SternPair.display_name(), "Stern pair");
        assert_eq!(SeatZone::EngineRoom.display_name(), "Engine room");
        assert_eq!(SeatZone::BowPair.display_name(), "Bow pair");
        assert_eq!(SeatZone::Bow.display_name(), "Bow");
    }
}
