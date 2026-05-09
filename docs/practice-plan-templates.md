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
    name            TEXT NOT NULL UNIQUE
    description     TEXT NOT NULL DEFAULT ''
    category        TEXT          -- nullable, free-form label
    timeline_json   TEXT NOT NULL  -- same format as practice.timeline_json
    created_at      TEXT NOT NULL
    updated_at      TEXT NOT NULL
```

Tenant-wide (no team_id). No per-team visibility for v1 — all teams see
all templates.

**Creating a template:**
- Dedicated management page at `/settings/practice-templates` (or similar
  path in the settings rail).
- Uses the same timeline editor (strip, palette, segment editors) as the
  practice detail page, but detached from a specific practice.
- The editor works on the template's `timeline_json` directly.

**Importing into a practice:**
- From the practice detail page, a coach can import a template.
- Import **replaces** the practice's current timeline entirely (with
  confirmation if one already exists).
- The imported timeline is a snapshot copy — subsequent edits to the
  template don't affect practices that already imported it, and edits
  to the practice plan don't affect the template.

**Management page layout:**
- List of all templates with name, description, category badge, and
  approximate duration.
- Filter/search by name and category.
- Click to open the template in the timeline editor.
- Actions: edit, duplicate, delete, rename.

**Categories:**
- Single optional label per template (nullable `category` column).
- Programs create their own categories organically by typing a label.
- UI shows existing categories as filter pills / autocomplete suggestions
  when assigning a category.
- Most programs won't use categories — the UI should work fine with
  everything uncategorized (just a flat searchable list).

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
    name            TEXT NOT NULL UNIQUE
    description     TEXT NOT NULL DEFAULT ''
    category        TEXT          -- nullable, same free-form labels
    group_json      TEXT NOT NULL  -- serialized Group (same structure as timeline Group)
    is_default      BOOLEAN NOT NULL DEFAULT FALSE  -- seeded from built-ins
    created_at      TEXT NOT NULL
    updated_at      TEXT NOT NULL
```

Tenant-wide. `group_json` contains a serialized `Group` (with its
segments, repeat, rotation config — everything the current built-in
templates define).

**Seeding defaults:**
- On first use (or migration), the 7 existing built-in templates are
  materialized into `drill_template` rows with `is_default = TRUE`.
- Coaches can delete the defaults. `is_default` flag preserved so a
  "restore defaults" action can re-insert them.
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
- List with name, description, category badge, group type (warmup/piece),
  approximate duration.
- Click to edit in a group editor (reuse the group/segment editor from
  the timeline editor, but standalone).
- Actions: edit, duplicate, delete, rename, restore defaults.

**Naming:**
"Drills and pieces" is accurate but verbose. Options for the UI label:
- "Activities" — generic but short
- "Drills" — covers both informally (coaches call pieces "drills" too)
- "Blocks" — already used internally for bare items (Launch/Rest/Turn/Dock)
- "Library" — focuses on the collection aspect ("Drill library")

Recommendation: **"Library"** as the nav label, with items described as
"drills" and "pieces" in context (badges, group type indicators). The
page title could be "Drill & Piece Library."

## UI Integration Points

**Settings rail:**
- "Practice Templates" — plan template management page
- "Library" (stretch) — drill & piece library

**Practice detail page:**
- "Use template" button/dropdown to import a plan template (replaces
  current timeline with confirmation)
- In the timeline editor palette: library dropdown/modal for inserting
  saved drills/pieces (stretch)

**Timeline editor (shared):**
- Same editor used in three contexts:
  1. Practice detail page (editing a specific practice's plan)
  2. Plan template management page (editing a template)
  3. Drill/piece editor (editing a single group — stretch)
- For context 3, the editor is scoped to a single group rather than a
  full timeline. This may mean a simpler variant that hides the
  launch/dock bookends and the palette.

## Migration Path

**Phase 1 — Plan templates:**
1. Create `practice_plan_template` table.
2. Build management page with timeline editor integration.
3. Add "Use template" import flow to practice detail page.
4. Add category autocomplete.

**Phase 2 — Drill library (stretch):**
5. Create `drill_template` table.
6. Seed default rows from `built_in_templates()`.
7. Remove hardcoded template rendering from palette; replace with
   DB-backed library dropdown/modal.
8. Build library management page with group editor.
