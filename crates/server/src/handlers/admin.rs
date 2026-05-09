//! `/admin` — PD+ hub with tabs: Users, Teams, Audit.

use axum::{
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    Extension, Form,
};
use axum_htmx::HxRequest;
use lineup_db::app_user::{AppUser, Role};
use maud::html;

use crate::handlers::audit::AuditQuery;
use crate::handlers::internal_error;
use crate::handlers::{self, not_found, ErrorResponse};
use crate::state::{TenantContext, TenantDb};
use crate::templates::layout::{tab_swap, tabbed_section, TabDef};

const TABS: &[TabDef] = &[
    TabDef {
        label: "Users",
        url: "/admin/users",
        id: "users",
    },
    TabDef {
        label: "Teams",
        url: "/admin/teams",
        id: "teams",
    },
    TabDef {
        label: "Roster",
        url: "/admin/roster",
        id: "roster",
    },
    TabDef {
        label: "Fleet",
        url: "/admin/fleet",
        id: "fleet",
    },
    TabDef {
        label: "Audit",
        url: "/admin/audit",
        id: "audit",
    },
    TabDef {
        label: "Settings",
        url: "/admin/settings",
        id: "settings",
    },
];
const TARGET: &str = "admin-tab-content";

/// `GET /admin` — render the default tab (Users).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::users::users_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "users", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "users", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/users`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn users_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::users::users_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "users", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "users", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/teams`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn teams_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::teams::teams_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "teams", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "teams", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/roster`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::teams::roster_matrix_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "roster", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "roster", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

// ── Fleet subtabs ────────────────────────────────────────────────
const FLEET_SUBTABS: &[TabDef] = &[
    TabDef {
        label: "Boats",
        url: "/admin/fleet/boats",
        id: "boats",
    },
    TabDef {
        label: "Team defaults",
        url: "/admin/fleet/defaults",
        id: "defaults",
    },
    TabDef {
        label: "Oar sets",
        url: "/admin/fleet/oars",
        id: "oars",
    },
];
const FLEET_TARGET: &str = "admin-fleet-content";

