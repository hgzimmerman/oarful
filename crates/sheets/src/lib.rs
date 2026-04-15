//! Google Sheets availability sync.
//!
//! **v1 — public sheets only.** We fetch the spreadsheet via its CSV
//! export URL
//! (`https://docs.google.com/spreadsheets/d/{ID}/export?format=csv&gid={GID}`),
//! which works for any sheet set to "Anyone with the link can view".
//! No authentication needed, no service-account JSON, no OAuth dance.
//!
//! **Future — private sheets.** When the club wants to keep the sheet
//! private, we add a second code path using `google-sheets4` +
//! `yup-oauth2` with a service account. The parser below is already
//! decoupled from the fetching layer (`sync_csv` takes a `&str`), so
//! only the transport changes.
//!
//! # Expected sheet format
//!
//! First 7 columns are rower metadata, followed by one column per
//! practice date:
//!
//! ```text
//! | Sweep/Scull | Last Name | First Name | Pronoun | Email | Can you Scull? | Side/Cox | 3/30 | 4/1 | 4/3 | ...
//! | Sweep       | Smith     | Alice      | she/her | a@... | Yes            | Port     | Att. | ... | ... | ...
//! ```
//!
//! - Date headers are `M/D` format. Year is inferred from the current
//!   calendar year (sync is intended to be run against the active
//!   season).
//! - Cell values: `Attending`, `Not Attending`, or empty. Empty means
//!   "no response yet" — no availability row is upserted.
//! - `Sweep/Scull` column distinguishes team membership. Sweep rows
//!   map attending → `Yes`; sculling rows map attending →
//!   `ScullingOnly` so the sweep solver still sees them in the
//!   snapshot but excludes them from seat assignment.
//! - Rower identity is matched on `Email` — the stable unique key.
//!   Rowers not yet in the DB are auto-created with middle-ground
//!   defaults (Medium weight, Intermediate skill + strength) that an
//!   admin can refine later.
//!
//! # The "promote specificity" rule
//!
//! When a rower already exists in the DB, the sync never demotes
//! coach-set values. Specifically:
//! - Side stays specific: `Port`/`Starboard` in the DB is never
//!   overwritten with `Either` from the sheet.
//! - Flags stay true: `can_cox`, `can_scull`, `is_designated_cox`
//!   are only promoted from false → true.
//!
//! This is enforced by `Rower::promote_from_sheet` in the db crate.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate, Utc};
use diesel::SqliteConnection;
use lineup_db::app_user::{AppUser, NewAppUser, Role};
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::availability::{Availability, NewAvailability};
use lineup_db::rower::types::Side;
use lineup_db::rower::{NewRower, Rower};

/// Count summary returned from a sync run, intended for logging /
/// CLI display so the operator can see what happened.
#[derive(Debug, Clone, Default)]
pub struct SyncSummary {
    /// Data rows read from the sheet (excludes the header).
    pub rows_read: usize,
    /// Rows that were skipped entirely because they lacked an email.
    pub rows_skipped_no_email: usize,
    /// New rower rows inserted because the email wasn't already in the DB.
    pub rowers_created: usize,
    /// Existing rowers whose attributes were promoted by the sync.
    pub rowers_updated: usize,
    /// Availability rows inserted / updated.
    pub availabilities_upserted: usize,
    /// Sweep team rows successfully processed (subset of rows_read).
    pub sweep_rows: usize,
    /// Sculling team rows successfully processed (subset of rows_read).
    pub sculling_rows: usize,
    /// Human-readable warnings about malformed data, unknown values,
    /// etc. Non-fatal — the sync continues when these happen.
    pub warnings: Vec<String>,
}

/// Fetch a publicly-shared Google Sheet by ID and sync its contents
/// to the database. The sheet must be set to "Anyone with the link
/// can view"; authenticated access is future work.
///
/// `gid` is the tab identifier inside the spreadsheet (0 for the
/// first tab, which is the default for most sheets).
pub async fn sync_public_sheet(
    spreadsheet_id: &str,
    gid: u32,
    team_id: lineup_db::team::TeamId,
    conn: &mut SqliteConnection,
) -> Result<SyncSummary> {
    let url = format!(
        "https://docs.google.com/spreadsheets/d/{spreadsheet_id}/export?format=csv&gid={gid}"
    );
    tracing::info!(url = %url, "fetching sheet csv export");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("building reqwest client")?;
    let response = client
        .get(&url)
        .send()
        .await
        .context("requesting sheet csv export")?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "sheet csv export returned HTTP {}: is the sheet set to \
             'Anyone with the link can view'?",
            status
        );
    }
    let csv_text = response
        .text()
        .await
        .context("reading sheet csv body")?;

    let year = Utc::now().year();
    sync_csv(&csv_text, year, team_id, conn)
}

