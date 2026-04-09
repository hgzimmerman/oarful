//! Post-solve extraction of `ProposedLineup`s from a Pumpkin solution.
//!
//! Decoupled from the model-building module so `lib.rs` stays focused
//! on the Pumpkin encoding and this file stays focused on "given the
//! assignment matrix and a value lookup, produce the coach-facing
//! shape". Pure data-in / data-out — no Solver, no posting, no
//! mutation.

use std::collections::BTreeMap;

use lineup_db::boat::Boat;
use lineup_db::rower::{types::RowerId, Rower};
use pumpkin_core::variables::DomainId;

use crate::ProposedLineup;

/// Walk the `x[(r, b, s)] ∈ {0, 1}` assignment matrix, bucketing
/// `x = 1` entries by boat and wrapping them up as `ProposedLineup`s
/// (one per candidate boat). `value_of` is the Pumpkin solution
/// accessor — the caller hands us a closure over the concrete
/// solution reference so this module never depends on the optimiser
/// result types directly.
pub(crate) fn decode_solution(
    x: &BTreeMap<(usize, usize, i32), DomainId>,
    use_b: &[DomainId],
    boats: &[&Boat],
    available: &[&Rower],
    mut value_of: impl FnMut(DomainId) -> i32,
) -> Vec<ProposedLineup> {
    let mut by_boat: BTreeMap<usize, Vec<(i32, usize)>> = BTreeMap::new();
    for (&(r_idx, b_idx, seat), &var) in x {
        if value_of(var) == 1 {
            by_boat.entry(b_idx).or_default().push((seat, r_idx));
        }
    }

    boats
        .iter()
        .enumerate()
        .map(|(b_idx, boat)| {
            let used = value_of(use_b[b_idx]) == 1;
            let mut seats: Vec<(i32, RowerId)> = by_boat
                .get(&b_idx)
                .map(|rows| {
                    rows.iter()
                        .map(|&(s, r_idx)| (s, available[r_idx].id))
                        .collect()
                })
                .unwrap_or_default();
            seats.sort_by_key(|&(s, _)| s);
            ProposedLineup {
                boat_id: boat.id,
                boat_name: boat.name.clone(),
                used,
                seats,
            }
        })
        .collect()
}
