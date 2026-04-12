# Demo Mode — Limitations

The demo gives prospective users a fully interactive preview of the
lineup generator. Everything works except the features listed below.

## What works

- Full solver with all constraint profiles and knobs
- Manual lineup builder (drag rowers, lock seats, swap boats)
- Practices: create, cancel, view availability
- Fleet management: add/edit/relinquish boats
- Roster: view rowers, edit attributes, seat and pair affinities
- History: view committed lineups, add practice notes
- Solver presets and custom profiles
- All role-gated views (demo user is a Program Director)

## What doesn't work

### Email

All email functionality is disabled. The demo uses a logging-only
mailer — no emails are actually sent. This affects:

- **Availability reminders** — the send button works but emails
  go to the server log, not to inboxes.
- **Lineup notifications** — same: the flow completes but nothing
  is delivered.
- **Magic-link sign-in** — not available for demo accounts.
- **User invites** — invite links are generated but the email
  isn't delivered. The link is still visible in the UI.

### Google Sheets sync

The sync page requires a real Google Sheet URL. Demo tenants have
no sheet configured, so sync will fail. Rowers are pre-loaded via
the fixture seeder instead.

### Multi-user

The demo runs as a single user (Demo Coach, Program Director).
There are no other user accounts to test:

- Role-based access differences (Member vs Coach vs PD)
- Self-service availability from a rower's perspective
- The `/my/profile` and `/my/availability` flows as a linked rower

### Account management

- **Password** — the demo account has no password. Access is via
  a direct JWT issued at demo creation, or the "Resume demo"
  button on the login page.
- **Logout** — logging out is recoverable via "Resume demo" (a
  cookie tracks the demo tenant). Clearing cookies loses access.
- **Email preferences** — the toggles save but have no observable
  effect since emails aren't sent.

### Data persistence

- Demo tenants expire after **7 days**. All data (rowers, boats,
  lineups, practices, notes) is permanently deleted.
- There is no export or backup for demo tenants.
- Creating a new demo after expiry starts fresh — no data carries
  over.

**Garbage collection.** A background cleanup task runs at server
startup and then every hour. It queries the master DB for tenants
where `demo_expires_at < now`, then for each expired tenant:
1. Deletes the tenant's SQLite file from disk.
2. Evicts the tenant from the in-memory connection pool cache.
3. Deletes the tenant row from the master DB.

### Billing and onboarding

- No payment flow exists in the demo (or anywhere yet).
- There is no self-service path from demo to paid tenant. A
  prospective customer would need to contact us to set up a
  real account.

## Future improvements

- Dismissable in-app banner explaining these limitations on first
  demo page load.
- Landing page with feature overview and demo CTA that sets
  expectations before entry.
- "Convert to real account" flow: keep the demo's data, set a
  password, connect billing.
