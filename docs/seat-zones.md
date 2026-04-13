# Seat Zones

Seat zones replace absolute seat numbers for expressing where in a
boat a rower should sit. Instead of saying "seat 6" (which only
exists in an 8+), a coach says "engine room" — and the solver maps
that to the correct seats regardless of boat size.

## Why zones?

Absolute seat numbers create an unintentional bias toward larger
boats. A rower with preferences for seats 5, 6, 7, 8 gets reward
terms in an 8+ but none in a 4+ (which only has seats 1–4). The
solver reads this as "this rower is better in an 8" when the coach
actually means "this rower belongs in the stern/power end."

Zones decouple positional preference from boat size.

## Zone definitions

| Zone | Meaning |
|------|---------|
| **Stroke** | The stroke seat — sets the rhythm, highest technical demand |
| **Stern pair** | The two seats closest to stern (stroke + the seat behind) |
| **Stern half** | The stern half of the boat |
| **Engine room** | The middle power seats, excluding bow and stern pairs |
| **Bow half** | The bow half of the boat |
| **Bow pair** | The two seats closest to bow |
| **Bow** | The bow seat — steers in coxless boats, exposed position |

## Zone → seat mapping by boat size

Zones map to concrete seat numbers based on the boat's seat count.
A dash (—) means the zone produces no seats for that size and the
constraint is silently skipped.

| Zone | 8 (8+) | 4 (4+/4-/4x) | 2 (pair/double) | 1 (single) |
|------|--------|--------------|-----------------|------------|
| Stroke | 8 | 4 | 2 | — |
| Stern pair | 7, 8 | 3, 4 | — | — |
| Stern half | 5, 6, 7, 8 | 3, 4 | 2 | — |
| Engine room | 3, 4, 5, 6 | 2, 3 | — | — |
| Bow half | 1, 2, 3, 4 | 1, 2 | 1 | — |
| Bow pair | 1, 2 | 1, 2 | — | — |
| Bow | 1 | 1 | 1 | — |

### Formulas

For a boat with **N** rowing seats:

- **Stroke:** seat N (skip if N = 1)
- **Stern pair:** seats N−1, N (skip if N < 3)
- **Stern half:** seats ⌊N/2⌋+1 through N (if N ≥ 3); seat 2 (if N = 2)
- **Engine room:** seats 3 through N−2 (if N ≥ 8); seats 2 through N−1 (if 4 ≤ N < 8); empty otherwise
- **Bow half:** seats 1 through ⌊N/2⌋ (if N ≥ 3); seat 1 (if N = 2)
- **Bow pair:** seats 1, 2 (skip if N < 3)
- **Bow:** seat 1 (skip if N = 1)

### Design notes

- **Zones overlap intentionally.** Stroke is a subset of stern pair,
  which is a subset of stern half. A coach can express layered
  preferences: "+5 stroke, +3 stern half" means "ideally stroke, but
  anywhere in the stern half is good."

- **Overlapping zones use MAX, not sum.** When multiple zones match
  the same seat for a rower, the solver takes the highest weight.
  So "+5 stroke, +3 stern half" on seat 8 of an 8+ produces +5
  (not +8). This keeps weights intuitive and independent.

- **Singles get nothing.** With only one seat, there is no positional
  preference to express.

- **Pairs only fire Stroke, Stern half, Bow half, and Bow.** The
  pair-specific zones (stern pair, bow pair) require N ≥ 3 because
  in a pair the two seats already *are* the stroke and the bow.
  Engine room requires N ≥ 4.

- **Side is handled separately.** Zones are side-agnostic — they
  express *where* in the boat (bow-to-stern), not *which side*.
  Side preference is a separate constraint (S4).

## Solver constraints that use zones

- **S3 (seat affinity):** Coach-set per-rower zone preferences
  stored in `rower_seat_affinity`. The primary mechanism for
  expressing seat placement intent.

- **S11 (end-pair skill):** Automatic reward for placing skilled
  rowers in bow pair + stern pair zones. Acts as a fallback
  heuristic when rowers don't have explicit zone preferences.

- **S12 (engine-room strength):** Automatic reward for placing
  strong rowers in the engine room zone. Same fallback role as S11.

## Database schema

```sql
CREATE TABLE rower_seat_affinity (
    rower_id INTEGER NOT NULL,
    zone     TEXT CHECK(zone IN (
        'Stroke','SternPair','SternHalf',
        'EngineRoom','BowHalf','BowPair','Bow'
    )) NOT NULL,
    weight   INTEGER CHECK(weight BETWEEN -5 AND 5 AND weight != 0) NOT NULL,
    PRIMARY KEY (rower_id, zone),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);
```

Each rower can have at most one weight per zone. Positive weights
reward placement in the zone; negative weights penalize it.
