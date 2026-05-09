# Practice Plan Templates & Drill Library

## Problem

Coaches build practice plans from scratch for each session using the
timeline editor. Over a season they develop patterns — a typical Tuesday
steady-state session, a race-prep plan, a technique day — but there's no
way to save and reuse these. Coaches end up rebuilding similar plans
repeatedly or relying on memory.

Additionally, the seven built-in group templates (pick drill, steady 4x15,
etc.) are hardcoded and can't be extended. Coaches accumulate their own
drills and pieces but have no library to manage them.

## Current State

| Aspect | Today |
|--------|-------|
| Practice plans | Stored as `timeline_json` on each practice row |
| Reuse | None — each plan built from scratch |
| Built-in templates | 7 hardcoded group templates (warmup drills + piece sets) |
| Group templates | Insert a single group before dock; not saveable |
| Editor | Embedded in practice detail page |

## Proposed Changes

### 1. Practice Plan Templates

Full practice timelines (launch through dock) that can be saved, searched,
categorized, named, and imported into a practice.

**Data model:**

```
practice_plan_template
    id              INTEGER PRIMARY KEY
    name            TEXT NOT NULL
    description     TEXT NOT NULL DEFAULT ''
    category        TEXT          -- nullable, free-form label (case-insensitive normalized)
    author_id       INTEGER REFERENCES app_user(id)  -- nullable, for attribution/search
    timeline_json   TEXT NOT NULL  -- same format as practice.timeline_json
    created_at      TEXT NOT NULL
    updated_at      TEXT NOT NULL
    UNIQUE (name, author_id)
```

Tenant-wide (no team_id). No per-team visibility for v1 — all teams see
all templates.

**Permissions:** Coach+ can create, edit, and delete any template
(last-edit-wins, no per-author locking). Author is tracked for
attribution and search filtering, not access control.

**Creating a template:**
- Dedicated management page at `/settings/practice-templates` (or similar
  path in the settings rail).
- Uses the same timeline editor (strip, palette, segment editors) as the
  practice detail page, but detached from a specific practice.
- The editor works on the template's `timeline_json` directly.
- The practice detail timeline handlers and template handlers both call
  into a shared set of extracted timeline-mutation functions. Each
  handler layer provides the shim for loading/saving from the right
  backing store (practice row vs template row).

**Importing into a practice:**
- From the practice detail page, a coach can import a template.
- Import **replaces** the practice's current timeline entirely (with
  confirmation if one already exists).
- The imported timeline is a snapshot copy — subsequent edits to the
  template don't affect practices that already imported it, and edits
  to the practice plan don't affect the template.
- Templates whose `target_minutes` exceed the practice's allocated time
  are shown with a warning indicator in the import picker. Negative
  slack (over-planned) is allowed — coaches can trim after import.
- Importing a template clears `plan_dismissed` if it was set. Dismissing
  a plan is not a terminal state — coaches can always come back and add
  or import a plan later.

**Management page layout:**
- List of all templates with name, author, description, category badge,
  and approximate duration.
- Filter/search by name, author, and category.
- Click to open the template in the timeline editor.
- Actions: edit, duplicate, delete, rename.

**Categories:**
- Single optional label per template (nullable `category` column).
- Stored case-insensitive (normalized to lowercase or title case on save).
- Programs create their own categories organically by typing a label.
- UI shows existing categories as filter pills / autocomplete suggestions
  when assigning a category.
- Categories are global across plan templates and drill library — a
  category created in one context appears as an autocomplete option in
  the other.
- Most programs won't use categories — the UI should work fine with
  everything uncategorized (just a flat searchable list).
- Future: may expand to allow multiple categories per item.

**Stretch — save from practice:**
A "Save as template" action on the practice detail page that copies the
current timeline into a new template. Low priority — coaches can
accomplish the same thing by creating a template on the management page
and building it there. Could add clutter to the practice page.

### 2. Drill & Piece Library (Stretch)

Reusable saved groups (warmups, drills, pieces) that can be inserted into
any practice plan or template. Replaces and extends the current hardcoded
built-in templates.

