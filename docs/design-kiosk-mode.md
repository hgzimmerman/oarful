# Kiosk Mode — Design Document

## Overview

Two kiosk surfaces sharing common infrastructure:

1. **Laptop kiosk** (priority) — a team laptop at the boathouse where
   rowers check in and view lineups. Honor-system: anyone at the laptop
   can mark anyone present/absent.

2. **TV kiosk** — a wall-mounted display showing upcoming practices,
   lineups, and a check-in QR code. Auto-transitions between views,
   high-contrast display-optimized layout.

Both are tenant-scoped (all teams), receive live updates via SSE, and
share the QR check-in flow.

---

## Kiosk Tokens

### Model

```
kiosk_token (master DB)
  id            INTEGER PRIMARY KEY
  tenant_id     INTEGER NOT NULL
  name          TEXT NOT NULL          -- coach-assigned label, e.g. "Boathouse TV"
  token_hash    TEXT NOT NULL UNIQUE   -- argon2 hash of the token
  mode          TEXT NOT NULL          -- "laptop" | "tv"
  created_at    TEXT NOT NULL
  last_used_at  TEXT                   -- updated on each page load (not SSE tick)
  revoked_at    TEXT                   -- soft-revoke; NULL = active
```

### Lifecycle

1. **Generate** — Coach/PD visits `/admin/kiosk`, fills in a name and
   mode (laptop or TV), submits. Server generates a random token
   (e.g. 32-byte base64url), stores its hash, returns the full URL
   once: `/kiosk/activate/{raw_token}`.

2. **Activate** — Opening the URL consumes the token: server verifies
   the hash, sets a long-lived `kiosk_session` cookie (HttpOnly,
   SameSite=Strict) containing `{ tenant_id, kiosk_token_id, mode }`.
   The raw token is single-use — reloading the activation URL after
   cookie is set returns a "already activated" message; if cookies were
   cleared, it returns "token already consumed, generate a new one".

3. **Session** — All `/kiosk/*` routes check the cookie. No user
   login required. The cookie carries tenant context, so SSE and
   page endpoints don't need per-tenant URL paths.

4. **Revoke** — Coach/PD clicks "Revoke" on `/admin/kiosk`. Sets
   `revoked_at`. Next request with that cookie gets a 401 and a
   "session revoked" message.

5. **Management UI** — `/admin/kiosk` (Coach+ gated) lists all tokens
   for the tenant: name, mode, created date, last used date, status.
   Actions: generate new, revoke existing. Last-used-at is rendered
   on page load (not live).

---

## QR Check-in

### QR Content

A single static URL per tenant: `https://{host}/checkin?t={tenant_slug}`

This URL is stable — it doesn't change per practice. The QR code is
generated server-side (e.g. `qrcode` crate) and embedded in both kiosk
views.

### Check-in Flow

1. Rower scans QR on their phone (or taps the link in a lineup email).
2. If not logged in: redirect to login, with a `?next=/checkin` param
   (or `?next=/checkin/{practice_id}`) so the flow resumes after
   authentication.
3. On `/checkin` (authenticated): server finds the next practice for
   any of the rower's teams within a configurable window (default 3
   hours). If found, marks the rower as present (creates a `presence`
   record) and renders the practice view with their seat highlighted.
   If no upcoming practice, shows a "no upcoming practice" message.
4. On `/checkin/{practice_id}` (authenticated, from email link): server
   checks the practice is within the check-in window. If so, marks
   present and shows the practice view. If outside the window, shows
   the practice view without checking in (with a message like
   "Check-in opens 3 hours before practice").
5. The presence record fires an SSE event to connected kiosk clients.

### Check-in Link in Lineup Emails

Lineup notification emails already go to placed rowers. Each email
includes a "I'll be there" link pointing to
`/checkin/{practice_id}`. The link works like the QR flow: requires
login (with redirect-after-login), checks the time window, and
registers presence. This gives rowers a one-tap check-in from their
inbox without needing to scan a QR code.

The link text in the email should include the check-in window so
rowers know when it becomes active, e.g.:

> "I'll be there" — check in opens at 3:30 AM (3 hours before practice)

The link is always present in the email. If tapped too early, the
rower sees the practice view with a note that check-in isn't open yet.
No error — just informational.

### What the Rower Sees (Phone)

Full practice view (condensed): all boats with seat assignments. The
rower's own seat (or bench position) is highlighted. Oar assignments
visible per boat. Practice plan/notes shown below.

---

## Presence Model

```
presence (tenant DB)
  id            INTEGER PRIMARY KEY
  practice_id   INTEGER NOT NULL REFERENCES practice(id)
  rower_id      INTEGER NOT NULL REFERENCES rower(id)
  checked_in_at TEXT NOT NULL
  source        TEXT NOT NULL          -- "qr" | "laptop"

  UNIQUE(practice_id, rower_id)
```

- Toggle semantics: inserting when a record exists deletes it (for the
  laptop tap-to-toggle flow).
- `source` tracks how the check-in happened.
- No user authentication required for laptop-originated check-ins
  (honor system).

### Laptop Check-in Interaction