/// Pure parser + upsert logic. Separated from the HTTP fetching so
/// unit tests can feed it a &str and drive the DB layer in isolation.
///
/// Handles the "prelude rows" case where the actual column header row
/// is not the first row of the sheet — the GGRC sheet has a
/// week-grouping row above the real header (`,,,,,,,Session 1 - Week
/// 1,,,Session 1 - Week 2,...`). We scan for the first row whose
/// column 0 matches the expected `Sweep/Scull` marker and treat that
/// as the header. Anything before it is ignored.
pub fn sync_csv(
    csv_text: &str,
    year: i32,
    team_id: lineup_db::team::TeamId,
    conn: &mut SqliteConnection,
) -> Result<SyncSummary> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    // Collect all rows, then locate the header.
    let all_records: Vec<csv::StringRecord> = reader
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("reading sheet rows")?;

    let header_idx = all_records
        .iter()
        .position(|r| {
            r.get(0)
                .map(|c| c.trim().eq_ignore_ascii_case("Sweep/Scull"))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow!(
                "could not find header row (expected first column to be \
                 'Sweep/Scull'). Sheet may be in an unexpected format."
            )
        })?;

    let headers = &all_records[header_idx];
    let date_columns = parse_date_headers(headers, year)?;

    let mut summary = SyncSummary::default();

    for record in &all_records[header_idx + 1..] {
        summary.rows_read += 1;
        match sync_row(record, &date_columns, team_id, conn, &mut summary) {
            Ok(()) => {}
            Err(e) => summary.warnings.push(format!(
                "row {}: {e}",
                summary.rows_read
            )),
        }
    }

    Ok(summary)
}

fn sync_row(
    record: &csv::StringRecord,
    date_columns: &[(usize, NaiveDate)],
    team_id: lineup_db::team::TeamId,
    conn: &mut SqliteConnection,
    summary: &mut SyncSummary,
) -> Result<()> {
    let team = field(record, 0);
    let last_name = field(record, 1);
    let first_name = field(record, 2);
    // field(3) is Pronoun — we don't store it
    let email = field(record, 4);
    let can_scull_text = field(record, 5);
    let side_cox_text = field(record, 6);

    if first_name.is_empty() && last_name.is_empty() {
        return Ok(()); // blank row — silently skip
    }
    if email.is_empty() {
        summary.rows_skipped_no_email += 1;
        summary.warnings.push(format!(
            "row has no email ({first_name} {last_name}), skipping"
        ));
        return Ok(());
    }

    let is_sculling = team.eq_ignore_ascii_case("sculling");
    if is_sculling {
        summary.sculling_rows += 1;
    } else {
        summary.sweep_rows += 1;
    }

    let display_name = if first_name.is_empty() {
        last_name.to_string()
    } else if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };

    let side = parse_side(side_cox_text);
    let is_designated_cox = side_cox_text.eq_ignore_ascii_case("cox");
    // The sheet doesn't carry a "cannot cox" signal; only "is explicitly
    // a designated cox" via the Cox value. Leave the general can_cox
    // flag at its default (true) for new rowers.
    let can_cox = true;
    let sheet_can_scull = can_scull_text.eq_ignore_ascii_case("yes");
    // Scullers can obviously scull, regardless of what the column says.
    let can_scull = is_sculling || sheet_can_scull;

    // Upsert the rower row. Identity is keyed on email via app_user.
    let rower = match AppUser::find_by_email(conn, email)? {
        Some(user) => {
            // User exists — load or create linked rower.
            match user.rower_id {
                Some(rower_id) => {
                    let existing = Rower::get(conn, rower_id)?
                        .ok_or_else(|| anyhow!("app_user.rower_id points to missing rower {rower_id}"))?;
                    let updated = Rower::promote_from_sheet(
                        conn, &existing, &display_name, side, can_scull, can_cox, is_designated_cox,
                    )?;
                    if updated.updated_at != existing.updated_at {
                        summary.rowers_updated += 1;
                    }
                    updated
                }
                None => {
                    // User exists but has no rower — create one and link.
                    let new = NewRower::from_sheet(&display_name, side, can_scull, can_cox, is_designated_cox);
                    let created = Rower::insert(conn, new)?;
                    AppUser::set_rower_id(conn, user.id, Some(created.id))?;
                    summary.rowers_created += 1;
                    created
                }
            }
        }
        None => {
            // No user with this email — create rower + passwordless active user.
            let new = NewRower::from_sheet(&display_name, side, can_scull, can_cox, is_designated_cox);
            let created = Rower::insert(conn, new)?;
            let now = chrono::Utc::now().naive_utc();
            let user = AppUser::create(conn, NewAppUser {
                email: email.to_string(),
                password_hash: None,
                name: display_name.clone(),
                status: "active".to_string(),
                created_at: now,
                updated_at: now,
            })?;
            AppUser::set_role(conn, user.id, Role::Member)?;
            AppUser::set_rower_id(conn, user.id, Some(created.id))?;
            summary.rowers_created += 1;
            created
        }
    };

    // Ensure team membership (idempotent).
    lineup_db::team::TeamMembership::add(conn, team_id, rower.id)?;

    // Upsert per-date availability.
    for (col_idx, date) in date_columns {
        let cell = record.get(*col_idx).unwrap_or("").trim();
        if cell.is_empty() {
            continue;
        }
        let status = match cell {
            "Attending" => {
                if is_sculling {
                    AvailabilityStatus::ScullingOnly
                } else {
                    AvailabilityStatus::Yes
                }
            }
            "Not Attending" => AvailabilityStatus::No,
            other => {
                summary.warnings.push(format!(
                    "unknown status {other:?} for {display_name} on {date}"
                ));
                continue;
            }
        };
        // Ensure practice exists for this date so we can key availability on it.
        let practice = lineup_db::practice::Practice::upsert(conn, team_id, *date, None, None)?;
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id: rower.id,
                practice_id: practice.id,
                status,
            },
        )?;
        summary.availabilities_upserted += 1;
    }

    Ok(())
}

