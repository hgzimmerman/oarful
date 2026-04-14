//! `GET /sync` — render the spreadsheet sync form.
//! `POST /sync` — fetch a publicly-shared Google Sheet by ID/gid and
//! upsert its rows into the database.
//!
//! Mirrors `cmd_sync_sheet` from `crates/cli/src/main.rs`: the HTTP
//! fetch happens in async land, then the parsed body is handed into
//! `db.with_conn` so `lineup_sheets::sync_csv` can run on a pooled
//! connection. The two-step shape is forced by `with_conn` taking a
//! sync closure — we can't `.await` the reqwest call inside it.

use axum::{
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{Datelike, Utc};
use lineup_sheets::SyncSummary;
use serde::{Deserialize, Deserializer};

/// Deserialize an empty string as `None` for optional numeric form fields.
fn empty_string_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        s.parse::<T>().map(Some).map_err(serde::de::Error::custom)
    }
}

use lineup_db::app_user::Role;

use crate::{handlers::internal_error, state::TenantContext, templates};

/// Google Sheet sync config, serialized as JSON in sync_source.config.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GoogleSheetConfig {
    sheet_id: String,
    gid: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SyncFormInput {
    pub(crate) spreadsheet_id: String,
    /// Tab identifier inside the spreadsheet. Defaults to 0 — the
    /// first/only tab on most sheets.
    #[serde(default)]
    pub(crate) gid: u32,
    /// Optional auto-sync interval in minutes. `None` or `Some(0)`
    /// disables polling; any positive value enables it.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub(crate) poll_interval_minutes: Option<u32>,
}

/// Build the sync form markup (shared by `/sync` and `/team/sync`).
pub(crate) async fn sync_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let saved = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::sync_source::SyncSource::find_by_type(conn, team_id, "google_sheet")
        })
        .await
        .map_err(internal_error)?;
    let last_synced = saved.as_ref().and_then(|s| s.last_synced_at);
    let prefill = saved.and_then(|s| {
        serde_json::from_str::<GoogleSheetConfig>(&s.config).ok().map(|cfg| SyncFormInput {
            spreadsheet_id: cfg.sheet_id,
            gid: cfg.gid,
            poll_interval_minutes: s.poll_interval_minutes.map(|m| m as u32),
        })
    });
    Ok(templates::sync::form_content(prefill.as_ref(), None, None, last_synced))
}


#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn sync_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<SyncFormInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let trimmed = input.spreadsheet_id.trim().to_string();
    if trimmed.is_empty() {
        let content = templates::sync::form_content(
            Some(&input), None, Some("Spreadsheet ID is required."), None,
        );
        return Ok(super::maybe_page_authed("Sync sheet", content, hx, &tenant));
    }

    // Step 1 — fetch the CSV from Google in async land. Render any
    // HTTP failure as a form error rather than a 500 so the operator
    // can correct the ID/gid and retry.
    let url = format!(
        "https://docs.google.com/spreadsheets/d/{}/export?format=csv&gid={}",
        trimmed, input.gid,
    );
    let csv_text = match fetch_csv(&url).await {
        Ok(text) => text,
        Err(err) => {
            tracing::warn!(?err, %url, "sheet fetch failed");
            let msg = format!("Failed to fetch sheet: {err}");
            let content = templates::sync::form_content(Some(&input), None, Some(&msg), None);
            return Ok(super::maybe_page_authed("Sync sheet", content, hx, &tenant));
        }
    };

    // Step 2 — sync inside the blocking pool. lineup_sheets::sync_csv
    // returns anyhow::Result, but with_conn wants
    // Result<_, diesel::result::Error>; flatten by stuffing the
    // anyhow into an Ok(Result<...>) and unwrapping outside.
    let year = Utc::now().year();
    let csv_for_sync = csv_text.clone();
    let sync_outcome: anyhow::Result<SyncSummary> = tenant
        .db
        .with_conn(move |conn| Ok(lineup_sheets::sync_csv(&csv_for_sync, year, team_id, conn)))
        .await
        .map_err(internal_error)?;

    match sync_outcome {
        Ok(summary) => {
            // Save sync config for one-click re-sync.
            let config_json = serde_json::to_string(&GoogleSheetConfig {
                sheet_id: trimmed.clone(),
                gid: input.gid,
            })
            .unwrap_or_default();
            // Normalize: 0 or None → no polling.
            let poll_minutes = input
                .poll_interval_minutes
                .filter(|&m| m > 0)
                .map(|m| m as i32);
            let _ = tenant
                .db
                .with_conn(move |conn| {
                    lineup_db::sync_source::SyncSource::upsert(
                        conn, team_id, "google_sheet", &config_json, poll_minutes,
                    )?;
                    // Mark synced so the timestamp persists on reload.
                    if let Some(src) = lineup_db::sync_source::SyncSource::find_by_type(
                        conn, team_id, "google_sheet",
                    )? {
                        lineup_db::sync_source::SyncSource::mark_synced(conn, src.id)?;
                    }
                    Ok(())
                })
                .await;
            crate::audit::record(
                &tenant.db,
                Some(tenant.claims.user_id().as_int()),
                "sync.import",
                "sync_source",
                "google_sheet",
                Some(serde_json::json!({
                    "rows": summary.rows_read,
                    "created": summary.rowers_created,
                    "updated": summary.rowers_updated,
                }).to_string()),
            );

            let now = Some(chrono::Utc::now().naive_utc());
            let content =
                templates::sync::form_content(Some(&input), Some(&summary), None, now);
            Ok(super::maybe_page_authed("Sync sheet", content, hx, &tenant))
        }
        Err(err) => {
            tracing::warn!(?err, "sheet parse/sync failed");
            let msg = format!("Sync failed: {err}");
            let content =
                templates::sync::form_content(Some(&input), None, Some(&msg), None);
            Ok(super::maybe_page_authed("Sync sheet", content, hx, &tenant))
        }
    }
}

