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
    sync_csv(&csv_text, year, conn)
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
        match sync_row(record, &date_columns, conn, &mut summary) {
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

    // Upsert the rower row (create if new, promote-update if existing).
    let rower = match Rower::find_by_email(conn, email)? {
        None => {
            let new = NewRower::from_sheet(
                &display_name,
                email.to_string(),
                side,
                can_scull,
                can_cox,
                is_designated_cox,
            );
            let created = Rower::insert(conn, new)?;
            summary.rowers_created += 1;
            created
        }
        Some(existing) => {
            let updated = Rower::promote_from_sheet(
                conn,
                &existing,
                &display_name,
                side,
                can_scull,
                can_cox,
                is_designated_cox,
            )?;
            if updated.updated_at != existing.updated_at {
                summary.rowers_updated += 1;
            }
            updated
        }
    };

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
        Availability::upsert(
            conn,
            NewAvailability {
                rower_id: rower.id,
                date: *date,
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