/// Pull the date columns (index, parsed date) out of the header row,
/// assuming the first 7 columns are the known metadata columns and
/// everything after that is an `M/D` date.
fn parse_date_headers(
    headers: &csv::StringRecord,
    year: i32,
) -> Result<Vec<(usize, NaiveDate)>> {
    const EXPECTED_METADATA: &[&str] = &[
        "Sweep/Scull",
        "Last Name",
        "First Name",
        "Pronoun",
        "Email",
        "Can you Scull?",
        "Side/Cox",
    ];

    for (i, expected) in EXPECTED_METADATA.iter().enumerate() {
        let got = headers.get(i).unwrap_or("").trim();
        if !got.eq_ignore_ascii_case(expected) {
            bail!(
                "unexpected header at column {i}: got {got:?}, expected {expected:?}"
            );
        }
    }

    let mut dates = Vec::new();
    for idx in EXPECTED_METADATA.len()..headers.len() {
        let raw = headers.get(idx).unwrap_or("").trim();
        if raw.is_empty() {
            continue;
        }
        let date = parse_month_day(raw, year)
            .with_context(|| format!("parsing date header column {idx}: {raw:?}"))?;
        dates.push((idx, date));
    }
    Ok(dates)
}

/// Parse an `M/D` header into a `NaiveDate` using the supplied year.
fn parse_month_day(s: &str, year: i32) -> Result<NaiveDate> {
    let (m, d) = s
        .split_once('/')
        .ok_or_else(|| anyhow!("expected M/D format, got {s:?}"))?;
    let month: u32 = m
        .trim()
        .parse()
        .with_context(|| format!("invalid month in {s:?}"))?;
    let day: u32 = d
        .trim()
        .parse()
        .with_context(|| format!("invalid day in {s:?}"))?;
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow!("invalid date: {year}-{month:02}-{day:02}"))
}

fn parse_side(s: &str) -> Side {
    match s.trim().to_ascii_lowercase().as_str() {
        "port" => Side::Port,
        "starboard" => Side::Starboard,
        // "Both", "Cox", "" and anything unrecognised all fall through
        // to Either. The Cox case is separately signalled via the
        // is_designated_cox flag so we don't need a special Side here.
        _ => Side::Either,
    }
}