/// Run one polling sweep: for every tenant, check pollable sync sources
/// and re-sync any whose interval has elapsed since the last sync.
pub async fn poll_sync_sources(state: &crate::AppState) {
    // List all tenants from the master DB.
    let tenants = match state
        .master_db
        .with_conn(|conn| lineup_master_db::tenant::Tenant::list_all(conn))
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(?e, "sync poll: failed to list tenants");
            return;
        }
    };

    for tenant in &tenants {
        // Skip demo tenants.
        if tenant.demo_expires_at.is_some() {
            continue;
        }

        let (db, _config) = match state.tenant_db(tenant.id).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(tenant_id = %tenant.id, ?e, "sync poll: failed to open tenant DB");
                continue;
            }
        };

        let sources = match db
            .with_conn(|conn| lineup_db::sync_source::SyncSource::list_pollable(conn))
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(tenant_id = %tenant.id, ?e, "sync poll: failed to list pollable sources");
                continue;
            }
        };

        let now = chrono::Utc::now().naive_utc();
        for src in sources {
            let interval_mins = match src.poll_interval_minutes {
                Some(m) if m > 0 => m,
                _ => continue,
            };

            // Check if enough time has elapsed since the last sync.
            if let Some(last) = src.last_synced_at {
                let elapsed = now.signed_duration_since(last);
                if elapsed < chrono::Duration::minutes(interval_mins as i64) {
                    continue;
                }
            }

            let config: GoogleSheetConfig = match serde_json::from_str(&src.config) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(sync_source_id = ?src.id, ?e, "sync poll: bad config JSON");
                    continue;
                }
            };

            let url = format!(
                "https://docs.google.com/spreadsheets/d/{}/export?format=csv&gid={}",
                config.sheet_id, config.gid,
            );

            tracing::info!(
                tenant_id = %tenant.id,
                team_id = ?src.team_id,
                "sync poll: fetching sheet"
            );

            let csv_text = match fetch_csv(&url).await {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!(tenant_id = %tenant.id, ?e, "sync poll: fetch failed");
                    let src_id = src.id;
                    let err_msg = format!("{e:#}");
                    let _ = db
                        .with_conn(move |conn| {
                            lineup_db::sync_source::SyncSource::mark_error(conn, src_id, &err_msg)
                        })
                        .await;
                    continue;
                }
            };

            let year = chrono::Utc::now().year();
            let team_id = src.team_id;
            let src_id = src.id;
            let sync_result: anyhow::Result<lineup_sheets::SyncSummary> = match db
                .with_conn(move |conn| {
                    Ok(lineup_sheets::sync_csv(&csv_text, year, team_id, conn))
                })
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(tenant_id = %tenant.id, ?e, "sync poll: DB error");
                    continue;
                }
            };

            match sync_result {
                Ok(summary) => {
                    tracing::info!(
                        tenant_id = %tenant.id,
                        team_id = ?team_id,
                        rows = summary.rows_read,
                        created = summary.rowers_created,
                        updated = summary.rowers_updated,
                        avail = summary.availabilities_upserted,
                        "sync poll: success"
                    );
                    crate::audit::record(
                        &db,
                        None, // system action
                        "sync.poll",
                        "sync_source",
                        "google_sheet",
                        Some(serde_json::json!({
                            "rows": summary.rows_read,
                            "created": summary.rowers_created,
                            "updated": summary.rowers_updated,
                        }).to_string()),
                    );
                    let _ = db
                        .with_conn(move |conn| {
                            lineup_db::sync_source::SyncSource::mark_synced(conn, src_id)
                        })
                        .await;
                }
                Err(e) => {
                    tracing::warn!(tenant_id = %tenant.id, ?e, "sync poll: sync_csv failed");
                    let err_msg = format!("{e:#}");
                    let _ = db
                        .with_conn(move |conn| {
                            lineup_db::sync_source::SyncSource::mark_error(conn, src_id, &err_msg)
                        })
                        .await;
                }
            }
        }
    }
}

pub(crate) async fn fetch_csv(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "HTTP {} from Google — is the sheet shared as 'Anyone with the link can view'?",
            resp.status()
        );
    }
    Ok(resp.text().await?)
}
