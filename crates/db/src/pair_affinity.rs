//! Pair affinity: two rowers' relationship as a rowing pair.
//!
//! In rowing, a "pair" is a fixed 2-seat partition of a boat, not just any
//! two adjacent rowers. For an 8+: `(1,2), (3,4), (5,6), (7,8)`. The
//! partition always contains one port and one starboard rower (standard
//! alternating rig), so pair affinities are implicitly opposite-sided.
//!
//! Stored rows are canonicalised with `rower_a_id < rower_b_id` to avoid
//! double-storing the symmetric relationship. Positive weight rewards the
//! solver for placing A and B in the same partition; negative weight
//! penalises it. The cox seat is not part of any pair — designated coxes
//! or rowers temporarily coxing a boat are never "in a partition".

use crate::rower::types::RowerId;
use crate::schema::pair_affinity;
use diesel::prelude::*;
use diesel::SqliteConnection;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
)]
#[diesel(table_name = crate::schema::pair_affinity)]
pub struct PairAffinity {
    pub rower_a_id: RowerId,
    pub rower_b_id: RowerId,
    pub weight: i32,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::pair_affinity)]
pub struct NewPairAffinity {
    pub rower_a_id: RowerId,
    pub rower_b_id: RowerId,
    pub weight: i32,
}

impl NewPairAffinity {
    /// Canonicalise the ordering so `rower_a_id < rower_b_id`, matching
    /// the SQL `CHECK` constraint. Panics on self-pairs since those are
    /// meaningless.
    pub fn canonical(a: RowerId, b: RowerId, weight: i32) -> Self {
        assert!(a != b, "pair affinity cannot reference the same rower twice");
        let (rower_a_id, rower_b_id) = if a.as_int() < b.as_int() {
            (a, b)
        } else {
            (b, a)
        };
        Self {
            rower_a_id,
            rower_b_id,
            weight,
        }
    }
}

impl PairAffinity {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewPairAffinity,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(pair_affinity::table)
            .values(new)
            .execute(conn)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Self>, diesel::result::Error> {
        pair_affinity::table
            .select(Self::as_select())
            .get_results(conn)
    }
}
