# Preset Management Redesign

## Problem

The current preset system has four hardcoded built-ins and supports custom
profiles via a modal editor embedded in the lineup view. As teams mature:

- They outgrow the stock defaults or find them irrelevant.
- They want a larger repertoire of presets tailored to their program.
- They want a sensible *team* default rather than always starting from
  "balanced".
- Admins don't want yet another thing to manage at fine detail — the system
  should work well with zero configuration but allow depth when wanted.

Different presets suit different stages of team data maturity. A freshly
onboarded team may only have sidedness and basic roster info, so
performance-focused presets work fine. A team that has invested in entering
pair affinities, seat/zone preferences, and social data gets more out of
presets that lean heavily on those inputs. Today there's no built-in preset
that says "honor people's stated preferences above all else."

## Current State

| Aspect | Today |
|--------|-------|
| Built-in presets | 4 hardcoded: balanced, even_speed, tiered, random |
| Custom presets | Per-team DB rows (`solver_profile` table, keyed on `team_id`) |
| Creation | Clone a built-in or custom preset via modal |
| Editing | Modal launched from preset bar in lineup view |
| Deletion | Custom only; built-ins cannot be removed |
| Default | Always "balanced" unless URL param says otherwise |
| UI surface | Modal in lineup view only |

## Proposed Changes

### 1. New Built-in Preset: "Preferences"

A new hardcoded preset optimized for teams with rich preference data. It
prioritizes rower-stated preferences — who they want to row with, where
they want to sit, what side they prefer — over performance optimization.

**Weight profile (draft):**

| Weight | Balanced | Preferences | Rationale |
|--------|----------|-------------|-----------|
| pair_affinity | 4 | 7 | Honor stated pair preferences strongly |
| seat_affinity | 5 | 7 | Respect zone/seat preferences |
| side_preference | 2 | 4 | Sidedness matters more here |
| skill_variance | 1 | 0 | Don't care about even speed |
| pair_strength | 1 | 0 | Not optimizing for power pairs |
| bow_pair_strength | 2 | 0 | Same |
| height_balance | 1 | 0 | Cosmetic, deprioritize |
| end_pair_skill | 1 | 0 | Not optimizing boat structure |
| engine_room_strength | 1 | 0 | Same |
| top_boat_stacking | 0 | 0 | No stacking |
| placement_reward | 4 | 5 | Everyone rows |
| minimize_bench | 4 | 5 | Minimize sitting out |
| cox_cooldown_penalty | 5 | 5 | Keep cox rotation |
| weight_class_slack | 3 | 2 | Light touch on weight class |
| (others) | (balanced defaults) | (balanced defaults) | |

**Description:** "Prioritize rower preferences — pair partners, favorite
seats, and side. Best for teams that have entered affinity and zone data."

Always visible in the preset bar. The description does the work of
explaining when it's useful. Hiding it behind data thresholds adds a
discovery problem — coaches won't know it exists until they've already
done the data entry.

### 2. Dedicated Preset Management Page

Add a page under settings for full preset CRUD. The lineup-view preset bar
stays for *selection* and quick-edit via the existing modal.

**Where it lives:** A section in the settings rail, e.g. `/settings/presets`.

**Page layout:**
- List of all presets (built-in + custom) with name, description, and
  built-in badge.
- Click to expand or navigate to a detail/edit view with all weights
  grouped by category.
- Actions: edit, duplicate, delete, set as default, rename.
- Team visibility grid (see section 4).

**Relationship to the lineup-view modal:**
The modal stays for practice-day quick edits. It could be trimmed — maybe
drop delete/clone from the modal and keep it focused on weight adjustment.
The modal's weight-group layout is the core of what the management page's
editor looks like too. Ideally extract a shared component; if that's not
feasible, duplicate with consistent look and feel.

The modal should link to the management page ("Manage all presets") for
when a coach wants to do more than a quick tweak.

### 3. Team Default Preset

Add a `default_preset` column to the `team` table (nullable text). When
set, new lineup solves start with this preset instead of "balanced".

- Fallback chain: team default -> first preset in the team's visible
  list (alphabetical) -> "balanced" -> `SolverConfig::default()`.
- If a team's default preset is deleted, it silently falls back to the
  first available preset. No error state.
- Settable from the preset management page ("Set as default" action).
- Shown with a visual indicator (e.g. a small star) in both the
  management page and the preset bar.
- A fresh team with no default set gets "balanced" as today — zero
  configuration required.

### 4. Program-Wide Presets with Per-Team Visibility

Move presets from per-team to per-program (tenant-wide) scope with a
join table controlling which teams see which presets.

**Schema change:**
- Remove `team_id` from `solver_profile`. Presets become program-wide
  resources with a unique constraint on `name` alone.
- Add `team_preset_visibility` join table, analogous to
  `team_boat_default`:

  ```
  team_preset_visibility (team_id, solver_profile_id)
      team_id -> INTEGER REFERENCES team(id)
      solver_profile_id -> INTEGER REFERENCES solver_profile(id)
  ```

**Behavior:**
- When a preset is created, all existing teams are attached by default.
  (Opposite of boat defaults, which start empty.)
- A newly created team does NOT auto-bind to all existing presets.
  Instead, if a team has zero visibility rows, it sees all presets
  (implicit "show everything" default). This means:
  - Fresh teams see everything with no admin action.
  - Once an admin explicitly manages a team's visibility (adding or
    removing any preset), the team switches to explicit mode.
  - At least one preset is always visible — the UI prevents removing the
    last one (or the fallback "show all" kicks in if all rows are deleted).
- The management page shows a teams-by-presets grid (like the boat
  defaults grid) for admins who want per-team control.

**Built-in presets:** Still hardcoded in `SolverConfig` as templates.
They don't live in the DB and don't participate in the visibility table.
They're always visible to all teams. The visibility grid only applies to
custom presets.

(Future option: materialize built-ins into the DB so teams can hide or
edit them. Not required for v1 — the current "clone to customize" flow
is adequate.)

## Migration Path

1. Add "preferences" preset to `SolverConfig` and `BUILTIN_NAMES`.
2. Drop `team_id` from `solver_profile`, add unique constraint on `name`.
   Migrate existing rows (if two teams have profiles with the same name,
   suffix with team name).
3. Create `team_preset_visibility` table.
4. Populate visibility rows for existing custom profiles (all teams
   attached).
5. Add `default_preset TEXT` column to `team`.
6. Build management page with weight editor, team visibility grid, and
   set-as-default action.
7. Update `SolveKnobs` resolution to check team default, then fall back
   to first visible preset.

