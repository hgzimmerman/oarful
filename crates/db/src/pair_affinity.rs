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
use crate::types::AffinityWeight;
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
    pub weight: AffinityWeight,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::pair_affinity)]
pub struct NewPairAffinity {
    pub rower_a_id: RowerId,
    pub rower_b_id: RowerId,
    pub weight: AffinityWeight,
}

impl NewPairAffinity {
    /// Canonicalise the ordering so `rower_a_id < rower_b_id`, matching
    /// the SQL `CHECK` constraint. Panics on self-pairs since those are
    /// meaningless.
    pub fn canonical(a: RowerId, b: RowerId, weight: AffinityWeight) -> Self {
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

    /// Every pair affinity that mentions `rower` on either side. Used
    /// by the per-rower detail page so the coach sees both directions
    /// of the symmetric relationship in one list.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn list_for_rower(
        conn: &mut SqliteConnection,
        rower: RowerId,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        pair_affinity::table
            .filter(
                pair_affinity::rower_a_id
                    .eq(rower)
                    .or(pair_affinity::rower_b_id.eq(rower)),
            )
            .select(Self::as_select())
            .get_results(conn)
    }

    /// Insert or update one canonical pair. Caller-supplied IDs are
    /// reordered so `a < b` matches the SQL `CHECK` constraint, then
    /// the unique `(rower_a_id, rower_b_id)` index resolves the
    /// upsert.
    ///
    /// Returns `Err(NotFound)` if `a == b` (a self-pair is meaningless
    /// and `NewPairAffinity::canonical` would panic). Callers should
    /// validate this at the form layer first; the check here is a
    /// belt-and-braces guard.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        a: RowerId,
        b: RowerId,
        weight: AffinityWeight,
    ) -> Result<(), diesel::result::Error> {
        if a == b {
            return Err(diesel::result::Error::NotFound);
        }
        let new = NewPairAffinity::canonical(a, b, weight);
        diesel::insert_into(pair_affinity::table)
            .values(&new)
            .on_conflict((pair_affinity::rower_a_id, pair_affinity::rower_b_id))
            .do_update()
            .set(pair_affinity::weight.eq(weight))
            .execute(conn)?;
        Ok(())
    }

    /// Remove one canonical pair. Caller-supplied IDs are reordered
    /// before the delete. Silently no-ops if the row didn't exist.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn delete(
        conn: &mut SqliteConnection,
        a: RowerId,
        b: RowerId,
    ) -> Result<(), diesel::result::Error> {
        if a == b {
            return Ok(());
        }
        let (a, b) = if a.as_int() < b.as_int() {
            (a, b)
        } else {
            (b, a)
        };
        diesel::delete(
            pair_affinity::table
                .filter(pair_affinity::rower_a_id.eq(a))
                .filter(pair_affinity::rower_b_id.eq(b)),
        )
        .execute(conn)?;
        Ok(())
    }
}