/// `GET /admin/fleet` — default to Boats subtab.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let boats_content = handlers::boats::fleet_content(&tenant).await?;
    let fleet_section = tabbed_section(FLEET_SUBTABS, "boats", FLEET_TARGET, boats_content);

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "fleet", TARGET, fleet_section).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, fleet_section);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/fleet/boats` — boat list subtab.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_boats_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let boats_content = handlers::boats::fleet_content(&tenant).await?;

    if is_fleet_subtab_swap(&headers) {
        return Ok(Html(
            tab_swap(FLEET_SUBTABS, "boats", FLEET_TARGET, boats_content).into_string(),
        ));
    }
    let fleet_section = tabbed_section(FLEET_SUBTABS, "boats", FLEET_TARGET, boats_content);
    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "fleet", TARGET, fleet_section).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, fleet_section);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/fleet/defaults` — team boat defaults subtab.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_defaults_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let defaults_content = handlers::teams::fleet_matrix_content(&tenant).await?;

    if is_fleet_subtab_swap(&headers) {
        return Ok(Html(
            tab_swap(FLEET_SUBTABS, "defaults", FLEET_TARGET, defaults_content).into_string(),
        ));
    }
    let fleet_section = tabbed_section(FLEET_SUBTABS, "defaults", FLEET_TARGET, defaults_content);
    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "fleet", TARGET, fleet_section).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, fleet_section);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/fleet/oars` — oar sets subtab.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_oars_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let oars_content = handlers::oar_sets::list_content(&tenant).await?;

    if is_fleet_subtab_swap(&headers) {
        return Ok(Html(
            tab_swap(FLEET_SUBTABS, "oars", FLEET_TARGET, oars_content).into_string(),
        ));
    }
    let fleet_section = tabbed_section(FLEET_SUBTABS, "oars", FLEET_TARGET, oars_content);
    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "fleet", TARGET, fleet_section).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, fleet_section);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

fn is_fleet_subtab_swap(headers: &HeaderMap) -> bool {
    headers.get("HX-Target").and_then(|v| v.to_str().ok()) == Some(FLEET_TARGET)
}

/// `GET /admin/audit`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn audit_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<AuditQuery>,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::audit::audit_content(&tenant, &query).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "audit", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "audit", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/settings` — tenant-level configuration.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn settings_handler(
    State(tdb): State<TenantDb>,
    State(stripe_ctx): State<Option<crate::state::StripeCtx>>,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = settings_content(&tdb, &tenant, stripe_ctx.is_some()).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "settings", TARGET, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "settings", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

async fn settings_content(
    tdb: &TenantDb,
    tenant: &TenantContext,
    stripe_enabled: bool,
) -> Result<maud::Markup, ErrorResponse> {
    let tenant_id = tenant.tenant_id;
    let t = tdb
        .master_db
        .with_conn(move |conn| lineup_master_db::tenant::Tenant::get(conn, tenant_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Tenant not found."))?;

    Ok(html! {
        // ── Header ──
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" {
                "Settings"
            }
            p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" {
                "Tenant-wide configuration"
            }
        }

        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto space-y-6" {
            // ── Display preferences ──
            form method="post" action="/admin/settings"
                 hx-post="/admin/settings"
                 hx-target={"#" (TARGET)}
                 class="rounded-lg p-6 space-y-5" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-1" style="color: var(--ink)" {
                    "Display preferences"
                }
                div {
                    label class="flex items-center gap-3 cursor-pointer" {
                        input type="checkbox" name="attributes_public" value="1"
                              checked[t.are_attributes_public()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        div {
                            div class="text-sm font-medium" style="color: var(--ink)" {
                                "Public rower attributes"
                            }
                            p class="text-xs" style="color: var(--muted)" {
                                "Show weight class, form, and strength to all members. When off, only Coach+ can see them."
                            }
                        }
                    }
                }
                div {
                    label class="flex items-center gap-3 cursor-pointer" {
                        input type="checkbox" name="emails_visible" value="1"
                              checked[t.are_emails_visible()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        div {
                            div class="text-sm font-medium" style="color: var(--ink)" {
                                "Visible email addresses"
                            }
                            p class="text-xs" style="color: var(--muted)" {
                                "Show rower emails on the roster and detail pages. When off, only Coach+ can see them."
                            }
                        }
                    }
                }
                div {
                    label class="flex items-center gap-3 cursor-pointer" {
                        input type="checkbox" name="force_cox_stern" value="1"
                              checked[t.force_cox_stern()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        div {
                            div class="text-sm font-medium" style="color: var(--ink)" {
                                "Force cox at stern"
                            }
                            p class="text-xs" style="color: var(--muted)" {
                                "Always display the coxswain at the top of lineup cards, regardless of per-boat bow/stern setting."
                            }
                        }
                    }
                }
                button type="submit" class="btn-warm-ink py-2 px-5" {
                    "Save"
                }
            }

            // ── Plan / billing ──
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-3" style="color: var(--ink)" {
                    "Plan"
                }
                div class="flex items-center justify-between" {
                    div class="flex items-center gap-2" {
                        @let (plan_label, plan_style) = match t.billing_status {
                            lineup_master_db::tenant::BillingStatus::Free =>
                                ("Free", "color: var(--ink-3); background: var(--paper-2); border-color: var(--rule)"),
                            lineup_master_db::tenant::BillingStatus::Active =>
                                ("Active", "color: var(--good); background: color-mix(in oklch, var(--good) 10%, var(--paper)); border-color: color-mix(in oklch, var(--good) 22%, var(--rule))"),
                            lineup_master_db::tenant::BillingStatus::Grandfathered =>
                                ("Grandfathered", "color: var(--accent); background: color-mix(in oklch, var(--accent) 10%, var(--paper)); border-color: color-mix(in oklch, var(--accent) 22%, var(--rule))"),
                        };
                        span class="stat-badge text-[10px]" style=(plan_style) { (plan_label) }
                        @if !tenant.config.can_send_email() {
                            span class="font-mono-stat text-[10px]" style="color: var(--muted)" { "(email disabled)" }
                        }
                    }
                    @if stripe_enabled {
                        @if t.stripe_customer_id.is_some() {
                            a href="/billing/portal"
                              class="font-mono-stat text-xs font-semibold hover:underline" style="color: var(--accent)" {
                                "Manage subscription"
                            }
                        } @else if t.billing_status == lineup_master_db::tenant::BillingStatus::Free {
                            form method="post" action="/billing/checkout" {
                                button type="submit" class="btn-warm-ink py-2 px-4 text-sm" {
                                    "Upgrade"
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct SettingsInput {
    #[serde(default)]
    attributes_public: Option<String>,
    #[serde(default)]
    emails_visible: Option<String>,
    #[serde(default)]
    force_cox_stern: Option<String>,
}

/// `POST /admin/settings` — update tenant-level flags.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn settings_update_handler(
    State(tdb): State<TenantDb>,
    State(stripe_ctx): State<Option<crate::state::StripeCtx>>,
    Extension(tenant): Extension<TenantContext>,
    Form(input): Form<SettingsInput>,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let tenant_id = tenant.tenant_id;
    let attrs = if input.attributes_public.is_some() {
        1
    } else {
        0
    };
    let emails = if input.emails_visible.is_some() { 1 } else { 0 };
    let cox = if input.force_cox_stern.is_some() {
        1
    } else {
        0
    };

    tdb.master_db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_master_db::schema::tenant as t;
            diesel::update(t::table.find(tenant_id))
                .set((
                    t::attributes_public.eq(attrs),
                    t::emails_visible.eq(emails),
                    t::force_cox_stern.eq(cox),
                ))
                .execute(conn)
        })
        .await
        .map_err(internal_error)?;

    // Bust the cached config so changes take effect immediately.
    tdb.evict_tenant(tenant.tenant_id);

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "tenant.settings.update",
        "tenant",
        &tenant_id.to_string(),
        Some(
            serde_json::json!({
                "attributes_public": attrs,
                "emails_visible": emails,
                "force_cox_stern": cox,
            })
            .to_string(),
        ),
    );

    // Re-load and re-render with fresh config.
    let tab_content = settings_content(&tdb, &tenant, stripe_ctx.is_some()).await?;
    Ok(Html(
        tab_swap(TABS, "settings", TARGET, tab_content).into_string(),
    ))
}

fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers.get("HX-Target").and_then(|v| v.to_str().ok()) == Some(TARGET)
}

// ── Export / Restore ────────────────────────────────────────────

/// `GET /admin/export` — download the tenant's SQLite database file.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn export_handler(
    State(tdb): State<TenantDb>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let tenant_id = tenant.tenant_id;
    let tenant_row = tdb
        .master_db
        .with_conn(move |conn| lineup_master_db::tenant::Tenant::get(conn, tenant_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Not found."))?;
    let db_path = tenant_row.db_path;

    // Checkpoint the WAL so the base file contains all recent writes.
    tenant
        .db
        .with_conn(|conn| {
            use diesel::prelude::*;
            diesel::sql_query("PRAGMA wal_checkpoint(TRUNCATE)").execute(conn)?;
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    let bytes = tokio::fs::read(&db_path).await.map_err(internal_error)?;
    let filename = format!(
        "lineup_{}_{}.db",
        tenant.config.tenant_slug,
        chrono::Utc::now().format("%Y-%m-%d")
    );
    let headers = [
        (
            axum::http::header::CONTENT_TYPE,
            "application/x-sqlite3".to_string(),
        ),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        ),
    ];
    Ok((headers, bytes))
}

/// `GET /admin/restore` — show the restore form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn restore_form_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let content = restore_form_markup(None, None);
    Ok(handlers::maybe_page_authed(
        "Restore Backup",
        content,
        hx,
        &tenant,
    ))
}

/// `POST /admin/restore` — receive the uploaded DB, validate, and restore.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn restore_handler(
    State(tdb): State<TenantDb>,
    Extension(tenant): Extension<TenantContext>,
    mut multipart: Multipart,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    // Read the uploaded file from multipart.
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart.next_field().await.map_err(internal_error)? {
        if field.name() == Some("backup") {
            file_bytes = Some(field.bytes().await.map_err(internal_error)?.to_vec());
            break;
        }
    }
    let file_bytes = match file_bytes {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Ok(Html(
                restore_form_markup(Some("No file uploaded."), None).into_string(),
            ));
        }
    };

    // Save to a temp file so diesel can open it.
    let temp_path = format!(
        "/tmp/lineup_restore_{}_{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    tokio::fs::write(&temp_path, &file_bytes)
        .await
        .map_err(internal_error)?;

    // Look up current user's email.
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::bad_request("Not available in superuser view."))?;
    let current_user = tenant
        .db
        .with_conn(move |conn| AppUser::get(conn, user_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| {
            crate::handlers::ErrorResponse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred.".into(),
            )
        })?;
    let current_email = current_user.email.clone();
    let current_hash = current_user.password_hash.clone();

    // Open the uploaded DB read-only and check for the current user.
    let check_temp = temp_path.clone();
    let check_email = current_email.clone();
    let check_result: Result<Option<BackupUser>, String> = tokio::task::spawn_blocking(move || {
        use diesel::prelude::*;
        let mut conn = diesel::SqliteConnection::establish(&check_temp)
            .map_err(|e| format!("Invalid SQLite file: {e}"))?;
        // Use raw SQL to avoid schema mismatch issues with older backups.
        // Use direct string formatting — email is from our own DB, not user input.
        let query = format!(
            "SELECT email, password_hash FROM app_user WHERE email = '{}' LIMIT 1",
            check_email.replace('\'', "''")
        );
        diesel::sql_query(&query)
            .get_result::<BackupUser>(&mut conn)
            .optional()
            .map_err(|e| format!("Error reading backup: {e}"))
    })
    .await
    .map_err(internal_error)?;

    match check_result {
        Err(msg) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            let content = restore_form_markup(Some(&msg), None);
            return Ok(Html(content.into_string()));
        }
        Ok(None) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            let msg = format!(
                "Your account ({current_email}) does not exist in this backup. Restore aborted."
            );
            return Ok(Html(restore_form_markup(Some(&msg), None).into_string()));
        }
        Ok(Some(backup_user)) => {
            let password_differs = backup_user.password_hash != current_hash;
            if password_differs {
                // Keep the temp file — the confirm form will reference it.
                return Ok(Html(restore_form_markup(
                    Some("Warning: your account exists in this backup but has different credentials. After restore you will need to use the password from the backup."),
                    Some(&temp_path),
                ).into_string()));
            }
        }
    }

    // No credential issues — proceed with restore directly.
    do_restore(&tdb, &tenant, &temp_path).await
}