**Data model:**

```
drill_template
    id              INTEGER PRIMARY KEY
    name            TEXT NOT NULL
    description     TEXT NOT NULL DEFAULT ''
    category        TEXT          -- nullable, same global category namespace
    author_id       INTEGER REFERENCES app_user(id)  -- nullable
    group_json      TEXT NOT NULL  -- serialized Group (same structure as timeline Group)
    is_default      BOOLEAN NOT NULL DEFAULT FALSE  -- seeded from built-ins
    created_at      TEXT NOT NULL
    updated_at      TEXT NOT NULL
    UNIQUE (name, author_id)
```

Tenant-wide. `group_json` contains a serialized `Group` (with its
segments, repeat, rotation config — everything the current built-in
templates define).

**Permissions:** Same as plan templates — Coach+ for all CRUD, last-edit-
wins, author tracked for attribution only.

**Seeding defaults:**
- On app startup, if a tenant has zero `drill_template` rows, the 7
  existing built-in templates are materialized into rows with
  `is_default = TRUE` and `author_id = NULL`.
- Coaches can delete the defaults. If all are deleted and no custom
  drills exist, they will be re-seeded on next restart (accepted minor
  annoyance — unlikely in practice).
- The `is_default` flag supports a "restore defaults" action that
  re-inserts missing defaults.
- The hardcoded `built_in_templates()` function in `timeline.rs` becomes
  the seed source only — runtime reads exclusively from DB.

**Using library items in the editor:**
- The timeline editor palette (both on practice plans and plan templates)
  gets a new entry point for library drills — a searchable dropdown or
  modal triggered from the palette area.
- Selecting a library item inserts a copy of the group before the dock
  (same as current built-in template insertion).
- Copy-on-insert: the inserted group is independent of the library item.
  Editing one doesn't affect the other.
- Library items can be filtered by category and searched by name.

**Management page:**
- A section on the settings page (possibly a tab alongside plan templates,
  or its own page at `/settings/drills`).
- List with name, author, description, category badge, group type
  (warmup/piece), approximate duration.
- Click to edit in the drill editor.
- Actions: edit, duplicate, delete, rename, restore defaults.

**Drill editor:**
- Operates on a timeline constrained to exactly one group (plus the
  launch/dock bookends for structural consistency, hidden in the UI).
- Separate endpoints from the practice/template timeline editors, but
  calls into the same shared timeline-mutation functions.
- Shows the group editor and segment editor; hides the palette, duration
  meter, and item-level add/reorder (since there's only one group).

**Naming:**
Recommendation: **"Library"** as the nav label, with items described as
"drills" and "pieces" in context (badges, group type indicators). The
page title could be "Drill & Piece Library."

## UI Integration Points

**Settings rail:**
- "Practice Templates" — plan template management page
- "Library" (stretch) — drill & piece library

**Practice detail page:**
- "Use template" button/dropdown to import a plan template (replaces
  current timeline with confirmation, clears plan_dismissed)
- In the timeline editor palette: library dropdown/modal for inserting
  saved drills/pieces (stretch)

**Timeline editor (shared logic):**
- Core timeline-mutation functions extracted from current handlers,
  used in three contexts with thin handler shims:
  1. Practice detail page (load/save from practice row)
  2. Plan template management page (load/save from template row)
  3. Drill editor (load/save from drill_template row, single-group
     constraint)

## Migration Path

**Phase 1 — Plan templates:**
1. Create `practice_plan_template` table.
2. Extract timeline-mutation logic from practice handlers into shared
   functions.
3. Build template handlers as shims over shared logic.
4. Build management page with timeline editor integration.
5. Add "Use template" import flow to practice detail page.
6. Revise `plan_dismissed` to be non-terminal.
7. Add category autocomplete (global namespace).

**Phase 2 — Drill library (stretch):**
8. Create `drill_template` table.
9. Add startup seeding logic for default drills.
10. Build drill editor (single-group variant of timeline editor).
11. Replace hardcoded template buttons in palette with DB-backed
    library dropdown/modal.
12. Build library management page.
