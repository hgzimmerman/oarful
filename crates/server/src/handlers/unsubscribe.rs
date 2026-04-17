//! Public (no-auth) unsubscribe endpoints.

use axum::extract::{Path, State};
use axum::response::Html;

use crate::jwt::JwtKeys;
use crate::state::TenantDb;
use crate::unsubscribe::{self, EmailType};

use lineup_db::app_user::{AppUser, UserId};

#[derive(serde::Deserialize)]
pub(crate) struct UnsubParams {
    slug: String,
    user_id: i32,
    email_type: String,
    signature: String,
}

/// `GET /unsubscribe/{slug}/{user_id}/{email_type}/{signature}`
///
/// Verifies the HMAC signature, toggles the preference, and renders a
/// standalone confirmation page.
#[tracing::instrument(level = "info", skip_all)]
pub(crate) async fn unsubscribe_handler(
    State(tenant_db): State<TenantDb>,
    State(jwt_keys): State<JwtKeys>,
    Path(params): Path<UnsubParams>,
) -> Html<String> {
    let Some(email_type) = EmailType::from_str(&params.email_type) else {
        return Html(
            crate::templates::unsubscribe::error_page("Invalid unsubscribe link.").into_string(),
        );
    };

    let user_id = UserId::new(params.user_id);

    if !unsubscribe::verify(
        &jwt_keys,
        &params.slug,
        user_id,
        email_type.as_str(),
        &params.signature,
    ) {
        return Html(
            crate::templates::unsubscribe::error_page("Invalid or corrupted unsubscribe link.")
                .into_string(),
        );
    }

    let Ok((_tenant_id, db, config)) = tenant_db.tenant_db_by_slug(&params.slug).await else {
        return Html(crate::templates::unsubscribe::error_page("Club not found.").into_string());
    };

    let club_name = config.tenant_name.clone();

    // Toggle the preference(s).
    let result = db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, user_id)?;
            let Some(user) = user else {
                return Ok(false);
            };
            match email_type {
                EmailType::Reminders => {
                    AppUser::set_email_prefs(conn, user_id, false, user.wants_lineups())?;
                }
                EmailType::Lineups => {
                    AppUser::set_email_prefs(conn, user_id, user.wants_reminders(), false)?;
                }
                EmailType::All => {
                    AppUser::set_email_prefs(conn, user_id, false, false)?;
                }
            }
            Ok(true)
        })
        .await;

    match result {
        Ok(true) => {
            Html(crate::templates::unsubscribe::success_page(&club_name, email_type).into_string())
        }
        Ok(false) => {
            Html(crate::templates::unsubscribe::error_page("User not found.").into_string())
        }
        Err(err) => {
            tracing::error!(?err, "unsubscribe DB error");
            Html(
                crate::templates::unsubscribe::error_page(
                    "Something went wrong. Please try again later.",
                )
                .into_string(),
            )
        }
    }
}

/// `POST /unsubscribe/{slug}/{user_id}/{email_type}/{signature}`
///
/// RFC 8058 one-click unsubscribe via `List-Unsubscribe-Post` header.
/// Same logic as GET but returns a plain 200.
#[tracing::instrument(level = "info", skip_all)]
pub(crate) async fn unsubscribe_post_handler(
    State(tenant_db): State<TenantDb>,
    State(jwt_keys): State<JwtKeys>,
    Path(params): Path<UnsubParams>,
) -> axum::http::StatusCode {
    let Some(email_type) = EmailType::from_str(&params.email_type) else {
        return axum::http::StatusCode::BAD_REQUEST;
    };

    let user_id = UserId::new(params.user_id);

    if !unsubscribe::verify(
        &jwt_keys,
        &params.slug,
        user_id,
        email_type.as_str(),
        &params.signature,
    ) {
        return axum::http::StatusCode::FORBIDDEN;
    }

    let Ok((_tenant_id, db, _config)) = tenant_db.tenant_db_by_slug(&params.slug).await else {
        return axum::http::StatusCode::NOT_FOUND;
    };

    let result = db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, user_id)?;
            let Some(user) = user else {
                return Ok(());
            };
            match email_type {
                EmailType::Reminders => {
                    AppUser::set_email_prefs(conn, user_id, false, user.wants_lineups())?;
                }
                EmailType::Lineups => {
                    AppUser::set_email_prefs(conn, user_id, user.wants_reminders(), false)?;
                }
                EmailType::All => {
                    AppUser::set_email_prefs(conn, user_id, false, false)?;
                }
            }
            Ok(())
        })
        .await;

    match result {
        Ok(()) => axum::http::StatusCode::OK,
        Err(err) => {
            tracing::error!(?err, "unsubscribe POST DB error");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