fn field<'a>(record: &'a csv::StringRecord, idx: usize) -> &'a str {
    record.get(idx).unwrap_or("").trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::prelude::*;
    use lineup_db::app_user::{AppUser, NewAppUser};
    use lineup_db::availability::types::AvailabilityStatus;
    use lineup_db::availability::Availability;
    use lineup_db::rower::types::{RowerWeightClass, Side, SideStrength, Skill, Strength};
    use lineup_db::rower::{NewRower, Rower};
    use lineup_db::schema::availability as availability_schema;
    use lineup_db::team::{NewTeam, Team, TeamId};
    use lineup_db::test_support::in_memory_conn;
    use lineup_db::types::IntBool;

    /// Standard header the real GGRC sheet uses. Tests can prepend
    /// rower rows and a trailing date column range on top of this.
    const HEADER: &str = "Sweep/Scull,Last Name,First Name,Pronoun,Email,Can you Scull?,Side/Cox";

    fn header_with_dates(dates: &[&str]) -> String {
        format!("{HEADER},{}", dates.join(","))
    }

    /// Year used by every test so date parsing is stable.
    const YEAR: i32 = 2026;

    /// Seed a team for tests. Returns its id.
    fn seed_team(conn: &mut SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(conn, NewTeam { name: "Test".into(), created_at: now })
            .expect("seed team")
            .id
    }

    /// Fetch the rowers table ordered by id so assertions can rely
    /// on insertion order.
    fn all_rowers(conn: &mut SqliteConnection) -> Vec<Rower> {
        use lineup_db::schema::rower::dsl::*;
        rower.order(id.asc()).select(Rower::as_select()).load(conn).unwrap()
    }

    fn all_availabilities(conn: &mut SqliteConnection) -> Vec<Availability> {
        availability_schema::table
            .order((availability_schema::rower_id.asc(), availability_schema::practice_id.asc()))
            .select(Availability::as_select())
            .load(conn)
            .unwrap()
    }

    #[test]
    fn happy_path_creates_rowers_and_availabilities() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending,Not Attending\n\
             Sweep,Jones,Bob,he/him,bob@example.com,No,Starboard,Attending,Attending\n",
            header_with_dates(&["4/11", "4/13"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 2);
        assert_eq!(summary.rowers_created, 2);
        assert_eq!(summary.rowers_updated, 0);
        assert_eq!(summary.availabilities_upserted, 4);
        assert_eq!(summary.sweep_rows, 2);
        assert_eq!(summary.sculling_rows, 0);
        assert!(summary.warnings.is_empty(), "warnings: {:?}", summary.warnings);

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 2);
        assert_eq!(rowers[0].name, "Alice Smith");
        assert_eq!(rowers[0].side, Side::Port);
        assert_eq!(rowers[1].name, "Bob Jones");
        assert_eq!(rowers[1].side, Side::Starboard);
        // Email now lives on app_user, not rower.
        let alice_user = AppUser::find_by_email(&mut conn, "alice@example.com").unwrap().expect("alice user");
        assert_eq!(alice_user.rower_id, Some(rowers[0].id));

        let avails = all_availabilities(&mut conn);
        assert_eq!(avails.len(), 4);
        // Alice 4/11 Attending, 4/13 Not Attending
        assert_eq!(avails[0].status, AvailabilityStatus::Yes);
        assert_eq!(avails[1].status, AvailabilityStatus::No);
        // Bob both Attending
        assert_eq!(avails[2].status, AvailabilityStatus::Yes);
        assert_eq!(avails[3].status, AvailabilityStatus::Yes);
    }

    #[test]
    fn prelude_row_above_header_is_skipped() {
        // Real sheet has a week-grouping row above the actual header
        // (`,,,,,,,Session 1 - Week 1,,,Session 1 - Week 2,...`).
        // The parser scans for "Sweep/Scull" and treats whatever row
        // matches as the header.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            ",,,,,,,Session 1 - Week 1,Session 1 - Week 2\n\
             {}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending,Attending\n",
            header_with_dates(&["4/11", "4/13"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 1);
        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 2);
        assert!(summary.warnings.is_empty(), "warnings: {:?}", summary.warnings);
    }

    #[test]
    fn sculling_row_maps_attending_to_scullingonly() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sculling,Scully,Nico,they/them,nico@example.com,Yes,Port,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 1);
        assert_eq!(summary.sculling_rows, 1);
        assert_eq!(summary.sweep_rows, 0);
        assert_eq!(summary.rowers_created, 1);

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        // Scullers always get can_scull = true, even when "Can you
        // Scull?" would have said No — the sheet's team column is
        // authoritative on this.
        assert_eq!(rowers[0].can_scull, IntBool::TRUE);

        let avails = all_availabilities(&mut conn);
        assert_eq!(avails.len(), 1);
        assert_eq!(avails[0].status, AvailabilityStatus::ScullingOnly);
    }

    #[test]
    fn row_with_no_email_is_skipped_with_warning() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,NoEmail,Ghost,they/them,,Yes,Port,Attending\n\
             Sweep,Real,Alice,she/her,alice@example.com,Yes,Port,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 2);
        assert_eq!(summary.rows_skipped_no_email, 1);
        assert_eq!(summary.rowers_created, 1); // only Alice
        assert_eq!(summary.availabilities_upserted, 1);
        assert_eq!(summary.warnings.len(), 1);
        assert!(summary.warnings[0].contains("no email"));
    }

    #[test]
    fn unknown_status_cell_warns_but_keeps_going() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Maybe\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 0); // Maybe isn't upserted
        assert_eq!(summary.warnings.len(), 1);
        assert!(
            summary.warnings[0].contains("Maybe"),
            "warning should mention the unrecognised status: {:?}",
            summary.warnings[0]
        );
    }

    #[test]
    fn empty_status_cell_is_silently_ignored() {
        // "no response yet" — the parser should not upsert anything
        // and should not produce a warning.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,,Attending\n",
            header_with_dates(&["4/11", "4/13"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 1);
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn promote_specificity_never_demotes_side() {
        // Existing rower has Side::Starboard set by a coach. The
        // sheet's Side/Cox value of "Both" (which parses to Either)
        // must NOT overwrite the specific side.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);

        // Seed a rower with a specific side directly via the db layer.
        let seeded = Rower::insert(
            &mut conn,
            NewRower {
                name: "Alice Smith".into(),
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Expert,
                strength: Strength::Strong,
                height: lineup_db::rower::types::Height::Tall,
                side: Side::Starboard,
                side_strength: SideStrength::default(),
                can_scull: IntBool::FALSE,
                can_cox: IntBool::TRUE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
            },
        )
        .unwrap();
        assert_eq!(seeded.side, Side::Starboard);

        // Create an app_user linked to this rower (email is the identity key).
        let now = chrono::Utc::now().naive_utc();
        let user = AppUser::create(&mut conn, NewAppUser {
            email: "alice@example.com".into(),
            password_hash: None,
            name: "Alice Smith".into(),
            status: "active".into(),
            created_at: now,
            updated_at: now,
        }).unwrap();
        AppUser::set_rower_id(&mut conn, user.id, Some(seeded.id)).unwrap();

        // Now re-import from a sheet that says "Both" for this rower.
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,No,Both,Attending\n",
            header_with_dates(&["4/11"]),
        );
        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();
        assert_eq!(summary.rowers_created, 0);

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        // The key assertion: Starboard survived the sync.
        assert_eq!(rowers[0].side, Side::Starboard);
    }

    #[test]
    fn malformed_date_header_fails_fast() {
        // A header column that doesn't look like `M/D` should
        // produce a hard error rather than silently dropping the
        // column — the sheet format changed and the operator should
        // know before running a bad import.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n",
            header_with_dates(&["not-a-date"]),
        );

        let err = sync_csv(&csv, YEAR, tid, &mut conn).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not-a-date") || msg.contains("M/D"),
            "error should mention the bad header: {msg}"
        );
    }

    #[test]
    fn missing_sweep_scull_header_is_an_error() {
        // If the first column isn't "Sweep/Scull" anywhere in the
        // file, we can't find the header row and should error.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "Team,Last,First,Pronoun,Email,Scull,Side,4/11\n\
                   Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n";

        let err = sync_csv(csv, YEAR, tid, &mut conn).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("header"),
            "error should mention header row: {msg}"
        );
    }

    #[test]
    fn blank_rows_are_silently_skipped() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n\
             ,,,,,,,\n\
             Sweep,Jones,Bob,he/him,bob@example.com,No,Starboard,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, &mut conn).unwrap();

        // The blank row is counted in rows_read but produces no
        // warning, no rower, and no availability.
        assert_eq!(summary.rows_read, 3);
        assert_eq!(summary.rowers_created, 2);
        assert_eq!(summary.availabilities_upserted, 2);
        assert!(summary.warnings.is_empty(), "warnings: {:?}", summary.warnings);
    }
}

