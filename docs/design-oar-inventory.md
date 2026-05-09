# Oar Inventory — Design Document

## Overview

Oar sets are team equipment assigned to boats per practice. Rowers need
to know which oars to bring to the dock, so this information appears in
the practice editor, history, and kiosk views.

Oar sets have a total count and can be split across multiple boats.
For example, a set of 8 sweep oars can serve one 8+ or be split between
two 4+s. The system tracks how many oars remain available from each set
and warns when a set is over-allocated.

---

## Data Model

```
oar_set (tenant DB)
  id          INTEGER PRIMARY KEY
  team_id     INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE
  name        TEXT NOT NULL              -- e.g. "Blue", "Pink", "Gold White"
  oar_count   INTEGER NOT NULL           -- total oars in this set (e.g. 8, 4)
  notes       TEXT                       -- e.g. "shorter shafts", "heavy blades"
  active      INTEGER NOT NULL DEFAULT 1 -- soft-delete
  created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP

  UNIQUE(team_id, name)
```

```
practice_boat_oars (tenant DB)
  id            INTEGER PRIMARY KEY
  practice_id   INTEGER NOT NULL REFERENCES practice(id)
  boat_id       INTEGER NOT NULL REFERENCES boat(id)
  oar_set_id    INTEGER NOT NULL REFERENCES oar_set(id)

  UNIQUE(practice_id, boat_id)
```

### Oar consumption per boat

Each boat consumes `seat_count * oars_per_seat` oars from its assigned
set. For a given practice, the total consumed from an oar set is the sum
across all boats assigned that set. If total consumed > `oar_count`, the
set is over-allocated and the UI shows a warning.

Examples:
- 8+ sweep (8 seats, 1 oar/seat) = 8 oars from the set
- 4+ sweep (4 seats, 1 oar/seat) = 4 oars — two 4+s can share an 8-oar set
- 2x scull (2 seats, 2 oars/seat) = 4 oars
- 1x scull (1 seat, 2 oars/seat) = 2 oars

### Notes

- Oar sets belong to a team. Different teams may name their oars
  differently — no shared/global oar pool.
- `name` is free-text. No color enum — teams may use any naming
  convention.
- `oar_count` is the total physical oars in the set.
- `notes` is optional, for distinguishing special equipment.
- Multiple boats per practice can share the same oar set (splitting).
  The `UNIQUE(practice_id, boat_id)` constraint ensures one oar set
  per boat, but the same oar set can appear on multiple boats.
- `active` flag for soft-delete (oars break, get retired).

---

## Fill Priorities

Oar sets may have preferred boats. Model as an ordered preference:

```
oar_set_preference (tenant DB)
  id          INTEGER PRIMARY KEY
  oar_set_id  INTEGER NOT NULL REFERENCES oar_set(id)
  boat_id     INTEGER NOT NULL REFERENCES boat(id)
  priority    INTEGER NOT NULL           -- lower = higher priority

  UNIQUE(oar_set_id, boat_id)
```

### Auto-suggest logic

When a coach assigns oars to a boat in the practice editor, the
dropdown shows oar sets sorted by:

1. Oar sets with a preference for this boat (by priority order)
2. All other active oar sets alphabetically

Each option shows remaining availability: e.g. "Blue (4/8 available)".
Sets that would be over-allocated are shown but marked with a warning.
Sets with zero remaining are grayed out but still selectable (coach
override).

The system could also auto-distribute: given the boats in use for a
practice, assign oar sets greedily by preference priority, consuming
from highest-priority first and splitting sets across multiple boats
when the set has enough oars. This is a convenience — coaches can
always override.

---

## UI Surfaces

### Oar Set CRUD — `/oars` (Coach+ gated)

- List of oar sets for the current team: name, oar count, notes, status.
- Add/edit/deactivate. Same inline-edit pattern as boats page.
- Preference management: on the oar set detail/edit view, a sortable
  list of boats showing priority order. "Add boat preference" dropdown.

### Practice Editor

- Per-boat dropdown for oar set assignment.
- Dropdown sorted by preference priority, then alphabetically.
- Shows remaining oar count per set (accounting for other boats in
  the same practice already using that set).
- Selected oar set name displayed on the boat card header.
- Over-allocation warning (amber) if total consumed exceeds oar_count.

### History Detail

- Oar set name shown next to each boat in the committed lineup.

### Kiosk Views

- Oar set name displayed next to each boat in the condensed lineup.
  Compact: just the name as a small label under/beside the boat name.

---

## Implementation Phases

### Phase 1: Model + CRUD
- `oar_set` table + migration
- `oar_set_preference` table + migration
- Oar set list/add/edit/deactivate endpoints + page
- Preference management UI

### Phase 2: Practice Integration
- `practice_boat_oars` table + migration
- Oar set selector in practice editor (per-boat)
- Remaining-oar tracking and over-allocation warnings
- Oar set display on history detail

### Phase 3: Kiosk Integration
- Oar set label on kiosk lineup views (depends on kiosk feature)