/// `POST /admin/restore/confirm` — proceed with restore after credential warning.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn restore_confirm_handler(
    State(tdb): State<TenantDb>,
    Extension(tenant): Extension<TenantContext>,
    Form(input): Form<RestoreConfirmInput>,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let temp_path = input.temp_path;
    if !std::path::Path::new(&temp_path).exists() {
        return Ok(Html(
            restore_form_markup(
                Some("Temp file expired. Please upload the backup again."),
                None,
            )
            .into_string(),
        ));
    }

    do_restore(&tdb, &tenant, &temp_path).await
}

#[derive(serde::Deserialize)]
pub(crate) struct RestoreConfirmInput {
    temp_path: String,
}

/// Minimal user info extracted from a backup DB via raw SQL.
#[derive(Debug, diesel::QueryableByName)]
struct BackupUser {
    #[diesel(sql_type = diesel::sql_types::Text)]
    #[allow(dead_code)]
    email: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    password_hash: Option<String>,
}

async fn do_restore(
    tdb: &TenantDb,
    tenant: &TenantContext,
    temp_path: &str,
) -> Result<Html<String>, ErrorResponse> {
    let tenant_id = tenant.tenant_id;
    let tenant_row = tdb
        .master_db
        .with_conn(move |conn| lineup_master_db::tenant::Tenant::get(conn, tenant_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Not found."))?;
    let db_path = tenant_row.db_path;

    // Evict the tenant from the connection cache before overwriting.
    tdb.evict_tenant(tenant.tenant_id);

    tokio::fs::copy(temp_path, &db_path)
        .await
        .map_err(internal_error)?;
    let _ = tokio::fs::remove_file(temp_path).await;

    // Re-connect the tenant (runs migrations on the restored DB).
    tdb.tenant_db(tenant.tenant_id)
        .await
        .map_err(internal_error)?;

    Ok(Html(restore_success_markup().into_string()))
}

fn restore_form_markup(error: Option<&str>, confirm_temp_path: Option<&str>) -> maud::Markup {
    html! {
        div class="max-w-lg mx-auto mt-8" {
            h2 class="text-xl font-bold mb-4" { "Restore Database Backup" }

            @if let Some(err) = error {
                @let is_warning = err.starts_with("Warning:");
                @let (bg, border, text) = if is_warning {
                    ("bg-amber-50", "border-amber-400", "text-amber-800")
                } else {
                    ("bg-red-50", "border-red-400", "text-red-800")
                };
                div class={(bg) " border " (border) " " (text) " px-4 py-3 rounded mb-4"} {
                    (err)
                }
            }

            @if let Some(temp_path) = confirm_temp_path {
                // Credential mismatch — show confirm form with temp path.
                form method="post" action="/admin/restore/confirm"
                     hx-post="/admin/restore/confirm"
                     hx-target="#content" {
                    input type="hidden" name="temp_path" value=(temp_path);
                    div class="flex gap-3" {
                        button type="submit"
                               class="bg-amber-600 hover:bg-amber-700 text-white font-semibold py-2 px-4 rounded" {
                            "Restore anyway"
                        }
                        a href="/admin/restore"
                          class="bg-slate-200 hover:bg-slate-300 text-slate-800 font-semibold py-2 px-4 rounded" {
                            "Cancel"
                        }
                    }
                }
            } @else if error.is_none() {
                p class="text-slate-600 mb-4" {
                    "Upload a previously exported .db file to restore your data. "
                    "This will overwrite all current data in this tenant."
                }

                form id="restore-form" method="post" action="/admin/restore" enctype="multipart/form-data" {
                    div class="mb-4" {
                        label class="block text-sm font-semibold text-slate-700 mb-1" for="backup" {
                            "Backup file (.db)"
                        }
                        input type="file" name="backup" id="backup" accept=".db"
                              class="block w-full text-sm text-slate-500 file:mr-4 file:py-2 file:px-4 file:rounded file:border-0 file:text-sm file:font-semibold file:bg-slate-100 file:text-slate-700 hover:file:bg-slate-200"
                              required;
                    }
                    button type="button"
                           hx-get="/confirm?kind=restore-backup"
                           hx-target="body"
                           hx-swap="beforeend"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold py-2 px-4 rounded shadow transition" {
                        "Restore backup"
                    }
                }
            }

            div class="mt-6" {
                a href="/admin"
                  hx-get="/admin"
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-emerald-700 hover:text-emerald-900 font-semibold text-sm" {
                    "Back to admin"
                }
            }
        }
    }
}

fn restore_success_markup() -> maud::Markup {
    html! {
        div class="max-w-lg mx-auto mt-8" {
            h2 class="text-xl font-bold mb-4" { "Restore Database Backup" }
            div class="bg-emerald-50 border border-emerald-400 text-emerald-800 px-4 py-3 rounded mb-4" {
                "Database restored successfully. You may need to log in again."
            }
            div class="mt-6" {
                a href="/admin"
                  class="text-emerald-700 hover:text-emerald-900 font-semibold text-sm" {
                    "Back to admin"
                }
            }
        }
    }
}
