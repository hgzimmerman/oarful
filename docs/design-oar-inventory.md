# Oar Inventory — Design Document

## Overview

Oar sets are team equipment assigned to boats per practice. Rowers need
to know which oars to bring to the dock, so this information appears in
the practice editor, history, and kiosk views.

---

## Data Model

```
oar_set (tenant DB)
  id          INTEGER PRIMARY KEY
  team_id     INTEGER NOT NULL REFERENCES team(id)
  name        TEXT NOT NULL              -- e.g. "Blue", "Pink", "Gold White"
  notes       TEXT                       -- e.g. "shorter shafts", "heavy blades"
  active      INTEGER NOT NULL DEFAULT 1 -- soft-delete
  created_at  TEXT NOT NULL

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

### Notes

- Oar sets belong to a team. Different teams may name their oars
  differently — no shared/global oar pool.
- `name` is free-text. No color enum — teams may use any naming
  convention.
- `notes` is optional, for distinguishing special equipment.
- One oar set per boat per practice. If a boat needs mixed oars,
  that's noted in the oar set's `notes` or by creating a combined
  set entry.
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

When a coach assigns oars in the practice editor, the UI can suggest
oar sets sorted by their preference for the selected boat. This is a
convenience, not a constraint — coaches can override freely.

---

## UI Surfaces

### Oar Set CRUD — `/oars` (Coach+ gated)

- List of oar sets for the current team: name, notes, status.
- Add/edit/deactivate. Same inline-edit pattern as boats page.
- Preference management: on the oar set detail/edit view, a sortable
  list of boats showing priority order. "Add boat preference" dropdown.

### Practice Editor

- Per-boat dropdown or pill selector for oar set assignment.
- Dropdown sorted by: (1) oar sets with a preference for this boat
  (by priority), then (2) all other active oar sets alphabetically.
- Selected oar set name displayed on the boat card.

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
- Oar set display on history detail

### Phase 3: Kiosk Integration
- Oar set label on kiosk lineup views (depends on kiosk feature)
