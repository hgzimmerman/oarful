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
//! Columns are matched by case-insensitive header name and can appear
//! in any order. Required columns:
//!
//! - **Sweep/Scull** — also serves as the header-row marker
//! - **Last Name** — rower surname
//! - **First Name** — rower given name
//! - **Email** — identity key for matching rowers to app users
//! - **Side/Cox** — `Port`, `Starboard`, `Either`, or `Cox`
//!
//! Optional / ignored columns: `Pronoun`, `Can you Scull?`, and any
//! other unrecognized column names.
//!
//! Any column whose header matches `M/D` format is treated as a
//! practice date (e.g. `3/30`, `11/5`). Year is inferred from the
//! current calendar year.
//!
//! ```text
//! | Sweep/Scull | Last Name | First Name | Email | Side/Cox | 3/30 | 4/1 | ...
//! | Sweep       | Smith     | Alice      | a@... | Port     | Att. | ... | ...
//! ```
//!
//! - Cell values: `Attending`, `Not Attending`, or empty. Empty means
//!   "no response yet" — no availability row is upserted.
//! - `Sweep/Scull` column distinguishes team membership. Sweep rows
//!   map attending → `Yes`; sculling rows map attending →
//!   `Yes` (sweep_bias on the rower handles the scull distinction).
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
//! - Flags stay true: `can_cox`, `is_designated_cox`
//!   are only promoted from false → true.
//! - `sweep_bias` is only overridden for sculling rows (set to SCULL_HARD).
//!
//! This is enforced by `Rower::promote_from_sheet` in the db crate.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate, Utc};
use diesel::SqliteConnection;
use lineup_db::app_user::{AppUser, NewAppUser, Role, UserStatus};
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::availability::{Availability, NewAvailability};
use lineup_db::rower::types::{Side, SweepBias};
use lineup_db::rower::{NewRower, Rower};

/// Controls which rows from the spreadsheet are imported based on
/// the Sweep/Scull column. Allows two teams to share the same
/// spreadsheet with different sync sources — one importing sweep
/// rowers, the other importing scullers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum RowFilter {
    /// Import all rows regardless of Sweep/Scull value.
    #[default]
    All,
    /// Import only rows where column 0 is "Sweep" (or anything other than "Sculling").
    Sweep,
    /// Import only rows where column 0 is "Sculling".
    Sculling,
}

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
    row_filter: RowFilter,
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
    let csv_text = response.text().await.context("reading sheet csv body")?;

    let year = Utc::now().year();
    sync_csv(&csv_text, year, team_id, row_filter, conn)
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
    row_filter: RowFilter,
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
            r.iter()
                .any(|c| c.trim().eq_ignore_ascii_case("Sweep/Scull"))
        })
        .ok_or_else(|| {
            anyhow!(
                "could not find header row (expected a column named \
                 'Sweep/Scull'). Sheet may be in an unexpected format."
            )
        })?;

    let headers = &all_records[header_idx];
    let col_map = ColumnMap::from_headers(headers, year)?;

    let mut summary = SyncSummary::default();

    // Load team defaults so sync-created practices get the configured time.
    let team = lineup_db::team::Team::get(conn, team_id)?;
    let default_time = team.as_ref().and_then(|t| t.default_practice_time);
    let default_duration = team
        .as_ref()
        .and_then(|t| t.default_practice_duration_minutes);

    for record in &all_records[header_idx + 1..] {
        summary.rows_read += 1;
        match sync_row(
            record,
            &col_map,
            team_id,
            row_filter,
            default_time,
            default_duration,
            conn,
            &mut summary,
        ) {
            Ok(()) => {}
            Err(e) => summary
                .warnings
                .push(format!("row {}: {e}", summary.rows_read)),
        }
    }

    Ok(summary)
}

