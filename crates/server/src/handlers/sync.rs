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
use serde::Deserialize;

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
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn form_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    // Pre-fill from saved sync config if available.
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
        })
    });
    let content = templates::sync::form_content(prefill.as_ref(), None, None, last_synced);
    Ok(super::maybe_page("Sync sheet", content, hx))
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
        return Ok(super::maybe_page("Sync sheet", content, hx));
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
            return Ok(super::maybe_page("Sync sheet", content, hx));
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
            let _ = tenant
                .db
                .with_conn(move |conn| {
                    lineup_db::sync_source::SyncSource::upsert(
                        conn, team_id, "google_sheet", &config_json,
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
            let now = Some(chrono::Utc::now().naive_utc());
            let content =
                templates::sync::form_content(Some(&input), Some(&summary), None, now);
            Ok(super::maybe_page("Sync sheet", content, hx))
        }
        Err(err) => {
            tracing::warn!(?err, "sheet parse/sync failed");
            let msg = format!("Sync failed: {err}");
            let content =
                templates::sync::form_content(Some(&input), None, Some(&msg), None);
            Ok(super::maybe_page("Sync sheet", content, hx))
        }
    }
}

async fn fetch_csv(url: &str) -> anyhow::Result<String> {
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