The laptop kiosk shows the lineup with rower names. Each rower row is
tappable to toggle present/absent. A search bar at the top filters the
displayed rowers by name — useful for large rosters. The search overlays
the lineup view and shows a flat filtered list of rowers with
present/absent toggle.

---

## SSE

### Endpoint

`GET /kiosk/events`

Authenticated via the kiosk session cookie. The cookie carries the
tenant ID, so no per-tenant path is needed.

### Event Types

```
event: checkin
data: { "practice_id": 42, "rower_id": 7, "present": true }

event: lineup
data: { "practice_id": 42 }

event: practice
data: { "action": "created" | "updated" | "deleted" }
```

- `checkin` — a rower checked in or was toggled. Kiosk re-renders
  the affected rower row (HTMX SSE swap on a targeted element).
- `lineup` — a lineup was committed or updated. Kiosk re-fetches
  the lineup for that practice.
- `practice` — a practice was created, updated, or deleted. Kiosk
  re-fetches the schedule view.

### Broadcasting

Server-side: a per-tenant broadcast channel (`tokio::sync::broadcast`).
Handlers that modify presence, lineups, or practices send events to the
channel. The SSE endpoint subscribes to the channel and forwards events.

---

## TV Kiosk Views

### Layout

High-contrast design: darker background, larger fonts than the main app,
but keeping information density high. Not full warm-paper — optimized
for readability at distance on a mounted screen.

### View: Upcoming Practices (Default)

Shows practices within the next 24 hours, all teams. If multiple
practices are co-incident (overlapping times), display them
side-by-side in a grid.

Each practice card shows:
- Team name, practice time, duration
- Lineup: condensed table per boat (seat number, rower name,
  side preference indicator, presence checkmark)
- Oar assignment per boat (see Oar Inventory below)
- Check-in QR code (static, same across all cards)

If lineups don't fit vertically, auto-scroll: slow CSS animation
scrolling the lineup section, pausing at top and bottom. Each practice
card scrolls independently.

### View: No Upcoming Practices

If nothing is scheduled in the next 24 hours, fall back to a weekly
schedule view showing practice times for the rest of the week. If
nothing at all, show a "No scheduled practices" message with the
team/tenant name.

### View Transitions

The TV auto-refreshes content via SSE. When practices enter/leave the
24-hour window (checked via a client-side timer, e.g. every 5 minutes),
the view transitions accordingly.

---

## Laptop Kiosk View

### Layout

Similar to TV but interactive. Shows the next upcoming practice(s).
Rower rows are tappable for presence toggle.

### Components

- **Practice card(s)** — same condensed lineup as TV, but with
  tap-to-toggle presence on each rower row.
- **Search bar** — fixed at top. Typing filters rowers across all
  displayed practices. Filtered view shows a flat list with rower
  name + team + present/absent toggle.
- **QR code** — displayed prominently (e.g. sidebar or top corner).

---

## Admin UI

### `/admin/kiosk` (Coach+ gated)

- **Token list** — table of all kiosk tokens: name, mode (laptop/TV),
  created date, last used (relative time), status (active/revoked).
- **Generate** — form with name (text) + mode (radio: laptop/TV).
  On submit, shows the activation URL once (with a "copy" button and
  a warning that it won't be shown again).
- **Revoke** — button per token row, with confirmation modal.

---

## Oar Inventory (Prerequisite — Separate Feature)

See [docs/design-oar-inventory.md](design-oar-inventory.md) for the
full oar inventory design. Summary of what the kiosk needs:

- Oar sets are assigned to boats per practice.
- The kiosk displays the oar set name next to each boat in the lineup.
- Rowers see which oars to bring to the dock.

---

## Implementation Phases

### Phase 1: Foundation
- `presence` table + migration
- `kiosk_token` table + migration (master DB)
- Kiosk token CRUD endpoints + `/admin/kiosk` page
- Token activation flow + cookie session
- QR code generation (static per-tenant URL)

### Phase 2: Laptop Kiosk (Priority)
- `/kiosk/laptop` page: condensed lineup view for upcoming practices
- Tap-to-toggle presence on rower rows (HTMX swap)
- Search/filter bar
- QR code display
- SSE endpoint + broadcast channel
- Check-in flow on rower's phone (`/checkin`)

### Phase 3: TV Kiosk
- `/kiosk/tv` page: high-contrast display layout
- Side-by-side co-incident practices
- Auto-scroll for overflowing lineups
- No-practice fallback (weekly schedule / empty message)
- Client-side timer for view transitions

### Phase 4: Polish (Post-MVP)
- Live attendance indicators (green dot / checkmark on checked-in
  rowers) visible to coaches on the main practice views
- Presence data feeding into stale-lineup / missing-rower detection
- Oar set display on kiosk views (depends on oar inventory feature)

---

## Open Questions

- **Check-in window**: default 3 hours before practice. Should this be
  configurable per tenant/team, or is a fixed window fine for MVP?
- **Multiple practices**: if a rower has overlapping practices on
  different teams, does QR check-in mark them present for all, or
  prompt them to choose?
- **Kiosk sleep/wake**: should the TV kiosk dim after hours of
  inactivity, or always stay on? (Probably a browser/OS concern, not
  app-level.)