fn sync_row(
    record: &csv::StringRecord,
    col_map: &ColumnMap,
    team_id: lineup_db::team::TeamId,
    row_filter: RowFilter,
    default_time: Option<chrono::NaiveTime>,
    default_duration: Option<lineup_db::types::DurationMinutes>,
    conn: &mut SqliteConnection,
    summary: &mut SyncSummary,
) -> Result<()> {
    let team = field(record, col_map.sweep_scull);
    let last_name = field(record, col_map.last_name);
    let first_name = field(record, col_map.first_name);
    let email = field(record, col_map.email);
    let side_cox_text = field(record, col_map.side_cox);

    if first_name.is_empty() && last_name.is_empty() {
        return Ok(()); // blank row — silently skip
    }

    let is_sculling = team.eq_ignore_ascii_case("sculling");

    // Apply row filter — skip rows that don't match the configured filter.
    match row_filter {
        RowFilter::All => {}
        RowFilter::Sweep if is_sculling => return Ok(()),
        RowFilter::Sculling if !is_sculling => return Ok(()),
        _ => {}
    }

    if email.is_empty() {
        summary.rows_skipped_no_email += 1;
        summary.warnings.push(format!(
            "row has no email ({first_name} {last_name}), skipping"
        ));
        return Ok(());
    }

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
    let first_opt = if first_name.is_empty() {
        None
    } else {
        Some(first_name.to_string())
    };
    let last_opt = if last_name.is_empty() {
        None
    } else {
        Some(last_name.to_string())
    };

    let side = parse_side(side_cox_text);
    let is_designated_cox = side_cox_text.eq_ignore_ascii_case("cox");
    // The sheet doesn't carry a "cannot cox" signal; only "is explicitly
    // a designated cox" via the Cox value. Leave the general can_cox
    // flag at its default (true) for new rowers.
    let can_cox = true;
    // Read the "Can you Scull?" column if present.
    let can_scull_answer = col_map
        .can_scull
        .map(|idx| field(record, idx))
        .unwrap_or("");
    let can_scull_yes = can_scull_answer.eq_ignore_ascii_case("yes");
    // Sculling rows get hard scull bias. Sweep rows: if they can scull,
    // sweep-preferred (1); otherwise sweep-hard (2).
    let sweep_bias = if is_sculling {
        SweepBias::SCULL_HARD
    } else if can_scull_yes {
        SweepBias::new(1) // sweep-preferred, can flex to scull
    } else {
        SweepBias::SWEEP_HARD
    };

    // Upsert the rower row. Identity is keyed on email via app_user.
    let rower = match AppUser::find_by_email(conn, email)? {
        Some(user) => {
            // User exists — load or create linked rower.
            match user.rower_id {
                Some(rower_id) => {
                    let existing = Rower::get(conn, rower_id)?.ok_or_else(|| {
                        anyhow!("app_user.rower_id points to missing rower {rower_id}")
                    })?;
                    // Reactivate if previously soft-deleted.
                    if !existing.active.as_bool() {
                        Rower::set_active(conn, rower_id, true)?;
                        summary.rowers_updated += 1;
                    }
                    let updated = Rower::promote_from_sheet(
                        conn,
                        &existing,
                        &display_name,
                        first_opt.as_deref(),
                        last_opt.as_deref(),
                        side,
                        is_sculling,
                        if col_map.can_scull.is_some() {
                            Some(can_scull_yes)
                        } else {
                            None
                        },
                        can_cox,
                        is_designated_cox,
                    )?;
                    if updated.updated_at != existing.updated_at {
                        summary.rowers_updated += 1;
                    }
                    updated
                }
                None => {
                    // User exists but has no rower — create one and link.
                    let new = NewRower::from_sheet(
                        &display_name,
                        first_opt.clone(),
                        last_opt.clone(),
                        side,
                        sweep_bias,
                        can_cox,
                        is_designated_cox,
                    );
                    let created = Rower::insert(conn, new)?;
                    AppUser::set_rower_id(conn, user.id, Some(created.id))?;
                    summary.rowers_created += 1;
                    created
                }
            }
        }
        None => {
            // No user with this email — create rower + passwordless active user.
            let new = NewRower::from_sheet(
                &display_name,
                first_opt.clone(),
                last_opt.clone(),
                side,
                sweep_bias,
                can_cox,
                is_designated_cox,
            );
            let created = Rower::insert(conn, new)?;
            let now = chrono::Utc::now().naive_utc();
            let user = AppUser::create(
                conn,
                NewAppUser {
                    email: email.to_string(),
                    password_hash: None,
                    name: display_name.clone(),
                    status: UserStatus::Active,
                    created_at: now,
                    updated_at: now,
                    first_name: if first_name.is_empty() {
                        None
                    } else {
                        Some(first_name.to_string())
                    },
                    last_name: if last_name.is_empty() {
                        None
                    } else {
                        Some(last_name.to_string())
                    },
                },
            )?;
            AppUser::set_role(conn, user.id, Role::Member)?;
            AppUser::set_rower_id(conn, user.id, Some(created.id))?;
            summary.rowers_created += 1;
            created
        }
    };

    // Ensure team membership (idempotent).
    lineup_db::team::TeamMembership::add(conn, team_id, rower.id)?;

    // Upsert per-date availability.
    for (col_idx, date) in &col_map.dates {
        let cell = record.get(*col_idx).unwrap_or("").trim();
        if cell.is_empty() {
            continue;
        }
        let status = match cell {
            "Attending" => AvailabilityStatus::Yes,
            "Not Attending" => AvailabilityStatus::No,
            other => {
                summary.warnings.push(format!(
                    "unknown status {other:?} for {display_name} on {date}"
                ));
                continue;
            }
        };
        // Ensure practice exists for this date so we can key availability on it.
        // Prefer an existing practice for the date (any time); only create with
        // team defaults when none exists yet.
        let practice = match lineup_db::practice::Practice::find_by_date(conn, team_id, *date)? {
            Some(p) => p,
            None => {
                let p = lineup_db::practice::Practice::upsert(
                    conn,
                    team_id,
                    *date,
                    default_time,
                    None,
                )?;
                // Apply team default duration on newly created practices.
                if p.duration_minutes.is_none() && default_duration.is_some() {
                    use diesel::prelude::*;
                    diesel::update(lineup_db::schema::practice::table.find(p.id))
                        .set(lineup_db::schema::practice::duration_minutes.eq(default_duration))
                        .execute(conn)?;
                }
                p
            }
        };
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

/// Resolved column indices for the known metadata fields plus any
/// date columns. Built from the header row by case-insensitive name
/// matching so columns can appear in any order and optional columns
/// (Pronoun, Can you Scull?) may be absent.
struct ColumnMap {
    sweep_scull: usize,
    last_name: usize,
    first_name: usize,
    email: usize,
    side_cox: usize,
    can_scull: Option<usize>,
    dates: Vec<(usize, NaiveDate)>,
}

impl ColumnMap {
    fn from_headers(headers: &csv::StringRecord, year: i32) -> Result<Self> {
        let mut sweep_scull = None;
        let mut last_name = None;
        let mut first_name = None;
        let mut email = None;
        let mut side_cox = None;
        let mut can_scull = None;
        let mut dates = Vec::new();

        for idx in 0..headers.len() {
            let raw = headers.get(idx).unwrap_or("").trim();
            if raw.is_empty() {
                continue;
            }
            let lower = raw.to_ascii_lowercase();
            match lower.as_str() {
                "sweep/scull" => sweep_scull = Some(idx),
                "last name" | "last" | "lastname" => last_name = Some(idx),
                "first name" | "first" | "firstname" => first_name = Some(idx),
                "email" | "e-mail" => email = Some(idx),
                "side/cox" | "side" | "cox" => side_cox = Some(idx),
                "can you scull?" | "can scull" | "scull" => can_scull = Some(idx),
                // Known optional columns — skip without error.
                "pronoun" | "pronouns" => {}
                _ => {
                    // Try to parse as M/D date.
                    if let Ok(date) = parse_month_day(raw, year) {
                        dates.push((idx, date));
                    }
                    // Otherwise silently ignore (extra metadata columns, etc.)
                }
            }
        }

        Ok(Self {
            sweep_scull: sweep_scull
                .ok_or_else(|| anyhow!("missing required column: Sweep/Scull"))?,
            last_name: last_name.ok_or_else(|| anyhow!("missing required column: Last Name"))?,
            first_name: first_name.ok_or_else(|| anyhow!("missing required column: First Name"))?,
            email: email.ok_or_else(|| anyhow!("missing required column: Email"))?,
            side_cox: side_cox.ok_or_else(|| anyhow!("missing required column: Side/Cox"))?,
            can_scull,
            dates,
        })
    }
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

fn field(record: &csv::StringRecord, idx: usize) -> &str {
    record.get(idx).unwrap_or("").trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::prelude::*;
    use lineup_db::app_user::{AppUser, NewAppUser};
    use lineup_db::availability::types::AvailabilityStatus;
    use lineup_db::availability::Availability;
    use lineup_db::rower::types::{
        RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias,
    };
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
        Team::create(
            conn,
            NewTeam {
                name: "Test".into(),
                created_at: now,
            },
        )
        .expect("seed team")
        .id
    }

    /// Fetch the rowers table ordered by id so assertions can rely
    /// on insertion order.
    fn all_rowers(conn: &mut SqliteConnection) -> Vec<Rower> {
        use lineup_db::schema::rower::dsl::*;
        rower
            .order(id.asc())
            .select(Rower::as_select())
            .load(conn)
            .unwrap()
    }

    fn all_availabilities(conn: &mut SqliteConnection) -> Vec<Availability> {
        availability_schema::table
            .order((
                availability_schema::rower_id.asc(),
                availability_schema::practice_id.asc(),
            ))
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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 2);
        assert_eq!(summary.rowers_created, 2);
        assert_eq!(summary.rowers_updated, 0);
        assert_eq!(summary.availabilities_upserted, 4);
        assert_eq!(summary.sweep_rows, 2);
        assert_eq!(summary.sculling_rows, 0);
        assert!(
            summary.warnings.is_empty(),
            "warnings: {:?}",
            summary.warnings
        );

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 2);
        assert_eq!(rowers[0].name, "Alice Smith");
        assert_eq!(rowers[0].side, Side::Port);
        // Alice answered "Yes" to Can you Scull? → sweep-preferred (1)
        assert_eq!(rowers[0].sweep_bias, SweepBias::new(1));
        assert_eq!(rowers[1].name, "Bob Jones");
        assert_eq!(rowers[1].side, Side::Starboard);
        // Bob answered "No" → sweep-hard (2)
        assert_eq!(rowers[1].sweep_bias, SweepBias::SWEEP_HARD);
        // Email now lives on app_user, not rower.
        let alice_user = AppUser::find_by_email(&mut conn, "alice@example.com")
            .unwrap()
            .expect("alice user");
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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 1);
        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 2);
        assert!(
            summary.warnings.is_empty(),
            "warnings: {:?}",
            summary.warnings
        );
    }

    #[test]
    fn sculling_row_maps_attending_to_yes_with_scull_bias() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sculling,Scully,Nico,they/them,nico@example.com,Yes,Port,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 1);
        assert_eq!(summary.sculling_rows, 1);
        assert_eq!(summary.sweep_rows, 0);
        assert_eq!(summary.rowers_created, 1);

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        // Sculling rows get sweep_bias = SCULL_HARD (-2).
        assert_eq!(rowers[0].sweep_bias, SweepBias::SCULL_HARD);

        let avails = all_availabilities(&mut conn);
        assert_eq!(avails.len(), 1);
        assert_eq!(avails[0].status, AvailabilityStatus::Yes);
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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

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
                first_name: None,
                last_name: None,
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Expert,
                strength: Strength::Strong,
                height: lineup_db::rower::types::Height::Tall,
                side: Side::Starboard,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
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
        let user = AppUser::create(
            &mut conn,
            NewAppUser {
                email: "alice@example.com".into(),
                password_hash: None,
                name: "Alice Smith".into(),
                first_name: None,
                last_name: None,
                status: UserStatus::Active,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap();
        AppUser::set_rower_id(&mut conn, user.id, Some(seeded.id)).unwrap();

        // Now re-import from a sheet that says "Both" for this rower.
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,No,Both,Attending\n",
            header_with_dates(&["4/11"]),
        );
        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();
        assert_eq!(summary.rowers_created, 0);

        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        // The key assertion: Starboard survived the sync.
        assert_eq!(rowers[0].side, Side::Starboard);
    }

    #[test]
    fn unrecognised_header_column_is_silently_ignored() {
        // Columns that don't match a known name or M/D date pattern
        // are silently skipped. This lets sheets have extra columns
        // (notes, internal IDs, etc.) without breaking the import.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n",
            header_with_dates(&["not-a-date"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();
        assert_eq!(summary.rowers_created, 1);
        // The non-date column is ignored — no availability upserted.
        assert_eq!(summary.availabilities_upserted, 0);
    }

    #[test]
    fn missing_sweep_scull_header_is_an_error() {
        // If the first column isn't "Sweep/Scull" anywhere in the
        // file, we can't find the header row and should error.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "Team,Last,First,Pronoun,Email,Scull,Side,4/11\n\
                   Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n";

        let err = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap_err();
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

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        // The blank row is counted in rows_read but produces no
        // warning, no rower, and no availability.
        assert_eq!(summary.rows_read, 3);
        assert_eq!(summary.rowers_created, 2);
        assert_eq!(summary.availabilities_upserted, 2);
        assert!(
            summary.warnings.is_empty(),
            "warnings: {:?}",
            summary.warnings
        );
    }

    #[test]
    fn row_filter_sweep_skips_sculling_rows() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n\
             Sculling,Scully,Nico,they/them,nico@example.com,Yes,Port,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::Sweep, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 2);
        assert_eq!(summary.sweep_rows, 1);
        assert_eq!(summary.sculling_rows, 0);
        assert_eq!(summary.rowers_created, 1);
        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        assert_eq!(rowers[0].name, "Alice Smith");
    }

    #[test]
    fn row_filter_sculling_skips_sweep_rows() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = format!(
            "{}\n\
             Sweep,Smith,Alice,she/her,alice@example.com,Yes,Port,Attending\n\
             Sculling,Scully,Nico,they/them,nico@example.com,Yes,Port,Attending\n",
            header_with_dates(&["4/11"]),
        );

        let summary = sync_csv(&csv, YEAR, tid, RowFilter::Sculling, &mut conn).unwrap();

        assert_eq!(summary.rows_read, 2);
        assert_eq!(summary.sweep_rows, 0);
        assert_eq!(summary.sculling_rows, 1);
        assert_eq!(summary.rowers_created, 1);
        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers.len(), 1);
        assert_eq!(rowers[0].name, "Nico Scully");
    }

    #[test]
    fn reordered_columns_work() {
        // Columns in a different order than the original GGRC layout.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "Email,First Name,Last Name,Sweep/Scull,Side/Cox,4/11\n\
                   alice@example.com,Alice,Smith,Sweep,Port,Attending\n";

        let summary = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 1);
        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers[0].name, "Alice Smith");
        assert_eq!(rowers[0].side, Side::Port);
    }

    #[test]
    fn missing_optional_columns_work() {
        // No Pronoun or "Can you Scull?" columns — just the required ones.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "Sweep/Scull,Last Name,First Name,Email,Side/Cox,4/11\n\
                   Sweep,Smith,Alice,alice@example.com,Starboard,Attending\n";

        let summary = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 1);
        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers[0].side, Side::Starboard);
    }

    #[test]
    fn case_insensitive_headers() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "sweep/scull,LAST NAME,first name,EMAIL,SIDE/COX,4/11\n\
                   Sweep,Smith,Alice,alice@example.com,Port,Attending\n";

        let summary = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 1);
    }

    #[test]
    fn extra_columns_are_ignored() {
        // Sheet has extra columns (Notes, Internal ID) that we don't know about.
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let csv = "Sweep/Scull,Last Name,First Name,Notes,Email,Internal ID,Side/Cox,4/11\n\
                   Sweep,Smith,Alice,some note,alice@example.com,12345,Port,Attending\n";

        let summary = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap();

        assert_eq!(summary.rowers_created, 1);
        assert_eq!(summary.availabilities_upserted, 1);
        let rowers = all_rowers(&mut conn);
        assert_eq!(rowers[0].name, "Alice Smith");
    }

    #[test]
    fn missing_required_column_errors() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        // Missing Email column.
        let csv = "Sweep/Scull,Last Name,First Name,Side/Cox,4/11\n\
                   Sweep,Smith,Alice,Port,Attending\n";

        let err = sync_csv(csv, YEAR, tid, RowFilter::All, &mut conn).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_ascii_lowercase().contains("email"),
            "error should mention missing Email column: {msg}"
        );
    }
}
